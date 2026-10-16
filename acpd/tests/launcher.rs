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
    // The port can accept a connection a moment before the server answers on it (a reset on a
    // busy runner), so retry the whole request until the deadline.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let (reply, count) = loop {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "fixture host exited before listening"
        );
        let attempt = (|| -> std::io::Result<([u8; 8192], usize)> {
            let mut stream = std::net::TcpStream::connect(address)?;
            stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
            stream.write_all(
                b"GET /v1/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )?;
            let mut reply = [0u8; 8192];
            let count = stream.read(&mut reply)?;
            Ok((reply, count))
        })();
        match attempt {
            Ok((reply, count)) if count > 0 => break (reply, count),
            _ => assert!(
                std::time::Instant::now() < deadline,
                "fixture listener timed out: {attempt:?}"
            ),
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert!(String::from_utf8_lossy(&reply[..count]).starts_with("HTTP/1.1 401"));
    assert_eq!(std::fs::read_to_string(&config).unwrap(), contents);
}

#[test]
fn bare_setup_creates_the_standard_per_user_files_and_keeps_state_in_the_data_folder() {
    let home = tempfile::tempdir().unwrap();
    let config_home = home.path().join("config-home");
    let data_home = home.path().join("data-home");
    let work = home.path().join("run-here");
    std::fs::create_dir(&work).unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_acpd"))
            .current_dir(&work)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .args(args)
            .output()
            .unwrap()
    };
    let setup = invoke(&[
        "setup",
        "--address",
        "https://host.example",
        "--name",
        "Laptop",
    ]);
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let settings = config_home.join("acpd");
    assert!(settings.join("config.toml").is_file());
    assert_eq!(
        std::fs::read_to_string(settings.join("agents.json"))
            .unwrap()
            .trim(),
        "[]"
    );
    assert!(settings.join("launcher.json").is_file());
    // Nothing is written next to where the command was run.
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    // The effective state folder is in the data folder, and pairing creates it there.
    let config = invoke(&["config"]);
    assert!(config.status.success());
    let shown = String::from_utf8_lossy(&config.stdout).replace('\\', "/");
    assert!(
        shown.contains(
            &data_home
                .join("acpd")
                .join("state")
                .to_string_lossy()
                .replace('\\', "/")
        ),
        "{shown}"
    );
    let pair = invoke(&["pair", "--manual"]);
    assert!(
        pair.status.success(),
        "{}",
        String::from_utf8_lossy(&pair.stderr)
    );
    assert!(data_home.join("acpd").join("state").is_dir());
    assert!(!settings.join("state").exists());
    // Running setup again keeps edits.
    std::fs::write(settings.join("config.toml"), "# edited by the user\n").unwrap();
    assert!(
        invoke(&["setup", "--address", "https://other.example"])
            .status
            .success()
    );
    assert_eq!(
        std::fs::read_to_string(settings.join("config.toml")).unwrap(),
        "# edited by the user\n"
    );
}
