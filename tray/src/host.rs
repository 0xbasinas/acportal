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

pub struct Cli {
    exe: PathBuf,
}

impl Cli {
    /// Finds `acpd` next to this program (how the release is shipped), else on PATH.
    pub fn locate() -> Result<Self> {
        let name = if cfg!(windows) { "acpd.exe" } else { "acpd" };
        let beside = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|dir| dir.join(name)))
            .filter(|path| path.is_file());
        let exe = beside.unwrap_or_else(|| PathBuf::from("acpd"));
        let cli = Self { exe };
        cli.run(&["--version"])
            .context("acpd was not found. Put acpd next to this program or on your PATH.")?;
        Ok(cli)
    }
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
    #[test]
    fn setup_state_comes_from_the_launcher_file() {
        let dir = tempfile::tempdir().unwrap();
        let cli = Cli::with_exe(Path::new("definitely-not-a-real-program"));
        assert_eq!(cli.status(dir.path()), HostState::NotSetUp);
        std::fs::write(dir.path().join("launcher.json"), "{}").unwrap();
        assert!(matches!(cli.status(dir.path()), HostState::Other(_)));
    }
}
