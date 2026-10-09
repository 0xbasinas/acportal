use acpd::{
    config::{Config, RuntimeConfig},
    connection::{AcpConnection, AgentRpcError},
    registry::{AgentDefinition, Registry},
    session::SessionManager,
};
use acportal_protocol::{acp, initialize_params};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::time::timeout;

#[tokio::test]
async fn prompt_media_and_embedded_resources_reach_the_agent_without_transformation() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &["--prompt-media"], 1);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    assert_eq!(
        session.metadata().initialization["agentCapabilities"]["promptCapabilities"]["image"],
        true
    );
    let params = json!({"sessionId":session.metadata().acp_session_id,"prompt":[
        {"type":"text","text":"Review these files"},
        {"type":"resource","resource":{"uri":"acportal-attachment://fixture/source.kt","mimeType":"text/plain","text":"val greeting = \"世界\""},"_meta":{"acportalAttachmentName":"source.kt"}},
        {"type":"resource","resource":{"uri":"acportal-attachment://fixture/binary","mimeType":"application/octet-stream","blob":"AP8BAg=="}},
        {"type":"image","mimeType":"image/png","data":"aW1hZ2U=","_meta":{"acportalAttachmentName":"image.png"}},
        {"type":"audio","mimeType":"audio/wav","data":"YXVkaW8="}
    ]});
    let _: acp::PromptRequest = serde_json::from_value(params.clone()).unwrap();
    let connection = session.connection.clone();
    let expected = params.clone();
    let prompt = tokio::spawn(async move { connection.request("session/prompt", params).await });
    next_permission(&session.connection).await;
    let received = session
        .connection
        .request("_mock/last_prompt", json!({}))
        .await
        .unwrap();
    assert_eq!(received, expected);
    session.cancel().await.unwrap();
    assert_eq!(prompt.await.unwrap().unwrap()["stopReason"], "cancelled");
    manager.shutdown().await;
}

#[tokio::test]
async fn workspace_access_is_negotiated_and_enforced_even_when_agent_ignores_it() {
    use acpd::connection::{AgentRpcError, WorkspaceAccess};
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("policy.txt");
    std::fs::write(&path, "unchanged").unwrap();
    for mask in 0..8 {
        let access = WorkspaceAccess {
            read_files: mask & 1 != 0,
            write_files: mask & 2 != 0,
            terminal: mask & 4 != 0,
            form_elicitation: false,
        };
        let manager = manager(
            workspace.path(),
            if mask & 1 != 0 {
                &["--require-auth"]
            } else {
                &[]
            },
            1,
        );
        let session = manager
            .open_configured(
                "test-agent",
                workspace.path(),
                if mask & 2 != 0 {
                    Some("saved-policy")
                } else {
                    None
                },
                vec![],
                access,
            )
            .await
            .unwrap();
        if mask & 1 != 0 {
            session
                .authenticate(json!({"methodId":"mock-login"}))
                .await
                .unwrap();
        }
        assert_eq!(session.metadata().workspace_access, access);
        assert_eq!(
            session
                .connection
                .request("_mock/client_capabilities", json!({}))
                .await
                .unwrap(),
            access.capabilities()
        );
        let session_id = session.metadata().acp_session_id;
        let read = session
            .connection
            .request(
                "_mock/read_file",
                json!({"sessionId":session_id,"path":path}),
            )
            .await;
        if access.read_files {
            assert_eq!(read.unwrap()["content"], "unchanged");
        } else {
            assert_eq!(
                read.unwrap_err().downcast_ref::<AgentRpcError>().unwrap().0,
                -32601
            );
        }
        if !access.write_files {
            let write = session
                .connection
                .request(
                    "_mock/write_file",
                    json!({"sessionId":session_id,"path":path,"content":"must not write"}),
                )
                .await;
            assert_eq!(
                write
                    .unwrap_err()
                    .downcast_ref::<AgentRpcError>()
                    .unwrap()
                    .0,
                -32601
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
        }
        if !access.terminal {
            for method in [
                "terminal/create",
                "terminal/output",
                "terminal/wait_for_exit",
                "terminal/kill",
                "terminal/release",
            ] {
                let reply=session.connection.request("_mock/terminal",json!({"method":method,"request":{"sessionId":session_id,"command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--terminal-fixture"],"terminalId":"not-created"}})).await;
                assert_eq!(
                    reply
                        .unwrap_err()
                        .downcast_ref::<AgentRpcError>()
                        .unwrap()
                        .0,
                    -32601
                );
            }
        }
        assert!(session.replay(0).pending_permissions.is_empty());
        manager.shutdown().await;
    }
}

fn definition(args: &[&str]) -> AgentDefinition {
    serde_json::from_value(json!({"id":"test-agent","name":"Test agent",
        "command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args}))
    .unwrap()
}

#[tokio::test]
async fn mcp_definitions_reach_new_load_and_post_authentication_setup() {
    let workspace = tempfile::tempdir().unwrap();
    let stdio = json!({"name":"tools","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["literal argument with spaces"],"env":[{"name":"TEST_KEY","value":"synthetic-test-value"}]});
    let http = json!({"type":"http","name":"docs","url":"https://example.com/mcp","headers":[{"name":"Authorization","value":"synthetic-test-header"}]});
    let sse = json!({"type":"sse","name":"events","url":"https://example.com/events","headers":[]});
    let definitions = vec![stdio, http, sse];
    let manager = manager(
        workspace.path(),
        &["--mcp-http", "--mcp-sse", "--require-auth"],
        4,
    );
    for load in [None, Some("saved-session")] {
        let session = manager
            .open_with_mcp("test-agent", workspace.path(), load, definitions.clone())
            .await
            .unwrap();
        assert_eq!(session.metadata().status, "authentication_required");
        session
            .authenticate(json!({"methodId":"mock-login"}))
            .await
            .unwrap();
        assert_eq!(
            session
                .connection
                .request("_mock/mcp_servers", json!({}))
                .await
                .unwrap(),
            json!(definitions)
        );
        let metadata = serde_json::to_string(&session.metadata()).unwrap();
        assert!(
            !metadata.contains("synthetic-test"),
            "secrets must not enter the restart catalog"
        );
        manager.remove(session.metadata().id).await.unwrap();
    }
    manager.shutdown().await;
}

/// The registry's `ownTools` flag reaches session metadata (and stays off by default), so
/// the phone can warn inside the conversation.
#[tokio::test]
async fn registry_own_tools_flag_reaches_session_metadata() {
    let workspace = tempfile::tempdir().unwrap();
    for flagged in [false, true] {
        let mut agent = serde_json::to_value(definition(&[])).unwrap();
        if flagged {
            agent["ownTools"] = json!(true);
        }
        let config = Config {
            workspace_roots: vec![workspace.path().into()],
            ..Default::default()
        };
        let registry = Registry::parse(&json!([agent]).to_string()).unwrap();
        let manager = SessionManager::new(config, registry).unwrap();
        let session = manager
            .open_configured(
                "test-agent",
                workspace.path(),
                None,
                Vec::new(),
                acpd::connection::WorkspaceAccess::default(),
            )
            .await
            .unwrap();
        let metadata = serde_json::to_value(session.metadata()).unwrap();
        assert_eq!(metadata["ownTools"], flagged, "{metadata}");
        manager.shutdown().await;
    }
}

#[tokio::test]
async fn unsupported_mcp_transport_leaves_no_live_session() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 2);
    let definition =
        json!({"type":"http","name":"docs","url":"https://example.com/mcp","headers":[]});
    assert!(
        manager
            .open_with_mcp("test-agent", workspace.path(), None, vec![definition])
            .await
            .is_err()
    );
    assert_eq!(manager.active_count().await, 0);
    manager.shutdown().await;
}
fn manager(workspace: &Path, args: &[&str], max_sessions: usize) -> SessionManager {
    let config = Config {
        workspace_roots: vec![workspace.into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 2,
            max_sessions,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry =
        Registry::parse(&serde_json::to_string(&vec![definition(args)]).unwrap()).unwrap();
    SessionManager::new(config, registry).unwrap()
}
async fn next_permission(connection: &AcpConnection) -> Value {
    timeout(Duration::from_secs(5), async {
        let mut events = connection.subscribe();
        loop {
            let replay = connection.replay(0);
            if let Some(permission) = replay.pending_permissions.first() {
                return permission.clone();
            }
            events.recv().await.unwrap();
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
#[cfg(windows)]
async fn descendants_stop_on_session_removal_agent_crash_and_request_timeout() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    for reason in ["remove", "crash", "timeout"] {
        let session = manager
            .create("test-agent", workspace.path())
            .await
            .unwrap();
        let descendants = session
            .connection
            .request("_mock/spawn_descendant", json!({}))
            .await
            .unwrap();
        let handles: Vec<OwnedHandle> = ["childPid", "grandchildPid"]
            .into_iter()
            .map(|key| {
                let raw = unsafe {
                    OpenProcess(
                        PROCESS_SYNCHRONIZE,
                        0,
                        descendants[key].as_u64().unwrap() as u32,
                    )
                };
                assert!(
                    !raw.is_null(),
                    "descendant must exist before stopping {reason}"
                );
                let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
                assert_eq!(
                    unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) },
                    WAIT_TIMEOUT
                );
                handle
            })
            .collect();
        match reason {
            "remove" => manager.remove(session.metadata().id).await.unwrap(),
            "crash" => {
                assert!(
                    session
                        .connection
                        .request("_mock/crash", json!({}))
                        .await
                        .is_err()
                );
            }
            "timeout" => {
                assert!(
                    session
                        .connection
                        .request("_mock/stall", json!({}))
                        .await
                        .is_err()
                );
            }
            _ => unreachable!(),
        }
        timeout(Duration::from_secs(5), session.connection.wait_closed())
            .await
            .unwrap();
        timeout(Duration::from_secs(5), async {
            loop {
                if handles.iter().all(|handle| unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } == WAIT_OBJECT_0) {break;}
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("both descendant generations must stop");
    }
    manager.shutdown().await;
}

#[tokio::test]
#[cfg(unix)]
async fn unix_descendants_stop_on_session_removal_agent_crash_and_request_timeout() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let own_group = unsafe { libc::getpgid(0) };
    for reason in ["remove", "crash", "timeout"] {
        let session = manager
            .create("test-agent", workspace.path())
            .await
            .unwrap();
        let descendants = session
            .connection
            .request("_mock/spawn_descendant", json!({}))
            .await
            .unwrap();
        let pids: Vec<u32> = ["childPid", "grandchildPid"]
            .into_iter()
            .map(|key| descendants[key].as_u64().unwrap() as u32)
            .collect();
        for pid in &pids {
            let group = running_process_group(*pid)
                .unwrap_or_else(|| panic!("descendant must run before stopping {reason}"));
            assert_ne!(group, own_group, "agent tree must not share the host group");
        }
        match reason {
            "remove" => manager.remove(session.metadata().id).await.unwrap(),
            "crash" => {
                assert!(
                    session
                        .connection
                        .request("_mock/crash", json!({}))
                        .await
                        .is_err()
                );
            }
            "timeout" => {
                assert!(
                    session
                        .connection
                        .request("_mock/stall", json!({}))
                        .await
                        .is_err()
                );
            }
            _ => unreachable!(),
        }
        timeout(Duration::from_secs(5), session.connection.wait_closed())
            .await
            .unwrap();
        timeout(Duration::from_secs(5), async {
            while pids.iter().any(|pid| running_process_group(*pid).is_some()) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("both descendant generations must stop after {reason}"));
    }
    manager.shutdown().await;
}

/// Other Unix systems (macOS): the same view through `ps`, which reports a zombie as `Z`.
#[cfg(all(unix, not(target_os = "linux")))]
fn running_process_group(pid: u32) -> Option<i32> {
    let output = std::process::Command::new("ps")
        .args(["-o", "stat=", "-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.split_whitespace();
    let state = fields.next()?;
    if state.starts_with('Z') {
        return None;
    }
    fields.next()?.parse().ok()
}

/// Linux view of a process: `None` once it has exited (gone or zombie), otherwise its
/// process group. A zombie has stopped running and only awaits reaping by its new parent.
#[cfg(target_os = "linux")]
fn running_process_group(pid: u32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the parenthesised command name: state, ppid, pgrp, ...
    let mut fields = stat[stat.rfind(')')? + 1..].split_whitespace();
    let state = fields.next()?;
    if state == "Z" || state == "X" {
        return None;
    }
    fields.nth(1)?.parse().ok()
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn shell_lines_require_their_own_approval_and_run_exactly_as_shown() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    let marker = workspace.path().join("marker.txt");
    let line = r#"printf '%s|' "a  b" $((1+2)) && echo done > marker.txt; cat marker.txt"#;
    let create = |command: &str, args: Vec<&str>| {
        let worker = session.clone();
        let request = json!({"sessionId":session_id,"command":command,"args":args,"cwd":workspace.path(),"env":[{"name":"FIXTURE_SECRET","value":"never-shown"}],"outputByteLimit":4096});
        tokio::spawn(async move {
            worker
                .connection
                .request(
                    "_mock/terminal",
                    json!({"method":"terminal/create","request":request}),
                )
                .await
        })
    };
    // Denial: the consent shows the exact line, cwd and env names, and nothing runs.
    let denied = create(line, vec![]);
    let consent = next_permission(&session.connection).await;
    let _: acp::RequestPermissionRequest =
        serde_json::from_value(consent["params"].clone()).unwrap();
    assert_eq!(
        consent["params"]["_meta"]["acpdSource"],
        "host-shell-command"
    );
    let shown = &consent["params"]["toolCall"]["rawInput"];
    assert_eq!(shown["shellLine"], line);
    assert_eq!(shown["shell"], "/bin/sh -c");
    assert_eq!(shown["environmentNames"], json!(["FIXTURE_SECRET"]));
    assert!(shown.get("command").is_none());
    assert!(!consent.to_string().contains("never-shown"));
    let options: Vec<&str> = consent["params"]["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|option| option["optionId"].as_str().unwrap())
        .collect();
    assert_eq!(options, ["acpd-shell-deny", "acpd-shell-allow"]);
    // A plain-command approval id cannot approve a shell line.
    assert!(
        session
            .connection
            .permission(
                consent["id"].clone(),
                json!({"outcome":"selected","optionId":"acpd-command-allow"})
            )
            .await
            .is_err()
    );
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-deny"}),
        )
        .await
        .unwrap();
    assert!(denied.await.unwrap().is_err());
    assert!(!marker.exists());
    // Cancellation while the approval is pending: nothing runs.
    let cancelled = create(line, vec![]);
    next_permission(&session.connection).await;
    session
        .connection
        .notify("session/cancel", json!({"sessionId":session_id}))
        .await
        .unwrap();
    assert!(cancelled.await.unwrap().is_err());
    assert!(!marker.exists());
    // Approval runs exactly the shown line through /bin/sh -c.
    let approved = create(line, vec![]);
    let consent = next_permission(&session.connection).await;
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-allow"}),
        )
        .await
        .unwrap();
    let created = approved.await.unwrap().unwrap();
    let scope = json!({"sessionId":session_id,"terminalId":created["terminalId"]});
    let exit = session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/wait_for_exit","request":scope}),
        )
        .await
        .unwrap();
    assert_eq!(exit["exitCode"], 0);
    let output = session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/output","request":scope}),
        )
        .await
        .unwrap();
    assert_eq!(output["output"], "a  b|3|done\n");
    assert!(marker.exists());
    session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/release","request":scope}),
        )
        .await
        .unwrap();
    // A plain program whose arguments contain shell syntax keeps the literal path.
    let plain = create(
        env!("CARGO_BIN_EXE_mock-acp-agent"),
        vec!["--terminal-fixture", "--fixture-text", "a && b; $(x)"],
    );
    let consent = next_permission(&session.connection).await;
    assert_eq!(consent["params"]["_meta"]["acpdSource"], "host-terminal");
    assert_eq!(
        consent["params"]["toolCall"]["rawInput"]["args"][2],
        "a && b; $(x)"
    );
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-command-allow"}),
        )
        .await
        .unwrap();
    let created = plain.await.unwrap().unwrap();
    let scope = json!({"sessionId":session_id,"terminalId":created["terminalId"]});
    session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/wait_for_exit","request":scope}),
        )
        .await
        .unwrap();
    let output = session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/output","request":scope}),
        )
        .await
        .unwrap();
    assert_eq!(output["output"], "a && b; $(x)");
    // A shell line's child and grandchild both stop when the terminal is released.
    let tree = create("echo $$; sleep 30 & echo $!; wait", vec![]);
    let consent = next_permission(&session.connection).await;
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-allow"}),
        )
        .await
        .unwrap();
    let created = tree.await.unwrap().unwrap();
    let scope = json!({"sessionId":session_id,"terminalId":created["terminalId"]});
    let pids: Vec<u32> = timeout(Duration::from_secs(5), async {
        loop {
            let output = session
                .connection
                .request(
                    "_mock/terminal",
                    json!({"method":"terminal/output","request":scope}),
                )
                .await
                .unwrap();
            let pids: Vec<u32> = output["output"]
                .as_str()
                .unwrap()
                .lines()
                .filter_map(|line| line.trim().parse().ok())
                .collect();
            if pids.len() == 2 {
                break pids;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let own_group = unsafe { libc::getpgid(0) };
    for pid in &pids {
        let group = running_process_group(*pid).expect("shell tree must be running");
        assert_ne!(group, own_group);
    }
    session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/release","request":scope}),
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        while pids.iter().any(|pid| running_process_group(*pid).is_some()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("shell and its background child must stop on release");
    manager.shutdown().await;
}

#[cfg(target_os = "linux")]
fn manager_with_review(
    workspace: &Path,
    args: &[&str],
    review: acpd::shell_review::ShellReviewConfig,
) -> SessionManager {
    let mut shell_review = std::collections::BTreeMap::new();
    shell_review.insert("test-agent".into(), review);
    let config = Config {
        workspace_roots: vec![workspace.into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 5,
            max_sessions: 4,
            ..Default::default()
        },
        shell_review,
        ..Default::default()
    };
    let registry =
        Registry::parse(&serde_json::to_string(&vec![definition(args)]).unwrap()).unwrap();
    SessionManager::new(config, registry).unwrap()
}

#[cfg(target_os = "linux")]
async fn create_shell(
    session: &Arc<acpd::session::AcpSession>,
    session_id: &str,
    line: &str,
) -> tokio::task::JoinHandle<anyhow::Result<Value>> {
    let worker = session.clone();
    let request = json!({"sessionId":session_id,"command":line,"args":[],"cwd":session.metadata().workspace,"outputByteLimit":4096});
    tokio::spawn(async move {
        worker
            .connection
            .request(
                "_mock/terminal",
                json!({"method":"terminal/create","request":request}),
            )
            .await
    })
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn shell_review_rules_auto_allow_deny_and_ask() {
    use acpd::shell_review::ShellReviewConfig;
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("a.txt"), "x").unwrap();
    let reviewed = manager_with_review(
        workspace.path(),
        &[],
        ShellReviewConfig {
            rules: true,
            model: None,
        },
    );
    let session = reviewed
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    // Allow: no phone consent; the timeline shows the auto-decision.
    let allowed = create_shell(&session, &session_id, "ls a.txt").await;
    let created = timeout(Duration::from_secs(3), allowed)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let _: acp::CreateTerminalResponse = serde_json::from_value(created.clone()).unwrap();
    let auto = session
        .connection
        .replay(0)
        .events
        .iter()
        .find_map(|event| {
            let update = &event.message["params"]["update"];
            (update["sessionUpdate"] == "tool_call")
                .then(|| update["_meta"]["acpdAutoReview"].clone())
                .filter(|value| !value.is_null())
        })
        .expect("auto-allow must be visible in the session");
    assert_eq!(auto["decision"], "allow");
    assert_eq!(auto["layer"], "rules");
    let scope = json!({"sessionId":session_id,"terminalId":created["terminalId"]});
    session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/wait_for_exit","request":scope}),
        )
        .await
        .unwrap();
    session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/release","request":scope}),
        )
        .await
        .unwrap();
    // Deny: hard-deny list refuses without a phone prompt.
    let denied = create_shell(&session, &session_id, "rm -rf /").await;
    let error = timeout(Duration::from_secs(3), denied)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    let code = error.downcast_ref::<AgentRpcError>().unwrap().0;
    assert_eq!(code, acpd::connection::CALLBACK_DENIED);
    assert!(session.connection.replay(0).pending_permissions.is_empty());
    // Ask: redirects still need explicit phone approval.
    let asked = create_shell(&session, &session_id, "echo hi > out.txt").await;
    let consent = next_permission(&session.connection).await;
    assert_eq!(
        consent["params"]["_meta"]["acpdSource"],
        "host-shell-command"
    );
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["decision"],
        "ask"
    );
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["layer"],
        "rules"
    );
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-deny"}),
        )
        .await
        .unwrap();
    assert!(asked.await.unwrap().is_err());
    // Off by default: a manager without shell_review still prompts.
    let plain_manager = manager(workspace.path(), &[], 4);
    let session = plain_manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    let pending = create_shell(&session, &session_id, "ls -la").await;
    let consent = next_permission(&session.connection).await;
    assert!(consent["params"]["_meta"].get("acpdAutoReview").is_none());
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-deny"}),
        )
        .await
        .unwrap();
    assert!(pending.await.unwrap().is_err());
    reviewed.shutdown().await;
    plain_manager.shutdown().await;
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn shell_review_model_layer_allow_deny_malformed_and_timeout() {
    use acpd::shell_review::{ModelReviewConfig, ShellReviewConfig};
    use axum::{Router, body::Bytes, http::StatusCode, routing::post};
    use std::sync::Arc as StdArc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let hits = StdArc::new(AtomicUsize::new(0));
    let state = hits.clone();
    let app = Router::new().route(
        "/chat/completions",
        post(move |body: Bytes| {
            let state = state.clone();
            async move {
                state.fetch_add(1, Ordering::SeqCst);
                let request: Value = serde_json::from_slice(&body).unwrap();
                let user: Value =
                    serde_json::from_str(request["messages"][1]["content"].as_str().unwrap())
                        .unwrap();
                assert_eq!(user["agent"], "test-agent");
                assert_eq!(user["shell"], "/bin/sh -c");
                assert!(user.get("env").is_none());
                let command = user["command"].as_str().unwrap().to_owned();
                let content = if command.contains("wc") || command.contains("allowme") {
                    r#"{"decision":"allow","reason":"counts lines"}"#
                } else if command.contains("make") {
                    r#"{"decision":"deny","reason":"looks destructive"}"#
                } else if command.contains("grep") {
                    "not-json"
                } else {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    r#"{"decision":"allow","reason":"late"}"#
                };
                (
                    StatusCode::OK,
                    axum::Json(json!({"choices":[{"message":{"content":content}}]})),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let key = "ACPD_SHELL_REVIEW_TEST_KEY";
    // SAFETY: test-only env, single-threaded per process for this name.
    unsafe { std::env::set_var(key, "synthetic-test-key") };
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("a.txt"), "x").unwrap();
    let review = ShellReviewConfig {
        rules: true,
        model: Some(ModelReviewConfig {
            base_url: format!("http://{address}"),
            model: "test-model".into(),
            api_key_env: key.into(),
            timeout_seconds: 1,
            max_requests_per_minute: 30,
            max_command_bytes: 2048,
        }),
    };
    let manager = manager_with_review(workspace.path(), &[], review);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    // Environment overrides require the phone, even for an otherwise allowlisted line.
    for name in ["PATH", "LD_PRELOAD"] {
        let worker = session.clone();
        let request = json!({"sessionId":session_id,"command":"ls -la",
            "env":[{"name":name,"value":"private-value"}]});
        let pending = tokio::spawn(async move {
            worker
                .connection
                .request(
                    "_mock/terminal",
                    json!({"method":"terminal/create","request":request}),
                )
                .await
        });
        let consent = next_permission(&session.connection).await;
        assert_eq!(
            consent["params"]["_meta"]["acpdAutoReview"]["layer"],
            "rules"
        );
        assert_eq!(
            consent["params"]["_meta"]["acpdAutoReview"]["decision"],
            "ask"
        );
        assert!(!consent.to_string().contains("private-value"));
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "environment is never sent to the model"
        );
        session
            .connection
            .permission(
                consent["id"].clone(),
                json!({"outcome":"selected","optionId":"acpd-shell-deny"}),
            )
            .await
            .unwrap();
        assert!(pending.await.unwrap().is_err());
    }
    // A positive model response cannot approve an outside-workspace glob.
    let pending = create_shell(&session, &session_id, "cat /etc/allowme*").await;
    let consent = next_permission(&session.connection).await;
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["decision"],
        "ask"
    );
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["layer"],
        "model"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-deny"}),
        )
        .await
        .unwrap();
    assert!(pending.await.unwrap().is_err());

    // Model allow of an undecided low-risk line.
    let allowed = create_shell(&session, &session_id, "wc -l a.txt").await;
    let created = timeout(Duration::from_secs(3), allowed)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let auto = session
        .connection
        .replay(0)
        .events
        .iter()
        .rev()
        .find_map(|event| {
            let update = &event.message["params"]["update"];
            update
                .get("_meta")
                .and_then(|meta| meta.get("acpdAutoReview"))
                .cloned()
        })
        .unwrap();
    assert_eq!(auto["decision"], "allow");
    assert_eq!(auto["layer"], "model");
    let scope = json!({"sessionId":session_id,"terminalId":created["terminalId"]});
    session
        .connection
        .request(
            "_mock/terminal",
            json!({"method":"terminal/release","request":scope}),
        )
        .await
        .unwrap();
    // Model deny.
    let denied = create_shell(&session, &session_id, "make test").await;
    let error = timeout(Duration::from_secs(3), denied)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<AgentRpcError>().unwrap().0,
        acpd::connection::CALLBACK_DENIED
    );
    // Malformed reply → ask the phone.
    let asked = create_shell(&session, &session_id, "grep -n add a.txt").await;
    let consent = next_permission(&session.connection).await;
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["decision"],
        "ask"
    );
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["layer"],
        "model"
    );
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-deny"}),
        )
        .await
        .unwrap();
    assert!(asked.await.unwrap().is_err());
    // Timeout → ask the phone.
    let timed = create_shell(&session, &session_id, "python3 calc.py").await;
    let consent = next_permission(&session.connection).await;
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["layer"],
        "model"
    );
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-deny"}),
        )
        .await
        .unwrap();
    assert!(timed.await.unwrap().is_err());
    // A model "allow" cannot auto-run a line the rules mark sensitive.
    let sensitive = create_shell(&session, &session_id, "echo allowme > out.txt").await;
    let consent = next_permission(&session.connection).await;
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["decision"],
        "ask"
    );
    assert_eq!(
        consent["params"]["_meta"]["acpdAutoReview"]["layer"],
        "model"
    );
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-shell-deny"}),
        )
        .await
        .unwrap();
    assert!(sensitive.await.unwrap().is_err());
    assert!(!workspace.path().join("out.txt").exists());
    // Cancel while the model is still reviewing: nothing runs and no consent appears.
    let reviewing = create_shell(&session, &session_id, "python3 slow.py").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    session
        .connection
        .notify("session/cancel", json!({"sessionId":session_id}))
        .await
        .unwrap();
    let error = timeout(Duration::from_secs(3), reviewing)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.downcast_ref::<AgentRpcError>().unwrap().0, -32800);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(session.connection.replay(0).pending_permissions.is_empty());
    // Rules already deny, so the model is never called for that line.
    let before = hits.load(Ordering::SeqCst);
    let hard = create_shell(&session, &session_id, "sudo ls").await;
    assert!(
        timeout(Duration::from_secs(3), hard)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(hits.load(Ordering::SeqCst), before);
    manager.shutdown().await;
}

#[tokio::test]
async fn a_quiet_terminal_wait_pauses_prompt_inactivity_until_command_exit() {
    let workspace = tempfile::tempdir().unwrap();
    let config = Config {
        workspace_roots: vec![workspace.path().into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = Registry::parse(
        &serde_json::to_string(&vec![definition(&[
            "--terminal-during-prompt",
            "--terminal-exit-delay-ms",
            "1600",
        ])])
        .unwrap(),
    )
    .unwrap();
    let manager = SessionManager::new(config, registry).unwrap();
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let worker = session.clone();
    let prompt = tokio::spawn(async move { worker.prompt("Run the quiet fixture").await });
    let consent = next_permission(&session.connection).await;
    session
        .connection
        .permission(
            consent["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-command-allow"}),
        )
        .await
        .unwrap();
    let agent_permission = next_permission(&session.connection).await;
    assert!(session.connection.closed_reason().is_none());
    session
        .connection
        .permission(
            agent_permission["id"].clone(),
            json!({"outcome":"selected","optionId":"deny"}),
        )
        .await
        .unwrap();
    assert_eq!(prompt.await.unwrap().unwrap()["stopReason"], "end_turn");
    manager.shutdown().await;
}

#[tokio::test]
async fn terminal_wait_does_not_block_actor_and_kill_or_cancel_stops_the_command() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    for cancel in [false, true] {
        let worker = session.clone();
        let session_id = session_id.clone();
        let create = tokio::spawn(async move {
            worker.connection.request("_mock/terminal",json!({"method":"terminal/create","request":{"sessionId":session_id,"command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--terminal-fixture","--fixture-stall"]}})).await
        });
        let permission = next_permission(&session.connection).await;
        session
            .connection
            .permission(
                permission["id"].clone(),
                json!({"outcome":"selected","optionId":"acpd-command-allow"}),
            )
            .await
            .unwrap();
        let created = create.await.unwrap().unwrap();
        let scope = json!({"sessionId":session.metadata().acp_session_id,"terminalId":created["terminalId"]});
        let worker = session.clone();
        let waiting_scope = scope.clone();
        let wait = tokio::spawn(async move {
            worker
                .connection
                .request(
                    "_mock/terminal",
                    json!({"method":"terminal/wait_for_exit","request":waiting_scope}),
                )
                .await
        });
        assert_eq!(
            session
                .connection
                .request("_mock/echo", json!({"responsive":true}))
                .await
                .unwrap()["responsive"],
            true
        );
        if cancel {
            session.cancel().await.unwrap();
        } else {
            session
                .connection
                .request(
                    "_mock/terminal",
                    json!({"method":"terminal/kill","request":scope}),
                )
                .await
                .unwrap();
        }
        let exit = timeout(Duration::from_secs(5), wait)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let _: acp::WaitForTerminalExitResponse = serde_json::from_value(exit).unwrap();
        session
            .connection
            .request(
                "_mock/terminal",
                json!({"method":"terminal/release","request":scope}),
            )
            .await
            .unwrap();
    }
    manager.shutdown().await;
}

#[tokio::test]
async fn acp_terminal_callbacks_require_consent_and_retain_final_output_after_release() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    let params = json!({"sessionId":session_id,"command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--terminal-fixture","--fixture-text","actual ACP terminal output\n"],"outputByteLimit":1024});
    for allow in [false, true] {
        let worker = session.clone();
        let params = params.clone();
        let create = tokio::spawn(async move {
            worker
                .connection
                .request(
                    "_mock/terminal",
                    json!({"method":"terminal/create","request":params}),
                )
                .await
        });
        let consent = next_permission(&session.connection).await;
        let _: acp::RequestPermissionRequest =
            serde_json::from_value(consent["params"].clone()).unwrap();
        assert_eq!(consent["params"]["_meta"]["acpdSource"], "host-terminal");
        let id = consent["id"].clone();
        let outcome = json!({"outcome":"selected","optionId":if allow {"acpd-command-allow"}else{"acpd-command-deny"}});
        session
            .connection
            .permission(id.clone(), outcome.clone())
            .await
            .unwrap();
        assert!(session.connection.permission(id, outcome).await.is_err());
        let result = create.await.unwrap();
        if !allow {
            assert!(result.is_err());
            continue;
        }
        let created = result.unwrap();
        let _: acp::CreateTerminalResponse = serde_json::from_value(created.clone()).unwrap();
        let terminal_id = created["terminalId"].clone();
        let scope = json!({"sessionId":session_id,"terminalId":terminal_id});
        let exit = session
            .connection
            .request(
                "_mock/terminal",
                json!({"method":"terminal/wait_for_exit","request":scope}),
            )
            .await
            .unwrap();
        assert_eq!(exit["exitCode"], 0);
        let output = session
            .connection
            .request(
                "_mock/terminal",
                json!({"method":"terminal/output","request":scope}),
            )
            .await
            .unwrap();
        assert_eq!(output["output"], "actual ACP terminal output\n");
        let mut other = scope.clone();
        other["sessionId"] = json!("other-session");
        assert!(
            session
                .connection
                .request(
                    "_mock/terminal",
                    json!({"method":"terminal/output","request":other})
                )
                .await
                .is_err()
        );
        session
            .connection
            .request(
                "_mock/terminal",
                json!({"method":"terminal/release","request":scope}),
            )
            .await
            .unwrap();
        assert!(
            session
                .connection
                .request(
                    "_mock/terminal",
                    json!({"method":"terminal/output","request":scope})
                )
                .await
                .is_err()
        );
        let replay = session.replay(0);
        assert!(replay.events.iter().any(|event| event.direction == "host"
            && event.message["params"]["update"]["_meta"]["acpdTerminal"]["terminalId"]
                == terminal_id
            && event.message["params"]["update"]["_meta"]["acpdTerminal"]["output"]
                == "actual ACP terminal output\n"));
    }
    let worker = session.clone();
    let create = tokio::spawn(async move {
        worker
            .connection
            .request(
                "_mock/terminal",
                json!({"method":"terminal/create","request":params}),
            )
            .await
    });
    next_permission(&session.connection).await;
    session.cancel().await.unwrap();
    assert!(create.await.unwrap().is_err());
    assert!(session.replay(0).pending_permissions.is_empty());
    manager.shutdown().await;
}

#[tokio::test]
async fn authentication_retains_initialized_process_and_finishes_new_or_loaded_session() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &["--require-auth"], 8);
    for load in [None, Some("previous-session")] {
        let session = match load {
            Some(id) => manager
                .load("test-agent", workspace.path(), id)
                .await
                .unwrap(),
            None => manager
                .create("test-agent", workspace.path())
                .await
                .unwrap(),
        };
        let waiting = session.metadata();
        assert_eq!(waiting.status, "authentication_required");
        assert!(waiting.acp_session_id.is_empty());
        assert!(session.connection.closed_reason().is_none());
        assert!(
            session
                .prompt("must not send before authentication")
                .await
                .is_err()
        );
        assert!(
            session
                .authenticate(json!({"methodId":"invented"}))
                .await
                .is_err()
        );
        assert_eq!(
            session
                .authenticate(json!({"methodId":"mock-login"}))
                .await
                .unwrap(),
            json!({})
        );
        let ready = session.metadata();
        assert_eq!(ready.id, waiting.id);
        assert_eq!(ready.status, "ready");
        assert_eq!(ready.acp_session_id, load.unwrap_or("mock-1"));
        session
            .authenticate(json!({"methodId":"mock-login"}))
            .await
            .unwrap();
        assert_eq!(session.metadata().acp_session_id, ready.acp_session_id);
    }
    manager.shutdown().await;
}

#[tokio::test]
async fn negotiated_filesystem_reads_are_scoped_and_errors_do_not_break_the_actor() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let path = workspace.path().join("sample.txt");
    std::fs::write(&path, "first\nsecond\nlast").unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    let capabilities = session
        .connection
        .request("_mock/client_capabilities", json!({}))
        .await
        .unwrap();
    assert_eq!(capabilities["fs"]["readTextFile"], true);
    assert_eq!(capabilities["fs"]["writeTextFile"], true);
    assert_eq!(capabilities["terminal"], true);
    let result = session
        .connection
        .request(
            "_mock/read_file",
            json!({"sessionId":session_id,"path":path,"line":2,"limit":1}),
        )
        .await
        .unwrap();
    let _: acp::ReadTextFileResponse = serde_json::from_value(result.clone()).unwrap();
    assert_eq!(result["content"], "second\n");
    assert!(
        session
            .connection
            .request(
                "_mock/read_file",
                json!({"sessionId":"other-session","path":path})
            )
            .await
            .is_err()
    );
    let secret = outside.path().join("secret.txt");
    std::fs::write(&secret, "outside secret").unwrap();
    let refused = session
        .connection
        .request(
            "_mock/read_file",
            json!({"sessionId":session_id,"path":secret}),
        )
        .await
        .unwrap_err();
    // ACP reserves -32000 for auth_required; a containment refusal must not claim it.
    assert_eq!(
        refused.downcast_ref::<AgentRpcError>().map(|error| error.0),
        Some(acpd::connection::CALLBACK_DENIED)
    );
    assert_eq!(
        session
            .connection
            .request("_mock/echo", json!({"still":"connected"}))
            .await
            .unwrap()["still"],
        "connected"
    );
    manager.shutdown().await;
    let loading_manager = self::manager(workspace.path(), &["--read-during-load"], 8);
    let loaded = loading_manager
        .load("test-agent", workspace.path(), "loaded-session")
        .await
        .unwrap();
    assert_eq!(loaded.metadata().acp_session_id, "loaded-session");
    loading_manager.shutdown().await;
}

#[tokio::test]
async fn host_file_writes_require_fresh_permission_and_preserve_concurrent_edits() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    let path = workspace.path().join("approved.txt");
    let start_write = |content: &str| {
        let connection = session.connection.clone();
        let params = json!({"sessionId":session_id,"path":path,"content":content});
        tokio::spawn(async move { connection.request("_mock/write_file", params).await })
    };
    let denied = start_write("denied");
    let permission = next_permission(&session.connection).await;
    let _: acp::RequestPermissionRequest =
        serde_json::from_value(permission["params"].clone()).unwrap();
    assert_eq!(
        permission["params"]["_meta"]["acpdSource"],
        "host-filesystem"
    );
    assert!(!path.exists());
    session
        .connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-write-deny"}),
        )
        .await
        .unwrap();
    let refused = denied.await.unwrap().unwrap_err();
    assert_eq!(
        refused.downcast_ref::<AgentRpcError>().map(|error| error.0),
        Some(acpd::connection::CALLBACK_DENIED)
    );
    assert!(!path.exists());
    assert!(
        session
            .connection
            .permission(
                permission["id"].clone(),
                json!({"outcome":"selected","optionId":"acpd-write-allow"})
            )
            .await
            .is_err()
    );
    let allowed = start_write("approved");
    let permission = next_permission(&session.connection).await;
    assert!(
        session
            .connection
            .permission(
                permission["id"].clone(),
                json!({"outcome":"selected","optionId":"invented"})
            )
            .await
            .is_err()
    );
    assert!(!path.exists());
    session
        .connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-write-allow"}),
        )
        .await
        .unwrap();
    let result = allowed.await.unwrap().unwrap();
    let _: acp::WriteTextFileResponse = serde_json::from_value(result).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "approved");
    // Identical content: nothing would change, so no consent; the phone sees why.
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let same = start_write("approved");
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), same)
        .await
        .expect("an unchanged write needs no consent")
        .unwrap()
        .unwrap();
    let _: acp::WriteTextFileResponse = serde_json::from_value(result).unwrap();
    assert!(session.replay(0).pending_permissions.is_empty());
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    let notice = session
        .replay(0)
        .events
        .iter()
        .rev()
        .find(|event| event.message["params"]["update"]["_meta"]["acpdWrite"] == "unchanged")
        .map(|event| event.message["params"]["update"].clone())
        .expect("unchanged write is shown");
    assert_eq!(notice["status"], "completed");
    assert_eq!(notice["_meta"]["acpdSource"], "host-filesystem");
    assert_eq!(
        notice["content"][0]["content"]["text"],
        acpd::connection::UNCHANGED_WRITE_NOTICE
    );
    let changed = start_write("proposed replacement");
    let permission = next_permission(&session.connection).await;
    std::fs::write(&path, "concurrent edit").unwrap();
    session
        .connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-write-allow"}),
        )
        .await
        .unwrap();
    assert!(changed.await.unwrap().is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "concurrent edit");
    let cancelled = start_write("cancelled replacement");
    let _permission = next_permission(&session.connection).await;
    session.cancel().await.unwrap();
    assert!(cancelled.await.unwrap().is_err());
    assert!(session.replay(0).pending_permissions.is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "concurrent edit");
    manager.shutdown().await;
}

/// Agents that edit files with their own tools can change a file while the host consent for the
/// same write is pending. A refused or cancelled consent cannot undo that, so the phone is told.
#[tokio::test]
async fn refused_host_write_already_applied_by_the_agent_is_flagged() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let session_id = session.metadata().acp_session_id;
    let flagged = |session: &acpd::session::AcpSession| {
        session
            .replay(0)
            .events
            .iter()
            .filter(|event| {
                event.message["params"]["update"]["_meta"]["acpdWrite"] == "changed-without-consent"
            })
            .map(|event| event.message["params"]["update"].clone())
            .collect::<Vec<_>>()
    };
    for (name, cancel) in [("denied.txt", false), ("cancelled.txt", true)] {
        let path = workspace.path().join(name);
        std::fs::write(&path, "original\n").unwrap();
        let connection = session.connection.clone();
        let params = json!({"sessionId":session_id,"path":path,"content":"agent text\n"});
        let request =
            tokio::spawn(async move { connection.request("_mock/write_file", params).await });
        let permission = next_permission(&session.connection).await;
        // The agent's own tool writes the same text while the consent is pending.
        std::fs::write(&path, "agent text\n").unwrap();
        if cancel {
            session.cancel().await.unwrap();
        } else {
            session
                .connection
                .permission(
                    permission["id"].clone(),
                    json!({"outcome":"selected","optionId":"acpd-write-deny"}),
                )
                .await
                .unwrap();
        }
        assert!(request.await.unwrap().is_err());
        let updates = flagged(&session);
        let update = updates.last().expect("refused write found applied");
        assert_eq!(
            update["toolCallId"],
            permission["params"]["toolCall"]["toolCallId"]
        );
        assert_eq!(update["status"], "failed");
        assert_eq!(update["content"][0]["oldText"], "original\n");
        assert_eq!(
            update["content"][1]["content"]["text"],
            acpd::connection::OUTSIDE_WRITE_NOTICE
        );
        assert_eq!(updates.len(), if cancel { 2 } else { 1 });
    }
    // An ordinary refusal of a write the agent did not apply is not flagged.
    let path = workspace.path().join("untouched.txt");
    std::fs::write(&path, "original\n").unwrap();
    let connection = session.connection.clone();
    let params = json!({"sessionId":session_id,"path":path,"content":"agent text\n"});
    let request = tokio::spawn(async move { connection.request("_mock/write_file", params).await });
    let permission = next_permission(&session.connection).await;
    session
        .connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":"acpd-write-deny"}),
        )
        .await
        .unwrap();
    assert!(request.await.unwrap().is_err());
    assert_eq!(flagged(&session).len(), 2);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
    manager.shutdown().await;
}

#[tokio::test]
async fn complete_prompt_stream_permissions_tools_diff_and_cleanup() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let prompt_session = session.clone();
    let prompt = tokio::spawn(async move { prompt_session.prompt("inspect a test project").await });
    let permission = next_permission(&session.connection).await;
    assert!(
        session
            .connection
            .permission(
                permission["id"].clone(),
                json!({"outcome":"selected","optionId":"invented"})
            )
            .await
            .is_err()
    );
    assert_eq!(session.replay(0).pending_permissions.len(), 1);
    session
        .connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":"allow"}),
        )
        .await
        .unwrap();
    assert!(
        session
            .connection
            .permission(
                permission["id"].clone(),
                json!({"outcome":"selected","optionId":"allow"})
            )
            .await
            .is_err()
    );
    let result = timeout(Duration::from_secs(5), prompt)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["stopReason"], "end_turn");
    let replay = session.replay(0);
    assert!(replay.pending_permissions.is_empty());
    assert!(
        replay
            .events
            .iter()
            .any(|event| event.message["params"]["update"]["kind"] == "execute")
    );
    assert!(
        replay
            .events
            .iter()
            .any(|event| event.message["params"]["update"]["content"][0]["type"] == "diff")
    );
    // The mock emits wire shapes validated against upstream stable v1 types.
    for event in &replay.events {
        if event.message["method"] == "session/update" {
            let _: acp::SessionNotification =
                serde_json::from_value(event.message["params"].clone()).unwrap();
        } else {
            let _: acp::RequestPermissionRequest =
                serde_json::from_value(event.message["params"].clone()).unwrap();
        }
    }
    assert!(!workspace.path().join("example.rs").exists());
    assert_eq!(manager.list().await[0].status, "ready");
    manager.remove(session.metadata().id).await.unwrap();
    assert!(manager.list().await.is_empty());
    assert!(session.connection.closed_reason().is_some());
}

#[tokio::test]
async fn subscriber_disconnect_does_not_kill_session_and_permission_is_recoverable() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let disconnected_client = session.connection.subscribe();
    drop(disconnected_client);
    let prompt_session = session.clone();
    let prompt = tokio::spawn(async move { prompt_session.prompt("offline turn").await });
    let permission = next_permission(&session.connection).await;
    let reattached = manager.get(session.metadata().id).await.unwrap();
    assert!(Arc::ptr_eq(&session, &reattached));
    assert_eq!(reattached.metadata().status, "running");
    reattached
        .connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":"deny"}),
        )
        .await
        .unwrap();
    assert_eq!(prompt.await.unwrap().unwrap()["stopReason"], "end_turn");
    assert!(reattached.connection.closed_reason().is_none());
    assert!(
        reattached
            .replay(0)
            .events
            .iter()
            .all(|event| event.message["params"]["update"]["content"][0]["type"] != "diff")
    );
    manager.shutdown().await;
}

#[tokio::test]
async fn cancellation_resolves_permissions_and_prompt() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 8);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let prompt_session = session.clone();
    let prompt = tokio::spawn(async move { prompt_session.prompt("cancel me").await });
    next_permission(&session.connection).await;
    session.cancel().await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), prompt)
            .await
            .unwrap()
            .unwrap()
            .unwrap()["stopReason"],
        "cancelled"
    );
    assert!(session.replay(0).pending_permissions.is_empty());
    assert_eq!(session.metadata().status, "ready");
    manager.shutdown().await;
}

#[tokio::test]
async fn multiple_sessions_and_capacity_enforcement() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 2);
    let (a, b) = tokio::join!(
        manager.create("test-agent", workspace.path()),
        manager.create("test-agent", workspace.path())
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.metadata().id, b.metadata().id);
    assert!(
        manager
            .create("test-agent", workspace.path())
            .await
            .is_err()
    );
    manager.remove(a.metadata().id).await.unwrap();
    assert!(manager.create("test-agent", workspace.path()).await.is_ok());
    manager.shutdown().await;
    assert!(b.connection.closed_reason().is_some());
}

#[tokio::test]
async fn version_mismatch_and_unsupported_resume_fail_closed() {
    let workspace = tempfile::tempdir().unwrap();
    let manager_v2 = manager(workspace.path(), &["--fault", "v2"], 2);
    assert!(
        manager_v2
            .create("test-agent", workspace.path())
            .await
            .unwrap_err_string()
            .contains("protocolVersion")
    );
    assert!(manager_v2.list().await.is_empty());
    let no_resume = manager(workspace.path(), &["--no-resume"], 2);
    assert!(
        no_resume
            .load("test-agent", workspace.path(), "existing")
            .await
            .unwrap_err_string()
            .contains("loadSession")
    );
    assert!(no_resume.list().await.is_empty());
    let supports_resume = manager(workspace.path(), &[], 2);
    let session = supports_resume
        .load("test-agent", workspace.path(), "existing")
        .await
        .unwrap();
    assert_eq!(session.metadata().acp_session_id, "existing");
    supports_resume.shutdown().await;
}
// Avoid requiring Debug on live process/session handles when checking errors.
trait ErrorString<T> {
    fn unwrap_err_string(self) -> String;
}
impl<T> ErrorString<T> for anyhow::Result<T> {
    fn unwrap_err_string(self) -> String {
        match self {
            Ok(_) => panic!("expected error"),
            Err(error) => error.to_string(),
        }
    }
}

#[tokio::test]
async fn crash_malformed_oversized_and_timeout_release_pending_requests() {
    let workspace = tempfile::tempdir().unwrap();
    for fault in ["exit", "malformed", "oversized", "stall"] {
        let def = definition(&["--fault", fault]);
        let limits = RuntimeConfig {
            request_timeout_seconds: 1,
            ..Default::default()
        };
        let connection =
            AcpConnection::spawn(&def, Path::new(&def.command), workspace.path(), limits).unwrap();
        let result = timeout(
            Duration::from_secs(5),
            connection.request("initialize", initialize_params()),
        )
        .await
        .unwrap();
        assert!(result.is_err(), "fault {fault}");
        timeout(Duration::from_secs(5), connection.wait_closed())
            .await
            .unwrap();
        assert!(connection.closed_reason().is_some());
    }
}

#[tokio::test]
async fn request_ids_route_concurrently_and_extensions_remain_lossless() {
    let workspace = tempfile::tempdir().unwrap();
    let def = definition(&[]);
    let connection = AcpConnection::spawn(
        &def,
        Path::new(&def.command),
        workspace.path(),
        RuntimeConfig::default(),
    )
    .unwrap();
    let a = json!({"_future":{"text":"literal $(touch x)\nline two"}});
    let b = json!({"another":[1,2,3]});
    let (a_result, b_result) = tokio::join!(
        connection.request("_mock/echo", a.clone()),
        connection.request("_mock/echo", b.clone())
    );
    assert_eq!(a_result.unwrap(), a);
    assert_eq!(b_result.unwrap(), b);
    connection.shutdown().await;
}

#[tokio::test]
async fn duplicate_prompt_and_oversized_input_do_not_corrupt_session() {
    let workspace = tempfile::tempdir().unwrap();
    let manager = manager(workspace.path(), &[], 2);
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    assert!(session.prompt(&"x".repeat(2 * 1024 * 1024)).await.is_err());
    assert!(
        session
            .connection
            .notify(
                "_mock/oversized",
                json!({"text":"x".repeat(2 * 1024 * 1024)})
            )
            .await
            .is_err()
    );
    assert!(
        session
            .connection
            .permission(json!(1), json!({"text":"x".repeat(2 * 1024 * 1024)}))
            .await
            .is_err()
    );
    assert!(session.connection.closed_reason().is_none());
    let prompt_session = session.clone();
    let prompt = tokio::spawn(async move { prompt_session.prompt("first").await });
    next_permission(&session.connection).await;
    assert!(session.prompt("second").await.is_err());
    session.cancel().await.unwrap();
    prompt.await.unwrap().unwrap();
    manager.shutdown().await;
}

#[tokio::test]
async fn waiting_for_an_offline_user_does_not_trigger_agent_timeout() {
    let workspace = tempfile::tempdir().unwrap();
    let def = definition(&[]);
    let config = Config {
        workspace_roots: vec![workspace.path().into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = Registry::parse(&serde_json::to_string(&vec![def]).unwrap()).unwrap();
    let manager = SessionManager::new(config, registry).unwrap();
    let session = manager
        .create("test-agent", workspace.path())
        .await
        .unwrap();
    let worker = session.clone();
    let prompt = tokio::spawn(async move { worker.prompt("wait for user").await });
    let permission = next_permission(&session.connection).await;
    tokio::time::sleep(Duration::from_millis(1300)).await;
    assert!(session.connection.closed_reason().is_none());
    assert_eq!(session.replay(0).pending_permissions.len(), 1);
    session
        .connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":"deny"}),
        )
        .await
        .unwrap();
    assert_eq!(prompt.await.unwrap().unwrap()["stopReason"], "end_turn");
    let worker = session.clone();
    let prompt = tokio::spawn(async move { worker.prompt("wait and cancel").await });
    next_permission(&session.connection).await;
    tokio::time::sleep(Duration::from_millis(1300)).await;
    session.cancel().await.unwrap();
    assert_eq!(prompt.await.unwrap().unwrap()["stopReason"], "cancelled");
    manager.shutdown().await;
}
