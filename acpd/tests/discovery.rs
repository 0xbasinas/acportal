use acpd::{
    config::RuntimeConfig, connection::AcpConnection, discovery::Launch, registry::AgentDefinition,
};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
use tokio::time::timeout;

fn definition() -> AgentDefinition {
    serde_json::from_value(json!({"id":"test-agent","name":"Test agent",
        "command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":[]}))
    .unwrap()
}
fn limits() -> RuntimeConfig {
    RuntimeConfig {
        request_timeout_seconds: 2,
        ..Default::default()
    }
}

/// An npm shim runs as `node script.js args`: the launch's own arguments come before the agent's. The
/// mock agent's `--leaf` flag makes it ignore stdin, so a request only fails if that argument arrived.
#[tokio::test]
async fn launch_arguments_are_passed_before_the_agents_own() {
    let workspace = tempfile::tempdir().unwrap();
    let program = PathBuf::from(env!("CARGO_BIN_EXE_mock-acp-agent"));

    let plain = AcpConnection::spawn(
        &definition(),
        Launch::plain(&program),
        workspace.path(),
        limits(),
    )
    .unwrap();
    let reply = timeout(
        Duration::from_secs(10),
        plain.request("_mock/echo", json!({"text":"hi"})),
    )
    .await;
    assert!(
        reply.expect("plain launch answers in time").is_ok(),
        "baseline works"
    );
    plain.shutdown().await;

    let with_prefix = Launch {
        prefix_args: vec!["--leaf".into()],
        ..Launch::plain(&program)
    };
    let leaf =
        AcpConnection::spawn(&definition(), with_prefix, workspace.path(), limits()).unwrap();
    let reply = timeout(
        Duration::from_secs(10),
        leaf.request("_mock/echo", json!({"text":"hi"})),
    )
    .await;
    assert!(
        !matches!(reply, Ok(Ok(_))),
        "the --leaf prefix argument reached the program, so it never answered"
    );
    leaf.shutdown().await;
}

/// Extra folders are put first on the child's PATH.
#[tokio::test]
async fn launch_folders_are_put_first_on_the_started_programs_path() {
    let workspace = tempfile::tempdir().unwrap();
    let extra = tempfile::tempdir().unwrap();
    let program = PathBuf::from(env!("CARGO_BIN_EXE_mock-acp-agent"));
    let launch = Launch {
        path_dirs: vec![extra.path().to_path_buf()],
        ..Launch::plain(&program)
    };
    let joined = launch.path_with_dirs(std::env::var_os("PATH")).unwrap();
    assert_eq!(std::env::split_paths(&joined).next().unwrap(), extra.path());
    // And it still starts and answers.
    let connection =
        AcpConnection::spawn(&definition(), launch, workspace.path(), limits()).unwrap();
    let reply = timeout(
        Duration::from_secs(10),
        connection.request("_mock/echo", json!({"text":"hi"})),
    )
    .await;
    assert!(reply.expect("answers in time").is_ok());
    connection.shutdown().await;
}
