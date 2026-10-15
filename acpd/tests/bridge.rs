//! The inbound agent bridge (`/acp/{agent_id}`) over real WebSockets: a paired peer host speaks
//! standard ACP to a real `Host`, which runs the mock agent. Roles, strictness, session id
//! translation, permission round trips, detach and reattach, and revocation.
use acpd::{
    api::Host,
    config::{Config, OrchestratorConfig, PeerRights, RuntimeConfig},
    registry::Registry,
    security::Role,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};
use uuid::Uuid;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Harness {
    host: Host,
    addr: String,
    client: reqwest::Client,
    phone: String,
    task: JoinHandle<()>,
    _directory: tempfile::TempDir,
}

impl Harness {
    async fn start() -> Self {
        Self::start_with(|_| {}).await
    }
    async fn start_with(tweak: impl FnOnce(&mut Config)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        std::fs::create_dir_all(workspace.join("shared")).unwrap();
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
                peer_workspaces: BTreeMap::from([("shared".to_owned(), workspace.join("shared"))]),
                bridge_resume_seconds: 120,
                bridge_ping_seconds: 5,
                ..Default::default()
            },
            ..Default::default()
        };
        tweak(&mut config);
        let agents: Vec<Value> = ["mock", "private"]
            .iter()
            .map(|id| json!({"id":id,"name":id,"command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--delay-ms","1"]}))
            .collect();
        let registry = Registry::parse(&serde_json::to_string(&agents).unwrap()).unwrap();
        let host = Host::new(config, registry).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let router = host.router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let mut harness = Self {
            host,
            addr,
            client,
            phone: String::new(),
            task,
            _directory: directory,
        };
        harness.phone = harness.pair(Role::Phone, "Test phone").await.0;
        harness
    }
    /// Pair a credential with `role`; returns its token and device id.
    async fn pair(&self, role: Role, name: &str) -> (String, Uuid) {
        let code = self.host.security.new_pairing_code_for(role).unwrap();
        let reply: Value = self
            .client
            .post(format!("http://{}/v1/pair", self.addr))
            .json(&json!({"code": code, "deviceName": name}))
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
    fn request(
        &self,
        path: &str,
        token: Option<&str>,
        origin: Option<&str>,
    ) -> tokio_tungstenite::tungstenite::http::Request<()> {
        let mut request = format!("ws://{}{path}", self.addr)
            .into_client_request()
            .unwrap();
        if let Some(token) = token {
            request
                .headers_mut()
                .insert("Authorization", format!("Bearer {token}").parse().unwrap());
        }
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert("Origin", origin.parse().unwrap());
        }
        request
    }
    async fn status_of(&self, path: &str, token: Option<&str>, origin: Option<&str>) -> u16 {
        match tokio_tungstenite::connect_async(self.request(path, token, origin)).await {
            Ok(_) => 101,
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                response.status().as_u16()
            }
            Err(other) => panic!("unexpected handshake error: {other}"),
        }
    }
    async fn connect(&self, token: &str) -> Client {
        let (socket, _) =
            tokio_tungstenite::connect_async(self.request("/acp/mock", Some(token), None))
                .await
                .expect("peer connects");
        Client {
            socket,
            next_id: 1,
            seen: vec![],
        }
    }
    async fn peer(&self, token: &str) -> Client {
        let mut client = self.connect(token).await;
        let init = client
            .call(
                "initialize",
                json!({"protocolVersion": 1, "clientCapabilities": {}}),
            )
            .await;
        assert_eq!(init["result"]["protocolVersion"], 1, "{init}");
        client
    }
    async fn stop(self) {
        self.task.abort();
        self.host.sessions.shutdown().await;
    }
}

struct Client {
    socket: Socket,
    next_id: u64,
    /// Everything received that was not the answer being waited for.
    seen: Vec<Value>,
}

impl Client {
    async fn send(&mut self, value: Value) {
        self.socket
            .send(Message::Text(value.to_string().into()))
            .await
            .unwrap();
    }
    async fn next_frame(&mut self) -> Option<Value> {
        // One deadline for the whole wait. A timeout per socket message would restart on every
        // WebSocket ping (the bridge sends one every few seconds) and never fire.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            match tokio::time::timeout_at(deadline, self.socket.next())
                .await
                .expect("frame in time")?
            {
                Ok(Message::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
                Ok(Message::Ping(_) | Message::Pong(_)) => {}
                Ok(Message::Close(_)) | Err(_) => return None,
                Ok(_) => {}
            }
        }
    }
    /// Send a request and wait for its response; other frames are kept in `seen`.
    async fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await;
        self.wait_for_response(json!(id)).await
    }
    async fn wait_for_response(&mut self, id: Value) -> Value {
        let give_up = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            assert!(
                tokio::time::Instant::now() < give_up,
                "no response to request {id} within 30 s; seen so far: {:?}",
                self.seen
            );
            let frame = self.next_frame().await.expect("socket stayed open");
            if frame.get("method").is_none() && frame["id"] == id {
                return frame;
            }
            self.seen.push(frame);
        }
    }
    /// Wait for a frame the agent sends (a request or notification) matching `wanted`.
    async fn wait_for(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self.seen.iter().position(&wanted) {
            return self.seen.remove(index);
        }
        let give_up = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            assert!(
                tokio::time::Instant::now() < give_up,
                "the expected frame did not arrive within 30 s; seen so far: {:?}",
                self.seen
            );
            let frame = self.next_frame().await.expect("socket stayed open");
            if wanted(&frame) {
                return frame;
            }
            self.seen.push(frame);
        }
    }
    async fn new_session(&mut self) -> String {
        let reply = self
            .call(
                "session/new",
                json!({"cwd": "/ignored", "mcpServers": [],
                "_meta": {"acpdWorkspace": "shared"}}),
            )
            .await;
        reply["result"]["sessionId"]
            .as_str()
            .unwrap_or_else(|| panic!("{reply}"))
            .to_owned()
    }
    /// Start a prompt without waiting for it; returns the request id.
    async fn prompt(&mut self, session: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":"session/prompt",
            "params":{"sessionId":session,"prompt":[{"type":"text","text":"go"}]}}))
            .await;
        id
    }
    async fn allow(&mut self, request: &Value, option: &str) {
        self.send(json!({"jsonrpc":"2.0","id":request["id"],
            "result":{"outcome":{"outcome":"selected","optionId":option}}}))
            .await;
    }
    fn texts(&self) -> String {
        self.seen
            .iter()
            .filter(|f| {
                f["method"] == "session/update"
                    && f["params"]["update"]["sessionUpdate"] == "agent_message_chunk"
            })
            .filter_map(|f| f["params"]["update"]["content"]["text"].as_str())
            .collect()
    }
}

fn is_permission(frame: &Value) -> bool {
    frame["method"] == "session/request_permission"
}

#[tokio::test]
async fn routes_roles_and_origin_are_enforced_before_any_session_exists() {
    let h = Harness::start().await;
    let (peer, _) = h.pair(Role::Peer, "Laptop").await;
    // Credentials: none, the wrong role, and a good peer.
    assert_eq!(h.status_of("/acp/mock", None, None).await, 401);
    assert_eq!(
        h.status_of("/acp/mock", Some(&h.phone), None).await,
        403,
        "a phone is not a peer"
    );
    assert_eq!(h.status_of("/acp/mock", Some(&peer), None).await, 101);
    // A peer's credential reaches nothing else.
    for path in ["/v1/status", "/v1/sessions", "/v1/agents", "/v1/workspaces"] {
        let reply = h
            .client
            .get(format!("http://{}{path}", h.addr))
            .bearer_auth(&peer)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 403, "{path}");
    }
    let revoke = h
        .client
        .delete(format!("http://{}/v1/device", h.addr))
        .bearer_auth(&peer)
        .send()
        .await
        .unwrap();
    assert_eq!(revoke.status(), 403, "a peer cannot even revoke itself");
    // ... while the phone still works as before.
    let status = h
        .client
        .get(format!("http://{}/v1/status", h.addr))
        .bearer_auth(&h.phone)
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    // Browsers are not peers.
    assert_eq!(
        h.status_of("/acp/mock", Some(&peer), Some("http://evil.example"))
            .await,
        403
    );
    // The agent must be named, and must be one this host shares with peers.
    let bare = h
        .client
        .get(format!("http://{}/acp", h.addr))
        .bearer_auth(&peer)
        .send()
        .await
        .unwrap();
    assert_eq!(bare.status(), 400);
    assert_eq!(
        h.status_of("/acp/private", Some(&peer), None).await,
        404,
        "registered but not shared"
    );
    assert_eq!(
        h.status_of("/acp/nobody", Some(&peer), None).await,
        404,
        "not registered"
    );
    // Streamable HTTP is not offered.
    for method in [reqwest::Method::POST, reqwest::Method::DELETE] {
        let reply = h
            .client
            .request(method, format!("http://{}/acp/mock", h.addr))
            .bearer_auth(&peer)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 501);
    }
    assert_eq!(
        h.host.sessions.active_count().await,
        0,
        "none of that started an agent"
    );
    h.stop().await;

    let off = Harness::start_with(|c| c.orchestrator.enabled = false).await;
    let (peer, _) = off.pair(Role::Peer, "Laptop").await;
    assert_eq!(
        off.status_of("/acp/mock", Some(&peer), None).await,
        404,
        "delegation off means no bridge"
    );
    off.stop().await;
}

#[tokio::test]
async fn the_socket_is_stricter_than_the_reference_server() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.connect(&token).await;
    // Nothing but initialize works first.
    let early = c
        .call(
            "session/new",
            json!({"cwd":"/x","mcpServers":[],"_meta":{"acpdWorkspace":"shared"}}),
        )
        .await;
    assert_eq!(early["error"]["code"], -32600);
    assert_eq!(h.host.sessions.active_count().await, 0);
    // Garbage and batches are refused with JSON-RPC errors; binary frames are ignored.
    c.socket
        .send(Message::Text("not json".into()))
        .await
        .unwrap();
    assert_eq!(c.next_frame().await.unwrap()["error"]["code"], -32700);
    c.socket
        .send(Message::Text(
            json!([{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}])
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let batch = c.next_frame().await.unwrap();
    assert_eq!(batch["error"]["code"], -32600);
    assert!(
        batch["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Batch")
    );
    c.socket
        .send(Message::Binary(vec![0, 1, 2].into()))
        .await
        .unwrap();
    // A different protocol version is answered with ours; a second initialize is refused.
    let init = c
        .call(
            "initialize",
            json!({"protocolVersion": 2, "clientCapabilities": {}}),
        )
        .await;
    assert_eq!(init["result"]["protocolVersion"], 1);
    assert_eq!(init["result"]["agentCapabilities"]["loadSession"], true);
    assert_eq!(
        init["result"]["agentCapabilities"]["mcpCapabilities"],
        json!({"http": false, "sse": false})
    );
    assert_eq!(init["result"]["authMethods"], json!([]));
    let again = c.call("initialize", json!({"protocolVersion": 1})).await;
    assert_eq!(again["error"]["code"], -32600);
    // The socket survived all of it.
    let session = c.new_session().await;
    assert!(Uuid::parse_str(&session).is_ok());
    // Unknown and unanswerable things get errors, not silence.
    let unknown = c
        .call("session/prompt", json!({"sessionId":"nope","prompt":[]}))
        .await;
    assert_eq!(unknown["error"]["code"], -32602);
    let missing = c.call("session/prompt", json!({"prompt":[]})).await;
    assert_eq!(missing["error"]["code"], -32602);
    // Oversized frames close the socket (the limit is the host's frame limit).
    let big = "x".repeat(h.host.config.runtime.max_frame_bytes + 1024);
    c.socket
        .send(Message::Text(
            json!({"jsonrpc":"2.0","id":99,"method":"ping","params":{"p":big}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert!(
        c.next_frame().await.is_none(),
        "an oversized frame ends the connection"
    );
    h.stop().await;
}

#[tokio::test]
async fn a_peer_names_a_workspace_by_alias_and_cannot_bring_commands_or_extra_rights() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let meta = |extra: Value| {
        let mut meta = json!({"acpdWorkspace": "shared"});
        for (k, v) in extra.as_object().unwrap() {
            meta[k] = v.clone();
        }
        meta
    };
    for (label, params) in [
        (
            "no cwd",
            json!({"mcpServers": [], "_meta": meta(json!({}))}),
        ),
        ("no alias", json!({"cwd": "/x", "mcpServers": []})),
        (
            "unknown alias",
            json!({"cwd": "/x", "mcpServers": [], "_meta": {"acpdWorkspace": "other"}}),
        ),
        (
            "a path instead of an alias",
            json!({"cwd": "/x", "mcpServers": [], "_meta": {"acpdWorkspace": "/etc"}}),
        ),
        (
            "a stdio mcp server",
            json!({"cwd": "/x", "mcpServers": [{"name":"x","command":"/bin/sh","args":["-c","id"],"env":[]}], "_meta": meta(json!({}))}),
        ),
        (
            "an http mcp server",
            json!({"cwd": "/x", "mcpServers": [{"type":"http","name":"x","url":"https://example.com","headers":[]}], "_meta": meta(json!({}))}),
        ),
        (
            "a malformed mcp field",
            json!({"cwd": "/x", "mcpServers": "all", "_meta": meta(json!({}))}),
        ),
        (
            "write rights",
            json!({"cwd": "/x", "mcpServers": [], "_meta": meta(json!({"acpdRights": {"write": true}}))}),
        ),
        (
            "terminal rights",
            json!({"cwd": "/x", "mcpServers": [], "_meta": meta(json!({"acpdRights": {"terminal": true}}))}),
        ),
        (
            "rights of the wrong type",
            json!({"cwd": "/x", "mcpServers": [], "_meta": meta(json!({"acpdRights": {"read": "yes"}}))}),
        ),
    ] {
        let reply = c.call("session/new", params).await;
        assert_eq!(reply["error"]["code"], -32602, "{label}: {reply}");
    }
    assert_eq!(
        h.host.sessions.active_count().await,
        0,
        "no refused request started an agent"
    );

    // The default is the host's ceiling: read only.
    let read_only = c.new_session().await;
    let session = h
        .host
        .sessions
        .get(read_only.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        session
            .connection
            .request("_mock/client_capabilities", json!({}))
            .await
            .unwrap(),
        json!({"fs":{"readTextFile":true,"writeTextFile":false},"terminal":false})
    );
    // A peer may ask for less than the ceiling.
    let reply = c
        .call(
            "session/new",
            json!({"cwd":"/x","mcpServers":[],"_meta":meta(json!({"acpdRights":{"read":false}}))}),
        )
        .await;
    let none: String = reply["result"]["sessionId"].as_str().unwrap().into();
    let session = h.host.sessions.get(none.parse().unwrap()).await.unwrap();
    assert_eq!(
        session
            .connection
            .request("_mock/client_capabilities", json!({}))
            .await
            .unwrap()["fs"]["readTextFile"],
        false
    );
    assert_eq!(
        session.metadata().workspace,
        h.host.config.workspace_roots[0]
            .join("shared")
            .canonicalize()
            .unwrap()
    );
    h.stop().await;

    // A host that allows peers to write says so in its own config; the peer may then ask for it.
    let permissive = Harness::start_with(|c| {
        c.orchestrator.peer_rights = PeerRights {
            read: true,
            write: true,
            terminal: false,
        };
    })
    .await;
    let (token, _) = permissive.pair(Role::Peer, "Laptop").await;
    let mut c = permissive.peer(&token).await;
    let reply = c.call("session/new", json!({"cwd":"/x","mcpServers":[],"_meta":{"acpdWorkspace":"shared","acpdRights":{"write":true}}})).await;
    assert!(reply["result"]["sessionId"].is_string(), "{reply}");
    let denied = c.call("session/new", json!({"cwd":"/x","mcpServers":[],"_meta":{"acpdWorkspace":"shared","acpdRights":{"terminal":true}}})).await;
    assert_eq!(denied["error"]["code"], -32602);
    permissive.stop().await;
}

#[tokio::test]
async fn a_prompt_round_trips_with_the_peer_deciding_the_permission() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let session = c.new_session().await;
    let prompt = c.prompt(&session).await;
    // The agent's permission request reaches the peer with the peer's session id on it.
    let request = c.wait_for(is_permission).await;
    assert_eq!(request["params"]["sessionId"], session.as_str());
    assert_eq!(request["params"]["toolCall"]["kind"], "edit");
    c.allow(&request, "allow").await;
    let done = c.wait_for_response(json!(prompt)).await;
    assert_eq!(done["result"]["stopReason"], "end_turn", "{done}");
    // Streamed text carried the bridge id, not the agent's own.
    assert!(c.texts().contains("Mock lifecycle complete"));
    for frame in c.seen.iter().filter(|f| f["method"] == "session/update") {
        assert_eq!(frame["params"]["sessionId"], session.as_str());
    }
    // The phone sees the same session in its own list.
    let list: Value = h
        .client
        .get(format!("http://{}/v1/sessions", h.addr))
        .bearer_auth(&h.phone)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == session.as_str()),
        "{list}"
    );
    h.stop().await;
}

#[tokio::test]
async fn cancel_ends_a_turn_with_the_stop_reason_cancelled() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let session = c.new_session().await;
    let prompt = c.prompt(&session).await;
    c.wait_for(is_permission).await;
    c.send(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session}}))
        .await;
    let done = c.wait_for_response(json!(prompt)).await;
    assert_eq!(done["result"]["stopReason"], "cancelled", "{done}");
    h.stop().await;
}

#[tokio::test]
async fn two_sessions_with_the_same_agent_id_never_cross() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let (a, b) = (c.new_session().await, c.new_session().await);
    assert_ne!(a, b);
    // Both agent processes call their session `mock-1`; the peer must never see that.
    let acp_ids: Vec<String> = {
        let mut ids = vec![];
        for id in [&a, &b] {
            ids.push(
                h.host
                    .sessions
                    .get(id.parse().unwrap())
                    .await
                    .unwrap()
                    .metadata()
                    .acp_session_id,
            );
        }
        ids
    };
    assert_eq!(acp_ids[0], acp_ids[1], "the collision is real");
    let first = c.prompt(&a).await;
    let request = c.wait_for(is_permission).await;
    assert_eq!(request["params"]["sessionId"], a.as_str());
    assert!(
        !c.seen
            .iter()
            .any(|f| f["params"]["sessionId"] == b.as_str()),
        "nothing from the other session"
    );
    c.allow(&request, "allow").await;
    assert_eq!(
        c.wait_for_response(json!(first)).await["result"]["stopReason"],
        "end_turn"
    );
    // Now the second one, on its own.
    let second = c.prompt(&b).await;
    let request = c.wait_for(is_permission).await;
    assert_eq!(request["params"]["sessionId"], b.as_str());
    c.allow(&request, "allow").await;
    assert_eq!(
        c.wait_for_response(json!(second)).await["result"]["stopReason"],
        "end_turn"
    );
    h.stop().await;
}

#[tokio::test]
async fn one_peer_cannot_use_or_reattach_another_peers_session() {
    let h = Harness::start().await;
    let (one, _) = h.pair(Role::Peer, "One").await;
    let (two, _) = h.pair(Role::Peer, "Two").await;
    let mut first = h.peer(&one).await;
    let session = first.new_session().await;
    let mut second = h.peer(&two).await;
    let prompt = second
        .call(
            "session/prompt",
            json!({"sessionId":session,"prompt":[{"type":"text","text":"mine now"}]}),
        )
        .await;
    assert_eq!(
        prompt["error"]["code"], -32602,
        "not attached to this connection"
    );
    let load = second
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert_eq!(load["error"]["code"], -32602);
    drop(first);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let load = second
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert_eq!(load["error"]["code"], -32602, "even after the owner left");
    // The owner can still take it back.
    let mut owner = h.peer(&one).await;
    let back = owner
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert!(back["result"].is_object(), "{back}");
    h.stop().await;
}

#[tokio::test]
async fn a_peer_reattaches_after_a_drop_and_the_turn_it_left_running_finishes() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let session = c.new_session().await;
    let prompt = c.prompt(&session).await;
    let request = c.wait_for(is_permission).await;
    assert!(
        c.texts().contains("inspect"),
        "streaming began before the drop"
    );
    // The socket vanishes with a permission outstanding.
    drop(c);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        h.host.sessions.active_count().await,
        1,
        "the session is kept, not ended"
    );

    let mut again = h.peer(&token).await;
    let load = again
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert!(load["result"].is_object(), "{load}");
    assert!(
        load["result"].get("_meta").is_none(),
        "nothing was lost: no gap"
    );
    // What happened is replayed: the text so far and the request still waiting for an answer.
    let replayed = again.wait_for(is_permission).await;
    assert_eq!(replayed["id"], request["id"]);
    assert_eq!(replayed["params"]["sessionId"], session.as_str());
    assert!(again.texts().contains("inspect"));
    // The peer answers it now, and the turn it started on the old socket completes.
    again.allow(&replayed, "allow").await;
    let done = again.wait_for_response(json!(prompt)).await;
    assert_eq!(done["result"]["stopReason"], "end_turn", "{done}");
    assert!(again.texts().contains("Mock lifecycle complete"));
    // Loading something already attached here is refused, as is a second attach elsewhere.
    let twice = again
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert_eq!(twice["error"]["code"], -32602);
    let mut third = h.peer(&token).await;
    let stolen = third
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert_eq!(stolen["error"]["code"], -32602, "one controller at a time");
    h.stop().await;
}

#[tokio::test]
async fn a_finished_turn_is_waiting_for_a_peer_that_reattaches_late() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let session = c.new_session().await;
    let prompt = c.prompt(&session).await;
    let request = c.wait_for(is_permission).await;
    // Somebody else (the host's phone) answers while the peer is away.
    drop(c);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let host_session = h.host.sessions.get(session.parse().unwrap()).await.unwrap();
    host_session
        .connection
        .permission(
            request["id"].clone(),
            json!({"outcome":"selected","optionId":"allow"}),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut again = h.peer(&token).await;
    let load = again
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert!(load["result"].is_object());
    // The result of the prompt it sent on the dead socket is replayed under its original id.
    let done = again
        .seen
        .iter()
        .find(|f| f.get("method").is_none() && f["id"] == json!(prompt))
        .cloned()
        .expect("the prompt's response was replayed");
    assert_eq!(done["result"]["stopReason"], "end_turn");
    assert!(again.texts().contains("Mock lifecycle complete"));
    h.stop().await;
}

#[tokio::test]
async fn a_bounded_journal_reports_what_it_could_not_replay() {
    let h = Harness::start_with(|c| c.runtime.history_events = 3).await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let session = c.new_session().await;
    let prompt = c.prompt(&session).await;
    let request = c.wait_for(is_permission).await;
    c.allow(&request, "allow").await;
    c.wait_for_response(json!(prompt)).await;
    drop(c);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut again = h.peer(&token).await;
    let load = again
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    let missed = load["result"]["_meta"]["acpdGap"]["missed"]
        .as_u64()
        .expect("a gap is reported");
    assert!(missed > 0, "{load}");
    // The extension lives in `_meta` only, never as a field of the standard result.
    assert_eq!(
        load["result"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["_meta"]
    );
    h.stop().await;
}

#[tokio::test]
async fn an_unclaimed_session_is_removed_after_the_resume_window() {
    let h = Harness::start_with(|c| c.orchestrator.bridge_resume_seconds = 1).await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let kept = c.new_session().await;
    let lost = c.new_session().await;
    drop(c);
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Reattaching inside the window cancels that session's removal.
    let mut again = h.peer(&token).await;
    let load = again
        .call(
            "session/load",
            json!({"sessionId":kept,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert!(load["result"].is_object(), "{load}");
    tokio::time::sleep(Duration::from_millis(1800)).await;
    assert!(
        h.host.sessions.get(kept.parse().unwrap()).await.is_ok(),
        "the reattached session stays"
    );
    assert!(
        h.host.sessions.get(lost.parse().unwrap()).await.is_err(),
        "the unclaimed one is gone"
    );
    let gone = again
        .call(
            "session/load",
            json!({"sessionId":lost,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert_eq!(gone["error"]["code"], -32602);
    assert_eq!(h.host.sessions.active_count().await, 1);
    h.stop().await;
}

#[tokio::test]
async fn an_old_removal_timer_cannot_end_a_session_that_was_reattached_and_left_again() {
    let h = Harness::start_with(|c| c.orchestrator.bridge_resume_seconds = 2).await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let session = c.new_session().await;
    let id: Uuid = session.parse().unwrap();
    drop(c); // first timer: fires about 2 s from now
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut again = h.peer(&token).await;
    let load = again
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert!(load["result"].is_object(), "{load}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    drop(again); // second timer: fires about 2 s after this
    // Past the first timer's deadline, before the second's: the session must still be there.
    tokio::time::sleep(Duration::from_millis(1400)).await;
    assert!(
        h.host.sessions.get(id).await.is_ok(),
        "the stale timer removed a session it no longer owned"
    );
    // And the second timer does end it.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(h.host.sessions.get(id).await.is_err());
    h.stop().await;
}

#[tokio::test]
async fn revoking_a_peer_ends_its_attached_session_at_its_next_message() {
    let h = Harness::start().await;
    let (token, device) = h.pair(Role::Peer, "Laptop").await;
    let (other, _) = h.pair(Role::Peer, "Other").await;
    let mut c = h.peer(&token).await;
    let mut bystander = h.peer(&other).await;
    let mine = c.new_session().await;
    let theirs = bystander.new_session().await;
    assert_eq!(h.host.sessions.active_count().await, 2);
    h.host.security.revoke(device).unwrap();
    let revoked_at = std::time::Instant::now();
    c.send(json!({"jsonrpc":"2.0","id":50,"method":"session/prompt","params":{"sessionId":mine,"prompt":[]}})).await;
    assert!(
        c.next_frame().await.is_none(),
        "the revoked peer is disconnected"
    );
    // The very next message does it; the 5-second heartbeat is only the backstop.
    assert!(
        revoked_at.elapsed() < Duration::from_secs(3),
        "{:?}",
        revoked_at.elapsed()
    );
    // Its session is gone at once; the other peer's is untouched.
    for _ in 0..40 {
        if h.host.sessions.get(mine.parse().unwrap()).await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(h.host.sessions.get(mine.parse().unwrap()).await.is_err());
    assert!(h.host.sessions.get(theirs.parse().unwrap()).await.is_ok());
    assert_eq!(
        h.status_of("/acp/mock", Some(&token), None).await,
        401,
        "and cannot come back"
    );
    h.stop().await;
}

#[tokio::test]
async fn the_phone_can_watch_a_bridged_session_but_not_take_control_of_it() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let session = c.new_session().await;
    let connect = |read_only: bool| {
        let mut request = format!(
            "ws://{}/v1/sessions/{session}/connect?readOnly={read_only}",
            h.addr
        )
        .into_client_request()
        .unwrap();
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", h.phone).parse().unwrap(),
        );
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "acpd.v1".parse().unwrap());
        request
    };
    match tokio_tungstenite::connect_async(connect(false)).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), 409)
        }
        other => panic!(
            "expected the controller conflict, got {:?}",
            other.map(|_| ())
        ),
    }
    assert!(
        tokio_tungstenite::connect_async(connect(true))
            .await
            .is_ok(),
        "read-only attach is fine"
    );
    // Once the peer lets go, the lease is free again.
    drop(c);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        tokio_tungstenite::connect_async(connect(false))
            .await
            .is_ok()
    );
    h.stop().await;
}

#[tokio::test]
async fn peer_info_tells_a_peer_what_it_may_use_and_nothing_more() {
    let h = Harness::start().await;
    let (peer, _) = h.pair(Role::Peer, "Laptop").await;
    let url = format!("http://{}/v1/peer/info", h.addr);
    // Credentials: none, and a phone (which has /v1/status for itself).
    assert_eq!(h.client.get(&url).send().await.unwrap().status(), 401);
    let phone = h
        .client
        .get(&url)
        .bearer_auth(&h.phone)
        .send()
        .await
        .unwrap();
    assert_eq!(phone.status(), 403);

    let reply = h.client.get(&url).bearer_auth(&peer).send().await.unwrap();
    assert_eq!(reply.status(), 200);
    let text = reply.text().await.unwrap();
    let info: Value = serde_json::from_str(&text).unwrap();
    // Only the shared agent: "private" exists in the registry but is not shared with peers.
    assert_eq!(info["agents"], json!([{"id": "mock", "name": "mock"}]));
    assert_eq!(info["workspaces"], json!(["shared"]));
    assert_eq!(
        info["rights"],
        json!({"read": true, "write": false, "terminal": false})
    );
    assert_eq!(info["resumeSeconds"], 120);
    assert_eq!(info["protocolVersion"], 1);
    assert!(info["hostId"].is_string());
    // An alias is all a peer ever learns: no path, no unshared agent, no token.
    assert!(!text.contains(['/', '\\']), "no path in {text}");
    assert!(!text.contains("private"), "{text}");
    assert!(!text.contains(&peer), "{text}");
}

#[tokio::test]
async fn peer_info_is_not_found_when_the_bridge_is_off() {
    let h = Harness::start_with(|config| config.orchestrator.enabled = false).await;
    let (peer, _) = h.pair(Role::Peer, "Laptop").await;
    let reply = h
        .client
        .get(format!("http://{}/v1/peer/info", h.addr))
        .bearer_auth(&peer)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 404);
}

#[tokio::test]
async fn a_peer_that_stops_reading_is_cut_off_and_can_reattach() {
    // Everything else that could end this connection is set out of reach: a 10-minute send
    // timeout and a 2-minute ping (so six minutes of silence). What is left is the full outbox.
    let h = Harness::start_with(|config| {
        config.orchestrator.bridge_send_timeout_seconds = 600;
        config.orchestrator.bridge_ping_seconds = 120;
    })
    .await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    let session = c.new_session().await;
    let id = h.host.sessions.list().await[0].id;
    let host_session = h.host.sessions.get(id).await.unwrap();
    // The peer reads nothing while the agent streams far more (300 x 256 KiB) than the socket
    // buffers and the 256-frame outbox can hold. With the other limits out of reach, only the
    // outbox filling can cut it off (a mutation run removing that signal makes this test fail:
    // the socket then stays open and the read below waits out its deadline).
    let text = "z".repeat(256 * 1024);
    for _ in 0..300 {
        host_session
            .connection
            .request(
                "_mock/emit_update",
                json!({"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}),
            )
            .await
            .unwrap();
    }
    // Reading now ends: the host closed the socket instead of buffering without bound, and the
    // peer saw only part of the stream.
    let mut received = 0;
    while let Some(frame) = c.next_frame().await {
        if frame["method"] == "session/update" {
            received += 1;
        }
    }
    assert!(received > 0 && received < 300, "received {received} of 300");
    // The session is kept, and the peer may come back for it.
    assert_eq!(h.host.sessions.active_count().await, 1);
    let mut again = h.peer(&token).await;
    let load = again
        .call(
            "session/load",
            json!({"sessionId":session,"cwd":"/x","mcpServers":[]}),
        )
        .await;
    assert!(load["result"].is_object(), "{load}");
    h.stop().await;
}

#[tokio::test]
async fn a_peer_that_floods_messages_is_closed_with_rate_limited() {
    let h = Harness::start().await;
    let (token, _) = h.pair(Role::Peer, "Laptop").await;
    let mut c = h.peer(&token).await;
    // A modest burst is fine.
    for _ in 0..20 {
        let reply = c.call("session/list", json!({})).await;
        assert!(reply.get("error").is_some() || reply.get("result").is_some());
    }
    // 150 in a row, without reading any reply, is a loop, not a person.
    for _ in 0..150 {
        c.send(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"none"}}))
            .await;
    }
    let mut closed = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while let Ok(Some(Ok(message))) = tokio::time::timeout_at(deadline, c.socket.next()).await {
        if let Message::Close(frame) = message {
            closed = frame.map(|f| (u16::from(f.code), f.reason.to_string()));
            break;
        }
    }
    assert_eq!(closed, Some((1008, "rate_limited".to_owned())));
    h.stop().await;
}
