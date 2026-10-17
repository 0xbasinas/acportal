//! `acpd peer add|list|remove`, run as the real binary against a real far host. What matters
//! here is what the process prints, what it stores, and what it does with a one-use code.
use acpd::{
    api::Host,
    config::{Config, OrchestratorConfig, RuntimeConfig},
    connection::WorkspaceAccess,
    peers::{MAX_PEERS, Peer, PeerAuth, PeerStore},
    registry::Registry,
    security::Role,
    session::SessionManager,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::TcpListener, task::JoinHandle, time::timeout};

/// The other host: serves `/v1/pair` and the bridge.
struct Far {
    host: Host,
    addr: String,
    task: JoinHandle<()>,
    _directory: tempfile::TempDir,
}
impl Far {
    async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let shared = workspace.join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        let config = Config {
            workspace_roots: vec![workspace.clone()],
            state_directory: directory.path().join("state"),
            runtime: RuntimeConfig {
                request_timeout_seconds: 10,
                ..Default::default()
            },
            orchestrator: OrchestratorConfig {
                enabled: true,
                peer_agents: vec!["mock".into()],
                peer_workspaces: BTreeMap::from([("shared".to_owned(), shared)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let registry = Registry::parse(
            &json!([{"id":"mock","name":"Mock","command":env!("CARGO_BIN_EXE_mock-acp-agent"),
                "args":["--delay-ms","1"]}])
            .to_string(),
        )
        .unwrap();
        let host = Host::new(config, registry).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let router = host.router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            host,
            addr,
            task,
            _directory: directory,
        }
    }
    fn code(&self) -> String {
        self.host.security.new_pairing_code_for(Role::Peer).unwrap()
    }
    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
    async fn stop(self) {
        self.task.abort();
        self.host.sessions.shutdown().await;
    }
}

/// This computer's own config and state, for the CLI to run against.
struct Near {
    dir: tempfile::TempDir,
}
impl Near {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "registry = 'agents.json'\nstate_directory = 'state'\nworkspace_roots = ['.']\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("agents.json"), "[]").unwrap();
        Self { dir }
    }
    fn state(&self) -> PathBuf {
        self.dir.path().join("state")
    }
    fn store(&self) -> PeerStore {
        PeerStore::open(&self.state())
    }
    fn command(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_acpd"));
        command
            .arg("--profile")
            .arg(self.dir.path().join("launcher.json"))
            .arg("--config")
            .arg(self.dir.path().join("config.toml"))
            .arg("peer")
            .args(args)
            .stdin(Stdio::null())
            .env_remove("ACPD_PEER_CODE");
        for (key, value) in env {
            command.env(key, value);
        }
        command
    }
    /// Run the CLI off the async runtime: the far host shares it and must keep serving.
    async fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut command = self.command(args, env);
        tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap()
    }
    fn peers_file(&self) -> PathBuf {
        self.state().join("peers.json")
    }
}
fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
fn stored_token(near: &Near, name: &str) -> String {
    match near.store().get(name).unwrap().auth {
        PeerAuth::Bearer { token } => token,
        PeerAuth::Header { value, .. } => value,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adding_a_peer_pairs_with_the_host_and_the_stored_credential_really_works() {
    let far = Far::start().await;
    let near = Near::new();
    let code = far.code();
    let added = near
        .run(
            &[
                "add",
                "--name",
                "far",
                "--address",
                &far.url(),
                "--code-env",
                "ACPD_PEER_CODE",
                "--as",
                "test laptop",
            ],
            &[("ACPD_PEER_CODE", &code)],
        )
        .await;
    assert!(added.status.success(), "{}", text(&added));
    let shown = text(&added);
    let token = stored_token(&near, "far");
    assert_eq!(token.len(), 64);
    assert!(!shown.contains(&token), "the credential is never printed");
    assert!(!shown.contains(&code), "nor is the pairing code");
    assert!(shown.contains(&format!("ws://{}", far.addr)), "{shown}");
    assert!(
        shown.contains("acpd revoke"),
        "it says how to revoke on the other host: {shown}"
    );
    assert!(
        shown.contains("\"transport\":\"ws\""),
        "and shows the registry entry to write: {shown}"
    );

    // The other host recorded a peer-role device under the name given, and the id printed is its.
    let devices = far.host.security.devices().unwrap();
    let device = devices
        .iter()
        .find(|d| d.name == "test laptop")
        .expect("the peer device exists");
    assert_eq!(device.role, Role::Peer);
    assert!(shown.contains(&device.id.to_string()), "{shown}");

    // The file is where the host looks, and `list` shows it without the secret.
    assert!(near.peers_file().exists());
    let listed = near.run(&["list"], &[]).await;
    let list_text = text(&listed);
    assert!(
        listed.status.success() && !list_text.contains(&token),
        "{list_text}"
    );
    let entries: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(entries[0]["name"], "far");
    assert_eq!(entries[0]["auth"], "bearer");
    assert_eq!(entries[0]["deviceId"], device.id.to_string());

    // And it is a real credential: a registry entry naming this peer opens a session over the link.
    let workspace = near.dir.path().join("work");
    std::fs::create_dir_all(&workspace).unwrap();
    let registry = Registry::parse(
        &json!([{"id":"remote","name":"R","transport":"ws",
            "url":format!("ws://{}/acp/mock", far.addr),"peer":"far","workspaceAlias":"shared"}])
        .to_string(),
    )
    .unwrap();
    let manager = SessionManager::new(
        Config {
            workspace_roots: vec![workspace.clone()],
            state_directory: near.state(),
            ..Default::default()
        },
        registry,
    )
    .unwrap();
    let read_only = WorkspaceAccess {
        read_files: true,
        write_files: false,
        terminal: false,
        form_elicitation: false,
        manual_approval: false,
        boolean_options: false,
    };
    let session = manager
        .open_configured("remote", &workspace, None, vec![], read_only)
        .await
        .expect("a peer added by the CLI opens a remote session");
    assert_eq!(session.metadata().remote_peer.as_deref(), Some("far"));
    manager.shutdown().await;

    // A pairing code is single-use: spending it again stores nothing new.
    let again = near
        .run(
            &[
                "add",
                "--name",
                "second",
                "--address",
                &far.url(),
                "--code-env",
                "ACPD_PEER_CODE",
            ],
            &[("ACPD_PEER_CODE", &code)],
        )
        .await;
    assert!(!again.status.success());
    assert!(near.store().get("second").is_err());
    far.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn removing_a_peer_forgets_it_and_says_what_that_does_not_do() {
    let far = Far::start().await;
    let near = Near::new();
    let code = far.code();
    assert!(
        near.run(
            &[
                "add",
                "--name",
                "far",
                "--address",
                &far.url(),
                "--code-env",
                "C"
            ],
            &[("C", &code)]
        )
        .await
        .status
        .success()
    );
    let removed = near.run(&["remove", "far"], &[]).await;
    assert!(removed.status.success());
    assert!(
        text(&removed).contains("revoke it there"),
        "{}",
        text(&removed)
    );
    assert_eq!(near.store().summaries().unwrap().len(), 0);
    let listed = near.run(&["list"], &[]).await;
    assert_eq!(String::from_utf8_lossy(&listed.stdout).trim(), "[]");
    let missing = near.run(&["remove", "far"], &[]).await;
    assert!(!missing.status.success());
    assert!(
        text(&missing).contains("no peer named far"),
        "{}",
        text(&missing)
    );
    far.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bad_input_is_refused_before_the_one_use_code_is_spent() {
    let far = Far::start().await;
    let near = Near::new();
    let code = far.code();
    let env = [("C", code.as_str())];
    // Each of these fails on this side, with the code in hand and the other host reachable, and
    // each for its own reason: a refusal from the far host would not count.
    let url = far.url();
    let cases: [(&str, &str, Vec<&str>); 5] = [
        (
            "a bad name",
            "peer names are",
            vec![
                "add",
                "--name",
                "Bad Name",
                "--address",
                &url,
                "--code-env",
                "C",
            ],
        ),
        (
            "plain ws to another computer",
            "only allowed to this computer",
            vec![
                "add",
                "--name",
                "x",
                "--address",
                "ws://example.com:80",
                "--code-env",
                "C",
            ],
        ),
        (
            "credentials in the address",
            "credentials in a URL",
            vec![
                "add",
                "--name",
                "x",
                "--address",
                "https://u:p@example.com",
                "--code-env",
                "C",
            ],
        ),
        (
            "a fragment in the address",
            "fragment",
            vec![
                "add",
                "--name",
                "x",
                "--address",
                "https://example.com/#f",
                "--code-env",
                "C",
            ],
        ),
        (
            "a malformed code",
            "not a pairing code",
            vec![
                "add",
                "--name",
                "x",
                "--address",
                &url,
                "--code",
                "not a code!",
            ],
        ),
    ];
    for (label, reason, args) in cases {
        let output = near.run(&args, &env).await;
        assert!(!output.status.success(), "{label}: {}", text(&output));
        assert!(
            text(&output).contains(reason),
            "{label} should say {reason:?}: {}",
            text(&output)
        );
        assert!(
            !near.peers_file().exists() || near.store().summaries().unwrap().is_empty(),
            "{label}"
        );
    }
    // A full store is noticed before the code is spent too.
    let store = near.store();
    for index in 0..MAX_PEERS {
        store
            .add(Peer {
                name: format!("p{index}"),
                origin: "wss://filler.example:443".into(),
                auth: PeerAuth::Bearer {
                    token: "a".repeat(64),
                },
                device_id: None,
            })
            .unwrap();
    }
    let full = near
        .run(
            &[
                "add",
                "--name",
                "one-more",
                "--address",
                &far.url(),
                "--code-env",
                "C",
            ],
            &env,
        )
        .await;
    assert!(
        !full.status.success() && text(&full).contains("too many peers"),
        "{}",
        text(&full)
    );
    store.remove("p0").unwrap();
    // The code survived every refusal above: it still pairs.
    let good = near
        .run(
            &[
                "add",
                "--name",
                "one-more",
                "--address",
                &far.url(),
                "--code-env",
                "C",
            ],
            &env,
        )
        .await;
    assert!(
        good.status.success(),
        "the code was wasted by an earlier failure: {}",
        text(&good)
    );
    far.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wrong_code_stores_nothing_and_repeats_nothing_the_server_said() {
    let far = Far::start().await;
    let near = Near::new();
    let _ = far.code(); // a real code exists; this is not it
    let wrong = near
        .run(
            &[
                "add",
                "--name",
                "far",
                "--address",
                &far.url(),
                "--code-env",
                "C",
            ],
            &[("C", "AAAA-BBBB-CCCC")],
        )
        .await;
    assert!(!wrong.status.success());
    let shown = text(&wrong);
    assert!(shown.contains("pairing was refused (HTTP 401)"), "{shown}");
    assert!(
        !shown.contains("pairing_failed"),
        "the server's own text is not repeated: {shown}"
    );
    assert!(near.store().summaries().unwrap().is_empty());
    // Nothing to reach at all is a different, equally quiet failure.
    let nobody = near
        .run(
            &[
                "add",
                "--name",
                "far",
                "--address",
                "http://127.0.0.1:9",
                "--code-env",
                "C",
            ],
            &[("C", "AAAA-BBBB-CCCC")],
        )
        .await;
    assert!(
        !nobody.status.success() && text(&nobody).contains("could not reach"),
        "{}",
        text(&nobody)
    );
    far.stop().await;
}

#[test]
fn a_server_that_is_not_acpd_gets_its_secret_from_the_environment_only() {
    let near = Near::new();
    let secret = "shared-secret-value-8812";
    let run = |args: &[&str], env: &[(&str, &str)]| near.command(args, env).output().unwrap();
    let added = run(
        &[
            "add",
            "--name",
            "goose",
            "--address",
            "ws://127.0.0.1:3284",
            "--header",
            "X-Secret-Key",
            "--secret-env",
            "PEER_SECRET",
        ],
        &[("PEER_SECRET", secret)],
    );
    assert!(added.status.success(), "{}", text(&added));
    assert!(!text(&added).contains(secret));
    assert_eq!(stored_token(&near, "goose"), secret);
    let listed = run(&["list"], &[]);
    assert!(!text(&listed).contains(secret));
    assert!(text(&listed).contains("header X-Secret-Key"));
    // Not set, forbidden, or contradictory: refused, and nothing is stored under those names.
    for (label, args, env) in [
        (
            "the variable is not set",
            vec![
                "add",
                "--name",
                "a",
                "--address",
                "ws://127.0.0.1:1",
                "--header",
                "X-K",
                "--secret-env",
                "UNSET_PEER_SECRET",
            ],
            vec![],
        ),
        (
            "a header that cannot carry a secret",
            vec![
                "add",
                "--name",
                "b",
                "--address",
                "ws://127.0.0.1:1",
                "--header",
                "Host",
                "--secret-env",
                "PEER_SECRET",
            ],
            vec![("PEER_SECRET", secret)],
        ),
        (
            "a code and a header together",
            vec![
                "add",
                "--name",
                "c",
                "--address",
                "ws://127.0.0.1:1",
                "--header",
                "X-K",
                "--secret-env",
                "PEER_SECRET",
                "--code",
                "AAAA-BBBB-CCCC",
            ],
            vec![("PEER_SECRET", secret)],
        ),
        (
            "a header without its variable",
            vec![
                "add",
                "--name",
                "d",
                "--address",
                "ws://127.0.0.1:1",
                "--header",
                "X-K",
            ],
            vec![],
        ),
        (
            "no code and no terminal",
            vec!["add", "--name", "e", "--address", "http://127.0.0.1:1"],
            vec![],
        ),
    ] {
        let output = run(&args, &env);
        assert!(!output.status.success(), "{label}: {}", text(&output));
        assert!(!text(&output).contains(secret), "{label}");
    }
    let names: Vec<String> = near
        .store()
        .summaries()
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, ["goose"]);
}

/// A tiny HTTP server that answers every request with `reply`.
async fn canned_server(reply: Vec<u8>) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let reply = reply.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buffer).await;
                let _ = stream.write_all(&reply).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (addr, task)
}
fn http_reply(status: &str, extra: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_redirect_is_never_followed_and_nothing_is_stored() {
    let elsewhere = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = elsewhere.local_addr().unwrap();
    let (addr, server) = canned_server(http_reply(
        "302 Found",
        &format!("Location: http://{target}/v1/pair\r\n"),
        "",
    ))
    .await;
    let near = Near::new();
    let output = near
        .run(
            &[
                "add",
                "--name",
                "far",
                "--address",
                &format!("http://{addr}"),
                "--code-env",
                "C",
            ],
            &[("C", "AAAA-BBBB-CCCC")],
        )
        .await;
    assert!(!output.status.success());
    assert!(text(&output).contains("HTTP 302"), "{}", text(&output));
    assert!(
        timeout(Duration::from_millis(600), elsewhere.accept())
            .await
            .is_err(),
        "the redirect target was contacted"
    );
    assert!(near.store().summaries().unwrap().is_empty());
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reply_without_a_valid_credential_stores_nothing() {
    let near = Near::new();
    // A perfectly good credential buried in an oversized reply: only the size limit refuses it.
    let oversized = format!(
        "{{\"token\":\"{}\",\"padding\":\"{}\"}}",
        "a".repeat(64),
        "x".repeat(20_000)
    );
    for (label, body) in [
        ("a short token", "{\"token\":\"abc\"}".to_owned()),
        (
            "a non-hex token",
            format!("{{\"token\":\"{}\"}}", "z".repeat(64)),
        ),
        ("no token", "{\"deviceId\":\"x\"}".to_owned()),
        ("not JSON", "welcome".to_owned()),
        ("an oversized reply", oversized),
    ] {
        let (addr, server) = canned_server(http_reply("200 OK", "", &body)).await;
        let output = near
            .run(
                &[
                    "add",
                    "--name",
                    "far",
                    "--address",
                    &format!("http://{addr}"),
                    "--code-env",
                    "C",
                ],
                &[("C", "AAAA-BBBB-CCCC")],
            )
            .await;
        assert!(!output.status.success(), "{label}: {}", text(&output));
        assert!(near.store().summaries().unwrap().is_empty(), "{label}");
        server.abort();
    }
}
