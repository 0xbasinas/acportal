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

pub const MARKER: &str = "acpd-service-marker: acportal";
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const MARKER_LINE: &str = "# acpd-service-marker: acportal";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PLIST_COMMENT: &str = "<!-- acpd-service-marker: acportal -->";
const UNIT: &str = "acpd.service";
#[cfg_attr(not(windows), allow(dead_code))]
const TASK: &str = "acpd";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const LABEL: &str = "dev.acportal.acpd";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PLIST_NAME: &str = "dev.acportal.acpd.plist";
const OUTPUT_CAP: usize = 64 * 1024;
/// Printed after install: a service starts with the service manager's environment, not a shell's.
const ENV_NOTE: &str = "The service does not inherit your shell environment. Put agent keys and PATH entries in the agent registry `env` (docs/agents.md) or in the agent's own configuration. If `acpd daemon start` is already running this host, stop it first with `acpd daemon stop`.";
/// Added when nothing else keeps the service's output.
#[cfg_attr(not(windows), allow(dead_code))]
const NO_LOG_NOTE: &str = "No log file is configured and a scheduled task keeps no output. Set `[logging] file` in the host config to keep logs.";
const LINGER_HINT: &str = "To keep the host after logout, run: loginctl enable-linger";

const SHOW_ARGS: &[&str] = &[
    "--user",
    "show",
    UNIT,
    "--property=LoadState",
    "--property=ActiveState",
    "--property=UnitFileState",
    "--no-pager",
];
#[cfg_attr(not(windows), allow(dead_code))]
const QUERY_XML: &[&str] = &["/Query", "/TN", TASK, "/XML", "ONE"];
#[cfg_attr(not(windows), allow(dead_code))]
const QUERY_ALL: &[&str] = &["/Query", "/FO", "CSV", "/NH"];
#[cfg_attr(not(windows), allow(dead_code))]
const QUERY_LIST: &[&str] = &["/Query", "/TN", TASK, "/FO", "LIST", "/V"];

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

#[cfg(target_os = "macos")]
fn current_uid() -> u32 {
    // libc is the unix dependency already used by the host. This reads the
    // calling user so the LaunchAgent is registered in that user's gui domain.
    unsafe { libc::getuid() }
}

#[cfg(windows)]
fn windows_user() -> Result<String> {
    let name = std::env::var("USERNAME").context("USERNAME is not set")?;
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let user = if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    };
    check_account(&user)?;
    Ok(user)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn check_task_user(value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains(['\\', '/', '<', '>', '&', '*', '?', '"', '\n', '\r'])
        || value.chars().any(|c| c.is_control())
    {
        bail!("the Windows account name cannot be used for a per-user task");
    }
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn check_account(user: &str) -> Result<()> {
    let mut parts = user.split('\\');
    let first = parts.next().unwrap_or("");
    match parts.next() {
        None => check_task_user(first),
        Some(name) if parts.next().is_none() => {
            check_task_user(first)?;
            check_task_user(name)
        }
        Some(_) => bail!("the Windows account name cannot be used for a per-user task"),
    }
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
fn home() -> Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .context("the home directory is not available")
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn config_home() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() {
            bail!("XDG_CONFIG_HOME is empty");
        }
        return Ok(path);
    }
    Ok(home()?.join(".config"))
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn systemd_unit_path() -> Result<PathBuf> {
    Ok(config_home()?.join("systemd").join("user").join(UNIT))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn launchd_plist_path() -> Result<PathBuf> {
    Ok(home()?
        .join("Library")
        .join("LaunchAgents")
        .join(PLIST_NAME))
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

/// Quote one ExecStart argument. The caller prefixes the executable with `:` so
/// systemd does not expand `$`. `%` is still a specifier, so each percent sign
/// is doubled. Backslash and double quote follow systemd.syntax(7).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn systemd_quote(arg: &str) -> Result<String> {
    if arg.chars().any(|c| c.is_control()) {
        bail!("a service argument contains a control character");
    }
    let mut out = String::from("\"");
    for c in arg.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    Ok(out)
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

/// Quote one task argument with CommandLineToArgvW rules. Every argument is quoted.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_arg(arg: &str) -> Result<String> {
    if arg.chars().any(|c| c.is_control()) {
        bail!("a service argument contains a control character");
    }
    let mut out = String::from("\"");
    let mut slashes = 0usize;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
            out.push('"');
            slashes = 0;
            continue;
        }
        if slashes > 0 {
            out.push_str(&"\\".repeat(slashes));
            slashes = 0;
        }
        out.push(c);
    }
    if slashes > 0 {
        out.push_str(&"\\".repeat(slashes * 2));
    }
    out.push('"');
    Ok(out)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_arguments(profile: &Path) -> Result<String> {
    let profile = profile
        .to_str()
        .context("the setup profile path must be valid text")?;
    // `--no-console` closes the console window the task would otherwise leave open at logon.
    Ok(format!(
        "{} {} {} {}",
        windows_arg("--profile")?,
        windows_arg(profile)?,
        windows_arg("--no-console")?,
        windows_arg("start")?
    ))
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

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn systemd_unit(exe: &Path, profile: &Path) -> Result<String> {
    let (exe, profile) = foreground_args(exe, profile)?;
    let line = format!(
        "ExecStart=:{} {} {} {}",
        systemd_quote(&exe)?,
        systemd_quote("--profile")?,
        systemd_quote(&profile)?,
        systemd_quote("start")?
    );
    Ok(format!(
        "{MARKER_LINE}\n[Unit]\nDescription=ACP Portal host\n\n[Service]\nType=exec\n{line}\nRestart=on-failure\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n"
    ))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn launchd_plist(exe: &Path, profile: &Path, log: &Path) -> Result<String> {
    let (exe, profile) = foreground_args(exe, profile)?;
    // launchd discards a job's output unless it is given files, so stdout and stderr go to one.
    check_literal(log)?;
    let exe = xml_escape(&exe);
    let profile = xml_escape(&profile);
    let log = xml_escape(log.to_str().context("the log path must be valid text")?);
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
{PLIST_COMMENT}
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>Program</key>
  <string>{exe}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>--profile</string>
    <string>{profile}</string>
    <string>start</string>
  </array>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
</dict>
</plist>
"#
    ))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn task_xml(exe: &Path, profile: &Path, user: &str) -> Result<String> {
    let (exe, profile) = foreground_args(exe, profile)?;
    check_account(user)?;
    let command = xml_escape(&exe);
    let arguments = xml_escape(&windows_arguments(Path::new(&profile))?);
    let user = xml_escape(user);
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>{MARKER}</Description>
  </RegistrationInfo>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure>
      <Interval>PT10S</Interval>
      <Count>999</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>{arguments}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    ))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn encode_utf16_le_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn systemd_marked(text: &str) -> bool {
    text.lines().any(|line| line.trim() == MARKER_LINE)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn plist_marked(text: &str) -> bool {
    text.lines().any(|line| line.trim() == PLIST_COMMENT)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn xml_element_is(text: &str, element: &str, value: &str) -> bool {
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    text.match_indices(&open).any(|(index, _)| {
        let rest = &text[index + open.len()..];
        rest.split_once(&close)
            .is_some_and(|(body, _)| body.trim() == value)
    })
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

pub fn systemctl_show() -> Result<SystemctlShow> {
    let captured = ProcessRunner.run("systemctl", SHOW_ARGS)?;
    if captured.code != 0 && !captured.stdout.contains("LoadState=") {
        bail!(tool_error("systemctl", &captured));
    }
    parse_systemctl_show(&captured.stdout)
}

pub struct SystemctlShow {
    pub load_state: String,
    pub active_state: String,
    pub unit_file_state: String,
}

fn parse_systemctl_show(text: &str) -> Result<SystemctlShow> {
    let mut load = None;
    let mut active = None;
    let mut enabled = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("LoadState=") {
            load = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("ActiveState=") {
            active = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("UnitFileState=") {
            enabled = Some(value.to_string());
        }
    }
    Ok(SystemctlShow {
        load_state: load.context("systemctl show omitted LoadState")?,
        active_state: active.context("systemctl show omitted ActiveState")?,
        unit_file_state: enabled.unwrap_or_default(),
    })
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

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_linger(text: &str) -> Linger {
    for line in text.lines() {
        if let Some(value) = line.trim().strip_prefix("Linger=") {
            return match value.trim() {
                "yes" => Linger::On,
                "no" => Linger::Off,
                _ => Linger::Unknown,
            };
        }
    }
    Linger::Unknown
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn launchd_running(text: &str) -> bool {
    text.lines().any(|line| line.trim() == "state = running")
}

#[cfg_attr(not(windows), allow(dead_code))]
fn task_running(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        let Some(value) = line.strip_prefix("Status:") else {
            return false;
        };
        value.trim().eq_ignore_ascii_case("running")
    })
}

#[cfg_attr(not(windows), allow(dead_code))]
fn xml_enabled(text: &str) -> bool {
    text.contains("<Enabled>true</Enabled>") && !text.contains("<Enabled>false</Enabled>")
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

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn install_systemd(
    unit_path: &Path,
    exe: &Path,
    profile: &Path,
    runner: &mut dyn Runner,
) -> Result<String> {
    expect_file_name(unit_path, UNIT)?;
    match open_definition(unit_path, systemd_marked)? {
        Definition::Foreign => {
            bail!("{UNIT} has no acpd service marker, so it was left in place")
        }
        Definition::Missing | Definition::Ours => {}
    }
    write_private(unit_path, &systemd_unit(exe, profile)?)?;
    must(runner, "systemctl", &["--user", "daemon-reload"])?;
    must(runner, "systemctl", &["--user", "enable", "--now", UNIT])?;
    let linger = linger_of(runner);
    Ok(installed_message(&unit_path.display().to_string(), linger))
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_status(unit_path: &Path, profile: &Path, runner: &mut dyn Runner) -> Result<String> {
    let (kind, running, enabled) = match open_definition(unit_path, systemd_marked)? {
        Definition::Missing => (Kind::Missing, Some(false), Some(false)),
        Definition::Foreign => (Kind::Foreign, None, None),
        Definition::Ours => match runner.run("systemctl", SHOW_ARGS) {
            Ok(captured) if captured.code == 0 || captured.stdout.contains("LoadState=") => {
                let show = parse_systemctl_show(&captured.stdout)?;
                let enabled = if show.unit_file_state.is_empty() {
                    None
                } else {
                    Some(show.unit_file_state == "enabled")
                };
                (Kind::Ours, Some(show.active_state == "active"), enabled)
            }
            Ok(_) | Err(_) => (Kind::Ours, None, None),
        },
    };
    Ok(format_report(&Report {
        kind,
        running,
        enabled,
        log: configured_log(profile),
        linger: linger_of(runner),
    }))
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn restart_systemd(unit_path: &Path, runner: &mut dyn Runner) -> Result<String> {
    match open_definition(unit_path, systemd_marked)? {
        Definition::Missing => bail!("Service: not installed"),
        Definition::Foreign => {
            bail!("{UNIT} has no acpd service marker, so it was left in place")
        }
        Definition::Ours => {}
    }
    must(runner, "systemctl", &["--user", "restart", UNIT])?;
    Ok(format!("Restarted {UNIT}"))
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn uninstall_systemd(unit_path: &Path, runner: &mut dyn Runner) -> Result<String> {
    match open_definition(unit_path, systemd_marked)? {
        Definition::Missing => {
            return Ok(
                "Service: not installed\nThe host config, state, tokens and paired devices were left in place"
                    .into(),
            );
        }
        Definition::Foreign => {
            bail!("{UNIT} has no acpd service marker, so it was left in place")
        }
        Definition::Ours => {}
    }
    must(runner, "systemctl", &["--user", "disable", "--now", UNIT])?;
    fs::remove_file(unit_path).context("remove the systemd unit")?;
    must(runner, "systemctl", &["--user", "daemon-reload"])?;
    Ok(format!(
        "Removed {}\nThe host config, state, tokens and paired devices were left in place",
        unit_path.display()
    ))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn domain(uid: u32) -> String {
    format!("gui/{uid}")
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn launchd_target(uid: u32) -> String {
    format!("gui/{uid}/{LABEL}")
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn install_launchd(
    plist_path: &Path,
    exe: &Path,
    profile: &Path,
    uid: u32,
    runner: &mut dyn Runner,
) -> Result<String> {
    expect_file_name(plist_path, PLIST_NAME)?;
    match open_definition(plist_path, plist_marked)? {
        Definition::Foreign => {
            bail!("{PLIST_NAME} has no acpd service marker, so it was left in place")
        }
        Definition::Missing | Definition::Ours => {}
    }
    // ~/Library/LaunchAgents/<plist> -> ~/Library/Logs/acpd-service.log
    let log = plist_path
        .parent()
        .and_then(Path::parent)
        .context("the LaunchAgents folder has no parent")?
        .join("Logs")
        .join("acpd-service.log");
    write_private(plist_path, &launchd_plist(exe, profile, &log)?)?;
    let target = launchd_target(uid);
    let _ = runner.run("launchctl", &["bootout", &target]);
    let plist = plist_path
        .to_str()
        .context("the LaunchAgent path must be valid text")?;
    let gui = domain(uid);
    must(runner, "launchctl", &["bootstrap", &gui, plist])?;
    Ok(format!(
        "Installed {}\nStarted {LABEL}\nOutput goes to {}\n{ENV_NOTE}",
        plist_path.display(),
        log.display()
    ))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn launchd_status(
    plist_path: &Path,
    profile: &Path,
    uid: u32,
    runner: &mut dyn Runner,
) -> Result<String> {
    let target = launchd_target(uid);
    let (kind, running, enabled) = match open_definition(plist_path, plist_marked)? {
        Definition::Missing => (Kind::Missing, Some(false), Some(false)),
        Definition::Foreign => (Kind::Foreign, None, None),
        Definition::Ours => match runner.run("launchctl", &["print", &target]) {
            Ok(captured) if captured.code == 0 => (
                Kind::Ours,
                Some(launchd_running(&captured.stdout)),
                Some(true),
            ),
            Ok(_) | Err(_) => (Kind::Ours, None, None),
        },
    };
    Ok(format_report(&Report {
        kind,
        running,
        enabled,
        log: configured_log(profile),
        linger: Linger::Skip,
    }))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn restart_launchd(plist_path: &Path, uid: u32, runner: &mut dyn Runner) -> Result<String> {
    match open_definition(plist_path, plist_marked)? {
        Definition::Missing => bail!("Service: not installed"),
        Definition::Foreign => {
            bail!("{PLIST_NAME} has no acpd service marker, so it was left in place")
        }
        Definition::Ours => {}
    }
    let target = launchd_target(uid);
    must(runner, "launchctl", &["kickstart", "-k", &target])?;
    Ok(format!("Restarted {LABEL}"))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn uninstall_launchd(plist_path: &Path, uid: u32, runner: &mut dyn Runner) -> Result<String> {
    match open_definition(plist_path, plist_marked)? {
        Definition::Missing => {
            return Ok(
                "Service: not installed\nThe host config, state, tokens and paired devices were left in place"
                    .into(),
            );
        }
        Definition::Foreign => {
            bail!("{PLIST_NAME} has no acpd service marker, so it was left in place")
        }
        Definition::Ours => {}
    }
    let target = launchd_target(uid);
    let stopped = runner.run("launchctl", &["bootout", &target])?;
    fs::remove_file(plist_path).context("remove the LaunchAgent")?;
    let mut message = format!(
        "Removed {}\nThe host config, state, tokens and paired devices were left in place",
        plist_path.display()
    );
    if stopped.code != 0 {
        message.push('\n');
        message.push_str(&tool_error("launchctl", &stopped).to_string());
    }
    Ok(message)
}

/// Look the task up. `schtasks` exits 1 for a missing task and for other errors, and its message
/// is localized, so a message that is not the English one is settled by listing every task
/// (`/FO CSV`, whose first column is the task name) and checking whether `\acpd` is there.
#[cfg_attr(not(windows), allow(dead_code))]
fn probe_task(runner: &mut dyn Runner) -> Result<(Definition, Captured)> {
    let query = runner.run("schtasks", QUERY_XML)?;
    if query.code != 0 && !is_missing_task(&query) {
        let all = runner.run("schtasks", QUERY_ALL)?;
        if all.code == 0 && !lists_task(&all.stdout) {
            return Ok((Definition::Missing, query));
        }
    }
    Ok((classify_task(&query)?, query))
}

/// Whether CSV output from `schtasks /Query /FO CSV /NH` has a row for this task.
#[cfg_attr(not(windows), allow(dead_code))]
fn lists_task(csv: &str) -> bool {
    let wanted = format!("\"\\{TASK}\"");
    csv.lines().any(|line| {
        let first = line.trim_start_matches('\u{feff}').split(',').next();
        first.is_some_and(|name| name.eq_ignore_ascii_case(&wanted))
    })
}

#[cfg_attr(not(windows), allow(dead_code))]
fn classify_task(captured: &Captured) -> Result<Definition> {
    if captured.code != 0 && is_missing_task(captured) {
        return Ok(Definition::Missing);
    }
    if captured.code != 0 {
        bail!(tool_error("schtasks", captured));
    }
    if xml_element_is(&captured.stdout, "Description", MARKER) {
        Ok(Definition::Ours)
    } else {
        Ok(Definition::Foreign)
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn is_missing_task(captured: &Captured) -> bool {
    let text = format!("{}{}", captured.stdout, captured.stderr).to_ascii_lowercase();
    text.contains("cannot find the file specified")
}

#[cfg_attr(not(windows), allow(dead_code))]
fn install_task(
    xml_path: &Path,
    exe: &Path,
    profile: &Path,
    user: &str,
    runner: &mut dyn Runner,
) -> Result<String> {
    match probe_task(runner)?.0 {
        Definition::Foreign => {
            bail!("scheduled task {TASK} has no acpd service marker, so it was left in place")
        }
        Definition::Missing | Definition::Ours => {}
    }
    let xml = task_xml(exe, profile, user)?;
    write_bytes(xml_path, &encode_utf16_le_bom(&xml))?;
    let _guard = RemoveOnDrop(xml_path.to_path_buf());
    let args = schtasks_create_args(xml_path)?;
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    must(runner, "schtasks", &refs)?;
    must(runner, "schtasks", &["/Run", "/TN", TASK])?;
    let mut message = format!("Installed scheduled task {TASK}\nStarted {TASK}\n{ENV_NOTE}");
    if matches!(configured_log(profile), LogLine::StderrOnly) {
        message.push('\n');
        message.push_str(NO_LOG_NOTE);
    }
    Ok(message)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn task_status(profile: &Path, runner: &mut dyn Runner) -> Result<String> {
    let (definition, query) = probe_task(runner)?;
    let (kind, running, enabled) = match definition {
        Definition::Missing => (Kind::Missing, Some(false), Some(false)),
        Definition::Foreign => (Kind::Foreign, None, None),
        Definition::Ours => {
            let enabled = Some(xml_enabled(&query.stdout));
            let running = match runner.run("schtasks", QUERY_LIST) {
                Ok(list) if list.code == 0 => Some(task_running(&list.stdout)),
                Ok(_) | Err(_) => None,
            };
            (Kind::Ours, running, enabled)
        }
    };
    Ok(format_report(&Report {
        kind,
        running,
        enabled,
        log: configured_log(profile),
        linger: Linger::Skip,
    }))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn restart_task(runner: &mut dyn Runner) -> Result<String> {
    match probe_task(runner)?.0 {
        Definition::Missing => bail!("Service: not installed"),
        Definition::Foreign => {
            bail!("scheduled task {TASK} has no acpd service marker, so it was left in place")
        }
        Definition::Ours => {}
    }
    // End exits nonzero when the task is already stopped. Run still starts it.
    let _ = runner.run("schtasks", &["/End", "/TN", TASK])?;
    must(runner, "schtasks", &["/Run", "/TN", TASK])?;
    Ok(format!("Restarted scheduled task {TASK}"))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn uninstall_task(runner: &mut dyn Runner) -> Result<String> {
    match probe_task(runner)?.0 {
        Definition::Missing => {
            return Ok(
                "Service: not installed\nThe host config, state, tokens and paired devices were left in place"
                    .into(),
            );
        }
        Definition::Foreign => {
            bail!("scheduled task {TASK} has no acpd service marker, so it was left in place")
        }
        Definition::Ours => {}
    }
    must(runner, "schtasks", &["/Delete", "/TN", TASK, "/F"])?;
    Ok(
        "Removed scheduled task acpd\nThe host config, state, tokens and paired devices were left in place"
            .into(),
    )
}

#[cfg_attr(not(windows), allow(dead_code))]
fn schtasks_create_args(xml_path: &Path) -> Result<Vec<String>> {
    let text = xml_path
        .to_str()
        .context("the task xml path must be valid text")?;
    if text.contains('*') {
        bail!("the task xml path is not usable");
    }
    debug_assert!(!TASK.contains('*') && TASK != "*");
    Ok(vec![
        "/Create".into(),
        "/TN".into(),
        TASK.into(),
        "/XML".into(),
        text.into(),
        "/F".into(),
    ])
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linger_of(runner: &mut dyn Runner) -> Linger {
    match runner.run("loginctl", &["show-user", "--property=Linger"]) {
        Ok(captured) if captured.code == 0 => parse_linger(&captured.stdout),
        Ok(_) | Err(_) => Linger::Unknown,
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn installed_message(path: &str, linger: Linger) -> String {
    let mut message = format!("Installed {path}\nStarted {UNIT}\n{ENV_NOTE}");
    if matches!(linger, Linger::Off | Linger::Unknown) {
        message.push('\n');
        message.push_str(LINGER_HINT);
    }
    message
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Script {
        calls: Vec<(String, Vec<String>)>,
        replies: VecDeque<Captured>,
        xml: Option<Vec<u8>>,
    }

    impl Script {
        fn ok(replies: Vec<Captured>) -> Self {
            Self {
                calls: Vec::new(),
                replies: replies.into(),
                xml: None,
            }
        }
    }

    impl Runner for Script {
        fn run(&mut self, program: &str, args: &[&str]) -> Result<Captured> {
            if let Some(index) = args.iter().position(|arg| *arg == "/XML")
                && args.first() == Some(&"/Create")
            {
                self.xml = args.get(index + 1).and_then(|path| fs::read(path).ok());
            }
            self.calls.push((
                program.to_string(),
                args.iter().map(|arg| (*arg).to_string()).collect(),
            ));
            Ok(self.replies.pop_front().unwrap_or(Captured {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }))
        }
    }

    fn captured(code: i32, stdout: &str, stderr: &str) -> Captured {
        Captured {
            code,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    fn platform_abs(tail: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!(r"C:\{tail}"))
        } else {
            PathBuf::from(format!("/{tail}"))
        }
    }

    fn fixture(dir: &Path) -> (PathBuf, PathBuf) {
        let exe = dir.join(if cfg!(windows) { "acpd.exe" } else { "acpd" });
        fs::write(&exe, b"x").unwrap();
        let config = dir.join("config.toml");
        fs::write(
            &config,
            "registry = \"agents.json\"\n[logging]\nfile = \"logs/acpd.log\"\n[server]\ntls_certificate = \"certs/fullchain.pem\"\ntls_private_key = \"certs/privkey.pem\"\n",
        )
        .unwrap();
        let profile = dir.join("launcher.json");
        let body = format!(
            "{{\"config\":{},\"address\":\"https://phone.example\",\"name\":\"Laptop\"}}",
            serde_json::to_string(&config).unwrap()
        );
        fs::write(&profile, body).unwrap();
        (exe, profile)
    }

    #[test]
    fn systemd_quoting_keeps_spaces_unicode_and_specifiers_literal() {
        assert_eq!(
            systemd_quote("a b$c%d\"e\\f").unwrap(),
            "\"a b$c%%d\\\"e\\\\f\""
        );
        assert!(systemd_quote("a\nb").is_err());
        let exe = platform_abs("opt/acp portal/café/acpd");
        let profile = platform_abs("home/me/A$B%C\"D\\E/launcher.json");
        let unit = systemd_unit(&exe, &profile).unwrap();
        let expected = format!(
            "ExecStart=:{} {} {} {}",
            systemd_quote(exe.to_str().unwrap()).unwrap(),
            systemd_quote("--profile").unwrap(),
            systemd_quote(profile.to_str().unwrap()).unwrap(),
            systemd_quote("start").unwrap()
        );
        assert!(unit.contains(&expected), "{unit}");
        assert!(unit.starts_with(MARKER_LINE));
        assert!(unit.contains("Type=exec"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("café"));
        assert!(!unit.contains("sh -c"));
        assert!(!unit.contains("Environment="));
        assert!(!unit.contains("--background"));
        assert!(!unit.contains("https://"));
        assert!(check_literal(&platform_abs("opt/../etc/acpd")).is_err());
        assert!(check_literal(Path::new("relative/acpd")).is_err());
    }

    #[test]
    fn plist_escapes_every_value() {
        assert_eq!(
            xml_escape("A & B <C> \"D\" 'E'"),
            "A &amp; B &lt;C&gt; &quot;D&quot; &apos;E&apos;"
        );
        let exe = platform_abs("Program Files/café/acpd");
        let profile = platform_abs("Users/me/A & B/launcher.json");
        let log = platform_abs("Users/me/Library/Logs/acpd-service.log");
        let plist = launchd_plist(&exe, &profile, &log).unwrap();
        assert!(plist.contains("<key>StandardOutPath</key>"));
        assert!(plist.contains("<key>StandardErrorPath</key>"));
        assert!(plist.contains("acpd-service.log"));
        assert!(plist.lines().any(|line| line.trim() == PLIST_COMMENT));
        assert!(plist.contains("<string>dev.acportal.acpd</string>"));
        assert!(plist.contains("café"));
        assert!(plist.contains("A &amp; B"));
        assert!(!plist.contains("A & B"));
        assert!(plist.contains("<string>--profile</string>"));
        assert!(plist.contains("<string>start</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<key>SuccessfulExit</key>"));
        assert!(!plist.contains("sh -c"));
        assert!(!plist.contains("--background"));
    }

    #[test]
    fn task_xml_is_least_privilege_and_quotes_arguments() {
        assert_eq!(
            windows_arg("C:\\A & B\\c\"d").unwrap(),
            "\"C:\\A & B\\c\\\"d\""
        );
        assert_eq!(windows_arg("C:\\dir\\").unwrap(), "\"C:\\dir\\\\\"");
        let exe = platform_abs("Program Files/acpd.exe");
        let profile = platform_abs("Users/me/A & B/launcher.json");
        let xml = task_xml(&exe, &profile, "OPERATOR\\me").unwrap();
        assert!(xml_element_is(&xml, "Description", MARKER));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert_eq!(xml.matches("<UserId>OPERATOR\\me</UserId>").count(), 2);
        assert!(xml.contains("A &amp; B"));
        assert!(!xml.contains("A & B"));
        assert!(xml.contains("start"));
        assert!(!xml.contains("Highest"));
        assert!(!xml.contains("HIGHEST"));
        assert!(!xml.contains("SYSTEM"));
        assert!(!xml.contains("/RU"));
        let args = schtasks_create_args(Path::new("C:/temp/acpd-service.xml")).unwrap();
        assert_eq!(
            args,
            [
                "/Create",
                "/TN",
                "acpd",
                "/XML",
                "C:/temp/acpd-service.xml",
                "/F"
            ]
        );
        assert!(
            args.iter()
                .all(|arg| arg != "*" && arg != "/RU" && arg != "/RP")
        );
        assert_eq!(TASK, "acpd");
        assert!(!TASK.contains('*'));
    }

    #[test]
    fn parsers_read_systemctl_linger_and_task_status() {
        let show =
            parse_systemctl_show("LoadState=loaded\nActiveState=active\nUnitFileState=enabled\n")
                .unwrap();
        assert_eq!(show.load_state, "loaded");
        assert_eq!(show.active_state, "active");
        assert_eq!(show.unit_file_state, "enabled");
        assert!(parse_systemctl_show("ActiveState=active\n").is_err());
        assert_eq!(parse_linger("Linger=yes\n"), Linger::On);
        assert_eq!(parse_linger("Linger=no\n"), Linger::Off);
        assert_eq!(parse_linger("Linger=maybe\n"), Linger::Unknown);
        assert!(launchd_running("\tstate = running\n"));
        assert!(!launchd_running("state = not running\n"));
        assert!(task_running("Status:        Running\n"));
        assert!(!task_running(
            "Status: Ready\nScheduled Task State: Enabled\n"
        ));
        assert!(xml_enabled("<Enabled>true</Enabled>"));
        assert!(!xml_enabled(
            "<Enabled>true</Enabled><Enabled>false</Enabled>"
        ));
        let missing = captured(
            1,
            "",
            "ERROR: The system cannot find the file specified.\r\n",
        );
        assert!(matches!(
            classify_task(&missing).unwrap(),
            Definition::Missing
        ));
        let foreign = captured(0, "<Description>someone else</Description>", "");
        assert!(matches!(
            classify_task(&foreign).unwrap(),
            Definition::Foreign
        ));
        let ours = captured(0, &format!("<Description>{MARKER}</Description>"), "");
        assert!(matches!(classify_task(&ours).unwrap(), Definition::Ours));
    }

    #[test]
    fn status_text_reports_the_log_path_and_leaves_secrets_out() {
        let dir = tempfile::tempdir().unwrap();
        let (_exe, profile) = fixture(dir.path());
        let text = format_report(&Report {
            kind: Kind::Ours,
            running: Some(true),
            enabled: Some(true),
            log: configured_log(&profile),
            linger: Linger::Off,
        });
        assert!(text.contains("Service: installed"));
        assert!(text.contains("Running: yes"));
        assert!(text.contains("acpd.log"));
        assert!(text.contains(LINGER_HINT));
        assert!(!text.contains("privkey"));
        assert!(!text.contains("fullchain"));
        assert!(!text.contains("phone.example"));
        assert!(!text.contains("Laptop"));
        assert!(!text.contains("sudo"));
        fs::write(dir.path().join("config.toml"), "this is not toml").unwrap();
        assert!(matches!(configured_log(&profile), LogLine::Unreadable));
    }

    #[test]
    fn uninstall_refuses_a_unit_without_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let unit = dir.path().join(UNIT);
        fs::write(&unit, "[Service]\nExecStart=/usr/bin/other\n").unwrap();
        let config = dir.path().join("config.toml");
        fs::write(&config, "keep").unwrap();
        let mut runner = Script::ok(vec![]);
        let error = uninstall_systemd(&unit, &mut runner).unwrap_err();
        assert!(error.to_string().contains("left in place"));
        assert_eq!(
            fs::read_to_string(&unit).unwrap(),
            "[Service]\nExecStart=/usr/bin/other\n"
        );
        assert_eq!(fs::read_to_string(&config).unwrap(), "keep");
        assert!(runner.calls.is_empty());

        let plist = dir.path().join(PLIST_NAME);
        fs::write(&plist, "<plist></plist>\n").unwrap();
        let error = uninstall_launchd(&plist, 501, &mut runner).unwrap_err();
        assert!(error.to_string().contains("left in place"));
        assert!(plist.exists());
        assert!(runner.calls.is_empty());

        let mut runner = Script::ok(vec![captured(
            0,
            "<Description>hand written</Description>",
            "",
        )]);
        let error = uninstall_task(&mut runner).unwrap_err();
        assert!(error.to_string().contains("left in place"));
        assert_eq!(runner.calls.len(), 1);
        assert_eq!(runner.calls[0].1, QUERY_XML);
    }

    #[test]
    fn install_writes_our_unit_and_status_parses_a_scripted_systemctl() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, profile) = fixture(dir.path());
        let unit = dir.path().join("systemd/user").join(UNIT);
        let mut runner = Script::ok(vec![
            captured(0, "", ""),
            captured(0, "", ""),
            captured(0, "Linger=no\n", ""),
        ]);
        let message = install_systemd(&unit, &exe, &profile, &mut runner).unwrap();
        assert!(systemd_marked(&fs::read_to_string(&unit).unwrap()));
        assert!(message.contains(LINGER_HINT));
        assert!(!message.contains("sudo"));
        assert_eq!(
            runner.calls[0].1,
            vec!["--user".to_string(), "daemon-reload".to_string()]
        );
        assert_eq!(
            runner.calls[1].1,
            vec![
                "--user".to_string(),
                "enable".to_string(),
                "--now".to_string(),
                UNIT.to_string()
            ]
        );
        assert_eq!(runner.calls[2].0, "loginctl");
        assert!(
            runner
                .calls
                .iter()
                .all(|call| call.0 != "sudo" && call.0 != "sh")
        );

        let mut runner = Script::ok(vec![
            captured(
                0,
                "LoadState=loaded\nActiveState=active\nUnitFileState=enabled\n",
                "",
            ),
            captured(0, "Linger=yes\n", ""),
        ]);
        let text = linux_status(&unit, &profile, &mut runner).unwrap();
        assert!(text.contains("Service: installed"));
        assert!(text.contains("Running: yes"));
        assert!(text.contains("Enabled: yes"));
        assert!(text.contains("Lingering: on"));
        assert!(!text.contains(LINGER_HINT));
        assert!(text.contains("acpd.log"));
        assert!(!text.contains("privkey"));

        fs::write(&unit, "[Service]\nExecStart=/bin/other\n").unwrap();
        let mut runner = Script::ok(vec![]);
        let error = install_systemd(&unit, &exe, &profile, &mut runner).unwrap_err();
        assert!(error.to_string().contains("left in place"));
        assert!(runner.calls.is_empty());
        assert_eq!(
            fs::read_to_string(&unit).unwrap(),
            "[Service]\nExecStart=/bin/other\n"
        );
    }

    #[test]
    fn restart_and_uninstall_touch_only_our_systemd_unit() {
        let dir = tempfile::tempdir().unwrap();
        let unit = dir.path().join(UNIT);
        fs::write(
            &unit,
            systemd_unit(
                &platform_abs("usr/bin/acpd"),
                &platform_abs("etc/acpd/launcher.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let config = dir.path().join("config.toml");
        fs::write(&config, "keep").unwrap();
        let mut runner = Script::ok(vec![captured(0, "", "")]);
        let message = restart_systemd(&unit, &mut runner).unwrap();
        assert!(message.contains("Restarted"));
        assert_eq!(
            runner.calls[0].1,
            vec![
                "--user".to_string(),
                "restart".to_string(),
                UNIT.to_string()
            ]
        );
        let mut runner = Script::ok(vec![captured(0, "", ""), captured(0, "", "")]);
        let message = uninstall_systemd(&unit, &mut runner).unwrap();
        assert!(!unit.exists());
        assert_eq!(fs::read_to_string(&config).unwrap(), "keep");
        assert!(message.contains("left in place"));
        assert_eq!(
            runner.calls[0].1,
            vec![
                "--user".to_string(),
                "disable".to_string(),
                "--now".to_string(),
                UNIT.to_string()
            ]
        );
        assert_eq!(
            runner.calls[1].1,
            vec!["--user".to_string(), "daemon-reload".to_string()]
        );
    }

    #[test]
    fn windows_uninstall_and_restart_use_the_fixed_task_name() {
        let ours = format!("<Description>{MARKER}</Description><Enabled>true</Enabled>");
        let mut runner = Script::ok(vec![
            captured(0, &ours, ""),
            captured(0, "Status:        Running\n", ""),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let (_exe, profile) = fixture(dir.path());
        let text = task_status(&profile, &mut runner).unwrap();
        assert!(text.contains("Service: installed"));
        assert!(text.contains("Running: yes"));
        assert!(text.contains("Enabled: yes"));
        assert!(!text.contains("Lingering"));
        assert!(!text.contains("privkey"));
        assert_eq!(runner.calls[1].1, QUERY_LIST);

        let mut runner = Script::ok(vec![
            captured(0, &ours, ""),
            captured(1, "", "ERROR: The task is not currently running."),
            captured(0, "", ""),
        ]);
        restart_task(&mut runner).unwrap();
        assert_eq!(runner.calls[1].1, vec!["/End", "/TN", "acpd"]);
        assert_eq!(runner.calls[2].1, vec!["/Run", "/TN", "acpd"]);
        assert!(
            runner
                .calls
                .iter()
                .all(|call| !call.1.iter().any(|arg| arg == "*"))
        );

        let mut runner = Script::ok(vec![captured(0, &ours, ""), captured(0, "", "")]);
        let message = uninstall_task(&mut runner).unwrap();
        assert!(message.contains("left in place"));
        assert_eq!(runner.calls[1].1, vec!["/Delete", "/TN", "acpd", "/F"]);
    }

    #[test]
    fn a_missing_task_is_found_without_the_english_message() {
        let german = "FEHLER: Das System kann die angegebene Datei nicht finden.";
        let others = "\"\\Other\",\"N/A\",\"Ready\"\r\n";
        // Not the English text, and the task list has no \acpd: the task is missing.
        let mut runner = Script::ok(vec![captured(1, "", german), captured(0, others, "")]);
        assert!(matches!(
            probe_task(&mut runner).unwrap().0,
            Definition::Missing
        ));
        assert_eq!(runner.calls[1].1, QUERY_ALL);
        // The same failure while \acpd is listed is a real error, not a missing task.
        let listed = "\"\\Other\",\"N/A\",\"Ready\"\r\n\"\\acpd\",\"N/A\",\"Ready\"\r\n";
        let mut runner = Script::ok(vec![captured(1, "", german), captured(0, listed, "")]);
        assert!(probe_task(&mut runner).is_err());
        // When the list cannot be read either, the first error is reported and nothing is assumed.
        let mut runner = Script::ok(vec![captured(1, "", german), captured(1, "", german)]);
        assert!(probe_task(&mut runner).is_err());
        // The English message needs no second query.
        let mut runner = Script::ok(vec![captured(
            1,
            "",
            "ERROR: The system cannot find the file specified.",
        )]);
        assert!(matches!(
            probe_task(&mut runner).unwrap().0,
            Definition::Missing
        ));
        assert_eq!(runner.calls.len(), 1);
        assert!(lists_task("\u{feff}\"\\ACPD\",\"x\"\r\n"));
        assert!(!lists_task(
            "\"\\acpd2\",\"x\"\r\n\"\\Folder\\acpd\",\"x\"\r\n"
        ));
    }

    #[test]
    fn the_logon_task_closes_its_console_and_install_says_what_the_service_lacks() {
        let profile = platform_abs("Users/me/launcher.json");
        let args = windows_arguments(&profile).unwrap();
        assert!(args.contains("\"--no-console\""), "{args}");
        assert!(args.ends_with("\"start\""), "{args}");
        let detail = captured(1, "", "Failed to connect to bus: No medium found");
        let error = tool_error("systemctl", &detail).to_string();
        assert!(error.contains("user manager"), "{error}");
        let other = tool_error("schtasks", &detail).to_string();
        assert!(!other.contains("user manager"), "{other}");
        assert!(installed_message("p", Linger::On).contains("shell environment"));
        assert!(ENV_NOTE.contains("daemon stop"));
    }

    #[test]
    fn windows_install_writes_utf16_xml_and_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, profile) = fixture(dir.path());
        let xml_path = dir.path().join("task.xml");
        let mut runner = Script::ok(vec![
            captured(1, "", "ERROR: The system cannot find the file specified."),
            captured(0, "", ""),
            captured(0, "", ""),
        ]);
        let message = install_task(&xml_path, &exe, &profile, "OPERATOR\\me", &mut runner).unwrap();
        assert!(message.contains("Installed scheduled task acpd"));
        assert!(!xml_path.exists());
        let bytes = runner.xml.expect("create saw the xml");
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        let text = String::from_utf16(&units).unwrap();
        assert!(text.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml_element_is(&text, "Description", MARKER));
        assert!(!text.contains("HIGHEST"));
        assert_eq!(
            runner.calls[1].1[..4],
            [
                "/Create".to_string(),
                "/TN".to_string(),
                "acpd".to_string(),
                "/XML".to_string()
            ]
        );
        assert_eq!(runner.calls[1].1.last().map(String::as_str), Some("/F"));
        assert_eq!(runner.calls[2].1, vec!["/Run", "/TN", "acpd"]);
    }

    #[test]
    fn launchd_install_status_and_restart_use_the_gui_domain() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, profile) = fixture(dir.path());
        let plist = dir.path().join(PLIST_NAME);
        let mut runner = Script::ok(vec![
            captured(3, "", "No such process"),
            captured(0, "", ""),
        ]);
        let message = install_launchd(&plist, &exe, &profile, 501, &mut runner).unwrap();
        assert!(plist_marked(&fs::read_to_string(&plist).unwrap()));
        assert!(message.contains("Started dev.acportal.acpd"));
        assert_eq!(
            runner.calls[0].1,
            vec![
                "bootout".to_string(),
                "gui/501/dev.acportal.acpd".to_string()
            ]
        );
        assert_eq!(
            runner.calls[1].1[0..2],
            ["bootstrap".to_string(), "gui/501".to_string()]
        );
        assert!(runner.calls[1].1[2].ends_with(PLIST_NAME));

        let mut runner = Script::ok(vec![captured(0, "state = running\npid = 42\n", "")]);
        let text = launchd_status(&plist, &profile, 501, &mut runner).unwrap();
        assert!(text.contains("Service: installed"));
        assert!(text.contains("Running: yes"));
        assert!(text.contains("Enabled: yes"));
        assert!(!text.contains("Lingering"));
        assert!(!text.contains("privkey"));
        assert_eq!(
            runner.calls[0].1,
            vec!["print".to_string(), "gui/501/dev.acportal.acpd".to_string()]
        );

        let mut runner = Script::ok(vec![captured(0, "", "")]);
        restart_launchd(&plist, 501, &mut runner).unwrap();
        assert_eq!(
            runner.calls[0].1,
            vec![
                "kickstart".to_string(),
                "-k".to_string(),
                "gui/501/dev.acportal.acpd".to_string()
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_unit_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, profile) = fixture(dir.path());
        let real = dir.path().join("real.service");
        fs::write(&real, "other").unwrap();
        let link = dir.path().join(UNIT);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let mut runner = Script::ok(vec![]);
        let error = install_systemd(&link, &exe, &profile, &mut runner).unwrap_err();
        assert!(error.to_string().contains("symlink"));
        assert_eq!(fs::read_to_string(&real).unwrap(), "other");
        assert!(runner.calls.is_empty());
    }
}
