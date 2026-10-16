//! Limits on a delegated task: idle timeout, deadline, cost budget, and the sweep that ends the
//! sessions of finished children. Every limit ends a task as `failed` with the host's own words.
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
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::timeout;

struct Life {
    manager: Arc<SessionManager>,
    orchestrator: Arc<Orchestrator>,
    workspace: std::path::PathBuf,
    _endpoint: Endpoint,
    _directory: tempfile::TempDir,
}
type Agent = ControlBackend<Box<dyn Stream>>;

/// `worker_args` are what the delegated agent is started with; `tweak` adjusts the limits.
fn start(worker_args: &[&str], tweak: impl FnOnce(&mut OrchestratorConfig)) -> Life {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    for sub in ["sub", "sub/deeper", "other"] {
        std::fs::create_dir_all(workspace.join(sub)).unwrap();
    }
    let mut orchestrator_config = OrchestratorConfig {
        enabled: true,
        orchestrate: vec!["test-agent".into()],
        delegable: vec!["worker".into()],
        ..Default::default()
    };
    tweak(&mut orchestrator_config);
    let config = Config {
        workspace_roots: vec![workspace.clone()],
        state_directory: directory.path().join("state"),
        runtime: RuntimeConfig {
            request_timeout_seconds: 10,
            max_sessions: 8,
            ..Default::default()
        },
        orchestrator: orchestrator_config,
        ..Default::default()
    };
    let agents = json!([
        {"id":"test-agent","name":"root","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--echo-mcp"]},
        {"id":"worker","name":"worker","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":worker_args},
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
    let definition = session.metadata().setup["_meta"]["mockMcpFull"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["name"] == "acpd-orchestrator").cloned())
        .expect("delegation tools");
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
async fn view(agent: &Agent, task: &str) -> Value {
    call(agent, "task_status", json!({"task_id": task})).await["tasks"][0].clone()
}
async fn reaches(agent: &Agent, task: &str, wanted: &str) -> Value {
    eventually(|| async {
        let task = view(agent, task).await;
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
async fn answer(session: &AcpSession) {
    let request =
        eventually(|| async { session.connection.pending_permissions().into_iter().next() }).await;
    session
        .connection
        .permission(
            request.message["id"].clone(),
            json!({"outcome":"selected","optionId":"allow"}),
        )
        .await
        .unwrap();
}
fn record_of(life: &Life, task: &str) -> acpd::orchestrator::records::TaskRecord {
    life.orchestrator
        .tasks()
        .into_iter()
        .find(|r| r.id.to_string() == task)
        .unwrap()
}

#[tokio::test]
async fn a_silent_task_is_stopped_for_having_no_progress_and_says_so() {
    let life = start(&["--stall-prompt"], |c| c.idle_timeout_seconds = 1);
    let (_, agent) = root(&life).await;
    let started = Instant::now();
    let task = delegate(&agent, json!({})).await;
    let failed = reaches(&agent, &task, "failed").await;
    assert_eq!(failed["summary"], "the task made no progress for 1 seconds");
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "not before the timeout: {:?}",
        started.elapsed()
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "{:?}",
        started.elapsed()
    );
    life.manager.shutdown().await;
}

#[tokio::test]
async fn an_agent_that_ignores_cancel_is_ended_and_the_reason_is_still_recorded() {
    // Silent and deaf to cancel: the host ends the session itself after the grace period.
    let life = start(&["--stall-prompt", "--ignore-cancel"], |c| {
        c.idle_timeout_seconds = 1
    });
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let failed = reaches(&agent, &task, "failed").await;
    assert_eq!(failed["summary"], "the task made no progress for 1 seconds");
    eventually(|| async { (life.manager.active_count().await == 1).then_some(()) }).await;

    // The same for a deadline, with an agent that is silent and deaf. (A permission it waits on
    // is not idleness, and cancelling would end that turn on its own, so that is not this case.)
    let life2 = start(&["--stall-prompt", "--ignore-cancel"], |c| {
        c.idle_timeout_seconds = 300
    });
    let (_, agent2) = root(&life2).await;
    let task2 = delegate(&agent2, json!({"deadline_seconds": 1})).await;
    let failed2 = reaches(&agent2, &task2, "failed").await;
    assert_eq!(
        failed2["summary"], "the task's deadline passed",
        "the force path used to leave no reason"
    );
    eventually(|| async { (life2.manager.active_count().await == 1).then_some(()) }).await;
    life.manager.shutdown().await;
    life2.manager.shutdown().await;
}

#[tokio::test]
async fn waiting_for_a_person_is_not_idleness_and_activity_resets_the_timer() {
    // Idle after 1 second, but the agent waits on a person from the start: it must not be killed.
    let life = start(&[], |c| c.idle_timeout_seconds = 1);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    reaches(&agent, &task, "input_required").await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(view(&agent, &task).await["state"], "input_required");
    {
        let waiting = worker_session(&life).await;
        answer(&waiting).await;
    }
    reaches(&agent, &task, "completed").await;
    life.manager.shutdown().await;

    // Chunks 700 ms apart with a 1 s timeout: every gap is shorter, though the whole is longer.
    let life = start(&["--delay-ms", "700"], |c| c.idle_timeout_seconds = 1);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    reaches(&agent, &task, "input_required").await;
    {
        let waiting = worker_session(&life).await;
        answer(&waiting).await;
    }
    reaches(&agent, &task, "completed").await;
    life.manager.shutdown().await;
}

#[tokio::test]
async fn a_cost_budget_stops_a_task_that_goes_over_it() {
    let life = start(&["--usage-cost", "0.5"], |c| c.task_max_cost_usd = 0.1);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let failed = reaches(&agent, &task, "failed").await;
    assert_eq!(failed["summary"], "the task went over its cost budget");
    assert_eq!(record_of(&life, &task).cost_usd, Some(0.5));
    life.manager.shutdown().await;

    // Under budget: carries on, and the cost is recorded.
    let life = start(&["--usage-cost", "0.5"], |c| c.task_max_cost_usd = 1.0);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    reaches(&agent, &task, "input_required").await;
    {
        let waiting = worker_session(&life).await;
        answer(&waiting).await;
    }
    reaches(&agent, &task, "completed").await;
    assert_eq!(record_of(&life, &task).cost_usd, Some(0.5));
    life.manager.shutdown().await;

    // No budget: any cost is fine.
    let life = start(&["--usage-cost", "500"], |c| c.task_max_cost_usd = 0.0);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    reaches(&agent, &task, "input_required").await;
    {
        let waiting = worker_session(&life).await;
        answer(&waiting).await;
    }
    reaches(&agent, &task, "completed").await;
    life.manager.shutdown().await;
}

#[tokio::test]
async fn an_agent_whose_cost_is_not_in_dollars_is_unmetered_and_not_stopped() {
    let life = start(&["--usage-cost", "9.0", "--usage-currency", "EUR"], |c| {
        c.task_max_cost_usd = 0.1
    });
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    reaches(&agent, &task, "input_required").await;
    {
        let waiting = worker_session(&life).await;
        answer(&waiting).await;
    }
    reaches(&agent, &task, "completed").await;
    assert_eq!(
        record_of(&life, &task).cost_usd,
        None,
        "no dollars were reported, so none are claimed"
    );
    life.manager.shutdown().await;
}

#[tokio::test]
async fn a_finished_childs_session_is_ended_after_its_keep_time_but_its_record_stays() {
    let life = start(&[], |c| c.finished_child_keep_seconds = 3);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let child = worker_session(&life).await;
    let child_id = child.metadata().id;
    answer(&child).await;
    reaches(&agent, &task, "completed").await;
    drop(child);
    // Kept for the whole of its keep time (a follow-up may still come)...
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        life.manager.get(child_id).await.is_ok(),
        "swept before its keep time was up"
    );
    // ...and then ended.
    eventually(|| async { life.manager.get(child_id).await.is_err().then_some(()) }).await;
    assert_eq!(
        life.manager.active_count().await,
        1,
        "only the root session is left"
    );
    let kept = view(&agent, &task).await;
    assert_eq!(kept["state"], "completed", "the record is the same: {kept}");
    let refused = agent
        .call("task_send", json!({"task_id": task, "prompt": "more"}))
        .await
        .unwrap_err();
    assert_eq!(refused.code, "invalid_state");
    life.manager.shutdown().await;

    // Zero means keep it until the root goes.
    let life = start(&[], |c| c.finished_child_keep_seconds = 0);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let child = worker_session(&life).await;
    answer(&child).await;
    reaches(&agent, &task, "completed").await;
    // With keep = 0 the janitor sleeps 10 s between passes and sweeps once at startup, so wait
    // past a whole second pass; a shorter wait would never see a sweep happen at all.
    tokio::time::sleep(Duration::from_millis(11_500)).await;
    assert!(life.manager.get(child.metadata().id).await.is_ok());
    life.manager.shutdown().await;
}

#[tokio::test]
async fn a_finished_child_with_open_work_below_it_is_not_ended_from_under_that_work() {
    let life = start(&["--echo-mcp"], |c| {
        c.finished_child_keep_seconds = 1;
        c.orchestrate = vec!["test-agent".into(), "worker".into()];
        c.delegable = vec!["worker".into()];
    });
    let (_, top) = root(&life).await;
    let child_task = delegate(&top, json!({})).await;
    let child = worker_session(&life).await;
    // The child delegates a grandchild that then waits on a person, and finishes itself.
    let middle = agent_of(&child).await;
    call(
        &middle,
        "task_delegate",
        json!({"agent":"worker","prompt":"go","workspace":"deeper"}),
    )
    .await;
    answer(&child).await;
    reaches(&top, &child_task, "completed").await;
    tokio::time::sleep(Duration::from_millis(3500)).await;
    assert!(
        life.manager.get(child.metadata().id).await.is_ok(),
        "ended while work below it was open"
    );
    life.manager.shutdown().await;
}

#[tokio::test]
async fn a_task_may_be_given_only_so_many_prompts_and_a_refusal_leaves_it_usable() {
    // Two turns: the first prompt and one follow-up.
    let life = start(&[], |c| c.max_turns_per_task = 2);
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let child = worker_session(&life).await;
    answer(&child).await;
    reaches(&agent, &task, "completed").await;
    call(
        &agent,
        "task_send",
        json!({"task_id": task, "prompt": "more"}),
    )
    .await;
    answer(&child).await;
    reaches(&agent, &task, "completed").await;
    // The third is refused with its own code, before anything is claimed or sent.
    let refused = agent
        .call("task_send", json!({"task_id": task, "prompt": "again"}))
        .await
        .unwrap_err();
    assert_eq!(refused.code, "turn_limit");
    assert_eq!(view(&agent, &task).await["state"], "completed");
    life.manager.shutdown().await;

    // 0 is unlimited: the same three prompts all go through.
    let life = start(&[], |_| {});
    let (_, agent) = root(&life).await;
    let task = delegate(&agent, json!({})).await;
    let child = worker_session(&life).await;
    answer(&child).await;
    reaches(&agent, &task, "completed").await;
    for _ in 0..2 {
        call(
            &agent,
            "task_send",
            json!({"task_id": task, "prompt": "more"}),
        )
        .await;
        answer(&child).await;
        reaches(&agent, &task, "completed").await;
    }
    life.manager.shutdown().await;
}

#[test]
fn nonsense_limits_are_refused_when_the_configuration_is_checked() {
    let bad = |tweak: fn(&mut OrchestratorConfig)| {
        let mut orchestrator = OrchestratorConfig::default();
        tweak(&mut orchestrator);
        Config {
            orchestrator,
            ..Default::default()
        }
        .validate()
        .is_err()
    };
    assert!(bad(|c| c.idle_timeout_seconds = 0));
    assert!(bad(|c| c.idle_timeout_seconds = 3601));
    assert!(bad(|c| c.task_max_cost_usd = -1.0));
    assert!(bad(|c| c.task_max_cost_usd = f64::NAN));
    assert!(bad(|c| c.task_max_cost_usd = f64::INFINITY));
    assert!(bad(|c| c.finished_child_keep_seconds = 86_401));
    assert!(bad(|c| c.max_turns_per_task = 1001));
    assert!(!bad(|c| c.max_turns_per_task = 1000));
    assert!(!bad(|c| c.idle_timeout_seconds = 1));
    assert!(!bad(|c| c.finished_child_keep_seconds = 0));
}
