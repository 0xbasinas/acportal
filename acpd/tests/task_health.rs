//! Whether a task's delegation tools really started, whether its trace follows it, and whether
//! its processes really end: the parts of "a task is running" the host can check for itself.
use acpd::{
    config::{Config, OrchestratorConfig, RuntimeConfig},
    connection::WorkspaceAccess,
    orchestrator::{
        Orchestrator,
        control::{self, ControlBackend, Endpoint, Stream},
        delegate::Backend,
    },
    registry::Registry,
    session::{AcpSession, SessionManager},
};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};
use tracing_subscriber::fmt::MakeWriter;

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
impl<'a> MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Capture {
        self.clone()
    }
}
/// One process-wide log capture, installed before any test emits.
fn log() -> &'static Capture {
    static CAPTURE: OnceLock<Capture> = OnceLock::new();
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

struct Life {
    manager: Arc<SessionManager>,
    orchestrator: Arc<Orchestrator>,
    workspace: PathBuf,
    _endpoint: Endpoint,
    _directory: tempfile::TempDir,
}
type Agent = ControlBackend<Box<dyn Stream>>;

fn start(worker_args: &[&str], tweak: impl FnOnce(&mut OrchestratorConfig)) -> Life {
    let _ = log();
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    for sub in ["sub", "other"] {
        std::fs::create_dir_all(workspace.join(sub)).unwrap();
    }
    let mut settings = OrchestratorConfig {
        enabled: true,
        orchestrate: vec!["test-agent".into()],
        delegable: vec!["worker".into()],
        ..Default::default()
    };
    tweak(&mut settings);
    let config = Config {
        workspace_roots: vec![workspace.clone()],
        state_directory: directory.path().join("state"),
        runtime: RuntimeConfig {
            request_timeout_seconds: 10,
            max_sessions: 8,
            ..Default::default()
        },
        orchestrator: settings,
        ..Default::default()
    };
    let mut worker = vec!["--echo-mcp"];
    worker.extend_from_slice(worker_args);
    let agents = json!([
        {"id":"test-agent","name":"root","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--echo-mcp"]},
        {"id":"worker","name":"worker","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":worker},
    ]);
    let manager = Arc::new(
        SessionManager::new(
            config.clone(),
            Registry::parse(&agents.to_string()).unwrap(),
        )
        .unwrap(),
    );
    let orchestrator =
        Orchestrator::with_cancel_grace(Arc::downgrade(&manager), Duration::from_millis(500));
    manager.attach_orchestrator(orchestrator.clone());
    let endpoint = control::start(orchestrator.clone(), &config.state_directory).unwrap();
    Life {
        manager,
        orchestrator,
        workspace,
        _endpoint: endpoint,
        _directory: directory,
    }
}

async fn eventually<T, F, Fut>(mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    timeout(Duration::from_secs(25), async {
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
fn definition(session: &AcpSession) -> Value {
    session.metadata().setup["_meta"]["mockMcpFull"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["name"] == "acpd-orchestrator").cloned())
        .expect("delegation tools were offered")
}
fn env_of(definition: &Value, name: &str) -> String {
    definition["env"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap()["value"]
        .as_str()
        .unwrap()
        .to_owned()
}
async fn agent_of(session: &AcpSession) -> Agent {
    let definition = definition(session);
    let stream = control::connect(Path::new(&env_of(&definition, "ACPD_CONTROL")))
        .await
        .unwrap();
    ControlBackend::new(stream, env_of(&definition, "ACPD_TASK_TOKEN"))
}
async fn root(life: &Life) -> (Arc<AcpSession>, Agent) {
    let session = life
        .manager
        .open_configured(
            "test-agent",
            &life.workspace,
            None,
            vec![],
            WorkspaceAccess::default(),
        )
        .await
        .unwrap();
    let agent = agent_of(&session).await;
    (session, agent)
}
async fn call(agent: &Agent, tool: &str, arguments: Value) -> Value {
    match agent.call(tool, arguments).await {
        Ok(value) => value,
        Err(failure) => panic!("{tool} failed: {} {}", failure.code, failure.message),
    }
}
async fn delegate(agent: &Agent, extra: Value) -> String {
    let mut arguments = json!({"agent":"worker","prompt":"go","workspace":"sub"});
    for (key, value) in extra.as_object().unwrap() {
        arguments[key] = value.clone();
    }
    call(agent, "task_delegate", arguments).await["task_id"]
        .as_str()
        .unwrap()
        .to_owned()
}
async fn reaches(agent: &Agent, task: &str, wanted: &str) -> Value {
    eventually(|| async {
        let status = call(agent, "task_status", json!({"task_id": task})).await;
        let task = status["tasks"][0].clone();
        (task["state"] == wanted).then_some(task)
    })
    .await
}
async fn worker_session(life: &Life) -> Arc<AcpSession> {
    eventually(|| async {
        for metadata in life.manager.list().await {
            if metadata.agent_id == "worker"
                && let Ok(session) = life.manager.get(metadata.id).await
            {
                return Some(session);
            }
        }
        None
    })
    .await
}
fn detail_of(life: &Life, task: uuid::Uuid) -> Value {
    life.orchestrator
        .task_detail(task)
        .expect("the task exists")
}
fn task_of_session(life: &Life, session: uuid::Uuid) -> uuid::Uuid {
    life.orchestrator
        .tasks()
        .into_iter()
        .find(|record| record.session == Some(session))
        .expect("a task for that session")
        .id
}

// ---- 1. did the delegation tools really start? --------------------------------------------

#[tokio::test]
async fn the_host_learns_whether_the_delegation_tools_really_started() {
    let life = start(&[], |_| {});
    let (first, _) = root(&life).await;
    let (second, _) = root(&life).await;
    let (a, b) = (
        task_of_session(&life, first.metadata().id),
        task_of_session(&life, second.metadata().id),
    );
    // Offered the tools, but nothing has launched them: not connected.
    assert_eq!(detail_of(&life, a)["delegate"], "not_connected");
    let listed: Vec<Value> = life.orchestrator.task_list();
    assert!(
        listed.iter().all(|t| t["delegate"] == "not_connected"),
        "{listed:?}"
    );

    // Launch the real delegate the way an agent would, with the environment it was given.
    let definition = definition(&first);
    let mut delegate = Command::new(env!("CARGO_BIN_EXE_acpd"))
        .arg("mcp-delegate")
        .env("ACPD_CONTROL", env_of(&definition, "ACPD_CONTROL"))
        .env("ACPD_TASK_TOKEN", env_of(&definition, "ACPD_TASK_TOKEN"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    eventually(|| async { (detail_of(&life, a)["delegate"] == "connected").then_some(()) }).await;
    assert_eq!(
        detail_of(&life, b)["delegate"],
        "not_connected",
        "only the task whose tools started"
    );

    // It is the real thing, and `_hello` is not a tool an agent can call.
    let mut input = delegate.stdin.take().unwrap();
    let mut output = BufReader::new(delegate.stdout.take().unwrap());
    for request in [
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"agents_list","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"_hello","arguments":{}}}),
    ] {
        input
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
    }
    let mut lines = vec![];
    for _ in 0..2 {
        let mut line = String::new();
        timeout(Duration::from_secs(10), output.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        lines.push(serde_json::from_str::<Value>(&line).unwrap());
    }
    assert_eq!(lines[0]["result"]["isError"], false);
    assert_eq!(
        lines[1]["error"]["code"], -32602,
        "unknown tool: {}",
        lines[1]
    );

    // A hello needs a live token, and marks only its own task.
    let stranger = ControlBackend::new(
        control::connect(Path::new(&env_of(&definition, "ACPD_CONTROL")))
            .await
            .unwrap(),
        "0".repeat(64),
    );
    assert_eq!(
        stranger.call("_hello", json!({})).await.unwrap_err().code,
        "invalid_task_token"
    );
    assert_eq!(detail_of(&life, b)["delegate"], "not_connected");
    life.manager.shutdown().await;
}

#[tokio::test]
async fn children_and_sessions_without_the_tools_have_no_delegate_status() {
    let life = start(&[], |_| {});
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let child = life
        .orchestrator
        .task_detail(task.parse().unwrap())
        .unwrap();
    assert!(child.get("delegate").is_none(), "{child}");
    life.manager.shutdown().await;
}

// ---- 2. does the trace follow the task? -----------------------------------------------------

fn parts(traceparent: &str) -> (String, String) {
    let fields: Vec<&str> = traceparent.split('-').collect();
    assert_eq!(fields.len(), 4, "{traceparent}");
    assert_eq!(
        (fields[0], fields[3], fields[1].len(), fields[2].len()),
        ("00", "01", 32, 16),
        "{traceparent}"
    );
    assert!(
        traceparent
            .bytes()
            .all(|b| b == b'-' || b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert!(fields[1].bytes().any(|b| b != b'0') && fields[2].bytes().any(|b| b != b'0'));
    (fields[1].into(), fields[2].into())
}

#[tokio::test]
async fn a_tree_shares_one_trace_and_each_task_has_its_own_span() {
    let life = start(&[], |_| {});
    let (top, agent) = root(&life).await;
    let (other, _) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let child = worker_session(&life).await;

    let root_trace = parts(
        top.metadata().setup["_meta"]["mockNewMeta"]["traceparent"]
            .as_str()
            .unwrap(),
    );
    let child_new = child.metadata().setup["_meta"]["mockNewMeta"]["traceparent"]
        .as_str()
        .unwrap()
        .to_owned();
    let child_trace = parts(&child_new);
    assert_eq!(root_trace.0, child_trace.0, "one trace for the whole tree");
    assert_ne!(root_trace.1, child_trace.1, "a span of its own");
    let other_trace = parts(
        other.metadata().setup["_meta"]["mockNewMeta"]["traceparent"]
            .as_str()
            .unwrap(),
    );
    assert_ne!(root_trace.0, other_trace.0, "another tree, another trace");

    // The prompt the agent receives carries the same traceparent as its session was made with.
    let prompt = child
        .connection
        .request("_mock/last_prompt", json!({}))
        .await
        .unwrap();
    assert_eq!(prompt["_meta"]["traceparent"], child_new.as_str());

    // The records keep the trace id, so it survives for `acpd tasks`.
    let records = life.orchestrator.tasks();
    let of = |id: &str| {
        records
            .iter()
            .find(|r| r.id.to_string() == id)
            .and_then(|r| r.trace_id.clone())
    };
    let root_id = records
        .iter()
        .find(|r| r.parent.is_none() && r.session == Some(top.metadata().id))
        .unwrap()
        .id
        .to_string();
    assert_eq!(of(&task), Some(root_trace.0.clone()));
    assert_eq!(of(&root_id), Some(root_trace.0.clone()));

    // State changes are logged with the trace id and the state, and with nothing the task said.
    let _ = call(&agent, "task_cancel", json!({"task_id": task})).await;
    reaches(&agent, &task, "canceled").await;
    let log = String::from_utf8(log().0.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("task state") && log.contains(&root_trace.0) && log.contains("canceled"),
        "{log}"
    );
    assert!(
        !log.contains("\"go\"") && !log.contains("Mock lifecycle"),
        "{log}"
    );
    life.manager.shutdown().await;
}

// ---- 3. do the processes really end? --------------------------------------------------------

/// A handle on a process taken while it is alive, so a reused id cannot be mistaken for it.
struct Watch {
    #[cfg(windows)]
    handle: std::os::windows::io::OwnedHandle,
    #[cfg(unix)]
    pid: u32,
}
impl Watch {
    #[cfg(windows)]
    fn new(pid: u32) -> Self {
        use std::os::windows::io::FromRawHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(
            !raw.is_null(),
            "process {pid} must exist before it is stopped"
        );
        Self {
            handle: unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(raw) },
        }
    }
    #[cfg(unix)]
    fn new(pid: u32) -> Self {
        Self { pid }
    }
    #[cfg(windows)]
    fn gone(&self) -> bool {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObject,
        };
        unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) == WAIT_OBJECT_0 }
    }
    #[cfg(unix)]
    fn gone(&self) -> bool {
        let missing = unsafe { libc::kill(self.pid as i32, 0) } != 0
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        // A process that has exited but is not yet reaped still answers `kill`; on Linux it shows
        // as a zombie in /proc and counts as stopped.
        let zombie =
            std::fs::read_to_string(format!("/proc/{}/stat", self.pid)).is_ok_and(|stat| {
                stat.rsplit(')')
                    .next()
                    .is_some_and(|rest| rest.trim_start().starts_with('Z'))
            });
        missing || zombie
    }
}
/// The agent's own process and the child and grandchild it starts.
async fn watch_agent(session: &AcpSession) -> Vec<Watch> {
    let own = session
        .connection
        .request("_mock/pid", json!({}))
        .await
        .unwrap()["pid"]
        .as_u64()
        .unwrap();
    let kids = session
        .connection
        .request("_mock/spawn_descendant", json!({}))
        .await
        .unwrap();
    [
        own,
        kids["childPid"].as_u64().unwrap(),
        kids["grandchildPid"].as_u64().unwrap(),
    ]
    .into_iter()
    .map(|pid| Watch::new(pid as u32))
    .collect()
}
async fn all_gone(watches: &[Watch]) {
    let ended = timeout(Duration::from_secs(20), async {
        while !watches.iter().all(Watch::gone) {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;
    // The agent, its child and its grandchild, in that order.
    let gone: Vec<bool> = watches.iter().map(Watch::gone).collect();
    assert!(
        ended.is_ok(),
        "still running after 20 s (agent, child, grandchild gone?): {gone:?}"
    );
}

#[tokio::test]
async fn cancelling_ends_the_turn_and_removing_the_root_ends_every_process() {
    let life = start(&[], |_| {});
    let (top, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let child = worker_session(&life).await;
    let watches = watch_agent(&child).await;
    call(&agent, "task_cancel", json!({"task_id": task})).await;
    reaches(&agent, &task, "canceled").await;
    // The turn ended; the session, and so its processes, are still there until it is removed.
    assert!(
        watches.iter().all(|w| !w.gone()),
        "cancel ends the turn, not the session"
    );
    life.manager.remove(top.metadata().id).await.unwrap();
    all_gone(&watches).await;
    life.manager.shutdown().await;
}

#[tokio::test]
async fn an_agent_that_ignores_cancel_is_ended_with_everything_it_started() {
    // Genuinely stuck: silent, and deaf to cancel. (An agent waiting on a permission would end its
    // turn anyway, because cancelling also answers that request as cancelled.)
    let life = start(&["--stall-prompt", "--ignore-cancel"], |_| {});
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({"deadline_seconds": 1})).await;
    let child = worker_session(&life).await;
    let watches = watch_agent(&child).await;
    let failed = reaches(&agent, &task, "failed").await;
    assert_eq!(failed["summary"], "the task's deadline passed");
    all_gone(&watches).await;
    life.manager.shutdown().await;
}

#[tokio::test]
async fn a_finished_child_that_is_swept_away_takes_its_processes_with_it() {
    let life = start(&[], |c| c.finished_child_keep_seconds = 1);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let child = worker_session(&life).await;
    let watches = watch_agent(&child).await;
    let request =
        eventually(|| async { child.connection.pending_permissions().into_iter().next() }).await;
    child
        .connection
        .permission(
            request.message["id"].clone(),
            json!({"outcome":"selected","optionId":"allow"}),
        )
        .await
        .unwrap();
    reaches(&agent, &task, "completed").await;
    all_gone(&watches).await;
    life.manager.shutdown().await;
}
