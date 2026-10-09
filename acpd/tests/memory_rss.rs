//! Whole-process memory check for the host's session buffers (Linux and Windows).
//!
//! This file holds a single test so the test binary's resident set size reflects only
//! this scenario: several sessions stream far more agent output than their history
//! limit while one subscriber per session stalls and another drains. The resident set
//! growth and peak must stay near `sessions × history_bytes`, far below the streamed
//! volume. Linux reads `/proc/self/status` (VmRSS/VmHWM); Windows reads the process
//! working set and its peak (`GetProcessMemoryInfo`). macOS reads the resident size
//! (`proc_pidinfo` task info) and the peak (`getrusage`, bytes on macOS). All include
//! allocator slack; this is a regression bound, not a precise heap profile.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
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

/// (current, peak) resident memory of this process in KiB.
#[cfg(target_os = "linux")]
fn resident_kib() -> (usize, usize) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
            .unwrap_or_else(|| panic!("{name} missing"))
    };
    (field("VmRSS:"), field("VmHWM:"))
}

/// (current, peak) resident memory of this process in KiB.
#[cfg(target_os = "macos")]
fn resident_kib() -> (usize, usize) {
    // SAFETY: both structs are plain data written by the kernel; the buffer size passed
    // matches the struct, and the return values are checked before use.
    unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        let written = libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTASKINFO,
            0,
            (&mut info as *mut libc::proc_taskinfo).cast(),
            size,
        );
        assert_eq!(written, size, "proc_pidinfo failed");
        let mut usage: libc::rusage = std::mem::zeroed();
        assert_eq!(
            libc::getrusage(libc::RUSAGE_SELF, &mut usage),
            0,
            "getrusage failed"
        );
        (
            (info.pti_resident_size / 1024) as usize,
            usage.ru_maxrss as usize / 1024,
        )
    }
}

/// (current, peak) working set of this process in KiB.
#[cfg(windows)]
fn resident_kib() -> (usize, usize) {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    // SAFETY: the counters struct is plain data, its size is passed, and the pseudo
    // handle from GetCurrentProcess needs no closing.
    let counters = unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        assert_ne!(
            GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb),
            0,
            "GetProcessMemoryInfo failed"
        );
        counters
    };
    (
        counters.WorkingSetSize / 1024,
        counters.PeakWorkingSetSize / 1024,
    )
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
    let (baseline, _) = resident_kib();
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
    let (after, peak) = resident_kib();
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
        "memory_rss: streamed {streamed_mib} MiB over {SESSIONS} sessions in {:.1}s; journal holds {} KiB; resident baseline {baseline} KiB, after {after} KiB (+{} KiB); peak {peak} KiB (+{} KiB over baseline)",
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
