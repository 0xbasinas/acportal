//! Removes what the one-line installers and the tray's own "Download host" put on this account: the
//! host and tray programs, the Start menu shortcut or desktop entry, the PATH entry, leftover pairing
//! pages and, only when asked, your settings and data. Mirrors `scripts/uninstall.ps1` / `.sh`.
//!
//! Safety: only files with the exact names `acpd`/`acpd-tray` inside the install folder are deleted
//! (a link in `~/.local/bin` only when it points there), folders to purge must be literally named
//! `acpd`, and nothing outside those is touched.
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// Where everything lives. Built from the real folders, or from a scratch folder in tests.
pub struct Locations {
    pub install_dir: PathBuf,
    /// `~/.local/bin` on Linux, where the installer links the programs.
    pub bin_links: Option<PathBuf>,
    pub shortcut: Option<PathBuf>,
    pub desktop_files: Vec<PathBuf>,
    pub temp_dir: PathBuf,
    pub settings_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Locations {
    pub fn current() -> Self {
        let data = acpd::config::default_data_directory();
        let home = || std::env::var_os("HOME").map(PathBuf::from);
        let data_home = data.parent().map(Path::to_path_buf).unwrap_or_default();
        Self {
            install_dir: crate::install::install_dir(),
            bin_links: if cfg!(windows) {
                None
            } else {
                home().map(|home| home.join(".local").join("bin"))
            },
            shortcut: if cfg!(windows) {
                std::env::var_os("APPDATA").map(|appdata| {
                    PathBuf::from(appdata)
                        .join(r"Microsoft\Windows\Start Menu\Programs\ACP Portal.lnk")
                })
            } else {
                None
            },
            desktop_files: if cfg!(windows) {
                vec![]
            } else {
                vec![
                    data_home.join("applications").join("acp-portal.desktop"),
                    data_home
                        .join("icons/hicolor/scalable/apps")
                        .join("acp-portal.svg"),
                ]
            },
            temp_dir: std::env::temp_dir(),
            settings_dir: crate::settings_dir(),
            data_dir: data,
        }
    }
}

const PROGRAMS: [&str; 2] = ["acpd", "acpd-tray"];

fn program_files(name: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![format!("{name}.exe")]
    } else {
        vec![name.to_string()]
    }
}

/// Deletes with a few retries: on Windows a program that was just stopped can stay locked briefly.
fn remove_file(path: &Path) -> Result<bool> {
    let mut last = None;
    for _ in 0..15 {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                last = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
    bail!(
        "Could not delete {}. Close anything using it and try again. ({})",
        path.display(),
        last.expect("a failure was recorded")
    )
}

/// A folder is only purged when it is literally named `acpd`, whatever the environment says.
fn purgeable(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "acpd")
}

/// What is left for the helper that runs after the tray has exited (Windows cannot delete a running
/// program).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Deferred {
    pub own_exe: Option<PathBuf>,
    pub install_dir: PathBuf,
    pub purge_dirs: Vec<PathBuf>,
}

/// Deletes everything that can be deleted now and returns a summary plus what must wait.
/// `own_exe` is the running tray program; `defer_own` is true where it cannot be deleted while running.
pub fn remove_now(
    at: &Locations,
    purge: bool,
    own_exe: Option<&Path>,
    defer_own: bool,
) -> Result<(Vec<String>, Deferred)> {
    let mut summary = Vec::new();
    let mut deferred = Deferred {
        install_dir: at.install_dir.clone(),
        ..Deferred::default()
    };
    let mut removed = 0;
    for name in PROGRAMS {
        for file in program_files(name) {
            let path = at.install_dir.join(&file);
            if let Some(links) = &at.bin_links {
                let link = links.join(&file);
                if std::fs::read_link(&link).is_ok_and(|target| target == path) {
                    let _ = std::fs::remove_file(&link);
                }
            }
            if defer_own && own_exe.is_some_and(|own| own == path) {
                deferred.own_exe = Some(path);
            } else if remove_file(&path)? {
                removed += 1;
            }
        }
    }
    summary.push(if removed > 0 {
        format!(
            "Removed {removed} program file(s) from {}",
            at.install_dir.display()
        )
    } else {
        format!("No programs found in {}", at.install_dir.display())
    });
    if deferred.own_exe.is_none() {
        // The folder is only removed when nothing else is in it.
        let _ = std::fs::remove_dir(&at.install_dir);
    }
    for file in at.shortcut.iter().chain(&at.desktop_files) {
        if remove_file(file)? {
            summary.push(format!("Removed {}", file.display()));
        }
    }
    if let Ok(entries) = std::fs::read_dir(&at.temp_dir) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("acpd-pair-")
                && entry.path().is_dir()
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    if purge {
        for folder in [&at.settings_dir, &at.data_dir] {
            if !purgeable(folder) {
                summary.push(format!(
                    "Not deleting {} (it is not an acpd folder)",
                    folder.display()
                ));
            } else if defer_own && at.install_dir.starts_with(folder) {
                // The running tray lives inside it.
                deferred.purge_dirs.push(folder.clone());
            } else if folder.is_dir() {
                std::fs::remove_dir_all(folder)?;
                summary.push(format!("Deleted {}", folder.display()));
            }
        }
    }
    Ok((summary, deferred))
}

/// `-EncodedCommand` takes UTF-16LE text in base64, which avoids every quoting problem.
pub fn encode_command(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

/// The Windows helper: waits for the tray to exit, deletes it, then tidies the folder, the user PATH
/// and, when purging, the data folders.
pub fn windows_helper_script(deferred: &Deferred, pid: u32) -> String {
    let mut script = String::from("$ErrorActionPreference = 'SilentlyContinue'\n");
    script.push_str(&format!("Wait-Process -Id {pid} -Timeout 30\n"));
    script.push_str("Start-Sleep -Milliseconds 300\n");
    if let Some(own) = &deferred.own_exe {
        script.push_str(&format!(
            "for ($i = 0; $i -lt 25 -and (Test-Path -LiteralPath {0}); $i++) {{ Remove-Item -LiteralPath {0} -Force; if (Test-Path -LiteralPath {0}) {{ Start-Sleep -Milliseconds 200 }} }}\n",
            quote(own)
        ));
    }
    let dir = quote(&deferred.install_dir);
    script.push_str(&format!(
        "if ((Test-Path -LiteralPath {dir}) -and -not (Get-ChildItem -LiteralPath {dir} -Force)) {{ Remove-Item -LiteralPath {dir} -Force }}\n"
    ));
    // Take the folder off the user PATH only when the host program is gone too.
    script.push_str(&format!(
        "if (-not (Test-Path -LiteralPath (Join-Path {dir} 'acpd.exe'))) {{\n\
         $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)\n\
         if ($key) {{\n\
           $raw = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)\n\
           $all = @($raw -split ';' | Where-Object {{ $_ }})\n\
           $keep = @($all | Where-Object {{ $_.TrimEnd('\\') -ne {dir}.TrimEnd('\\') }})\n\
           if ($keep.Count -ne $all.Count) {{ $key.SetValue('Path', ($keep -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString) }}\n\
           $key.Close()\n\
         }}\n\
         }}\n"
    ));
    for folder in &deferred.purge_dirs {
        script.push_str(&format!(
            "if (Test-Path -LiteralPath {0}) {{ Remove-Item -LiteralPath {0} -Recurse -Force }}\n",
            quote(folder)
        ));
    }
    script
}

#[cfg(windows)]
fn spawn_helper(deferred: &Deferred) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: it outlives the tray.
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
        ])
        .arg(encode_command(&windows_helper_script(
            deferred,
            std::process::id(),
        )))
        .creation_flags(0x0800_0000 | 0x0000_0008 | 0x0000_0200)
        .spawn()
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("start the cleanup helper: {error}"))
}

#[cfg(not(windows))]
fn spawn_helper(_: &Deferred) -> Result<()> {
    Ok(())
}

/// Stops the host, removes everything and starts the Windows helper. The caller then quits the tray.
pub fn uninstall(cli: Option<&crate::host::Cli>, purge: bool) -> Result<Vec<String>> {
    if let Some(cli) = cli {
        let _ = cli.stop();
    }
    let at = Locations::current();
    let own = std::env::current_exe().ok();
    let (summary, deferred) = remove_now(&at, purge, own.as_deref(), cfg!(windows))?;
    if cfg!(windows) {
        spawn_helper(&deferred)?;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> (tempfile::TempDir, Locations) {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data").join("acpd");
        let at = Locations {
            install_dir: data.join("bin"),
            bin_links: Some(root.path().join("links")),
            shortcut: Some(root.path().join("ACP Portal.lnk")),
            desktop_files: vec![root.path().join("acp-portal.desktop")],
            temp_dir: root.path().join("tmp"),
            settings_dir: root.path().join("config").join("acpd"),
            data_dir: data,
        };
        std::fs::create_dir_all(&at.install_dir).unwrap();
        std::fs::create_dir_all(at.bin_links.as_ref().unwrap()).unwrap();
        std::fs::create_dir_all(&at.temp_dir).unwrap();
        std::fs::create_dir_all(at.data_dir.join("state")).unwrap();
        std::fs::create_dir_all(&at.settings_dir).unwrap();
        (root, at)
    }

    fn touch(path: &Path) {
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn removes_programs_and_leftovers_but_keeps_settings_by_default() {
        let (_root, at) = scratch();
        for file in program_files("acpd")
            .into_iter()
            .chain(program_files("acpd-tray"))
        {
            touch(&at.install_dir.join(file));
        }
        touch(&at.install_dir.join("someone-elses-file"));
        touch(at.shortcut.as_ref().unwrap());
        touch(&at.desktop_files[0]);
        touch(&at.settings_dir.join("config.toml"));
        std::fs::create_dir(at.temp_dir.join("acpd-pair-1")).unwrap();
        std::fs::create_dir(at.temp_dir.join("other")).unwrap();
        let (summary, deferred) = remove_now(&at, false, None, false).unwrap();
        assert_eq!(deferred.own_exe, None);
        assert!(summary[0].starts_with("Removed 2 program file(s)"));
        for file in program_files("acpd")
            .into_iter()
            .chain(program_files("acpd-tray"))
        {
            assert!(!at.install_dir.join(file).exists());
        }
        assert!(
            at.install_dir.join("someone-elses-file").exists(),
            "other files stay"
        );
        assert!(at.install_dir.exists(), "a folder with other files stays");
        assert!(!at.shortcut.as_ref().unwrap().exists());
        assert!(!at.desktop_files[0].exists());
        assert!(!at.temp_dir.join("acpd-pair-1").exists());
        assert!(at.temp_dir.join("other").exists());
        assert!(at.settings_dir.join("config.toml").exists());
        assert!(at.data_dir.join("state").exists());
    }

    #[test]
    fn purge_deletes_settings_and_data_only_when_named_acpd() {
        let (root, mut at) = scratch();
        let (_, deferred) = remove_now(&at, true, None, false).unwrap();
        assert!(deferred.purge_dirs.is_empty());
        assert!(!at.settings_dir.exists());
        assert!(!at.data_dir.exists());
        // A folder with another name is refused.
        let other = root.path().join("precious");
        std::fs::create_dir_all(&other).unwrap();
        touch(&other.join("file"));
        at.settings_dir = other.clone();
        at.data_dir = other.clone();
        let (summary, _) = remove_now(&at, true, None, false).unwrap();
        assert!(other.join("file").exists());
        assert!(
            summary
                .iter()
                .any(|line| line.contains("not an acpd folder"))
        );
    }

    #[test]
    fn running_tray_and_its_folders_are_left_to_the_helper() {
        let (_root, at) = scratch();
        let host = at.install_dir.join(program_files("acpd")[0].clone());
        let tray = at.install_dir.join(program_files("acpd-tray")[0].clone());
        touch(&host);
        touch(&tray);
        let (_, deferred) = remove_now(&at, true, Some(&tray), true).unwrap();
        assert!(!host.exists());
        assert!(tray.exists());
        assert_eq!(deferred.own_exe.as_deref(), Some(tray.as_path()));
        assert_eq!(deferred.purge_dirs, vec![at.data_dir.clone()]);
        assert!(
            !at.settings_dir.exists(),
            "settings are outside the install folder"
        );
        assert!(at.data_dir.exists());
    }

    #[cfg(unix)]
    #[test]
    fn only_links_that_point_at_the_install_folder_are_removed() {
        let (root, at) = scratch();
        let host = at.install_dir.join("acpd");
        touch(&host);
        let links = at.bin_links.as_ref().unwrap();
        std::os::unix::fs::symlink(&host, links.join("acpd")).unwrap();
        let elsewhere = root.path().join("other-acpd");
        touch(&elsewhere);
        std::os::unix::fs::symlink(&elsewhere, links.join("acpd-tray")).unwrap();
        remove_now(&at, false, None, false).unwrap();
        assert!(std::fs::symlink_metadata(links.join("acpd")).is_err());
        assert!(std::fs::symlink_metadata(links.join("acpd-tray")).is_ok());
    }

    #[test]
    fn powershell_command_encoding_matches_known_values() {
        assert_eq!(encode_command("A"), "QQA=");
        assert_eq!(encode_command("AB"), "QQBCAA==");
        assert_eq!(encode_command("ABC"), "QQBCAEMA");
    }

    #[test]
    fn helper_script_quotes_paths_and_waits_for_the_tray() {
        let deferred = Deferred {
            own_exe: Some(PathBuf::from(r"C:\Users\o'neil\acpd\bin\acpd-tray.exe")),
            install_dir: PathBuf::from(r"C:\Users\o'neil\acpd\bin"),
            purge_dirs: vec![PathBuf::from(r"C:\Users\o'neil\acpd")],
        };
        let script = windows_helper_script(&deferred, 4242);
        assert!(script.contains("Wait-Process -Id 4242"));
        assert!(script.contains(r"'C:\Users\o''neil\acpd\bin\acpd-tray.exe'"));
        assert!(
            script.contains(r"Remove-Item -LiteralPath 'C:\Users\o''neil\acpd' -Recurse -Force")
        );
    }

    /// Runs the real helper in PowerShell against scratch files, after a short-lived "tray" exits.
    #[cfg(windows)]
    #[test]
    fn helper_deletes_the_program_folder_and_purge_dirs_after_the_tray_exits() {
        let (_root, at) = scratch();
        let tray = at.install_dir.join("acpd-tray.exe");
        touch(&tray);
        touch(&at.data_dir.join("state").join("paired.json"));
        let mut waiter = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 2"])
            .spawn()
            .unwrap();
        let deferred = Deferred {
            own_exe: Some(tray.clone()),
            install_dir: at.install_dir.clone(),
            purge_dirs: vec![at.data_dir.clone()],
        };
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
            .arg(encode_command(&windows_helper_script(
                &deferred,
                waiter.id(),
            )))
            .status()
            .unwrap();
        assert!(status.success());
        let _ = waiter.wait();
        assert!(!tray.exists());
        assert!(!at.data_dir.exists());
    }
}
