//! The outbound half of the bridge: this host acting as a client of another host's `/acp`.
//! Two real hosts in one process (the far one serves the bridge over loopback and runs the mock
//! agent), a TCP proxy that can be cut to simulate a lost link, and a few deliberately
//! misbehaving peers.
use acpd::{
    api::Host,
    config::{Config, OrchestratorConfig, RuntimeConfig},
    connection::WorkspaceAccess,
    orchestrator::{
        Orchestrator,
        control::{self, ControlBackend, Endpoint, Stream},
        delegate::Backend,
    },
    peers::{Peer, PeerAuth, PeerStore},
    registry::Registry,
    security::Role,
    session::{AcpSession, SessionManager},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    future::Future,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    task::{AbortHandle, JoinHandle},
    time::timeout,
};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

const READ_ONLY: WorkspaceAccess = WorkspaceAccess {
    read_files: true,
    write_files: false,
    terminal: false,
    form_elicitation: false,
};

async fn eventually<T, F, Fut>(mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    timeout(Duration::from_secs(20), async {
        loop {
            if let Some(value) = check().await {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("condition not reached in time")
}

/// The other host: it serves the bridge and runs the mock agent.
struct Far {
    host: Host,
    addr: String,
    client: reqwest::Client,
    task: JoinHandle<()>,
    shared: PathBuf,
    _directory: tempfile::TempDir,
}
impl Far {
    async fn start() -> Self {
        Self::start_with(|_| {}).await
    }
    async fn start_with(tweak: impl FnOnce(&mut Config)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let shared = workspace.join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        let mut config = Config {
            workspace_roots: vec![workspace.clone()],
            state_directory: directory.path().join("state"),
            runtime: RuntimeConfig {
                request_timeout_seconds: 10,
                max_sessions: 8,
                ..Default::default()
            },
            orchestrator: OrchestratorConfig {
                enabled: true,
                peer_agents: vec!["mock".into()],
                peer_workspaces: BTreeMap::from([("shared".to_owned(), shared.clone())]),
                bridge_ping_seconds: 5,
                ..Default::default()
            },
            ..Default::default()
        };
        tweak(&mut config);
        let registry = Registry::parse(
            &json!([{"id":"mock","name":"Mock","command":env!("CARGO_BIN_EXE_mock-acp-agent"),
                "args":["--delay-ms","1"]}])
            .to_string(),
        )
        .unwrap();
        let host = Host::new(config, registry).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let router = host.router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            host,
            addr,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            task,
            shared,
            _directory: directory,
        }
    }
    /// Pair this host's operator-made peer code, as the other host's `acpd peer add` would.
    async fn pair_peer(&self) -> (String, Uuid) {
        let code = self.host.security.new_pairing_code_for(Role::Peer).unwrap();
        let reply: Value = self
            .client
            .post(format!("http://{}/v1/pair", self.addr))
            .json(&json!({"code": code, "deviceName": "near"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        (
            reply["token"].as_str().unwrap().to_owned(),
            reply["deviceId"].as_str().unwrap().parse().unwrap(),
        )
    }
    async fn stop(self) {
        self.task.abort();
        self.host.sessions.shutdown().await;
    }
}

/// This host: the client side.
struct Near {
    manager: Arc<SessionManager>,
    orchestrator: Arc<Orchestrator>,
    workspace: PathBuf,
    _endpoint: Option<Endpoint>,
    _directory: tempfile::TempDir,
}
impl Near {
    /// `url` is where the `remote` agent points; the peer record `b` binds `token` to `origin`.
    fn start(url: &str, origin: &str, token: &str, tweak: impl FnOnce(&mut Config)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        std::fs::create_dir_all(workspace.join("sub")).unwrap();
        let state = directory.path().join("state");
        PeerStore::open(&state)
            .add(Peer {
                device_id: None,
                name: "b".into(),
                origin: origin.into(),
                auth: PeerAuth::Bearer {
                    token: token.into(),
                },
            })
            .unwrap();
        let mut config = Config {
            workspace_roots: vec![workspace.clone()],
            state_directory: state,
            runtime: RuntimeConfig {
                request_timeout_seconds: 10,
                max_sessions: 8,
                ..Default::default()
            },
            orchestrator: OrchestratorConfig {
                enabled: true,
                orchestrate: vec!["test-agent".into()],
                delegable: vec!["remote".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        tweak(&mut config);
        let agents = json!([
            {"id":"remote","name":"Remote mock","transport":"ws","url":url,"peer":"b","workspaceAlias":"shared"},
            {"id":"test-agent","name":"Local","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--echo-mcp"]},
        ]);
        let registry = Registry::parse(&agents.to_string()).unwrap();
        let manager = Arc::new(SessionManager::new(config.clone(), registry).unwrap());
        let orchestrator =
            Orchestrator::with_cancel_grace(Arc::downgrade(&manager), Duration::from_millis(500));
        manager.attach_orchestrator(orchestrator.clone());
        let endpoint =
            control::start(orchestrator.clone(), &config.state_directory.join("..")).ok();
        Self {
            manager,
            orchestrator,
            workspace,
            _endpoint: endpoint,
            _directory: directory,
        }
    }
    async fn open_remote(&self) -> anyhow::Result<Arc<AcpSession>> {
        self.manager
            .open_configured("remote", &self.workspace, None, vec![], READ_ONLY)
            .await
    }
}

fn origin_of(addr: &str) -> String {
    format!("ws://{addr}")
}
fn bridge_url(addr: &str) -> String {
    format!("ws://{addr}/acp/mock")
}
fn agent_text(session: &AcpSession) -> String {
    session
        .replay(0)
        .events
        .iter()
        .filter(|event| event.message["method"] == "session/update")
        .filter_map(|event| {
            let update = &event.message["params"]["update"];
            (update["sessionUpdate"] == "agent_message_chunk")
                .then(|| update["content"]["text"].as_str())
                .flatten()
        })
        .collect()
}
async fn first_pending(session: &AcpSession) -> acpd::connection::PendingPermission {
    eventually(|| async { session.connection.pending_permissions().into_iter().next() }).await
}
async fn allow(session: &AcpSession, request: &Value) {
    session
        .connection
        .permission(
            request["id"].clone(),
            json!({"outcome":"selected","optionId":"allow"}),
        )
        .await
        .unwrap();
}

/// A TCP forwarder that can be cut.
struct Proxy {
    port: u16,
    accepted: Arc<AtomicUsize>,
    tasks: Arc<Mutex<Vec<AbortHandle>>>,
    accept: JoinHandle<()>,
    /// While set, a new connection is accepted (and counted) and dropped at once.
    refusing: Arc<std::sync::atomic::AtomicBool>,
    /// While set, what the dialer sends is swallowed (the dialer's writes still succeed) but what
    /// the far side sends still comes through: a line written just as a link dies.
    swallowing: Arc<std::sync::atomic::AtomicBool>,
}
impl Proxy {
    async fn start(target: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = Arc::new(AtomicUsize::new(0));
        let tasks: Arc<Mutex<Vec<AbortHandle>>> = Arc::default();
        let refusing: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let swallowing: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let (count, handles, refuse, swallow) = (
            accepted.clone(),
            tasks.clone(),
            refusing.clone(),
            swallowing.clone(),
        );
        let accept = tokio::spawn(async move {
            while let Ok((mut inbound, _)) = listener.accept().await {
                count.fetch_add(1, Ordering::SeqCst);
                if refuse.load(Ordering::SeqCst) {
                    continue; // dropped: the dialer sees a connection that ends at once
                }
                let target = target.clone();
                let swallow = swallow.clone();
                let task = tokio::spawn(async move {
                    if let Ok(mut outbound) = TcpStream::connect(&target).await {
                        use tokio::io::AsyncWriteExt;
                        let (mut from_dialer, mut to_dialer) = inbound.split();
                        let (mut from_far, mut to_far) = outbound.split();
                        let up = async {
                            let mut buffer = [0u8; 8192];
                            loop {
                                let n = from_dialer.read(&mut buffer).await?;
                                if n == 0 {
                                    return Ok::<(), std::io::Error>(());
                                }
                                if !swallow.load(Ordering::SeqCst) {
                                    to_far.write_all(&buffer[..n]).await?;
                                }
                            }
                        };
                        let down = tokio::io::copy(&mut from_far, &mut to_dialer);
                        tokio::select! { _ = up => {}, _ = down => {} }
                    }
                });
                handles.lock().unwrap().push(task.abort_handle());
            }
        });
        Self {
            port,
            accepted,
            tasks,
            accept,
            refusing,
            swallowing,
        }
    }
    fn swallow(&self, on: bool) {
        self.swallowing.store(on, Ordering::SeqCst);
    }
    /// Cut every connection now; new ones still get through (unless refused).
    fn sever(&self) {
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
    }
    fn refuse(&self, on: bool) {
        self.refusing.store(on, Ordering::SeqCst);
    }
    fn kill(&self) {
        self.accept.abort();
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
    }
}

#[tokio::test]
async fn a_remote_agent_runs_on_the_other_host_and_its_permission_reaches_this_one() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let near = Near::start(
        &bridge_url(&far.addr),
        &origin_of(&far.addr),
        &token,
        |_| {},
    );
    let session = near.open_remote().await.expect("the remote session opens");
    let metadata = session.metadata();
    assert_eq!(metadata.remote_peer.as_deref(), Some("b"));
    assert_eq!(metadata.initialization["agentInfo"]["name"], "acpd");
    assert!(
        Uuid::parse_str(&metadata.acp_session_id).is_ok(),
        "the id is the other host's bridge id"
    );
    assert_eq!(far.host.sessions.active_count().await, 1);
    // It really runs there, in the folder the alias names, read-only.
    let remote = far
        .host
        .sessions
        .get(metadata.acp_session_id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        remote.metadata().workspace,
        far.shared.canonicalize().unwrap()
    );
    assert_eq!(
        remote
            .connection
            .request("_mock/client_capabilities", json!({}))
            .await
            .unwrap(),
        json!({"fs":{"readTextFile":true,"writeTextFile":false},"terminal":false})
    );

    let prompting = session.clone();
    let prompt = tokio::spawn(async move { prompting.prompt("go").await });
    let request = first_pending(&session).await;
    assert!(
        !request.host_origin,
        "to this host it is an agent's request"
    );
    assert_eq!(
        request.message["params"]["sessionId"],
        metadata.acp_session_id.as_str()
    );
    allow(&session, &request.message).await;
    let done = prompt.await.unwrap().expect("the turn finishes");
    assert_eq!(done["stopReason"], "end_turn");
    assert!(agent_text(&session).contains("Mock lifecycle complete"));
    far.stop().await;
}

#[tokio::test]
async fn a_credential_is_never_sent_anywhere_but_its_own_origin() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    // A listener standing in for an attacker's server: any connection to it is a failure.
    let attacker = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let elsewhere = format!("ws://{}/acp/mock", attacker.local_addr().unwrap());
    let near = Near::start(&elsewhere, &origin_of(&far.addr), &token, |_| {});
    let refused = near
        .open_remote()
        .await
        .err()
        .expect("the open is refused")
        .to_string();
    assert!(refused.contains("not this peer's"), "{refused}");
    assert!(!refused.contains(&token));
    assert!(
        timeout(Duration::from_millis(600), attacker.accept())
            .await
            .is_err(),
        "the attacker's server was never even contacted"
    );
    assert_eq!(near.manager.active_count().await, 0);
    far.stop().await;
}

#[tokio::test]
async fn an_encrypted_peer_is_never_downgraded_to_plaintext() {
    // A plaintext server where a wss:// peer is expected: the client must speak TLS to it (and
    // so fail), not fall back to sending the credential in the clear.
    let plain = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = plain.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::<u8>::new()));
    let capture = seen.clone();
    let server = tokio::spawn(async move {
        while let Ok((mut stream, _)) = plain.accept().await {
            let mut buffer = vec![0u8; 4096];
            if let Ok(Ok(count)) = timeout(Duration::from_secs(2), stream.read(&mut buffer)).await {
                capture.lock().unwrap().extend_from_slice(&buffer[..count]);
            }
        }
    });
    // The credential is bound to wss://127.0.0.1:<port>, which the peer store accepts only for
    // plain ws on loopback; use a name that exercises the wss path through the registry URL.
    let secret = "s3cret-token-value-0123456789abcdef0123456789abcdef0123456789abcdef";
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join("state");
    let store = PeerStore::open(&state);
    store
        .add(Peer {
            device_id: None,
            name: "b".into(),
            origin: format!("wss://localhost:{port}"),
            auth: PeerAuth::Bearer {
                token: secret.into(),
            },
        })
        .unwrap();
    let result = acpd::peer_link::connect(
        &store,
        "b",
        &format!("wss://localhost:{port}/acp/mock"),
        1 << 20,
        acpd::peer_link::Resume::OFF,
    )
    .await;
    assert!(
        result.is_err(),
        "a TLS handshake with a plaintext server cannot succeed"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    let captured = seen.lock().unwrap().clone();
    assert!(!captured.is_empty(), "something was sent: a TLS hello");
    assert_eq!(
        captured[0], 0x16,
        "it starts like a TLS handshake record, not an HTTP request"
    );
    let text = String::from_utf8_lossy(&captured);
    assert!(!text.contains(secret) && !text.to_ascii_lowercase().contains("authorization"));
    server.abort();
}

#[tokio::test]
async fn the_other_hosts_rights_are_a_ceiling_and_this_host_may_ask_for_less() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let near = Near::start(
        &bridge_url(&far.addr),
        &origin_of(&far.addr),
        &token,
        |_| {},
    );
    // `create` asks for everything; the other host only grants read access to peers.
    assert!(
        near.manager
            .create("remote", &near.workspace)
            .await
            .is_err()
    );
    assert_eq!(
        far.host.sessions.active_count().await,
        0,
        "no session was left behind"
    );
    let none = WorkspaceAccess {
        read_files: false,
        write_files: false,
        terminal: false,
        form_elicitation: false,
    };
    let session = near
        .manager
        .open_configured("remote", &near.workspace, None, vec![], none)
        .await
        .expect("asking for less is fine");
    let remote = far
        .host
        .sessions
        .get(session.metadata().acp_session_id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        remote
            .connection
            .request("_mock/client_capabilities", json!({}))
            .await
            .unwrap()["fs"]["readTextFile"],
        false
    );
    far.stop().await;
}

#[tokio::test]
async fn with_reconnecting_off_losing_the_link_ends_the_session_and_nothing_is_resent() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let proxy = Proxy::start(far.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        &token,
        |config| {
            config.orchestrator.peer_reconnect_seconds = 0;
        },
    );
    let session = near.open_remote().await.unwrap();
    let prompting = session.clone();
    let prompt = tokio::spawn(async move { prompting.prompt("go").await });
    first_pending(&session).await;

    proxy.kill();
    eventually(|| async { session.connection.closed_reason() }).await;
    assert!(prompt.await.unwrap().is_err(), "the turn in flight fails");
    assert_eq!(session.metadata().status, "exited");
    // This host does not reconnect and replay on its own, and the other host keeps the session
    // for the resume window instead of dropping a turn's work on the floor.
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(
        proxy.accepted.load(Ordering::SeqCst),
        1,
        "no automatic reconnect"
    );
    assert_eq!(far.host.sessions.active_count().await, 1);
    far.stop().await;
}

#[tokio::test]
async fn revoking_this_host_on_the_other_one_ends_the_session_and_blocks_new_ones() {
    let far = Far::start().await;
    let (token, device) = far.pair_peer().await;
    let near = Near::start(
        &bridge_url(&far.addr),
        &origin_of(&far.addr),
        &token,
        |_| {},
    );
    let session = near.open_remote().await.unwrap();
    far.host.security.revoke(device).unwrap();
    // The other host notices at its next heartbeat (5 s here) and closes the socket.
    eventually(|| async { session.connection.closed_reason() }).await;
    let refused = near
        .open_remote()
        .await
        .err()
        .expect("the open is refused")
        .to_string();
    assert!(refused.contains("HTTP 401"), "{refused}");
    far.stop().await;
}

#[tokio::test]
async fn remote_agents_take_no_mcp_servers_and_cannot_load_saved_sessions() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let near = Near::start(
        &bridge_url(&far.addr),
        &origin_of(&far.addr),
        &token,
        |_| {},
    );
    let stdio =
        json!({"name":"x","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":[],"env":[]});
    // The refusal is this host's own, said before anything is sent: the other host would have
    // refused it too, and a test that cannot tell the difference proves nothing.
    for refused in [
        near.manager
            .open_with_mcp("remote", &near.workspace, None, vec![stdio])
            .await
            .err(),
        near.manager
            .load("remote", &near.workspace, "some-saved-id")
            .await
            .err(),
    ] {
        let refused = refused.expect("refused").to_string();
        assert!(refused.contains("do not take MCP servers"), "{refused}");
    }
    assert_eq!(
        far.host.sessions.active_count().await,
        0,
        "the other host was never contacted"
    );
    far.stop().await;
}

#[tokio::test]
async fn removing_a_remote_session_ends_it_on_the_other_host_at_once() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let near = Near::start(
        &bridge_url(&far.addr),
        &origin_of(&far.addr),
        &token,
        |_| {},
    );
    let session = near.open_remote().await.unwrap();
    assert_eq!(far.host.sessions.active_count().await, 1);
    let started = Instant::now();
    near.manager.remove(session.metadata().id).await.unwrap();
    eventually(|| async { (far.host.sessions.active_count().await == 0).then_some(()) }).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "not kept for the 120-second resume window: {:?}",
        started.elapsed()
    );
    far.stop().await;
}

/// A peer that records what it is sent (frames and handshake headers) and answers from a script.
struct FakePeer {
    addr: String,
    frames: Arc<Mutex<Vec<Value>>>,
    headers: Arc<Mutex<Vec<(String, String)>>>,
    task: JoinHandle<()>,
}
impl FakePeer {
    async fn start(script: impl Fn(&Value) -> Vec<String> + Send + Sync + 'static) -> Self {
        use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let frames: Arc<Mutex<Vec<Value>>> = Arc::default();
        let headers: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
        let script = Arc::new(script);
        let (seen, seen_headers) = (frames.clone(), headers.clone());
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (seen, seen_headers, script) =
                    (seen.clone(), seen_headers.clone(), script.clone());
                tokio::spawn(async move {
                    // The callback's signature is tungstenite's; its error type is large.
                    #[allow(clippy::result_large_err)]
                    let record = |request: &Request,
                                  response: Response|
                     -> Result<Response, ErrorResponse> {
                        for (name, value) in request.headers() {
                            seen_headers.lock().unwrap().push((
                                name.as_str().to_owned(),
                                value.to_str().unwrap_or("").to_owned(),
                            ));
                        }
                        Ok(response)
                    };
                    let Ok(mut socket) = tokio_tungstenite::accept_hdr_async(stream, record).await
                    else {
                        return;
                    };
                    while let Some(Ok(Message::Text(text))) = socket.next().await {
                        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
                            continue;
                        };
                        seen.lock().unwrap().push(frame.clone());
                        for reply in script(&frame) {
                            if socket.send(Message::Text(reply.into())).await.is_err() {
                                return;
                            }
                        }
                    }
                });
            }
        });
        Self {
            addr,
            frames,
            headers,
            task,
        }
    }
}
fn initialize_reply(id: &Value) -> String {
    json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":1,
        "agentCapabilities":{"loadSession":true},"authMethods":[]}})
    .to_string()
}
fn session_reply(id: &Value) -> String {
    json!({"jsonrpc":"2.0","id":id,"result":{"sessionId":"s-1"}}).to_string()
}

#[tokio::test]
async fn what_this_host_sends_a_peer_is_exactly_the_alias_the_rights_and_the_right_credential() {
    let peer = FakePeer::start(|frame| match frame["method"].as_str() {
        Some("initialize") => vec![initialize_reply(&frame["id"])],
        Some("session/new") => vec![session_reply(&frame["id"])],
        _ => vec![],
    })
    .await;
    let token = "t".repeat(64);
    let near = Near::start(
        &bridge_url(&peer.addr),
        &origin_of(&peer.addr),
        &token,
        |_| {},
    );
    let session = near
        .open_remote()
        .await
        .expect("opens against a scripted peer");
    assert_eq!(session.metadata().remote_peer.as_deref(), Some("b"));
    assert_eq!(session.metadata().acp_session_id, "s-1");

    let frames = peer.frames.lock().unwrap().clone();
    assert_eq!(frames.len(), 2, "{frames:?}");
    // This host serves no file or terminal callbacks for an agent that runs elsewhere.
    assert_eq!(frames[0]["method"], "initialize");
    assert_eq!(frames[0]["params"]["clientCapabilities"], json!({}));
    // The workspace is the peer's alias, the rights are only what was asked for, and nothing
    // that could run a command crosses the link.
    assert_eq!(frames[1]["method"], "session/new");
    assert_eq!(frames[1]["params"]["mcpServers"], json!([]));
    assert_eq!(
        frames[1]["params"]["_meta"],
        json!({"acpdWorkspace":"shared","acpdRights":{"read":true,"write":false,"terminal":false}})
    );
    // No local path leaks to the peer.
    let sent = serde_json::to_string(&frames).unwrap();
    let local = serde_json::to_string(&near.workspace.to_string_lossy()).unwrap();
    assert!(!sent.contains(&local[1..local.len() - 1]), "{sent}");
    // The credential went in the header the peer record names, and nowhere else.
    let headers = peer.headers.lock().unwrap().clone();
    let auth = headers
        .iter()
        .find(|(name, _)| name == "authorization")
        .expect("a credential header");
    assert_eq!(auth.1, format!("Bearer {token}"));
    assert!(!sent.contains(&token));
    peer.task.abort();
}

#[tokio::test]
async fn a_frame_carrying_a_second_message_after_a_newline_drops_the_link() {
    // A peer that smuggles a second message into one frame: the first line is a perfectly valid
    // answer, so only the framing check can stop the open from carrying on.
    let peer = FakePeer::start(|frame| match frame["method"].as_str() {
        Some("initialize") => vec![format!(
            "{}\n{}",
            initialize_reply(&frame["id"]),
            json!({"jsonrpc":"2.0","method":"session/update","params":{}})
        )],
        _ => vec![],
    })
    .await;
    let near = Near::start(
        &bridge_url(&peer.addr),
        &origin_of(&peer.addr),
        "any-token",
        |_| {},
    );
    let started = Instant::now();
    assert!(near.open_remote().await.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the link dropped at once instead of the open waiting for its next answer: {:?}",
        started.elapsed()
    );
    assert_eq!(
        peer.frames.lock().unwrap().len(),
        1,
        "nothing was sent after the bad frame (no session/new)"
    );
    peer.task.abort();
}

#[test]
fn registry_entries_for_remote_agents_are_validated() {
    let base = |extra: Value| {
        let mut entry = json!({"id":"r","name":"R","transport":"ws","url":"wss://far.example:8765/acp/goose",
            "peer":"far","workspaceAlias":"shared"});
        for (key, value) in extra.as_object().unwrap() {
            entry[key] = value.clone();
        }
        Registry::parse(&json!([entry]).to_string())
    };
    assert!(base(json!({})).is_ok());
    assert!(
        base(json!({"url":"ws://127.0.0.1:9/acp/x"})).is_ok(),
        "plain ws to this computer"
    );
    assert!(base(json!({"workspaceAlias":null,"remoteCwd":"/srv/work"})).is_ok());
    // A remote folder follows the other host's conventions, whichever OS this one is.
    for cwd in [
        "/srv/work",
        r"C:\srv\work",
        "C:/srv/work",
        r"\\server\share",
    ] {
        assert!(
            base(json!({"workspaceAlias":null,"remoteCwd":cwd})).is_ok(),
            "{cwd}"
        );
    }
    for cwd in ["work", "./work", "C:work", "~/work", ""] {
        assert!(
            base(json!({"workspaceAlias":null,"remoteCwd":cwd})).is_err(),
            "{cwd:?}"
        );
    }
    for (label, extra) in [
        ("a command", json!({"command":"goose"})),
        ("arguments", json!({"args":["acp"]})),
        ("an environment", json!({"env":{"A":"b"}})),
        ("a working directory", json!({"workingDirectory":"/tmp"})),
        ("no peer", json!({"peer":null})),
        ("a bad peer name", json!({"peer":"Has Space"})),
        ("no url", json!({"url":null})),
        (
            "plain ws to another computer",
            json!({"url":"ws://far.example:8765/acp/x"}),
        ),
        (
            "credentials in the url",
            json!({"url":"wss://u:p@far.example/acp/x"}),
        ),
        ("the wrong path", json!({"url":"wss://far.example/acp"})),
        ("a nested path", json!({"url":"wss://far.example/acp/a/b"})),
        ("an unrelated path", json!({"url":"wss://far.example/x/y"})),
        ("no workspace", json!({"workspaceAlias":null})),
        ("an alias and a folder", json!({"remoteCwd":"/srv/work"})),
        (
            "a relative folder",
            json!({"workspaceAlias":null,"remoteCwd":"work"}),
        ),
        ("a bad alias", json!({"workspaceAlias":"../etc"})),
    ] {
        assert!(base(extra).is_err(), "{label} should be refused");
    }
    // The remote-only fields do not belong on a local agent.
    for extra in ["url", "peer", "workspaceAlias", "remoteCwd"] {
        let mut local = json!({"id":"l","name":"L","command":"goose"});
        local[extra] = json!("x");
        assert!(
            Registry::parse(&json!([local]).to_string()).is_err(),
            "{extra}"
        );
    }
    assert!(
        Registry::parse(
            &json!([{"id":"l","name":"L","transport":"carrier-pigeon","command":"x"}]).to_string()
        )
        .is_err()
    );
    // Discovery reports it as available without looking for a program.
    let registry = base(json!({})).unwrap();
    let found = registry.discover();
    assert_eq!(found.len(), 1);
    assert!(found[0].installed && found[0].executable.is_none());
}

// ---- delegation across hosts ----------------------------------------------------------------

type Agent = ControlBackend<Box<dyn Stream>>;

async fn agent_of(session: &AcpSession) -> Agent {
    let definition = session.metadata().setup["_meta"]["mockMcpFull"]
        .as_array()
        .and_then(|servers| {
            servers
                .iter()
                .find(|s| s["name"] == "acpd-orchestrator")
                .cloned()
        })
        .expect("the orchestrating agent has delegation tools");
    let env = |name: &str| {
        definition["env"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name)
            .unwrap()["value"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let stream = control::connect(std::path::Path::new(&env("ACPD_CONTROL")))
        .await
        .unwrap();
    ControlBackend::new(stream, env("ACPD_TASK_TOKEN"))
}
async fn call(agent: &Agent, tool: &str, arguments: Value) -> Value {
    match agent.call(tool, arguments).await {
        Ok(value) => value,
        Err(failure) => panic!("{tool} failed: {} {}", failure.code, failure.message),
    }
}
async fn state_of(agent: &Agent, task: &str) -> String {
    call(agent, "task_status", json!({"task_id": task})).await["tasks"][0]["state"]
        .as_str()
        .unwrap()
        .to_owned()
}
async fn reaches(agent: &Agent, task: &str, wanted: &str) {
    eventually(|| async { (state_of(agent, task).await == wanted).then_some(()) }).await;
}

#[tokio::test]
async fn an_agent_delegates_to_an_agent_on_the_other_host_and_only_a_person_answers_for_it() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let near = Near::start(
        &bridge_url(&far.addr),
        &origin_of(&far.addr),
        &token,
        |_| {},
    );
    let root = near
        .manager
        .open_configured(
            "test-agent",
            &near.workspace,
            None,
            vec![],
            WorkspaceAccess::default(),
        )
        .await
        .unwrap();
    let agent = agent_of(&root).await;
    let listed = call(&agent, "agents_list", json!({})).await;
    assert_eq!(listed["agents"][0]["ref"], "remote");

    let started = call(
        &agent,
        "task_delegate",
        json!({"agent":"remote","prompt":"go","workspace":"sub","wait_seconds":30}),
    )
    .await;
    let task = started["task_id"].as_str().unwrap().to_owned();
    // The request that comes back from the other host is for a person: its paths are that host's.
    assert_eq!(started["state"], "input_required", "{started}");
    let request = &started["needs_decision"][0];
    assert_eq!(request["decidable"], false);
    assert_eq!(request["reason"], "human_only");
    assert!(request["note"].as_str().unwrap().contains("another host"));
    let attempt = agent
        .call("task_decide", json!({"task_id": task, "request_id": request["request_id"], "digest": request["digest"], "decision":"allow_once","rationale":"looks fine"}))
        .await
        .unwrap_err();
    assert_eq!(attempt.code, "not_decidable");

    // The phone on this host answers; the turn on the other host finishes and comes back.
    let child = eventually(|| async {
        near.manager
            .list()
            .await
            .into_iter()
            .find(|m| m.agent_id == "remote")
    })
    .await;
    let child = near.manager.get(child.id).await.unwrap();
    assert_eq!(child.metadata().remote_peer.as_deref(), Some("b"));
    let pending = first_pending(&child).await;
    allow(&child, &pending.message).await;
    reaches(&agent, &task, "completed").await;
    let result = call(&agent, "task_result", json!({"task_id": task})).await;
    assert_eq!(result["untrusted"], true);
    assert!(
        result["result"]
            .as_str()
            .unwrap()
            .contains("Mock lifecycle complete"),
        "{result}"
    );
    let _ = &near.orchestrator;
    far.stop().await;
}

#[tokio::test]
async fn cancelling_a_delegated_remote_task_reaches_the_other_host() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let near = Near::start(
        &bridge_url(&far.addr),
        &origin_of(&far.addr),
        &token,
        |_| {},
    );
    let root = near
        .manager
        .open_configured(
            "test-agent",
            &near.workspace,
            None,
            vec![],
            WorkspaceAccess::default(),
        )
        .await
        .unwrap();
    let agent = agent_of(&root).await;
    let started = call(
        &agent,
        "task_delegate",
        json!({"agent":"remote","prompt":"go","workspace":"sub","wait_seconds":30}),
    )
    .await;
    let task = started["task_id"].as_str().unwrap().to_owned();
    assert_eq!(started["state"], "input_required");
    call(&agent, "task_cancel", json!({"task_id": task})).await;
    // `session/cancel` crossed the link, the other agent ended its turn `cancelled`, and the
    // answer came back the same way.
    reaches(&agent, &task, "canceled").await;
    far.stop().await;
}

#[tokio::test]
async fn a_delegated_remote_task_whose_link_drops_fails_and_says_why_when_reconnecting_is_off() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let proxy = Proxy::start(far.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        &token,
        |config| {
            config.orchestrator.peer_reconnect_seconds = 0;
        },
    );
    let root = near
        .manager
        .open_configured(
            "test-agent",
            &near.workspace,
            None,
            vec![],
            WorkspaceAccess::default(),
        )
        .await
        .unwrap();
    let agent = agent_of(&root).await;
    let started = call(
        &agent,
        "task_delegate",
        json!({"agent":"remote","prompt":"go","workspace":"sub","wait_seconds":30}),
    )
    .await;
    let task = started["task_id"].as_str().unwrap().to_owned();
    assert_eq!(started["state"], "input_required");

    proxy.kill();
    reaches(&agent, &task, "failed").await;
    let status = call(&agent, "task_status", json!({"task_id": task})).await;
    assert_eq!(
        status["tasks"][0]["summary"],
        "the link to the other host was lost; the prompt was not resent"
    );
    assert_eq!(
        proxy.accepted.load(Ordering::SeqCst),
        1,
        "and it did not reconnect to try again"
    );
    far.stop().await;
}

#[tokio::test]
async fn a_delegated_remote_task_carries_its_trace_next_to_the_alias_and_rights() {
    let peer = FakePeer::start(|frame| match frame["method"].as_str() {
        Some("initialize") => vec![initialize_reply(&frame["id"])],
        Some("session/new") => vec![session_reply(&frame["id"])],
        _ => vec![],
    })
    .await;
    let near = Near::start(
        &bridge_url(&peer.addr),
        &origin_of(&peer.addr),
        &"t".repeat(64),
        |_| {},
    );
    let root = near
        .manager
        .open_configured(
            "test-agent",
            &near.workspace,
            None,
            vec![],
            WorkspaceAccess::default(),
        )
        .await
        .unwrap();
    let agent = agent_of(&root).await;
    call(
        &agent,
        "task_delegate",
        json!({"agent":"remote","prompt":"go","workspace":"sub"}),
    )
    .await;
    let frames = eventually(|| async {
        let seen = peer.frames.lock().unwrap().clone();
        (seen.len() >= 3).then_some(seen)
    })
    .await;
    assert_eq!(frames[1]["method"], "session/new");
    let meta = &frames[1]["params"]["_meta"];
    assert_eq!(meta["acpdWorkspace"], "shared", "{meta}");
    assert_eq!(
        meta["acpdRights"],
        json!({"read":true,"write":false,"terminal":false})
    );
    let trace = meta["traceparent"]
        .as_str()
        .expect("a trace goes with the session");
    let fields: Vec<&str> = trace.split('-').collect();
    assert_eq!(
        (fields[0], fields[1].len(), fields[2].len(), fields[3]),
        ("00", 32, 16, "01"),
        "{trace}"
    );
    // The prompt that follows carries the very same one.
    assert_eq!(frames[2]["method"], "session/prompt");
    assert_eq!(frames[2]["params"]["_meta"]["traceparent"], trace);
    peer.task.abort();
}

fn event_count(session: &AcpSession, method: &str) -> usize {
    session
        .replay(0)
        .events
        .iter()
        .filter(|event| event.message["method"] == method)
        .count()
}
async fn far_session(far: &Far, near_session: &AcpSession) -> Arc<AcpSession> {
    far.host
        .sessions
        .get(near_session.metadata().acp_session_id.parse().unwrap())
        .await
        .unwrap()
}
fn reconnecting(config: &mut Config) {
    config.orchestrator.peer_reconnect_seconds = 30;
}

#[tokio::test]
async fn a_lost_link_is_replaced_and_the_turn_in_flight_finishes_with_nothing_sent_twice() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let proxy = Proxy::start(far.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        &token,
        reconnecting,
    );
    let session = near.open_remote().await.unwrap();
    let prompting = session.clone();
    let prompt = tokio::spawn(async move { prompting.prompt("go").await });
    let request = first_pending(&session).await;
    let streamed = agent_text(&session);
    assert!(!streamed.is_empty(), "streaming began before the cut");

    proxy.sever();
    // The link comes back by itself (the first new try is about a second later).
    eventually(|| async { (proxy.accepted.load(Ordering::SeqCst) >= 2).then_some(()) }).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    // The far host replayed its whole journal; none of it reached the session a second time.
    let pending = session.connection.pending_permissions();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].message["id"], request.message["id"]);
    assert_eq!(event_count(&session, "session/request_permission"), 1);
    assert_eq!(agent_text(&session), streamed);
    assert_ne!(session.metadata().status, "exited");

    // The person answers on this side; the answer reaches the far side over the new socket and the
    // turn the first socket started finishes.
    allow(&session, &request.message).await;
    let done = prompt.await.unwrap().expect("the turn finishes");
    assert_eq!(done["stopReason"], "end_turn");
    let text = agent_text(&session);
    assert_eq!(text.matches("Mock lifecycle complete").count(), 1, "{text}");
    // Nothing was sent twice: the far host got one prompt.
    let remote = far_session(&far, &session).await;
    assert_eq!(event_count(&remote, "session/prompt"), 1);
    far.stop().await;
}

#[tokio::test]
async fn what_the_actor_writes_while_the_link_is_down_is_held_and_sent_once_when_it_is_back() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let proxy = Proxy::start(far.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        &token,
        reconnecting,
    );
    let session = near.open_remote().await.unwrap();
    let first = session.clone();
    let one = tokio::spawn(async move { first.prompt("one").await });
    let request = first_pending(&session).await;
    allow(&session, &request.message).await;
    one.await.unwrap().expect("the first turn finishes");

    proxy.refuse(true);
    proxy.sever();
    // Wait until a try has been made and refused: the pump is now in its retry wait, so what the
    // actor writes next is held (not written to a dying socket).
    eventually(|| async { (proxy.accepted.load(Ordering::SeqCst) >= 2).then_some(()) }).await;
    let second = session.clone();
    let two = tokio::spawn(async move { second.prompt("two").await });
    tokio::time::sleep(Duration::from_millis(800)).await;
    let remote = far_session(&far, &session).await;
    assert_eq!(event_count(&remote, "session/prompt"), 1, "nothing got out");
    proxy.refuse(false);

    let request = first_pending(&session).await;
    allow(&session, &request.message).await;
    two.await
        .unwrap()
        .expect("the held prompt ran once the link was back");
    assert_eq!(
        event_count(&remote, "session/prompt"),
        2,
        "once each, never twice"
    );
    assert_eq!(
        agent_text(&session)
            .matches("Mock lifecycle complete")
            .count(),
        2
    );
    far.stop().await;
}

#[tokio::test]
async fn a_link_that_stays_down_ends_the_session_when_the_window_runs_out() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let proxy = Proxy::start(far.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        &token,
        |config| {
            config.orchestrator.peer_reconnect_seconds = 2;
        },
    );
    let session = near.open_remote().await.unwrap();
    let prompting = session.clone();
    let prompt = tokio::spawn(async move { prompting.prompt("go").await });
    first_pending(&session).await;
    proxy.refuse(true);
    proxy.sever();
    eventually(|| async { session.connection.closed_reason() }).await;
    assert!(
        prompt.await.unwrap().is_err(),
        "the turn fails, as it always did"
    );
    assert_eq!(session.metadata().status, "exited");
    assert!(
        proxy.accepted.load(Ordering::SeqCst) >= 2,
        "it tried before giving up"
    );
    far.stop().await;
}

#[tokio::test]
async fn journal_events_the_far_host_evicted_before_this_host_saw_them_end_the_link() {
    // A far host that keeps eight journal events: while the link is down far more happen, so
    // some this host never saw are gone and a replay could not be complete.
    let far = Far::start_with(|config| config.runtime.history_events = 8).await;
    let (token, _) = far.pair_peer().await;
    let proxy = Proxy::start(far.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        &token,
        reconnecting,
    );
    let session = near.open_remote().await.unwrap();
    let prompting = session.clone();
    let prompt = tokio::spawn(async move { prompting.prompt("go").await });
    first_pending(&session).await;
    proxy.refuse(true);
    proxy.sever();
    let remote = far_session(&far, &session).await;
    for n in 0..30 {
        remote
            .connection
            .request(
                "_mock/emit_update",
                json!({"update":{"sessionUpdate":"agent_message_chunk",
                    "content":{"type":"text","text":format!("while away {n}")}}}),
            )
            .await
            .unwrap();
    }
    proxy.refuse(false);
    eventually(|| async { session.connection.closed_reason() }).await;
    assert!(prompt.await.unwrap().is_err());
    assert_eq!(session.metadata().status, "exited");
    far.stop().await;
}

#[tokio::test]
async fn a_far_host_that_does_not_number_its_frames_is_never_redialled() {
    let peer = FakePeer::start(|frame| match frame["method"].as_str() {
        Some("initialize") => vec![initialize_reply(&frame["id"])],
        Some("session/new") => vec![session_reply(&frame["id"])],
        _ => vec![],
    })
    .await;
    let proxy = Proxy::start(peer.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        "any-token",
        reconnecting,
    );
    let session = near.open_remote().await.unwrap();
    proxy.sever();
    eventually(|| async { session.connection.closed_reason() }).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        proxy.accepted.load(Ordering::SeqCst),
        1,
        "without frame numbers a replay cannot be told from news, so there is no second try"
    );
    peer.task.abort();
}

#[tokio::test]
async fn a_credential_revoked_while_the_link_is_down_ends_it_without_more_tries() {
    let far = Far::start().await;
    let (token, device) = far.pair_peer().await;
    let proxy = Proxy::start(far.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        &token,
        reconnecting,
    );
    let session = near.open_remote().await.unwrap();
    proxy.refuse(true);
    proxy.sever();
    far.host.security.revoke(device).unwrap();
    proxy.refuse(false);
    eventually(|| async { session.connection.closed_reason() }).await;
    assert_eq!(session.metadata().status, "exited");
    // One connection to open the session, one try that the far host refused: no more.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        proxy.accepted.load(Ordering::SeqCst) <= 3,
        "{} connections",
        proxy.accepted.load(Ordering::SeqCst)
    );
    far.stop().await;
}

#[tokio::test]
async fn a_prompt_written_as_the_link_died_is_sent_again_exactly_once_when_it_never_arrived() {
    let far = Far::start().await;
    let (token, _) = far.pair_peer().await;
    let proxy = Proxy::start(far.addr.clone()).await;
    let through = format!("127.0.0.1:{}", proxy.port);
    let near = Near::start(
        &bridge_url(&through),
        &origin_of(&through),
        &token,
        reconnecting,
    );
    let session = near.open_remote().await.unwrap();
    let remote = far_session(&far, &session).await;

    // From here what this side writes succeeds locally but goes nowhere: the prompt is "sent"
    // into a link that is already dead.
    proxy.swallow(true);
    let prompting = session.clone();
    let prompt = tokio::spawn(async move { prompting.prompt("go").await });
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        event_count(&remote, "session/prompt"),
        0,
        "it never arrived"
    );

    // The link goes. The far host's list of what it received lacks the prompt, so it is sent
    // again over the new socket, once.
    proxy.swallow(false);
    proxy.sever();
    let request = first_pending(&session).await;
    allow(&session, &request.message).await;
    let done = prompt.await.unwrap().expect("the prompt ran");
    assert_eq!(done["stopReason"], "end_turn");
    assert_eq!(event_count(&remote, "session/prompt"), 1);
    far.stop().await;
}
