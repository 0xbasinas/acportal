//! Task records across a host restart: what is saved, what comes back, and what never does.
//! Two host lifetimes share one state directory; the first is shut down with tasks still open.
use acpd::{
    api::Host,
    config::{Config, OrchestratorConfig, RuntimeConfig},
    connection::WorkspaceAccess,
    orchestrator::{
        Orchestrator,
        control::{self, ControlBackend, Endpoint, Stream},
        delegate::Backend,
        records::{self, INTERRUPTED, TaskRecord},
    },
    registry::Registry,
    session::{AcpSession, SessionManager},
};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};
use tokio::time::timeout;

const PROMPT: &str = "PROMPT-MARKER-7731";

struct Life {
    manager: Arc<SessionManager>,
    orchestrator: Arc<Orchestrator>,
    workspace: PathBuf,
    state: PathBuf,
    _endpoint: Endpoint,
}
/// One host lifetime on `root`: the same state directory every time.
fn start(root: &Path) -> Life {
    let workspace = root.join("workspace");
    for sub in ["sub", "other"] {
        std::fs::create_dir_all(workspace.join(sub)).unwrap();
    }
    let state = root.join("state");
    let config = Config {
        workspace_roots: vec![workspace.clone()],
        state_directory: state.clone(),
        runtime: RuntimeConfig {
            request_timeout_seconds: 10,
            max_sessions: 8,
            ..Default::default()
        },
        orchestrator: OrchestratorConfig {
            enabled: true,
            orchestrate: vec!["test-agent".into()],
            delegable: vec!["worker".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let agents: Vec<Value> = [("test-agent", vec!["--echo-mcp"]), ("worker", vec![])]
        .into_iter()
        .map(|(id, args)| {
            json!({"id":id,"name":id,"command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args})
        })
        .collect();
    let registry = Registry::parse(&json!(agents).to_string()).unwrap();
    let manager = Arc::new(SessionManager::new(config, registry).unwrap());
    let orchestrator =
        Orchestrator::with_cancel_grace(Arc::downgrade(&manager), Duration::from_millis(500));
    manager.attach_orchestrator(orchestrator.clone());
    orchestrator.restore(&state);
    let endpoint = control::start(orchestrator.clone(), &state).unwrap();
    Life {
        manager,
        orchestrator,
        workspace,
        state,
        _endpoint: endpoint,
    }
}

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

type Agent = ControlBackend<Box<dyn Stream>>;
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
async fn root_agent(life: &Life) -> (Arc<AcpSession>, Agent, String) {
    let root = life
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
    let definition = root.metadata().setup["_meta"]["mockMcpFull"]
        .as_array()
        .and_then(|servers| {
            servers
                .iter()
                .find(|s| s["name"] == "acpd-orchestrator")
                .cloned()
        })
        .unwrap();
    let token = env_of(&definition, "ACPD_TASK_TOKEN");
    let stream = control::connect(Path::new(&env_of(&definition, "ACPD_CONTROL")))
        .await
        .unwrap();
    (root, ControlBackend::new(stream, token.clone()), token)
}
async fn call(agent: &Agent, tool: &str, arguments: Value) -> Value {
    match agent.call(tool, arguments).await {
        Ok(value) => value,
        Err(failure) => panic!("{tool} failed: {} {}", failure.code, failure.message),
    }
}
async fn delegate(agent: &Agent, workspace: &str) -> String {
    call(
        agent,
        "task_delegate",
        json!({"agent":"worker","prompt":PROMPT,"workspace":workspace}),
    )
    .await["task_id"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn saved(life: &Life) -> Vec<TaskRecord> {
    records::load(&life.state).unwrap_or_default()
}
fn by_agent<'a>(records: &'a [TaskRecord], agent: &str) -> Vec<&'a TaskRecord> {
    records.iter().filter(|r| r.agent == agent).collect()
}

#[tokio::test]
async fn after_a_restart_open_tasks_show_as_interrupted_and_nothing_is_restarted_or_resent() {
    let root = tempfile::tempdir().unwrap();
    let first = start(root.path());
    let (root_session, agent, token) = root_agent(&first).await;
    // One child is left waiting for a permission; another is answered and completes.
    let open = delegate(&agent, "sub").await;
    let done = delegate(&agent, "other").await;
    let child = eventually(|| async {
        for metadata in first.manager.list().await {
            if metadata.agent_id == "worker" && metadata.workspace.ends_with("other") {
                return first.manager.get(metadata.id).await.ok();
            }
        }
        None
    })
    .await;
    let permission =
        eventually(|| async { child.connection.pending_permissions().into_iter().next() }).await;
    child
        .connection
        .permission(
            permission.message["id"].clone(),
            json!({"outcome":"selected","optionId":"allow"}),
        )
        .await
        .unwrap();
    // The records on disk follow along without anyone asking for a save.
    eventually(|| async {
        let records = saved(&first);
        let finished = records
            .iter()
            .any(|r| r.id.to_string() == done && r.state == "completed");
        let waiting = records
            .iter()
            .any(|r| r.id.to_string() == open && r.state == "input_required");
        (records.len() == 3 && finished && waiting).then_some(())
    })
    .await;
    let root_id = root_session.metadata().id;
    first.manager.shutdown().await;
    drop(agent);
    drop(first);

    // A new host on the same state directory.
    let second = start(root.path());
    let tasks = second.orchestrator.tasks();
    assert_eq!(tasks.len(), 3);
    for record in by_agent(&tasks, "test-agent")
        .into_iter()
        .chain(by_agent(&tasks, "worker"))
    {
        if record.id.to_string() == done {
            assert_eq!(
                (record.state.as_str(), record.summary.as_deref()),
                ("completed", None)
            );
        } else {
            assert_eq!(record.state, "failed", "{record:?}");
            assert_eq!(record.summary.as_deref(), Some(INTERRUPTED));
        }
    }
    assert!(
        tasks.iter().any(|r| r.session == Some(root_id)),
        "the record still names its session"
    );
    // Nothing was started and no prompt was sent: no session, no process.
    assert_eq!(second.manager.active_count().await, 0);
    assert!(second.manager.list().await.is_empty());
    // The interrupted state is on disk too, not just in memory.
    let on_disk = saved(&second);
    assert!(on_disk.iter().filter(|r| r.state == "failed").count() == 2);

    // What was saved says what happened, never what was said or who may speak.
    let raw = std::fs::read_to_string(records::path(&second.state)).unwrap();
    for hidden in [
        PROMPT,
        "Mock lifecycle complete",
        token.as_str(),
        "needs_decision",
        "ACPD_TASK_TOKEN",
    ] {
        assert!(!raw.contains(hidden), "{hidden:?} must not be saved");
    }

    // The new host works normally and keeps the old records.
    let (_, fresh, _) = root_agent(&second).await;
    delegate(&fresh, "sub").await;
    eventually(|| async { (second.orchestrator.tasks().len() == 5).then_some(()) }).await;
    assert_eq!(
        by_agent(&second.orchestrator.tasks(), "worker")
            .iter()
            .filter(|r| r.summary.as_deref() == Some(INTERRUPTED))
            .count(),
        1
    );
    second.manager.shutdown().await;

    // `acpd tasks` reads the same records without a running host.
    std::fs::write(
        root.path().join("config.toml"),
        "registry = 'agents.json'\nstate_directory = 'state'\nworkspace_roots = ['workspace']\n",
    )
    .unwrap();
    std::fs::write(root.path().join("agents.json"), "[]").unwrap();
    let cli = |extra: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_acpd"));
        command
            .arg("--profile")
            .arg(root.path().join("launcher.json"))
            .arg("--config")
            .arg(root.path().join("config.toml"))
            .arg("tasks")
            .args(extra);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let table = cli(&[]);
    assert!(
        table.contains("worker") && table.contains("failed") && table.contains("completed"),
        "{table}"
    );
    assert!(table.contains("the host restarted"), "{table}");
    let listing: Value = serde_json::from_str(&cli(&["--json"])).unwrap();
    assert!(listing.as_array().unwrap().len() >= 5);
}

#[tokio::test]
async fn an_unreadable_record_file_is_set_aside_and_the_host_carries_on() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("state")).unwrap();
    let file = root.path().join("state").join("tasks.json");
    std::fs::write(&file, "this is not a task record").unwrap();
    let life = start(root.path());
    assert!(life.orchestrator.tasks().is_empty());
    assert_eq!(
        std::fs::read_to_string(root.path().join("state").join("tasks.corrupt.json")).unwrap(),
        "this is not a task record",
        "kept for inspection, not deleted"
    );
    // And it works: a new task is recorded in a fresh, valid file.
    let (_, agent, _) = root_agent(&life).await;
    delegate(&agent, "sub").await;
    eventually(|| async {
        (records::load(&life.state).map(|r| r.len()).unwrap_or(0) == 2).then_some(())
    })
    .await;
    life.manager.shutdown().await;
}

#[tokio::test]
async fn the_file_follows_the_final_state_after_a_burst_of_changes() {
    let root = tempfile::tempdir().unwrap();
    let life = start(root.path());
    let (_, agent, _) = root_agent(&life).await;
    let mut tasks = vec![];
    for sub in ["sub", "other", "sub"] {
        tasks.push(delegate(&agent, sub).await);
    }
    for _ in 0..3 {
        // Answer whatever is waiting, as fast as it appears.
        for metadata in life.manager.list().await {
            if metadata.agent_id == "worker"
                && let Ok(session) = life.manager.get(metadata.id).await
                && let Some(request) = session.connection.pending_permissions().into_iter().next()
            {
                let _ = session
                    .connection
                    .permission(
                        request.message["id"].clone(),
                        json!({"outcome":"selected","optionId":"allow"}),
                    )
                    .await;
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    // Whatever order the writes landed in, the file ends up equal to the live state.
    eventually(|| async {
        let live = life.orchestrator.tasks();
        let disk = records::load(&life.state).unwrap_or_default();
        let key = |records: &[TaskRecord]| {
            let mut pairs: Vec<(String, String)> = records
                .iter()
                .map(|r| (r.id.to_string(), r.state.clone()))
                .collect();
            pairs.sort();
            pairs
        };
        (live.len() == 4 && key(&live) == key(&disk)).then_some(())
    })
    .await;
    life.manager.shutdown().await;
}

#[tokio::test]
async fn forgetting_a_root_session_forgets_its_tree_on_disk_and_a_quiet_host_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let life = start(root.path());
    assert!(!records::path(&life.state).exists(), "no tasks, no file");
    let (session, agent, _) = root_agent(&life).await;
    delegate(&agent, "sub").await;
    eventually(|| async { (saved(&life).len() == 2).then_some(()) }).await;
    life.manager.remove(session.metadata().id).await.unwrap();
    eventually(|| async { saved(&life).is_empty().then_some(()) }).await;
    life.manager.shutdown().await;

    // Neither does a host that ran without delegating anything, even though shutdown flushes.
    let quiet = tempfile::tempdir().unwrap();
    let idle = start(quiet.path());
    idle.manager.shutdown().await;
    assert!(
        !records::path(&idle.state).exists(),
        "an empty list is not worth a file"
    );

    // A host with the feature off never creates the file at all.
    let off = tempfile::tempdir().unwrap();
    let host = Host::new(
        Config {
            workspace_roots: vec![off.path().to_path_buf()],
            state_directory: off.path().join("state"),
            ..Default::default()
        },
        Registry::parse("[]").unwrap(),
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!records::path(&off.path().join("state")).exists());
    host.sessions.shutdown().await;
}
