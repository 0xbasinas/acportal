//! Separate test binary: frame metadata uses a process-wide switch and global subscriber.
use acpd::{
    config::{Config, RuntimeConfig},
    registry::Registry,
    session::SessionManager,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frame_metadata_logs_methods_and_sizes_but_never_content() {
    use tracing_subscriber::{fmt, layer::SubscriberExt};
    let capture = Capture::default();
    let writer = capture.clone();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("acpd::acp_frames=debug"))
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_writer(move || writer.clone()),
        );
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let workspace = tempfile::tempdir().unwrap();
    let secret = "synthetic-frame-secret";
    let config = Config {
        workspace_roots: vec![workspace.path().into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 5,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = Registry::parse(
        &json!([{"id":"test-agent","name":"Test agent",
            "command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":[],
            "env":{"FIXTURE_PROVIDER_KEY":secret}}])
        .to_string(),
    )
    .unwrap();

    // Disabled by default: nothing is recorded.
    let manager = SessionManager::new(config.clone(), registry.clone()).unwrap();
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    manager.shutdown().await;
    drop(session);
    assert!(capture.0.lock().unwrap().is_empty());

    acpd::connection::enable_frame_metadata();
    let manager = SessionManager::new(config, registry).unwrap();
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    session
        .connection
        .request("_mock/echo", json!({"text":secret,"token":secret}))
        .await
        .unwrap();
    manager.shutdown().await;

    let text = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("direction=\"host_to_agent\""), "{text}");
    assert!(text.contains("direction=\"agent_to_host\""), "{text}");
    assert!(text.contains("method=session/new"), "{text}");
    assert!(text.contains("method=_mock/echo"), "{text}");
    assert!(text.contains("kind=\"response\""), "{text}");
    assert!(text.contains("bytes="), "{text}");
    assert!(!text.contains(secret), "{text}");
    assert!(
        !text.contains("mock-1"),
        "ACP session ids must not be logged: {text}"
    );
}
