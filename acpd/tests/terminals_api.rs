//! `/v1/terminals`: the switch, the role and folder checks, limits and a real shell over the socket.
use acpd::{
    api::Host, config::Config, registry::Registry, security::Role, terminals::TerminalConfig,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest, http::HeaderValue};

struct H {
    addr: String,
    token: String,
    peer_token: String,
    client: reqwest::Client,
    root: std::path::PathBuf,
    _dir: tempfile::TempDir,
}
impl H {
    async fn start(terminal: TerminalConfig) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("projects/app")).unwrap();
        std::fs::create_dir_all(dir.path().join("outside")).unwrap();
        let root = dir.path().join("projects").canonicalize().unwrap();
        let config = Config {
            workspace_roots: vec![root.clone()],
            state_directory: dir.path().join("state"),
            terminal,
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
        let pair = |role| {
            let code = host.security.new_pairing_code_for(role).unwrap();
            let client = client.clone();
            let addr = addr.clone();
            async move {
                let reply: Value = client
                    .post(format!("http://{addr}/v1/pair"))
                    .json(&json!({"code": code, "deviceName": "test"}))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                reply["token"].as_str().unwrap().to_string()
            }
        };
        let token = pair(Role::Phone).await;
        let peer_token = pair(Role::Peer).await;
        H {
            addr,
            token,
            peer_token,
            client,
            root,
            _dir: dir,
        }
    }
    async fn open(&self, token: &str, folder: &std::path::Path) -> (u16, Value) {
        let r = self
            .client
            .post(format!("http://{}/v1/terminals", self.addr))
            .bearer_auth(token)
            .json(&json!({"folder": folder, "rows": 24, "cols": 80}))
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
}
fn on() -> TerminalConfig {
    TerminalConfig {
        enabled: true,
        max_terminals: 2,
        ..Default::default()
    }
}

#[tokio::test]
async fn off_by_default_and_not_advertised() {
    let h = H::start(TerminalConfig::default()).await;
    let (status, body) = h.open(&h.token, &h.root.join("app")).await;
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (403, Some("terminal_disabled"))
    );
    let list: Value = h
        .client
        .get(format!("http://{}/v1/terminals", h.addr))
        .bearer_auth(&h.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["enabled"], false);
    let status: Value = h
        .client
        .get(format!("http://{}/v1/status", h.addr))
        .bearer_auth(&h.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        !status["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "terminal")
    );
}

#[tokio::test]
async fn checks_role_folder_and_limit() {
    let h = H::start(on()).await;
    let app = h.root.join("app");
    let status: Value = h
        .client
        .get(format!("http://{}/v1/status", h.addr))
        .bearer_auth(&h.token)
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
            .any(|f| f == "terminal")
    );
    assert_eq!(
        h.open(&h.peer_token, &app).await.0,
        403,
        "a peer host is not a phone"
    );
    assert_eq!(
        h.open(&h.token, &h.root.parent().unwrap().join("outside"))
            .await
            .0,
        403
    );
    assert_eq!(h.open("not-a-token", &app).await.0, 401);
    let (s, a) = h.open(&h.token, &app).await;
    assert_eq!(s, 201, "{a}");
    assert_eq!(h.open(&h.token, &app).await.0, 201);
    assert_eq!(h.open(&h.token, &app).await.0, 429);
    let id = a["id"].as_str().unwrap();
    let url = format!("http://{}/v1/terminals/{id}", h.addr);
    assert_eq!(
        h.client
            .delete(&url)
            .bearer_auth(&h.peer_token)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        h.client
            .delete(&url)
            .bearer_auth(&h.token)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        h.client
            .delete(&url)
            .bearer_auth(&h.token)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_over_the_socket_resumes_from_an_offset() {
    let h = H::start(on()).await;
    let (_, t) = h.open(&h.token, &h.root.join("app")).await;
    let id = t["id"].as_str().unwrap().to_string();
    let connect = |after: u64| {
        let mut request = format!("ws://{}/v1/terminals/{id}/ws?after={after}", h.addr)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", h.token)).unwrap(),
        );
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_static("acpd.terminal.v1"),
        );
        async move { tokio_tungstenite::connect_async(request).await.unwrap().0 }
    };
    let mut ws = connect(0).await;
    let hello = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let hello: Value = serde_json::from_str(hello.to_text().unwrap()).unwrap();
    assert_eq!(
        (hello["type"].as_str(), hello["you"].as_bool()),
        (Some("hello"), Some(true))
    );
    ws.send(Message::Binary(b"echo over-the-wire\r".to_vec().into()))
        .await
        .unwrap();
    let mut seen = Vec::<u8>::new();
    let mut next = 0u64;
    let end = std::time::Instant::now() + Duration::from_secs(20);
    while !String::from_utf8_lossy(&seen).contains("over-the-wire")
        && std::time::Instant::now() < end
    {
        if let Ok(Some(Ok(Message::Binary(frame)))) =
            tokio::time::timeout(Duration::from_millis(500), ws.next()).await
        {
            let offset = u64::from_be_bytes(frame[..8].try_into().unwrap());
            assert_eq!(offset, next, "frames are contiguous");
            next = offset + (frame.len() - 8) as u64;
            seen.extend_from_slice(&frame[8..]);
        }
    }
    assert!(String::from_utf8_lossy(&seen).contains("over-the-wire"));
    drop(ws);
    // A new connection from the offset it already has gets nothing twice.
    let mut again = connect(next).await;
    let _hello = again.next().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(700), again.next())
            .await
            .is_err(),
        "no repeated output"
    );
    again
        .send(Message::Text(
            json!({"type":"resize","rows":30,"cols":90})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let info: Value = h
        .client
        .get(format!("http://{}/v1/terminals", h.addr))
        .bearer_auth(&h.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["terminals"].as_array().unwrap().len(), 1);
}
