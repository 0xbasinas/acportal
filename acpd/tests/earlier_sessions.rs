//! `GET /v1/agents/{id}/sessions` and `session/close` on removal, against the mock agent.
use acpd::{api::Host, config::Config, registry::Registry, security::Role};
use serde_json::{Value, json};
use std::time::Duration;

fn mock(args: &[&str]) -> Value {
    json!({"id":"mock","name":"Mock","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args})
}

async fn serve(
    workspace: &std::path::Path,
    state: &std::path::Path,
    args: &[&str],
) -> (Host, String, String) {
    let config = Config {
        workspace_roots: vec![workspace.to_path_buf()],
        state_directory: state.to_path_buf(),
        ..Default::default()
    };
    let registry = Registry::parse(&json!([mock(args)]).to_string()).unwrap();
    let host = Host::new(config, registry).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let router = host.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::new();
    let code = host.security.new_pairing_code_for(Role::Phone).unwrap();
    let reply: Value = client
        .post(format!("http://{addr}/v1/pair"))
        .json(&json!({"code": code, "deviceName": "earlier-test"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    (host, addr, reply["token"].as_str().unwrap().to_owned())
}

#[tokio::test]
async fn earlier_sessions_are_listed_for_the_folder_only_and_closing_is_polite() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("work");
    std::fs::create_dir_all(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let close_log = dir.path().join("closed.txt");
    let (host, addr, token) = serve(
        &workspace,
        &dir.path().join("state"),
        &[
            "--session-capabilities",
            "--close-log",
            close_log.to_str().unwrap(),
        ],
    )
    .await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let ask = |query: String| {
        client
            .get(format!("http://{addr}/v1/agents/mock/sessions?{query}"))
            .bearer_auth(&token)
            .send()
    };

    let status: Value = client
        .get(format!("http://{addr}/v1/status"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        status["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature == "agentSessionList")
    );

    let reply: Value = ask(format!("workspace={}", workspace.display()))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply["listSupported"], true, "{reply}");
    assert_eq!(reply["loadSupported"], true, "{reply}");
    let ids: Vec<&str> = reply["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["sessionId"].as_str().unwrap())
        .collect();
    // The second page is followed; the entry for another folder is dropped.
    assert_eq!(ids, ["old-1", "old-long", "old-2"], "{reply}");
    assert!(!reply.to_string().contains("somewhere"), "{reply}");
    let long_title = reply["sessions"][1]["title"].as_str().unwrap();
    assert!(long_title.chars().count() <= 200, "{}", long_title.len());
    assert_eq!(reply["sessions"][0]["title"], "First chat");

    // The throwaway session is gone, and the agent was asked to close it first.
    assert!(host.sessions.list().await.is_empty());
    let closed = std::fs::read_to_string(&close_log).unwrap();
    assert_eq!(closed.lines().count(), 1, "{closed}");

    // A folder the host does not allow is refused before any agent starts.
    let outside = dir.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    let refused = ask(format!("workspace={}", outside.display()))
        .await
        .unwrap();
    assert!(refused.status().is_client_error());
    assert_eq!(
        std::fs::read_to_string(&close_log).unwrap().lines().count(),
        1
    );
    host.sessions.shutdown().await;
}

#[tokio::test]
async fn an_agent_without_the_capabilities_lists_nothing_and_is_not_asked_to_close() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("work");
    std::fs::create_dir_all(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let close_log = dir.path().join("closed.txt");
    let (host, addr, token) = serve(
        &workspace,
        &dir.path().join("state"),
        &["--close-log", close_log.to_str().unwrap()],
    )
    .await;
    let reply: Value = reqwest::Client::new()
        .get(format!(
            "http://{addr}/v1/agents/mock/sessions?workspace={}",
            workspace.display()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply["listSupported"], false, "{reply}");
    assert_eq!(reply["sessions"], json!([]));
    assert!(
        !close_log.exists(),
        "close sent to an agent that never offered it"
    );
    host.sessions.shutdown().await;
}
