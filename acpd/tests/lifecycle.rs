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
#[cfg(target_os = "linux")]
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
