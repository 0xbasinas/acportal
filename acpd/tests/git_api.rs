//! `/v1/git/{op}` and the session `branch` option, against a real git repository.
use acpd::{api::Host, config::Config, registry::Registry, security::Role};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

struct H {
    addr: String,
    token: String,
    client: reqwest::Client,
    root: std::path::PathBuf,
    _dir: tempfile::TempDir,
}
impl H {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir
            .path()
            .join("projects")
            .canonicalize()
            .unwrap_or_else(|_| {
                std::fs::create_dir_all(dir.path().join("projects")).unwrap();
                dir.path().join("projects").canonicalize().unwrap()
            });
        let repo = root.join("calc");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        std::fs::create_dir_all(dir.path().join("outside")).unwrap();
        let config = Config {
            workspace_roots: vec![root.clone()],
            state_directory: dir.path().join("state"),
            ..Default::default()
        };
        let agents =
            json!([{"id":"a","name":"a","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":[]}]);
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
        H {
            addr,
            token: reply["token"].as_str().unwrap().into(),
            client,
            root,
            _dir: dir,
        }
    }
    async fn op(&self, op: &str, body: Value) -> (u16, Value) {
        let r = self
            .client
            .post(format!("http://{}/v1/git/{op}", self.addr))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
}

#[tokio::test]
async fn git_routes_are_scoped_and_work() {
    let h = H::start().await;
    let repo = h.root.join("calc");
    let (s, v) = h.op("status", json!({"path": repo})).await;
    assert_eq!(s, 200);
    assert!(v["out"].as_str().unwrap().starts_with("## main"), "{v}");
    assert_eq!(v["readOnly"], true);

    let outside = h.root.parent().unwrap().join("outside");
    assert_eq!(h.op("status", json!({"path": outside})).await.0, 403);
    assert_eq!(h.op("rm", json!({"path": repo})).await.0, 404);
    assert_eq!(
        h.op(
            "checkout",
            json!({"path": repo, "params": {"branch": "--orphan=x"}})
        )
        .await
        .0,
        400
    );
    assert_eq!(
        h.op(
            "branch-create",
            json!({"path": repo, "params": {"branch": "bad..name"}})
        )
        .await
        .0,
        400
    );

    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    let (_, v) = h
        .op("add", json!({"path": repo, "params": {"file": "a.txt"}}))
        .await;
    assert_eq!(v["code"], 0, "{v}");
    let (_, v) = h
        .op(
            "commit",
            json!({"path": repo, "params": {"message": "second"}}),
        )
        .await;
    // No identity on a CI box is a git failure reported with its exit code, not an HTTP error.
    assert!(v["code"].is_number(), "{v}");
    let (_, v) = h
        .op(
            "branch-create",
            json!({"path": repo, "params": {"branch": "feature/x"}}),
        )
        .await;
    assert_eq!(v["code"], 0, "{v}");
    let (_, v) = h.op("branch-list", json!({"path": repo})).await;
    assert!(v["out"].as_str().unwrap().contains("feature/x"), "{v}");
    let (_, v) = h.op("merging", json!({"path": repo})).await;
    assert_eq!(v["out"], "");
    let (_, v) = h
        .op("diff", json!({"path": repo, "params": {"staged": ""}}))
        .await;
    assert_eq!(v["args"][0], "git");
}

#[tokio::test]
async fn session_can_start_in_its_own_worktree() {
    let h = H::start().await;
    let repo = h.root.join("calc");
    let r = h
        .client
        .post(format!("http://{}/v1/sessions", h.addr))
        .bearer_auth(&h.token)
        .json(&json!({"agentId":"a","workspace":repo,"branch":{"name":"acportal/try","create":true,"worktree":true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
    let v: Value = r.json().await.unwrap();
    let folder = h.root.join("calc-acportal-try");
    assert!(folder.join("a.txt").exists());
    assert!(v.to_string().contains("calc-acportal-try"), "{v}");
    // The same branch again cannot reuse the folder.
    let r = h
        .client
        .post(format!("http://{}/v1/sessions", h.addr))
        .bearer_auth(&h.token)
        .json(&json!({"agentId":"a","workspace":repo,"branch":{"name":"acportal/try","create":true,"worktree":true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 409);
}
