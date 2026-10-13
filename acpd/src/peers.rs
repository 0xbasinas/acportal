//! Peer hosts this host connects to as a client (design sections 7.2 and 7.3).
//!
//! A peer record holds the credential for one other host and the single origin it may be sent
//! to. The registry (which the phone can edit) names a peer but never carries a secret, and a
//! registry URL whose origin differs from the peer's is refused before anything is sent, so a
//! registry entry cannot aim a peer's token at some other server.
//!
//! The file is `peers.json` in the state directory, owner-only like the device tokens, read
//! afresh on every connection so `acpd peer add` and `remove` (another process) take effect at
//! once. Secrets are never printed, logged or returned by `summaries`.
use crate::security::{protect, reject_symlink, write_private_json};
use anyhow::{Context, Result, bail, ensure};
use axum::http::Uri;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_PEERS: usize = 32;
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum PeerAuth {
    /// An acpd peer credential, sent as `Authorization: Bearer`.
    Bearer { token: String },
    /// Any other server's shared secret in a header of its choosing (Goose's `serve` wants
    /// `X-Secret-Key`).
    Header { name: String, value: String },
}
impl std::fmt::Debug for PeerAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bearer { .. } => formatter.write_str("Bearer(<redacted>)"),
            Self::Header { name, .. } => write!(formatter, "Header({name}: <redacted>)"),
        }
    }
}
impl PeerAuth {
    /// The header to put on the WebSocket upgrade request.
    pub fn header(&self) -> (String, String) {
        match self {
            Self::Bearer { token } => ("authorization".into(), format!("Bearer {token}")),
            Self::Header { name, value } => (name.to_ascii_lowercase(), value.clone()),
        }
    }
    fn validate(&self) -> Result<()> {
        match self {
            Self::Bearer { token } => ensure!(printable(token, 4096), "invalid bearer token"),
            Self::Header { name, value } => {
                ensure!(
                    !name.is_empty()
                        && name.len() <= 64
                        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                    "invalid header name"
                );
                // Headers that would change the meaning of the handshake itself.
                let lower = name.to_ascii_lowercase();
                ensure!(
                    !matches!(
                        lower.as_str(),
                        "host" | "upgrade" | "connection" | "content-length" | "transfer-encoding"
                    ) && !lower.starts_with("sec-websocket-"),
                    "that header cannot carry a credential"
                );
                ensure!(printable(value, 4096), "invalid header value");
            }
        }
        Ok(())
    }
}
fn printable(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text == text.trim()
        && text.chars().all(|c| !c.is_control())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Peer {
    pub name: String,
    /// `wss://host:port` (or `ws://` to a loopback address). The only place this credential goes.
    pub origin: String,
    pub auth: PeerAuth,
    /// This host's device id on the other host, when it was paired with a code: what that host's
    /// operator revokes (`acpd revoke <id>`). Not a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<uuid::Uuid>,
}
impl std::fmt::Debug for Peer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Peer")
            .field("name", &self.name)
            .field("origin", &self.origin)
            .field("auth", &self.auth)
            .field("device_id", &self.device_id)
            .finish()
    }
}

/// What may be shown about a peer: never the secret.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PeerSummary {
    pub name: String,
    pub origin: String,
    pub auth: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<uuid::Uuid>,
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// The `scheme://host:port` a URL points at, normalized (`https` and `http` become `wss` and
/// `ws`, the host is lowercased, the default port is made explicit). Plain `ws` is accepted only
/// for loopback hosts; credentials in the URL and fragments are refused.
pub fn origin_of(url: &str) -> Result<String> {
    let uri: Uri = url.parse().context("not a valid URL")?;
    let scheme = match uri.scheme_str() {
        Some("wss" | "https") => "wss",
        Some("ws" | "http") => "ws",
        _ => bail!("the URL must use wss:// (or ws:// to this computer)"),
    };
    let authority = uri.authority().context("the URL has no host")?;
    ensure!(
        !authority.as_str().contains('@'),
        "credentials in a URL are not accepted"
    );
    ensure!(!url.contains('#'), "a URL fragment is not accepted");
    let host = authority.host().to_ascii_lowercase();
    ensure!(!host.is_empty(), "the URL has no host");
    ensure!(
        scheme == "wss" || loopback(&host),
        "unencrypted ws:// is only allowed to this computer"
    );
    let port = authority
        .port_u16()
        .unwrap_or(if scheme == "wss" { 443 } else { 80 });
    Ok(format!("{scheme}://{host}:{port}"))
}

/// Peer records, kept in `peers.json`.
#[derive(Clone)]
pub struct PeerStore {
    directory: PathBuf,
}
impl PeerStore {
    pub fn open(directory: &Path) -> Self {
        Self {
            directory: directory.to_path_buf(),
        }
    }
    fn file(&self) -> PathBuf {
        self.directory.join("peers.json")
    }
    fn load(&self) -> Result<Vec<Peer>> {
        let path = self.file();
        if !path.exists() {
            return Ok(vec![]);
        }
        reject_symlink(&path)?;
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .context("open the peers file")?
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_FILE_BYTES,
            "the peers file is too large"
        );
        let peers: Vec<Peer> =
            serde_json::from_slice(&bytes).context("the peers file is not valid")?;
        ensure!(peers.len() <= MAX_PEERS, "too many peers");
        Ok(peers)
    }
    /// Every change takes this lock, so two `acpd peer` commands cannot lose each other's edit.
    fn locked<T>(&self, change: impl FnOnce(&mut Vec<Peer>) -> Result<T>) -> Result<T> {
        std::fs::create_dir_all(&self.directory)?;
        let lock_path = self.directory.join("peers.lock");
        reject_symlink(&lock_path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        protect(&lock_path, false)?;
        lock.lock_exclusive()?;
        let mut peers = self.load()?;
        let result = change(&mut peers)?;
        write_private_json(&self.file(), &peers)?;
        Ok(result)
    }
    /// Everything `add` would refuse, without writing. Used before a one-use pairing code is
    /// spent, so a failure that could have been seen earlier does not waste it.
    pub fn check_can_add(&self, peer: &Peer) -> Result<()> {
        ensure!(
            valid_name(&peer.name),
            "peer names are 1..32 of a-z, 0-9, '-' or '_'"
        );
        ensure!(
            origin_of(&peer.origin)? == peer.origin,
            "the peer origin must be exactly scheme://host:port"
        );
        peer.auth.validate()?;
        let existing = self.load()?;
        ensure!(
            existing.len() < MAX_PEERS || existing.iter().any(|p| p.name == peer.name),
            "too many peers; remove one first"
        );
        Ok(())
    }
    pub fn add(&self, peer: Peer) -> Result<()> {
        self.check_can_add(&peer)?;
        self.locked(|peers| {
            peers.retain(|existing| existing.name != peer.name);
            ensure!(peers.len() < MAX_PEERS, "too many peers; remove one first");
            peers.push(peer);
            Ok(())
        })
    }
    pub fn remove(&self, name: &str) -> Result<bool> {
        self.locked(|peers| {
            let before = peers.len();
            peers.retain(|peer| peer.name != name);
            Ok(peers.len() != before)
        })
    }
    pub fn get(&self, name: &str) -> Result<Peer> {
        self.load()?
            .into_iter()
            .find(|peer| peer.name == name)
            .with_context(|| format!("no peer named {name}"))
    }
    pub fn summaries(&self) -> Result<Vec<PeerSummary>> {
        Ok(self
            .load()?
            .into_iter()
            .map(|peer| PeerSummary {
                name: peer.name,
                origin: peer.origin,
                auth: match peer.auth {
                    PeerAuth::Bearer { .. } => "bearer".into(),
                    PeerAuth::Header { name, .. } => format!("header {name}"),
                },
                device_id: peer.device_id,
            })
            .collect())
    }
    /// The peer and the credential header for connecting to `url`, only if `url` points at the
    /// peer's own origin.
    pub fn credential_for(&self, name: &str, url: &str) -> Result<(Peer, (String, String))> {
        let peer = self.get(name)?;
        ensure!(
            origin_of(url)? == peer.origin,
            "that address is not this peer's; its credential is not sent"
        );
        let header = peer.auth.header();
        Ok((peer, header))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bearer(name: &str, origin: &str) -> Peer {
        Peer {
            device_id: None,
            name: name.into(),
            origin: origin.into(),
            auth: PeerAuth::Bearer {
                token: "a".repeat(64),
            },
        }
    }

    #[test]
    fn origins_are_normalized_and_unsafe_ones_refused() {
        for (input, expected) in [
            (
                "wss://Laptop.Example:8765/acp/goose",
                "wss://laptop.example:8765",
            ),
            ("https://laptop.example", "wss://laptop.example:443"),
            ("wss://laptop.example/", "wss://laptop.example:443"),
            ("ws://127.0.0.1:9000/acp/x", "ws://127.0.0.1:9000"),
            ("http://localhost:9000", "ws://localhost:9000"),
        ] {
            assert_eq!(origin_of(input).unwrap(), expected, "{input}");
        }
        for bad in [
            "ws://laptop.example:8765",
            "http://192.168.1.5:8765",
            "ftp://laptop.example",
            "wss://user:pass@laptop.example",
            "wss://laptop.example/#frag",
            "laptop.example",
            "",
            "wss://",
        ] {
            assert!(origin_of(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn peers_are_stored_replaced_listed_without_secrets_and_removed() {
        let directory = tempfile::tempdir().unwrap();
        let store = PeerStore::open(directory.path());
        assert!(store.summaries().unwrap().is_empty());
        store
            .add(bearer("laptop", "wss://laptop.example:8765"))
            .unwrap();
        store
            .add(Peer {
                device_id: None,
                name: "goose".into(),
                origin: "ws://127.0.0.1:3284".into(),
                auth: PeerAuth::Header {
                    name: "X-Secret-Key".into(),
                    value: "very-secret-value".into(),
                },
            })
            .unwrap();
        // Same name replaces; the list is a summary with no secret in it.
        store
            .add(bearer("laptop", "wss://other.example:8765"))
            .unwrap();
        let summaries = store.summaries().unwrap();
        assert_eq!(summaries.len(), 2);
        let shown = format!("{summaries:?} {:?}", store.get("goose").unwrap());
        assert!(
            !shown.contains("very-secret-value") && !shown.contains(&"a".repeat(64)),
            "{shown}"
        );
        assert!(
            summaries
                .iter()
                .any(|s| s.name == "goose" && s.auth == "header X-Secret-Key")
        );
        assert_eq!(
            store.get("laptop").unwrap().origin,
            "wss://other.example:8765"
        );
        assert!(store.remove("laptop").unwrap());
        assert!(!store.remove("laptop").unwrap());
        assert!(store.get("laptop").is_err());
    }

    #[test]
    fn invalid_peers_and_credentials_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let store = PeerStore::open(directory.path());
        for name in ["", "Has Space", "UPPER", &"x".repeat(33), "a/b"] {
            assert!(
                store
                    .add(bearer(name, "wss://laptop.example:8765"))
                    .is_err(),
                "{name:?}"
            );
        }
        // An origin must already be in normal form.
        assert!(store.add(bearer("laptop", "wss://laptop.example")).is_err());
        assert!(
            store
                .add(bearer("laptop", "ws://laptop.example:80"))
                .is_err()
        );
        for (name, value) in [
            ("Host", "x"),
            ("Upgrade", "x"),
            ("Sec-WebSocket-Key", "x"),
            ("Content-Length", "1"),
            ("Bad Name", "x"),
            ("", "x"),
            ("X-Key", ""),
            ("X-Key", " padded "),
            ("X-Key", "line\nbreak"),
        ] {
            let peer = Peer {
                device_id: None,
                name: "p".into(),
                origin: "wss://laptop.example:8765".into(),
                auth: PeerAuth::Header {
                    name: name.into(),
                    value: value.into(),
                },
            };
            assert!(store.add(peer).is_err(), "{name:?}: {value:?}");
        }
        assert!(
            store.summaries().unwrap().is_empty(),
            "nothing partial was saved"
        );
    }

    #[test]
    fn a_credential_goes_only_to_its_own_origin() {
        let directory = tempfile::tempdir().unwrap();
        let store = PeerStore::open(directory.path());
        store
            .add(bearer("laptop", "wss://laptop.example:8765"))
            .unwrap();
        let (_, header) = store
            .credential_for("laptop", "wss://LAPTOP.example:8765/acp/goose")
            .unwrap();
        assert_eq!(header.0, "authorization");
        assert!(header.1.starts_with("Bearer "));
        for elsewhere in [
            "wss://attacker.example:8765/acp/goose",
            "wss://laptop.example:9999/acp/goose",
            "wss://laptop.example/acp/goose",
            "ws://127.0.0.1:8765/acp/goose",
        ] {
            let refused = store
                .credential_for("laptop", elsewhere)
                .unwrap_err()
                .to_string();
            assert!(
                refused.contains("not this peer's"),
                "{elsewhere}: {refused}"
            );
        }
        assert!(
            store
                .credential_for("nobody", "wss://laptop.example:8765/x")
                .is_err()
        );
    }

    #[test]
    fn the_file_is_private_bounded_and_strictly_parsed() {
        let directory = tempfile::tempdir().unwrap();
        let store = PeerStore::open(directory.path());
        store
            .add(bearer("laptop", "wss://laptop.example:8765"))
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(directory.path().join("peers.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "owner only, got {mode:o}");
        }
        let path = directory.path().join("peers.json");
        std::fs::write(&path, br#"[{"name":"x","origin":"wss://a:1","auth":{"kind":"bearer","token":"t"},"extra":1}]"#).unwrap();
        assert!(store.summaries().is_err(), "unknown fields are refused");
        std::fs::write(&path, vec![b' '; (MAX_FILE_BYTES + 10) as usize]).unwrap();
        assert!(
            store
                .summaries()
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
        std::fs::write(&path, b"not json").unwrap();
        assert!(store.get("x").is_err());
    }

    #[test]
    fn the_limit_on_peers_holds() {
        let directory = tempfile::tempdir().unwrap();
        let store = PeerStore::open(directory.path());
        for index in 0..MAX_PEERS {
            store
                .add(bearer(&format!("p{index}"), "wss://laptop.example:8765"))
                .unwrap();
        }
        assert!(
            store
                .add(bearer("one-too-many", "wss://laptop.example:8765"))
                .is_err()
        );
        // Replacing an existing name is still fine at the limit.
        store
            .add(bearer("p0", "wss://laptop.example:8765"))
            .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_peers_file_is_not_followed() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("elsewhere.json");
        std::fs::write(&target, "[]").unwrap();
        std::os::unix::fs::symlink(&target, directory.path().join("peers.json")).unwrap();
        let store = PeerStore::open(directory.path());
        assert!(store.summaries().is_err());
        assert!(
            store
                .add(bearer("laptop", "wss://laptop.example:8765"))
                .is_err()
        );
    }
}
