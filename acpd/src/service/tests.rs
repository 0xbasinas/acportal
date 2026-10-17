use super::{launchd::*, schtasks::*, systemd::*, *};
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
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| u16::from_le_bytes(*chunk))
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
