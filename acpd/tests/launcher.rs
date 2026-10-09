use std::{path::Path, process::Command};

fn fixture(dir: &Path, state: &str) -> std::path::PathBuf {
    let file = dir.join(format!("{state}.toml"));
    std::fs::write(
        &file,
        format!("registry = 'agents.json'\nstate_directory = '{state}'\nworkspace_roots = ['.']\n"),
    )
    .unwrap();
    std::fs::write(dir.join("agents.json"), "[]").unwrap();
    file
}
#[test]
fn native_setup_short_pair_and_explicit_config_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let config = fixture(dir.path(), "first");
    let other = fixture(dir.path(), "second");
    let profile = dir.path().join("launcher.json");
    let original = std::fs::read(&config).unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_acpd"))
            .arg("--profile")
            .arg(&profile)
            .args(args)
            .output()
            .unwrap()
    };
    let setup = invoke(&[
        "--config",
        config.to_str().unwrap(),
        "setup",
        "--address",
        "https://host.example",
        "--name",
        "Laptop",
    ]);
    assert!(setup.status.success(), "setup failed");
    assert_eq!(std::fs::read(&config).unwrap(), original);
    assert!(!dir.path().join("first").exists());
    let config_read = invoke(&["config"]);
    assert!(config_read.status.success());
    assert!(String::from_utf8_lossy(&config_read.stdout).contains("first"));
    let qr = invoke(&["pair"]);
    assert!(qr.status.success(), "short pair failed");
    let text = String::from_utf8_lossy(&qr.stdout);
    assert!(text.contains('█') && text.contains("host.example"));
    let manual = invoke(&["pair", "--manual"]);
    assert!(manual.status.success());
    assert!(!String::from_utf8_lossy(&manual.stdout).contains('█'));
    // QR data is not printed in assertion failures or saved by the fixture.
    let other_pair = invoke(&["--config", other.to_str().unwrap(), "pair"]);
    assert!(other_pair.status.success());
    assert!(!String::from_utf8_lossy(&other_pair.stdout).contains("host.example"));
    assert!(dir.path().join("second").exists());
}
#[test]
fn invalid_setup_preserves_previous_profile_and_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = fixture(dir.path(), "state");
    let profile = dir.path().join("launcher.json");
    acpd::launcher::Launcher::prepare(&config, "https://host.example", "Laptop")
        .unwrap()
        .save(&profile)
        .unwrap();
    let original = std::fs::read(&profile).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_acpd"))
        .arg("--profile")
        .arg(&profile)
        .arg("--config")
        .arg(&config)
        .args(["setup", "--address", "http://host.example"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&profile).unwrap(), original);
    assert!(!dir.path().join("state").exists());
}

#[test]
fn short_start_uses_saved_listener_and_still_requires_authentication() {
    use std::io::{Read, Write};
    let dir = tempfile::tempdir().unwrap();
    let config = fixture(dir.path(), "state");
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    let mut contents = std::fs::read_to_string(&config).unwrap();
    contents.push_str(&format!("[server]\nlisten = '{address}'\n"));
    std::fs::write(&config, &contents).unwrap();
    let profile = dir.path().join("launcher.json");
    acpd::launcher::Launcher::prepare(&config, "https://host.example", "Laptop")
        .unwrap()
        .save(&profile)
        .unwrap();
    drop(reservation);
    struct Owned(std::process::Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Owned(
        Command::new(env!("CARGO_BIN_EXE_acpd"))
            .arg("--profile")
            .arg(&profile)
            .arg("start")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let mut stream = loop {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "fixture host exited before listening"
        );
        if let Ok(stream) = std::net::TcpStream::connect(address) {
            break stream;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fixture listener timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    stream
        .write_all(b"GET /v1/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut reply = [0u8; 8192];
    let count = stream.read(&mut reply).unwrap();
    assert!(String::from_utf8_lossy(&reply[..count]).starts_with("HTTP/1.1 401"));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), contents);
}
