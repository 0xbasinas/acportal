//! Sessions outside any project: `POST /v1/scratch` makes an empty folder below the host's
//! scratch root, and a session may start there even though no workspace root contains it.
use acpd::{api::Host, config::Config, registry::Registry, security::Role};
use serde_json::{Value, json};
use std::time::Duration;

#[tokio::test]
async fn scratch_folders_are_new_empty_and_allowed_for_sessions_only_below_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    // No workspace roots at all: only scratch folders are allowed.
    let config = Config {
        workspace_roots: vec![],
        state_directory: state.clone(),
        ..Default::default()
    };
    let mock = env!("CARGO_BIN_EXE_mock-acp-agent");
    let agents = json!([{"id":"a","name":"a","command":mock}]);
    let host = Host::new(
        config.clone(),
        Registry::parse(&agents.to_string()).unwrap(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let router = host.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let code = host.security.new_pairing_code_for(Role::Phone).unwrap();
    let paired: Value = client
        .post(format!("http://{addr}/v1/pair"))
        .json(&json!({"code": code, "deviceName": "test"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = paired["token"].as_str().unwrap().to_owned();
    let scratch = || {
        client
            .post(format!("http://{addr}/v1/scratch"))
            .bearer_auth(&token)
            .send()
    };
    let start = |workspace: Value| {
        client
            .post(format!("http://{addr}/v1/sessions"))
            .bearer_auth(&token)
            .json(&json!({"agentId":"a","workspace":workspace}))
            .send()
    };

    let first = scratch().await.unwrap();
    assert_eq!(first.status(), 201);
    let first: Value = first.json().await.unwrap();
    let second: Value = scratch().await.unwrap().json().await.unwrap();
    let path = std::path::PathBuf::from(first["path"].as_str().unwrap());
    assert_ne!(first["path"], second["path"]);
    assert!(path.is_dir() && std::fs::read_dir(&path).unwrap().next().is_none());
    assert!(
        path.canonicalize()
            .unwrap()
            .starts_with(config.scratch_root().canonicalize().unwrap())
    );

    let session = start(json!(path)).await.unwrap();
    assert_eq!(session.status(), 201, "{:?}", session.text().await);
    // The scratch root itself, the state folder and other folders stay refused.
    assert_eq!(
        start(json!(config.scratch_root())).await.unwrap().status(),
        400
    );
    assert_eq!(start(json!(state)).await.unwrap().status(), 400);
    assert_eq!(start(json!(dir.path())).await.unwrap().status(), 400);
    // The scratch area is not offered as a project.
    let workspaces: Value = client
        .get(format!("http://{addr}/v1/workspaces"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(workspaces, json!([]));
}
