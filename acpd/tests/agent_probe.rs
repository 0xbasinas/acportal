//! `POST /v1/agents/{id}/probe` and the `setup` field of `GET /v1/agents`.
use acpd::{api::Host, config::Config, registry::Registry, security::Role};
use serde_json::{Value, json};
use std::time::Duration;

#[tokio::test]
async fn probe_reports_and_caches_choices_without_leaving_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("projects");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let config = Config {
        workspace_roots: vec![root.clone()],
        state_directory: dir.path().join("state"),
        ..Default::default()
    };
    let mock = env!("CARGO_BIN_EXE_mock-acp-agent");
    let agents = json!([
        {"id":"a","name":"a","command":mock,"args":["--setup-choices"]},
        {"id":"plain","name":"plain","command":mock,"args":[]}
    ]);
    let host = Host::new(config, Registry::parse(&agents.to_string()).unwrap()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let router = host.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let code = host.security.new_pairing_code_for(Role::Phone).unwrap();
    let reply: Value = client
        .post(format!("http://{addr}/v1/pair"))
        .json(&json!({"code": code, "deviceName": "test"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = reply["token"].as_str().unwrap().to_owned();
    let get = |path: &str| {
        client
            .get(format!("http://{addr}{path}"))
            .bearer_auth(&token)
            .send()
    };
    let probe = |agent: &str, workspace: Value| {
        client
            .post(format!("http://{addr}/v1/agents/{agent}/probe"))
            .bearer_auth(&token)
            .json(&json!({"workspace": workspace}))
            .send()
    };

    let agents: Value = get("/v1/agents").await.unwrap().json().await.unwrap();
    assert!(agents[0].get("setup").is_none(), "{agents}");

    let r = probe("a", json!(root)).await.unwrap();
    assert_eq!(r.status(), 200);
    let v: Value = r.json().await.unwrap();
    assert_eq!(
        v["setup"]["modes"]["availableModes"][1]["id"], "code",
        "{v}"
    );
    assert_eq!(
        v["setup"]["configOptions"][0]["currentValue"], "small",
        "{v}"
    );
    assert!(
        v["setup"].get("sessionId").is_none(),
        "only choices are kept: {v}"
    );

    // The throwaway session is gone, and the choices are cached for the agent list.
    let sessions: Value = get("/v1/sessions").await.unwrap().json().await.unwrap();
    assert_eq!(sessions.as_array().map(Vec::len), Some(0), "{sessions}");
    let agents: Value = get("/v1/agents").await.unwrap().json().await.unwrap();
    let a = agents
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == "a")
        .unwrap();
    assert_eq!(a["setup"]["modes"]["currentModeId"], "ask", "{agents}");

    // An agent that offers no choices probes to an empty object.
    let v: Value = probe("plain", json!(root))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["setup"], json!({}));
    // Folders outside the approved roots and unknown agents are refused.
    assert_eq!(probe("a", json!(dir.path())).await.unwrap().status(), 400);
    assert_eq!(probe("nope", json!(root)).await.unwrap().status(), 400);
}
