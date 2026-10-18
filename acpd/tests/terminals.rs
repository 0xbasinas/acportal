//! Real shells on a real pseudo-terminal (Unix openpty, Windows ConPTY).
use acpd::terminals::{Event, TerminalConfig, TerminalError, Terminals};
use std::time::{Duration, Instant};
use uuid::Uuid;

fn on() -> TerminalConfig {
    TerminalConfig {
        enabled: true,
        ..Default::default()
    }
}

/// Reads output until `marker` shows up (or the time runs out) and returns everything seen.
async fn read_until(
    sub: &mut acpd::terminals::Subscription,
    seen: &mut String,
    marker: &str,
) -> bool {
    let end = Instant::now() + Duration::from_secs(20);
    for (_, data) in std::mem::take(&mut sub.replay) {
        seen.push_str(&String::from_utf8_lossy(&data));
    }
    while !seen.contains(marker) && Instant::now() < end {
        match tokio::time::timeout(Duration::from_millis(500), sub.rx.recv()).await {
            Ok(Ok(Event::Output { data, .. })) => seen.push_str(&String::from_utf8_lossy(&data)),
            Ok(Ok(Event::Exit(_))) | Ok(Err(_)) => break,
            Err(_) => {}
        }
    }
    seen.contains(marker)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_runs_a_command_resizes_and_ends_with_its_tree() {
    let terminals = Terminals::new();
    let folder = tempfile::tempdir().unwrap();
    let me = Uuid::new_v4();
    let info = terminals
        .open(&on(), folder.path(), me, 24, 80, None)
        .unwrap();
    let mut sub = terminals.subscribe(info.id, 0).unwrap();
    let mut seen = String::new();
    terminals
        .input(info.id, me, b"echo pty-works-$((1+2))\r".to_vec())
        .ok();
    // cmd.exe and PowerShell do not expand $(( )); an echo of a fixed word works everywhere.
    terminals
        .input(info.id, me, b"echo hello-pty\r".to_vec())
        .unwrap();
    assert!(
        read_until(&mut sub, &mut seen, "hello-pty").await,
        "got: {seen}"
    );
    terminals.resize(info.id, me, 40, 100).unwrap();
    assert_eq!(
        (
            terminals.info(info.id).unwrap().rows,
            terminals.info(info.id).unwrap().cols
        ),
        (40, 100)
    );
    // Someone who attaches later gets the history from the offset they ask for.
    let again = terminals.subscribe(info.id, 0).unwrap();
    assert!(
        again
            .replay
            .iter()
            .any(|(_, d)| String::from_utf8_lossy(d).contains("hello-pty"))
    );
    assert!(!again.gap);
    drop(again);
    drop(sub);
    assert!(terminals.close(info.id));
    assert_eq!(
        terminals.info(info.id).unwrap_err(),
        TerminalError::NotFound
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shell_ending_itself_is_reported() {
    let terminals = Terminals::new();
    let folder = tempfile::tempdir().unwrap();
    let me = Uuid::new_v4();
    let info = terminals
        .open(&on(), folder.path(), me, 24, 80, None)
        .unwrap();
    let mut sub = terminals.subscribe(info.id, 0).unwrap();
    terminals.input(info.id, me, b"exit 3\r".to_vec()).unwrap();
    let end = Instant::now() + Duration::from_secs(20);
    let mut code = None;
    while code.is_none() && Instant::now() < end {
        if let Ok(Ok(Event::Exit(c))) =
            tokio::time::timeout(Duration::from_millis(500), sub.rx.recv()).await
        {
            code = Some(c);
        }
    }
    assert_eq!(code, Some(3));
    assert_eq!(terminals.info(info.id).unwrap().exit_code, Some(3));
    assert_eq!(
        terminals.input(info.id, me, b"x".to_vec()),
        Err(TerminalError::Ended)
    );
    terminals.close_all();
}

#[tokio::test(flavor = "multi_thread")]
async fn refusals() {
    let terminals = Terminals::new();
    let folder = tempfile::tempdir().unwrap();
    let me = Uuid::new_v4();
    assert_eq!(
        terminals
            .open(&TerminalConfig::default(), folder.path(), me, 24, 80, None)
            .unwrap_err(),
        TerminalError::Disabled
    );
    let two = TerminalConfig {
        max_terminals: 2,
        ..on()
    };
    let a = terminals
        .open(&two, folder.path(), me, 24, 80, None)
        .unwrap();
    terminals
        .open(&two, folder.path(), me, 24, 80, None)
        .unwrap();
    assert_eq!(
        terminals
            .open(&two, folder.path(), me, 24, 80, None)
            .unwrap_err(),
        TerminalError::TooMany
    );
    // Only the device holding the keyboard types; another takes it on request.
    let other = Uuid::new_v4();
    assert_eq!(
        terminals.input(a.id, other, b"x".to_vec()),
        Err(TerminalError::NotController)
    );
    assert_eq!(
        terminals.resize(a.id, other, 10, 10),
        Err(TerminalError::NotController)
    );
    terminals.take_control(a.id, other).unwrap();
    assert!(terminals.input(a.id, other, b"x".to_vec()).is_ok());
    assert_eq!(
        terminals.input(a.id, me, b"x".to_vec()),
        Err(TerminalError::NotController)
    );
    assert_eq!(
        terminals.input(a.id, other, vec![0; 70_000]),
        Err(TerminalError::TooLarge)
    );
    assert_eq!(
        terminals.info(Uuid::new_v4()).unwrap_err(),
        TerminalError::NotFound
    );
    terminals.close_all();
    assert!(terminals.list().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unattended_terminal_is_closed_when_idle() {
    let terminals = Terminals::new();
    let folder = tempfile::tempdir().unwrap();
    let info = terminals
        .open(&on(), folder.path(), Uuid::new_v4(), 24, 80, None)
        .unwrap();
    assert_eq!(terminals.reap(Duration::from_secs(3600)), 0);
    let sub = terminals.subscribe(info.id, 0).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        terminals.reap(Duration::from_millis(1)),
        0,
        "an attached terminal is never idle"
    );
    drop(sub);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(terminals.reap(Duration::from_millis(1)), 1);
    assert!(terminals.list().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_starts_in_the_shell_that_was_picked() {
    let terminals = Terminals::new();
    let folder = tempfile::tempdir().unwrap();
    let me = Uuid::new_v4();
    let shells = acpd::shell::available_shells();
    assert!(!shells.is_empty(), "this host offers at least one shell");
    // Each id is unique and a client can only choose among them.
    let mut ids: Vec<_> = shells.iter().map(|s| s.id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), shells.len());
    let picked = &shells[0];
    let info = terminals
        .open(&on(), folder.path(), me, 24, 80, Some(&picked.id))
        .unwrap();
    assert_eq!(info.shell, picked.id);
    terminals.close(info.id);
    for bad in [
        "definitely-not-a-shell",
        r"C:\Windows\System32\cmd.exe",
        "/bin/sh",
        "",
    ] {
        assert_eq!(
            terminals
                .open(&on(), folder.path(), me, 24, 80, Some(bad))
                .unwrap_err(),
            TerminalError::UnknownShell,
            "{bad} is not an id"
        );
    }
    assert!(terminals.list().is_empty());
}
