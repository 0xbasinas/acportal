//! Protocol-uniform orchestrator flows against generic ACP agents (mock and testy).
//! Behaviour is driven by ACP + MCP only; no Goose-specific paths.
#![cfg(unix)]

use acpd::{
    config::{Config, OrchestratorConfig, RuntimeConfig},
    connection::WorkspaceAccess,
    orchestrator::{Orchestrator, control, delegate::Backend},
    registry::Registry,
    session::SessionManager,
};
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::time::timeout;

fn testy_bin() -> Option<PathBuf> {
    let path = std::env::var_os("ACPD_TESTY_BIN").map(PathBuf::from)?;
    assert!(
        path.is_file(),
        "ACPD_TESTY_BIN={} is not a file",
        path.display()
    );
    Some(path)
}

async fn eventually<T, F, Fut>(mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    timeout(Duration::from_secs(40), async {
        loop {
            if let Some(value) = check().await {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("condition not reached")
}

fn agent_text(session: &acpd::session::AcpSession) -> String {
    let mut text = String::new();
    for event in &session.replay(0).events {
        let update = &event.message["params"]["update"];
        if event.message["method"] == "session/update"
            && update["sessionUpdate"] == "agent_message_chunk"
            && let Some(chunk) = update["content"]["text"].as_str()
        {
            text.push_str(chunk);
        }
    }
    text
}

/// Mock supervisor: mcpServers injection, control-channel agents_list + task_delegate,
/// phone answers worker permissions with protocol option kinds (allow_once).
#[tokio::test]
async fn mock_supervisor_delegates_through_the_control_channel() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("sub")).unwrap();
    let state = tempfile::tempdir().unwrap();
    let config = Config {
        workspace_roots: vec![workspace.path().into()],
        state_directory: state.path().into(),
        runtime: RuntimeConfig {
            request_timeout_seconds: 30,
            max_sessions: 8,
            ..Default::default()
        },
        orchestrator: OrchestratorConfig {
            enabled: true,
            orchestrate: vec!["supervisor".into()],
            delegable: vec!["worker".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let agents = json!([
        {"id":"supervisor","name":"Supervisor","command":env!("CARGO_BIN_EXE_mock-acp-agent"),
         "args":["--echo-mcp"]},
        {"id":"worker","name":"Worker","command":env!("CARGO_BIN_EXE_mock-acp-agent"),
         "args":[]}
    ]);
    let registry = Registry::parse(&agents.to_string()).unwrap();
    let manager = Arc::new(SessionManager::new(config, registry).unwrap());
    let orchestrator =
        Orchestrator::with_cancel_grace(Arc::downgrade(&manager), Duration::from_millis(500));
    manager.attach_orchestrator(orchestrator.clone());
    let endpoint = control::start(orchestrator.clone(), state.path()).unwrap();
    let access = WorkspaceAccess {
        read_files: true,
        write_files: true,
        terminal: true,
        form_elicitation: false,
        manual_approval: false,
        boolean_options: false,
    };
    let root = manager
        .open_configured("supervisor", workspace.path(), None, vec![], access)
        .await
        .unwrap();
    let setup = root.metadata().setup;
    let definition = setup["_meta"]["mockMcpFull"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|server| server["name"] == "acpd-orchestrator")
        .expect("mock supervisor receives mcp-delegate");
    let token = definition["env"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == "ACPD_TASK_TOKEN")
        .unwrap()["value"]
        .as_str()
        .unwrap()
        .to_owned();
    let stream = control::connect(endpoint.path()).await.unwrap();
    let agent: control::ControlBackend<Box<dyn control::Stream>> =
        control::ControlBackend::new(stream, token);
    let listed = agent.call("agents_list", json!({})).await.unwrap();
    assert!(
        listed["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["ref"] == "worker")
    );
    let started = agent
        .call(
            "task_delegate",
            json!({"agent":"worker","prompt":"inspect","workspace":"sub","wait_seconds":0}),
        )
        .await
        .unwrap();
    let task = started["task_id"].as_str().unwrap().to_owned();
    let worker = eventually(|| async {
        orchestrator
            .tasks()
            .into_iter()
            .find(|t| t.agent == "worker" && t.session.is_some())
            .and_then(|t| t.session)
    })
    .await;
    let child = manager.get(worker).await.unwrap();
    eventually(|| async {
        let pending = child.connection.replay(0).pending_permissions;
        if pending.is_empty() {
            return None;
        }
        for permission in pending {
            let option = permission["params"]["options"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["kind"] == "allow_once")
                .and_then(|o| o["optionId"].as_str())
                .unwrap_or("allow");
            let _ = child
                .connection
                .permission(
                    permission["id"].clone(),
                    json!({"outcome": "selected", "optionId": option}),
                )
                .await;
        }
        Some(())
    })
    .await;
    eventually(|| async {
        let status = agent
            .call("task_status", json!({"task_id": &task}))
            .await
            .ok()?;
        (status["tasks"][0]["state"] == "completed").then_some(())
    })
    .await;
    let result = agent
        .call("task_result", json!({"task_id": task}))
        .await
        .unwrap();
    assert!(
        result["result"]
            .as_str()
            .unwrap_or("")
            .contains("Mock lifecycle complete"),
        "{result}"
    );
    manager.shutdown().await;
}

/// Testy (rust-sdk) supervisor: same mcp-delegate tools over real MCP stdio.
#[tokio::test]
async fn testy_supervisor_lists_and_calls_delegation_tools_over_mcp() {
    let Some(testy) = testy_bin() else {
        eprintln!("skipping: set ACPD_TESTY_BIN (scripts/build-testy.sh)");
        return;
    };
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("sub")).unwrap();
    let state = tempfile::tempdir().unwrap();
    let config = Config {
        workspace_roots: vec![workspace.path().into()],
        state_directory: state.path().into(),
        runtime: RuntimeConfig {
            request_timeout_seconds: 30,
            max_sessions: 8,
            ..Default::default()
        },
        orchestrator: OrchestratorConfig {
            enabled: true,
            orchestrate: vec!["testy".into()],
            delegable: vec!["worker".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let agents = json!([
        {"id":"testy","name":"Testy","command":testy,"args":[]},
        {"id":"worker","name":"Worker","command":env!("CARGO_BIN_EXE_mock-acp-agent"),
         "args":[]}
    ]);
    let registry = Registry::parse(&agents.to_string()).unwrap();
    let manager = Arc::new(SessionManager::new(config, registry).unwrap());
    let orchestrator =
        Orchestrator::with_cancel_grace(Arc::downgrade(&manager), Duration::from_millis(500));
    orchestrator.set_delegate_executable(env!("CARGO_BIN_EXE_acpd").into());
    manager.attach_orchestrator(orchestrator.clone());
    let _endpoint = control::start(orchestrator.clone(), state.path()).unwrap();
    let access = WorkspaceAccess {
        read_files: true,
        write_files: true,
        terminal: true,
        form_elicitation: false,
        manual_approval: false,
        boolean_options: false,
    };
    let root = manager
        .open_configured("testy", workspace.path(), None, vec![], access)
        .await
        .unwrap();

    let root_task = eventually(|| async {
        orchestrator
            .tasks()
            .into_iter()
            .find(|t| t.agent == "testy")
            .map(|t| t.id)
    })
    .await;
    root.prompt(r#"{"command":"list_tools","server":"acpd-orchestrator"}"#)
        .await
        .unwrap();
    let listed = eventually(|| async {
        let text = agent_text(&root);
        text.contains("task_delegate").then_some(text)
    })
    .await;
    assert!(
        listed.contains("agents_list") && listed.contains("task_decide"),
        "{listed}"
    );
    let detail = orchestrator.task_detail(root_task).unwrap();
    assert_eq!(detail["delegate"], "connected", "{detail}");

    root.prompt(
        r#"{"command":"call_tool","server":"acpd-orchestrator","tool":"task_delegate","params":{"agent":"worker","prompt":"inspect","workspace":"sub","wait_seconds":0}}"#,
    )
    .await
    .unwrap();

    let worker_id = eventually(|| async {
        orchestrator
            .tasks()
            .into_iter()
            .find(|t| t.agent == "worker")
            .map(|t| t.id)
    })
    .await;
    eventually(|| async {
        let record = orchestrator
            .tasks()
            .into_iter()
            .find(|t| t.id == worker_id)?;
        if record.state == "completed" {
            return Some(());
        }
        let session = record.session?;
        let child = manager.get(session).await.ok()?;
        for permission in child.connection.replay(0).pending_permissions {
            let option = permission["params"]["options"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|o| o["kind"] == "allow_once")
                .and_then(|o| o["optionId"].as_str())
                .unwrap_or("allow");
            let _ = child
                .connection
                .permission(
                    permission["id"].clone(),
                    json!({"outcome": "selected", "optionId": option}),
                )
                .await;
        }
        None
    })
    .await;

    let reply = agent_text(&root);
    assert!(
        reply.contains("completed")
            || reply.contains("task_id")
            || reply.contains("working")
            || reply.contains("submitted"),
        "testy MCP call_tool reply missing task info: {reply}"
    );
    manager.shutdown().await;
}
