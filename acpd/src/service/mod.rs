//! User-level service registration. systemd, launchd and Task Scheduler start
//! `acpd start` in the foreground. This module does not detach a process and
//! does not copy the shell environment into the service definition.

use crate::{config::Config, launcher::Launcher};
use anyhow::{Context, Result, anyhow, bail};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

mod launchd;
mod schtasks;
mod systemd;
#[cfg(test)]
mod tests;

#[allow(unused_imports)]
use launchd::*;
#[allow(unused_imports)]
use schtasks::*;
#[allow(unused_imports)]
use systemd::*;
pub use systemd::{SystemctlShow, systemctl_show};

pub const MARKER: &str = "acpd-service-marker: acportal";
const OUTPUT_CAP: usize = 64 * 1024;
/// Printed after install: a service starts with the service manager's environment, not a shell's.
const ENV_NOTE: &str = "The service does not inherit your shell environment. Put agent keys and PATH entries in the agent registry `env` (docs/agents.md) or in the agent's own configuration. If `acpd daemon start` is already running this host, stop it first with `acpd daemon stop`.";
const LINGER_HINT: &str = "To keep the host after logout, run: loginctl enable-linger";

#[derive(Clone, Copy)]
pub enum Action {
    Install,
    Status,
    Restart,
    Uninstall,
}

pub fn execute(action: Action, profile: &Path) -> Result<()> {
    let message = match action {
        Action::Install => install(profile)?,
        Action::Status => status(profile)?,
        Action::Restart => restart()?,
        Action::Uninstall => uninstall()?,
    };
    println!("{message}");
    Ok(())
}

fn install(profile: &Path) -> Result<String> {
    let exe = current_exe_plain()?;
    if !exe.is_file() {
        bail!("the acpd executable is not a file");
    }
    let profile = require_setup(profile)?;
    let mut runner = ProcessRunner;
    #[cfg(target_os = "linux")]
    {
        install_systemd(&systemd_unit_path()?, &exe, &profile, &mut runner)
    }
    #[cfg(target_os = "macos")]
    {
        install_launchd(
            &launchd_plist_path()?,
            &exe,
            &profile,
            current_uid(),
            &mut runner,
        )
    }
    #[cfg(windows)]
    {
        let xml = std::env::temp_dir().join(format!("acpd-service-{}.xml", uuid::Uuid::new_v4()));
        install_task(&xml, &exe, &profile, &windows_user()?, &mut runner)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (exe, profile, runner);
        bail!("acpd service install supports Linux, macOS and Windows")
    }
}

fn status(profile: &Path) -> Result<String> {
    let mut runner = ProcessRunner;
    #[cfg(target_os = "linux")]
    {
        linux_status(&systemd_unit_path()?, profile, &mut runner)
    }
    #[cfg(target_os = "macos")]
    {
        launchd_status(&launchd_plist_path()?, profile, current_uid(), &mut runner)
    }
    #[cfg(windows)]
    {
        task_status(profile, &mut runner)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (profile, runner);
        bail!("acpd service status supports Linux, macOS and Windows")
    }
}

fn restart() -> Result<String> {
    let mut runner = ProcessRunner;
    #[cfg(target_os = "linux")]
    {
        restart_systemd(&systemd_unit_path()?, &mut runner)
    }
    #[cfg(target_os = "macos")]
    {
        restart_launchd(&launchd_plist_path()?, current_uid(), &mut runner)
    }
    #[cfg(windows)]
    {
        restart_task(&mut runner)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = runner;
        bail!("acpd service restart supports Linux, macOS and Windows")
    }
}

fn uninstall() -> Result<String> {
    let mut runner = ProcessRunner;
    #[cfg(target_os = "linux")]
    {
        uninstall_systemd(&systemd_unit_path()?, &mut runner)
    }
    #[cfg(target_os = "macos")]
    {
        uninstall_launchd(&launchd_plist_path()?, current_uid(), &mut runner)
    }
    #[cfg(windows)]
    {
        uninstall_task(&mut runner)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = runner;
        bail!("acpd service uninstall supports Linux, macOS and Windows")
    }
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn home() -> Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .context("the home directory is not available")
}

fn current_exe_plain() -> Result<PathBuf> {
    let path = std::env::current_exe().context("find the acpd executable")?;
    let path = plain_path(&path)?;
    check_literal(&path)?;
    Ok(path)
}

fn require_setup(profile: &Path) -> Result<PathBuf> {
    if !profile.is_file() {
        bail!("run acpd setup before acpd service install");
    }
    let profile = plain_path(&std::path::absolute(profile).context("resolve the setup profile")?)?;
    check_literal(&profile)?;
    let saved = Launcher::load(&profile)?;
    Config::load(&saved.config)?;
    Ok(profile)
}

fn plain_path(path: &Path) -> Result<PathBuf> {
    let text = path.to_str().context("a service path must be valid text")?;
    let text = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        text.to_string()
    };
    Ok(PathBuf::from(text))
}

fn check_literal(path: &Path) -> Result<()> {
    let text = path.to_str().context("a service path must be valid text")?;
    if text.len() > 4096 {
        bail!("a service path is too long");
    }
    if !path.is_absolute() {
        bail!("a service path must be absolute");
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!("a service path must not contain ..");
    }
    if text.chars().any(|c| c.is_control()) {
        bail!("a service path contains a control character");
    }
    Ok(())
}

#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn foreground_args(exe: &Path, profile: &Path) -> Result<(String, String)> {
    check_literal(exe)?;
    check_literal(profile)?;
    let exe = exe
        .to_str()
        .context("the acpd path must be valid text")?
        .to_string();
    let profile = profile
        .to_str()
        .context("the setup profile path must be valid text")?
        .to_string();
    Ok((exe, profile))
}

struct Captured {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Captured {
    fn from_output(output: std::process::Output) -> Self {
        Self {
            code: output.status.code().unwrap_or(1),
            stdout: clip_owned(&String::from_utf8_lossy(&output.stdout), OUTPUT_CAP),
            stderr: clip_owned(&String::from_utf8_lossy(&output.stderr), OUTPUT_CAP),
        }
    }
}

trait Runner {
    fn run(&mut self, program: &str, args: &[&str]) -> Result<Captured>;
}

struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> Result<Captured> {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("run {program}"))?;
        Ok(Captured::from_output(output))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Linger {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    On,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Off,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Unknown,
    Skip,
}

enum Kind {
    Missing,
    Foreign,
    Ours,
}

enum LogLine {
    Path(String),
    StderrOnly,
    MissingSetup,
    Unreadable,
}

struct Report {
    kind: Kind,
    running: Option<bool>,
    enabled: Option<bool>,
    log: LogLine,
    linger: Linger,
}

fn format_report(report: &Report) -> String {
    let service = match report.kind {
        Kind::Missing => "not installed",
        Kind::Foreign => "present without the acpd service marker",
        Kind::Ours => "installed",
    };
    let mut out = format!(
        "Service: {service}\nRunning: {}\nEnabled: {}\n{}",
        flag(report.running),
        flag(report.enabled),
        log_text(&report.log),
    );
    match report.linger {
        Linger::On => out.push_str("\nLingering: on"),
        Linger::Off => {
            out.push_str("\nLingering: off\n");
            out.push_str(LINGER_HINT);
        }
        Linger::Unknown => {
            out.push_str("\nLingering: not reported\n");
            out.push_str(LINGER_HINT);
        }
        Linger::Skip => {}
    }
    out
}

fn flag(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "not reported",
    }
}

fn log_text(log: &LogLine) -> String {
    match log {
        LogLine::Path(path) => format!("Log: {path}"),
        LogLine::StderrOnly => "Log: stderr only".into(),
        LogLine::MissingSetup => "Log: no setup profile".into(),
        LogLine::Unreadable => "Log: configuration could not be read".into(),
    }
}

fn configured_log(profile: &Path) -> LogLine {
    if !profile.is_file() {
        return LogLine::MissingSetup;
    }
    let saved = match Launcher::load(profile) {
        Ok(saved) => saved,
        Err(_) => return LogLine::Unreadable,
    };
    match Config::load(&saved.config) {
        Ok(config) => match config.logging.file {
            Some(path) => LogLine::Path(path.display().to_string()),
            None => LogLine::StderrOnly,
        },
        Err(_) => LogLine::Unreadable,
    }
}

enum Definition {
    Missing,
    Foreign,
    Ours,
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn open_definition(path: &Path, marked: fn(&str) -> bool) -> Result<Definition> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Definition::Missing);
        }
        Err(error) => return Err(error).context("read the service definition"),
        Ok(meta) if meta.file_type().is_symlink() => {
            bail!("refusing to write or remove a service definition through a symlink")
        }
        Ok(_) => {}
    }
    let text = read_capped(path, OUTPUT_CAP)?;
    if marked(&text) {
        Ok(Definition::Ours)
    } else {
        Ok(Definition::Foreign)
    }
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn expect_file_name(path: &Path, name: &str) -> Result<()> {
    if path.file_name().and_then(|item| item.to_str()) != Some(name) {
        bail!("the service file must be named {name}");
    }
    Ok(())
}

fn must(runner: &mut dyn Runner, program: &str, args: &[&str]) -> Result<()> {
    let captured = runner.run(program, args)?;
    if captured.code != 0 {
        bail!(tool_error(program, &captured));
    }
    Ok(())
}

fn tool_error(program: &str, captured: &Captured) -> anyhow::Error {
    let detail = if captured.stderr.trim().is_empty() {
        captured.stdout.trim()
    } else {
        captured.stderr.trim()
    };
    let hint = if program == "systemctl" && detail.to_ascii_lowercase().contains("bus") {
        " (systemctl --user needs a running user manager: log in at the machine or in a session that has one, then run the command again)"
    } else {
        ""
    };
    anyhow!(
        "{program} exited {}: {}{hint}",
        captured.code,
        clip(detail, 400)
    )
}

fn clip(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn clip_owned(text: &str, max: usize) -> String {
    clip(text, max).to_string()
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn read_capped(path: &Path, max: usize) -> Result<String> {
    let mut file = fs::File::open(path).context("read the service definition")?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)
        .context("read the service definition")?;
    if bytes.len() > max {
        bail!("the service definition is too large");
    }
    String::from_utf8(bytes).context("the service definition is not text")
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn write_private(path: &Path, text: &str) -> Result<()> {
    write_bytes(path, text.as_bytes())
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).context("create the service directory")?;
    }
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            bail!("refusing to write or remove a service definition through a symlink")
        }
        Ok(_) | Err(_) => {}
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .context("write the service definition")?
        .write_all(bytes)
        .context("write the service definition")
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
