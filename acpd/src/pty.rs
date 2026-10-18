//! A pseudo-terminal and the interactive shell on it, for the host terminals (`terminals.rs`).
//!
//! Unix uses `openpty`; Windows uses ConPTY. The shell and everything it starts belong to one
//! process group (Unix) or kill-on-close job (Windows), so ending the terminal ends the tree.
//! This is containment, not a sandbox: the shell runs as the host user.
use anyhow::{Context, Result};
use std::fs::File;
use std::path::Path;

pub struct Spawn<'a> {
    pub program: &'a Path,
    pub args: &'a [String],
    pub cwd: &'a Path,
    /// The whole environment of the shell; nothing else is inherited.
    pub env: &'a [(String, String)],
    pub rows: u16,
    pub cols: u16,
}

pub struct Pty {
    reader: Option<File>,
    writer: File,
    inner: imp::Inner,
}

impl Pty {
    pub fn spawn(spec: &Spawn<'_>) -> Result<Pty> {
        let (reader, writer, inner) = imp::spawn(spec)?;
        Ok(Pty {
            reader: Some(reader),
            writer,
            inner,
        })
    }
    /// The output end. Blocking reads; ends (error or 0 bytes) when the shell has gone.
    pub fn take_reader(&mut self) -> Option<File> {
        self.reader.take()
    }
    /// A second handle to the input end, for a writer thread.
    pub fn writer(&self) -> Result<File> {
        self.writer.try_clone().context("duplicate terminal input")
    }
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        imp::resize(&self.inner, rows.max(1), cols.max(1))
    }
    /// Stops the shell and every process it started. Safe to call twice.
    pub fn kill(&mut self) {
        imp::kill(&mut self.inner);
    }
    /// The exit code once the shell has ended, `None` while it runs.
    pub fn exit_code(&mut self) -> Option<i32> {
        imp::exit_code(&mut self.inner)
    }
}
impl Drop for Pty {
    fn drop(&mut self) {
        imp::kill(&mut self.inner);
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};

    pub struct Inner {
        child: Child,
        group: Option<i32>,
        master: File,
    }

    pub fn spawn(spec: &Spawn<'_>) -> Result<(File, File, Inner)> {
        let size = libc::winsize {
            ws_row: spec.rows.max(1),
            ws_col: spec.cols.max(1),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let (mut master, mut slave) = (-1, -1);
        if unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error()).context("open pseudo-terminal");
        }
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        unsafe {
            libc::fcntl(
                std::os::fd::AsRawFd::as_raw_fd(&master),
                libc::F_SETFD,
                libc::FD_CLOEXEC,
            )
        };
        unsafe {
            libc::fcntl(
                std::os::fd::AsRawFd::as_raw_fd(&slave),
                libc::F_SETFD,
                libc::FD_CLOEXEC,
            )
        };
        let mut command = Command::new(spec.program);
        command
            .args(spec.args)
            .current_dir(spec.cwd)
            .env_clear()
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave.try_clone()?));
        unsafe {
            command.pre_exec(|| {
                // A new session whose controlling terminal is the pty, so job control and Ctrl-C work.
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().context("start terminal shell")?;
        drop(command);
        drop(slave);
        let group = i32::try_from(child.id()).ok();
        if let Some(group) = group {
            crate::process_tree::records::add(group);
        }
        let master = File::from(master);
        let reader = master.try_clone()?;
        let writer = master.try_clone()?;
        Ok((
            reader,
            writer,
            Inner {
                child,
                group,
                master,
            },
        ))
    }

    pub fn resize(inner: &Inner, rows: u16, cols: u16) -> Result<()> {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let fd = std::os::fd::AsRawFd::as_raw_fd(&inner.master);
        if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ as _, &size) } != 0 {
            return Err(std::io::Error::last_os_error()).context("resize terminal");
        }
        Ok(())
    }

    pub fn kill(inner: &mut Inner) {
        if let Some(group) = inner.group.take() {
            let _ = crate::process_tree::kill_tree(group);
            crate::process_tree::records::remove(group);
        }
        let _ = inner.child.kill();
        let _ = inner.child.wait();
    }

    pub fn exit_code(inner: &mut Inner) -> Option<i32> {
        use std::os::unix::process::ExitStatusExt;
        inner
            .child
            .try_wait()
            .ok()
            .flatten()
            .map(|s| s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)))
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{
        CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Console::{
        COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
    };
    use windows_sys::Win32::System::JobObjects::*;
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::*;

    struct Console(HPCON);
    unsafe impl Send for Console {}
    struct Handle(HANDLE);
    unsafe impl Send for Handle {}

    pub struct Inner {
        console: Option<Console>,
        process: Handle,
        job: Option<OwnedHandle>,
    }

    fn pipe() -> Result<(OwnedHandle, OwnedHandle)> {
        let (mut read, mut write): (HANDLE, HANDLE) = (std::ptr::null_mut(), std::ptr::null_mut());
        if unsafe { CreatePipe(&mut read, &mut write, std::ptr::null(), 0) } == 0 {
            return Err(std::io::Error::last_os_error()).context("create pipe");
        }
        Ok(unsafe {
            (
                OwnedHandle::from_raw_handle(read),
                OwnedHandle::from_raw_handle(write),
            )
        })
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// One argument as `CommandLineToArgvW` reads it back.
    fn quote(arg: &str) -> String {
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
            return arg.to_string();
        }
        let mut out = String::from("\"");
        let mut slashes = 0;
        for c in arg.chars() {
            match c {
                '\\' => slashes += 1,
                '"' => {
                    out.push_str(&"\\".repeat(slashes * 2 + 1));
                    out.push('"');
                    slashes = 0;
                }
                _ => {
                    out.push_str(&"\\".repeat(slashes));
                    out.push(c);
                    slashes = 0;
                }
            }
        }
        out.push_str(&"\\".repeat(slashes * 2));
        out.push('"');
        out
    }

    pub fn spawn(spec: &Spawn<'_>) -> Result<(File, File, Inner)> {
        let (in_read, in_write) = pipe()?;
        let (out_read, out_write) = pipe()?;
        let mut console: HPCON = 0;
        let size = COORD {
            X: spec.cols.max(1) as i16,
            Y: spec.rows.max(1) as i16,
        };
        let hr = unsafe {
            CreatePseudoConsole(
                size,
                std::os::windows::io::AsRawHandle::as_raw_handle(&in_read),
                std::os::windows::io::AsRawHandle::as_raw_handle(&out_write),
                0,
                &mut console,
            )
        };
        if hr != 0 {
            anyhow::bail!("this Windows version has no pseudo-terminal support (0x{hr:08x})");
        }
        let console = Console(console);
        // The pseudo-console owns its ends now.
        drop(in_read);
        drop(out_write);

        // The job exists first and the shell is created inside it, so nothing it starts can escape.
        let job = new_job().inspect_err(|_| unsafe {
            ClosePseudoConsole(console.0);
        })?;
        let mut job_handle: HANDLE = std::os::windows::io::AsRawHandle::as_raw_handle(&job);
        let mut list_size = 0usize;
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut list_size) };
        let mut list = vec![0u8; list_size];
        let list_ptr = list.as_mut_ptr().cast::<c_void>();
        if unsafe { InitializeProcThreadAttributeList(list_ptr, 2, 0, &mut list_size) } == 0 {
            let error = std::io::Error::last_os_error();
            unsafe { ClosePseudoConsole(console.0) };
            return Err(error).context("prepare terminal attributes");
        }
        let attached = unsafe {
            UpdateProcThreadAttribute(
                list_ptr,
                0,
                0x0002_0016, // PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE
                console.0 as *const c_void,
                std::mem::size_of::<HPCON>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        let listed = unsafe {
            UpdateProcThreadAttribute(
                list_ptr,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                (&mut job_handle as *mut HANDLE).cast::<c_void>(),
                std::mem::size_of::<HANDLE>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if attached == 0 || listed == 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                DeleteProcThreadAttributeList(list_ptr);
                ClosePseudoConsole(console.0);
            }
            return Err(error).context("attach pseudo-terminal");
        }

        let mut line = quote(&spec.program.to_string_lossy());
        for arg in spec.args {
            line.push(' ');
            line.push_str(&quote(arg));
        }
        let mut line = wide(std::ffi::OsStr::new(&line));
        let mut block: Vec<u16> = Vec::new();
        let mut env: Vec<&(String, String)> = spec.env.iter().collect();
        env.sort_by_key(|(k, _)| k.to_uppercase());
        for (k, v) in env {
            block.extend(format!("{k}={v}").encode_utf16());
            block.push(0);
        }
        block.push(0);
        if block.len() == 1 {
            block.push(0);
        }
        let cwd = wide(spec.cwd.as_os_str());

        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        // Without this the shell would inherit this process's redirected standard handles instead of the pty.
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
        startup.lpAttributeList = list_ptr;
        let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let created = unsafe {
            CreateProcessW(
                std::ptr::null(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                block.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        };
        let failure = (created == 0).then(std::io::Error::last_os_error);
        unsafe { DeleteProcThreadAttributeList(list_ptr) };
        if let Some(error) = failure {
            unsafe { ClosePseudoConsole(console.0) };
            return Err(error).context("start terminal shell");
        }
        let process = Handle(info.hProcess);
        unsafe { CloseHandle(info.hThread) };
        Ok((
            File::from(out_read),
            File::from(in_write),
            Inner {
                console: Some(console),
                process,
                job: Some(job),
            },
        ))
    }

    fn new_job() -> Result<OwnedHandle> {
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error()).context("create terminal job");
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                std::os::windows::io::AsRawHandle::as_raw_handle(&job),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::last_os_error()).context("configure terminal job");
        }
        Ok(job)
    }

    pub fn resize(inner: &Inner, rows: u16, cols: u16) -> Result<()> {
        let console = inner.console.as_ref().context("terminal is closed")?;
        let size = COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        let hr = unsafe { ResizePseudoConsole(console.0, size) };
        anyhow::ensure!(hr == 0, "resize failed (0x{hr:08x})");
        Ok(())
    }

    pub fn kill(inner: &mut Inner) {
        // Closing the job kills the shell and everything it started.
        inner.job.take();
        if let Some(console) = inner.console.take() {
            // Closing can wait for output to drain on older Windows; do it off the caller's thread.
            std::thread::spawn(move || unsafe { ClosePseudoConsole(console.0) });
        }
    }

    pub fn exit_code(inner: &mut Inner) -> Option<i32> {
        if unsafe { WaitForSingleObject(inner.process.0, 0) } != WAIT_OBJECT_0 {
            return None;
        }
        let mut code = 0u32;
        unsafe { GetExitCodeProcess(inner.process.0, &mut code) };
        Some(code as i32)
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }
}
