//! Phase 0 measurement: does a real Goose accept the delegation MCP server and use it?
//!
//! Opt-in and costs real model requests, so it is `#[ignore]`d. Run it with the provider key in
//! the environment (never in a file) and read the printed report:
//!
//!   OPENAI_API_KEY=... cargo test -p acpd --test real_goose -- --ignored --nocapture
//!
//! It measures and prints; the only assertions are that the sessions start. Nothing here is a
//! claim about any other agent or version.
use acpd::{
    config::{Config, OrchestratorConfig, RuntimeConfig},
    orchestrator::{Orchestrator, control},
    registry::Registry,
    session::{AcpSession, SessionManager},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

const TOOLS: [&str; 6] = [
    "agents_list",
    "task_delegate",
    "task_status",
    "task_result",
    "task_send",
    "task_cancel",
];

fn agent(id: &str) -> Value {
    json!({"id":id,"name":format!("Goose ({id})"),"command":"goose","args":["acp"],
        "env":{"GOOSE_MODE":"approve","GOOSE_PROVIDER":"openai","GOOSE_MODEL":"deepseek-flash",
               "OPENAI_HOST":"https://api.deepseek.com"}})
}

/// Everything an agent session has published, reduced to what phase 0 asks about.
fn digest(session: &AcpSession) -> Value {
    let replay = session.replay(0);
    let mut calls: BTreeMap<String, Value> = BTreeMap::new();
    let mut text = String::new();
    let mut usage = Value::Null;
    let mut kinds = HashSet::new();
    let mut updates = HashSet::new();
    for event in &replay.events {
        let update = &event.message["params"]["update"];
        if event.message["method"] == "session/update" {
            let name = update["sessionUpdate"].as_str().unwrap_or("?").to_owned();
            updates.insert(name.clone());
            match name.as_str() {
                "agent_message_chunk" => {
                    if let Some(chunk) = update["content"]["text"].as_str() {
                        text.push_str(chunk);
                    }
                }
                "usage_update" => usage = update.clone(),
                "tool_call" | "tool_call_update" => {
                    let id = update["toolCallId"].as_str().unwrap_or("?").to_owned();
                    let entry = calls.entry(id).or_insert_with(|| json!({}));
                    for key in ["title", "kind", "status", "rawInput"] {
                        if !update[key].is_null() {
                            entry[key] = update[key].clone();
                        }
                    }
                    if let Some(content) = update["content"][0]["content"]["text"].as_str() {
                        entry["output"] = json!(content.chars().take(600).collect::<String>());
                    }
                }
                _ => {}
            }
        }
        if event.message["method"] == "session/request_permission" {
            for option in event.message["params"]["options"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(kind) = option["kind"].as_str() {
                    kinds.insert(kind.to_owned());
                }
            }
        }
    }
    let mut kinds: Vec<_> = kinds.into_iter().collect();
    kinds.sort();
    let mut updates: Vec<_> = updates.into_iter().collect();
    updates.sort();
    json!({"tool_calls": calls.values().collect::<Vec<_>>(), "text": text, "usage": usage,
           "permission_option_kinds": kinds, "update_kinds": updates})
}

fn names_a_delegation_tool(permission: &Value) -> bool {
    let text = permission["params"]["toolCall"].to_string();
    TOOLS.iter().any(|tool| text.contains(tool)) || text.contains("acpd-orchestrator")
}

/// Act as the phone for every session until `done` is set: delegation tools are allowed once,
/// everything else is refused. Returns what was decided so the report can show it.
async fn phone(
    manager: Arc<SessionManager>,
    done: Arc<std::sync::atomic::AtomicBool>,
) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut decisions = vec![];
    while !done.load(std::sync::atomic::Ordering::Relaxed) {
        for metadata in manager.list().await {
            let Ok(session) = manager.get(metadata.id).await else {
                continue;
            };
            for permission in session.replay(0).pending_permissions {
                let key = (metadata.id, permission["id"].to_string());
                if !seen.insert(key) {
                    continue;
                }
                let allow = names_a_delegation_tool(&permission);
                let wanted = if allow { "allow_once" } else { "reject_once" };
                let option = permission["params"]["options"]
                    .as_array()
                    .and_then(|options| options.iter().find(|o| o["kind"] == wanted))
                    .and_then(|o| o["optionId"].as_str())
                    .map(str::to_owned);
                decisions.push(json!({
                    "agent": metadata.agent_id,
                    "title": permission["params"]["toolCall"]["title"],
                    "source": permission["params"]["_meta"]["acpdSource"],
                    "decision": wanted, "answered": option.is_some()}));
                if let Some(option) = option {
                    let _ = session
                        .connection
                        .permission(
                            permission["id"].clone(),
                            json!({"outcome":"selected","optionId":option}),
                        )
                        .await;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    decisions
}

#[tokio::test]
#[ignore = "uses a real Goose and real model requests; see the file comment"]
async fn goose_phase_0() {
    assert!(
        std::env::var_os("OPENAI_API_KEY").is_some(),
        "set OPENAI_API_KEY in the environment for the provider (not in a file)"
    );
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("sub")).unwrap();
    let state = tempfile::tempdir().unwrap();
    let config = Config {
        workspace_roots: vec![workspace.path().into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 240,
            max_sessions: 4,
            ..Default::default()
        },
        orchestrator: OrchestratorConfig {
            enabled: true,
            orchestrate: vec!["goose".into()],
            delegable: vec!["worker".into()],
            default_deadline_seconds: 300,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry =
        Registry::parse(&serde_json::to_string(&vec![agent("goose"), agent("worker")]).unwrap())
            .unwrap();
    let manager = Arc::new(SessionManager::new(config, registry).unwrap());
    let orchestrator = Orchestrator::new(Arc::downgrade(&manager));
    // In a test the running executable is the test binary; Goose must launch the real acpd.
    orchestrator.set_delegate_executable(env!("CARGO_BIN_EXE_acpd").into());
    manager.attach_orchestrator(orchestrator.clone());
    let _endpoint = control::start(orchestrator, state.path()).unwrap();

    let started = Instant::now();
    let root = manager
        .create("goose", workspace.path())
        .await
        .expect("Goose starts and accepts session/new with the delegation MCP server");
    let metadata = root.metadata();
    let mut report = json!({
        "goose_version": std::process::Command::new("goose").arg("--version").output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default(),
        "startup_seconds": started.elapsed().as_secs_f32(),
        "session_status": metadata.status,
        "initialize": {
            "protocolVersion": metadata.initialization["protocolVersion"],
            "agentInfo": metadata.initialization["agentInfo"],
            "agentCapabilities": metadata.initialization["agentCapabilities"],
            "authMethods": metadata.initialization["authMethods"],
        },
        "session_new_meta_keys": metadata.setup.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()),
        "session_modes_offered": metadata.setup["modes"]["availableModes"],
        "own_tools": metadata.own_tools,
    });

    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let phone = tokio::spawn(phone(manager.clone(), done.clone()));

    // 1. Can Goose find and use the delegation tools from a plain-language prompt?
    let prompt = "You have an MCP server named acpd-orchestrator. Call its agents_list tool. \
        Then call its task_delegate tool with agent \"worker\", workspace \"sub\", \
        wait_seconds 120 and prompt \"Reply with exactly the word PONG and nothing else.\" \
        Finally tell me, in one sentence, the state and result that task_delegate returned.";
    let begun = Instant::now();
    let outcome = tokio::time::timeout(Duration::from_secs(300), root.prompt(prompt)).await;
    let delegation = match &outcome {
        Ok(Ok(value)) => json!({"stopReason": value["stopReason"]}),
        Ok(Err(error)) => json!({"error": error.to_string().chars().take(300).collect::<String>()}),
        Err(_) => json!({"error": "timed out after 300 s"}),
    };
    let mut children = vec![];
    for metadata in manager.list().await {
        if metadata.agent_id == "worker"
            && let Ok(child) = manager.get(metadata.id).await
        {
            children.push(digest(&child));
        }
    }
    report["delegation_prompt"] = json!({
        "outcome": delegation,
        "seconds": begun.elapsed().as_secs_f32(),
        "parent": digest(&root),
        "children": children,
    });

    // 2. Does session/cancel end a Goose turn?
    if matches!(outcome, Ok(Ok(_))) {
        let busy = Arc::clone(&root);
        let long = tokio::spawn(async move {
            busy.prompt("Write a detailed 600-word essay about the history of lighthouses.")
                .await
        });
        let waited = Instant::now();
        while !root.connection.is_busy() && waited.elapsed() < Duration::from_secs(10) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
        let cancelled_at = Instant::now();
        root.cancel().await.unwrap();
        let ended = tokio::time::timeout(Duration::from_secs(30), long).await;
        report["cancel"] = match ended {
            Ok(Ok(Ok(value))) => json!({"stopReason": value["stopReason"],
                "seconds_to_stop": cancelled_at.elapsed().as_secs_f32()}),
            Ok(Ok(Err(error))) => {
                json!({"error": error.to_string().chars().take(200).collect::<String>()})
            }
            _ => json!({"error": "did not end within 30 s of session/cancel"}),
        };
    }

    done.store(true, std::sync::atomic::Ordering::Relaxed);
    report["phone_decisions"] = json!(phone.await.unwrap());
    manager.shutdown().await;
    println!(
        "PHASE0-REPORT {}",
        serde_json::to_string_pretty(&report).unwrap()
    );
}
