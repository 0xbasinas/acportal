use crate::config::Config;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{
    fs::OpenOptions,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Doctor fails when the state or log volume has less free space than this.
pub const MIN_FREE_BYTES: u64 = 64 * 1024 * 1024;
/// Certificates expiring sooner than this pass with a renewal notice.
const RENEWAL_NOTICE_SECONDS: i64 = 30 * 24 * 3600;

/// Operational checks use disposable resources and never open credential stores.
/// The configured TLS private key is read only to confirm it matches the certificate,
/// exactly as `start` loads it; its contents are never printed.
pub fn operational_checks(config: &Config) -> Vec<(String, bool)> {
    let port = TcpListener::bind(config.server.listen);
    let port_ok = port.is_ok();
    drop(port);
    let storage = writable_ancestor(&config.state_directory);
    vec![
        (
            format!(
                "Listener {}: {}",
                config.server.listen,
                if port_ok {
                    "available"
                } else {
                    "unavailable; check for a running host or address conflict"
                }
            ),
            port_ok,
        ),
        (
            format!(
                "State storage: {}",
                if storage {
                    "write/remove probe passed in existing directory or nearest parent"
                } else {
                    "write/remove probe failed; check directory access"
                }
            ),
            storage,
        ),
        (
            format!(
                "Runtime: {}s request timeout, {} byte frames, {} sessions, {} events / {} bytes of history",
                config.runtime.request_timeout_seconds,
                config.runtime.max_frame_bytes,
                config.runtime.max_sessions,
                config.runtime.history_events,
                config.runtime.history_bytes
            ),
            true,
        ),
        free_space_check(&config.state_directory, config.logging.file.as_deref()),
        logging_check(config),
        tls_check(config, unix_now()),
    ]
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Opt-in sign-in check. Starts the agent with every host callback disabled, sends
/// `initialize` and `session/new` in an authorized workspace, then stops it. No prompt is sent,
/// so no model request is made by the host; agents may still contact their provider while
/// creating a session. Agent error text and credentials are never printed.
pub async fn agent_sign_in_check(
    config: &Config,
    registry: crate::registry::Registry,
    agent: &str,
    workspace: &Path,
) -> (String, bool) {
    let label = format!("Agent {} sign-in", printable(agent));
    let manager = match crate::session::SessionManager::new(config.clone(), registry) {
        Ok(manager) => manager,
        Err(_) => return (format!("{label}: host configuration is invalid"), false),
    };
    let access = crate::connection::WorkspaceAccess {
        read_files: false,
        write_files: false,
        terminal: false,
    };
    let opened = manager
        .open_configured(agent, workspace, None, Vec::new(), access)
        .await;
    let result = match opened {
        Ok(session) => {
            let metadata = session.metadata();
            if metadata.acp_session_id.is_empty() {
                let methods: Vec<String> = metadata.initialization["authMethods"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(8)
                    .map(|method| {
                        let id = printable(method["id"].as_str().unwrap_or("?"));
                        match method["name"].as_str() {
                            Some(name) => format!("{id} ({})", printable(name)),
                            None => id,
                        }
                    })
                    .collect();
                (
                    format!(
                        "{label}: sign-in required before sessions can start; agent offers {}. Sign in through the agent itself or from the app",
                        methods.join(", ")
                    ),
                    false,
                )
            } else {
                (
                    format!(
                        "{label}: ready; initialize and session/new succeeded with no prompt sent. A model request was not exercised"
                    ),
                    true,
                )
            }
        }
        Err(error) => {
            let reason =
                if let Some(code) = error.downcast_ref::<crate::connection::AgentRpcError>() {
                    format!("agent returned JSON-RPC error code {}", code.0)
                } else {
                    // Host-side causes are fixed strings (unknown/disabled agent, workspace policy,
                    // missing executable, protocol mismatch); agent text never reaches here.
                    format!("{error}")
                };
            (format!("{label}: failed; {reason}"), false)
        }
    };
    manager.shutdown().await;
    result
}

/// Bounds agent- or operator-supplied labels before printing: at most 64 printable characters.
fn printable(text: &str) -> String {
    let mut bounded: String = text
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect();
    if text.chars().count() > 64 {
        bounded.push('…');
    }
    bounded
}

/// Nearest existing directory at or above `path`, without creating anything.
fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut directory = path;
    loop {
        match std::fs::metadata(directory) {
            Ok(metadata) if metadata.is_dir() => return Some(directory.to_owned()),
            Ok(_) => return None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        directory = match directory.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ if directory != Path::new(".") => Path::new("."),
            _ => return None,
        };
    }
}

fn free_space_check(state: &Path, log_file: Option<&Path>) -> (String, bool) {
    let mut lines = Vec::new();
    let mut passed = true;
    let log_directory = log_file.and_then(Path::parent);
    for (label, path) in [("state", Some(state)), ("log", log_directory)] {
        let Some(path) = path else { continue };
        match existing_ancestor(path).map(|directory| fs2::available_space(&directory)) {
            Some(Ok(bytes)) => {
                let enough = bytes >= MIN_FREE_BYTES;
                passed &= enough;
                lines.push(format!(
                    "{label} {} MiB available{}",
                    bytes / (1024 * 1024),
                    if enough {
                        String::new()
                    } else {
                        format!(" (below {} MiB minimum)", MIN_FREE_BYTES / (1024 * 1024))
                    }
                ));
            }
            _ => {
                passed = false;
                lines.push(format!("{label} volume could not be read"));
            }
        }
    }
    (format!("Free space: {}", lines.join("; ")), passed)
}

fn logging_check(config: &Config) -> (String, bool) {
    let (message, passed) = log_file_check(config);
    if config.logging.frame_metadata {
        return (
            format!("{message}; ACP frame metadata (no content) logged at debug level"),
            passed,
        );
    }
    (message, passed)
}

fn log_file_check(config: &Config) -> (String, bool) {
    let logging = &config.logging;
    let Some(file) = &logging.file else {
        return (
            "Log file: not configured; host logs go to stderr only".into(),
            true,
        );
    };
    let budget = format!(
        "{} bytes per file, {} retained, at most {} bytes total",
        logging.max_file_bytes,
        logging.retained_files,
        logging.disk_budget()
    );
    let parent = file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let parent_ok = parent.is_none_or(|parent| parent.is_dir());
    if file.is_dir() || !parent_ok {
        return (
            format!(
                "Log file: unusable; the parent must be an existing directory and the path a file ({budget})"
            ),
            false,
        );
    }
    let writable = writable_ancestor(parent.unwrap_or(Path::new(".")));
    (
        format!(
            "Log file: {} ({budget})",
            if writable {
                "directory write/remove probe passed"
            } else {
                "directory write/remove probe failed; check access"
            }
        ),
        writable,
    )
}

fn tls_check(config: &Config, now: i64) -> (String, bool) {
    let (Some(certificate), Some(key)) = (
        &config.server.tls_certificate,
        &config.server.tls_private_key,
    ) else {
        return (
            "TLS: not configured; only a loopback cleartext listener is allowed".into(),
            true,
        );
    };
    match inspect_tls(certificate, key, now) {
        Ok(summary) => (format!("TLS: {summary}"), true),
        Err(problem) => (format!("TLS: {problem}"), false),
    }
}

/// Mirrors the startup PEM loading and key check, then reads the leaf validity period.
/// Hostname coverage and issuer trust depend on the phone and are not checked here.
fn inspect_tls(certificate: &Path, key: &Path, now: i64) -> Result<String, String> {
    let chain =
        std::fs::read(certificate).map_err(|_| "certificate file could not be read".to_string())?;
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&chain)
        .collect::<Result<_, _>>()
        .map_err(|_| "certificate file is not valid PEM".to_string())?;
    if chain.is_empty() {
        return Err("certificate file contains no certificates".into());
    }
    let key = std::fs::read(key).map_err(|_| "private key file could not be read".to_string())?;
    let mut keys = PrivateKeyDer::pem_slice_iter(&key).filter_map(Result::ok);
    let parsed_key = keys
        .next()
        .ok_or_else(|| "private key file contains no supported key".to_string())?;
    if keys.next().is_some() {
        return Err("private key file contains more than one key".into());
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| format!("TLS configuration failed: {error}"))?
        .with_no_client_auth()
        .with_single_cert(chain.clone(), parsed_key)
        .map_err(|error| format!("certificate and private key are unusable together: {error}"))?;
    let (not_before, not_after) = certificate_validity(&chain[0])
        .ok_or_else(|| "leaf certificate validity could not be parsed".to_string())?;
    let until = format_utc(not_after);
    if now < not_before {
        return Err(format!(
            "leaf certificate is not valid until {}",
            format_utc(not_before)
        ));
    }
    if now > not_after {
        return Err(format!("leaf certificate expired at {until}"));
    }
    let days = (not_after - now) / 86_400;
    Ok(format!(
        "{} certificate(s), key matches, leaf valid until {until} ({days} days){}; restart the host after renewal",
        chain.len(),
        if not_after - now < RENEWAL_NOTICE_SECONDS {
            ", renew soon"
        } else {
            ""
        }
    ))
}

/// Reads one DER element, returning (tag, contents, remainder).
fn der_element(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = input.split_first()?;
    let (&first, mut rest) = rest.split_first()?;
    let length = if first < 0x80 {
        usize::from(first)
    } else {
        let count = usize::from(first & 0x7F);
        if count == 0 || count > 4 || rest.len() < count {
            return None;
        }
        let mut length = 0usize;
        for &byte in &rest[..count] {
            length = (length << 8) | usize::from(byte);
        }
        rest = &rest[count..];
        length
    };
    if rest.len() < length {
        return None;
    }
    Some((tag, &rest[..length], &rest[length..]))
}

/// Extracts notBefore/notAfter as Unix seconds from an X.509 certificate.
fn certificate_validity(certificate: &[u8]) -> Option<(i64, i64)> {
    const SEQUENCE: u8 = 0x30;
    let (tag, certificate, _) = der_element(certificate)?;
    if tag != SEQUENCE {
        return None;
    }
    let (tag, tbs, _) = der_element(certificate)?;
    if tag != SEQUENCE {
        return None;
    }
    let mut fields = tbs;
    let (tag, _, rest) = der_element(fields)?;
    // Optional explicit [0] version precedes the serial number.
    if tag == 0xA0 {
        fields = rest;
    }
    let (_, _, fields) = der_element(fields)?; // serialNumber
    let (_, _, fields) = der_element(fields)?; // signature algorithm
    let (_, _, fields) = der_element(fields)?; // issuer
    let (tag, validity, _) = der_element(fields)?;
    if tag != SEQUENCE {
        return None;
    }
    let (before_tag, before, rest) = der_element(validity)?;
    let (after_tag, after, _) = der_element(rest)?;
    Some((
        parse_time(before_tag, before)?,
        parse_time(after_tag, after)?,
    ))
}

/// Parses DER UTCTime (0x17) or GeneralizedTime (0x18) in the RFC 5280 `Z` form.
fn parse_time(tag: u8, value: &[u8]) -> Option<i64> {
    let text = std::str::from_utf8(value).ok()?;
    let digits = text.strip_suffix('Z')?;
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (year, rest) = match (tag, digits.len()) {
        (0x17, 12) => {
            let short: i64 = digits[..2].parse().ok()?;
            (
                if short < 50 {
                    2000 + short
                } else {
                    1900 + short
                },
                &digits[2..],
            )
        }
        (0x18, 14) => (digits[..4].parse().ok()?, &digits[4..]),
        _ => return None,
    };
    let field = |range: std::ops::Range<usize>| rest[range].parse::<i64>().ok();
    let (month, day) = (field(0..2)?, field(2..4)?);
    let (hour, minute, second) = (field(4..6)?, field(6..8)?, field(8..10)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn format_utc(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    // Inverse of days_from_civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        time / 3600,
        time % 3600 / 60
    )
}

fn writable_ancestor(path: &Path) -> bool {
    let Some(directory) = existing_ancestor(path) else {
        return false;
    };
    let probe = directory.join(format!(".acpd-doctor-{}", uuid::Uuid::new_v4()));
    let Ok(mut file) = OpenOptions::new().write(true).create_new(true).open(&probe) else {
        return false;
    };
    let written = file
        .write_all(b"acpd storage probe\n")
        .and_then(|_| file.sync_all())
        .is_ok();
    drop(file);
    let removed = std::fs::remove_file(probe).is_ok();
    written && removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_occupied_port_and_preserves_storage_contents() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("existing"), b"keep").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut config = Config::default();
        config.server.listen = listener.local_addr().unwrap();
        config.state_directory = directory.path().to_owned();
        let checks = operational_checks(&config);
        assert!(!checks[0].1);
        assert!(checks[1].1);
        assert_eq!(
            std::fs::read(directory.path().join("existing")).unwrap(),
            b"keep"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        drop(listener);
        assert!(operational_checks(&config)[0].1);
    }

    #[test]
    fn absent_storage_checks_parent_without_creating_state_and_rejects_files() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("new").join("state");
        assert!(writable_ancestor(&missing));
        assert!(!directory.path().join("new").exists());
        let file = directory.path().join("file");
        std::fs::write(&file, b"keep").unwrap();
        assert!(!writable_ancestor(&file));
        assert!(!writable_ancestor(&file.join("state")));
    }

    fn certificate(
        directory: &Path,
        not_before: (i32, u8, u8),
        not_after: (i32, u8, u8),
        mismatched_key: bool,
    ) -> (PathBuf, PathBuf) {
        let key = rcgen::KeyPair::generate().unwrap();
        let mut params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
        params.not_before = rcgen::date_time_ymd(not_before.0, not_before.1, not_before.2);
        params.not_after = rcgen::date_time_ymd(not_after.0, not_after.1, not_after.2);
        let cert = params.self_signed(&key).unwrap();
        let written_key = if mismatched_key {
            rcgen::KeyPair::generate().unwrap()
        } else {
            key
        };
        let cert_path = directory.join("fullchain.pem");
        let key_path = directory.join("privkey.pem");
        std::fs::write(&cert_path, cert.pem()).unwrap();
        std::fs::write(&key_path, written_key.serialize_pem()).unwrap();
        (cert_path, key_path)
    }

    fn tls_config(cert: PathBuf, key: PathBuf) -> Config {
        let mut config = Config::default();
        config.server.tls_certificate = Some(cert);
        config.server.tls_private_key = Some(key);
        config
    }

    // 2026-06-01 00:00:00 UTC
    const NOW: i64 = 1_780_272_000;

    #[test]
    fn tls_check_reports_validity_and_rejects_expired_future_and_mismatched_material() {
        let directory = tempfile::tempdir().unwrap();
        assert!(tls_check(&Config::default(), NOW).1);

        let (cert, key) = certificate(directory.path(), (2026, 1, 1), (2027, 1, 1), false);
        let (message, passed) = tls_check(&tls_config(cert, key), NOW);
        assert!(passed, "{message}");
        assert!(
            message.contains("1 certificate(s), key matches"),
            "{message}"
        );
        assert!(
            message.contains("2027-01-01 00:00 UTC (214 days)"),
            "{message}"
        );
        assert!(!message.contains("renew soon"), "{message}");
        assert!(!message.contains("PRIVATE KEY"));

        let (cert, key) = certificate(directory.path(), (2026, 1, 1), (2026, 6, 15), false);
        let (message, passed) = tls_check(&tls_config(cert, key), NOW);
        assert!(passed && message.contains("renew soon"), "{message}");

        let (cert, key) = certificate(directory.path(), (2025, 1, 1), (2026, 5, 1), false);
        let (message, passed) = tls_check(&tls_config(cert, key), NOW);
        assert!(
            !passed && message.contains("expired at 2026-05-01"),
            "{message}"
        );

        let (cert, key) = certificate(directory.path(), (2026, 7, 1), (2027, 7, 1), false);
        let (message, passed) = tls_check(&tls_config(cert, key), NOW);
        assert!(
            !passed && message.contains("not valid until 2026-07-01"),
            "{message}"
        );

        let (cert, key) = certificate(directory.path(), (2026, 1, 1), (2027, 1, 1), true);
        let (message, passed) = tls_check(&tls_config(cert, key.clone()), NOW);
        assert!(
            !passed && message.contains("unusable together"),
            "{message}"
        );

        std::fs::write(&key, b"not a key").unwrap();
        let (message, passed) = tls_check(
            &tls_config(directory.path().join("fullchain.pem"), key),
            NOW,
        );
        assert!(!passed && message.contains("no supported key"), "{message}");

        let missing = directory.path().join("missing.pem");
        let (message, passed) = tls_check(&tls_config(missing.clone(), missing), NOW);
        assert!(
            !passed && message.contains("could not be read"),
            "{message}"
        );
        assert!(!message.contains(directory.path().to_str().unwrap()));
    }

    #[test]
    fn validity_parser_handles_both_time_forms_and_round_trips_dates() {
        assert_eq!(parse_time(0x17, b"700101000000Z"), Some(0));
        assert_eq!(parse_time(0x17, b"491231235959Z"), Some(2_524_607_999));
        assert_eq!(parse_time(0x18, b"20500101000000Z"), Some(2_524_608_000));
        assert_eq!(parse_time(0x17, b"7001010000Z"), None);
        assert_eq!(parse_time(0x18, b"20501301000000Z"), None);
        assert_eq!(parse_time(0x17, b"700101000000+0100"), None);
        assert_eq!(format_utc(NOW), "2026-06-01 00:00 UTC");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00 UTC");
        assert!(certificate_validity(b"\x30\x03\x02\x01").is_none());
    }

    #[test]
    fn free_space_and_log_checks_create_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("missing").join("state");
        let (message, passed) = free_space_check(&state, None);
        assert!(message.starts_with("Free space: state "), "{message}");
        assert!(
            passed || message.contains("below 64 MiB minimum"),
            "{message}"
        );
        assert!(!directory.path().join("missing").exists());

        let mut config = Config::default();
        assert!(logging_check(&config).1);
        config.logging.frame_metadata = true;
        assert!(
            logging_check(&config)
                .0
                .contains("frame metadata (no content)")
        );
        config.logging.file = Some(directory.path().join("acpd.log"));
        let (message, passed) = logging_check(&config);
        assert!(passed, "{message}");
        assert!(
            message.contains("at most 16777216 bytes total"),
            "{message}"
        );
        config.logging.file = Some(directory.path().join("absent").join("acpd.log"));
        assert!(!logging_check(&config).1);
        config.logging.file = Some(directory.path().to_owned());
        assert!(!logging_check(&config).1);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
