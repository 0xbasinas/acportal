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
            let mut child = command.spawn().context("launch job-owned process")?;
            // Tokio exposes the process handle only after spawn. Descendants created
            // before assignment are not covered; do not describe this as an OS sandbox.
            let process = child.raw_handle().context("process handle unavailable")?;
            if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process) } == 0 {
                let failure = std::io::Error::last_os_error();
                let _ = child.start_kill();
                return Err(failure).context("assign agent process to cleanup job");
            }
            Ok((child, Self { job }))
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
impl Drop for ProcessTree {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
