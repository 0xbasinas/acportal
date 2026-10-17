//! systemd user unit (Linux).
//!
//! Compiled on every platform so its text builders and parsers are tested everywhere.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use super::*;

pub(super) const MARKER_LINE: &str = "# acpd-service-marker: acportal";

pub(super) const UNIT: &str = "acpd.service";

pub(super) const SHOW_ARGS: &[&str] = &[
    "--user",
    "show",
    UNIT,
    "--property=LoadState",
    "--property=ActiveState",
    "--property=UnitFileState",
    "--no-pager",
];

pub(super) fn config_home() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() {
            bail!("XDG_CONFIG_HOME is empty");
        }
        return Ok(path);
    }
    Ok(home()?.join(".config"))
}

pub(super) fn systemd_unit_path() -> Result<PathBuf> {
    Ok(config_home()?.join("systemd").join("user").join(UNIT))
}

/// Quote one ExecStart argument. The caller prefixes the executable with `:` so
/// systemd does not expand `$`. `%` is still a specifier, so each percent sign
/// is doubled. Backslash and double quote follow systemd.syntax(7).
pub(super) fn systemd_quote(arg: &str) -> Result<String> {
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

pub(super) fn systemd_unit(exe: &Path, profile: &Path) -> Result<String> {
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

pub(super) fn systemd_marked(text: &str) -> bool {
    text.lines().any(|line| line.trim() == MARKER_LINE)
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

pub(super) fn parse_systemctl_show(text: &str) -> Result<SystemctlShow> {
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

pub(super) fn parse_linger(text: &str) -> Linger {
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

pub(super) fn install_systemd(
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

pub(super) fn linux_status(
    unit_path: &Path,
    profile: &Path,
    runner: &mut dyn Runner,
) -> Result<String> {
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

pub(super) fn restart_systemd(unit_path: &Path, runner: &mut dyn Runner) -> Result<String> {
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

pub(super) fn uninstall_systemd(unit_path: &Path, runner: &mut dyn Runner) -> Result<String> {
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

pub(super) fn linger_of(runner: &mut dyn Runner) -> Linger {
    match runner.run("loginctl", &["show-user", "--property=Linger"]) {
        Ok(captured) if captured.code == 0 => parse_linger(&captured.stdout),
        Ok(_) | Err(_) => Linger::Unknown,
    }
}

pub(super) fn installed_message(path: &str, linger: Linger) -> String {
    let mut message = format!("Installed {path}\nStarted {UNIT}\n{ENV_NOTE}");
    if matches!(linger, Linger::Off | Linger::Unknown) {
        message.push('\n');
        message.push_str(LINGER_HINT);
    }
    message
}
