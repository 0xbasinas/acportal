//! Host-side auto-approval end to end: a real session manager, the mock agent raising permission
//! requests, and (for the model reviewer) a local fake OpenAI-compatible server.
use acpd::{
    auto_approve::{AutoApproveConfig, AutoRisk},
    config::{Config, RuntimeConfig},
    registry::Registry,
    session::{AcpSession, SessionManager},
    shell_review::ModelReviewConfig,
};
use axum::{Router, extract::State, routing::post};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

/// A manager with one mock agent, started with `agent_args`, and `policy` for it (if any).
fn manager(
    workspace: &Path,
    state: &Path,
    agent_args: &[&str],
    policy: Option<AutoApproveConfig>,
) -> Arc<SessionManager> {
    let config = Config {
        workspace_roots: vec![workspace.to_path_buf()],
        state_directory: state.to_path_buf(),
        runtime: RuntimeConfig {
            request_timeout_seconds: 10,
            max_sessions: 8,
            ..Default::default()
        },
        auto_approve: policy
            .map(|policy| BTreeMap::from([("mock".to_owned(), policy)]))
            .unwrap_or_default(),
        ..Default::default()
    };
    let registry = Registry::parse(
        &json!([{"id":"mock","name":"Mock","command":env!("CARGO_BIN_EXE_mock-acp-agent"),
            "args":agent_args}])
        .to_string(),
    )
    .unwrap();
    Arc::new(SessionManager::new(config, registry).unwrap())
}

fn medium() -> AutoApproveConfig {
    AutoApproveConfig {
        max_risk: AutoRisk::Medium,
        only_in_worktree: false,
        ..Default::default()
    }
}

struct Rig {
    manager: Arc<SessionManager>,
    session: Arc<AcpSession>,
    _workspace: tempfile::TempDir,
    _state: tempfile::TempDir,
}
async fn rig(agent_args: &[&str], policy: Option<AutoApproveConfig>) -> Rig {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), state.path(), agent_args, policy);
    let session = manager.create("mock", workspace.path()).await.unwrap();
    Rig {
        manager,
        session,
        _workspace: workspace,
        _state: state,
    }
}

/// Start a turn and give the host a moment to act on the request it raises.
fn start_turn(session: &Arc<AcpSession>) -> tokio::task::JoinHandle<anyhow::Result<Value>> {
    let session = session.clone();
    tokio::spawn(async move { session.prompt("go").await })
}
async fn wait_for_request(session: &AcpSession) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(pending) = session.connection.pending_permissions().first() {
                return pending.message.clone();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("the agent raised a request")
}
/// The request is still waiting for a person after the host has had every chance to act.
async fn stays_with_the_person(
    session: &Arc<AcpSession>,
    turn: tokio::task::JoinHandle<anyhow::Result<Value>>,
) -> Value {
    let request = wait_for_request(session).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(
        session.connection.pending_permissions().len(),
        1,
        "the host answered a request it must leave"
    );
    assert!(!turn.is_finished());
    // The person refuses; the turn ends.
    session
        .connection
        .permission(
            request["id"].clone(),
            json!({"outcome":"selected","optionId":"deny"}),
        )
        .await
        .unwrap();
    turn.await.unwrap().unwrap();
    request
}
fn reviews(session: &AcpSession) -> Vec<Value> {
    session
        .replay(0)
        .events
        .iter()
        .filter_map(|event| {
            let review = &event.message["params"]["update"]["_meta"]["acpdAutoReview"];
            review.is_object().then(|| review.clone())
        })
        .collect()
}

#[tokio::test]
async fn an_edit_inside_the_folder_is_answered_by_the_host_and_nobody_is_asked() {
    let rig = rig(&["--permission-path", "src/lib.rs"], Some(medium())).await;
    let done = tokio::time::timeout(Duration::from_secs(10), rig.session.prompt("go"))
        .await
        .expect("the turn finishes without a person")
        .unwrap();
    assert_eq!(done["stopReason"], "end_turn");
    assert!(rig.session.connection.pending_permissions().is_empty());
    let seen = reviews(&rig.session);
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["decision"], "allow_once");
    assert_eq!(seen[0]["layer"], "rules");
    let reason = seen[0]["reason"].as_str().unwrap();
    assert!(
        reason.contains("src/lib.rs") && reason.contains("medium risk"),
        "{reason}"
    );
    rig.manager.shutdown().await;
}

#[tokio::test]
async fn nothing_is_answered_without_a_policy_and_a_phone_session_needs_the_worktree_switch_off() {
    // No policy at all.
    let plain = rig(&[], None).await;
    let turn = start_turn(&plain.session);
    stays_with_the_person(&plain.session, turn).await;
    assert!(reviews(&plain.session).is_empty());
    plain.manager.shutdown().await;

    // A policy, but the session is an ordinary one, not in a task worktree: by default it is
    // left alone.
    let contained = AutoApproveConfig {
        max_risk: AutoRisk::Medium,
        ..Default::default()
    };
    assert!(contained.only_in_worktree);
    let phone = rig(&[], Some(contained)).await;
    let turn = start_turn(&phone.session);
    stays_with_the_person(&phone.session, turn).await;
    assert!(reviews(&phone.session).is_empty());
    phone.manager.shutdown().await;

    // A ceiling of none is off even with the rest set.
    let off = AutoApproveConfig {
        max_risk: AutoRisk::None,
        only_in_worktree: false,
        own_edit_shape: true,
        ..Default::default()
    };
    let session = rig(&[], Some(off)).await;
    let turn = start_turn(&session.session);
    stays_with_the_person(&session.session, turn).await;
    session.manager.shutdown().await;
}

#[tokio::test]
async fn what_is_not_safe_stays_with_the_person_and_is_never_refused_by_the_host() {
    for args in [
        vec!["--permission-kind", "delete"],
        vec!["--permission-kind", "execute"],
        vec!["--permission-path", ".env"],
        vec!["--permission-path", "../outside.txt"],
    ] {
        let rig = rig(&args, Some(medium())).await;
        let turn = start_turn(&rig.session);
        stays_with_the_person(&rig.session, turn).await;
        // The host neither allowed nor refused: no automatic review of this request at all.
        assert!(
            reviews(&rig.session).is_empty(),
            "{args:?}: {:?}",
            reviews(&rig.session)
        );
        rig.manager.shutdown().await;
    }
}

#[tokio::test]
async fn an_agents_own_write_is_approved_only_when_its_declared_shape_is_trusted() {
    let args = [
        "--permission-kind",
        "other",
        "--permission-path",
        "src/lib.rs",
        "--permission-content",
        "fn a() {}",
    ];
    let untrusting = rig(&args, Some(medium())).await;
    let turn = start_turn(&untrusting.session);
    stays_with_the_person(&untrusting.session, turn).await;
    untrusting.manager.shutdown().await;

    let trusting = rig(
        &args,
        Some(AutoApproveConfig {
            own_edit_shape: true,
            ..medium()
        }),
    )
    .await;
    let done = tokio::time::timeout(Duration::from_secs(10), trusting.session.prompt("go"))
        .await
        .expect("approved by the host")
        .unwrap();
    assert_eq!(done["stopReason"], "end_turn");
    let seen = reviews(&trusting.session);
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0]["reason"]
            .as_str()
            .unwrap()
            .contains("declared shape"),
        "the phone is told it was the shape that qualified it: {seen:?}"
    );
    trusting.manager.shutdown().await;
}

/// A fake model server: it records what it is sent and answers by the request's path.
async fn fake_model() -> (String, Arc<Mutex<Vec<Value>>>) {
    let bodies: Arc<Mutex<Vec<Value>>> = Arc::default();
    let app = Router::new()
        .route(
            "/chat/completions",
            post(
                |State(bodies): State<Arc<Mutex<Vec<Value>>>>,
                 axum::Json(body): axum::Json<Value>| async move {
                    let user: Value =
                        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap())
                            .unwrap();
                    let path = user["path"].as_str().unwrap_or("").to_owned();
                    bodies.lock().unwrap().push(body);
                    let content = if path.ends_with("ci.yml") {
                        r#"{"decision":"ask","reason":"touches the CI workflow"}"#
                    } else if path.ends_with("scary.sh") {
                        r#"{"decision":"deny","reason":"downloads and runs code"}"#
                    } else {
                        r#"{"decision":"allow","reason":"an ordinary source change"}"#
                    };
                    axum::Json(json!({"choices":[{"message":{"content":content}}]}))
                },
            ),
        )
        .with_state(bodies.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), bodies)
}
fn with_model(base_url: &str, sees_content: bool) -> AutoApproveConfig {
    let key = "ACPD_AUTO_APPROVE_TEST_KEY";
    // SAFETY: test-only variable with a name nothing else uses.
    unsafe { std::env::set_var(key, "synthetic-test-key") };
    AutoApproveConfig {
        own_edit_shape: true,
        model_sees_content: sees_content,
        model: Some(ModelReviewConfig {
            base_url: base_url.to_owned(),
            model: "test-model".into(),
            api_key_env: key.into(),
            timeout_seconds: 2,
            max_requests_per_minute: 30,
            max_command_bytes: 2048,
        }),
        ..medium()
    }
}
const SECRET_BODY: &str = "SECRET-FILE-CONTENT-12345";
fn write_args(path: &str) -> [&str; 6] {
    [
        "--permission-kind",
        "other",
        "--permission-path",
        path,
        "--permission-content",
        SECRET_BODY,
    ]
}

#[tokio::test]
async fn a_model_that_agrees_lets_the_host_answer_and_never_sees_the_content_by_default() {
    let (url, bodies) = fake_model().await;
    let rig = rig(&write_args("src/lib.rs"), Some(with_model(&url, false))).await;
    let done = tokio::time::timeout(Duration::from_secs(10), rig.session.prompt("go"))
        .await
        .expect("approved")
        .unwrap();
    assert_eq!(done["stopReason"], "end_turn");
    let seen = reviews(&rig.session);
    assert_eq!(seen[0]["layer"], "model");
    assert_eq!(seen[0]["decision"], "allow_once");
    assert!(
        seen[0]["reason"]
            .as_str()
            .unwrap()
            .contains("an ordinary source change")
    );
    let sent = bodies.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    let text = sent[0].to_string();
    assert!(
        text.contains("src/lib.rs"),
        "the relative path is sent: {text}"
    );
    assert!(text.contains("mock"), "the agent id is sent: {text}");
    assert!(
        !text.contains(SECRET_BODY),
        "the file content went to the model: {text}"
    );
    assert!(
        !text.contains(
            &rig._workspace
                .path()
                .to_string_lossy()
                .replace('\\', "\\\\")
        ),
        "an absolute path went to the model"
    );
    rig.manager.shutdown().await;
}

#[tokio::test]
async fn the_content_goes_to_the_model_only_when_that_is_switched_on() {
    let (url, bodies) = fake_model().await;
    let rig = rig(&write_args("src/lib.rs"), Some(with_model(&url, true))).await;
    tokio::time::timeout(Duration::from_secs(10), rig.session.prompt("go"))
        .await
        .expect("approved")
        .unwrap();
    assert!(bodies.lock().unwrap()[0].to_string().contains(SECRET_BODY));
    rig.manager.shutdown().await;
}

#[tokio::test]
async fn a_model_that_objects_can_only_make_it_stricter_and_the_person_is_told_why() {
    let (url, bodies) = fake_model().await;
    for (path, reason) in [
        ("ci.yml", "touches the CI workflow"),
        ("scary.sh", "downloads and runs code"),
    ] {
        let rig = rig(&write_args(path), Some(with_model(&url, false))).await;
        let turn = start_turn(&rig.session);
        stays_with_the_person(&rig.session, turn).await;
        let seen = reviews(&rig.session);
        assert_eq!(seen.len(), 1, "{path}: {seen:?}");
        assert_eq!(
            seen[0]["decision"], "ask",
            "a model never makes the host refuse"
        );
        assert_eq!(seen[0]["layer"], "model");
        assert!(seen[0]["reason"].as_str().unwrap().contains(reason));
        rig.manager.shutdown().await;
    }
    assert_eq!(bodies.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_model_that_cannot_be_reached_leaves_it_to_the_person() {
    // Nothing listens here.
    let unreachable = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let rig = rig(
        &write_args("src/lib.rs"),
        Some(with_model(&unreachable, false)),
    )
    .await;
    let turn = start_turn(&rig.session);
    stays_with_the_person(&rig.session, turn).await;
    let seen = reviews(&rig.session);
    assert_eq!(seen[0]["decision"], "ask");
    assert!(seen[0]["reason"].as_str().unwrap().contains("unavailable"));
    rig.manager.shutdown().await;
}

#[tokio::test]
async fn the_model_is_never_asked_about_what_the_rules_would_not_allow() {
    let (url, bodies) = fake_model().await;
    let mut args = write_args("src/lib.rs").to_vec();
    args[1] = "delete"; // a delete is never auto-approved, whatever the model thinks
    let rig = rig(&args, Some(with_model(&url, true))).await;
    let turn = start_turn(&rig.session);
    stays_with_the_person(&rig.session, turn).await;
    assert!(
        bodies.lock().unwrap().is_empty(),
        "the model was asked, and sent a request it should never see"
    );
    rig.manager.shutdown().await;
}

/// Same agent, same policy, same request as the first test. Only the session's switch differs.
#[tokio::test]
async fn a_session_with_manual_approval_is_never_answered_by_the_host() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let manager = manager(
        workspace.path(),
        state.path(),
        &["--permission-path", "src/lib.rs"],
        Some(medium()),
    );
    // Control: without the switch the host answers this request itself.
    let control = manager.create("mock", workspace.path()).await.unwrap();
    let done = tokio::time::timeout(Duration::from_secs(10), control.prompt("go"))
        .await
        .expect("the control turn finishes without a person")
        .unwrap();
    assert_eq!(done["stopReason"], "end_turn");
    assert_eq!(reviews(&control).len(), 1);

    let manual = manager
        .open_configured(
            "mock",
            workspace.path(),
            None,
            Vec::new(),
            acpd::connection::WorkspaceAccess {
                manual_approval: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(manual.metadata().workspace_access.manual_approval);
    let turn = start_turn(&manual);
    stays_with_the_person(&manual, turn).await;
    assert!(
        reviews(&manual).is_empty(),
        "the host recorded a review for a manual session"
    );
    manager.shutdown().await;
}
