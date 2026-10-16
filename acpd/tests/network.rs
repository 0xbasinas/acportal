use acpd::{api::Host, config::Config, registry::Registry};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{net::TcpStream, task::JoinHandle, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[tokio::test]
async fn api_workspace_access_negotiates_and_returns_the_immutable_session_policy() {
    let fixture = Fixture::start().await;
    let policy = json!({"readFiles":true,"writeFiles":false,"terminal":false});
    let reply=fixture.client.post(format!("{}/v1/sessions",fixture.base)).bearer_auth(&fixture.token)
        .json(&json!({"agentId":"mock","workspace":fixture.host.config.workspace_roots[0],"workspaceAccess":policy})).send().await.unwrap().error_for_status().unwrap();
    let info: Value = reply.json().await.unwrap();
    assert_eq!(info["workspaceAccess"], policy);
    let session = fixture
        .host
        .sessions
        .get(info["id"].as_str().unwrap().parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        session
            .connection
            .request("_mock/client_capabilities", json!({}))
            .await
            .unwrap(),
        json!({"fs":{"readTextFile":true,"writeTextFile":false},"terminal":false})
    );
    let partial=fixture.client.post(format!("{}/v1/sessions",fixture.base)).bearer_auth(&fixture.token)
        .json(&json!({"agentId":"mock","workspace":fixture.host.config.workspace_roots[0],"workspaceAccess":{"readFiles":false}})).send().await.unwrap();
    assert_eq!(partial.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(fixture.host.sessions.active_count().await, 1);
    fixture.stop().await;
}

#[tokio::test]
async fn phone_pauses_and_resumes_supervisor_approvals_for_its_own_session() {
    let fixture = Fixture::start_configured(&["--delay-ms", "1"], |config| {
        config.orchestrator.enabled = true;
        config.orchestrator.orchestrate = vec!["mock".into()];
    })
    .await;
    let id = fixture.create().await["id"].as_str().unwrap().to_owned();
    let url = format!("{}/v1/sessions/{id}/supervisor", fixture.base);
    let send = |paused: Value| {
        fixture
            .client
            .post(&url)
            .bearer_auth(&fixture.token)
            .json(&json!({ "paused": paused }))
            .send()
    };
    assert_eq!(
        send(json!(true)).await.unwrap().status(),
        reqwest::StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(json!(false)).await.unwrap().status(),
        reqwest::StatusCode::NO_CONTENT
    );
    // A wrong body, no credentials and an unknown session are all refused.
    assert!(send(json!("yes")).await.unwrap().status().is_client_error());
    let anonymous = fixture
        .client
        .post(&url)
        .json(&json!({"paused": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);
    let unknown = fixture
        .client
        .post(format!(
            "{}/v1/sessions/{}/supervisor",
            fixture.base,
            uuid::Uuid::new_v4()
        ))
        .bearer_auth(&fixture.token)
        .json(&json!({"paused": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);
    fixture.stop().await;

    // With delegation off the session is not part of any task tree, so there is nothing to pause.
    let plain = Fixture::start().await;
    let id = plain.create().await["id"].as_str().unwrap().to_owned();
    let reply = plain
        .client
        .post(format!("{}/v1/sessions/{id}/supervisor", plain.base))
        .bearer_auth(&plain.token)
        .json(&json!({"paused": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), reqwest::StatusCode::NOT_FOUND);
    plain.stop().await;
}

#[tokio::test]
async fn phone_adds_lists_and_removes_folders_and_they_survive_restart() {
    let fixture = Fixture::start().await;
    let extra = tempfile::tempdir().unwrap();
    let project = extra.path().join("project");
    std::fs::create_dir_all(project.join("inner")).unwrap();
    std::fs::write(extra.path().join("not-a-folder.txt"), "x").unwrap();
    let auth = |request: reqwest::RequestBuilder| request.bearer_auth(&fixture.token);
    let create = |path: &std::path::Path| {
        auth(fixture.client.post(format!("{}/v1/sessions", fixture.base)))
            .json(&json!({"agentId":"mock","workspace":path}))
    };
    // Before adding, the folder is outside every root.
    assert_eq!(
        create(&project).send().await.unwrap().status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    // Directory listing shows folders only, with a parent.
    let listing: Value = auth(
        fixture
            .client
            .get(format!("{}/v1/filesystem/directories", fixture.base)),
    )
    .query(&[("path", extra.path().to_str().unwrap())])
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    let names: Vec<_> = listing["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["project"]);
    assert!(listing["parent"].is_string());
    // A file is not a folder; a missing path is rejected.
    for bad in [
        extra.path().join("not-a-folder.txt"),
        extra.path().join("missing"),
    ] {
        let reply = auth(
            fixture
                .client
                .post(format!("{}/v1/workspaces", fixture.base)),
        )
        .json(&json!({"path":bad}))
        .send()
        .await
        .unwrap();
        assert_eq!(reply.status(), reqwest::StatusCode::BAD_REQUEST);
    }
    let added = auth(
        fixture
            .client
            .post(format!("{}/v1/workspaces", fixture.base)),
    )
    .json(&json!({"path":project}))
    .send()
    .await
    .unwrap();
    assert_eq!(added.status(), reqwest::StatusCode::CREATED);
    let added: Value = added.json().await.unwrap();
    assert_eq!(added["name"], "project");
    assert_eq!(added["removable"], true);
    // It is listed (removable) next to the config's own root (not removable) and usable at once.
    let list: Value = auth(
        fixture
            .client
            .get(format!("{}/v1/workspaces", fixture.base)),
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert!(
        list.iter()
            .any(|e| e["name"] == "project" && e["removable"] == true)
    );
    assert!(
        list.iter()
            .any(|e| e["name"] == "workspace" && e["removable"] == false)
    );
    assert_eq!(
        create(&project.join("inner"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::CREATED
    );
    // The host's own root cannot be removed from the phone.
    let own = fixture.host.config.workspace_roots[0].clone();
    let refused = auth(
        fixture
            .client
            .delete(format!("{}/v1/workspaces", fixture.base)),
    )
    .json(&json!({"path":own}))
    .send()
    .await
    .unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::NOT_FOUND);
    // Restart: the saved folder is still authorized.
    let fixture = fixture.restart().await;
    let again = fixture
        .client
        .post(format!("{}/v1/sessions", fixture.base))
        .bearer_auth(&fixture.token)
        .json(&json!({"agentId":"mock","workspace":project}))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), reqwest::StatusCode::CREATED);
    // Removal makes it unusable again.
    let removed = fixture
        .client
        .delete(format!("{}/v1/workspaces", fixture.base))
        .bearer_auth(&fixture.token)
        .json(&json!({"path":project}))
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), reqwest::StatusCode::NO_CONTENT);
    let denied = fixture
        .client
        .post(format!("{}/v1/sessions", fixture.base))
        .bearer_auth(&fixture.token)
        .json(&json!({"agentId":"mock","workspace":project}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::BAD_REQUEST);
    fixture.stop().await;
}

#[tokio::test]
async fn phone_checks_registers_and_removes_agents_and_they_survive_restart() {
    let fixture = Fixture::start().await;
    let mock = env!("CARGO_BIN_EXE_mock-acp-agent");
    let auth = |request: reqwest::RequestBuilder| request.bearer_auth(&fixture.token);
    // Check: a real executable is found; a missing one is reported, not an error.
    let found: Value = auth(
        fixture
            .client
            .post(format!("{}/v1/agents/check", fixture.base)),
    )
    .json(&json!({"command":mock}))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(found["found"], true);
    assert!(found["executable"].is_string());
    let missing: Value = auth(
        fixture
            .client
            .post(format!("{}/v1/agents/check", fixture.base)),
    )
    .json(&json!({"command":"definitely-not-installed-acp-cli"}))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(missing["found"], false);
    // Register: invalid id, missing executable and a host-defined id are refused.
    let register =
        |body: Value| auth(fixture.client.post(format!("{}/v1/agents", fixture.base))).json(&body);
    for body in [
        json!({"id":"bad id!","name":"X","command":mock}),
        json!({"id":"ghost","name":"Ghost","command":"definitely-not-installed-acp-cli"}),
    ] {
        assert_eq!(
            register(body).send().await.unwrap().status(),
            reqwest::StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        register(json!({"id":"mock","name":"Replace","command":mock}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::CONFLICT
    );
    let created = register(
        json!({"id":"extra","name":"Extra agent","command":mock,"args":["--delay-ms","1"]}),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(created.status(), reqwest::StatusCode::CREATED);
    let list: Value = auth(fixture.client.get(format!("{}/v1/agents", fixture.base)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let find = |list: &Value, id: &str| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .cloned()
    };
    assert_eq!(find(&list, "extra").unwrap()["removable"], true);
    assert_eq!(find(&list, "mock").unwrap()["removable"], false);
    let workspace = fixture.host.config.workspace_roots[0].clone();
    let start = |id: &str| {
        auth(fixture.client.post(format!("{}/v1/sessions", fixture.base)))
            .json(&json!({"agentId":id,"workspace":workspace}))
    };
    assert_eq!(
        start("extra").send().await.unwrap().status(),
        reqwest::StatusCode::CREATED
    );
    // The host's own agent cannot be removed from the phone.
    assert_eq!(
        auth(
            fixture
                .client
                .delete(format!("{}/v1/agents/mock", fixture.base))
        )
        .send()
        .await
        .unwrap()
        .status(),
        reqwest::StatusCode::NOT_FOUND
    );
    // Restart keeps it; removal makes it unknown again.
    let fixture = fixture.restart().await;
    let workspace = fixture.host.config.workspace_roots[0].clone();
    let again = fixture
        .client
        .post(format!("{}/v1/sessions", fixture.base))
        .bearer_auth(&fixture.token)
        .json(&json!({"agentId":"extra","workspace":workspace}))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), reqwest::StatusCode::CREATED);
    let removed = fixture
        .client
        .delete(format!("{}/v1/agents/extra", fixture.base))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap();
    assert_eq!(removed.status(), reqwest::StatusCode::NO_CONTENT);
    let gone = fixture
        .client
        .post(format!("{}/v1/sessions", fixture.base))
        .bearer_auth(&fixture.token)
        .json(&json!({"agentId":"extra","workspace":workspace}))
        .send()
        .await
        .unwrap();
    assert!(gone.status().is_client_error());
    fixture.stop().await;
}

#[tokio::test]
async fn phone_locates_installed_commands_by_name_only() {
    let fixture = Fixture::start().await;
    let url = format!("{}/v1/agents/locate", fixture.base);
    // Needs the credential like every other route.
    let anonymous = fixture
        .client
        .post(&url)
        .json(&json!({"commands":["cargo"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), reqwest::StatusCode::UNAUTHORIZED);
    let found: Value = fixture
        .client
        .post(&url)
        .bearer_auth(&fixture.token)
        .json(&json!({"commands":["cargo","definitely-not-installed-acp-cli"]}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(found[0]["command"], "cargo");
    assert_eq!(found[0]["found"], true);
    assert!(found[0]["executable"].is_string());
    assert_eq!(found[1]["found"], false);
    assert!(found[1].get("executable").is_none());
    // Names only: paths, empty names and long lists are refused.
    for body in [
        json!({"commands":["/usr/bin/cargo"]}),
        json!({"commands":["sub/cargo"]}),
        json!({"commands":[""]}),
        json!({"commands":(0..65).map(|n| format!("tool{n}")).collect::<Vec<_>>()}),
    ] {
        let reply = fixture
            .client
            .post(&url)
            .bearer_auth(&fixture.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), reqwest::StatusCode::BAD_REQUEST, "{body}");
    }
    fixture.stop().await;
}

#[tokio::test]
async fn api_mcp_configuration_is_validated_forwarded_and_not_catalogued() {
    let fixture = Fixture::start_with_args(&["--mcp-http"]).await;
    let definitions = json!([{"type":"http","name":"docs","url":"https://example.com/mcp","headers":[{"name":"Authorization","value":"synthetic-network-secret"}]}]);
    let response = fixture.client.post(format!("{}/v1/sessions", fixture.base)).bearer_auth(&fixture.token)
        .json(&json!({"agentId":"mock","workspace":fixture.host.config.workspace_roots[0],"mcpServers":definitions}))
        .send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let info: Value = response.json().await.unwrap();
    assert!(!info.to_string().contains("synthetic-network-secret"));
    let session = fixture
        .host
        .sessions
        .get(info["id"].as_str().unwrap().parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        session
            .connection
            .request("_mock/mcp_servers", json!({}))
            .await
            .unwrap(),
        definitions
    );
    let rejected = fixture.client.post(format!("{}/v1/sessions", fixture.base)).bearer_auth(&fixture.token)
        .json(&json!({"agentId":"mock","workspace":fixture.host.config.workspace_roots[0],"mcpServers":[{"type":"sse","name":"events","url":"https://example.com/events","headers":[]}]}))
        .send().await.unwrap();
    assert_eq!(rejected.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(fixture.host.sessions.active_count().await, 1);
    fixture.stop().await;
}

#[tokio::test]
async fn restart_retains_metadata_and_credentials_without_restarting_or_replaying_work() {
    let fixture = Fixture::start().await;
    let original = fixture.create().await;
    let id = original["id"].as_str().unwrap().to_owned();
    let mut socket = fixture.connect(&id, 0).await;
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    socket.send(Message::Text(json!({"jsonrpc":"2.0","id":"pending-before-restart","method":"session/prompt","params":{"sessionId":original["acpSessionId"],"prompt":[{"type":"text","text":"prompt text must not enter the metadata catalog"}]}}).to_string().into())).await.unwrap();
    receive(&mut socket, |value| {
        value["message"]["method"] == "session/request_permission"
    })
    .await;
    socket.close(None).await.unwrap();
    drop(socket);
    let catalog =
        std::fs::read_to_string(fixture.host.config.state_directory.join("sessions.json")).unwrap();
    assert!(!catalog.contains("prompt text must not enter"));
    assert!(!catalog.contains("session/request_permission"));
    let fixture = fixture.restart().await;
    assert_eq!(fixture.host.sessions.active_count().await, 0);
    let agents: Value = fixture
        .client
        .get(format!("{}/v1/agents", fixture.base))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        agents[0]["status"], "available",
        "retained interrupted sessions must not imply a live agent"
    );
    let retained: Value = fixture
        .client
        .get(format!("{}/v1/sessions/{id}", fixture.base))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(retained["status"], "interrupted");
    assert_eq!(retained["acpSessionId"], original["acpSessionId"]);
    assert!(
        fixture
            .host
            .sessions
            .get(id.parse().unwrap())
            .await
            .is_err()
    );
    let resumed:Value=fixture.client.post(format!("{}/v1/sessions",fixture.base)).bearer_auth(&fixture.token).json(&json!({"agentId":"mock","workspace":original["workspace"],"loadSessionId":original["acpSessionId"]})).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert_ne!(resumed["id"], original["id"]);
    assert_eq!(resumed["acpSessionId"], original["acpSessionId"]);
    let session = fixture
        .host
        .sessions
        .get(resumed["id"].as_str().unwrap().parse().unwrap())
        .await
        .unwrap();
    assert!(session.replay(0).pending_permissions.is_empty());
    assert!(
        !session
            .replay(0)
            .events
            .iter()
            .any(|event| event.message["method"] == "session/prompt")
    );
    drop(session);
    let response = fixture
        .client
        .delete(format!("{}/v1/sessions/{id}", fixture.base))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);
    let fixture = fixture.restart().await;
    assert!(
        fixture
            .host
            .sessions
            .metadata(id.parse().unwrap())
            .await
            .is_err()
    );
    assert_eq!(fixture.host.sessions.list().await.len(), 1);
    fixture.stop().await;
}

#[tokio::test]
async fn terminal_permission_reconnect_preserves_one_command_and_released_output() {
    let fixture = Fixture::start_with_args(&["--terminal-during-prompt"]).await;
    let info = fixture.create().await;
    let id = info["id"].as_str().unwrap();
    let mut socket = fixture.connect(id, 0).await;
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    let prompt = json!({"jsonrpc":"2.0","id":"terminal-prompt","method":"session/prompt","params":{"sessionId":info["acpSessionId"],"prompt":[{"type":"text","text":"Run the terminal fixture"}]}});
    socket
        .send(Message::Text(prompt.to_string().into()))
        .await
        .unwrap();
    let consent = receive(&mut socket, |value| {
        value["message"]["method"] == "session/request_permission"
    })
    .await;
    assert_eq!(
        consent["message"]["params"]["_meta"]["acpdSource"],
        "host-terminal"
    );
    socket.close(None).await.unwrap();
    drop(socket);
    let mut socket = fixture
        .connect(id, consent["sequence"].as_u64().unwrap())
        .await;
    let pending = receive(&mut socket, |value| value["type"] == "pending_permission").await;
    assert_eq!(pending["message"]["id"], consent["message"]["id"]);
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    socket.send(Message::Text(json!({"jsonrpc":"2.0","id":pending["message"]["id"],"result":{"outcome":{"outcome":"selected","optionId":"acpd-command-allow"}}}).to_string().into())).await.unwrap();
    let agent_permission = receive(&mut socket, |value| {
        value["message"]["method"] == "session/request_permission"
    })
    .await;
    socket.send(Message::Text(json!({"jsonrpc":"2.0","id":agent_permission["message"]["id"],"result":{"outcome":{"outcome":"selected","optionId":"deny"}}}).to_string().into())).await.unwrap();
    receive(&mut socket, |value| {
        value["message"]["id"] == "terminal-prompt"
            && value["message"]["result"]["stopReason"] == "end_turn"
    })
    .await;
    socket
        .send(Message::Text(prompt.to_string().into()))
        .await
        .unwrap();
    receive(&mut socket, |value| {
        value["message"]["id"] == "terminal-prompt"
            && value["message"]["result"]["stopReason"] == "end_turn"
    })
    .await;
    let session = fixture
        .host
        .sessions
        .get(id.parse().unwrap())
        .await
        .unwrap();
    let replay = session.replay(0);
    assert_eq!(
        replay
            .events
            .iter()
            .filter(
                |event| event.message["method"] == "session/request_permission"
                    && event.message["params"]["_meta"]["acpdSource"] == "host-terminal"
            )
            .count(),
        1
    );
    assert!(replay.events.iter().any(
        |event| event.message["params"]["update"]["_meta"]["acpdTerminal"]["output"]
            == "ACP terminal output verified.\n"
            && event.message["params"]["update"]["_meta"]["acpdTerminal"]["exitStatus"]["exitCode"]
                == 0
    ));
    socket.close(None).await.unwrap();
    fixture.stop().await;
}

#[tokio::test]
async fn protocol_authentication_finishes_waiting_session_and_replays_without_recreating_it() {
    let fixture = Fixture::start_with_args(&["--require-auth"]).await;
    let waiting = fixture.create().await;
    assert_eq!(waiting["status"], "authentication_required");
    assert_eq!(waiting["acpSessionId"], "");
    let agents: Value = fixture
        .client
        .get(format!("{}/v1/agents", fixture.base))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(agents[0]["status"], "authentication_required");
    let id = waiting["id"].as_str().unwrap();
    let mut socket = fixture.connect(id, 0).await;
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    socket.send(Message::Text(json!({"jsonrpc":"2.0","id":"invalid-login","method":"authenticate","params":{"methodId":"invented"}}).to_string().into())).await.unwrap();
    receive(&mut socket, |value| {
        value["message"]["id"] == "invalid-login" && value["message"].get("error").is_some()
    })
    .await;
    let request = json!({"jsonrpc":"2.0","id":"login","method":"authenticate","params":{"methodId":"mock-login"}});
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    receive(&mut socket, |value| {
        value["message"]["id"] == "login" && value["message"]["result"] == json!({})
    })
    .await;
    let ready: Value = fixture
        .client
        .get(format!("{}/v1/sessions/{id}", fixture.base))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["acpSessionId"], "mock-1");
    let agents: Value = fixture
        .client
        .get(format!("{}/v1/agents", fixture.base))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(agents[0]["status"], "running");
    socket.close(None).await.unwrap();
    drop(socket);
    let mut socket = fixture.connect(id, 0).await;
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    receive(&mut socket, |value| {
        value["message"]["id"] == "login" && value["message"]["result"] == json!({})
    })
    .await;
    assert_eq!(
        fixture
            .host
            .sessions
            .get(id.parse().unwrap())
            .await
            .unwrap()
            .metadata()
            .acp_session_id,
        "mock-1"
    );
    socket.close(None).await.unwrap();
    fixture.stop().await;
}

#[tokio::test]
async fn host_write_consent_survives_socket_disconnect_and_commits_only_once() {
    let fixture = Fixture::start().await;
    let info = fixture.create().await;
    let id = info["id"].as_str().unwrap();
    let path = fixture.host.config.workspace_roots[0].join("approved.txt");
    let mut socket = fixture.connect(id, 0).await;
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    let request = json!({"jsonrpc":"2.0","id":"phone-write","method":"_mock/write_file","params":{"sessionId":info["acpSessionId"],"path":path,"content":"approved from phone"}});
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    let consent = receive(&mut socket, |value| {
        value["message"]["method"] == "session/request_permission"
    })
    .await;
    assert_eq!(consent["direction"], "host");
    assert_eq!(
        consent["message"]["params"]["_meta"]["acpdSource"],
        "host-filesystem"
    );
    assert!(!path.exists());
    socket.close(None).await.unwrap();
    drop(socket);
    let mut socket = fixture
        .connect(id, consent["sequence"].as_u64().unwrap())
        .await;
    let restored = receive(&mut socket, |value| value["type"] == "pending_permission").await;
    assert_eq!(restored["message"]["id"], consent["message"]["id"]);
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    socket.send(Message::Text(json!({"jsonrpc":"2.0","id":restored["message"]["id"],"result":{"outcome":{"outcome":"selected","optionId":"acpd-write-allow"}}}).to_string().into())).await.unwrap();
    let result = receive(&mut socket, |value| {
        value["message"]["id"] == "phone-write" && value["message"].get("result").is_some()
    })
    .await;
    assert_eq!(result["message"]["result"], json!({}));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "approved from phone"
    );
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    receive(&mut socket, |value| {
        value["message"]["id"] == "phone-write" && value["message"].get("result").is_some()
    })
    .await;
    let session = fixture
        .host
        .sessions
        .get(id.parse().unwrap())
        .await
        .unwrap();
    assert!(session.replay(0).pending_permissions.is_empty());
    assert_eq!(
        session
            .replay(0)
            .events
            .iter()
            .filter(
                |event| event.message["method"] == "session/request_permission"
                    && event.message["params"]["_meta"]["acpdSource"] == "host-filesystem"
            )
            .count(),
        1
    );
    socket.close(None).await.unwrap();
    fixture.stop().await;
}
struct Fixture {
    host: Host,
    base: String,
    token: String,
    client: reqwest::Client,
    task: JoinHandle<()>,
    _directory: tempfile::TempDir,
}
impl Fixture {
    async fn restart(self) -> Self {
        let Self {
            host,
            base: _,
            token,
            client,
            task,
            _directory,
        } = self;
        task.abort();
        let _ = task.await;
        host.sessions.shutdown().await;
        let config = (*host.config).clone();
        let registry = (*host.registry).clone();
        drop(host);
        // `axum::serve` runs each connection in its own task, and aborting the listener does not
        // end them: a keep-alive connection from the client's pool keeps the old router, and so
        // the old host and its session-catalog lock, alive. Close the pool, then wait for those
        // tasks to see the socket close. (This was an intermittent failure under load.)
        drop(client);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let mut attempts = 0;
        let host = loop {
            match Host::new(config.clone(), registry.clone()) {
                Ok(host) => break host,
                Err(error)
                    if attempts < 100
                        && format!("{error:#}").contains("owns the session catalog") =>
                {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => panic!("{error:#}"),
            }
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = host.router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            host,
            base,
            token,
            client,
            task,
            _directory,
        }
    }
    async fn start() -> Self {
        Self::start_with_args(&["--delay-ms", "1"]).await
    }
    async fn start_with_args(args: &[&str]) -> Self {
        Self::start_configured(args, |_| {}).await
    }
    async fn start_configured(args: &[&str], tweak: impl FnOnce(&mut Config)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let mut config = Config {
            workspace_roots: vec![workspace],
            state_directory: directory.path().join("state"),
            ..Default::default()
        };
        tweak(&mut config);
        let registry = Registry::parse(&json!([{"id":"mock","name":"Mock ACP","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args}]).to_string()).unwrap();
        let host = Host::new(config, registry).unwrap();
        let code = host.security.new_pairing_code().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = host.router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let paired: Value = client
            .post(format!("{base}/v1/pair"))
            .json(&json!({"code":code,"deviceName":"Test phone"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        Self {
            host,
            base,
            token: paired["token"].as_str().unwrap().into(),
            client,
            task,
            _directory: directory,
        }
    }
    async fn create(&self) -> Value {
        self.client
            .post(format!("{}/v1/sessions", self.base))
            .bearer_auth(&self.token)
            .json(&json!({"agentId":"mock","workspace":self.host.config.workspace_roots[0]}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    async fn connect(&self, id: &str, after: u64) -> Socket {
        let url = format!(
            "{}/v1/sessions/{id}/connect?after={after}",
            self.base.replace("http:", "ws:")
        );
        for _ in 0..100 {
            let mut request = url.clone().into_client_request().unwrap();
            request.headers_mut().insert(
                "Authorization",
                format!("Bearer {}", self.token).parse().unwrap(),
            );
            request
                .headers_mut()
                .insert("Sec-WebSocket-Protocol", "acpd.v1".parse().unwrap());
            match tokio_tungstenite::connect_async(request).await {
                Ok((socket, _)) => return socket,
                Err(tokio_tungstenite::tungstenite::Error::Http(response))
                    if response.status().as_u16() == 409 =>
                {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                Err(error) => panic!("WS connect: {error}"),
            }
        }
        panic!("previous controller lease did not release")
    }
    async fn stop(self) {
        self.host.sessions.shutdown().await;
        self.task.abort();
    }
}
async fn receive(socket: &mut Socket, condition: impl Fn(&Value) -> bool) -> Value {
    timeout(Duration::from_secs(5), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if condition(&value) {
                        return value;
                    }
                }
                Message::Ping(_) => {
                    socket.flush().await.unwrap();
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap()
}
async fn send(socket: &mut Socket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

#[tokio::test]
async fn discovery_workspace_and_session_routes_require_credentials() {
    let fixture = Fixture::start().await;
    for path in ["/v1/status", "/v1/agents", "/v1/workspaces", "/v1/sessions"] {
        assert_eq!(
            fixture
                .client
                .get(format!("{}{path}", fixture.base))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        assert_eq!(
            fixture
                .client
                .get(format!("{}{path}", fixture.base))
                .bearer_auth(&fixture.token)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    assert_eq!(
        fixture
            .client
            .get(format!("{}/v1/status", fixture.base))
            .bearer_auth(&fixture.token)
            .header("Origin", "https://evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let outside = tempfile::tempdir().unwrap();
    assert_eq!(
        fixture
            .client
            .get(format!("{}/v1/workspaces/browse", fixture.base))
            .bearer_auth(&fixture.token)
            .query(&[("path", outside.path().to_str().unwrap())])
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let session = fixture.create().await;
    let deleted = fixture
        .client
        .delete(format!(
            "{}/v1/sessions/{}",
            fixture.base,
            session["id"].as_str().unwrap()
        ))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);
    fixture.stop().await;
}

#[tokio::test]
async fn socket_disconnect_preserves_prompt_and_pending_permission_then_replays_result() {
    let fixture = Fixture::start().await;
    let session = fixture.create().await;
    let id = session["id"].as_str().unwrap();
    let mut socket = fixture.connect(id, 0).await;
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    let prompt = json!({"jsonrpc":"2.0","id":"mobile-prompt-1","method":"session/prompt","params":{"sessionId":session["acpSessionId"],"prompt":[{"type":"text","text":"network test"}]}});
    send(&mut socket, prompt.clone()).await;
    let permission = receive(&mut socket, |value| {
        value["message"]["method"] == "session/request_permission"
    })
    .await;
    let cursor = permission["sequence"].as_u64().unwrap();
    socket.close(None).await.unwrap();
    drop(socket);
    let running = fixture
        .host
        .sessions
        .get(id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(running.metadata().status, "running");
    let mut reconnected = fixture.connect(id, cursor).await;
    let recovered = receive(&mut reconnected, |value| {
        value["type"] == "pending_permission"
    })
    .await;
    assert_eq!(recovered["message"], permission["message"]);
    receive(&mut reconnected, |value| value["type"] == "replay_complete").await;
    send(&mut reconnected, json!({"jsonrpc":"2.0","id":permission["message"]["id"],"result":{"outcome":{"outcome":"selected","optionId":"allow"}}})).await;
    let result = receive(&mut reconnected, |value| {
        value["message"]["id"] == "mobile-prompt-1" && value["message"].get("result").is_some()
    })
    .await;
    assert_eq!(result["message"]["result"]["stopReason"], "end_turn");
    let latest = result["sequence"].as_u64().unwrap();
    send(&mut reconnected, prompt).await;
    let duplicate = receive(&mut reconnected, |value| {
        value["sequence"]
            .as_u64()
            .is_some_and(|sequence| sequence > latest)
            && value["message"]["id"] == "mobile-prompt-1"
    })
    .await;
    assert_eq!(duplicate["message"], result["message"]);
    assert_eq!(
        running
            .replay(0)
            .events
            .iter()
            .filter(
                |event| event.direction == "client" && event.message["method"] == "session/prompt"
            )
            .count(),
        1
    );
    reconnected.close(None).await.unwrap();
    fixture.stop().await;
}

#[tokio::test]
async fn session_scope_and_revocation_are_enforced_on_socket_input() {
    let fixture = Fixture::start().await;
    let session = fixture.create().await;
    let mut socket = fixture.connect(session["id"].as_str().unwrap(), 0).await;
    receive(&mut socket, |value| value["type"] == "replay_complete").await;
    send(&mut socket, json!({"jsonrpc":"2.0","id":1,"method":"session/prompt","params":{"sessionId":"wrong","prompt":[]}})).await;
    let rejection = receive(&mut socket, |value| {
        value["message"]["id"] == 1 && value["message"].get("error").is_some()
    })
    .await;
    assert_eq!(rejection["message"]["error"]["code"], -32602);
    // An agent error keeps the agent's code instead of becoming ACP auth_required.
    send(
        &mut socket,
        json!({"jsonrpc":"2.0","id":3,"method":"_mock/unknown","params":{}}),
    )
    .await;
    let failed = receive(&mut socket, |value| {
        value["message"]["id"] == 3 && value["message"].get("error").is_some()
    })
    .await;
    assert_eq!(failed["message"]["error"]["code"], -32601);
    assert_eq!(
        failed["message"]["error"]["message"],
        "Agent request failed"
    );
    let device = fixture.host.security.authorize(&fixture.token).unwrap();
    fixture.host.security.revoke(device.id).unwrap();
    send(&mut socket, json!({"jsonrpc":"2.0","id":2,"method":"session/prompt","params":{"sessionId":session["acpSessionId"],"prompt":[]}})).await;
    let revoked = receive(&mut socket, |value| value["type"] == "error").await;
    assert_eq!(revoked["code"], "credentials_revoked_or_expired");
    assert_eq!(
        fixture
            .client
            .get(format!("{}/v1/status", fixture.base))
            .bearer_auth(&fixture.token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    fixture.stop().await;
}

#[tokio::test]
async fn non_loopback_plaintext_and_partial_tls_configuration_are_rejected() {
    let mut config = Config::default();
    config.server.listen = "0.0.0.0:8765".parse().unwrap();
    assert!(config.validate().is_err());
    config.server.tls_certificate = Some("certificate.pem".into());
    assert!(config.validate().is_err());
    config.server.tls_private_key = Some("key.pem".into());
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn takeover_moves_control_between_clients_and_the_old_one_keeps_watching() {
    let fixture = Fixture::start().await;
    let session = fixture.create().await;
    let id = session["id"].as_str().unwrap();
    let open = |query: &str| {
        let mut request = format!(
            "{}/v1/sessions/{id}/connect?{query}",
            fixture.base.replace("http:", "ws:")
        )
        .into_client_request()
        .unwrap();
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", fixture.token).parse().unwrap(),
        );
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "acpd.v1".parse().unwrap());
        tokio_tungstenite::connect_async(request)
    };
    let refused = |result: Result<_, tokio_tungstenite::tungstenite::Error>| matches!(result, Err(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status().as_u16() == 409);
    let mut desktop = fixture.connect(id, 0).await;
    receive(&mut desktop, |value| value["type"] == "replay_complete").await;
    assert!(refused(open("after=0").await.map(|_| ())));
    let (mut phone, _) = open("after=0&takeover=true").await.unwrap();
    receive(&mut desktop, |value| value["type"] == "control_taken").await;
    receive(&mut phone, |value| value["type"] == "replay_complete").await;
    // The replaced client is an observer now.
    send(&mut desktop, json!({"jsonrpc":"2.0","id":"late","method":"session/cancel","params":{"sessionId":session["acpSessionId"]}})).await;
    receive(&mut desktop, |value| {
        value["type"] == "error" && value["code"] == "read_only"
    })
    .await;
    // The new controller prompts; the observer sees the same events.
    send(&mut phone, json!({"jsonrpc":"2.0","id":"phone-1","method":"session/prompt","params":{"sessionId":session["acpSessionId"],"prompt":[{"type":"text","text":"handoff"}]}})).await;
    let permission = receive(&mut phone, |value| {
        value["message"]["method"] == "session/request_permission"
    })
    .await;
    receive(&mut desktop, |value| {
        value["sequence"] == permission["sequence"]
    })
    .await;
    // Closing the replaced client must not release the new controller's lease.
    desktop.close(None).await.unwrap();
    drop(desktop);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(refused(open("after=0").await.map(|_| ())));
    send(&mut phone, json!({"jsonrpc":"2.0","id":permission["message"]["id"],"result":{"outcome":{"outcome":"selected","optionId":"allow"}}})).await;
    receive(&mut phone, |value| {
        value["message"]["id"] == "phone-1" && value["message"].get("result").is_some()
    })
    .await;
    // Taking control back works the same way.
    let (mut back, _) = open("after=0&takeover=true").await.unwrap();
    receive(&mut phone, |value| value["type"] == "control_taken").await;
    receive(&mut back, |value| value["type"] == "replay_complete").await;
    fixture.stop().await;
}
