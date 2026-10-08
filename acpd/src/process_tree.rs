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
            // A fresh group is established by the OS before the executable starts.
            command.process_group(0);
            let mut child = command.spawn().context("launch isolated process group")?;
            let group = match child.id().and_then(|id| i32::try_from(id).ok()) {
                Some(id) => id,
                None => {
                    let _ = child.start_kill();
                    anyhow::bail!("process group id unavailable")
                }
            };
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
            // Signal only the group created for this process, never the daemon's group.
            if unsafe { libc::kill(-group, libc::SIGKILL) } != 0 {
                let failure = std::io::Error::last_os_error();
                if failure.raw_os_error() != Some(libc::ESRCH) {
                    return Err(failure.into());
                }
            }
        }
        Ok(())
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
