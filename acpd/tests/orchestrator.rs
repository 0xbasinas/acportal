//! Delegation through real sessions: the orchestrator, the control channel (a Unix socket or a
//! Windows named pipe) and the mock agent. The mock agent waits for a permission answer in every
//! turn, which is what keeps a delegated child "working" until the test acts as the phone.
use acpd::{
    config::{Config, OrchestratorConfig, RuntimeConfig, SupervisorRisk},
    connection::{AcpConnection, WorkspaceAccess},
    orchestrator::{
        Orchestrator,
        control::{self, ControlBackend, Endpoint, Stream},
        delegate::{Backend, ToolFailure},
    },
    registry::Registry,
    session::{AcpSession, SessionManager},
};
use serde_json::{Value, json};
use std::{future::Future, sync::Arc, time::Duration};
use tempfile::TempDir;
use tokio::time::timeout;

struct Fixture {
    manager: Arc<SessionManager>,
    orchestrator: Arc<Orchestrator>,
    _endpoint: Endpoint,
    workspace: TempDir,
    _state: TempDir,
}
type Agent = ControlBackend<Box<dyn Stream>>;

/// Mock-agent variants: what permission each one raises, and whether it declares its own tools.
const AGENTS: [(&str, &[&str], bool); 8] = [
    ("test-agent", &["--echo-mcp"], false),
    ("worker", &["--echo-mcp"], false),
    ("plain", &["--echo-mcp"], false),
    (
        "reader",
        &["--echo-mcp", "--permission-kind", "read"],
        false,
    ),
    (
        "deleter",
        &["--echo-mcp", "--permission-kind", "delete"],
        false,
    ),
    (
        "outside",
        &["--echo-mcp", "--permission-path", "../../elsewhere.rs"],
        false,
    ),
    (
        "secret",
        &["--echo-mcp", "--permission-path", ".env"],
        false,
    ),
    ("owntool", &["--echo-mcp"], true),
];

fn fixture(tweak: impl FnOnce(&mut Config)) -> Fixture {
    let _ = capture();
    let workspace = tempfile::tempdir().unwrap();
    for directory in ["sub", "sub/deeper", "sub/deeper/deepest", "other"] {
        std::fs::create_dir_all(workspace.path().join(directory)).unwrap();
    }
    let state = tempfile::tempdir().unwrap();
    let mut config = Config {
        workspace_roots: vec![workspace.path().into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 10,
            max_sessions: 8,
            ..Default::default()
        },
        orchestrator: OrchestratorConfig {
            enabled: true,
            orchestrate: vec!["test-agent".into()],
            delegable: AGENTS
                .iter()
                .map(|(id, _, _)| (*id).to_owned())
                .filter(|id| id != "plain")
                .collect(),
            ..Default::default()
        },
        ..Default::default()
    };
    tweak(&mut config);
    let agents: Vec<Value> = AGENTS
        .iter()
        .map(|(id, args, own_tools)| {
            json!({"id":id,"name":format!("{id} agent"),"ownTools":own_tools,
                "command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args})
        })
        .collect();
    let registry = Registry::parse(&serde_json::to_string(&agents).unwrap()).unwrap();
    let manager = Arc::new(SessionManager::new(config.clone(), registry).unwrap());
    let orchestrator =
        Orchestrator::with_cancel_grace(Arc::downgrade(&manager), Duration::from_millis(500));
    manager.attach_orchestrator(orchestrator.clone());
    let endpoint = control::start(orchestrator.clone(), state.path()).unwrap();
    Fixture {
        manager,
        orchestrator,
        _endpoint: endpoint,
        workspace,
        _state: state,
    }
}

impl Fixture {
    async fn root(&self, agent: &str) -> Arc<AcpSession> {
        self.manager
            .create(agent, self.workspace.path())
            .await
            .unwrap()
    }
    async fn root_in(
        &self,
        agent: &str,
        directory: &str,
        access: WorkspaceAccess,
    ) -> Arc<AcpSession> {
        self.manager
            .open_configured(
                agent,
                &self.workspace.path().join(directory),
                None,
                vec![],
                access,
            )
            .await
            .unwrap()
    }
    async fn children_of(&self, agent: &str) -> Vec<Arc<AcpSession>> {
        let mut found = vec![];
        for metadata in self.manager.list().await {
            if metadata.agent_id == agent
                && let Ok(session) = self.manager.get(metadata.id).await
            {
                found.push(session);
            }
        }
        found
    }
}

/// The delegation MCP server the host added to this session, as the agent saw it.
fn delegate_definition(session: &AcpSession) -> Option<Value> {
    session.metadata().setup["_meta"]["mockMcpFull"]
        .as_array()?
        .iter()
        .find(|server| server["name"] == "acpd-orchestrator")
        .cloned()
}
fn env_of(definition: &Value, name: &str) -> String {
    definition["env"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == name)
        .unwrap()["value"]
        .as_str()
        .unwrap()
        .to_owned()
}
async fn agent_of(session: &AcpSession) -> Agent {
    let definition = delegate_definition(session).expect("the session has delegation tools");
    let path = env_of(&definition, "ACPD_CONTROL");
    let token = env_of(&definition, "ACPD_TASK_TOKEN");
    let stream = control::connect(std::path::Path::new(&path)).await.unwrap();
    ControlBackend::new(stream, token)
}
async fn ok(agent: &Agent, tool: &str, arguments: Value) -> Value {
    match agent.call(tool, arguments).await {
        Ok(value) => value,
        Err(failure) => panic!("{tool} failed: {} {}", failure.code, failure.message),
    }
}
async fn code(agent: &Agent, tool: &str, arguments: Value) -> String {
    match agent.call(tool, arguments).await {
        Ok(value) => panic!("{tool} unexpectedly succeeded: {value}"),
        Err(ToolFailure { code, .. }) => code,
    }
}
async fn eventually<T, F, Fut>(mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    timeout(Duration::from_secs(15), async {
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
async fn state_of(agent: &Agent, task: &str) -> String {
    let status = ok(agent, "task_status", json!({"task_id": task})).await;
    status["tasks"][0]["state"].as_str().unwrap().to_owned()
}
async fn reaches(agent: &Agent, task: &str, wanted: &str) {
    eventually(|| async { (state_of(agent, task).await == wanted).then_some(()) }).await;
}
/// Act as the phone: wait for the child's pending permission and answer it.
async fn answer(connection: &AcpConnection, option: &str) {
    let permission =
        eventually(|| async { connection.replay(0).pending_permissions.first().cloned() }).await;
    connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":option}),
        )
        .await
        .unwrap();
}
async fn child_session(fixture: &Fixture, agent: &str) -> Arc<AcpSession> {
    eventually(|| async { fixture.children_of(agent).await.into_iter().next() }).await
}

#[tokio::test]
async fn only_listed_agents_get_delegation_tools_and_nothing_without_the_feature() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let definition = delegate_definition(&root).expect("orchestrating agent gets the tools");
    assert_eq!(definition["args"], json!(["mcp-delegate"]));
    assert!(std::path::Path::new(definition["command"].as_str().unwrap()).is_absolute());
    assert_eq!(env_of(&definition, "ACPD_TASK_TOKEN").len(), 64);
    // Not in the orchestrate list: an ordinary session, untouched.
    let plain = fx.root("plain").await;
    assert!(delegate_definition(&plain).is_none());
    // The token never reaches saved metadata or the session summary the phone reads.
    let token = env_of(&definition, "ACPD_TASK_TOKEN");
    let listing = serde_json::to_string(
        &fx.manager
            .list()
            .await
            .iter()
            .map(|m| {
                let mut m = m.clone();
                m.setup = Value::Null; // the mock echoes its own env into setup; the host never stores it
                m
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(!listing.contains(&token));
    fx.manager.shutdown().await;

    let disabled = fixture(|config| config.orchestrator.enabled = false);
    let root = disabled.root("test-agent").await;
    assert!(delegate_definition(&root).is_none());
    disabled.manager.shutdown().await;
}

#[tokio::test]
async fn a_delegated_task_runs_completes_returns_untrusted_text_and_takes_a_follow_up() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;

    let listed = ok(&agent, "agents_list", json!({})).await;
    let refs: Vec<_> = listed["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["ref"].as_str().unwrap())
        .collect();
    assert!(refs.contains(&"worker") && refs.contains(&"test-agent"));
    assert!(
        !refs.contains(&"plain"),
        "agents not listed as delegable stay hidden"
    );

    let started = ok(
        &agent,
        "task_delegate",
        json!({"agent":"worker","prompt":"inspect","workspace":"sub"}),
    )
    .await;
    let task = started["task_id"].as_str().unwrap().to_owned();
    assert!(matches!(
        started["state"].as_str().unwrap(),
        "submitted" | "working"
    ));

    let child = child_session(&fx, "worker").await;
    assert_eq!(
        child.metadata().workspace,
        fx.workspace.path().join("sub").canonicalize().unwrap()
    );
    reaches(&agent, &task, "working").await;
    answer(&child.connection, "allow").await;
    reaches(&agent, &task, "completed").await;

    let result = ok(&agent, "task_result", json!({"task_id": task})).await;
    assert_eq!(result["untrusted"], true);
    assert_eq!(result["stop_reason"], "end_turn");
    assert_eq!(result["truncated"], false);
    assert!(
        result["result"]
            .as_str()
            .unwrap()
            .contains("Mock lifecycle complete")
    );
    // A bounded read really is bounded.
    let short = ok(
        &agent,
        "task_result",
        json!({"task_id": task, "max_bytes": 5}),
    )
    .await;
    assert_eq!(short["result"].as_str().unwrap().len(), 5);
    assert_eq!(short["truncated"], true);

    // Follow-up on the same child.
    let sent = ok(
        &agent,
        "task_send",
        json!({"task_id": task, "prompt":"again"}),
    )
    .await;
    assert_eq!(sent["state"], "working");
    assert_eq!(
        code(
            &agent,
            "task_send",
            json!({"task_id": task, "prompt":"racing"})
        )
        .await,
        "invalid_state"
    );
    answer(&child.connection, "allow").await;
    reaches(&agent, &task, "completed").await;

    let all = ok(&agent, "task_status", json!({})).await;
    assert_eq!(all["tasks"].as_array().unwrap().len(), 1);
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn a_refused_permission_still_completes_with_the_agents_own_words() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let started = ok(
        &agent,
        "task_delegate",
        json!({"agent":"worker","prompt":"edit","workspace":"sub","wait_seconds":0}),
    )
    .await;
    let task = started["task_id"].as_str().unwrap();
    let child = child_session(&fx, "worker").await;
    answer(&child.connection, "deny").await;
    reaches(&agent, task, "completed").await;
    let result = ok(&agent, "task_result", json!({"task_id": task})).await;
    assert!(
        result["result"]
            .as_str()
            .unwrap()
            .contains("No change was approved")
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn waiting_returns_early_when_a_decision_is_needed_and_again_when_it_finishes() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let started = std::time::Instant::now();
    let first = ok(
        &agent,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","workspace":"sub","wait_seconds":60}),
    )
    .await;
    assert_eq!(first["state"], "input_required");
    assert_eq!(first["needs_decision"].as_array().unwrap().len(), 1);
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "returned when a decision was needed, not at the timeout"
    );
    let task = first["task_id"].as_str().unwrap().to_owned();
    // The phone answers while the supervisor waits on the task's status.
    let child = child_session(&fx, "worker").await;
    let phone = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        answer(&child.connection, "allow").await;
    });
    let waited = std::time::Instant::now();
    let done = ok(
        &agent,
        "task_status",
        json!({"task_id": task, "wait_seconds": 60}),
    )
    .await;
    assert!(waited.elapsed() < Duration::from_secs(30));
    // The first change is the request going away or the state moving on; either way keep waiting
    // until it has finished.
    let mut state = done["tasks"][0]["state"].as_str().unwrap().to_owned();
    while state != "completed" {
        let next = ok(
            &agent,
            "task_status",
            json!({"task_id": task, "wait_seconds": 20}),
        )
        .await;
        state = next["tasks"][0]["state"].as_str().unwrap().to_owned();
    }
    phone.await.unwrap();
    let result = ok(&agent, "task_result", json!({"task_id": task})).await;
    assert!(
        result["result"]
            .as_str()
            .unwrap()
            .contains("Mock lifecycle complete")
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn every_admission_code_is_reported_and_leaves_no_session_behind() {
    let fx = fixture(|config| config.orchestrator.max_children_per_task = 1);
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let base = json!({"prompt":"x"});
    let with = |extra: Value| {
        let mut args = base.clone();
        for (key, value) in extra.as_object().unwrap() {
            args[key] = value.clone();
        }
        args
    };
    assert_eq!(
        code(&agent, "task_delegate", with(json!({"agent":"plain"}))).await,
        "agent_not_delegable"
    );
    assert_eq!(
        code(&agent, "task_delegate", with(json!({"agent":"nobody"}))).await,
        "agent_not_delegable"
    );
    assert_eq!(
        code(&agent, "task_delegate", with(json!({"agent":"test-agent"}))).await,
        "cycle"
    );
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            with(json!({"agent":"worker","workspace":"../other"}))
        )
        .await,
        "scope_not_allowed"
    );
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            with(json!({"agent":"worker","workspace":"/etc"}))
        )
        .await,
        "scope_not_allowed"
    );
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            with(json!({"agent":"worker","workspace":"missing"}))
        )
        .await,
        "workspace_unavailable"
    );
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            with(json!({"agent":"worker","isolation":"worktree"}))
        )
        .await,
        "isolation_unsupported"
    );
    assert_eq!(code(&agent, "task_decide", json!({"task_id":"123e4567-e89b-42d3-a456-426614174000","request_id":"1","digest":"a".repeat(64),"decision":"deny","rationale":"r"})).await, "unknown_task");
    assert_eq!(
        fx.manager.active_count().await,
        1,
        "refusals started no process"
    );

    // Fan-out: one open child allowed, the second is refused.
    ok(
        &agent,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","workspace":"sub"}),
    )
    .await;
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            with(json!({"agent":"worker","workspace":"other"}))
        )
        .await,
        "fan_out_limit"
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn rights_cannot_exceed_the_callers_and_default_to_read_only() {
    let fx = fixture(|_| {});
    let read_only = WorkspaceAccess {
        read_files: true,
        write_files: false,
        terminal: false,
        form_elicitation: false,
    };
    let root = fx.root_in("test-agent", "", read_only).await;
    let agent = agent_of(&root).await;
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            json!({"agent":"worker","prompt":"x","workspace":"sub","rights":{"write":true}})
        )
        .await,
        "rights_not_allowed"
    );
    ok(
        &agent,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","workspace":"sub"}),
    )
    .await;
    let child = child_session(&fx, "worker").await;
    let access = child.metadata().workspace_access;
    assert!(
        access.read_files && !access.write_files && !access.terminal,
        "{access:?}"
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn the_session_limit_applies_to_children() {
    let fx = fixture(|config| config.runtime.max_sessions = 1);
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            json!({"agent":"worker","prompt":"x","workspace":"sub"})
        )
        .await,
        "session_limit"
    );
    assert_eq!(fx.manager.active_count().await, 1);
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn cancel_ends_the_child_and_the_whole_subtree() {
    let fx = fixture(|config| {
        config.orchestrator.orchestrate = vec!["test-agent".into(), "worker".into()]
    });
    let root = fx.root("test-agent").await;
    let top = agent_of(&root).await;
    let started = ok(
        &top,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","workspace":"sub"}),
    )
    .await;
    let child_task = started["task_id"].as_str().unwrap().to_owned();
    let child = child_session(&fx, "worker").await;
    let middle = agent_of(&child).await;
    let grand = ok(
        &middle,
        "task_delegate",
        json!({"agent":"worker","prompt":"y","workspace":"deeper","rights":{"read":true}}),
    )
    .await;
    let grand_task = grand["task_id"].as_str().unwrap().to_owned();
    assert_eq!(fx.children_of("worker").await.len(), 2);
    // A third level is one deeper than the default limit of two.
    let grandchild = eventually(|| async {
        fx.children_of("worker")
            .await
            .into_iter()
            .find(|s| s.metadata().workspace.ends_with("deeper"))
    })
    .await;
    let bottom = agent_of(&grandchild).await;
    assert_eq!(
        code(
            &bottom,
            "task_delegate",
            json!({"agent":"test-agent","prompt":"z","workspace":"deepest"})
        )
        .await,
        "depth_limit"
    );
    // A child cannot manage its own parent's task, only below itself.
    assert_eq!(
        code(&middle, "task_cancel", json!({"task_id": child_task})).await,
        "not_authorized_for_task"
    );

    let cancelled = ok(&top, "task_cancel", json!({"task_id": child_task})).await;
    assert_eq!(
        cancelled["cancelled"].as_array().unwrap().len(),
        2,
        "the child and its child"
    );
    reaches(&top, &child_task, "canceled").await;
    // The grandchild is below the root's child, so the root may look at it too.
    reaches(&top, &grand_task, "canceled").await;
    // A cancelled task's token stops working.
    assert_eq!(
        code(&middle, "task_status", json!({})).await,
        "invalid_task_token"
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn the_deadline_cancels_a_task_that_never_finishes() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let started = ok(
        &agent,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","workspace":"sub","deadline_seconds":1}),
    )
    .await;
    let task = started["task_id"].as_str().unwrap();
    // Nobody answers the permission, so the deadline is what ends it.
    reaches(&agent, task, "failed").await;
    let status = ok(&agent, "task_status", json!({"task_id": task})).await;
    assert_eq!(status["tasks"][0]["summary"], "the task's deadline passed");
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn trees_are_isolated_and_tokens_die_with_their_session() {
    let fx = fixture(|_| {});
    let first = fx
        .root_in("test-agent", "sub", WorkspaceAccess::default())
        .await;
    let second = fx
        .root_in("test-agent", "other", WorkspaceAccess::default())
        .await;
    let a = agent_of(&first).await;
    let b = agent_of(&second).await;
    let started = ok(
        &a,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","workspace":"deeper"}),
    )
    .await;
    let task = started["task_id"].as_str().unwrap().to_owned();

    assert_eq!(
        code(&b, "task_status", json!({"task_id": task})).await,
        "not_authorized_for_task"
    );
    assert_eq!(
        code(&b, "task_cancel", json!({"task_id": task})).await,
        "not_authorized_for_task"
    );
    assert_eq!(
        code(&b, "task_result", json!({"task_id": task})).await,
        "not_authorized_for_task"
    );
    assert_eq!(
        code(
            &b,
            "task_status",
            json!({"task_id":"123e4567-e89b-42d3-a456-426614174000"})
        )
        .await,
        "unknown_task"
    );
    assert_eq!(ok(&b, "task_status", json!({})).await["tasks"], json!([]));

    // Removing the first root takes its child down, and its token with it.
    let child = child_session(&fx, "worker").await;
    let child_id = child.metadata().id;
    drop(child);
    fx.manager.remove(first.metadata().id).await.unwrap();
    assert!(
        fx.manager.get(child_id).await.is_err(),
        "no child outlives its root"
    );
    assert_eq!(
        code(&a, "task_status", json!({})).await,
        "invalid_task_token"
    );
    // The other tree is untouched.
    assert_eq!(ok(&b, "task_status", json!({})).await["tasks"], json!([]));
    assert_eq!(fx.manager.active_count().await, 1);
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn the_host_does_not_trust_the_delegate_process() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    // A client can skip `mcp-delegate` and write to the endpoint itself; arguments are re-checked.
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            json!({"agent":"worker","prompt":""})
        )
        .await,
        "invalid_arguments"
    );
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            json!({"agent":"worker","prompt":"x","extra":1})
        )
        .await,
        "invalid_arguments"
    );
    assert_eq!(
        code(
            &agent,
            "task_delegate",
            json!({"agent":"worker","prompt":"x","wait_seconds":9999})
        )
        .await,
        "invalid_arguments"
    );
    assert_eq!(code(&agent, "rm_rf", json!({})).await, "unknown_tool");
    assert_eq!(
        code(&agent, "task_status", json!("not an object")).await,
        "invalid_arguments"
    );
    // A wrong token is refused before anything else is looked at.
    let definition = delegate_definition(&root).unwrap();
    let stream = control::connect(std::path::Path::new(&env_of(&definition, "ACPD_CONTROL")))
        .await
        .unwrap();
    let stranger: Agent = ControlBackend::new(stream, "0".repeat(64));
    assert_eq!(
        code(&stranger, "agents_list", json!({})).await,
        "invalid_task_token"
    );
    // Garbage on the wire gets an error reply, not a crash or a hang.
    let orchestrator_reply = control_garbage(&definition).await;
    assert_eq!(orchestrator_reply["code"], "invalid_request");
    assert_eq!(fx.manager.active_count().await, 1);
    fx.manager.shutdown().await;
}

async fn control_garbage(definition: &Value) -> Value {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut stream = control::connect(std::path::Path::new(&env_of(definition, "ACPD_CONTROL")))
        .await
        .unwrap();
    stream.write_all(b"{this is not json}\n").await.unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    timeout(Duration::from_secs(5), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn the_real_mcp_delegate_process_reaches_the_host_end_to_end() {
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        process::Command,
    };
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let definition = delegate_definition(&root).unwrap();
    // The host injects its own executable (the test binary here); run the real acpd instead.
    let mut delegate = Command::new(env!("CARGO_BIN_EXE_acpd"))
        .arg("mcp-delegate")
        .env("ACPD_CONTROL", env_of(&definition, "ACPD_CONTROL"))
        .env("ACPD_TASK_TOKEN", env_of(&definition, "ACPD_TASK_TOKEN"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = delegate.stdin.take().unwrap();
    let mut output = BufReader::new(delegate.stdout.take().unwrap());
    let calls = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"agents_list","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"task_delegate","arguments":{"agent":"plain","prompt":"x"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"task_delegate","arguments":{"agent":"worker","prompt":"x","workspace":"sub"}}}),
    ];
    let mut replies = vec![];
    for call in &calls {
        input
            .write_all(format!("{call}\n").as_bytes())
            .await
            .unwrap();
        if call.get("id").is_some() {
            let mut line = String::new();
            timeout(Duration::from_secs(15), output.read_line(&mut line))
                .await
                .unwrap()
                .unwrap();
            replies.push(serde_json::from_str::<Value>(&line).unwrap());
        }
    }
    assert_eq!(
        replies[0]["result"]["serverInfo"]["name"],
        "acpd-orchestrator"
    );
    let listed: Vec<&str> = replies[1]["result"]["structuredContent"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|agent| agent["ref"].as_str().unwrap())
        .collect();
    assert!(listed.contains(&"worker"), "{listed:?}");
    assert_eq!(replies[2]["result"]["isError"], true);
    assert_eq!(
        replies[2]["result"]["structuredContent"]["code"],
        "agent_not_delegable"
    );
    assert_eq!(replies[3]["result"]["isError"], false);
    assert_eq!(replies[3]["result"]["structuredContent"]["agent"], "worker");
    assert_eq!(
        fx.children_of("worker").await.len(),
        1,
        "a real child session was started"
    );
    drop(input);
    timeout(Duration::from_secs(10), delegate.wait())
        .await
        .unwrap()
        .unwrap();
    // stdout carried MCP messages only; the token never appears on stderr either.
    let mut stderr = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut delegate.stderr.take().unwrap(), &mut stderr)
        .await
        .unwrap();
    assert!(!stderr.contains(&env_of(&definition, "ACPD_TASK_TOKEN")));
    fx.manager.shutdown().await;
}

// ---- Supervisor approvals (phase 2) ----------------------------------------------------------

use acpd::orchestrator::approvals;

async fn start(agent: &Agent, name: &str) -> String {
    let started = ok(
        agent,
        "task_delegate",
        json!({"agent": name, "prompt": "go", "workspace": "sub"}),
    )
    .await;
    started["task_id"].as_str().unwrap().to_owned()
}
/// The first request waiting on a decision, as the supervisor reads it from `task_status`.
async fn pending(agent: &Agent, task: &str) -> Value {
    eventually(|| async {
        let status = ok(agent, "task_status", json!({"task_id": task})).await;
        status["tasks"][0]["needs_decision"]
            .as_array()
            .and_then(|list| list.first().cloned())
    })
    .await
}
fn decision(task: &str, request: &Value, choice: &str, why: &str) -> Value {
    json!({"task_id": task, "request_id": request["request_id"], "digest": request["digest"],
           "decision": choice, "rationale": why})
}
/// What the host told the phone about automatic decisions on this session.
fn reviews(session: &AcpSession) -> Vec<Value> {
    session
        .replay(0)
        .events
        .iter()
        .filter(|event| event.direction == "host")
        .filter_map(|event| {
            let review = &event.message["params"]["update"]["_meta"]["acpdAutoReview"];
            review.is_object().then(|| review.clone())
        })
        .collect()
}
async fn finish_by_phone(fx: &Fixture, agent_id: &str) {
    let child = child_session(fx, agent_id).await;
    answer(&child.connection, "allow").await;
}

#[tokio::test]
async fn the_supervisor_approves_a_medium_request_and_the_child_finishes() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let request = pending(&agent, &task).await;
    assert_eq!(request["risk"], "medium");
    assert_eq!(request["decidable"], true);
    assert_eq!(request["source"], "agent");
    assert_eq!(request["kind"], "edit");
    assert_eq!(request["facts"]["paths"], json!(["example.rs"]));
    assert_eq!(request["digest"].as_str().unwrap().len(), 64);
    assert_eq!(state_of(&agent, &task).await, "input_required");

    let applied = ok(
        &agent,
        "task_decide",
        decision(
            &task,
            &request,
            "allow_once",
            "small edit inside the workspace",
        ),
    )
    .await;
    assert_eq!(applied, json!({"applied": true, "outcome": "allowed"}));
    reaches(&agent, &task, "completed").await;
    let result = ok(&agent, "task_result", json!({"task_id": task})).await;
    assert!(
        result["result"]
            .as_str()
            .unwrap()
            .contains("Mock lifecycle complete")
    );
    assert_eq!(result["needs_decision"], json!([]));

    let child = child_session(&fx, "worker").await;
    let seen = reviews(&child);
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["layer"], "supervisor");
    assert_eq!(seen[0]["decision"], "allow_once");
    assert_eq!(seen[0]["reason"], "small edit inside the workspace");
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn a_supervisor_refusal_reaches_the_agent() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let request = pending(&agent, &task).await;
    let applied = ok(
        &agent,
        "task_decide",
        decision(&task, &request, "deny", "not needed for this task"),
    )
    .await;
    assert_eq!(applied["outcome"], "denied");
    reaches(&agent, &task, "completed").await;
    let result = ok(&agent, "task_result", json!({"task_id": task})).await;
    assert!(
        result["result"]
            .as_str()
            .unwrap()
            .contains("No change was approved")
    );
    let child = child_session(&fx, "worker").await;
    assert_eq!(reviews(&child)[0]["decision"], "deny");
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn wrong_digests_unknown_requests_and_replays_change_nothing() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let request = pending(&agent, &task).await;

    let mut stale = decision(&task, &request, "allow_once", "r");
    stale["digest"] = json!("a".repeat(64));
    assert_eq!(code(&agent, "task_decide", stale).await, "digest_mismatch");
    let mut unknown = decision(&task, &request, "allow_once", "r");
    unknown["request_id"] = json!("no-such-request");
    assert_eq!(
        code(&agent, "task_decide", unknown).await,
        "unknown_request"
    );
    // Neither touched the request: it is still waiting, unchanged.
    assert_eq!(pending(&agent, &task).await, request);
    assert_eq!(state_of(&agent, &task).await, "input_required");

    ok(
        &agent,
        "task_decide",
        decision(&task, &request, "allow_once", "r"),
    )
    .await;
    // A replay of the same decision, even with the right digest, does nothing.
    assert_eq!(
        code(
            &agent,
            "task_decide",
            decision(&task, &request, "allow_once", "r")
        )
        .await,
        "already_answered"
    );
    assert_eq!(
        code(
            &agent,
            "task_decide",
            decision(&task, &request, "deny", "r")
        )
        .await,
        "already_answered"
    );
    reaches(&agent, &task, "completed").await;
    let child = child_session(&fx, "worker").await;
    assert_eq!(reviews(&child).len(), 1);
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn when_the_phone_answers_first_the_late_decision_changes_nothing() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let request = pending(&agent, &task).await;
    let child = child_session(&fx, "worker").await;
    answer(&child.connection, "deny").await; // the phone refuses
    assert_eq!(
        code(
            &agent,
            "task_decide",
            decision(&task, &request, "allow_once", "late")
        )
        .await,
        "already_answered"
    );
    reaches(&agent, &task, "completed").await;
    let result = ok(&agent, "task_result", json!({"task_id": task})).await;
    assert!(
        result["result"]
            .as_str()
            .unwrap()
            .contains("No change was approved"),
        "the phone's answer stood"
    );
    assert!(
        reviews(&child).is_empty(),
        "no supervisor outcome is claimed"
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn the_first_answer_wins_when_phone_and_supervisor_race() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let child = child_session(&fx, "worker").await;
    for round in 0..8 {
        if round > 0 {
            ok(
                &agent,
                "task_send",
                json!({"task_id": task, "prompt": "again"}),
            )
            .await;
        }
        let request = pending(&agent, &task).await;
        let id = json!(request["request_id"].as_str().unwrap());
        let (supervisor, phone) = tokio::join!(
            agent.call(
                "task_decide",
                decision(&task, &request, "allow_once", "race")
            ),
            child
                .connection
                .permission(id, json!({"outcome":"selected","optionId":"allow"}))
        );
        assert!(
            supervisor.is_ok() != phone.is_ok(),
            "round {round}: exactly one answer may win (supervisor {supervisor:?}, phone {phone:?})"
        );
        if let Err(loser) = supervisor {
            assert_eq!(loser.code, "already_answered", "round {round}");
        }
        reaches(&agent, &task, "completed").await;
    }
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn escalating_leaves_the_request_to_the_phone_with_the_reasoning_attached() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let request = pending(&agent, &task).await;
    let out = ok(
        &agent,
        "task_decide",
        decision(
            &task,
            &request,
            "escalate",
            "I cannot tell if this edit is safe",
        ),
    )
    .await;
    assert_eq!(out, json!({"applied": false, "outcome": "escalated"}));
    let after = pending(&agent, &task).await;
    assert_eq!(after["decidable"], false);
    assert_eq!(after["reason"], "escalated");
    assert_eq!(
        code(
            &agent,
            "task_decide",
            decision(&task, &request, "allow_once", "changed my mind")
        )
        .await,
        "escalated"
    );
    assert_eq!(
        state_of(&agent, &task).await,
        "input_required",
        "still waiting for the phone"
    );
    let child = child_session(&fx, "worker").await;
    let seen = reviews(&child);
    assert_eq!(seen[0]["decision"], "escalate");
    assert_eq!(seen[0]["reason"], "I cannot tell if this edit is safe");
    answer(&child.connection, "allow").await;
    reaches(&agent, &task, "completed").await;
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn risk_classes_and_the_ceiling_decide_who_may_answer() {
    // (agent, ceiling, risk, may the supervisor decide)
    let cases = [
        ("reader", SupervisorRisk::Medium, "low", true),
        ("worker", SupervisorRisk::Medium, "medium", true),
        ("deleter", SupervisorRisk::Medium, "high", false),
        ("outside", SupervisorRisk::Medium, "high", false),
        ("reader", SupervisorRisk::Low, "low", true),
        ("worker", SupervisorRisk::Low, "medium", false),
    ];
    for (name, ceiling, risk, decidable) in cases {
        let fx = fixture(|config| config.orchestrator.supervisor_max_risk = ceiling);
        let root = fx.root("test-agent").await;
        let agent = agent_of(&root).await;
        let task = start(&agent, name).await;
        let request = pending(&agent, &task).await;
        let label = format!("{name} {ceiling:?}");
        assert_eq!(request["risk"], risk, "{label}");
        assert_eq!(request["decidable"], decidable, "{label}");
        let attempt = agent
            .call("task_decide", decision(&task, &request, "allow_once", "r"))
            .await;
        if decidable {
            attempt.unwrap_or_else(|f| panic!("{label}: {f:?}"));
            reaches(&agent, &task, "completed").await;
        } else {
            assert_eq!(attempt.unwrap_err().code, "risk_too_high", "{label}");
            assert_eq!(request["reason"], "risk_too_high", "{label}");
            finish_by_phone(&fx, name).await; // the phone still can
            reaches(&agent, &task, "completed").await;
        }
        fx.manager.shutdown().await;
    }
}

#[tokio::test]
async fn with_the_supervisor_turned_off_nothing_changes_for_the_phone() {
    let fx = fixture(|config| config.orchestrator.supervisor_max_risk = SupervisorRisk::None);
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let child = child_session(&fx, "worker").await;
    eventually(|| async { (!child.connection.pending_permissions().is_empty()).then_some(()) })
        .await;
    let status = ok(&agent, "task_status", json!({"task_id": task})).await;
    assert_eq!(status["tasks"][0]["needs_decision"], json!([]));
    assert_eq!(
        status["tasks"][0]["state"], "working",
        "no watcher, no input_required tracking"
    );
    let fake = json!({"task_id": task, "request_id": "permission-1", "digest": "a".repeat(64),
                      "decision": "allow_once", "rationale": "r"});
    assert_eq!(code(&agent, "task_decide", fake).await, "unknown_request");
    answer(&child.connection, "allow").await;
    reaches(&agent, &task, "completed").await;
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn credential_paths_are_refused_without_asking_anyone() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "secret").await; // the child asks to edit `.env`
    reaches(&agent, &task, "completed").await;
    let result = ok(&agent, "task_result", json!({"task_id": task})).await;
    assert!(
        result["result"]
            .as_str()
            .unwrap()
            .contains("No change was approved")
    );
    assert_eq!(
        result["needs_decision"],
        json!([]),
        "a refused request is never offered"
    );
    let child = child_session(&fx, "secret").await;
    let seen = reviews(&child);
    assert_eq!(seen[0]["layer"], "rules");
    assert_eq!(seen[0]["decision"], "deny");
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn an_agent_with_its_own_tools_is_never_decided_by_a_supervisor() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "owntool").await;
    let request = pending(&agent, &task).await;
    assert_eq!(request["decidable"], false);
    assert_eq!(request["reason"], "human_only");
    assert!(request["note"].as_str().unwrap().contains("own tools"));
    assert_eq!(
        code(
            &agent,
            "task_decide",
            decision(&task, &request, "allow_once", "r")
        )
        .await,
        "not_decidable"
    );
    finish_by_phone(&fx, "owntool").await;
    reaches(&agent, &task, "completed").await;
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn only_the_parent_may_decide_and_never_for_itself() {
    let fx = fixture(|config| {
        config.orchestrator.orchestrate = vec!["test-agent".into(), "worker".into()]
    });
    let first = fx
        .root_in("test-agent", "sub", WorkspaceAccess::default())
        .await;
    let second = fx
        .root_in("test-agent", "other", WorkspaceAccess::default())
        .await;
    let a = agent_of(&first).await;
    let b = agent_of(&second).await;
    let started = ok(
        &a,
        "task_delegate",
        json!({"agent":"worker","prompt":"go","workspace":"deeper"}),
    )
    .await;
    let task = started["task_id"].as_str().unwrap().to_owned();
    let request = pending(&a, &task).await;
    // Another tree cannot decide it, and an unknown task is just unknown.
    assert_eq!(
        code(
            &b,
            "task_decide",
            decision(&task, &request, "allow_once", "r")
        )
        .await,
        "not_authorized_for_task"
    );
    let nobody = json!({"task_id":"123e4567-e89b-42d3-a456-426614174000","request_id":"1",
        "digest":"a".repeat(64),"decision":"deny","rationale":"r"});
    assert_eq!(code(&b, "task_decide", nobody).await, "unknown_task");
    // The child itself holds a token (it may delegate) but cannot approve its own request.
    let child = child_session(&fx, "worker").await;
    let own = agent_of(&child).await;
    assert_eq!(
        code(
            &own,
            "task_decide",
            decision(&task, &request, "allow_once", "r")
        )
        .await,
        "not_authorized_for_task"
    );
    assert_eq!(
        pending(&a, &task).await,
        request,
        "none of that touched the request"
    );
    ok(
        &a,
        "task_decide",
        decision(&task, &request, "allow_once", "the parent decides"),
    )
    .await;
    reaches(&a, &task, "completed").await;
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn the_supervisors_time_runs_out_and_then_only_the_phone_decides() {
    let fx = fixture(|config| config.orchestrator.supervisor_timeout_seconds = 1);
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let request = pending(&agent, &task).await;
    tokio::time::sleep(Duration::from_millis(1300)).await;
    let late = pending(&agent, &task).await;
    assert_eq!(late["decidable"], false);
    assert_eq!(late["reason"], "expired");
    assert_eq!(
        code(
            &agent,
            "task_decide",
            decision(&task, &request, "allow_once", "too late")
        )
        .await,
        "supervisor_timeout"
    );
    finish_by_phone(&fx, "worker").await;
    reaches(&agent, &task, "completed").await;
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn pausing_a_root_stops_supervisor_approvals_until_resumed() {
    let fx = fixture(|_| {});
    let root = fx.root("test-agent").await;
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let request = pending(&agent, &task).await;
    let root_id = root.metadata().id;
    assert!(fx.orchestrator.set_supervisor_paused(root_id, true));
    let paused = pending(&agent, &task).await;
    assert_eq!(paused["decidable"], false);
    assert_eq!(paused["reason"], "paused");
    assert_eq!(
        code(
            &agent,
            "task_decide",
            decision(&task, &request, "allow_once", "r")
        )
        .await,
        "supervisor_paused"
    );
    // Only a phone-started root can be paused.
    let child = child_session(&fx, "worker").await;
    assert!(
        !fx.orchestrator
            .set_supervisor_paused(child.metadata().id, true)
    );
    assert!(
        !fx.orchestrator
            .set_supervisor_paused(uuid::Uuid::new_v4(), true)
    );
    assert!(fx.orchestrator.set_supervisor_paused(root_id, false));
    ok(
        &agent,
        "task_decide",
        decision(&task, &request, "allow_once", "resumed"),
    )
    .await;
    reaches(&agent, &task, "completed").await;
    fx.manager.shutdown().await;
}

#[derive(Clone, Default)]
struct Capture(Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Capture {
        self.clone()
    }
}

/// One process-wide capture, installed before any test emits: a per-thread subscriber races
/// with `tracing`'s per-callsite caching when tests run on several threads.
fn capture() -> &'static Capture {
    static CAPTURE: std::sync::OnceLock<Capture> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let capture = Capture::default();
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(capture.clone())
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .finish(),
        );
        capture
    })
}

#[tokio::test]
async fn logs_record_the_decision_but_never_the_reasoning_paths_or_tokens() {
    let fx = fixture(|_| {});
    let capture = capture();
    let root = fx.root("test-agent").await;
    let token = env_of(&delegate_definition(&root).unwrap(), "ACPD_TASK_TOKEN");
    let agent = agent_of(&root).await;
    let task = start(&agent, "worker").await;
    let request = pending(&agent, &task).await;
    ok(
        &agent,
        "task_decide",
        decision(&task, &request, "allow_once", "SECRET-RATIONALE-4411"),
    )
    .await;
    reaches(&agent, &task, "completed").await;
    let secret = start(&agent, "secret").await; // and one refused by rules
    reaches(&agent, &secret, "completed").await;
    fx.manager.shutdown().await;

    let log = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("supervisor decision"),
        "the decision itself is logged:\n{log}"
    );
    assert!(
        log.contains("refused"),
        "so is an automatic refusal:\n{log}"
    );
    for hidden in [
        "SECRET-RATIONALE-4411",
        token.as_str(),
        request["digest"].as_str().unwrap(),
        "example.rs",
        ".env",
        "Mock lifecycle complete",
    ] {
        assert!(!log.contains(hidden), "{hidden:?} must not be logged");
    }
}

#[tokio::test]
async fn the_connection_knows_which_permissions_the_host_raised() {
    let fx = fixture(|_| {});
    let session = fx.root("plain").await;
    let scope = session.metadata().workspace;
    // A request the agent raises itself.
    let prompting = session.clone();
    let prompt = tokio::spawn(async move { prompting.prompt("go").await });
    let from_agent =
        eventually(|| async { session.connection.pending_permissions().into_iter().next() }).await;
    assert!(!from_agent.host_origin);
    let class = approvals::classify(
        &from_agent.message,
        from_agent.host_origin,
        &scope,
        false,
        1000,
    );
    assert_eq!(class.source, "agent");
    session.cancel().await.unwrap();
    prompt.await.unwrap().unwrap();
    // A terminal consent the host raises on the agent's behalf.
    let acp = session.metadata().acp_session_id;
    let asking = session.clone();
    let create = tokio::spawn(async move {
        asking
            .connection
            .request(
                "_mock/terminal",
                json!({"method":"terminal/create","request":{"sessionId":acp,
                    "command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--terminal-fixture"]}}),
            )
            .await
    });
    let from_host =
        eventually(|| async { session.connection.pending_permissions().into_iter().next() }).await;
    assert!(from_host.host_origin);
    assert_eq!(
        from_host.message["params"]["_meta"]["acpdSource"],
        "host-terminal"
    );
    let class = approvals::classify(
        &from_host.message,
        from_host.host_origin,
        &scope,
        false,
        1000,
    );
    assert_eq!(class.source, "host-terminal");
    assert_eq!(class.risk, approvals::Risk::High);
    session
        .connection
        .permission(
            from_host.message["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-command-deny"}),
        )
        .await
        .unwrap();
    let _ = create.await;
    // The same words in an agent's request prove nothing: with the host-origin flag off it is
    // just an agent request.
    let spoof = approvals::classify(&from_host.message, false, &scope, false, 1000);
    assert_eq!(spoof.source, "agent");
    fx.manager.shutdown().await;
}
