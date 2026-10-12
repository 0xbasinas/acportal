//! Finding agent programs. The host looks on `PATH` first, then in the folders where command-line
//! tools usually install themselves (a host started from a tray, a service or a login item often has a
//! shorter `PATH` than your terminal). On Windows an npm "shim" (`gemini.cmd`) is turned into what it
//! really runs, `node script.js`, so it can be started with literal arguments and no shell.
use std::{
    collections::HashSet,
    ffi::OsString,
    path::{Path, PathBuf},
};

/// How to start a found program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    /// The file that was found. This is what the phone shows (for an npm shim, the `.cmd` file).
    pub found: PathBuf,
    /// The program to start: `found` itself, or `node` for an npm shim.
    pub program: PathBuf,
    /// Arguments that go before the agent's own (the script, for an npm shim).
    pub prefix_args: Vec<String>,
    /// Folders put at the front of the child's `PATH`, so a script that starts `env node` finds the
    /// `node` that sits beside it.
    pub path_dirs: Vec<PathBuf>,
}

impl Launch {
    pub fn plain(path: &Path) -> Self {
        Self {
            found: path.to_path_buf(),
            program: path.to_path_buf(),
            prefix_args: vec![],
            path_dirs: vec![],
        }
    }
    /// The child's `PATH` with `path_dirs` first, or None when nothing needs adding.
    pub fn path_with_dirs(&self, base: Option<OsString>) -> Option<OsString> {
        if self.path_dirs.is_empty() {
            return None;
        }
        let mut dirs = self.path_dirs.clone();
        if let Some(base) = base {
            dirs.extend(std::env::split_paths(&base));
        }
        std::env::join_paths(dirs).ok()
    }
}
impl From<&Path> for Launch {
    fn from(path: &Path) -> Self {
        Self::plain(path)
    }
}
impl From<&PathBuf> for Launch {
    fn from(path: &PathBuf) -> Self {
        Self::plain(path)
    }
}
impl From<PathBuf> for Launch {
    fn from(path: PathBuf) -> Self {
        Self::plain(&path)
    }
}

/// Folders that can hold a user-installed command, best first. Nothing is checked here.
pub(crate) fn candidate_directories(
    home: Option<&Path>,
    appdata: Option<&Path>,
    local_appdata: Option<&Path>,
    program_files: Option<&Path>,
    windows: bool,
) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = vec![];
    let mut add = |path: Option<PathBuf>| list.extend(path);
    if windows {
        add(appdata.map(|p| p.join("npm")));
        add(local_appdata.map(|p| p.join("pnpm")));
        add(home.map(|p| p.join(".local").join("bin")));
        add(home.map(|p| p.join(".cargo").join("bin")));
        add(home.map(|p| p.join(".bun").join("bin")));
        add(home.map(|p| p.join("go").join("bin")));
        add(home.map(|p| p.join(".volta").join("bin")));
        add(home.map(|p| p.join(".opencode").join("bin")));
        add(local_appdata.map(|p| p.join("Microsoft").join("WinGet").join("Links")));
        add(home.map(|p| p.join("scoop").join("shims")));
        add(program_files.map(|p| p.join("nodejs")));
    } else {
        add(home.map(|p| p.join(".local").join("bin")));
        add(home.map(|p| p.join(".cargo").join("bin")));
        add(home.map(|p| p.join(".bun").join("bin")));
        add(home.map(|p| p.join("go").join("bin")));
        add(home.map(|p| p.join(".npm-global").join("bin")));
        add(home.map(|p| p.join(".volta").join("bin")));
        add(home.map(|p| p.join(".opencode").join("bin")));
        add(home.map(|p| p.join(".deno").join("bin")));
        add(home.map(|p| p.join(".local").join("share").join("pnpm")));
        for fixed in [
            "/usr/local/bin",
            "/opt/homebrew/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/snap/bin",
        ] {
            list.push(PathBuf::from(fixed));
        }
    }
    list
}

/// `bin` folders of Node versions kept by nvm and fnm, newest first.
fn node_manager_directories(home: &Path) -> Vec<PathBuf> {
    let mut found = vec![];
    for (root, suffix) in [
        (home.join(".nvm").join("versions").join("node"), "bin"),
        (
            home.join(".local")
                .join("share")
                .join("fnm")
                .join("node-versions"),
            "installation/bin",
        ),
        (home.join(".fnm").join("node-versions"), "installation/bin"),
    ] {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut versions: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        versions.sort();
        versions.reverse();
        found.extend(versions.into_iter().map(|v| v.join(suffix)));
    }
    found
}

/// The existing, absolute, de-duplicated folders worth searching after `PATH`.
pub fn known_tool_directories() -> Vec<PathBuf> {
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut list = candidate_directories(
        home.as_deref(),
        var("APPDATA").as_deref(),
        var("LOCALAPPDATA").as_deref(),
        var("ProgramFiles").as_deref(),
        cfg!(windows),
    );
    if let Some(home) = &home {
        list.extend(node_manager_directories(home));
    }
    let mut seen = HashSet::new();
    list.into_iter()
        .filter(|dir| dir.is_absolute() && dir.is_dir() && seen.insert(dir.clone()))
        .collect()
}

fn is_cmd(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"))
}

/// The script an npm-style `.cmd` shim runs: the quoted `"%dp0%\...\file.js"` before `%*`. Anything
/// else (a hand-written batch file) is not recognised, and is never run.
pub(crate) fn shim_script(text: &str) -> Option<String> {
    for line in text.lines() {
        let Some(at) = line.find("%dp0%") else {
            continue;
        };
        let after = line[at + "%dp0%".len()..].trim_start_matches(['\\', '/']);
        let Some(end) = after.find('"') else {
            continue;
        };
        let relative = &after[..end];
        let lower = relative.to_ascii_lowercase();
        let script = [".js", ".mjs", ".cjs"]
            .iter()
            .any(|ext| lower.ends_with(ext));
        if script && !relative.contains(['%', '\r', '\n', '\0']) {
            return Some(relative.to_string());
        }
    }
    None
}

fn resolve_shim(shim: &Path, extra: &[PathBuf]) -> Option<Launch> {
    let text = std::fs::read(shim)
        .ok()
        .filter(|bytes| bytes.len() <= 64 * 1024)?;
    let relative = shim_script(&String::from_utf8_lossy(&text))?;
    let directory = shim.parent()?;
    let script = directory
        .join(relative.replace('\\', "/"))
        .canonicalize()
        .ok()
        .map(|path| crate::userdata::plain_path(&path))?;
    if !script.is_file() {
        return None;
    }
    // Node: the one next to the shim (how some installs ship it), else the first on PATH or a known folder.
    let beside = directory.join(if cfg!(windows) { "node.exe" } else { "node" });
    let node = if crate::registry::executable(&beside) {
        beside
    } else {
        let found = resolve_in("node", None, extra)?;
        if !found.prefix_args.is_empty() {
            return None;
        }
        found.program
    };
    let node_dir = node.parent().map(Path::to_path_buf);
    Some(Launch {
        found: shim.to_path_buf(),
        program: node,
        prefix_args: vec![script.to_string_lossy().into_owned()],
        path_dirs: node_dir.into_iter().collect(),
    })
}

fn launch_for(path: &Path, extra: &[PathBuf]) -> Option<Launch> {
    if crate::registry::executable(path) {
        Some(Launch::plain(path))
    } else if is_cmd(path) && path.is_file() {
        resolve_shim(path, extra)
    } else {
        None
    }
}

/// Finds how to start `command`: an absolute path, or a bare name looked up on `custom_path` (default:
/// this process's `PATH`) and then in the known tool folders.
pub fn resolve(command: &str, custom_path: Option<&str>) -> Option<Launch> {
    resolve_in(command, custom_path, &known_tool_directories())
}

pub(crate) fn resolve_in(
    command: &str,
    custom_path: Option<&str>,
    extra: &[PathBuf],
) -> Option<Launch> {
    let given = Path::new(command);
    if given.is_absolute() {
        return launch_for(given, extra);
    }
    // A relative path with separators is ambiguous across workspaces.
    if command.contains(['/', '\\']) || command.is_empty() {
        return None;
    }
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut names = vec![command.to_string()];
    #[cfg(windows)]
    if given.extension().is_none() {
        let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.COM;.BAT;.CMD".into());
        names.extend(
            extensions
                .split(';')
                .filter(|ext| !ext.is_empty())
                .map(|ext| format!("{command}{ext}")),
        );
        // npm shims are .cmd; make sure they are tried even if PATHEXT was trimmed.
        if !names
            .iter()
            .any(|n| n.to_ascii_lowercase().ends_with(".cmd"))
        {
            names.push(format!("{command}.cmd"));
        }
    }
    let path_value = custom_path
        .map(OsString::from)
        .or_else(|| std::env::var_os("PATH"));
    let mut directories: Vec<(PathBuf, bool)> = vec![];
    if let Some(value) = &path_value {
        directories.extend(std::env::split_paths(value).map(|dir| (dir, true)));
    }
    directories.extend(extra.iter().cloned().map(|dir| (dir, false)));
    let mut seen = HashSet::new();
    for (directory, on_path) in directories {
        if !directory.is_absolute() || !seen.insert(directory.clone()) {
            continue;
        }
        for name in &names {
            if let Some(mut launch) = launch_for(&directory.join(name), extra) {
                // Found in a known folder rather than on PATH: let the program find its neighbours.
                if !on_path && !launch.path_dirs.contains(&directory) {
                    launch.path_dirs.push(directory.clone());
                }
                return Some(launch);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(not(unix))]
    fn make_executable(_: &Path) {}
    /// Windows may report a name in the case PATHEXT uses (`.EXE`); compare paths without case there.
    fn same(a: &Path, b: &Path) -> bool {
        if cfg!(windows) {
            a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
        } else {
            a == b
        }
    }
    fn program_name(base: &str) -> String {
        if cfg!(windows) {
            format!("{base}.exe")
        } else {
            base.into()
        }
    }
    fn write_program(dir: &Path, base: &str) -> PathBuf {
        let path = dir.join(program_name(base));
        fs::write(&path, "not really a program").unwrap();
        make_executable(&path);
        path
    }

    const NPM_SHIM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@google\\gemini-cli\\dist\\index.js\" %*\r\n";

    #[test]
    fn the_script_of_an_npm_shim_is_read_and_other_batch_files_are_not() {
        assert_eq!(
            shim_script(NPM_SHIM).as_deref(),
            Some("node_modules\\@google\\gemini-cli\\dist\\index.js")
        );
        assert_eq!(shim_script("@echo off\r\ndel /q C:\\stuff\r\n"), None);
        assert_eq!(
            shim_script("\"%dp0%\\evil.exe\" %*"),
            None,
            "only .js files are followed"
        );
        assert_eq!(
            shim_script("\"%dp0%\\a%b%.js\" %*"),
            None,
            "no variable expansion"
        );
        assert_eq!(shim_script(""), None);
    }

    #[test]
    fn an_npm_shim_becomes_node_with_the_script_as_its_first_argument() {
        let dir = tempfile::tempdir().unwrap();
        let script_dir = dir.path().join("node_modules/@google/gemini-cli/dist");
        fs::create_dir_all(&script_dir).unwrap();
        fs::write(script_dir.join("index.js"), "// cli").unwrap();
        let shim = dir.path().join("gemini.cmd");
        fs::write(&shim, NPM_SHIM).unwrap();
        let node = write_program(dir.path(), "node");
        let launch = resolve_shim(&shim, &[]).unwrap();
        assert_eq!(launch.found, shim);
        assert_eq!(launch.program, node);
        assert_eq!(launch.prefix_args.len(), 1);
        assert!(
            launch.prefix_args[0]
                .replace('\\', "/")
                .ends_with("gemini-cli/dist/index.js")
        );
        assert_eq!(launch.path_dirs, vec![dir.path().to_path_buf()]);
    }

    #[test]
    fn a_shim_whose_script_is_missing_or_unrecognised_is_not_launchable() {
        let dir = tempfile::tempdir().unwrap();
        write_program(dir.path(), "node");
        let shim = dir.path().join("gone.cmd");
        fs::write(&shim, NPM_SHIM).unwrap();
        assert_eq!(
            resolve_shim(&shim, &[]),
            None,
            "the script file does not exist"
        );
        let plain = dir.path().join("plain.cmd");
        fs::write(&plain, "@echo off\r\necho hi\r\n").unwrap();
        assert_eq!(resolve_shim(&plain, &[]), None);
    }

    #[test]
    fn a_program_outside_path_is_found_in_a_known_folder_and_brings_its_folder_along() {
        let on_path = tempfile::tempdir().unwrap();
        let known = tempfile::tempdir().unwrap();
        let program = write_program(known.path(), "my-agent-cli");
        let path_value = on_path.path().to_string_lossy().into_owned();
        // Not on PATH: missing without the known folder, found with it.
        assert_eq!(resolve_in("my-agent-cli", Some(&path_value), &[]), None);
        let launch = resolve_in(
            "my-agent-cli",
            Some(&path_value),
            &[known.path().to_path_buf()],
        )
        .unwrap();
        assert!(
            same(&launch.program, &program),
            "{:?} vs {:?}",
            launch.program,
            program
        );
        assert_eq!(launch.path_dirs, vec![known.path().to_path_buf()]);
        // On PATH: found first, and no extra folder is added.
        let both = std::env::join_paths([on_path.path(), known.path()]).unwrap();
        let direct = resolve_in("my-agent-cli", Some(both.to_str().unwrap()), &[]).unwrap();
        assert!(direct.path_dirs.is_empty());
    }

    #[test]
    fn path_wins_over_known_folders_and_junk_input_is_refused() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let wanted = write_program(first.path(), "tool");
        write_program(second.path(), "tool");
        let path_value = first.path().to_string_lossy().into_owned();
        let found = resolve_in("tool", Some(&path_value), &[second.path().to_path_buf()]).unwrap();
        assert!(
            same(&found.program, &wanted),
            "PATH wins: {:?}",
            found.program
        );
        for junk in ["", "./tool", "sub/tool", "..\\tool"] {
            assert_eq!(resolve_in(junk, Some(&path_value), &[]), None, "{junk:?}");
        }
        let missing = first.path().join(program_name("nope"));
        assert_eq!(resolve_in(missing.to_str().unwrap(), None, &[]), None);
        assert_eq!(
            resolve_in(wanted.to_str().unwrap(), None, &[])
                .unwrap()
                .program,
            wanted
        );
    }

    #[test]
    fn candidate_folders_cover_the_usual_installs_on_each_system() {
        let home = Path::new("/home/me");
        let linux = candidate_directories(Some(home), None, None, None, false);
        for expected in [
            "/home/me/.local/bin",
            "/home/me/.cargo/bin",
            "/home/me/.npm-global/bin",
            "/usr/local/bin",
            "/snap/bin",
        ] {
            assert!(linux.contains(&PathBuf::from(expected)), "{expected}");
        }
        let windows = candidate_directories(
            Some(Path::new("C:/Users/me")),
            Some(Path::new("C:/Users/me/AppData/Roaming")),
            Some(Path::new("C:/Users/me/AppData/Local")),
            Some(Path::new("C:/Program Files")),
            true,
        );
        for expected in [
            "C:/Users/me/AppData/Roaming/npm",
            "C:/Users/me/AppData/Local/pnpm",
            "C:/Users/me/.local/bin",
            "C:/Users/me/AppData/Local/Microsoft/WinGet/Links",
            "C:/Program Files/nodejs",
        ] {
            assert!(windows.contains(&PathBuf::from(expected)), "{expected}");
        }
        assert!(
            !windows.iter().any(|p| p.starts_with("/usr")),
            "no Unix folders on Windows"
        );
    }

    #[test]
    fn nvm_node_versions_are_searched_newest_first() {
        let home = tempfile::tempdir().unwrap();
        for version in ["v18.0.0", "v22.1.0", "v20.5.0"] {
            fs::create_dir_all(
                home.path()
                    .join(".nvm/versions/node")
                    .join(version)
                    .join("bin"),
            )
            .unwrap();
        }
        let found = node_manager_directories(home.path());
        let names: Vec<String> = found
            .iter()
            .map(|p| {
                p.parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, ["v22.1.0", "v20.5.0", "v18.0.0"]);
    }

    #[test]
    fn the_child_path_gets_the_launch_folders_first() {
        let launch = Launch {
            path_dirs: vec![PathBuf::from("/opt/node/bin")],
            ..Launch::plain(Path::new("/opt/node/bin/agent"))
        };
        let joined = launch
            .path_with_dirs(Some(OsString::from("/usr/bin")))
            .unwrap();
        let parts: Vec<PathBuf> = std::env::split_paths(&joined).collect();
        assert_eq!(
            parts,
            [PathBuf::from("/opt/node/bin"), PathBuf::from("/usr/bin")]
        );
        assert_eq!(
            Launch::plain(Path::new("/x")).path_with_dirs(Some(OsString::from("/usr/bin"))),
            None
        );
    }
}
