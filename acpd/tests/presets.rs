//! `GET /v1/presets` against an isolated host. No agent process is started.
use acpd::{
    api::Host,
    auto_approve::{AutoApproveConfig, AutoRisk},
    config::Config,
    registry::Registry,
    security::Role,
    shell_review::{ModelReviewConfig, ShellReviewConfig},
};
use serde_json::{Value, json};
use std::time::Duration;

fn model() -> ModelReviewConfig {
    ModelReviewConfig {
        base_url: "https://example.test/secret-path".into(),
        model: "m".into(),
        api_key_env: "SUPER_KEY".into(),
        timeout_seconds: 10,
        max_requests_per_minute: 6,
        max_command_bytes: 2048,
    }
}

#[tokio::test]
async fn presets_follow_config_and_a_preset_field_does_not_start_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("projects");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let missing = dir.path().join("no-such-agent");
    let mut config = Config {
        workspace_roots: vec![root.clone()],
        state_directory: dir.path().join("state"),
        ..Default::default()
    };
    config.auto_approve.insert(
        "goose".into(),
        AutoApproveConfig {
            max_risk: AutoRisk::Medium,
            model: Some(model()),
            ..Default::default()
        },
    );
    config.shell_review.insert(
        "goose".into(),
        ShellReviewConfig {
            rules: true,
            model: Some(model()),
            ..Default::default()
        },
    );
    config
        .auto_approve
        .insert("idle".into(), AutoApproveConfig::default());
    config
        .shell_review
        .insert("quiet".into(), ShellReviewConfig::default());
    config.auto_approve.insert(
        "far".into(),
        AutoApproveConfig {
            max_risk: AutoRisk::Medium,
            ..Default::default()
        },
    );
    config.shell_review.insert(
        "far".into(),
        ShellReviewConfig {
            rules: true,
            ..Default::default()
        },
    );
    let agents = json!([
        {"id":"goose","name":"Goose","command":missing,"ownTools":true,"env":{"SUPER_KEY":"preset-marker-value"}},
        {"id":"plain","name":"Plain","command":missing},
        {"id":"idle","name":"Idle","command":missing},
        {"id":"quiet","name":"Quiet","command":missing},
        {"id":"far","name":"Far","transport":"ws","url":"wss://example.test/acp/far","peer":"box","workspaceAlias":"work"}
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

    let open = client
        .get(format!("http://{addr}/v1/presets?agent=goose"))
        .send()
        .await
        .unwrap();
    assert_eq!(open.status(), 401);

    let code = host.security.new_pairing_code_for(Role::Phone).unwrap();
    let reply: Value = client
        .post(format!("http://{addr}/v1/pair"))
        .json(&json!({"code": code, "deviceName": "preset-test"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = reply["token"].as_str().unwrap().to_owned();
    let peer_code = host.security.new_pairing_code_for(Role::Peer).unwrap();
    let peer_reply: Value = client
        .post(format!("http://{addr}/v1/pair"))
        .json(&json!({"code": peer_code, "deviceName": "preset-peer"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let peer = peer_reply["token"].as_str().unwrap();

    let get = |path: &str, bearer: &str| {
        client
            .get(format!("http://{addr}{path}"))
            .bearer_auth(bearer)
            .send()
    };
    let body = async |path: &str| -> (u16, String) {
        let response = get(path, &token).await.unwrap();
        let status = response.status().as_u16();
        (status, response.text().await.unwrap())
    };

    let status: Value = get("/v1/status", &token)
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
            .any(|feature| feature == "presets")
    );

    let peer_status = get("/v1/presets?agent=goose", peer).await.unwrap().status();
    assert_eq!(peer_status, 403);

    let (code, goose) = body("/v1/presets?agent=goose").await;
    assert_eq!(code, 200, "{goose}");
    assert!(!goose.contains("preset-marker-value"), "{goose}");
    assert!(!goose.contains("example.test"), "{goose}");
    assert!(!goose.contains("SUPER_KEY"), "{goose}");
    assert!(!goose.contains("secret-path"), "{goose}");
    assert!(!goose.to_lowercase().contains("full access"), "{goose}");
    let parsed: Value = serde_json::from_str(&goose).unwrap();
    let ids: Vec<&str> = parsed["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|preset| preset["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["supervised", "auto-edits", "auto-review"]);
    assert!(goose.contains("Undecided lines still ask"), "{goose}");
    assert!(goose.contains("own tools are not covered"), "{goose}");
    assert_eq!(parsed["presets"][0]["workspaceAccess"]["writeFiles"], true);
    assert!(
        parsed["presets"][0]["workspaceAccess"]
            .get("formElicitation")
            .is_none()
    );

    let (code, plain) = body("/v1/presets?agent=plain").await;
    assert_eq!(code, 200, "{plain}");
    let plain: Value = serde_json::from_str(&plain).unwrap();
    assert_eq!(plain["presets"].as_array().unwrap().len(), 1);
    assert_eq!(plain["presets"][0]["id"], "supervised");
    let effects = plain["presets"][0]["effects"].to_string();
    assert!(
        effects.contains("Each write asks you before it runs."),
        "{effects}"
    );
    assert!(!effects.contains("allowed once"), "{effects}");

    for agent in ["idle", "quiet", "far"] {
        let (code, text) = body(&format!("/v1/presets?agent={agent}")).await;
        assert_eq!(code, 200, "{agent} {text}");
        let parsed: Value = serde_json::from_str(&text).unwrap();
        let ids: Vec<&str> = parsed["presets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|preset| preset["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["supervised"], "{agent}");
    }

    let (code, _) = body("/v1/presets?agent=missing").await;
    assert_eq!(code, 404);
    let (code, _) = body("/v1/presets?agent=a.b").await;
    assert_eq!(code, 400);

    let created = client
        .post(format!("http://{addr}/v1/sessions"))
        .bearer_auth(&token)
        .json(&json!({"agentId":"goose","workspace":root,"preset":"auto-edits","workspaceAccess":{"readFiles":false,"writeFiles":false,"terminal":false}}))
        .send()
        .await
        .unwrap();
    assert!(created.status().is_client_error(), "{}", created.status());
    let sessions: Value = get("/v1/sessions", &token)
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(sessions.as_array().map(Vec::len), Some(0), "{sessions}");
}
