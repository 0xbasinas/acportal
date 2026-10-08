//! Byte-pressure checks for host-side buffers. These assert ownership and admission
//! accounting; they are not allocator or whole-process heap measurements.
use acpd::{
    config::{Config, RuntimeConfig},
    connection::AcpConnection,
    registry::{AgentDefinition, Registry},
    session::SessionManager,
};
use serde_json::json;
use std::{path::Path, time::Duration};
use tokio::time::timeout;

fn definition(args: &[&str]) -> AgentDefinition {
    serde_json::from_value(json!({"id":"test-agent","name":"Test agent",
        "command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args}))
    .unwrap()
}

#[tokio::test]
async fn callers_beyond_the_input_budget_are_rejected_instead_of_waiting() {
    let workspace = tempfile::tempdir().unwrap();
    // `--leaf` never reads stdin, so the actor blocks writing the first request.
    let connection = AcpConnection::spawn(
        &definition(&["--leaf"]),
        Path::new(env!("CARGO_BIN_EXE_mock-acp-agent")),
        workspace.path(),
        RuntimeConfig {
            request_timeout_seconds: 30,
            ..Default::default()
        },
    )
    .unwrap();
    let payload = "x".repeat(900 * 1024);
    let mut calls = Vec::new();
    for _ in 0..20 {
        let connection = connection.clone();
        let params = json!({"text": payload});
        calls.push(tokio::spawn(async move {
            connection.request("_mock/echo", params).await
        }));
    }
    // Rejections are immediate; admitted callers wait until the stalled write fails.
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let rejected_early = calls.iter().filter(|call| call.is_finished()).count();
    let cancel = {
        let connection = connection.clone();
        tokio::spawn(async move {
            connection
                .notify("session/cancel", json!({"sessionId":"s"}))
                .await
        })
    };
    let mut rejected = 0;
    for call in calls {
        let error = timeout(Duration::from_secs(15), call)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err()
            .to_string();
        if error.contains("input queue is full") {
            rejected += 1;
        }
    }
    // 8 MiB budget / ~900 KiB requests admits at most nine callers at a time.
    assert!(rejected >= 11, "rejected {rejected}");
    assert!(rejected < 20, "some callers must be admitted");
    assert_eq!(rejected, rejected_early);
    let cancel = timeout(Duration::from_secs(15), cancel)
        .await
        .unwrap()
        .unwrap();
    if let Err(error) = cancel {
        assert!(!error.to_string().contains("input queue is full"));
    }
    connection.shutdown().await;
}

#[tokio::test]
async fn a_stalled_subscriber_does_not_retain_history_beyond_the_journal() {
    let workspace = tempfile::tempdir().unwrap();
    let config = Config {
        workspace_roots: vec![workspace.path().into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 10,
            history_bytes: 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry =
        Registry::parse(&serde_json::to_string(&vec![definition(&[])]).unwrap()).unwrap();
    let manager = SessionManager::new(config, registry).unwrap();
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let mut stalled = session.connection.subscribe();
    let text = "y".repeat(100 * 1024);
    for _ in 0..100 {
        session
            .connection
            .request(
                "_mock/emit_update",
                json!({"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}),
            )
            .await
            .unwrap();
    }
    let replay = session.replay(0);
    assert!(replay.gap);
    let retained: usize = replay
        .events
        .iter()
        .map(|event| event.message.to_string().len())
        .sum();
    assert!(retained <= 1024 * 1024, "journal retained {retained} bytes");
    // Fewer than 256 events were broadcast, so only weak ownership makes this lag.
    assert!(matches!(
        stalled.recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
    ));
    manager.shutdown().await;
}
