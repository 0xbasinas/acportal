//! "Auto-approve everything": a session may ask the host to answer its permission requests, but
//! only where the operator allowed it for the agent, and the deny rules still hold.
use acpd::{
    config::{Config, RuntimeConfig, YoloConfig},
    connection::WorkspaceAccess,
    registry::Registry,
    session::{AcpSession, SessionManager},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

fn manager(
    workspace: &Path,
    state: &Path,
    agent_args: &[&str],
    allowed: bool,
) -> Arc<SessionManager> {
    let config = Config {
        workspace_roots: vec![workspace.to_path_buf()],
        state_directory: state.to_path_buf(),
        runtime: RuntimeConfig {
            request_timeout_seconds: 10,
            max_sessions: 8,
            ..Default::default()
        },
        yolo: if allowed {
            BTreeMap::from([("mock".to_owned(), YoloConfig { enabled: true })])
        } else {
            BTreeMap::new()
        },
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
fn everything() -> WorkspaceAccess {
    WorkspaceAccess {
        yolo: true,
        ..Default::default()
    }
}
struct Rig {
    manager: Arc<SessionManager>,
    session: Arc<AcpSession>,
    _workspace: tempfile::TempDir,
    _state: tempfile::TempDir,
}
async fn rig(agent_args: &[&str], access: WorkspaceAccess) -> Rig {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), state.path(), agent_args, true);
    let session = manager
        .open_configured("mock", workspace.path(), None, Vec::new(), access)
        .await
        .unwrap();
    Rig {
        manager,
        session,
        _workspace: workspace,
        _state: state,
    }
}
async fn left_to_the_person(rig: &Rig) {
    let session = rig.session.clone();
    let turn = tokio::spawn(async move { session.prompt("go").await });
    let request = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(pending) = rig.session.connection.pending_permissions().first() {
                return pending.message.clone();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "the agent raised no request; reviews: {:?}",
            reviews(&rig.session)
        )
    });
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(rig.session.connection.pending_permissions().len(), 1);
    assert!(!turn.is_finished());
    rig.session
        .connection
        .permission(
            request["id"].clone(),
            json!({"outcome":"selected","optionId":"deny"}),
        )
        .await
        .unwrap();
    turn.await.unwrap().unwrap();
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
async fn an_everything_session_is_answered_for_requests_a_normal_policy_would_leave() {
    for kind in ["execute", "delete", "fetch"] {
        let rig = rig(&["--permission-kind", kind], everything()).await;
        let done = tokio::time::timeout(Duration::from_secs(10), rig.session.prompt("go"))
            .await
            .expect("the turn finishes without a person")
            .unwrap();
        assert_eq!(done["stopReason"], "end_turn", "{kind}");
        assert!(rig.session.connection.pending_permissions().is_empty());
        let seen = reviews(&rig.session);
        assert_eq!(seen.len(), 1, "{kind}: {seen:?}");
        assert_eq!(seen[0]["decision"], "allow_once");
        rig.manager.shutdown().await;
    }
}

#[tokio::test]
async fn the_deny_rules_still_hold_in_an_everything_session() {
    for path in [".env", "../outside.txt"] {
        let rig = rig(&["--permission-path", path], everything()).await;
        left_to_the_person(&rig).await;
        assert!(reviews(&rig.session).is_empty(), "{path}");
        rig.manager.shutdown().await;
    }
}

#[tokio::test]
async fn an_ordinary_session_on_the_same_host_is_still_asked() {
    let rig = rig(
        &["--permission-kind", "execute"],
        WorkspaceAccess::default(),
    )
    .await;
    left_to_the_person(&rig).await;
    assert!(reviews(&rig.session).is_empty());
    rig.manager.shutdown().await;
}

#[tokio::test]
async fn a_host_that_did_not_allow_it_refuses_the_session() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), state.path(), &[], false);
    let error = manager
        .open_configured("mock", workspace.path(), None, Vec::new(), everything())
        .await
        .err()
        .expect("refused");
    assert!(error.to_string().contains("not turned on"), "{error}");
    manager.shutdown().await;

    // Allowed, but combined with the opposite switch.
    let manager = self::manager(workspace.path(), state.path(), &[], true);
    let both = WorkspaceAccess {
        manual_approval: true,
        ..everything()
    };
    let error = manager
        .open_configured("mock", workspace.path(), None, Vec::new(), both)
        .await
        .err()
        .expect("refused");
    assert!(error.to_string().contains("cannot both"), "{error}");
    manager.shutdown().await;
}
