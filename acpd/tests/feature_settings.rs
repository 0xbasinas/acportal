//! `/v1/settings/features`: turning terminals and preview on and off from an app, without a restart.
use acpd::{api::Host, config::Config, registry::Registry, security::Role};
use serde_json::{Value, json};
use std::time::Duration;

struct H {
    addr: String,
    phone: String,
    peer: String,
    client: reqwest::Client,
    state: std::path::PathBuf,
    _dir: tempfile::TempDir,
}
impl H {
    async fn start(config: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let mut c = Config {
            state_directory: state.clone(),
            ..Default::default()
        };
        config(&mut c);
        let agents =
            json!([{"id":"a","name":"a","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":[]}]);
        let host = Host::new(c, Registry::parse(&agents.to_string()).unwrap()).unwrap();
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
            state,
            _dir: dir,
        }
    }
    async fn get(&self) -> Value {
        self.client
            .get(format!("http://{}/v1/settings/features", self.addr))
            .bearer_auth(&self.phone)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    async fn put(&self, token: &str, body: Value) -> (u16, Value) {
        let r = self
            .client
            .put(format!("http://{}/v1/settings/features", self.addr))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.json().await.unwrap_or(Value::Null))
    }
    async fn features(&self) -> Vec<String> {
        let s: Value = self
            .client
            .get(format!("http://{}/v1/status", self.addr))
            .bearer_auth(&self.phone)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        s["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap().to_string())
            .collect()
    }
}

#[tokio::test]
async fn terminals_and_preview_can_be_switched_on_and_off_without_a_restart() {
    let h = H::start(|_| {}).await;
    let now = h.get().await;
    assert_eq!(
        (
            now["terminal"]["enabled"].as_bool(),
            now["terminal"]["setHere"].as_bool()
        ),
        (Some(false), Some(false))
    );
    assert!(!h.features().await.contains(&"terminal".to_string()));

    let (status, after) = h
        .put(
            &h.phone,
            json!({"terminal": true, "previewEnabled": true, "previewPorts": [5173, 3000]}),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        (
            after["terminal"]["enabled"].as_bool(),
            after["terminal"]["setHere"].as_bool()
        ),
        (Some(true), Some(true))
    );
    assert_eq!(after["preview"]["ports"], json!([5173, 3000]));
    let features = h.features().await;
    assert!(
        features.contains(&"terminal".to_string()) && features.contains(&"preview".to_string())
    );
    assert!(
        h.state.join("user-settings.json").exists(),
        "kept with the phone-added folders and agents"
    );

    // A missing key leaves the others alone; off is off at once.
    let (_, off) = h.put(&h.phone, json!({"terminal": false})).await;
    assert_eq!(off["terminal"]["enabled"], false);
    assert_eq!(off["preview"]["enabled"], true);
    assert!(!h.features().await.contains(&"terminal".to_string()));
}

#[tokio::test]
async fn null_falls_back_to_what_the_host_config_says() {
    let h = H::start(|c| {
        c.terminal.enabled = true;
    })
    .await;
    assert_eq!(h.get().await["terminal"]["enabled"], true);
    h.put(&h.phone, json!({"terminal": false})).await;
    assert_eq!(h.get().await["terminal"]["enabled"], false);
    let (_, back) = h.put(&h.phone, json!({"terminal": null})).await;
    assert_eq!(
        (
            back["terminal"]["enabled"].as_bool(),
            back["terminal"]["setHere"].as_bool()
        ),
        (Some(true), Some(false))
    );
}

#[tokio::test]
async fn only_a_phone_may_change_it_and_bad_ports_are_refused() {
    let h = H::start(|_| {}).await;
    assert_eq!(
        h.put(&h.peer, json!({"terminal": true})).await.0,
        403,
        "a peer host is not a phone"
    );
    assert_eq!(h.put("nope", json!({"terminal": true})).await.0, 401);
    assert_eq!(h.put(&h.phone, json!({"previewPorts": [0]})).await.0, 400);
    assert_eq!(
        h.put(
            &h.phone,
            json!({"previewPorts": (1..=40).collect::<Vec<u16>>()})
        )
        .await
        .0,
        400
    );
    assert_eq!(h.put(&h.phone, json!({"unknownThing": true})).await.0, 422);
    assert_eq!(
        h.get().await["terminal"]["enabled"],
        false,
        "nothing changed"
    );
}
