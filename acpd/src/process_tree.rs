//! Lifetime-bound process cleanup. This is containment for ordinary descendants, not a sandbox.
use anyhow::{Context, Result};
use tokio::process::{Child, Command};

pub struct ProcessTree {
    #[cfg(windows)]
    job: std::os::windows::io::OwnedHandle,
    #[cfg(unix)]
    group: Option<i32>,
}
impl ProcessTree {
    pub fn spawn(command: &mut Command) -> Result<(Child, Self)> {
        #[cfg(unix)]
        {
            // A fresh session (and with it a fresh group) is established before the
            // executable starts. Descendants that later leave the group keep the session,
            // which lets cleanup find them on Linux.
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut child = command.spawn().context("launch isolated process group")?;
            let group = match child.id().and_then(|id| i32::try_from(id).ok()) {
                Some(id) => id,
                None => {
                    let _ = child.start_kill();
                    anyhow::bail!("process group id unavailable")
                }
            };
            records::add(group);
            Ok((child, Self { group: Some(group) }))
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            };
            // The unnamed handle is non-inheritable. Only this guard owns it, so closing
            // the daemon or dropping the task also terminates the assigned job members.
            let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if raw.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let job = unsafe { OwnedHandle::from_raw_handle(raw) };
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                SetInformationJobObject(
                    job.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32,
                )
            };
            if configured == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            // Start the primary thread suspended so no agent code, and therefore no
            // descendant, runs before the process belongs to the job. No caller sets
            // other creation flags; std adds CREATE_UNICODE_ENVIRONMENT itself.
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
            let mut child = command.spawn().context("launch job-owned process")?;
            let process = match child.raw_handle() {
                Some(process) => process,
                None => {
                    let _ = child.start_kill();
                    anyhow::bail!("process handle unavailable")
                }
            };
            if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process) } == 0 {
                let failure = std::io::Error::last_os_error();
                let _ = child.start_kill();
                return Err(failure).context("assign agent process to cleanup job");
            }
            let tree = Self { job };
            let resumed = child
                .id()
                .context("process id unavailable")
                .and_then(resume_suspended_threads);
            if let Err(failure) = resumed {
                // Dropping the tree terminates the job, including the suspended child.
                drop(tree);
                let _ = child.start_kill();
                return Err(failure).context("resume job-owned process");
            }
            Ok((child, tree))
        }
    }
    pub fn terminate(&mut self) -> Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            if unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        #[cfg(unix)]
        if let Some(group) = self.group.take() {
            let result = kill_tree(group);
            records::remove(group);
            result?;
        }
        Ok(())
    }
}
/// Kills the group and session created for one launched process, plus descendants that
/// moved to another process group. Linux finds those through their session id (the
/// launched process is the session leader, so the id equals the group). Other Unix
/// systems follow the parent chain from a snapshot taken before the group is killed.
/// Descendants that start a new session themselves (`setsid`) on non-Linux systems,
/// or that re-parent before the snapshot, are outside this cleanup.
#[cfg(unix)]
pub(crate) fn kill_tree(group: i32) -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    let escaped = unix_snapshot::escaped_descendants(group);
    // Signal only the group created for this process, never the daemon's group.
    let outcome = signal_group(group);
    #[cfg(target_os = "linux")]
    let escaped = linux_proc::session_members(group);
    for pid in escaped {
        if pid > 1 && pid != std::process::id() as i32 {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
    outcome
}
#[cfg(unix)]
fn signal_group(group: i32) -> Result<()> {
    if group <= 1 || group == unsafe { libc::getpgid(0) } || group == unsafe { libc::getsid(0) } {
        anyhow::bail!("refusing to signal process group {group}");
    }
    if unsafe { libc::kill(-group, libc::SIGKILL) } != 0 {
        let failure = std::io::Error::last_os_error();
        if failure.raw_os_error() != Some(libc::ESRCH) {
            return Err(failure.into());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(crate) mod linux_proc {
    /// state, pgrp, session and start time (clock ticks since boot) of a live process.
    pub struct Stat {
        pub state: char,
        pub group: i32,
        pub session: i32,
        pub start: u64,
    }
    pub fn stat(pid: i32) -> Option<Stat> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let fields: Vec<&str> = text[text.rfind(')')? + 1..].split_whitespace().collect();
        Some(Stat {
            state: fields.first()?.chars().next()?,
            group: fields.get(2)?.parse().ok()?,
            session: fields.get(3)?.parse().ok()?,
            // Field 22 overall; index 19 after the command name.
            start: fields.get(19)?.parse().ok()?,
        })
    }
    pub fn boot_id() -> String {
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map(|id| id.trim().to_owned())
            .unwrap_or_default()
    }
    /// Live (non-zombie) processes whose session id is `session`.
    pub fn session_members(session: i32) -> Vec<i32> {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<i32>().ok())
            .filter(|pid| {
                stat(*pid).is_some_and(|stat| {
                    stat.session == session && stat.state != 'Z' && stat.state != 'X'
                })
            })
            .collect()
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
pub(crate) mod unix_snapshot {
    /// (pid, ppid, pgid) for every process, through `ps` (macOS has no `/proc`).
    fn table() -> Vec<(i32, i32, i32)> {
        let Ok(output) = std::process::Command::new("ps")
            .args(["-A", "-o", "pid=", "-o", "ppid=", "-o", "pgid="])
            .output()
        else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let mut fields = line
                    .split_whitespace()
                    .map(|field| field.parse::<i32>().ok());
                Some((fields.next()??, fields.next()??, fields.next()??))
            })
            .collect()
    }
    /// Descendants of the group's members that are no longer in the group.
    pub fn escaped_descendants(group: i32) -> Vec<i32> {
        let table = table();
        let mut owned: Vec<i32> = table
            .iter()
            .filter(|row| row.2 == group)
            .map(|row| row.0)
            .collect();
        let mut escaped = Vec::new();
        let mut index = 0;
        while index < owned.len() {
            let parent = owned[index];
            for row in table
                .iter()
                .filter(|row| row.1 == parent && !owned.contains(&row.0))
            {
                owned.push(row.0);
                if row.2 != group {
                    escaped.push(row.0);
                }
            }
            index += 1;
        }
        escaped
    }
    /// Start time as printed by `ps -o lstart=` (one-second resolution).
    pub fn start(pid: i32) -> Option<String> {
        let output = std::process::Command::new("ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!text.is_empty()).then_some(text)
    }
    pub fn group(pid: i32) -> Option<i32> {
        let output = std::process::Command::new("ps")
            .args(["-o", "stat=", "-o", "pgid=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let mut fields = text.split_whitespace();
        if fields.next()?.starts_with('Z') {
            return None;
        }
        fields.next()?.parse().ok()
    }
}

/// Records of launched process groups, so a host that was killed abruptly can stop the
/// groups it left behind the next time it starts with the same state directory.
///
/// Each record names the group, the identity (start time, and boot id on Linux) of its
/// leader, and the host process that launched it. A sweep signals a group only if the
/// recording host is gone and the group's leader still has the recorded identity, so a
/// reused process id is never signalled. On Linux, when the leader has already exited,
/// the remaining members of its session (same boot, started no earlier than the leader)
/// are stopped; the session number cannot be reused while they exist. Elsewhere such
/// leftovers are not signalled, because their ownership can no longer be proven.
#[cfg(unix)]
pub mod records {
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    pub const FILE_NAME: &str = "process-groups.json";

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Record {
        pub group: i32,
        pub start: String,
        pub owner: i32,
        pub owner_start: String,
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct Sweep {
        /// Groups that were signalled.
        pub stopped: Vec<i32>,
        /// Records dropped because the leader is gone or is a different process.
        pub stale: usize,
        /// Records kept because the recording host is still running.
        pub live_owner: usize,
    }

    struct State {
        path: PathBuf,
        records: Vec<Record>,
    }
    static STATE: Mutex<Option<State>> = Mutex::new(None);

    /// Identity of a live process: `None` once it has exited or became a zombie.
    pub fn identity(pid: i32) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            let stat = super::linux_proc::stat(pid)?;
            if stat.state == 'Z' || stat.state == 'X' {
                return None;
            }
            Some(format!("{}:{}", super::linux_proc::boot_id(), stat.start))
        }
        #[cfg(not(target_os = "linux"))]
        {
            super::unix_snapshot::group(pid)?;
            super::unix_snapshot::start(pid)
        }
    }
    fn group_of(pid: i32) -> Option<i32> {
        #[cfg(target_os = "linux")]
        {
            Some(super::linux_proc::stat(pid)?.group)
        }
        #[cfg(not(target_os = "linux"))]
        {
            super::unix_snapshot::group(pid)
        }
    }

    pub fn load(path: &Path) -> Vec<Record> {
        // A missing, oversized or unreadable file holds no usable records.
        match std::fs::metadata(path) {
            Ok(meta) if meta.len() <= 1 << 20 => {}
            _ => return Vec::new(),
        }
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    fn save(path: &Path, records: &[Record]) -> std::io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let temporary = path.with_extension("json.tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(records).unwrap_or_default())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    }

    /// Stops groups left by a previous host that used this state directory, then records
    /// groups launched from now on in that directory.
    pub fn enable(directory: &Path) -> Sweep {
        let path = directory.join(FILE_NAME);
        let mut guard = STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (sweep, kept) = sweep_records(load(&path));
        if let Err(error) = save(&path, &kept) {
            tracing::warn!(%error, "could not update process group records");
        }
        if !sweep.stopped.is_empty() {
            tracing::warn!(
                groups = sweep.stopped.len(),
                "stopped agent process groups left by an earlier host"
            );
        }
        // Records kept for a still-running host stay in the file; this host adds its own.
        *guard = Some(State {
            path,
            records: kept,
        });
        sweep
    }

    /// Applies the sweep rules to `records` and returns what to keep.
    pub fn sweep_records(records: Vec<Record>) -> (Sweep, Vec<Record>) {
        let me = std::process::id() as i32;
        let mut sweep = Sweep::default();
        let mut kept = Vec::new();
        for record in records {
            let owner_alive = record.owner == me
                || identity(record.owner).is_some_and(|id| id == record.owner_start);
            if owner_alive {
                sweep.live_owner += 1;
                kept.push(record);
                continue;
            }
            let leader_matches = identity(record.group).is_some_and(|id| id == record.start)
                && group_of(record.group) == Some(record.group);
            if leader_matches && super::kill_tree(record.group).is_ok() {
                sweep.stopped.push(record.group);
                continue;
            }
            #[cfg(target_os = "linux")]
            if orphaned_session(&record) {
                sweep.stopped.push(record.group);
                continue;
            }
            sweep.stale += 1;
        }
        (sweep, kept)
    }

    /// Linux only: the session leader (the launched agent) has exited, which agents do
    /// when the host's pipes close, but descendants still run in its session. The number
    /// cannot have been reused while those members exist, and no live process may own it
    /// as a pid. Only members on the same boot that started no earlier than the recorded
    /// leader are signalled.
    #[cfg(target_os = "linux")]
    fn orphaned_session(record: &Record) -> bool {
        use super::linux_proc;
        let Some((boot, start)) = record.start.split_once(':') else {
            return false;
        };
        let Ok(start) = start.parse::<u64>() else {
            return false;
        };
        if boot.is_empty()
            || boot != linux_proc::boot_id()
            || linux_proc::stat(record.group).is_some()
        {
            return false;
        }
        let members: Vec<i32> = linux_proc::session_members(record.group)
            .into_iter()
            .filter(|pid| linux_proc::stat(*pid).is_some_and(|stat| stat.start >= start))
            .collect();
        for pid in &members {
            if *pid > 1 && *pid != std::process::id() as i32 {
                unsafe { libc::kill(*pid, libc::SIGKILL) };
            }
        }
        !members.is_empty()
    }

    pub(crate) fn add(group: i32) {
        let mut guard = STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(state) = guard.as_mut() else { return };
        let me = std::process::id() as i32;
        let (Some(start), Some(owner_start)) = (identity(group), identity(me)) else {
            return;
        };
        state.records.push(Record {
            group,
            start,
            owner: me,
            owner_start,
        });
        if let Err(error) = save(&state.path, &state.records) {
            tracing::warn!(%error, "could not record agent process group");
        }
    }
    pub(crate) fn remove(group: i32) {
        let mut guard = STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(state) = guard.as_mut() else { return };
        let me = std::process::id() as i32;
        let before = state.records.len();
        state
            .records
            .retain(|record| !(record.group == group && record.owner == me));
        if state.records.len() != before
            && let Err(error) = save(&state.path, &state.records)
        {
            tracing::warn!(%error, "could not update process group records");
        }
    }
}

/// Resumes the threads of a process created with CREATE_SUSPENDED. A new process has
/// exactly one thread; finding none is reported so the caller can kill the job.
#[cfg(windows)]
fn resume_suspended_threads(pid: u32) -> Result<()> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error()).context("snapshot threads");
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut resumed = 0usize;
    let mut more = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid {
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() {
                return Err(std::io::Error::last_os_error()).context("open agent thread");
            }
            let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
            if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                return Err(std::io::Error::last_os_error()).context("resume agent thread");
            }
            resumed += 1;
        }
        more = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } != 0;
    }
    anyhow::ensure!(resumed > 0, "no thread found for suspended process {pid}");
    Ok(())
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::records::{Record, identity, sweep_records};
    use std::os::unix::process::CommandExt;

    fn session_sleeper() -> std::process::Child {
        let mut command = std::process::Command::new("sleep");
        command.arg("30");
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command.spawn().unwrap()
    }
    fn exited_pid() -> i32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id() as i32;
        child.wait().unwrap();
        pid
    }
    fn wait_for_start(pid: i32) -> String {
        for _ in 0..100 {
            if let Some(id) = identity(pid) {
                return id;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no identity for {pid}");
    }

    #[test]
    fn sweep_stops_only_groups_whose_leader_identity_matches_and_whose_host_is_gone() {
        let mut matching = session_sleeper();
        let mut reused = session_sleeper();
        let mut live_owner = session_sleeper();
        let (a, b, c) = (
            matching.id() as i32,
            reused.id() as i32,
            live_owner.id() as i32,
        );
        let (start_a, start_c) = (wait_for_start(a), wait_for_start(c));
        wait_for_start(b);
        let dead = exited_pid();
        let me = std::process::id() as i32;
        let records = vec![
            Record {
                group: a,
                start: start_a,
                owner: dead,
                owner_start: "gone".into(),
            },
            // Same pid, different process: a reused id must never be signalled.
            Record {
                group: b,
                start: "not-this-process".into(),
                owner: dead,
                owner_start: "gone".into(),
            },
            // Recorded by a host that is still running (this process).
            Record {
                group: c,
                start: start_c,
                owner: me,
                owner_start: identity(me).unwrap(),
            },
        ];
        let (sweep, kept) = sweep_records(records);
        assert_eq!(sweep.stopped, vec![a]);
        assert_eq!(sweep.stale, 1);
        assert_eq!(sweep.live_owner, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].group, c);
        assert!(
            !matching.wait().unwrap().success(),
            "matching group must be killed"
        );
        assert!(
            reused.try_wait().unwrap().is_none(),
            "mismatched identity must survive"
        );
        assert!(
            live_owner.try_wait().unwrap().is_none(),
            "live host's group must survive"
        );
        for child in [&mut reused, &mut live_owner] {
            child.kill().unwrap();
            child.wait().unwrap();
        }
    }

    #[test]
    fn unreadable_or_oversized_record_files_hold_no_records() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(super::records::FILE_NAME);
        std::fs::write(&path, b"{not json").unwrap();
        assert!(super::records::load(&path).is_empty());
        std::fs::write(&path, vec![b' '; (1 << 20) + 1]).unwrap();
        assert!(super::records::load(&path).is_empty());
        assert!(super::records::load(&directory.path().join("missing.json")).is_empty());
    }

    #[test]
    fn refuses_to_signal_its_own_or_init_group() {
        assert!(super::signal_group(1).is_err());
        assert!(super::signal_group(unsafe { libc::getpgid(0) }).is_err());
    }
}
