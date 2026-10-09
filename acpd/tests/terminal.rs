use acpd::terminal::TerminalService;
use acportal_protocol::acp;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use tokio::time::timeout;

fn request(workspace: &Path, args: Vec<String>, limit: u64) -> Value {
    json!({"sessionId":"session","command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":args,"cwd":workspace,"outputByteLimit":limit})
}
#[cfg(windows)]
#[tokio::test]
async fn terminal_kill_release_and_service_drop_stop_descendants() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let workspace = tempfile::tempdir().unwrap();
    for reason in ["kill", "release", "drop"] {
        let mut service = TerminalService::new("session", workspace.path(), 1024).unwrap();
        let prepared = service
            .prepare(request(workspace.path(), vec!["--descendant".into()], 1024))
            .unwrap();
        let id = service.create_approved(prepared).unwrap()["terminalId"]
            .as_str()
            .unwrap()
            .to_owned();
        let pid = timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(pid) = service.output("session", &id).unwrap()["output"]
                    .as_str()
                    .unwrap()
                    .trim()
                    .parse::<u32>()
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!raw.is_null());
        let descendant = unsafe { OwnedHandle::from_raw_handle(raw) };
        assert_eq!(
            unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) },
            WAIT_TIMEOUT
        );
        match reason {
            "kill" => {
                service.kill("session", &id).await.unwrap();
            }
            "release" => {
                service.release("session", &id).await.unwrap();
            }
            "drop" => drop(service),
            _ => unreachable!(),
        }
        timeout(Duration::from_secs(5), async {
            while unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("terminal descendants must stop");
    }
}
#[cfg(unix)]
#[tokio::test]
async fn unix_terminal_kill_release_and_service_drop_stop_descendants() {
    let workspace = tempfile::tempdir().unwrap();
    let own_group = unsafe { libc::getpgid(0) };
    for reason in ["kill", "release", "drop"] {
        let mut service = TerminalService::new("session", workspace.path(), 1024).unwrap();
        let prepared = service
            .prepare(request(workspace.path(), vec!["--descendant".into()], 1024))
            .unwrap();
        let id = service.create_approved(prepared).unwrap()["terminalId"]
            .as_str()
            .unwrap()
            .to_owned();
        let pid = timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(pid) = service.output("session", &id).unwrap()["output"]
                    .as_str()
                    .unwrap()
                    .trim()
                    .parse::<u32>()
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let group = running_process_group(pid).expect("terminal descendant must run");
        assert_ne!(
            group, own_group,
            "terminal tree must not share the host group"
        );
        match reason {
            "kill" => {
                service.kill("session", &id).await.unwrap();
            }
            "release" => {
                service.release("session", &id).await.unwrap();
            }
            "drop" => drop(service),
            _ => unreachable!(),
        }
        timeout(Duration::from_secs(5), async {
            while running_process_group(pid).is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("terminal descendant must stop after {reason}"));
    }
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
async fn finished(service: &TerminalService, id: &str) -> Value {
    timeout(
        Duration::from_secs(5),
        TerminalService::wait(service.exit_receiver("session", id).unwrap()),
    )
    .await
    .unwrap()
    .unwrap()
}
#[tokio::test]
async fn terminal_retains_bounded_utf8_and_reports_exit_before_release() {
    let workspace = tempfile::tempdir().unwrap();
    let mut service = TerminalService::new("session", workspace.path(), 1024).unwrap();
    let text = "🙂".repeat(3000) + "done\n";
    let prepared = service
        .prepare(request(
            workspace.path(),
            vec![
                "--terminal-fixture".into(),
                "--fixture-text".into(),
                text,
                "--fixture-exit-code".into(),
                "7".into(),
            ],
            37,
        ))
        .unwrap();
    let created = service.create_approved(prepared).unwrap();
    let _: acp::CreateTerminalResponse = serde_json::from_value(created.clone()).unwrap();
    let id = created["terminalId"].as_str().unwrap();
    let status = finished(&service, id).await;
    let _: acp::WaitForTerminalExitResponse = serde_json::from_value(status.clone()).unwrap();
    assert_eq!(status["exitCode"], 7);
    let output = service.output("session", id).unwrap();
    let _: acp::TerminalOutputResponse = serde_json::from_value(output.clone()).unwrap();
    let text = output["output"].as_str().unwrap();
    assert!(text.len() <= 37 && text.ends_with("done\n"));
    assert!(!text.contains('�'));
    assert_eq!(output["truncated"], true);
    assert_eq!(output["exitStatus"], status);
    assert!(service.output("other-session", id).is_err());
    service.release("session", id).await.unwrap();
    assert!(service.output("session", id).is_err());
}
#[tokio::test]
async fn literal_arguments_and_both_output_streams_survive_with_zero_limit_supported() {
    let workspace = tempfile::tempdir().unwrap();
    let mut service = TerminalService::new("session", workspace.path(), 1024).unwrap();
    for limit in [1024, 0] {
        let literal = "literal $(echo forbidden); & text";
        let prepared = service
            .prepare(request(
                workspace.path(),
                vec![
                    "--terminal-fixture".into(),
                    "--fixture-text".into(),
                    literal.into(),
                    "--fixture-stderr".into(),
                    "stderr message".into(),
                ],
                limit,
            ))
            .unwrap();
        let created = service.create_approved(prepared).unwrap();
        let id = created["terminalId"].as_str().unwrap();
        assert_eq!(finished(&service, id).await["exitCode"], 0);
        let output = service.output("session", id).unwrap();
        if limit == 0 {
            assert_eq!(output["output"], "");
            assert_eq!(output["truncated"], true);
        } else {
            let text = output["output"].as_str().unwrap();
            assert!(text.contains(literal) && text.contains("stderr message"));
            assert_eq!(output["truncated"], false);
        }
        service.release("session", id).await.unwrap();
    }
    assert_eq!(std::fs::read_dir(workspace.path()).unwrap().count(), 0);
}
#[tokio::test]
async fn scoped_handles_capacity_kill_and_shutdown_prevent_unbounded_processes() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut service = TerminalService::new("session", workspace.path(), 1024).unwrap();
    let mut params = request(
        workspace.path(),
        vec!["--terminal-fixture".into(), "--fixture-stall".into()],
        100,
    );
    let mut invalid = params.clone();
    invalid["sessionId"] = json!("other");
    assert!(service.prepare(invalid).is_err());
    let mut invalid = params.clone();
    invalid["cwd"] = json!(outside.path());
    assert!(service.prepare(invalid).is_err());
    let mut invalid = params.clone();
    invalid["env"] = json!([{"name":"A","value":"x"},{"name":"A","value":"y"}]);
    assert!(service.prepare(invalid).is_err());
    let prepared = service.prepare(params.clone()).unwrap();
    let mut other = TerminalService::new("other", workspace.path(), 1024).unwrap();
    assert!(other.create_approved(prepared).is_err());
    let mut ids = vec![];
    for _ in 0..8 {
        ids.push(
            service
                .create_approved(service.prepare(params.clone()).unwrap())
                .unwrap()["terminalId"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    assert!(
        service
            .create_approved(service.prepare(params.clone()).unwrap())
            .is_err()
    );
    let killed = service.kill("session", &ids[0]).await.unwrap();
    let _: acp::KillTerminalResponse = serde_json::from_value(killed).unwrap();
    assert!(
        service
            .output("session", &ids[0])
            .unwrap()
            .get("exitStatus")
            .is_some()
    );
    let released = service.release("session", &ids[0]).await.unwrap();
    let _: acp::ReleaseTerminalResponse = serde_json::from_value(released).unwrap();
    assert!(service.exit_receiver("session", &ids[0]).is_err());
    params["args"] = json!(["--terminal-fixture"]);
    let new_id = service
        .create_approved(service.prepare(params).unwrap())
        .unwrap()["terminalId"]
        .as_str()
        .unwrap()
        .to_string();
    let receiver = service.exit_receiver("session", &ids[1]).unwrap();
    service.shutdown().await;
    TerminalService::wait(receiver).await.unwrap();
    assert!(service.output("session", &new_id).is_err());
}

fn shell_request(workspace: &Path, line: &str) -> Value {
    json!({"sessionId":"session","command":line,"args":[],"cwd":workspace,"outputByteLimit":4096})
}

/// Runs a real shell line through the fixed host shell (`/bin/sh -c` or
/// `cmd.exe /D /S /C`) and checks the output, the file it wrote and the exit code.
#[tokio::test]
async fn shell_lines_run_exactly_through_the_fixed_host_shell() {
    let workspace = tempfile::tempdir().unwrap();
    let mut service = TerminalService::new("session", workspace.path(), 4096).unwrap();
    let (line, expected, failing) = if cfg!(windows) {
        (
            r#"echo "q  x"&& echo done> marker.txt&& type marker.txt"#,
            "\"q  x\"\ndone\n",
            "echo partial&& exit /b 3",
        )
    } else {
        (
            r#"printf '%s|' "q  x" && echo done > marker.txt && cat marker.txt"#,
            "q  x|done\n",
            "echo partial; exit 3",
        )
    };
    let prepared = service
        .prepare(shell_request(workspace.path(), line))
        .unwrap();
    assert!(prepared.is_shell());
    let operation = prepared.operation();
    assert_eq!(operation["shellLine"], line);
    assert_eq!(operation["shell"], acpd::terminal::SHELL_LABEL);
    let id = service.create_approved(prepared).unwrap()["terminalId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(finished(&service, &id).await["exitCode"], 0);
    let output = service.output("session", &id).unwrap()["output"]
        .as_str()
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(output, expected);
    assert!(workspace.path().join("marker.txt").exists());
    service.release("session", &id).await.unwrap();
    let prepared = service
        .prepare(shell_request(workspace.path(), failing))
        .unwrap();
    let id = service.create_approved(prepared).unwrap()["terminalId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(finished(&service, &id).await["exitCode"], 3);
    assert!(
        service.output("session", &id).unwrap()["output"]
            .as_str()
            .unwrap()
            .starts_with("partial")
    );
    service.release("session", &id).await.unwrap();
    // cmd.exe stops at a line break, so multi-line lines are refused on Windows only.
    assert_eq!(
        service
            .prepare(shell_request(workspace.path(), "echo a\necho b"))
            .is_err(),
        cfg!(windows)
    );
}

/// A program started by an approved Windows shell line, and its own child, stop on kill.
#[cfg(windows)]
#[tokio::test]
async fn windows_shell_line_descendants_stop_on_kill() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let workspace = tempfile::tempdir().unwrap();
    let mut service = TerminalService::new("session", workspace.path(), 1024).unwrap();
    let line = format!("\"{}\" --descendant", env!("CARGO_BIN_EXE_mock-acp-agent"));
    let prepared = service
        .prepare(shell_request(workspace.path(), &line))
        .unwrap();
    assert!(prepared.is_shell());
    let id = service.create_approved(prepared).unwrap()["terminalId"]
        .as_str()
        .unwrap()
        .to_owned();
    let pid = timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(pid) = service.output("session", &id).unwrap()["output"]
                .as_str()
                .unwrap()
                .trim()
                .parse::<u32>()
            {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(!raw.is_null());
    let descendant = unsafe { OwnedHandle::from_raw_handle(raw) };
    assert_eq!(
        unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) },
        WAIT_TIMEOUT
    );
    service.kill("session", &id).await.unwrap();
    timeout(Duration::from_secs(5), async {
        while unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("descendant of the shell line must stop");
}
