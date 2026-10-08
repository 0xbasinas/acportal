//! End-to-end checks against `testy`, the deterministic ACP test agent from the official
//! Rust SDK (agentclientprotocol/rust-sdk, `agent-client-protocol-test`). testy is registered
//! through the normal registry and driven through the real host router and phone WebSocket.
//!
//! These tests do nothing unless `ACPD_TESTY_BIN` points at a built testy binary (see
//! `scripts/build-testy.sh` and docs/testy.md), so the ordinary `cargo test` stays fast.
#![cfg(unix)]
use acpd::{api::Host, config::Config, registry::Registry};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{net::TcpStream, task::JoinHandle, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const WAIT: Duration = Duration::from_secs(30);

fn testy() -> Option<PathBuf> {
    match std::env::var_os("ACPD_TESTY_BIN") {
        Some(path) => {
            let path = PathBuf::from(path);
            assert!(
                path.is_file(),
                "ACPD_TESTY_BIN={} is not a file",
                path.display()
            );
            Some(path)
        }
        None => {
            eprintln!(
                "skipping testy test: set ACPD_TESTY_BIN to a testy binary (scripts/build-testy.sh prints the path)"
            );
            None
        }
    }
}

struct Fixture {
    host: Host,
    base: String,
    token: String,
    client: reqwest::Client,
    task: JoinHandle<()>,
    workspace: PathBuf,
    _directory: tempfile::TempDir,
}

impl Fixture {
    /// `workspace` defaults to a fresh temporary directory; testy's file and terminal callbacks
    /// use fixed `/tmp` paths, so the in-workspace callback tests pass `/tmp`.
    async fn start(testy: &Path, workspace: Option<PathBuf>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = workspace.unwrap_or_else(|| {
            let path = directory.path().join("workspace");
            std::fs::create_dir(&path).unwrap();
            path
        });
        let config = Config {
            workspace_roots: vec![workspace.clone()],
            state_directory: directory.path().join("state"),
            ..Default::default()
        };
        let registry = Registry::parse(
            &json!([{"id":"testy","name":"Testy","command":testy,"args":[]}]).to_string(),
        )
        .unwrap();
        let host = Host::new(config, registry).unwrap();
        let code = host.security.new_pairing_code().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = host.router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let paired: Value = client
            .post(format!("{base}/v1/pair"))
            .json(&json!({"code":code,"deviceName":"Testy phone"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        Self {
            host,
            base,
            token: paired["token"].as_str().unwrap().into(),
            client,
            task,
            workspace,
            _directory: directory,
        }
    }
    async fn create(&self) -> Value {
        self.create_with_mcp(json!([])).await
    }
    async fn create_with_mcp(&self, mcp_servers: Value) -> Value {
        let info: Value = self
            .client
            .post(format!("{}/v1/sessions", self.base))
            .bearer_auth(&self.token)
            .json(&json!({"agentId":"testy","workspace":self.workspace,"mcpServers":mcp_servers}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(info["status"], "ready", "{info}");
        info
    }
    async fn connect(&self, id: &str) -> Socket {
        let url = format!(
            "{}/v1/sessions/{id}/connect?after=0",
            self.base.replace("http:", "ws:")
        );
        let mut request = url.into_client_request().unwrap();
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", self.token).parse().unwrap(),
        );
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "acpd.v1".parse().unwrap());
        let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        next_until(&mut socket, |value| value["type"] == "replay_complete").await;
        socket
    }
    async fn stop(self) {
        self.host.sessions.shutdown().await;
        self.task.abort();
    }
}

async fn send(socket: &mut Socket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

async fn next_value(socket: &mut Socket) -> Value {
    timeout(WAIT, async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(_) => socket.flush().await.unwrap(),
                _ => {}
            }
        }
    })
    .await
    .expect("timed out waiting for a host message")
}

async fn next_until(socket: &mut Socket, condition: impl Fn(&Value) -> bool) -> Value {
    loop {
        let value = next_value(socket).await;
        if condition(&value) {
            return value;
        }
    }
}

/// What the test "phone" answers when the host shows a consent or forwards an agent permission.
#[derive(Clone, Copy)]
enum Answer {
    Allow,
    Deny,
}

/// Everything seen while one request ran, plus its JSON-RPC response.
struct Turn {
    response: Value,
    events: Vec<Value>,
    /// (`_meta.acpdSource` or "agent", chosen option id) per consent answered.
    consents: Vec<(String, String)>,
}

impl Turn {
    fn updates(&self) -> impl Iterator<Item = &Value> {
        self.events
            .iter()
            .filter(|event| event["message"]["method"] == "session/update")
            .map(|event| &event["message"]["params"]["update"])
    }
    /// Agent `session/update` notifications that reached the phone.
    fn agent_update_count(&self) -> usize {
        self.events
            .iter()
            .filter(|event| {
                event["direction"] == "agent" && event["message"]["method"] == "session/update"
            })
            .count()
    }
    fn update_kinds(&self) -> BTreeSet<String> {
        self.updates()
            .filter_map(|update| update["sessionUpdate"].as_str().map(str::to_owned))
            .collect()
    }
    fn agent_text(&self) -> String {
        self.updates()
            .filter(|update| update["sessionUpdate"] == "agent_message_chunk")
            .filter_map(|update| update["content"]["text"].as_str())
            .collect()
    }
    fn stop_reason(&self) -> &Value {
        &self.response["result"]["stopReason"]
    }
    fn error_code(&self) -> Option<i64> {
        self.response["error"]["code"].as_i64()
    }
}

/// Sends one phone request and answers every consent with `answer` until its response arrives.
async fn run(socket: &mut Socket, request: Value, answer: Answer) -> Turn {
    let id = request["id"].clone();
    send(socket, request).await;
    let mut events = Vec::new();
    let mut consents = Vec::new();
    loop {
        let value = next_value(socket).await;
        let message = &value["message"];
        if message["id"] == id && message.get("method").is_none() {
            return Turn {
                response: message.clone(),
                events,
                consents,
            };
        }
        if message["method"] == "session/request_permission" && message.get("id").is_some() {
            let source = message["params"]["_meta"]["acpdSource"]
                .as_str()
                .unwrap_or("agent")
                .to_owned();
            let wanted = match answer {
                Answer::Allow => ["allow_once", "allow_always"],
                Answer::Deny => ["reject_once", "reject_always"],
            };
            let option = message["params"]["options"]
                .as_array()
                .unwrap()
                .iter()
                .find(|option| wanted.contains(&option["kind"].as_str().unwrap_or("")))
                .unwrap_or_else(|| panic!("no {wanted:?} option in {message}"));
            let option_id = option["optionId"].as_str().unwrap().to_owned();
            send(
                socket,
                json!({"jsonrpc":"2.0","id":message["id"],"result":{"outcome":{"outcome":"selected","optionId":option_id}}}),
            )
            .await;
            consents.push((source, option_id));
        }
        events.push(value);
    }
}

/// The SDK's stdio MCP test server, built next to testy by `scripts/build-testy.sh`
/// (`ACPD_TESTY_MCP_BIN` overrides the path).
fn mcp_echo_server(testy: &Path) -> Option<PathBuf> {
    let path = std::env::var_os("ACPD_TESTY_MCP_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| testy.with_file_name("mcp-echo-server"));
    if path.is_file() {
        Some(path)
    } else {
        eprintln!(
            "skipping MCP part: {} not found; rebuild with scripts/build-testy.sh",
            path.display()
        );
        None
    }
}

fn prompt(session: &Value, id: &str, text: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":{"sessionId":session["acpSessionId"],"prompt":[{"type":"text","text":text}]}})
}

fn has_consent(turn: &Turn, source: &str, option: &str) -> bool {
    turn.consents
        .iter()
        .any(|(seen, chosen)| seen == source && chosen == option)
}

#[tokio::test]
async fn testy_echo_wait_for_cancel_and_cancel_status() {
    let Some(testy) = testy() else { return };
    let fixture = Fixture::start(&testy, None).await;
    let session = fixture.create().await;
    let mut socket = fixture.connect(session["id"].as_str().unwrap()).await;

    let echo = run(
        &mut socket,
        prompt(&session, "echo", "echo hello from acpd"),
        Answer::Deny,
    )
    .await;
    assert_eq!(echo.stop_reason(), "end_turn", "{}", echo.response);
    assert_eq!(echo.agent_text(), "hello from acpd");

    // wait_for_cancel only returns after session/cancel; the host forwards the phone's cancel.
    send(&mut socket, prompt(&session, "wait", "wait_for_cancel")).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    send(
        &mut socket,
        json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session["acpSessionId"]}}),
    )
    .await;
    let cancelled = next_until(&mut socket, |value| {
        value["message"]["id"] == "wait" && value["message"].get("method").is_none()
    })
    .await;
    assert_eq!(
        cancelled["message"]["result"]["stopReason"], "cancelled",
        "{cancelled}"
    );

    let status = run(
        &mut socket,
        prompt(&session, "status", "cancel_status"),
        Answer::Deny,
    )
    .await;
    assert_eq!(status.stop_reason(), "end_turn", "{}", status.response);
    assert!(
        status.agent_text().contains("cancel_status: not_cancelled"),
        "{}",
        status.agent_text()
    );
    fixture.stop().await;
}

#[tokio::test]
async fn testy_session_updates_tool_calls_modes_config_and_auth_pass_through() {
    let Some(testy) = testy() else { return };
    let fixture = Fixture::start(&testy, None).await;
    let session = fixture.create().await;
    // session/new setup (modes and config options) is kept for the phone.
    let setup = &session["setup"];
    assert_eq!(setup["modes"]["currentModeId"], "chat", "{setup}");
    assert!(
        setup["configOptions"]
            .as_array()
            .is_some_and(|options| !options.is_empty())
    );
    let mut socket = fixture.connect(session["id"].as_str().unwrap()).await;

    let updates = run(
        &mut socket,
        prompt(&session, "updates", "session_updates"),
        Answer::Deny,
    )
    .await;
    assert_eq!(updates.stop_reason(), "end_turn", "{}", updates.response);
    // testy v3.2.0 sends 19 updates for this scenario (checked with a stdio tap); none may be
    // dropped by the host's upstream validation. Update the count if the pinned rev changes.
    assert_eq!(updates.agent_update_count(), 19);
    let kinds = updates.update_kinds();
    for expected in [
        "session_info_update",
        "current_mode_update",
        "config_option_update",
        "available_commands_update",
        "usage_update",
        "agent_message_chunk",
        "agent_thought_chunk",
        "plan",
        "tool_call",
        "tool_call_update",
    ] {
        assert!(kinds.contains(expected), "missing {expected} in {kinds:?}");
    }

    let tools = run(
        &mut socket,
        prompt(&session, "tools", "tool_calls"),
        Answer::Deny,
    )
    .await;
    assert_eq!(tools.stop_reason(), "end_turn", "{}", tools.response);
    assert_eq!(tools.agent_update_count(), 7);
    let tool_kinds = tools.update_kinds();
    assert!(tool_kinds.contains("tool_call") && tool_kinds.contains("tool_call_update"));
    assert!(tools.consents.is_empty(), "tool_calls asks for nothing");

    let content = run(
        &mut socket,
        prompt(&session, "content", "content"),
        Answer::Deny,
    )
    .await;
    assert_eq!(content.stop_reason(), "end_turn", "{}", content.response);
    // User, thought and text chunks, image, audio, resource link and embedded resource, plus
    // the reply: every stable ContentBlock variant reaches the phone.
    assert_eq!(content.agent_update_count(), 8);

    let acp = &session["acpSessionId"];
    let mode = run(
        &mut socket,
        json!({"jsonrpc":"2.0","id":"mode","method":"session/set_mode","params":{"sessionId":acp,"modeId":"plan"}}),
        Answer::Deny,
    )
    .await;
    assert!(mode.response.get("result").is_some(), "{}", mode.response);
    let bad_mode = run(
        &mut socket,
        json!({"jsonrpc":"2.0","id":"bad-mode","method":"session/set_mode","params":{"sessionId":acp,"modeId":"invented"}}),
        Answer::Deny,
    )
    .await;
    assert_eq!(bad_mode.error_code(), Some(-32602), "{}", bad_mode.response);
    let config = run(
        &mut socket,
        json!({"jsonrpc":"2.0","id":"config","method":"session/set_config_option","params":{"sessionId":acp,"configId":"verbosity","value":"verbose"}}),
        Answer::Deny,
    )
    .await;
    assert!(
        config.response["result"]["configOptions"].is_array(),
        "{}",
        config.response
    );
    let method = session["initialization"]["authMethods"][0]["id"].clone();
    let login = run(
        &mut socket,
        json!({"jsonrpc":"2.0","id":"login","method":"authenticate","params":{"methodId":method}}),
        Answer::Deny,
    )
    .await;
    assert_eq!(login.response["result"], json!({}), "{}", login.response);
    let logout = run(
        &mut socket,
        json!({"jsonrpc":"2.0","id":"logout","method":"logout","params":{}}),
        Answer::Deny,
    )
    .await;
    assert!(
        logout.response.get("result").is_some(),
        "{}",
        logout.response
    );
    fixture.stop().await;
}

/// acpd does not advertise elicitation, so testy refuses its elicitation scenario with a
/// deterministic invalid-params prompt error and the session stays usable.
#[tokio::test]
async fn testy_elicitation_is_not_advertised_and_fails_cleanly() {
    let Some(testy) = testy() else { return };
    let fixture = Fixture::start(&testy, None).await;
    let session = fixture.create().await;
    let mut socket = fixture.connect(session["id"].as_str().unwrap()).await;
    let elicit = run(
        &mut socket,
        prompt(&session, "elicit", "elicitations"),
        Answer::Allow,
    )
    .await;
    assert_eq!(elicit.error_code(), Some(-32602), "{}", elicit.response);
    assert!(elicit.consents.is_empty());
    let echo = run(
        &mut socket,
        prompt(&session, "after", "echo still usable"),
        Answer::Deny,
    )
    .await;
    assert_eq!(echo.agent_text(), "still usable");
    fixture.stop().await;
}

/// testy's callbacks use fixed `/tmp` paths. With a different workspace the host refuses the
/// file and terminal callbacks without asking the phone; only testy's own permission request
/// is forwarded.
#[tokio::test]
async fn testy_callbacks_outside_the_workspace_are_refused_without_consent() {
    let Some(testy) = testy() else { return };
    let fixture = Fixture::start(&testy, None).await;
    let session = fixture.create().await;
    let mut socket = fixture.connect(session["id"].as_str().unwrap()).await;
    let callbacks = run(
        &mut socket,
        prompt(&session, "callbacks", "callbacks"),
        Answer::Allow,
    )
    .await;
    // The callbacks run, then the elicitation part fails the prompt (not advertised).
    assert_eq!(
        callbacks.error_code(),
        Some(-32602),
        "{}",
        callbacks.response
    );
    assert_eq!(callbacks.consents.len(), 1, "{:?}", callbacks.consents);
    assert!(has_consent(&callbacks, "agent", "allow_once"));
    fixture.stop().await;
}

/// The full callback set inside an authorized workspace (`/tmp`, because testy's paths are
/// fixed): agent permission, host write consent, read, terminal create/output/wait/kill/
/// release, first approved and then denied. Linux only (`/tmp` is a symlink on macOS).
#[cfg(target_os = "linux")]
#[tokio::test]
async fn testy_callbacks_and_full_inside_the_workspace_follow_phone_decisions() {
    let Some(testy) = testy() else { return };
    // The upstream agent uses fixed paths. Refuse to run against the machine's /tmp.
    // scripts/run-testy-isolated.sh creates a fresh tmpfs in a private mount namespace.
    assert_eq!(
        std::env::var("ACPD_TESTY_PRIVATE_TMP").as_deref(),
        Ok("1"),
        "run this fixture with scripts/run-testy-isolated.sh"
    );
    assert!(
        std::fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                fields.get(4) == Some(&"/tmp") && line.contains(" - tmpfs ")
            }),
        "testy requires a separate tmpfs mounted at /tmp"
    );
    assert_eq!(
        std::fs::read_to_string("/tmp/acpd-testy-private-mount").unwrap(),
        "acpd-testy isolated fixture\n"
    );
    let target = Path::new("/tmp/testy-write.txt");
    assert!(
        !target.exists(),
        "fresh private /tmp must not contain the callback target"
    );
    let fixture = Fixture::start(&testy, Some(PathBuf::from("/tmp"))).await;
    let session = fixture.create().await;
    let mut socket = fixture.connect(session["id"].as_str().unwrap()).await;

    let allowed = run(
        &mut socket,
        prompt(&session, "allowed", "callbacks"),
        Answer::Allow,
    )
    .await;
    assert_eq!(allowed.error_code(), Some(-32602), "{}", allowed.response);
    assert!(
        has_consent(&allowed, "agent", "allow_once"),
        "{:?}",
        allowed.consents
    );
    assert!(has_consent(&allowed, "host-filesystem", "acpd-write-allow"));
    assert!(has_consent(&allowed, "host-terminal", "acpd-command-allow"));
    assert_eq!(
        std::fs::read_to_string(target).unwrap(),
        "written by testy\n"
    );
    let terminal_output = allowed.updates().any(|update| {
        update["_meta"]["acpdTerminal"]["output"]
            .as_str()
            .is_some_and(|output| output.contains("testy terminal"))
    });
    assert!(
        terminal_output,
        "host published the approved command's output"
    );

    std::fs::write(target, "before deny\n").unwrap();
    let denied = run(
        &mut socket,
        prompt(&session, "denied", "callbacks"),
        Answer::Deny,
    )
    .await;
    assert_eq!(denied.error_code(), Some(-32602), "{}", denied.response);
    assert!(
        has_consent(&denied, "agent", "reject_once"),
        "{:?}",
        denied.consents
    );
    assert!(has_consent(&denied, "host-filesystem", "acpd-write-deny"));
    assert!(has_consent(&denied, "host-terminal", "acpd-command-deny"));
    assert_eq!(std::fs::read_to_string(target).unwrap(), "before deny\n");

    let full = run(&mut socket, prompt(&session, "full", "full"), Answer::Allow).await;
    assert_eq!(full.error_code(), Some(-32602), "{}", full.response);
    let kinds = full.update_kinds();
    for expected in [
        "usage_update",
        "available_commands_update",
        "plan",
        "tool_call_update",
    ] {
        assert!(kinds.contains(expected), "missing {expected} in {kinds:?}");
    }
    // Two commands (tool-call content terminal and the callback terminal) each needed approval.
    let commands = full
        .consents
        .iter()
        .filter(|(source, _)| source == "host-terminal")
        .count();
    assert_eq!(commands, 2, "{:?}", full.consents);
    assert_eq!(
        std::fs::read_to_string(target).unwrap(),
        "written by testy\n"
    );
    fixture.stop().await;
}

/// A custom registry entry (testy) through the same checks as `acpd doctor --agent`:
/// sign-in without a prompt, then the opt-in provider check with one prompt.
#[tokio::test]
async fn testy_doctor_reports_ready_and_the_opt_in_prompt_check_passes() {
    let Some(testy) = testy() else { return };
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let config = Config {
        workspace_roots: vec![workspace.clone()],
        state_directory: directory.path().join("state"),
        ..Default::default()
    };
    let registry = || {
        Registry::parse(
            &json!([{"id":"custom-testy","name":"Custom testy","command":testy,"args":[],"transport":"stdio"}])
                .to_string(),
        )
        .unwrap()
    };
    let (message, passed) =
        acpd::doctor::agent_check(&config, registry(), "custom-testy", &workspace, false).await;
    assert!(passed, "{message}");
    assert!(
        message.contains("ready") && message.contains("no prompt sent"),
        "{message}"
    );
    let (message, passed) =
        acpd::doctor::agent_check(&config, registry(), "custom-testy", &workspace, true).await;
    assert!(passed, "{message}");
    assert!(
        message.contains("provider responded (stopReason end_turn)"),
        "{message}"
    );
    // testy's greeting must not be printed; doctor reports only the stop reason.
    assert!(!message.contains("Hello"), "{message}");
}

/// A phone-supplied stdio MCP definition reaches the agent unchanged: testy starts the SDK's
/// echo MCP server from it, lists its tools and calls one. The env value stays out of the
/// session metadata the phone receives.
#[tokio::test]
async fn testy_uses_a_phone_supplied_stdio_mcp_server() {
    let Some(testy) = testy() else { return };
    let Some(server) = mcp_echo_server(&testy) else {
        return;
    };
    let fixture = Fixture::start(&testy, None).await;
    let definitions = json!([{"name":"echo","command":server,"args":[],"env":[{"name":"ACPD_MCP_MARKER","value":"synthetic-mcp-secret"}]}]);
    let session = fixture.create_with_mcp(definitions).await;
    assert!(
        !session.to_string().contains("synthetic-mcp-secret"),
        "{session}"
    );
    let mut socket = fixture.connect(session["id"].as_str().unwrap()).await;
    let tools = run(
        &mut socket,
        prompt(
            &session,
            "tools",
            r#"{"command":"list_tools","server":"echo"}"#,
        ),
        Answer::Deny,
    )
    .await;
    assert_eq!(tools.stop_reason(), "end_turn", "{}", tools.response);
    let listed = tools.agent_text();
    assert!(
        listed.starts_with("Available tools:") && listed.contains("echo"),
        "{listed}"
    );
    let call = run(
        &mut socket,
        prompt(
            &session,
            "call",
            r#"{"command":"call_tool","server":"echo","tool":"echo","params":{"message":"through acpd"}}"#,
        ),
        Answer::Deny,
    )
    .await;
    assert_eq!(call.stop_reason(), "end_turn", "{}", call.response);
    let reply = call.agent_text();
    assert!(
        reply.starts_with("OK:") && reply.contains("through acpd"),
        "{reply}"
    );
    // An unknown server name is the agent's own error, reported as text by testy.
    let missing = run(
        &mut socket,
        prompt(
            &session,
            "missing",
            r#"{"command":"list_tools","server":"absent"}"#,
        ),
        Answer::Deny,
    )
    .await;
    assert!(
        missing.agent_text().starts_with("ERROR:"),
        "{}",
        missing.agent_text()
    );
    assert!(tools.consents.is_empty() && call.consents.is_empty());
    fixture.stop().await;
}
