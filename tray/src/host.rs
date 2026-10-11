//! Talks to the `acpd` command-line program. The tray never reimplements host logic: it runs the
//! same commands you would type (`acpd start --bg`, `acpd daemon stop`, `acpd pair --manual`, ...),
//! so behaviour and files stay identical to the command line.
use anyhow::{Context, Result, bail};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostState {
    /// The `acpd` program was not found; the tray can download it.
    NotInstalled,
    /// No saved setup yet (`acpd setup` has not been run).
    NotSetUp,
    Stopped,
    Running {
        pid: u32,
    },
    /// The host answered with something unexpected; shown as is.
    Other(String),
}

/// A fresh one-use pairing code and the address saved by setup.
#[derive(Debug, PartialEq, Eq)]
pub struct Pairing {
    pub code: String,
    pub address: String,
}

/// Places the host program may be, best first: beside the tray as `acpd` or under its release name
/// (`acpd-windows-x86_64.exe`, `acpd-linux-x86_64`), then the folder the tray installs it into.
pub fn candidates(beside: Option<&Path>, installed_dir: &Path) -> Vec<PathBuf> {
    let release = if cfg!(windows) {
        "acpd-windows-x86_64.exe"
    } else {
        "acpd-linux-x86_64"
    };
    let mut list = Vec::new();
    if let Some(dir) = beside {
        list.push(dir.join(crate::install::installed_name()));
        list.push(dir.join(release));
    }
    list.push(installed_dir.join(crate::install::installed_name()));
    list
}

/// The first candidate that exists, else `acpd` on PATH when that runs, else None.
pub fn find_host(beside: Option<&Path>, installed_dir: &Path) -> Option<PathBuf> {
    candidates(beside, installed_dir)
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| {
            let on_path = PathBuf::from("acpd");
            Cli {
                exe: on_path.clone(),
            }
            .run(&["--version"])
            .ok()
            .filter(|out| out.status.success())
            .map(|_| on_path)
        })
}

pub struct Cli {
    exe: PathBuf,
}

/// The tray's handle on the host program: it can be missing at start and appear after an install.
pub struct Control {
    cli: std::sync::Mutex<Option<std::sync::Arc<Cli>>>,
}

impl Control {
    pub fn new() -> Self {
        let control = Self {
            cli: std::sync::Mutex::new(None),
        };
        control.refresh();
        control
    }
    /// Looks for the host again (after an install, or when it was copied next to the tray).
    pub fn refresh(&self) {
        let beside = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        let found = find_host(beside.as_deref(), &crate::install::install_dir())
            .map(|exe| std::sync::Arc::new(Cli { exe }));
        *self.cli.lock().expect("host lock") = found;
    }
    pub fn get(&self) -> Option<std::sync::Arc<Cli>> {
        self.cli.lock().expect("host lock").clone()
    }
    pub fn status(&self, settings: &Path) -> HostState {
        match self.get() {
            None => HostState::NotInstalled,
            Some(cli) => cli.status(settings),
        }
    }
    /// Runs `action` with the host program, or explains that it is not installed.
    pub fn with<T>(&self, action: impl FnOnce(&Cli) -> Result<T>) -> Result<T> {
        match self.get() {
            Some(cli) => action(&cli),
            None => {
                bail!("The ACP Portal host is not installed. Choose Download host in the menu.")
            }
        }
    }
}

impl Cli {
    #[cfg(test)]
    pub fn with_exe(exe: &Path) -> Self {
        Self { exe: exe.into() }
    }
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        let mut command = Command::new(&self.exe);
        command.args(args);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // no console window
        command
            .output()
            .with_context(|| format!("run {}", self.exe.display()))
    }
    fn run_ok(&self, args: &[&str]) -> Result<String> {
        let output = self.run(args)?;
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            bail!("{}", first_line(&String::from_utf8_lossy(&output.stderr)))
        }
        Ok(text)
    }
    pub fn status(&self, settings: &Path) -> HostState {
        if !settings.join("launcher.json").is_file() {
            return HostState::NotSetUp;
        }
        match self.run(&["daemon", "status"]) {
            Ok(output) if output.status.success() => {
                parse_status(&String::from_utf8_lossy(&output.stdout))
            }
            Ok(output) => HostState::Other(first_line(&String::from_utf8_lossy(&output.stderr))),
            Err(error) => HostState::Other(error.to_string()),
        }
    }
    pub fn start(&self) -> Result<()> {
        self.run_ok(&["start", "--bg"]).map(|_| ())
    }
    pub fn stop(&self) -> Result<()> {
        self.run_ok(&["daemon", "stop"]).map(|_| ())
    }
    pub fn setup(&self, address: &str, name: &str) -> Result<()> {
        self.run_ok(&["setup", "--address", address, "--name", name])
            .map(|_| ())
    }
    /// Creates a one-use pairing code (valid 60 seconds) with the saved address.
    pub fn pair(&self) -> Result<Pairing> {
        parse_pairing(&self.run_ok(&["pair", "--manual"])?)
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown error")
        .trim()
        .chars()
        .take(160)
        .collect()
}

/// Reads `acpd daemon status` output: `Host running (PID 4242).` or `No managed host ...`.
pub fn parse_status(stdout: &str) -> HostState {
    let line = first_line(stdout);
    if line.starts_with("No managed host") {
        return HostState::Stopped;
    }
    if let Some(rest) = line.strip_prefix("Host running (PID ")
        && let Some(pid) = rest.trim_end_matches(['.', ')']).trim().parse().ok()
    {
        return HostState::Running { pid };
    }
    HostState::Other(line)
}

/// Reads `acpd pair --manual` output: a `Pairing code:` line and a `Host address:` line.
pub fn parse_pairing(stdout: &str) -> Result<Pairing> {
    let field = |label: &str| {
        stdout
            .lines()
            .find_map(|line| line.trim().strip_prefix(label))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let code = field("Pairing code:").context("acpd did not print a pairing code")?;
    let address = field("Host address:").context(
        "No phone address is saved yet. Choose Set up with Tailscale, or run acpd setup --address https://YOUR-HOST",
    )?;
    Ok(Pairing { code, address })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_text_maps_to_states() {
        assert_eq!(
            parse_status("Host running (PID 4242).\n"),
            HostState::Running { pid: 4242 }
        );
        assert_eq!(
            parse_status("No managed host for this config.\n"),
            HostState::Stopped
        );
        assert_eq!(
            parse_status("something else\n"),
            HostState::Other("something else".into())
        );
        assert_eq!(parse_status(""), HostState::Other("unknown error".into()));
    }
    #[test]
    fn pairing_output_is_read_and_missing_address_explained() {
        let ok = parse_pairing(
            "Pairing code: ABCD-1234-EF56\nExpires in 60 seconds.\nHost address: https://laptop.example\n█▀▀█\n",
        )
        .unwrap();
        assert_eq!(ok.code, "ABCD-1234-EF56");
        assert_eq!(ok.address, "https://laptop.example");
        let no_address = parse_pairing("Pairing code: ABCD-1234-EF56\nExpires in 60 seconds.\n");
        assert!(
            no_address
                .unwrap_err()
                .to_string()
                .contains("No phone address")
        );
        assert!(parse_pairing("nothing useful").is_err());
    }
    fn release_name() -> &'static str {
        if cfg!(windows) {
            "acpd-windows-x86_64.exe"
        } else {
            "acpd-linux-x86_64"
        }
    }
    #[test]
    fn host_is_found_beside_the_tray_under_either_name_then_in_the_install_folder() {
        let beside = tempfile::tempdir().unwrap();
        let installed = tempfile::tempdir().unwrap();
        let plain = beside.path().join(crate::install::installed_name());
        let release = beside.path().join(release_name());
        let fallback = installed.path().join(crate::install::installed_name());
        // The release-named download is found with no renaming.
        std::fs::write(&release, "x").unwrap();
        assert_eq!(
            find_host(Some(beside.path()), installed.path()),
            Some(release.clone())
        );
        // A plainly named acpd beside the tray wins over it.
        std::fs::write(&plain, "x").unwrap();
        assert_eq!(
            find_host(Some(beside.path()), installed.path()),
            Some(plain.clone())
        );
        // Beside the tray beats the install folder.
        std::fs::write(&fallback, "x").unwrap();
        assert_eq!(
            find_host(Some(beside.path()), installed.path()),
            Some(plain)
        );
        // With nothing beside the tray, the install folder is used.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            find_host(Some(empty.path()), installed.path()),
            Some(fallback)
        );
        assert_eq!(candidates(Some(beside.path()), installed.path()).len(), 3);
    }
    #[test]
    fn control_reports_not_installed_and_explains_actions() {
        let control = Control {
            cli: std::sync::Mutex::new(None),
        };
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(control.status(dir.path()), HostState::NotInstalled);
        let error = control.with(|cli| cli.start()).unwrap_err();
        assert!(error.to_string().contains("not installed"));
    }
    #[test]
    fn setup_state_comes_from_the_launcher_file() {
        let dir = tempfile::tempdir().unwrap();
        let cli = Cli::with_exe(Path::new("definitely-not-a-real-program"));
        assert_eq!(cli.status(dir.path()), HostState::NotSetUp);
        std::fs::write(dir.path().join("launcher.json"), "{}").unwrap();
        assert!(matches!(cli.status(dir.path()), HostState::Other(_)));
    }
}
