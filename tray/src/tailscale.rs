//! Optional helper for the common setup: reach the phone over Tailscale. It only reads Tailscale's
//! own status and asks it to publish the host's local port; nothing else on the network changes.
use anyhow::{Context, Result, bail};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
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

/// Reads `tailscale status --json`.
pub fn parse_status(json: &str) -> Result<TailnetHost> {
    let value: serde_json::Value = serde_json::from_str(json).context("read Tailscale status")?;
    if value["BackendState"] != "Running" {
        bail!("Tailscale is not connected. Sign in to Tailscale, then try again.")
    }
    let dns_name = value["Self"]["DNSName"]
        .as_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_string();
    if dns_name.is_empty() {
        bail!(
            "Tailscale did not report a name for this computer. Enable MagicDNS and HTTPS in the Tailscale admin console."
        )
    }
    let host_name = value["Self"]["HostName"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or("Computer")
        .to_string();
    Ok(TailnetHost {
        host_name,
        dns_name,
    })
}

fn candidates() -> Vec<PathBuf> {
    let mut list = vec![PathBuf::from("tailscale")];
    if cfg!(windows) {
        list.push(PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe"));
    }
    if cfg!(target_os = "macos") {
        list.push(PathBuf::from(
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        ));
    }
    list
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

pub fn status() -> Result<TailnetHost> {
    let exe = locate().context("Tailscale is not installed. Install it from tailscale.com/download, sign in, then try again.")?;
    let output = command(&exe, &["status", "--json"])
        .output()
        .context("run tailscale")?;
    parse_status(&String::from_utf8_lossy(&output.stdout))
}

/// `tailscale serve --bg PORT`: publishes the host's local port over HTTPS inside your tailnet and
/// keeps doing so after this program exits. Gives up after 20 seconds (Tailscale waits for you when
/// HTTPS is not yet enabled for the tailnet).
pub fn serve_background(port: u16) -> Result<()> {
    let exe = locate().context("Tailscale is not installed.")?;
    let port = port.to_string();
    let mut child = command(&exe, &["serve", "--bg", &port])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("run tailscale serve")?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            bail!(
                "tailscale serve failed. Run tailscale serve --bg {port} in a terminal to see why."
            )
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            bail!(
                "tailscale serve is waiting for you. Run tailscale serve --bg {port} in a terminal; Tailscale may ask you to enable HTTPS for your tailnet."
            )
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn running_status_gives_the_https_address() {
        let host = parse_status(
            r#"{"BackendState":"Running","Self":{"DNSName":"basinakos.taild0277.ts.net.","HostName":"basinakos"}}"#,
        )
        .unwrap();
        assert_eq!(host.https_address(), "https://basinakos.taild0277.ts.net");
        assert_eq!(host.host_name, "basinakos");
    }
    #[test]
    fn stopped_or_nameless_tailscale_is_explained() {
        let stopped = parse_status(r#"{"BackendState":"Stopped","Self":{}}"#).unwrap_err();
        assert!(stopped.to_string().contains("not connected"));
        let nameless =
            parse_status(r#"{"BackendState":"Running","Self":{"DNSName":"."}}"#).unwrap_err();
        assert!(nameless.to_string().contains("MagicDNS"));
        assert!(parse_status("not json").is_err());
    }
}
