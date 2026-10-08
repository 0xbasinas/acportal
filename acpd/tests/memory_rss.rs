//! Whole-process memory check for the host's session buffers (Linux only).
//!
//! This file holds a single test so the test binary's resident set size reflects only
//! this scenario: several sessions stream far more agent output than their history
//! limit while one subscriber per session stalls and another drains. The resident set
//! growth and peak must stay near `sessions × history_bytes`, far below the streamed
//! volume. It measures `/proc/self/status` (VmRSS/VmHWM), so it includes allocator
//! slack; it is a regression bound, not a precise heap profile.
#![cfg(target_os = "linux")]
use acpd::{
    config::{Config, RuntimeConfig},
    registry::{AgentDefinition, Registry},
    session::SessionManager,
};
use serde_json::json;

const SESSIONS: usize = 4;
const UPDATES_PER_SESSION: usize = 400;
const UPDATE_BYTES: usize = 200 * 1024;
const HISTORY_BYTES: usize = 8 * 1024 * 1024;

fn status_kib(field: &str) -> usize {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix(field))
        .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or_else(|| panic!("{field} missing"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn streamed_output_far_beyond_history_keeps_resident_memory_bounded() {
    let workspace = tempfile::tempdir().unwrap();
    let definition: AgentDefinition = serde_json::from_value(json!({
        "id":"test-agent","name":"Test agent",
        "command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":[]}))
    .unwrap();
    let config = Config {
        workspace_roots: vec![workspace.path().into()],
        runtime: RuntimeConfig {
            request_timeout_seconds: 30,
            history_bytes: HISTORY_BYTES,
            ..Default::default()
        },
        ..Default::default()
    };
    let registry = Registry::parse(&serde_json::to_string(&vec![definition]).unwrap()).unwrap();
    let manager = SessionManager::new(config, registry).unwrap();
    let mut sessions = Vec::new();
    for _ in 0..SESSIONS {
        sessions.push(
            manager
                .create("test-agent", workspace.path())
                .await
                .unwrap(),
        );
    }
    let text = "z".repeat(UPDATE_BYTES);
    let emit = |session: &std::sync::Arc<acpd::session::AcpSession>, count: usize| {
        let session = session.clone();
        let text = text.clone();
        tokio::spawn(async move {
            for _ in 0..count {
                session
                    .connection
                    .request(
                        "_mock/emit_update",
                        json!({"update":{"sessionUpdate":"agent_message_chunk",
                            "content":{"type":"text","text":text}}}),
                    )
                    .await
                    .unwrap();
            }
        })
    };
    // Warm up allocator arenas and fill each history once before the baseline.
    let warmup = HISTORY_BYTES / UPDATE_BYTES + 8;
    for task in sessions
        .iter()
        .map(|session| emit(session, warmup))
        .collect::<Vec<_>>()
    {
        task.await.unwrap();
    }
    let baseline = status_kib("VmRSS:");
    // One stalled and one draining subscriber per session, like a stuck and a live phone.
    let stalled: Vec<_> = sessions
        .iter()
        .map(|session| session.connection.subscribe())
        .collect();
    let drained: Vec<_> = sessions
        .iter()
        .map(|session| {
            let mut receiver = session.connection.subscribe();
            tokio::spawn(async move {
                let mut seen = 0usize;
                loop {
                    match receiver.recv().await {
                        Ok(_) => seen += 1,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break seen,
                    }
                }
            })
        })
        .collect();
    let started = std::time::Instant::now();
    for task in sessions
        .iter()
        .map(|session| emit(session, UPDATES_PER_SESSION))
        .collect::<Vec<_>>()
    {
        task.await.unwrap();
    }
    let elapsed = started.elapsed();
    let after = status_kib("VmRSS:");
    let peak = status_kib("VmHWM:");
    let streamed_mib = SESSIONS * UPDATES_PER_SESSION * UPDATE_BYTES / (1024 * 1024);
    let retained: usize = sessions
        .iter()
        .map(|session| {
            session
                .replay(0)
                .events
                .iter()
                .map(|event| event.message.to_string().len())
                .sum::<usize>()
        })
        .sum();
    eprintln!(
        "memory_rss: streamed {streamed_mib} MiB over {SESSIONS} sessions in {:.1}s; journal holds {} KiB; VmRSS baseline {baseline} KiB, after {after} KiB (+{} KiB); VmHWM {peak} KiB (+{} KiB over baseline)",
        elapsed.as_secs_f64(),
        retained / 1024,
        after.saturating_sub(baseline),
        peak.saturating_sub(baseline)
    );
    assert!(retained <= SESSIONS * HISTORY_BYTES);
    // Streamed volume is 312 MiB. Allow 64 MiB of growth over a baseline that already
    // holds full histories; an unbounded journal or retained broadcast would exceed it.
    let bound_kib = 64 * 1024;
    assert!(
        after.saturating_sub(baseline) < bound_kib,
        "resident set grew {} KiB",
        after - baseline
    );
    assert!(
        peak.saturating_sub(baseline) < bound_kib,
        "peak exceeded baseline by {} KiB",
        peak - baseline
    );
    drop(stalled);
    manager.shutdown().await;
    drop(sessions);
    for task in drained {
        // The draining subscriber must have received live events while the others lagged.
        let seen = tokio::time::timeout(std::time::Duration::from_secs(10), task)
            .await
            .expect("subscriber ends after shutdown")
            .unwrap();
        assert!(seen > 0);
    }
}
