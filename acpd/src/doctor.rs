use crate::config::Config;
use std::{fs::OpenOptions, io::Write, net::TcpListener, path::Path};

/// Operational checks use disposable resources and never open credential stores.
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
    ]
}

fn writable_ancestor(path: &Path) -> bool {
    let mut directory = path;
    loop {
        match std::fs::metadata(directory) {
            Ok(metadata) if metadata.is_dir() => break,
            Ok(_) => return false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return false,
        }
        directory = match directory.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ if directory != Path::new(".") => Path::new("."),
            _ => return false,
        };
    }
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
}
