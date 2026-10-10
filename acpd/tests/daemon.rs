use acpd::{config::Config, daemon, launcher::Launcher, security::SecurityStore};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::{path::PathBuf, process::Command};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

struct Fixture {
    directory: tempfile::TempDir,
    config: PathBuf,
    profile: PathBuf,
    text: String,
}

struct OwnedHost(std::process::Child);
impl Drop for OwnedHost {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Fixture {
    fn foreground(&self) -> OwnedHost {
        use std::process::Stdio;
        let mut host = OwnedHost(
            Command::new(env!("CARGO_BIN_EXE_acpd"))
                .arg("--profile")
                .arg(&self.profile)
                .arg("start")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            assert!(
                host.0.try_wait().unwrap().is_none(),
                "owned host exited before readiness"
            );
            if self.path().exists() && self.run(&["daemon", "status"]).status.success() {
                return host;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "owned host readiness timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

/// Signals only a directly spawned, still-live child, never a PID from a saved record.
#[cfg(unix)]
#[test]
fn foreground_sigterm_releases_control_record_and_listener() {
    let fixture = Fixture::new();
    let mut host = fixture.foreground();
    let pid = i32::try_from(host.0.id()).unwrap();
    assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(status) = host.0.try_wait().unwrap() {
            assert!(status.success(), "SIGTERM bypassed graceful shutdown");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "owned host did not stop"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        !fixture.path().exists(),
        "graceful shutdown left a control record"
    );
    fixture.start();
    assert!(fixture.run(&["daemon", "stop"]).status.success());
}

#[test]
fn wrong_token_oversized_requests_and_stale_pid_cannot_stop_an_unrelated_process() {
    use std::io::{Read, Write};
    let fixture = Fixture::new();
    fixture.start();
    let original = std::fs::read(fixture.path()).unwrap();
    let mut record: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let address = record["address"].as_str().unwrap().to_owned();
    record["token"] = json!("0".repeat(64));
    std::fs::write(fixture.path(), serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(!fixture.run(&["daemon", "stop"]).status.success());
    std::fs::write(fixture.path(), &original).unwrap();
    let mut stream = std::net::TcpStream::connect(&address).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(4)))
        .unwrap();
    stream.write_all(&vec![b'x'; 5000]).unwrap();
    // The host may already have rejected the oversized input and closed the socket.
    if let Err(error) = stream.shutdown(std::net::Shutdown::Write) {
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::NotConnected | std::io::ErrorKind::ConnectionReset
            ),
            "unexpected write shutdown failure: {error}"
        );
    }
    let mut byte = [0u8; 1];
    let result = stream.read(&mut byte);
    assert!(
        matches!(result, Ok(0))
            || matches!(
                result,
                Err(ref error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::NotConnected
                    )
            ),
        "oversized request was accepted"
    );
    assert!(fixture.run(&["daemon", "status"]).status.success());
    assert!(fixture.run(&["daemon", "stop"]).status.success());
    let mut stale: serde_json::Value = serde_json::from_slice(&original).unwrap();
    stale["pid"] = json!(std::process::id());
    std::fs::write(fixture.path(), serde_json::to_vec(&stale).unwrap()).unwrap();
    assert!(!fixture.run(&["daemon", "stop"]).status.success());
    // This runner is still alive; stale PID records are never used to terminate it.
    fixture.start();
    assert!(fixture.run(&["daemon", "stop"]).status.success());
}

#[cfg(windows)]
#[tokio::test]
async fn abrupt_host_exit_stops_both_descendant_generations_and_preserves_credentials() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let fixture = Fixture::new();
    std::fs::write(
        fixture.directory.path().join("agents.json"),
        serde_json::to_vec(&json!([{"id":"mock","name":"Owned mock","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":[],"enabled":true}])).unwrap(),
    ).unwrap();
    let mut host = fixture.foreground();
    let config = Config::load(&fixture.config).unwrap();
    let security = SecurityStore::open(&config.state_directory).unwrap();
    let paired = security
        .redeem(
            &security.new_pairing_code().unwrap(),
            "Owned abrupt fixture",
            3600,
        )
        .unwrap();
    let base = format!("http://{}", config.server.listen);
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/v1/sessions"))
        .bearer_auth(&paired.token)
        .json(&json!({"agentId":"mock","workspace":fixture.directory.path()}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let session: serde_json::Value = response.json().await.unwrap();
    let id = session["id"].as_str().unwrap();
    let mut socket = connect(
        &format!(
            "{}/v1/sessions/{id}/connect?after=0",
            base.replace("http:", "ws:")
        ),
        &paired.token,
    )
    .await;
    receive(&mut socket, |v| v["type"] == "replay_complete").await;
    socket.send(Message::Text(json!({"jsonrpc":"2.0","id":"owned-descendants","method":"_mock/spawn_descendant","params":{}}).to_string().into())).await.unwrap();
    let descendants = receive(&mut socket, |v| {
        v["message"]["id"] == "owned-descendants" && v["message"]["result"].is_object()
    })
    .await;
    let handles: Vec<OwnedHandle> = ["childPid", "grandchildPid"]
        .into_iter()
        .map(|key| {
            let pid = descendants["message"]["result"][key].as_u64().unwrap() as u32;
            let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            assert!(!raw.is_null(), "owned descendant must exist");
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            assert_eq!(
                unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) },
                WAIT_TIMEOUT
            );
            handle
        })
        .collect();
    host.0.kill().unwrap();
    host.0.wait().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while handles
            .iter()
            .any(|h| unsafe { WaitForSingleObject(h.as_raw_handle(), 0) } != WAIT_OBJECT_0)
        {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("kill-on-close job must stop both generations after abrupt host exit");
    drop(socket);
    fixture.start();
    let restored = client
        .get(format!("{base}/v1/sessions/{id}"))
        .bearer_auth(&paired.token)
        .send()
        .await
        .unwrap();
    assert!(
        restored.status().is_success(),
        "restart lost the credential or session record"
    );
    let metadata: serde_json::Value = restored.json().await.unwrap();
    assert_eq!(metadata["status"], "interrupted");
    assert!(fixture.run(&["daemon", "stop"]).status.success());
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config.toml");
        let profile = directory.path().join("launcher.json");
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        let text = format!(
            "registry = 'agents.json'\nstate_directory = 'state'\nworkspace_roots = ['.']\n[server]\nlisten = '{address}'\n"
        );
        std::fs::write(&config, &text).unwrap();
        std::fs::write(directory.path().join("agents.json"), "[]").unwrap();
        Launcher::prepare(&config, "https://fixture.invalid", "Fixture")
            .unwrap()
            .save(&profile)
            .unwrap();
        Self {
            directory,
            config,
            profile,
            text,
        }
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_acpd"))
            .arg("--profile")
            .arg(&self.profile)
            .args(args)
            .output()
            .unwrap()
    }
    fn path(&self) -> PathBuf {
        daemon::record_path(&self.config, &self.profile).unwrap()
    }
    fn start(&self) {
        assert!(
            self.run(&["start", "--background"]).status.success(),
            "fixture daemon startup failed"
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.path().exists() {
            let _ = self.run(&["daemon", "stop"]);
        }
    }
}

#[test]
fn detached_lifecycle_invalid_reload_and_restart_required_are_safe() {
    let fixture = Fixture::new();
    fixture.start();
    let status = fixture.run(&["daemon", "status"]);
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("running"));
    assert!(
        !fixture.run(&["daemon", "start"]).status.success(),
        "duplicate start must fail"
    );
    let record_before = std::fs::read(fixture.path()).unwrap();
    std::fs::write(&fixture.config, "invalid = 'secret-fixture-marker'\n").unwrap();
    let rejected = fixture.run(&["daemon", "reload"]);
    assert!(!rejected.status.success());
    assert!(!String::from_utf8_lossy(&rejected.stderr).contains("secret-fixture-marker"));
    assert!(
        std::fs::read(fixture.path()).unwrap() == record_before,
        "control record changed on rejected reload"
    );
    // Status and stop do not need to parse the now-invalid host config.
    assert!(fixture.run(&["daemon", "status"]).status.success());
    std::fs::write(
        &fixture.config,
        fixture.text.replace(
            "state_directory = 'state'",
            "state_directory = 'other-state'",
        ),
    )
    .unwrap();
    let restart = fixture.run(&["daemon", "reload"]);
    assert!(!restart.status.success());
    assert!(String::from_utf8_lossy(&restart.stderr).contains("restart required"));
    assert!(!fixture.directory.path().join("other-state").exists());
    std::fs::write(&fixture.config, &fixture.text).unwrap();
    assert!(fixture.run(&["daemon", "reload"]).status.success());
    assert!(fixture.run(&["daemon", "stop"]).status.success());
    assert!(!fixture.path().exists());
    fixture.start();
    assert!(fixture.run(&["daemon", "stop"]).status.success());
}

#[tokio::test]
async fn reload_changes_future_discovery_but_preserves_live_session_and_credentials() {
    let fixture = Fixture::new();
    let registry = fixture.directory.path().join("agents.json");
    let definition = json!([{"id":"mock","name":"Before","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":[],"enabled":true}]);
    std::fs::write(&registry, serde_json::to_vec(&definition).unwrap()).unwrap();
    fixture.start();
    let loaded = Config::load(&fixture.config).unwrap();
    let security = SecurityStore::open(&loaded.state_directory).unwrap();
    let code = security.new_pairing_code().unwrap();
    let paired = security.redeem(&code, "Fixture", 3600).unwrap();
    let client = reqwest::Client::new();
    let base = format!("http://{}", loaded.server.listen);
    let create = client
        .post(format!("{base}/v1/sessions"))
        .bearer_auth(&paired.token)
        .json(&json!({"agentId":"mock","workspace":fixture.directory.path()}))
        .send()
        .await
        .unwrap();
    assert!(
        create.status().is_success(),
        "fixture session creation failed"
    );
    let session: serde_json::Value = create.json().await.unwrap();
    let id = session["id"].as_str().unwrap();
    let url = format!(
        "{}/v1/sessions/{id}/connect?after=0",
        base.replace("http:", "ws:")
    );
    let mut socket = connect(&url, &paired.token).await;
    receive(&mut socket, |v| v["type"] == "replay_complete").await;
    socket.send(Message::Text(json!({"jsonrpc":"2.0","id":"reload-live","method":"session/prompt","params":{"sessionId":session["acpSessionId"],"prompt":[{"type":"text","text":"isolated reload permission fixture"}]}}).to_string().into())).await.unwrap();
    let permission = receive(&mut socket, |v| {
        v["message"]["method"] == "session/request_permission"
    })
    .await;
    let mut changed = definition;
    changed[0]["name"] = json!("After");
    changed[0]["enabled"] = json!(false);
    std::fs::write(&registry, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(fixture.run(&["daemon", "reload"]).status.success());
    socket.close(None).await.unwrap();
    drop(socket);
    let mut socket = connect(&url, &paired.token).await;
    let pending = receive(&mut socket, |v| v["type"] == "pending_permission").await;
    assert_eq!(pending["message"]["id"], permission["message"]["id"]);
    receive(&mut socket, |v| v["type"] == "replay_complete").await;
    let agents: serde_json::Value = client
        .get(format!("{base}/v1/agents"))
        .bearer_auth(&paired.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let agent = agents
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "mock")
        .unwrap();
    assert_eq!(agent["name"], "After");
    assert_eq!(agent["enabled"], false);
    let existing = client
        .get(format!("{base}/v1/sessions/{id}"))
        .bearer_auth(&paired.token)
        .send()
        .await
        .unwrap();
    assert!(
        existing.status().is_success(),
        "reload removed a live session or credential"
    );
    let again = client
        .post(format!("{base}/v1/sessions"))
        .bearer_auth(&paired.token)
        .json(&json!({"agentId":"mock","workspace":fixture.directory.path()}))
        .send()
        .await
        .unwrap();
    assert!(
        !again.status().is_success(),
        "disabled agent launched after reload"
    );
    socket.close(None).await.unwrap();
    drop(socket);
    assert!(fixture.run(&["daemon", "stop"]).status.success());
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
async fn connect(url: &str, token: &str) -> Socket {
    for _ in 0..50 {
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse().unwrap());
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "acpd.v1".parse().unwrap());
        match tokio_tungstenite::connect_async(request).await {
            Ok((socket, _)) => return socket,
            Err(tokio_tungstenite::tungstenite::Error::Http(reply))
                if reply.status().as_u16() == 409 =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await
            }
            _ => panic!("fixture WebSocket failed"),
        }
    }
    panic!("fixture controller lease did not release")
}
async fn receive(
    socket: &mut Socket,
    condition: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Message::Text(text) = socket.next().await.unwrap().unwrap() {
                let value = serde_json::from_str(&text).unwrap();
                if condition(&value) {
                    return value;
                }
            }
        }
    })
    .await
    .expect("fixture event timed out")
}
