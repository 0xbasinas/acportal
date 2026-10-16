//! `acpd peer add`: pair this host with another acpd host as a peer client (design section 7.3).
//!
//! The other host's operator makes a one-use, 60-second code with `acpd pair --role peer`; this
//! side spends it at that host's `/v1/pair` and keeps the credential in `peers.json`. The reply
//! is never printed or logged, redirects are never followed (the credential must come from the
//! origin that was asked, nowhere else), and the other host's error text is never repeated.
use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

const MAX_REPLY: usize = 8 * 1024;

/// Spend a pairing code at `origin` (a normalized `wss://` or loopback `ws://` origin). Returns
/// the credential and, when the other host reports it, this host's device id there.
pub async fn redeem(origin: &str, code: &str, device_name: &str) -> Result<(String, Option<Uuid>)> {
    ensure!(
        !code.is_empty()
            && code.len() <= 32
            && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "that is not a pairing code"
    );
    let device_name = device_name.trim();
    ensure!(
        !device_name.is_empty()
            && device_name.len() <= 80
            && !device_name.chars().any(char::is_control),
        "the device name must be 1..80 printable characters"
    );
    // The pairing route is plain HTTPS (or HTTP to this computer) on the same host and port.
    let base = origin
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .context("could not start the HTTP client")?;
    let mut response = client
        .post(format!("{base}/v1/pair"))
        .json(&json!({"code": code, "deviceName": device_name}))
        .send()
        .await
        .map_err(|_| anyhow!("could not reach {origin}"))?;
    let status = response.status();
    if !status.is_success() {
        // Not the body: the other host's text is not ours to repeat. A redirect lands here too.
        bail!(
            "pairing was refused (HTTP {}); the code may have expired or already been used",
            status.as_u16()
        )
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow!("the reply was cut off"))?
    {
        body.extend_from_slice(&chunk);
        ensure!(body.len() <= MAX_REPLY, "the reply was too large");
    }
    let reply: Value = serde_json::from_slice(&body).context("the reply was not valid")?;
    let token = reply["token"]
        .as_str()
        .filter(|token| token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .context("the reply held no valid credential")?
        .to_owned();
    let device_id = reply["deviceId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok());
    Ok((token, device_id))
}
