use acpd::{
    config::{Config, RuntimeConfig},
    doctor::{agent_check, agent_sign_in_check as check_without_secret_scan},
    registry::Registry,
};
use serde_json::json;
use std::path::Path;

fn setup(workspace: &Path, args: &[&str]) -> (Config, Registry) {
    let config = Config {
        workspace_roots: vec![workspace.into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 5,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = Registry::parse(
        &json!([{"id":"test-agent","name":"Test agent",
            "command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args,
            "env":{"FIXTURE_PROVIDER_KEY":"synthetic-provider-secret"}}])
        .to_string(),
    )
    .unwrap();
    (config, registry)
}

async fn agent_sign_in_check(
    config: &Config,
    registry: Registry,
    agent: &str,
    workspace: &Path,
) -> (String, bool) {
    let result = check_without_secret_scan(config, registry, agent, workspace).await;
    assert!(!result.0.contains("synthetic-provider-secret"));
    assert!(!result.0.contains("Sign in to the test agent"));
    result
}

#[tokio::test]
async fn sign_in_check_reports_ready_required_and_failures_without_secrets() {
    let workspace = tempfile::tempdir().unwrap();

    let (config, registry) = setup(workspace.path(), &[]);
    let (message, passed) =
        agent_sign_in_check(&config, registry, "test-agent", workspace.path()).await;
    assert!(passed, "{message}");
    assert!(message.contains("ready"), "{message}");
    assert!(message.contains("no prompt sent"), "{message}");

    let (config, registry) = setup(workspace.path(), &["--require-auth"]);
    let (message, passed) =
        agent_sign_in_check(&config, registry, "test-agent", workspace.path()).await;
    assert!(!passed, "{message}");
    assert!(
        message.contains("sign-in required") && message.contains("mock-login (Mock sign-in)"),
        "{message}"
    );

    let (config, registry) = setup(workspace.path(), &["--fault", "exit"]);
    let (message, passed) =
        agent_sign_in_check(&config, registry, "test-agent", workspace.path()).await;
    assert!(!passed, "{message}");

    let outside = tempfile::tempdir().unwrap();
    let (config, registry) = setup(workspace.path(), &[]);
    let (message, passed) =
        agent_sign_in_check(&config, registry, "test-agent", outside.path()).await;
    assert!(!passed && message.contains("workspace_roots"), "{message}");

    let (config, registry) = setup(workspace.path(), &[]);
    let (message, passed) =
        agent_sign_in_check(&config, registry, "missing-agent", workspace.path()).await;
    assert!(!passed && message.contains("unknown agent"), "{message}");
}

#[tokio::test]
async fn prompt_check_is_opt_in_and_reports_provider_failures_by_code_only() {
    let workspace = tempfile::tempdir().unwrap();
    // A turn that asks for permission still completes: doctor refuses it.
    let (config, registry) = setup(workspace.path(), &[]);
    let (message, passed) =
        agent_check(&config, registry, "test-agent", workspace.path(), true).await;
    assert!(passed, "{message}");
    assert!(
        message.contains("provider responded (stopReason end_turn)")
            && message.contains("one small model request"),
        "{message}"
    );
    // Session creation works but the first model call fails, as with real Goose and no key.
    let (config, registry) = setup(workspace.path(), &["--fault", "prompt-error"]);
    let (message, passed) =
        agent_check(&config, registry, "test-agent", workspace.path(), false).await;
    assert!(passed, "without the opt-in, no prompt is sent: {message}");
    assert!(message.contains("no prompt sent"), "{message}");
    let (config, registry) = setup(workspace.path(), &["--fault", "prompt-error"]);
    let (message, passed) =
        agent_check(&config, registry, "test-agent", workspace.path(), true).await;
    assert!(!passed, "{message}");
    assert!(message.contains("JSON-RPC error code -32603"), "{message}");
    assert!(
        !message.contains("synthetic-provider-secret") && !message.contains("Provider rejected")
    );
    let (config, registry) = setup(workspace.path(), &["--fault", "prompt-auth"]);
    let (message, passed) =
        agent_check(&config, registry, "test-agent", workspace.path(), true).await;
    assert!(!passed, "{message}");
    assert!(message.contains("auth_required (-32000)"), "{message}");
    assert!(!message.contains("synthetic-provider-secret"));
}
