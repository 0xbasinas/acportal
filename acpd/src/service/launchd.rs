//! launchd LaunchAgent (macOS).
//!
//! Compiled on every platform so its text builders and parsers are tested everywhere.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::*;

pub(super) const PLIST_COMMENT: &str = "<!-- acpd-service-marker: acportal -->";

pub(super) const LABEL: &str = "dev.acportal.acpd";

pub(super) const PLIST_NAME: &str = "dev.acportal.acpd.plist";

#[cfg(target_os = "macos")]
pub(super) fn current_uid() -> u32 {
    // libc is the unix dependency already used by the host. This reads the
    // calling user so the LaunchAgent is registered in that user's gui domain.
    unsafe { libc::getuid() }
}

pub(super) fn launchd_plist_path() -> Result<PathBuf> {
    Ok(home()?
        .join("Library")
        .join("LaunchAgents")
        .join(PLIST_NAME))
}

pub(super) fn launchd_plist(exe: &Path, profile: &Path, log: &Path) -> Result<String> {
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

pub(super) fn plist_marked(text: &str) -> bool {
    text.lines().any(|line| line.trim() == PLIST_COMMENT)
}

pub(super) fn launchd_running(text: &str) -> bool {
    text.lines().any(|line| line.trim() == "state = running")
}

pub(super) fn domain(uid: u32) -> String {
    format!("gui/{uid}")
}

pub(super) fn launchd_target(uid: u32) -> String {
    format!("gui/{uid}/{LABEL}")
}

pub(super) fn install_launchd(
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

pub(super) fn launchd_status(
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

pub(super) fn restart_launchd(
    plist_path: &Path,
    uid: u32,
    runner: &mut dyn Runner,
) -> Result<String> {
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

pub(super) fn uninstall_launchd(
    plist_path: &Path,
    uid: u32,
    runner: &mut dyn Runner,
) -> Result<String> {
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
