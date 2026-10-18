//! `/v1/preview`: a dev server on this computer shown through the host, and what it must not do.
use acpd::{
    api::Host,
    config::{Config, PreviewConfig},
    registry::Registry,
    security::Role,
};
use axum::{
    Router,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde_json::{Value, json};
use std::time::Duration;

/// A stand-in dev server: a page that sets a cookie, an echo of what it was sent, and a redirect.
async fn dev_server() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let landed = format!("http://localhost:{port}/landed");
    let app = Router::new()
        .route(
            "/",
            get(|| async {
                (
                    [
                        (header::SET_COOKIE, "sid=1"),
                        (header::CONTENT_TYPE, "text/html"),
                    ],
                    "<h1>Calc</h1>",
                )
            }),
        )
        .route(
            "/echo",
            get(|headers: HeaderMap| async move {
                format!(
                    "auth={} cookie={} origin={}",
                    headers.contains_key(header::AUTHORIZATION),
                    headers.contains_key(header::COOKIE),
                    headers.contains_key(header::ORIGIN)
                )
            }),
        )
        .route(
            "/go",
            get(move || async move {
                (StatusCode::FOUND, [(header::LOCATION, landed)]).into_response()
            }),
        )
        .route(
            "/assets/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    "console.log(1)",
                )
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    port
}

struct H {
    addr: String,
    phone: String,
    peer: String,
    client: reqwest::Client,
    _dir: tempfile::TempDir,
}
impl H {
    async fn start(preview: PreviewConfig) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            state_directory: dir.path().join("state"),
            preview,
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
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let pair = |role| {
            let code = host.security.new_pairing_code_for(role).unwrap();
            let (client, addr) = (client.clone(), addr.clone());
            async move {
                let v: Value = client
                    .post(format!("http://{addr}/v1/pair"))
                    .json(&json!({"code": code, "deviceName": "t"}))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                v["token"].as_str().unwrap().to_string()
            }
        };
        let phone = pair(Role::Phone).await;
        let peer = pair(Role::Peer).await;
        H {
            addr,
            phone,
            peer,
            client,
            _dir: dir,
        }
    }
    async fn get(&self, token: &str, path: &str) -> reqwest::Response {
        self.client
            .get(format!("http://{}/v1/preview/{path}", self.addr))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
    }
}
fn on(port: u16) -> PreviewConfig {
    PreviewConfig {
        enabled: true,
        ports: vec![port],
    }
}

#[tokio::test]
async fn off_by_default_and_not_advertised() {
    let port = dev_server().await;
    let h = H::start(PreviewConfig {
        enabled: false,
        ports: vec![port],
    })
    .await;
    assert_eq!(h.get(&h.phone, &format!("{port}/")).await.status(), 403);
    let status: Value = h
        .client
        .get(format!("http://{}/v1/status", h.addr))
        .bearer_auth(&h.phone)
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
            .any(|f| f == "preview")
    );
}

#[tokio::test]
async fn shows_a_listed_dev_server_without_leaking_the_phone_or_keeping_cookies() {
    let port = dev_server().await;
    let h = H::start(on(port)).await;
    let status: Value = h
        .client
        .get(format!("http://{}/v1/status", h.addr))
        .bearer_auth(&h.phone)
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
            .any(|f| f == "preview")
    );

    let page = h.get(&h.phone, &format!("{port}/")).await;
    assert_eq!(page.status(), 200);
    assert!(
        page.headers().get(header::SET_COOKIE).is_none(),
        "cookies are not kept"
    );
    assert_eq!(page.text().await.unwrap(), "<h1>Calc</h1>");
    assert_eq!(
        h.get(&h.phone, &format!("{port}/assets/app.js"))
            .await
            .text()
            .await
            .unwrap(),
        "console.log(1)"
    );

    // The device token is for the host; the dev server never sees it.
    let echo = h
        .get(&h.phone, &format!("{port}/echo"))
        .await
        .text()
        .await
        .unwrap();
    assert_eq!(echo, "auth=false cookie=false origin=false");

    // A redirect to the dev server's own address stays on the page's origin.
    let go = h.get(&h.phone, &format!("{port}/go")).await;
    assert_eq!(go.status(), 302);
    assert_eq!(go.headers()[header::LOCATION], "/landed");
}

#[tokio::test]
async fn refuses_other_ports_other_roles_and_writes() {
    let port = dev_server().await;
    let h = H::start(on(port)).await;
    let other = if port == 65000 { 65001 } else { 65000 };
    assert_eq!(
        h.get(&h.phone, &format!("{other}/")).await.status(),
        403,
        "only listed ports"
    );
    assert_eq!(
        h.get(&h.peer, &format!("{port}/")).await.status(),
        403,
        "a peer host is not a phone"
    );
    assert_eq!(h.get("nope", &format!("{port}/")).await.status(), 401);
    let post = h
        .client
        .post(format!("http://{}/v1/preview/{port}/", h.addr))
        .bearer_auth(&h.phone)
        .send()
        .await
        .unwrap();
    assert_eq!(post.status(), 405);
}

#[tokio::test]
async fn says_so_when_nothing_is_listening() {
    // A port that was open a moment ago and is closed now.
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let h = H::start(on(closed)).await;
    let reply = h.get(&h.phone, &format!("{closed}/")).await;
    assert_eq!(reply.status(), 502);
    let body: Value = reply.json().await.unwrap();
    assert_eq!(body["error"]["code"], "nothing_listening");
}
