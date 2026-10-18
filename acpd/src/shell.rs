//! Which shell runs host shell lines (ACP `terminal/create` lines and pre-merge checks).
//!
//! Unix always uses `/bin/sh -c`. On Windows the host setting `[windows] shell` (with an
//! optional `bash_path`) and the agent's `shell` override choose between `cmd` (the default,
//! `auto` keeps it), PowerShell (`pwsh`, then Windows PowerShell) and Git Bash. Git Bash is
//! found through `bash_path`, the Git for Windows registry key, common install folders and
//! `git` on PATH; the WSL launcher in System32 is never used. A choice that cannot be met falls
//! back to pwsh → powershell → cmd and says why in the approval card.
//!
//! Git Bash (MSYS2) cannot start inside the Windows AppContainer sandbox (see
//! `docs/security.md`), so a sandboxed line asked to use bash runs in PowerShell or cmd instead,
//! with a note; bash runs only for agents with `sandbox: off`.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellChoice {
    #[default]
    Auto,
    Cmd,
    Pwsh,
    Bash,
}
impl ShellChoice {
    pub fn is_auto(&self) -> bool {
        *self == ShellChoice::Auto
    }
}

/// `[windows]` in `config.toml`. Ignored on other systems.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowsConfig {
    pub shell: ShellChoice,
    pub bash_path: Option<PathBuf>,
}

static HOST: RwLock<Option<WindowsConfig>> = RwLock::new(None);

/// Set at start and on reload; later sessions and checks use it.
pub fn set_host(config: &WindowsConfig) {
    *HOST.write().unwrap() = Some(config.clone());
}
#[cfg(windows)]
fn host() -> WindowsConfig {
    HOST.read().unwrap().clone().unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    Sh,
    Cmd,
    PowerShell,
    Bash,
}

/// A resolved shell: program, how lines are passed, and why it differs from the choice.
#[derive(Clone, Debug)]
pub struct Shell {
    pub kind: ShellKind,
    pub executable: PathBuf,
    pub note: Option<String>,
}

impl Shell {
    /// Shown in consent next to the line.
    pub fn label(&self) -> String {
        match self.kind {
            ShellKind::Sh => "/bin/sh -c".into(),
            ShellKind::Cmd => "cmd.exe /D /S /C".into(),
            ShellKind::PowerShell => format!("{} -NoProfile -Command", file_name(&self.executable)),
            ShellKind::Bash => "Git Bash (bash --noprofile --norc -c)".into(),
        }
    }
    /// Short name for metadata (`sh`, `cmd`, `powershell`, `bash`).
    pub fn name(&self) -> &'static str {
        match self.kind {
            ShellKind::Sh => "sh",
            ShellKind::Cmd => "cmd",
            ShellKind::PowerShell => "powershell",
            ShellKind::Bash => "bash",
        }
    }
    /// cmd.exe stops at a line break, so a multi-line line could not run as approved.
    pub fn allows_multiline(&self) -> bool {
        self.kind != ShellKind::Cmd
    }
    /// Append the arguments that make the shell run `line` exactly once, unparsed by the host.
    pub fn push_line(&self, command: &mut tokio::process::Command, line: &str) {
        match self.kind {
            ShellKind::Sh => {
                command.arg("-c").arg(line);
            }
            ShellKind::Bash => {
                command.args(["--noprofile", "--norc", "-c"]).arg(line);
            }
            ShellKind::PowerShell => {
                command
                    .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
                    .arg(line);
            }
            ShellKind::Cmd => {
                // /S makes cmd strip only the outer quotes and run the inner text unchanged.
                #[cfg(windows)]
                command.raw_arg("/D /S /C").raw_arg(format!("\"{line}\""));
                #[cfg(not(windows))]
                command.args(["/D", "/S", "/C", line]);
            }
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The shell for a line. `agent` is the agent's override (`None` or `auto` = host setting).
pub fn resolve(agent: Option<ShellChoice>, sandboxed: bool) -> Result<Shell> {
    #[cfg(unix)]
    {
        let _ = (agent, sandboxed);
        let shell = PathBuf::from("/bin/sh");
        anyhow::ensure!(shell.is_file(), "/bin/sh unavailable");
        Ok(Shell {
            kind: ShellKind::Sh,
            executable: shell,
            note: None,
        })
    }
    #[cfg(windows)]
    {
        let config = host();
        let choice = match agent {
            Some(choice) if !choice.is_auto() => choice,
            _ => config.shell,
        };
        let found = Found {
            cmd: windows::cmd(),
            pwsh: windows::pwsh(),
            powershell: windows::powershell(),
            bash: if choice == ShellChoice::Bash {
                windows::find_bash(config.bash_path.as_deref())
            } else {
                None
            },
        };
        pick(choice, sandboxed, found)
    }
}

/// Programs that exist on this host (Windows), for [`pick`].
#[derive(Default)]
pub struct Found {
    pub cmd: Option<PathBuf>,
    pub pwsh: Option<PathBuf>,
    pub powershell: Option<PathBuf>,
    pub bash: Option<PathBuf>,
}

/// Whether Git Bash can run inside the Windows sandbox. It cannot: the MSYS2 runtime needs
/// named objects an AppContainer may not create (checked by a Windows test).
pub const BASH_IN_SANDBOX: bool = false;

/// The Windows choice logic, pure so every platform tests it.
pub fn pick(choice: ShellChoice, sandboxed: bool, found: Found) -> Result<Shell> {
    let cmd = |note: Option<String>| -> Result<Shell> {
        Ok(Shell {
            kind: ShellKind::Cmd,
            executable: found.cmd.clone().context("cmd.exe unavailable")?,
            note,
        })
    };
    let powershell = |why: &str| -> Result<Shell> {
        if let Some(p) = found.pwsh.clone().or(found.powershell.clone()) {
            return Ok(Shell {
                kind: ShellKind::PowerShell,
                executable: p,
                note: (!why.is_empty()).then(|| format!("{why}; runs in PowerShell.")),
            });
        }
        cmd(Some(if why.is_empty() {
            "PowerShell not found; runs in cmd.exe.".into()
        } else {
            format!("{why}, and PowerShell was not found; runs in cmd.exe.")
        }))
    };
    match choice {
        ShellChoice::Auto | ShellChoice::Cmd => cmd(None),
        ShellChoice::Pwsh => powershell(""),
        ShellChoice::Bash => match found.bash.clone() {
            None => powershell("Git Bash not found"),
            Some(_) if sandboxed && !BASH_IN_SANDBOX => powershell(
                "Git Bash cannot run inside the Windows sandbox (set this agent's sandbox to off to use it)",
            ),
            Some(bash) => Ok(Shell {
                kind: ShellKind::Bash,
                executable: bash,
                note: None,
            }),
        },
    }
}

/// Git Bash as found on this Windows host with no `bash_path` (tests).
#[cfg(windows)]
pub fn windows_find_bash_for_tests() -> Option<PathBuf> {
    windows::find_bash(None)
}

/// WSL's `bash.exe` in System32 (or SysWOW64) is not Git Bash.
pub fn is_wsl_bash(path: &Path) -> bool {
    let text = path
        .to_string_lossy()
        .to_ascii_lowercase()
        .replace('/', "\\");
    text.contains("\\system32\\")
        || text.contains("\\syswow64\\")
        || text.contains("\\windowsapps\\")
}

/// Git Bash locations to try, in order, from the Git install folders known so far.
pub fn bash_candidates(
    configured: Option<&Path>,
    install_roots: &[PathBuf],
    git_on_path: Option<&Path>,
) -> Vec<PathBuf> {
    let mut out = vec![];
    if let Some(path) = configured {
        out.push(path.to_path_buf());
    }
    for root in install_roots {
        out.push(root.join("bin").join("bash.exe"));
    }
    // git.exe lives in <root>\cmd, <root>\bin or <root>\mingw64\bin.
    if let Some(git) = git_on_path {
        for root in git.ancestors().skip(1).take(3) {
            out.push(root.join("bin").join("bash.exe"));
        }
    }
    out.retain(|p| !is_wsl_bash(p));
    out
}

/// True for an executable linked against the MSYS2 or Cygwin runtime (Git Bash's `bash`,
/// `grep`, `sed`, …), which cannot start in the Windows sandbox.
pub fn is_msys_image(bytes: &[u8]) -> bool {
    fn has(bytes: &[u8], needle: &[u8]) -> bool {
        bytes
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle))
    }
    bytes.starts_with(b"MZ") && (has(bytes, b"msys-2.0.dll") || has(bytes, b"cygwin1.dll"))
}

/// Reads at most 16 MiB of `path` and checks [`is_msys_image`].
pub fn is_msys_program(path: &Path) -> bool {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut bytes = vec![];
    file.take(16 << 20).read_to_end(&mut bytes).is_ok() && is_msys_image(&bytes)
}

/// A shell a person can pick for an interactive terminal. The id is what a client sends; the host
/// looks the program up itself, so a client can never name an executable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ShellOption {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub path: PathBuf,
}

/// The shells installed on this host that a terminal can start.
pub fn available_shells() -> Vec<ShellOption> {
    let mut out: Vec<ShellOption> = Vec::new();
    let mut add = |id: &str, name: &str, path: Option<PathBuf>| {
        if let Some(path) = path.filter(|p| p.is_file())
            && !out.iter().any(|o| o.id == id)
        {
            out.push(ShellOption {
                id: id.to_string(),
                name: name.to_string(),
                path,
            });
        }
    };
    #[cfg(windows)]
    {
        add("cmd", "Command Prompt", windows::cmd());
        add("pwsh", "PowerShell 7", windows::pwsh());
        add("powershell", "Windows PowerShell", windows::powershell());
        add(
            "bash",
            "Git Bash",
            windows::find_bash(host().bash_path.as_deref()),
        );
    }
    #[cfg(unix)]
    {
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Some(shell) = std::env::var_os("SHELL").map(PathBuf::from)
            && shell.is_absolute()
        {
            paths.push(shell);
        }
        if let Ok(text) = std::fs::read_to_string("/etc/shells") {
            for line in text.lines().map(str::trim) {
                if line.starts_with('/') {
                    paths.push(PathBuf::from(line));
                }
            }
        }
        paths.push(PathBuf::from("/bin/sh"));
        for path in paths {
            let id = file_name(&path);
            if matches!(id.as_str(), "nologin" | "false" | "git-shell" | "") {
                continue;
            }
            let name = match id.as_str() {
                "bash" => "Bash".to_string(),
                "zsh" => "Zsh".to_string(),
                "sh" => "sh".to_string(),
                other => other.to_string(),
            };
            add(&id, &name, Some(path));
        }
    }
    out
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
    };

    fn system32() -> Option<PathBuf> {
        Some(PathBuf::from(std::env::var_os("SystemRoot")?).join("System32"))
    }
    /// The system cmd.exe, never one found through PATH or the agent's environment.
    pub fn cmd() -> Option<PathBuf> {
        system32()
            .map(|s| s.join("cmd.exe"))
            .filter(|p| p.is_file())
    }
    pub fn powershell() -> Option<PathBuf> {
        system32()
            .map(|s| s.join(r"WindowsPowerShell\v1.0\powershell.exe"))
            .filter(|p| p.is_file())
    }
    pub fn pwsh() -> Option<PathBuf> {
        let installed = std::env::var_os("ProgramFiles")
            .map(|p| PathBuf::from(p).join(r"PowerShell\7\pwsh.exe"));
        installed
            .filter(|p| p.is_file())
            .or_else(|| crate::registry::resolve_executable("pwsh", None))
    }
    fn registry_install(root: HKEY) -> Option<PathBuf> {
        let key: Vec<u16> = "SOFTWARE\\GitForWindows\0".encode_utf16().collect();
        let name: Vec<u16> = "InstallPath\0".encode_utf16().collect();
        let mut buffer = vec![0u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as _,
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let len = buffer.iter().position(|c| *c == 0).unwrap_or(0);
        Some(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..len])))
    }
    pub fn find_bash(configured: Option<&Path>) -> Option<PathBuf> {
        let mut roots: Vec<PathBuf> = [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
            .into_iter()
            .filter_map(registry_install)
            .collect();
        for (var, sub) in [
            ("ProgramFiles", "Git"),
            ("ProgramW6432", "Git"),
            ("LocalAppData", r"Programs\Git"),
            ("USERPROFILE", r"scoop\apps\git\current"),
        ] {
            if let Some(base) = std::env::var_os(var) {
                roots.push(PathBuf::from(base).join(sub));
            }
        }
        let git = crate::registry::resolve_executable("git", None);
        bash_candidates(configured, &roots, git.as_deref())
            .into_iter()
            .find(|p| p.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn found(bash: bool, pwsh: bool) -> Found {
        Found {
            cmd: Some("C:\\Windows\\System32\\cmd.exe".into()),
            pwsh: pwsh.then(|| "C:\\Program Files\\PowerShell\\7\\pwsh.exe".into()),
            powershell: None,
            bash: bash.then(|| "C:\\Program Files\\Git\\bin\\bash.exe".into()),
        }
    }
    #[test]
    fn choices_fall_back_with_a_reason() {
        assert_eq!(
            pick(ShellChoice::Auto, false, found(true, true))
                .unwrap()
                .kind,
            ShellKind::Cmd
        );
        assert_eq!(
            pick(ShellChoice::Bash, false, found(true, true))
                .unwrap()
                .kind,
            ShellKind::Bash
        );
        let sandboxed = pick(ShellChoice::Bash, true, found(true, true)).unwrap();
        assert_eq!(sandboxed.kind, ShellKind::PowerShell);
        assert!(sandboxed.note.unwrap().contains("sandbox"));
        let missing = pick(ShellChoice::Bash, false, found(false, false)).unwrap();
        assert_eq!(missing.kind, ShellKind::Cmd);
        assert!(missing.note.unwrap().contains("Git Bash not found"));
        let pwsh = pick(ShellChoice::Pwsh, true, found(false, true)).unwrap();
        assert_eq!((pwsh.kind, pwsh.note), (ShellKind::PowerShell, None));
        assert!(
            pick(ShellChoice::Pwsh, false, found(false, false))
                .unwrap()
                .note
                .unwrap()
                .contains("cmd.exe")
        );
        assert!(
            !pick(ShellChoice::Cmd, false, found(false, false))
                .unwrap()
                .allows_multiline()
        );
    }
    #[test]
    fn bash_is_found_from_git_installs_and_never_wsl() {
        let list = bash_candidates(
            Some(Path::new("C:\\Windows\\System32\\bash.exe")),
            &[PathBuf::from("D:\\Git")],
            Some(Path::new("E:\\tools\\Git\\cmd\\git.exe")),
        );
        assert_eq!(
            list[0],
            PathBuf::from("D:\\Git").join("bin").join("bash.exe")
        );
        // Backslash paths only split on Windows.
        #[cfg(windows)]
        assert!(list.contains(&Path::new("E:\\tools\\Git").join("bin").join("bash.exe")));
        assert!(list.iter().all(|p| !is_wsl_bash(p)));
        assert!(is_wsl_bash(Path::new("C:/Windows/system32/bash.exe")));
    }
    #[test]
    fn msys_programs_are_recognised_by_their_runtime_import() {
        let mut image = b"MZ\x90\0".to_vec();
        image.extend_from_slice(b"....KERNEL32.dll\0msys-2.0.dll\0");
        assert!(is_msys_image(&image));
        assert!(is_msys_image(b"MZ..CYGWIN1.DLL"));
        assert!(!is_msys_image(b"MZ..KERNEL32.dll"));
        assert!(!is_msys_image(b"msys-2.0.dll"));
    }
    #[test]
    fn the_windows_section_parses() {
        let c: WindowsConfig =
            toml::from_str("shell = \"bash\"\nbash_path = 'D:\\Git\\bin\\bash.exe'\n").unwrap();
        assert_eq!(c.shell, ShellChoice::Bash);
        assert!(toml::from_str::<WindowsConfig>("shell = \"zsh\"").is_err());
    }
}
