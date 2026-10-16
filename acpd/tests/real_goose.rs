//! Phase 0 measurement: does a real Goose accept the delegation MCP server and use it?
//!
//! Opt-in and costs real model requests, so it is `#[ignore]`d. Run it with the provider key in
//! the environment (never in a file) and read the printed report:
//!
//!   OPENAI_API_KEY=... cargo test -p acpd --test real_goose -- --ignored --nocapture
//!
//! `ACPD_GOOSE_BIN` names the goose executable (default `goose` on PATH). `goose_worktree_trial`
//! also needs `ACPD_GOOSE_HOME`, a scratch folder Goose uses as HOME and XDG folders.
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
    let command = std::env::var("ACPD_GOOSE_BIN").unwrap_or_else(|_| "goose".into());
    json!({"id":id,"name":format!("Goose ({id})"),"command":command,"args":["acp"],
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

/// Phone for the supervisor trial. The root Goose's own delegation-tool permissions are allowed.
/// A request from the worker is left alone so the supervisor gets its chance; it is recorded in
/// full, and only if it is still unanswered `after` later (the supervisor window has closed and it
/// has escalated) does the phone allow it once. The workspace is a temporary directory.
async fn escalating_phone(
    manager: Arc<SessionManager>,
    done: Arc<std::sync::atomic::AtomicBool>,
    after: Duration,
) -> Vec<Value> {
    let mut first_seen: BTreeMap<(uuid::Uuid, String), Instant> = BTreeMap::new();
    let mut answered = HashSet::new();
    let mut log: Vec<Value> = vec![];
    while !done.load(std::sync::atomic::Ordering::Relaxed) {
        for metadata in manager.list().await {
            let Ok(session) = manager.get(metadata.id).await else {
                continue;
            };
            for permission in session.replay(0).pending_permissions {
                let key = (metadata.id, permission["id"].to_string());
                let seen = *first_seen.entry(key.clone()).or_insert_with(|| {
                    log.push(json!({
                        "event": "request_seen", "agent": metadata.agent_id,
                        "toolCall": permission["params"]["toolCall"],
                        "meta": permission["params"]["_meta"],
                        "options": permission["params"]["options"],
                    }));
                    Instant::now()
                });
                if answered.contains(&key) {
                    continue;
                }
                let root = metadata.agent_id == "goose";
                if !root && seen.elapsed() < after {
                    continue;
                }
                let wanted = if root && !names_a_delegation_tool(&permission) {
                    "reject_once"
                } else {
                    "allow_once"
                };
                let option = permission["params"]["options"]
                    .as_array()
                    .and_then(|options| options.iter().find(|o| o["kind"] == wanted))
                    .and_then(|o| o["optionId"].as_str())
                    .map(str::to_owned);
                if let Some(option) = option {
                    answered.insert(key);
                    log.push(json!({
                        "event": "phone_answered", "agent": metadata.agent_id, "decision": wanted,
                        "title": permission["params"]["toolCall"]["title"],
                        "seconds_after_first_seen": seen.elapsed().as_secs_f32(),
                    }));
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
    log
}

/// A delegated child gets read-only rights unless the delegation asks for more. Without write
/// rights Goose writes with its own tool (a request the host rates High); with them the host
/// offers `fs/write_text_file`, and a write becomes a host consent the supervisor may decide.
#[tokio::test]
#[ignore = "uses a real Goose and real model requests; see the file comment"]
async fn goose_supervisor_trial() {
    supervisor_trial("", "TRIAL-REPORT", false).await;
}

#[tokio::test]
#[ignore = "uses a real Goose and real model requests; see the file comment"]
async fn goose_supervisor_trial_with_write_rights() {
    supervisor_trial(
        " and rights {\"read\": true, \"write\": true}",
        "TRIAL-REPORT-WRITE",
        false,
    )
    .await;
}

/// The same task with the host-side auto-approve on for the worker (a git repository, so the
/// worker runs in a task worktree, which is what the default policy requires): nobody should be
/// asked, the supervisor and the phone alike.
#[tokio::test]
#[ignore = "uses a real Goose and real model requests; see the file comment"]
async fn goose_auto_approve_trial() {
    supervisor_trial(
        " and rights {\"read\": true, \"write\": true}",
        "TRIAL-REPORT-AUTO",
        true,
    )
    .await;
}

async fn supervisor_trial(rights_clause: &str, label: &str, auto: bool) {
    assert!(
        std::env::var_os("OPENAI_API_KEY").is_some(),
        "set OPENAI_API_KEY in the environment for the provider (not in a file)"
    );
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("sub")).unwrap();
    let state = tempfile::tempdir().unwrap();
    if auto {
        // A repository with a commit, so a writer's task gets its own worktree.
        std::fs::write(workspace.path().join("sub").join("README.md"), "project\n").unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "core.autocrlf", "false"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "first",
            ],
        ] {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(workspace.path())
                .args(&args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }
    }
    let mut config = Config {
        workspace_roots: vec![workspace.path().into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 300,
            max_sessions: 4,
            ..Default::default()
        },
        orchestrator: OrchestratorConfig {
            enabled: true,
            orchestrate: vec!["goose".into()],
            delegable: vec!["worker".into()],
            default_deadline_seconds: 300,
            idle_timeout_seconds: 240,
            supervisor_timeout_seconds: 20,
            ..Default::default()
        },
        ..Default::default()
    };
    if auto {
        config.state_directory = state.path().to_path_buf();
        config.auto_approve.insert(
            "worker".into(),
            acpd::auto_approve::AutoApproveConfig {
                max_risk: acpd::auto_approve::AutoRisk::Medium,
                own_edit_shape: true,
                ..Default::default()
            },
        );
    }
    let registry =
        Registry::parse(&serde_json::to_string(&vec![agent("goose"), agent("worker")]).unwrap())
            .unwrap();
    let manager = Arc::new(SessionManager::new(config, registry).unwrap());
    let orchestrator = Orchestrator::new(Arc::downgrade(&manager));
    orchestrator.set_delegate_executable(env!("CARGO_BIN_EXE_acpd").into());
    manager.attach_orchestrator(orchestrator.clone());
    let _endpoint = control::start(orchestrator.clone(), state.path()).unwrap();

    let root = manager.create("goose", workspace.path()).await.unwrap();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let phone = tokio::spawn(escalating_phone(
        manager.clone(),
        done.clone(),
        Duration::from_secs(30),
    ));
    // What the host shows as waiting for a decision, and when it changes (offsets from the start).
    let started = Instant::now();
    let watched = {
        let (orchestrator, done, root) = (orchestrator.clone(), done.clone(), root.clone());
        tokio::spawn(async move {
            let mut last = String::new();
            let mut changes: Vec<Value> = vec![];
            let mut seen_events = 0usize;
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                // The root's own tool calls, as they start and finish, with the same clock.
                let events = root.replay(0).events;
                for event in events.iter().skip(seen_events) {
                    let update = &event.message["params"]["update"];
                    if matches!(
                        update["sessionUpdate"].as_str(),
                        Some("tool_call" | "tool_call_update")
                    ) && update["status"].is_string()
                    {
                        changes.push(json!({
                            "at": started.elapsed().as_secs_f32(),
                            "root_tool": update["title"], "status": update["status"],
                            "input": update["rawInput"],
                            "output": update["rawOutput"].to_string().chars().take(300).collect::<String>(),
                        }));
                    }
                }
                seen_events = events.len();
                let mut now = vec![];
                for task in orchestrator.task_list() {
                    let Some(id) = task["id"].as_str().and_then(|id| id.parse().ok()) else {
                        continue;
                    };
                    if let Some(detail) = orchestrator.task_detail(id) {
                        now.push(json!({"state": detail["state"], "needsDecision": detail["needsDecision"]}));
                    }
                }
                let text = serde_json::to_string(&now).unwrap();
                if text != last {
                    changes.push(json!({"at": started.elapsed().as_secs_f32(), "tasks": now}));
                    last = text;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            changes
        })
    };

    let prompt = format!(
        "You supervise a worker agent through the MCP server acpd-orchestrator. \
        Call task_delegate with agent \"worker\", workspace \"sub\"{rights_clause}, wait_seconds 60 and prompt \
        \"Create a file named hello.txt in your current directory containing exactly the word hi, \
        using your file tools, then reply done.\" \
        Whenever a result lists needs_decision entries: if an entry is decidable, call task_decide \
        for it with decision allow_once, copying request_id and digest exactly and giving a short \
        rationale; if it is not decidable, do not decide it. Keep calling task_status with \
        wait_seconds 60 until the task is completed or failed. Finally say in two sentences what \
        happened, including who or what allowed the worker's request."
    );
    let begun = Instant::now();
    let outcome = tokio::time::timeout(Duration::from_secs(400), root.prompt(&prompt)).await;
    let outcome = match &outcome {
        Ok(Ok(value)) => json!({"stopReason": value["stopReason"]}),
        Ok(Err(error)) => json!({"error": error.to_string().chars().take(300).collect::<String>()}),
        Err(_) => json!({"error": "timed out after 400 s"}),
    };
    let mut workers = vec![];
    let mut reviews = vec![];
    for metadata in manager.list().await {
        if metadata.agent_id == "worker"
            && let Ok(child) = manager.get(metadata.id).await
        {
            workers.push(digest(&child));
            for event in child.replay(0).events {
                let text = event.message.to_string();
                if text.contains("acpdAutoReview") {
                    reviews.push(text.chars().take(500).collect::<String>());
                }
            }
        }
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let file = workspace.path().join("sub").join("hello.txt");
    // A completed task keeps its worktree (a follow-up may come), so the file is still there.
    let branch_file = auto.then(|| {
        let root = state.path().join("worktrees");
        let found = std::fs::read_dir(&root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("sub").join("hello.txt"))
            .find(|path| path.is_file());
        json!({"in_a_task_worktree": found.is_some(),
               "content": found.and_then(|path| std::fs::read_to_string(path).ok())})
    });
    let report = json!({
        "outcome": outcome,
        "seconds": begun.elapsed().as_secs_f32(),
        "file_exists": file.exists(),
        "file_content": std::fs::read_to_string(&file).ok(),
        "branch_file": branch_file,
        "parent": digest(&root),
        "workers": workers,
        "auto_review_events": reviews,
        "phone_log": phone.await.unwrap(),
        "host_view": watched.await.unwrap(),
    });
    manager.shutdown().await;
    println!("{label} {}", serde_json::to_string_pretty(&report).unwrap());
}

/// Phase 4 with a real Goose: a supervising Goose delegates two writers with worktree isolation.
/// Each writes its own file in its own worktree; the parent checkout and the other worktree must
/// not change. Goose's own config is kept away from the operator's by `ACPD_GOOSE_HOME` (a
/// scratch folder used as HOME and XDG folders), so nothing in the user's Goose setup is read
/// or written.
#[tokio::test]
#[ignore = "uses a real Goose and real model requests; see the file comment"]
async fn goose_worktree_trial() {
    assert!(
        std::env::var_os("OPENAI_API_KEY").is_some(),
        "set OPENAI_API_KEY in the environment"
    );
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("work/repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |dir: &std::path::Path, args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?} {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "base\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let repo = repo.canonicalize().unwrap();
    let state = base.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    let home = std::env::var("ACPD_GOOSE_HOME").expect("set ACPD_GOOSE_HOME to a scratch folder");
    let mut agents = vec![agent("goose"), agent("worker")];
    for entry in &mut agents {
        let env = entry["env"].as_object_mut().unwrap();
        for (name, sub) in [
            ("HOME", ""),
            ("XDG_CONFIG_HOME", "/config"),
            ("XDG_DATA_HOME", "/data"),
            ("XDG_STATE_HOME", "/state"),
            ("XDG_CACHE_HOME", "/cache"),
        ] {
            env.insert(name.into(), json!(format!("{home}{sub}")));
        }
        env.insert("GOOSE_DISABLE_KEYRING".into(), json!("1"));
    }
    let config = Config {
        workspace_roots: vec![base.path().join("work")],
        state_directory: state.clone(),
        runtime: RuntimeConfig {
            request_timeout_seconds: 300,
            max_sessions: 4,
            ..Default::default()
        },
        orchestrator: OrchestratorConfig {
            enabled: true,
            orchestrate: vec!["goose".into()],
            delegable: vec!["worker".into()],
            default_deadline_seconds: 300,
            idle_timeout_seconds: 240,
            supervisor_timeout_seconds: 20,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = Registry::parse(&serde_json::to_string(&agents).unwrap()).unwrap();
    let manager = Arc::new(SessionManager::new(config, registry).unwrap());
    let orchestrator = Orchestrator::new(Arc::downgrade(&manager));
    orchestrator.set_delegate_executable(env!("CARGO_BIN_EXE_acpd").into());
    manager.attach_orchestrator(orchestrator.clone());
    let _endpoint = control::start(orchestrator.clone(), &state).unwrap();
    let all = acpd::connection::WorkspaceAccess {
        read_files: true,
        write_files: true,
        terminal: false,
        form_elicitation: false,
    };
    let root = manager
        .open_configured("goose", &repo, None, vec![], all)
        .await
        .unwrap();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let phone = tokio::spawn(escalating_phone(
        manager.clone(),
        done.clone(),
        Duration::from_secs(2),
    ));
    let prompt = "You supervise worker agents through the MCP server acpd-orchestrator. \
        Call task_delegate twice, both with agent \"worker\", isolation \"worktree\", rights {\"read\": true, \"write\": true}, wait_seconds 0. \
        First prompt: \"Create a file named a.txt in your current directory containing exactly the word alpha, using your file tools, then reply done.\" \
        Second prompt: \"Create a file named b.txt in your current directory containing exactly the word beta, using your file tools, then reply done.\" \
        Then call task_status with wait_seconds 60 until both tasks are completed or failed (do not call task_decide). \
        Finally report each task's state and its worktree branch in one sentence each.";
    let begun = Instant::now();
    let outcome = tokio::time::timeout(Duration::from_secs(400), root.prompt(prompt)).await;
    let tasks = orchestrator.task_list();
    let mut seen = vec![];
    for task in &tasks {
        let Some(id) = task["id"].as_str().and_then(|id| id.parse().ok()) else {
            continue;
        };
        let Some(detail) = orchestrator.task_detail(id) else {
            continue;
        };
        seen.push(detail);
    }
    let worktrees: Vec<Value> = {
        let mut out = vec![];
        for metadata in manager.list().await {
            if metadata.agent_id == "worker" {
                let path = metadata.workspace.clone();
                out.push(json!({"workspace": path, "files": std::fs::read_dir(&path).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>()).unwrap_or_default(),
                    "a": std::fs::read_to_string(path.join("a.txt")).ok(), "b": std::fs::read_to_string(path.join("b.txt")).ok()}));
            }
        }
        out
    };
    let parent_status = git(&repo, &["status", "--porcelain"]);
    let parent_files: Vec<String> = std::fs::read_dir(&repo)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    // The phone merges the first completed worker (its uncommitted file is committed to its
    // branch first); the other is released when the root ends (snapshotted to its branch).
    let mut merge = json!(null);
    if let Some(first) = seen
        .iter()
        .find(|t| t["agent"] == "worker" && t["state"] == "completed")
        .and_then(|t| t["id"].as_str())
        .and_then(|id| id.parse().ok())
    {
        merge = match orchestrator.merge_task(first).await {
            Ok(merged) => merged,
            Err(refusal) => json!({"refused": refusal.code()}),
        };
    }
    let parent_after_merge = git(&repo, &["log", "--format=%h %an %s", "-3"]);
    // End the root: the remaining worker's worktree is released: its uncommitted work is
    // committed to its branch and the folder removed (`worktree_uncommitted = "commit"`).
    manager.remove(root.metadata().id).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let after = git(&repo, &["worktree", "list", "--porcelain"]);
    let branches = git(&repo, &["branch", "--list", "acpd/*"]);
    let branch_tips: Vec<Value> = branches
        .lines()
        .map(|line| line.trim_start_matches(['*', '+', ' ']).to_owned())
        .filter(|b| !b.is_empty())
        .map(|b| {
            json!({"branch": b, "tip": git(&repo, &["log", "-1", "--format=%an: %s", &b]),
                "files": git(&repo, &["show", "--name-only", "--format=", &b])})
        })
        .collect();
    let mut records = vec![];
    orchestrator.persist_now();
    for record in acpd::orchestrator::records::load(&state).unwrap_or_default() {
        records.push(
            json!({"agent": record.agent, "state": record.state, "worktree": record.worktree}),
        );
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let phone_log = phone.await.unwrap();
    let report = json!({
        "seconds": begun.elapsed().as_secs_f32(),
        "outcome": match &outcome { Ok(Ok(v)) => json!(v["stopReason"]), Ok(Err(e)) => json!(e.to_string()), Err(_) => json!("timeout") },
        "rootText": digest(&root)["text"],
        "tasks": seen, "workerFolders": worktrees,
        "parentStatus": parent_status, "parentFiles": parent_files,
        "phoneMerge": merge, "parentLogAfterMerge": parent_after_merge,
        "worktreesAfterRootEnded": after, "branchesAfter": branches, "branchTips": branch_tips,
        "parentFilesAtEnd": std::fs::read_dir(&repo).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>(),
        "records": records,
        "phoneAnswers": phone_log.iter().filter(|e| e["event"] == "phone_answered").count(),
    });
    println!(
        "TRIAL-REPORT-WORKTREE {}",
        serde_json::to_string_pretty(&report).unwrap()
    );
    manager.shutdown().await;
}
