//! Task Scheduler logon task (Windows).
//!
//! Compiled on every platform so its text builders and parsers are tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use super::*;

pub(super) const TASK: &str = "acpd";

/// Added when nothing else keeps the service's output.
pub(super) const NO_LOG_NOTE: &str = "No log file is configured and a scheduled task keeps no output. Set `[logging] file` in the host config to keep logs.";

pub(super) const QUERY_XML: &[&str] = &["/Query", "/TN", TASK, "/XML", "ONE"];

pub(super) const QUERY_ALL: &[&str] = &["/Query", "/FO", "CSV", "/NH"];

pub(super) const QUERY_LIST: &[&str] = &["/Query", "/TN", TASK, "/FO", "LIST", "/V"];

#[cfg(windows)]
pub(super) fn windows_user() -> Result<String> {
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

pub(super) fn check_task_user(value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains(['\\', '/', '<', '>', '&', '*', '?', '"', '\n', '\r'])
        || value.chars().any(|c| c.is_control())
    {
        bail!("the Windows account name cannot be used for a per-user task");
    }
    Ok(())
}

pub(super) fn check_account(user: &str) -> Result<()> {
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

/// Quote one task argument with CommandLineToArgvW rules. Every argument is quoted.
pub(super) fn windows_arg(arg: &str) -> Result<String> {
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

pub(super) fn windows_arguments(profile: &Path) -> Result<String> {
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

pub(super) fn task_xml(exe: &Path, profile: &Path, user: &str) -> Result<String> {
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

pub(super) fn encode_utf16_le_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

pub(super) fn xml_element_is(text: &str, element: &str, value: &str) -> bool {
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    text.match_indices(&open).any(|(index, _)| {
        let rest = &text[index + open.len()..];
        rest.split_once(&close)
            .is_some_and(|(body, _)| body.trim() == value)
    })
}

pub(super) fn task_running(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        let Some(value) = line.strip_prefix("Status:") else {
            return false;
        };
        value.trim().eq_ignore_ascii_case("running")
    })
}

pub(super) fn xml_enabled(text: &str) -> bool {
    text.contains("<Enabled>true</Enabled>") && !text.contains("<Enabled>false</Enabled>")
}

/// Look the task up. `schtasks` exits 1 for a missing task and for other errors, and its message
/// is localized, so a message that is not the English one is settled by listing every task
/// (`/FO CSV`, whose first column is the task name) and checking whether `\acpd` is there.
pub(super) fn probe_task(runner: &mut dyn Runner) -> Result<(Definition, Captured)> {
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
pub(super) fn lists_task(csv: &str) -> bool {
    let wanted = format!("\"\\{TASK}\"");
    csv.lines().any(|line| {
        let first = line.trim_start_matches('\u{feff}').split(',').next();
        first.is_some_and(|name| name.eq_ignore_ascii_case(&wanted))
    })
}

pub(super) fn classify_task(captured: &Captured) -> Result<Definition> {
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

pub(super) fn is_missing_task(captured: &Captured) -> bool {
    let text = format!("{}{}", captured.stdout, captured.stderr).to_ascii_lowercase();
    text.contains("cannot find the file specified")
}

pub(super) fn install_task(
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

pub(super) fn task_status(profile: &Path, runner: &mut dyn Runner) -> Result<String> {
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

pub(super) fn restart_task(runner: &mut dyn Runner) -> Result<String> {
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

pub(super) fn uninstall_task(runner: &mut dyn Runner) -> Result<String> {
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

pub(super) fn schtasks_create_args(xml_path: &Path) -> Result<Vec<String>> {
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
