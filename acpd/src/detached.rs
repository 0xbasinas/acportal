//! Detached worker launch with literal arguments and no inherited Windows handles.
use anyhow::{Context, Result};
use std::{ffi::OsString, path::Path};

pub(crate) struct Child {
    #[cfg(windows)]
    process: std::os::windows::io::OwnedHandle,
    #[cfg(windows)]
    pid: u32,
    #[cfg(unix)]
    process: std::process::Child,
}
impl Child {
    pub fn spawn(executable: &Path, args: &[OsString]) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::{
                os::unix::process::CommandExt,
                process::{Command, Stdio},
            };
            let mut command = Command::new(executable);
            command
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            // setsid is async-signal-safe; no allocations or locks between fork and exec.
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
            Ok(Self {
                process: command.spawn().context("launch detached host")?,
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
            use windows_sys::Win32::System::Threading::{
                CREATE_NEW_PROCESS_GROUP, CreateProcessW, DETACHED_PROCESS, PROCESS_INFORMATION,
                STARTUPINFOW,
            };
            let application: Vec<u16> = executable.as_os_str().encode_wide().chain([0]).collect();
            let mut line = quote(&executable.as_os_str().encode_wide().collect::<Vec<_>>());
            for arg in args {
                line.push(b' ' as u16);
                line.extend(quote(&arg.encode_wide().collect::<Vec<_>>()));
            }
            line.push(0);
            // Zero initialization is the documented STARTUPINFO/PROCESS_INFORMATION baseline.
            let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut information: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
            // bInheritHandles=false prevents even unused caller pipes from holding output open.
            let created = unsafe {
                CreateProcessW(
                    application.as_ptr(),
                    line.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
                    std::ptr::null(),
                    std::ptr::null(),
                    &startup,
                    &mut information,
                )
            };
            if created == 0 {
                return Err(std::io::Error::last_os_error()).context("launch detached host");
            }
            let process =
                unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(information.hProcess) };
            let thread =
                unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(information.hThread) };
            drop(thread);
            Ok(Self {
                process,
                pid: information.dwProcessId,
            })
        }
    }
    pub fn id(&self) -> u32 {
        #[cfg(windows)]
        {
            self.pid
        }
        #[cfg(unix)]
        {
            self.process.id()
        }
    }
    pub fn exited(&mut self) -> Result<bool> {
        #[cfg(unix)]
        {
            Ok(self.process.try_wait()?.is_some())
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{
                Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
                System::Threading::WaitForSingleObject,
            };
            match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                _ => Err(std::io::Error::last_os_error().into()),
            }
        }
    }
    pub fn kill(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            self.process.kill()?;
            Ok(())
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            if unsafe {
                windows_sys::Win32::System::Threading::TerminateProcess(
                    self.process.as_raw_handle(),
                    1,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(())
        }
    }
    pub fn wait(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            self.process.wait()?;
            Ok(())
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{
                Foundation::WAIT_OBJECT_0,
                System::Threading::{INFINITE, WaitForSingleObject},
            };
            if unsafe { WaitForSingleObject(self.process.as_raw_handle(), INFINITE) }
                != WAIT_OBJECT_0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(())
        }
    }
}
#[cfg(any(windows, test))]
fn quote(arg: &[u16]) -> Vec<u16> {
    let mut output = vec![b'"' as u16];
    let mut slashes = 0;
    for &ch in arg {
        if ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if ch == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        output.push(ch);
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_arguments_preserve_spaces_quotes_and_trailing_slashes() {
        for (input, expected) in [
            ("", "\"\""),
            ("C:\\a b\\", "\"C:\\a b\\\\\""),
            ("a\"b", "\"a\\\"b\""),
        ] {
            assert_eq!(
                String::from_utf16(&quote(&input.encode_utf16().collect::<Vec<_>>())).unwrap(),
                expected
            );
        }
    }
}
