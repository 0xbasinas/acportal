//! Editing an agent added from an app: `GET /v1/agents/{id}` returns its saved definition,
//! `POST /v1/agents` with the same id replaces it, and the agent list reports its sandbox.
use acpd::{api::Host, config::Config, registry::Registry, security::Role};
use serde_json::{Value, json};
use std::time::Duration;

#[tokio::test]
async fn app_added_agents_can_be_read_back_edited_and_report_their_sandbox() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("projects");
    std::fs::create_dir_all(&root).unwrap();
    let config = Config {
        workspace_roots: vec![root.canonicalize().unwrap()],
        state_directory: dir.path().join("state"),
        ..Default::default()
    };
    let mock = env!("CARGO_BIN_EXE_mock-acp-agent");
    // One agent from the host's own registry file; it is not editable from an app.
    let registry = json!([{"id":"hosted","name":"Hosted","command":mock}]);
    let host = Host::new(config, Registry::parse(&registry.to_string()).unwrap()).unwrap();
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
    let get = |path: String| {
        client
            .get(format!("http://{addr}{path}"))
            .bearer_auth(&token)
            .send()
    };
    let save = |body: Value| {
        client
            .post(format!("http://{addr}/v1/agents"))
            .bearer_auth(&token)
            .json(&body)
            .send()
    };

    let added = save(json!({"id":"mine","name":"Mine","command":mock,"args":["--setup-choices"],"env":{"MODE":"approve"}}))
        .await
        .unwrap();
    assert_eq!(added.status(), 201);
    let saved: Value = get("/v1/agents/mine".into())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(saved["command"], mock, "{saved}");
    assert_eq!(saved["args"], json!(["--setup-choices"]));
    assert_eq!(saved["env"], json!({"MODE":"approve"}));
    // Defaults are spelled out for the edit form.
    assert_eq!(saved["sandbox"], "auto");
    assert_eq!(saved["shell"], "auto");
    assert_eq!(saved["ownTools"], false);

    // Saving with the same id replaces it: sandbox off, renamed, disabled.
    let mut edited = saved.clone();
    edited["name"] = json!("Mine (unsandboxed)");
    edited["sandbox"] = json!("off");
    edited["enabled"] = json!(false);
    assert_eq!(save(edited).await.unwrap().status(), 201);
    let saved: Value = get("/v1/agents/mine".into())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(saved["sandbox"], "off", "{saved}");
    assert_eq!(saved["name"], "Mine (unsandboxed)");
    assert_eq!(saved["enabled"], false);
    let list: Value = get("/v1/agents".into())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mine = list
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "mine")
        .unwrap();
    assert_eq!(mine["sandbox"], "off", "{list}");
    assert_eq!(mine["enabled"], false);
    let hosted = list
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "hosted")
        .unwrap();
    assert_eq!(hosted["sandbox"], "auto");

    // Host registry agents and unknown ids are not editable from an app.
    assert_eq!(get("/v1/agents/hosted".into()).await.unwrap().status(), 404);
    assert_eq!(get("/v1/agents/nope".into()).await.unwrap().status(), 404);
    assert_eq!(
        save(json!({"id":"hosted","name":"x","command":mock}))
            .await
            .unwrap()
            .status(),
        409
    );
}
