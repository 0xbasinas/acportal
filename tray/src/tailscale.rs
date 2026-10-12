//! Optional helper for the common setup: reach the phone over Tailscale. It only reads Tailscale's
//! own status and asks it to publish the host's local port; nothing else on the network changes.
//!
//! Detection tells the situations apart (not installed, app not running, signed out, turned off,
//! waiting for approval, HTTPS certificates not enabled, no permission to control it) so the menu can
//! say exactly what to fix instead of one generic failure.
use anyhow::{Context, Result, bail};
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
pub struct TailnetHost {
    /// Machine name, for example "basinakos".
    pub host_name: String,
    /// Full tailnet name without the trailing dot, for example "basinakos.taild0277.ts.net".
    pub dns_name: String,
}

impl TailnetHost {
    pub fn https_address(&self) -> String {
        format!("https://{}", self.dns_name)
    }
}

/// Where Tailscale stands on this computer.
#[derive(Debug, PartialEq, Eq)]
pub enum Tailnet {
    NotInstalled,
    /// Installed, but its background service or app is not running.
    NotRunning,
    NeedsLogin,
    /// Signed in, but a tailnet admin must approve this computer.
    NeedsApproval,
    /// Signed in but switched off.
    Stopped,
    Starting,
    /// Connected, but the tailnet has no HTTPS certificates (needed for the phone's https address).
    HttpsOff,
    /// Connected, but no MagicDNS name is reported.
    NoName,
    /// This user may not control Tailscale (Linux, when no operator is set).
    NeedsOperator,
    Ready(TailnetHost),
    Unknown(String),
}

impl Tailnet {
    /// One short line for the menu.
    pub fn line(&self) -> String {
        match self {
            Self::NotInstalled => "Tailscale: not installed".into(),
            Self::NotRunning => "Tailscale: not started (open the app and sign in)".into(),
            Self::NeedsLogin => "Tailscale: sign-in needed".into(),
            Self::NeedsApproval => "Tailscale: waiting for admin approval".into(),
            Self::Stopped => "Tailscale: turned off".into(),
            Self::Starting => "Tailscale: starting…".into(),
            Self::HttpsOff => "Tailscale: HTTPS not enabled for your tailnet".into(),
            Self::NoName => "Tailscale: no MagicDNS name".into(),
            Self::NeedsOperator => "Tailscale: this user cannot control it".into(),
            Self::Ready(host) => format!("Tailscale: ready ({})", host.host_name),
            Self::Unknown(text) => {
                format!("Tailscale: {}", text.chars().take(60).collect::<String>())
            }
        }
    }

    /// The host when it is ready, otherwise a message that says what to do.
    pub fn into_result(self) -> Result<TailnetHost> {
        match self {
            Self::Ready(host) => Ok(host),
            Self::NotInstalled => bail!(
                "Tailscale is not installed. Install it from tailscale.com/download, sign in, then try again."
            ),
            Self::NotRunning => bail!(
                "Tailscale is installed but has not started or is not signed in. Open the Tailscale app, sign in and connect (or start the tailscaled service), then try again."
            ),
            Self::NeedsLogin => {
                bail!("Tailscale is signed out. Sign in to Tailscale, then try again.")
            }
            Self::NeedsApproval => bail!(
                "This computer is waiting for approval in your tailnet. Ask an admin to approve it in the Tailscale admin console."
            ),
            Self::Stopped => {
                bail!("Tailscale is turned off. Turn it on (Connect), then try again.")
            }
            Self::Starting => {
                bail!("Tailscale is still starting. Wait a few seconds and try again.")
            }
            Self::HttpsOff => bail!(
                "HTTPS certificates are not enabled for your tailnet. In the Tailscale admin console open DNS, turn on MagicDNS and HTTPS Certificates, then try again."
            ),
            Self::NoName => bail!(
                "Tailscale did not report a name for this computer. Enable MagicDNS and HTTPS in the Tailscale admin console."
            ),
            Self::NeedsOperator => bail!(
                "Tailscale will not let this user control it. On Linux run once: sudo tailscale set --operator=$USER"
            ),
            Self::Unknown(text) => bail!("Tailscale: {text}"),
        }
    }
}

/// Reads the JSON of `tailscale status --json`.
pub fn classify(json: &str) -> Tailnet {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Tailnet::Unknown("could not read its status".into());
    };
    match value["BackendState"].as_str().unwrap_or_default() {
        "Running" => {}
        "NeedsLogin" => return Tailnet::NeedsLogin,
        "NeedsMachineAuth" => return Tailnet::NeedsApproval,
        "Stopped" => return Tailnet::Stopped,
        "Starting" => return Tailnet::Starting,
        // The service answers but has not finished loading yet (right after boot or sign-in, or while the
        // Tailscale app starts). `detect` looks at the saved preferences to tell this from a real sign-out.
        "NoState" => return Tailnet::Starting,
        other => return Tailnet::Unknown(format!("unexpected state {other:?}")),
    }
    let dns_name = value["Self"]["DNSName"]
        .as_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_string();
    if dns_name.is_empty() || value["CurrentTailnet"]["MagicDNSEnabled"] == false {
        return Tailnet::NoName;
    }
    // `CertDomains` lists the names that may get HTTPS certificates; empty means HTTPS is off. Very old
    // versions do not report it at all, and then the check is skipped.
    if value["CertDomains"].as_array().is_some_and(Vec::is_empty) {
        return Tailnet::HttpsOff;
    }
    let host_name = value["Self"]["HostName"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or("Computer")
        .to_string();
    Tailnet::Ready(TailnetHost {
        host_name,
        dns_name,
    })
}

/// Tailscale's own state name from its status JSON (empty when unreadable).
fn backend_state(json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|value| value["BackendState"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// While the state is `NoState`, the saved preferences say whether Tailscale is really signed out or
/// switched off, or is just still starting.
pub fn refine_no_state(prefs_json: &str) -> Tailnet {
    let Ok(prefs) = serde_json::from_str::<serde_json::Value>(prefs_json) else {
        return Tailnet::Starting;
    };
    if prefs["LoggedOut"] == true {
        Tailnet::NeedsLogin
    } else if prefs["WantRunning"] == false {
        Tailnet::Stopped
    } else {
        Tailnet::Starting
    }
}

/// What the command printed when it failed before giving any status.
pub fn classify_failure(message: &str) -> Tailnet {
    let text = message.to_ascii_lowercase();
    if text.contains("access denied")
        || text.contains("permission denied")
        || text.contains("--operator")
    {
        Tailnet::NeedsOperator
    } else if text.contains("failed to connect")
        || text.contains("is tailscale running")
        || text.contains("tailscaled")
        || text.contains("no such file")
        || text.contains("the system cannot find the file")
        || text.contains("connection refused")
    {
        Tailnet::NotRunning
    } else if text.contains("logged out") || text.contains("needslogin") {
        Tailnet::NeedsLogin
    } else {
        let line = message
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("it did not answer");
        Tailnet::Unknown(line.trim().chars().take(120).collect())
    }
}

/// Places the command may be, in the order they are tried. `env` reads an environment variable.
fn candidate_paths(env: impl Fn(&str) -> Option<String>, os: &str) -> Vec<PathBuf> {
    let mut list = vec![PathBuf::from("tailscale")];
    match os {
        "windows" => {
            for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
                if let Some(base) = env(variable) {
                    list.push(PathBuf::from(base).join("Tailscale").join("tailscale.exe"));
                }
            }
            if let Some(base) = env("LOCALAPPDATA") {
                list.push(PathBuf::from(base).join("Tailscale").join("tailscale.exe"));
            }
            list.push(PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe"));
        }
        "macos" => {
            list.push("/Applications/Tailscale.app/Contents/MacOS/Tailscale".into());
            list.push("/opt/homebrew/bin/tailscale".into());
            list.push("/usr/local/bin/tailscale".into());
        }
        _ => {
            // A tray started from a menu or autostart often has a shorter PATH than a terminal.
            for path in [
                "/usr/bin/tailscale",
                "/usr/sbin/tailscale",
                "/usr/local/bin/tailscale",
                "/usr/local/sbin/tailscale",
                "/snap/bin/tailscale",
                "/home/linuxbrew/.linuxbrew/bin/tailscale",
            ] {
                list.push(path.into());
            }
        }
    }
    let mut seen = Vec::new();
    list.retain(|path| {
        let fresh = !seen.contains(path);
        seen.push(path.clone());
        fresh
    });
    list
}

fn candidates() -> Vec<PathBuf> {
    candidate_paths(|name| std::env::var(name).ok(), std::env::consts::OS)
}

fn command(exe: &PathBuf, args: &[&str]) -> Command {
    let mut command = Command::new(exe);
    command.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // no console window
    command
}

/// The Tailscale command, if it is installed.
pub fn locate() -> Option<PathBuf> {
    candidates().into_iter().find(|exe| {
        command(exe, &["version"])
            .output()
            .is_ok_and(|out| out.status.success())
    })
}

/// Looks at Tailscale right now. Never starts, signs in or changes anything.
pub fn detect() -> Tailnet {
    let Some(exe) = locate() else {
        return Tailnet::NotInstalled;
    };
    // `--peers=false` keeps the answer small; very old versions reject the flag, so try without it.
    for args in [
        &["status", "--json", "--peers=false"][..],
        &["status", "--json"][..],
    ] {
        let Ok(output) = command(&exe, args).output() else {
            return Tailnet::NotRunning;
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim_start().starts_with('{') {
            let state = classify(&stdout);
            if state == Tailnet::Starting
                && backend_state(&stdout) == "NoState"
                && let Ok(prefs) = command(&exe, &["debug", "prefs"]).output()
                && prefs.status.success()
            {
                return refine_no_state(&String::from_utf8_lossy(&prefs.stdout));
            }
            return state;
        }
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if args.len() == 3 && stderr.to_ascii_lowercase().contains("flag") {
            continue;
        }
        return classify_failure(&stderr);
    }
    Tailnet::Unknown("it did not answer".into())
}

/// The address Tailscale prints when serving must first be enabled in the admin console.
pub fn find_enable_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')' | '<' | '>' | ',' | '.'))
        })
        .find(|word| word.starts_with("https://login.tailscale.com/"))
        .map(str::to_string)
}

fn collect(mut stream: impl Read + Send + 'static, into: Arc<Mutex<String>>) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        while let Ok(count) = stream.read(&mut buffer) {
            if count == 0 {
                break;
            }
            if let Ok(mut text) = into.lock()
                && text.len() < 8192
            {
                text.push_str(&String::from_utf8_lossy(&buffer[..count]));
            }
        }
    });
}

/// The result of a command that was given a time limit.
struct Captured {
    /// `Some(success)` when it finished, `None` when it was still running at the limit and was stopped.
    finished: Option<bool>,
    /// What it printed (first 8 KiB of both streams).
    text: String,
}

fn run_captured(exe: &PathBuf, args: &[&str], limit: Duration) -> Result<Captured> {
    let mut child = command(exe, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("run tailscale {}", args.first().copied().unwrap_or("")))?;
    let output = Arc::new(Mutex::new(String::new()));
    if let Some(stream) = child.stdout.take() {
        collect(stream, output.clone());
    }
    if let Some(stream) = child.stderr.take() {
        collect(stream, output.clone());
    }
    let deadline = Instant::now() + limit;
    let finished = loop {
        if let Some(status) = child.try_wait()? {
            std::thread::sleep(Duration::from_millis(100)); // let the readers finish
            break Some(status.success());
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let text = output.lock().map(|text| text.clone()).unwrap_or_default();
    Ok(Captured { finished, text })
}

/// `tailscale serve --bg PORT`: publishes the host's local port over HTTPS inside your tailnet and
/// keeps doing so after this program exits. Gives up after 20 seconds (Tailscale waits for you when
/// Serve or HTTPS is not yet enabled for the tailnet); then the page it asked you to visit is opened.
pub fn serve_background(port: u16) -> Result<()> {
    let exe = locate().context("Tailscale is not installed.")?;
    let port = port.to_string();
    let result = run_captured(&exe, &["serve", "--bg", &port], Duration::from_secs(20))?;
    match result.finished {
        Some(true) => Ok(()),
        Some(false) => {
            if matches!(classify_failure(&result.text), Tailnet::NeedsOperator) {
                return Tailnet::NeedsOperator.into_result().map(|_| ());
            }
            let reason = result.text.lines().find(|line| !line.trim().is_empty());
            bail!(
                "tailscale serve failed{}. Run tailscale serve --bg {port} in a terminal to see why.",
                reason
                    .map(|line| format!(": {}", line.trim().chars().take(120).collect::<String>()))
                    .unwrap_or_default()
            )
        }
        None => {
            if let Some(url) = find_enable_url(&result.text) {
                let _ = open::that(&url);
                bail!(
                    "Tailscale needs you to enable Serve/HTTPS for your tailnet. The page was opened in your browser ({url}); enable it, then choose Set up again."
                )
            }
            bail!(
                "tailscale serve is waiting for you. Run tailscale serve --bg {port} in a terminal; Tailscale may ask you to enable HTTPS for your tailnet."
            )
        }
    }
}

/// `tailscale up`: reconnects a Tailscale that is signed in but switched off. If it needs a sign-in, the
/// page it prints is opened in your browser and this gives up (you finish signing in there).
fn bring_up() -> Result<()> {
    let exe = locate().context("Tailscale is not installed.")?;
    let result = run_captured(&exe, &["up"], Duration::from_secs(15))?;
    match result.finished {
        Some(true) => Ok(()),
        Some(false) => match classify_failure(&result.text) {
            Tailnet::NeedsOperator => Tailnet::NeedsOperator.into_result().map(|_| ()),
            _ => bail!("tailscale up failed. Open the Tailscale app and connect it."),
        },
        None => {
            if let Some(url) = find_enable_url(&result.text) {
                let _ = open::that(&url);
                bail!(
                    "Tailscale needs you to sign in. The sign-in page was opened in your browser ({url}); finish there, then choose Set up again."
                )
            }
            bail!(
                "Tailscale did not connect. Open the Tailscale app and connect it, then try again."
            )
        }
    }
}

/// How the wait for Tailscale goes. Pure so it can be tested with a scripted Tailscale.
///
/// - `Ready` returns at once.
/// - `Starting` (still loading) and `NotRunning` (service not answering yet) are waited out, because both
///   are normal for a few seconds after boot or sign-in; `NotRunning` gives up sooner.
/// - `Stopped` (signed in but switched off) is reconnected once with `up`, then waited out.
/// - Everything else (not installed, signed out, needs approval, HTTPS off, ...) needs the user, so it
///   returns that explanation immediately.
pub fn wait_until_ready(
    mut detect: impl FnMut() -> Tailnet,
    mut up: impl FnMut() -> Result<()>,
    pause: impl Fn(),
    progress: impl Fn(&str),
    polls: u32,
) -> Result<TailnetHost> {
    let mut tried_up = false;
    let mut not_running = 0;
    for _ in 0..polls {
        match detect() {
            Tailnet::Ready(host) => return Ok(host),
            Tailnet::Stopped if !tried_up => {
                tried_up = true;
                progress("Tailscale is turned off; turning it on…");
                up()?;
            }
            Tailnet::NotRunning => {
                not_running += 1;
                if not_running > 8 {
                    return Tailnet::NotRunning.into_result();
                }
                progress("Waiting for Tailscale to start…");
            }
            Tailnet::Starting | Tailnet::Stopped => progress("Waiting for Tailscale to connect…"),
            other => return other.into_result(),
        }
        pause();
    }
    bail!(
        "Tailscale did not finish connecting. Open the Tailscale app, check it is signed in and connected, then try again."
    )
}

/// Waits (up to about 30 seconds) for Tailscale to be connected and ready for the phone, reconnecting it
/// if it was switched off, and reports progress as it goes. Runs on a worker thread.
pub fn ensure_ready(progress: impl Fn(&str)) -> Result<TailnetHost> {
    wait_until_ready(
        detect,
        bring_up,
        || std::thread::sleep(Duration::from_secs(1)),
        progress,
        30,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(json: &str) -> TailnetHost {
        match classify(json) {
            Tailnet::Ready(host) => host,
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn running_status_gives_the_https_address() {
        let host = ready(
            r#"{"BackendState":"Running","Self":{"DNSName":"basinakos.taild0277.ts.net.","HostName":"basinakos"},"CertDomains":["basinakos.taild0277.ts.net"],"CurrentTailnet":{"MagicDNSEnabled":true}}"#,
        );
        assert_eq!(host.https_address(), "https://basinakos.taild0277.ts.net");
        assert_eq!(host.host_name, "basinakos");
        // Older versions do not report CertDomains; that is not treated as "HTTPS off".
        ready(r#"{"BackendState":"Running","Self":{"DNSName":"a.b.ts.net.","HostName":"a"}}"#);
    }

    #[test]
    fn every_state_is_told_apart_and_explained() {
        let state =
            |backend: &str| classify(&format!(r#"{{"BackendState":"{backend}","Self":{{}}}}"#));
        assert_eq!(state("NeedsLogin"), Tailnet::NeedsLogin);
        assert_eq!(state("NeedsMachineAuth"), Tailnet::NeedsApproval);
        assert_eq!(state("Stopped"), Tailnet::Stopped);
        assert_eq!(state("Starting"), Tailnet::Starting);
        assert_eq!(state("NoState"), Tailnet::Starting);
        assert!(matches!(state("Weird"), Tailnet::Unknown(_)));
        assert_eq!(
            classify(r#"{"BackendState":"Running","Self":{"DNSName":"."}}"#),
            Tailnet::NoName
        );
        assert_eq!(
            classify(
                r#"{"BackendState":"Running","Self":{"DNSName":"a.ts.net."},"CurrentTailnet":{"MagicDNSEnabled":false}}"#
            ),
            Tailnet::NoName
        );
        assert_eq!(
            classify(
                r#"{"BackendState":"Running","Self":{"DNSName":"a.ts.net."},"CertDomains":[]}"#
            ),
            Tailnet::HttpsOff
        );
        assert!(matches!(classify("not json"), Tailnet::Unknown(_)));
        let stopped = state("Stopped").into_result().unwrap_err();
        assert!(stopped.to_string().contains("turned off"));
        assert!(
            Tailnet::HttpsOff
                .into_result()
                .unwrap_err()
                .to_string()
                .contains("HTTPS")
        );
        assert!(
            Tailnet::NoName
                .into_result()
                .unwrap_err()
                .to_string()
                .contains("MagicDNS")
        );
    }

    #[test]
    fn failures_without_status_are_recognised() {
        assert_eq!(
            classify_failure("failed to connect to local Tailscale service; is Tailscale running?"),
            Tailnet::NotRunning
        );
        assert_eq!(
            classify_failure(
                "Access denied: status access denied\nUse 'sudo tailscale ...' or --operator"
            ),
            Tailnet::NeedsOperator
        );
        assert_eq!(classify_failure("Logged out."), Tailnet::NeedsLogin);
        match classify_failure("\nsomething new\nmore") {
            Tailnet::Unknown(text) => assert_eq!(text, "something new"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn menu_lines_are_short_and_name_the_computer() {
        let host = ready(
            r#"{"BackendState":"Running","Self":{"DNSName":"pc.x.ts.net.","HostName":"pc"}}"#,
        );
        assert_eq!(Tailnet::Ready(host).line(), "Tailscale: ready (pc)");
        assert!(Tailnet::Unknown("x".repeat(500)).line().len() < 80);
    }

    #[test]
    fn candidate_places_cover_each_system_without_duplicates() {
        let env = |name: &str| match name {
            "ProgramFiles" => Some(r"C:\Program Files".to_string()),
            "ProgramW6432" => Some(r"C:\Program Files".to_string()),
            "LOCALAPPDATA" => Some(r"C:\Users\me\AppData\Local".to_string()),
            _ => None,
        };
        let windows = candidate_paths(env, "windows");
        assert_eq!(windows[0], PathBuf::from("tailscale"));
        assert!(
            windows.contains(
                &PathBuf::from(r"C:\Users\me\AppData\Local")
                    .join("Tailscale")
                    .join("tailscale.exe")
            )
        );
        assert_eq!(
            windows
                .iter()
                .filter(|path| path.to_string_lossy().starts_with(r"C:\Program Files\"))
                .count(),
            1,
            "the same folder is listed once"
        );
        let linux = candidate_paths(|_| None, "linux");
        assert!(linux.contains(&PathBuf::from("/snap/bin/tailscale")));
        assert!(linux.contains(&PathBuf::from("/usr/sbin/tailscale")));
        assert!(
            candidate_paths(|_| None, "macos")
                .iter()
                .any(|p| p.to_string_lossy().contains("Tailscale.app"))
        );
    }

    #[test]
    fn the_enable_page_is_found_only_on_tailscale_com() {
        let text = "\nServe is not enabled on your tailnet.\nTo enable, visit:\n\n\t  https://login.tailscale.com/f/serve?node=n123\n";
        assert_eq!(
            find_enable_url(text).as_deref(),
            Some("https://login.tailscale.com/f/serve?node=n123")
        );
        assert_eq!(find_enable_url("visit https://evil.example/f/serve"), None);
        assert_eq!(find_enable_url(""), None);
    }

    /// Runs the real lookup: whatever this computer has, it must answer with a sentence, not fail.
    #[test]
    fn detecting_the_real_tailscale_gives_a_menu_line() {
        let state = detect();
        println!("{state:?}");
        assert!(state.line().starts_with("Tailscale: "));
    }

    #[test]
    fn a_no_state_service_is_told_apart_by_its_saved_preferences() {
        assert_eq!(
            refine_no_state(r#"{"LoggedOut":true,"WantRunning":true}"#),
            Tailnet::NeedsLogin
        );
        assert_eq!(
            refine_no_state(r#"{"LoggedOut":false,"WantRunning":false}"#),
            Tailnet::Stopped
        );
        assert_eq!(
            refine_no_state(r#"{"LoggedOut":false,"WantRunning":true}"#),
            Tailnet::Starting
        );
        assert_eq!(refine_no_state("garbage"), Tailnet::Starting);
        assert_eq!(backend_state(r#"{"BackendState":"NoState"}"#), "NoState");
        assert_eq!(backend_state("x"), "");
    }

    fn host() -> TailnetHost {
        TailnetHost {
            host_name: "pc".into(),
            dns_name: "pc.x.ts.net".into(),
        }
    }

    type Run = (Result<TailnetHost>, Vec<String>, u32, usize);

    /// Runs the wait against a scripted Tailscale: (result, notes, `up` calls, polls used).
    fn scripted(states: Vec<Tailnet>, up_fails: bool) -> Run {
        let mut queue = states.into_iter();
        let polls = std::cell::Cell::new(0usize);
        let ups = std::cell::Cell::new(0u32);
        let notes = std::cell::RefCell::new(Vec::new());
        let result = wait_until_ready(
            || {
                polls.set(polls.get() + 1);
                queue.next().unwrap_or(Tailnet::Starting)
            },
            || {
                ups.set(ups.get() + 1);
                if up_fails { bail!("up failed") } else { Ok(()) }
            },
            || {},
            |note| notes.borrow_mut().push(note.to_string()),
            30,
        );
        (result, notes.into_inner(), ups.get(), polls.get())
    }

    #[test]
    fn a_slow_start_is_waited_out_then_succeeds() {
        let (result, notes, ups, polls) = scripted(
            vec![
                Tailnet::NotRunning,
                Tailnet::Starting,
                Tailnet::Starting,
                Tailnet::Ready(host()),
            ],
            false,
        );
        assert_eq!(result.unwrap(), host());
        assert_eq!(polls, 4);
        assert_eq!(ups, 0, "nothing is changed while it is just starting");
        assert!(
            notes
                .iter()
                .any(|note| note.contains("Waiting for Tailscale"))
        );
    }

    #[test]
    fn a_switched_off_tailscale_is_reconnected_once() {
        let (result, notes, ups, _) = scripted(
            vec![Tailnet::Stopped, Tailnet::Stopped, Tailnet::Ready(host())],
            false,
        );
        assert!(result.is_ok());
        assert_eq!(ups, 1);
        assert!(notes[0].contains("turning it on"));
        let (result, _, ups, polls) = scripted(vec![Tailnet::Stopped], true);
        assert!(result.unwrap_err().to_string().contains("up failed"));
        assert_eq!((ups, polls), (1, 1));
    }

    #[test]
    fn problems_that_need_the_user_are_reported_at_once() {
        for (state, text) in [
            (Tailnet::NeedsLogin, "signed out"),
            (Tailnet::NeedsApproval, "approval"),
            (Tailnet::HttpsOff, "HTTPS"),
            (Tailnet::NotInstalled, "not installed"),
            (Tailnet::NeedsOperator, "operator"),
        ] {
            let (result, _, ups, polls) = scripted(vec![state], false);
            assert!(result.unwrap_err().to_string().contains(text));
            assert_eq!((ups, polls), (0, 1), "no waiting and no changes");
        }
    }

    #[test]
    fn waiting_ends_with_a_clear_message() {
        let (result, _, _, polls) = scripted(vec![], false);
        assert_eq!(polls, 30);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("did not finish connecting")
        );
        let (result, _, _, polls) = scripted((0..20).map(|_| Tailnet::NotRunning).collect(), false);
        assert_eq!(
            polls, 9,
            "a service that never answers is given up on sooner"
        );
        assert!(result.unwrap_err().to_string().contains("has not started"));
    }
}
