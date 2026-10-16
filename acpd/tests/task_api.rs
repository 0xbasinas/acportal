//! The phone's view of the task tree over HTTP: `/v1/tasks`, detail, cancel, supervisor pause.
use acpd::{
    api::Host,
    config::{Config, OrchestratorConfig, RuntimeConfig},
    orchestrator::{
        control::{self, ControlBackend, Endpoint, Stream},
        delegate::Backend,
        records::{self, INTERRUPTED, RecordRights, TaskRecord},
    },
    registry::Registry,
    security::Role,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{task::JoinHandle, time::timeout};
use uuid::Uuid;

struct Harness {
    host: Host,
    addr: String,
    client: reqwest::Client,
    phone: String,
    workspace: PathBuf,
    task: JoinHandle<()>,
    _endpoint: Option<Endpoint>,
    _directory: tempfile::TempDir,
}
impl Harness {
    async fn start() -> Self {
        Self::start_with(true, |_| {}).await
    }
    async fn start_with(enabled: bool, before: impl FnOnce(&Path)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        for sub in ["sub", "other"] {
            std::fs::create_dir_all(workspace.join(sub)).unwrap();
        }
        let state = directory.path().join("state");
        before(&state);
        let config = Config {
            workspace_roots: vec![workspace.clone()],
            state_directory: state,
            runtime: RuntimeConfig {
                request_timeout_seconds: 10,
                max_sessions: 8,
                ..Default::default()
            },
            orchestrator: OrchestratorConfig {
                enabled,
                orchestrate: vec!["test-agent".into()],
                delegable: vec!["worker".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let agents: Vec<Value> = [("test-agent", vec!["--echo-mcp"]), ("worker", vec![])]
            .into_iter()
            .map(|(id, args)| json!({"id":id,"name":id,"command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args}))
            .collect();
        let host = Host::new(config, Registry::parse(&json!(agents).to_string()).unwrap()).unwrap();
        let endpoint = host.start_control_channel().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let router = host.router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut harness = Self {
            host,
            addr,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            phone: String::new(),
            workspace,
            task,
            _endpoint: endpoint,
            _directory: directory,
        };
        harness.phone = harness.pair(Role::Phone).await;
        harness
    }
    async fn pair(&self, role: Role) -> String {
        let code = self.host.security.new_pairing_code_for(role).unwrap();
        let reply: Value = self
            .client
            .post(format!("http://{}/v1/pair", self.addr))
            .json(&json!({"code": code, "deviceName": "test"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        reply["token"].as_str().unwrap().to_owned()
    }
    async fn get(&self, path: &str, token: Option<&str>) -> reqwest::Response {
        let mut request = self.client.get(format!("http://{}{path}", self.addr));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        request.send().await.unwrap()
    }
    async fn post(&self, path: &str, token: Option<&str>) -> reqwest::Response {
        let mut request = self.client.post(format!("http://{}{path}", self.addr));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        request.send().await.unwrap()
    }
    async fn json(&self, path: &str) -> Value {
        let reply = self.get(path, Some(&self.phone)).await;
        assert_eq!(reply.status(), 200, "{path}");
        reply.json().await.unwrap()
    }
    /// A root session the phone starts, and the control connection its agent would hold.
    async fn root(&self) -> (String, Agent) {
        let reply: Value = self
            .client
            .post(format!("http://{}/v1/sessions", self.addr))
            .bearer_auth(&self.phone)
            .json(&json!({"agentId":"test-agent","workspace":self.workspace}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let id: Uuid = reply["id"].as_str().unwrap().parse().unwrap();
        let session = self.host.sessions.get(id).await.unwrap();
        let definition = session.metadata().setup["_meta"]["mockMcpFull"]
            .as_array()
            .and_then(|s| s.iter().find(|s| s["name"] == "acpd-orchestrator").cloned())
            .expect("delegation tools");
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
        let stream = control::connect(Path::new(&env("ACPD_CONTROL")))
            .await
            .unwrap();
        (
            id.to_string(),
            ControlBackend::new(stream, env("ACPD_TASK_TOKEN")),
        )
    }
    async fn stop(self) {
        self.task.abort();
        self.host.sessions.shutdown().await;
    }
}
type Agent = ControlBackend<Box<dyn Stream>>;

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
async fn call(agent: &Agent, tool: &str, arguments: Value) -> Value {
    match agent.call(tool, arguments).await {
        Ok(value) => value,
        Err(failure) => panic!("{tool} failed: {} {}", failure.code, failure.message),
    }
}
/// Delegate to a worker that then waits for a permission, and return its task id.
async fn waiting_child(_h: &Harness, agent: &Agent, sub: &str) -> String {
    let started = call(
        agent,
        "task_delegate",
        json!({"agent":"worker","prompt":"go","workspace":sub,"wait_seconds":30}),
    )
    .await;
    assert_eq!(started["state"], "input_required", "{started}");
    started["task_id"].as_str().unwrap().to_owned()
}
fn root_of<'a>(tasks: &'a [Value], root: &str) -> &'a Value {
    tasks
        .iter()
        .find(|t| t["id"] == root)
        .expect("the root task")
}

#[tokio::test]
async fn the_phone_sees_the_tree_and_what_is_waiting_on_a_decision() {
    let h = Harness::start().await;
    let (root_session, agent) = h.root().await;
    let child = waiting_child(&h, &agent, "sub").await;
    let tasks = h.json("/v1/tasks").await;
    let tasks = tasks.as_array().unwrap();
    assert_eq!(tasks.len(), 2, "{tasks:?}");
    let parent = tasks
        .iter()
        .find(|t| t["parent"].is_null())
        .expect("a root");
    assert_eq!(parent["state"], "working");
    assert_eq!(parent["agent"], "test-agent");
    assert_eq!(parent["session"], root_session.as_str());
    assert_eq!(parent["pendingDecisions"], 0);
    let waiting = tasks.iter().find(|t| t["id"] == child.as_str()).unwrap();
    assert_eq!(waiting["parent"], parent["id"]);
    assert_eq!(waiting["state"], "input_required");
    assert_eq!(waiting["pendingDecisions"], 1);
    assert_eq!(waiting["depth"], 1);
    assert_eq!(
        waiting["rights"],
        json!({"read":true,"write":false,"terminal":false})
    );
    for key in ["createdAt", "updatedAt", "stopReason", "traceId", "summary"] {
        assert!(waiting.get(key).is_some(), "camelCase key {key} present");
    }
    // The detail carries the request, in the phone's naming.
    let detail = h.json(&format!("/v1/tasks/{child}")).await;
    let request = &detail["needsDecision"][0];
    assert_eq!(request["risk"], "medium");
    assert_eq!(request["decidable"], true);
    assert_eq!(request["digest"].as_str().unwrap().len(), 64);
    assert!(request.get("requestId").is_some() && request.get("expiresInSeconds").is_some());
    assert!(
        request.get("request_id").is_none(),
        "no agent-style snake_case keys: {request}"
    );
    h.stop().await;
}

#[tokio::test]
async fn only_the_phone_may_use_the_task_routes() {
    let h = Harness::start().await;
    let (_, agent) = h.root().await;
    let child = waiting_child(&h, &agent, "sub").await;
    let peer = h.pair(Role::Peer).await;
    for (method, path) in [
        ("GET", "/v1/tasks".to_owned()),
        ("GET", format!("/v1/tasks/{child}")),
        ("POST", format!("/v1/tasks/{child}/cancel")),
        ("POST", format!("/v1/tasks/{child}/merge")),
        ("POST", format!("/v1/tasks/{child}/supervisor/pause")),
        ("POST", format!("/v1/tasks/{child}/supervisor/resume")),
    ] {
        for (who, token, expected) in [("nobody", None, 401), ("a peer", Some(peer.as_str()), 403)]
        {
            let reply = if method == "GET" {
                h.get(&path, token).await
            } else {
                h.post(&path, token).await
            };
            assert_eq!(reply.status(), expected, "{who}: {method} {path}");
        }
    }
    // Nothing a refused caller tried changed anything.
    assert_eq!(
        h.json(&format!("/v1/tasks/{child}")).await["state"],
        "input_required"
    );
    h.stop().await;
}

#[tokio::test]
async fn unknown_tasks_are_not_found_and_a_malformed_id_is_too() {
    let h = Harness::start().await;
    let missing = Uuid::new_v4();
    for path in [
        format!("/v1/tasks/{missing}"),
        "/v1/tasks/not-a-uuid".to_owned(),
    ] {
        assert_eq!(h.get(&path, Some(&h.phone)).await.status(), 404, "{path}");
    }
    for path in [
        format!("/v1/tasks/{missing}/cancel"),
        format!("/v1/tasks/{missing}/merge"),
        format!("/v1/tasks/{missing}/supervisor/pause"),
    ] {
        assert_eq!(h.post(&path, Some(&h.phone)).await.status(), 404, "{path}");
    }
    h.stop().await;
}

#[tokio::test]
async fn the_phone_cancels_a_task_and_the_whole_subtree_below_a_root() {
    let h = Harness::start().await;
    let (root_session, agent) = h.root().await;
    let child = waiting_child(&h, &agent, "sub").await;

    let cancelled: Value = h
        .post(&format!("/v1/tasks/{child}/cancel"), Some(&h.phone))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(cancelled["cancelled"], json!([child]));
    eventually(|| async {
        (h.json(&format!("/v1/tasks/{child}")).await["state"] == "canceled").then_some(())
    })
    .await;
    // Ended already: refused, not silently accepted.
    let again = h
        .post(&format!("/v1/tasks/{child}/cancel"), Some(&h.phone))
        .await;
    assert_eq!(again.status(), 409);
    assert_eq!(
        again.json::<Value>().await.unwrap()["error"]["code"],
        "task_not_open"
    );

    // Cancelling the root cancels what is open below it and leaves the root session alone.
    let second = waiting_child(&h, &agent, "other").await;
    let tasks = h.json("/v1/tasks").await;
    let root_id = root_of(tasks.as_array().unwrap(), &root_session_task(&tasks)).clone();
    let reply: Value = h
        .post(
            &format!("/v1/tasks/{}/cancel", root_id["id"].as_str().unwrap()),
            Some(&h.phone),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(reply["cancelled"], json!([second]));
    eventually(|| async {
        (h.json(&format!("/v1/tasks/{second}")).await["state"] == "canceled").then_some(())
    })
    .await;
    assert_eq!(
        h.json(&format!("/v1/tasks/{}", root_id["id"].as_str().unwrap()))
            .await["state"],
        "working"
    );
    assert!(
        h.host
            .sessions
            .get(root_session.parse().unwrap())
            .await
            .is_ok(),
        "the root session is still there"
    );
    h.stop().await;
}
fn root_session_task(tasks: &Value) -> String {
    tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["parent"].is_null())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn the_phone_pauses_and_resumes_supervision_for_a_tree_by_its_root_task() {
    let h = Harness::start().await;
    let (_, agent) = h.root().await;
    let child = waiting_child(&h, &agent, "sub").await;
    let root = root_session_task(&h.json("/v1/tasks").await);
    let detail = h.json(&format!("/v1/tasks/{child}")).await;
    let request = detail["needsDecision"][0].clone();
    let decide = || {
        agent.call(
            "task_decide",
            json!({"task_id": child, "request_id": request["requestId"], "digest": request["digest"],
                   "decision": "deny", "rationale": "r"}),
        )
    };

    assert_eq!(
        h.post(
            &format!("/v1/tasks/{root}/supervisor/pause"),
            Some(&h.phone)
        )
        .await
        .status(),
        204
    );
    let paused = h.json(&format!("/v1/tasks/{child}")).await;
    assert_eq!(paused["needsDecision"][0]["decidable"], false);
    assert_eq!(paused["needsDecision"][0]["reason"], "paused");
    assert_eq!(decide().await.unwrap_err().code, "supervisor_paused");

    assert_eq!(
        h.post(
            &format!("/v1/tasks/{root}/supervisor/resume"),
            Some(&h.phone)
        )
        .await
        .status(),
        204
    );
    assert_eq!(
        h.json(&format!("/v1/tasks/{child}")).await["needsDecision"][0]["decidable"],
        true
    );
    assert!(decide().await.is_ok());

    // Only the root of a tree can be paused.
    let refused = h
        .post(
            &format!("/v1/tasks/{child}/supervisor/pause"),
            Some(&h.phone),
        )
        .await;
    assert_eq!(refused.status(), 409);
    assert_eq!(
        refused.json::<Value>().await.unwrap()["error"]["code"],
        "not_a_root_task"
    );
    h.stop().await;
}

#[tokio::test]
async fn a_restarted_host_shows_its_old_tasks_to_the_phone_as_interrupted() {
    let (root, child) = (Uuid::new_v4(), Uuid::new_v4());
    let record = |id: Uuid, parent: Option<Uuid>, state: &str| TaskRecord {
        id,
        parent,
        root,
        depth: u8::from(parent.is_some()),
        agent: if parent.is_some() {
            "worker"
        } else {
            "test-agent"
        }
        .into(),
        scope: PathBuf::from("/w"),
        rights: RecordRights {
            read: true,
            write: false,
            terminal: false,
        },
        state: state.into(),
        session: None,
        created_at: 1,
        updated_at: 1,
        summary: None,
        stop_reason: None,
        trace_id: None,
        cost_usd: None,
        worktree: None,
    };
    let h = Harness::start_with(true, |state| {
        records::save(
            state,
            &[
                record(root, None, "working"),
                record(child, Some(root), "input_required"),
            ],
        )
        .unwrap();
    })
    .await;
    let tasks = h.json("/v1/tasks").await;
    let tasks = tasks.as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    for task in tasks {
        assert_eq!(task["state"], "failed", "{task}");
        assert_eq!(task["summary"], INTERRUPTED);
    }
    h.stop().await;

    // With delegation off the routes exist and simply have nothing to show.
    let off = Harness::start_with(false, |_| {}).await;
    assert_eq!(off.json("/v1/tasks").await, json!([]));
    off.stop().await;
}

#[tokio::test]
async fn the_status_lists_the_orchestrator_feature_only_while_it_is_enabled() {
    for enabled in [true, false] {
        let h = Harness::start_with(enabled, |_| {}).await;
        let status = h.json("/v1/status").await;
        let features = status["features"].as_array().unwrap();
        assert!(features.contains(&json!("formElicitation")), "{status}");
        assert_eq!(
            features.contains(&json!("orchestrator")),
            enabled,
            "{status}"
        );
        h.stop().await;
    }
}
