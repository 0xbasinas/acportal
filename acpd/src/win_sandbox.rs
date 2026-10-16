//! Windows sandbox for host-approved terminal commands (agent setting `sandbox: auto`).
//!
//! The approach follows Chromium's and Codex CLI's AppContainer sandboxes. acpd runs itself
//! as a small launcher (`acpd __acpd-sandbox-exec …`) with the terminal's pipes. The launcher:
//!
//! 1. creates a fresh AppContainer profile named by the host (`acpd.sbx.<pid>.<n>`),
//! 2. grants that AppContainer SID modify rights on the workspace folder (inherited by
//!    everything below it); nothing else on disk is granted, so the user profile and other
//!    folders stay closed while system folders stay readable through ALL APPLICATION PACKAGES,
//! 3. starts the approved command in the AppContainer with the `internetClient` and
//!    `privateNetworkClientServer` capabilities, matching the Linux sandbox which keeps the
//!    network (outbound loopback to the host's own services stays blocked by Windows),
//! 4. puts it in a kill-on-close job object (the host's own job around the launcher
//!    already kills the whole tree on stop or timeout), waits, and exits with its code,
//! 5. removes the ACL entry and deletes the profile.
//!
//! The host runs the same cleanup again after the launcher ends, so a killed launcher does
//! not leave the grant behind. The command line after the marker is passed to
//! `CreateProcessW` unchanged, so `cmd /D /S /C "<line>"` runs exactly as approved.
use std::path::{Path, PathBuf};

/// First argument that turns acpd into the launcher.
pub const LAUNCHER_ARG: &str = "__acpd-sandbox-exec";
/// Separates launcher options from the command line it runs verbatim.
pub const MARKER: &str = "--acpd-sandbox--";

/// The raw command line after the marker token, exactly as written (cross-platform helper).
pub fn tail_after_marker(command_line: &str) -> Option<&str> {
    let mut search = 0;
    while let Some(found) = command_line[search..].find(MARKER) {
        let start = search + found;
        let end = start + MARKER.len();
        let before_ok = start == 0 || command_line[..start].ends_with([' ', '\t']);
        let after = &command_line[end..];
        if before_ok && (after.is_empty() || after.starts_with([' ', '\t'])) {
            return Some(after.trim_start_matches([' ', '\t']));
        }
        search = end;
    }
    None
}

static LAUNCHER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Called by the acpd binary, which handles [`LAUNCHER_ARG`]. Other programs that embed the
/// library (tests, tools) do not, so their terminals stay unsandboxed unless set explicitly.
pub fn enable_launcher() {
    LAUNCHER.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether this executable can act as the launcher.
pub fn launcher_enabled() -> bool {
    LAUNCHER.load(std::sync::atomic::Ordering::Relaxed)
}

/// A profile name unique to this terminal.
pub fn profile_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "acpd.sbx.{}.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Launcher arguments placed before the program's own arguments.
pub fn launcher_args(
    workspace: &Path,
    cwd: &Path,
    executable: &Path,
    profile: &str,
) -> Vec<std::ffi::OsString> {
    vec![
        LAUNCHER_ARG.into(),
        "--profile".into(),
        profile.into(),
        "--workspace".into(),
        workspace.into(),
        "--cwd".into(),
        cwd.into(),
        MARKER.into(),
        executable.into(),
    ]
}

/// `\\?\C:\x` → `C:\x`; other paths unchanged. Some programs (cmd.exe) refuse verbatim
/// current directories.
pub fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

#[cfg(windows)]
pub use imp::{available, cleanup, launcher_main};

#[cfg(windows)]
mod imp {
    use super::*;
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr::null, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree, WAIT_OBJECT_0},
        Security::{
            ACL,
            Authorization::{
                ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW,
                REVOKE_ACCESS, SE_FILE_OBJECT, SetEntriesInAclW, SetNamedSecurityInfoW,
                TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
            },
            DACL_SECURITY_INFORMATION, FreeSid,
            Isolation::{
                CreateAppContainerProfile, DeleteAppContainerProfile,
                DeriveAppContainerSidFromAppContainerName,
            },
            PSECURITY_DESCRIPTOR, PSID, SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES,
            SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        },
        Storage::FileSystem::{
            DELETE, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        },
        System::{
            Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
            Environment::GetCommandLineW,
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            Threading::{
                CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
                DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
                INFINITE, InitializeProcThreadAttributeList,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, PROCESS_INFORMATION, ResumeThread,
                STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute,
                WaitForSingleObject,
            },
        },
    };

    const SE_GROUP_ENABLED: u32 = 0x4;
    /// HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)
    const ALREADY_EXISTS: i32 = 0x800700B7u32 as i32;

    fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(Some(0)).collect()
    }

    /// An AppContainer SID owned by us (freed with FreeSid).
    struct Sid(PSID);
    impl Drop for Sid {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { FreeSid(self.0) };
            }
        }
    }

    fn container(profile: &str, create: bool) -> Option<Sid> {
        let name = wide(profile);
        let mut sid: PSID = null_mut();
        let hr = if create {
            unsafe {
                CreateAppContainerProfile(
                    name.as_ptr(),
                    name.as_ptr(),
                    wide("acpd sandboxed terminal command").as_ptr(),
                    null(),
                    0,
                    &mut sid,
                )
            }
        } else {
            ALREADY_EXISTS
        };
        if hr == ALREADY_EXISTS {
            let hr = unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) };
            return (hr >= 0).then_some(Sid(sid));
        }
        (hr >= 0).then_some(Sid(sid))
    }

    /// Add (grant) or remove (revoke) the AppContainer's rights on `folder`.
    fn set_access(folder: &Path, sid: PSID, grant: bool) -> bool {
        let path = wide(folder);
        let mut dacl: *mut ACL = null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        let got = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor,
            )
        };
        if got != ERROR_SUCCESS {
            return false;
        }
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_GENERIC_READ
                | FILE_GENERIC_WRITE
                | FILE_GENERIC_EXECUTE
                | DELETE,
            grfAccessMode: if grant { GRANT_ACCESS } else { REVOKE_ACCESS },
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: 0,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: sid as _,
            },
        };
        let mut next: *mut ACL = null_mut();
        let ok = unsafe { SetEntriesInAclW(1, &entry, dacl, &mut next) } == ERROR_SUCCESS
            && unsafe {
                SetNamedSecurityInfoW(
                    path.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    next,
                    null(),
                )
            } == ERROR_SUCCESS;
        unsafe {
            if !next.is_null() {
                LocalFree(next as _);
            }
            LocalFree(descriptor as _);
        }
        ok
    }

    /// Remove the workspace grant and the profile. Safe to call more than once.
    pub fn cleanup(profile: &str, workspace: &Path) {
        if let Some(sid) = container(profile, false) {
            set_access(&plain_path(workspace), sid.0, false);
        }
        unsafe { DeleteAppContainerProfile(wide(profile).as_ptr()) };
    }

    /// Whether AppContainers can be created here; probed once.
    pub fn available() -> bool {
        static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *OK.get_or_init(|| {
            let name = profile_name();
            let ok = container(&name, true).is_some();
            unsafe { DeleteAppContainerProfile(wide(&name).as_ptr()) };
            ok
        })
    }

    fn capability(sddl: &str) -> Option<PSID> {
        let mut sid: PSID = null_mut();
        let ok = unsafe { ConvertStringSidToSidW(wide(sddl).as_ptr(), &mut sid) } != 0;
        ok.then_some(sid)
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    fn run(
        profile: &str,
        workspace: &Path,
        cwd: &Path,
        executable: &Path,
        line: &str,
    ) -> Result<u32, String> {
        let sid = container(profile, true).ok_or("could not create the AppContainer profile")?;
        if !set_access(workspace, sid.0, true) {
            return Err("could not grant the sandbox access to the workspace".into());
        }
        // internetClient, privateNetworkClientServer: network like the Linux sandbox.
        let caps: Vec<PSID> = ["S-1-15-3-1", "S-1-15-3-3"]
            .iter()
            .filter_map(|s| capability(s))
            .collect();
        let mut attrs: Vec<SID_AND_ATTRIBUTES> = caps
            .iter()
            .map(|sid| SID_AND_ATTRIBUTES {
                Sid: *sid,
                Attributes: SE_GROUP_ENABLED,
            })
            .collect();
        let security = SECURITY_CAPABILITIES {
            AppContainerSid: sid.0,
            Capabilities: attrs.as_mut_ptr(),
            CapabilityCount: attrs.len() as u32,
            Reserved: 0,
        };
        let result = (|| unsafe {
            let mut size = 0usize;
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size);
            let mut buffer = vec![0u8; size];
            let list = buffer.as_mut_ptr() as _;
            if InitializeProcThreadAttributeList(list, 1, 0, &mut size) == 0 {
                return Err("attribute list".to_string());
            }
            let updated = UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                &security as *const _ as _,
                std::mem::size_of::<SECURITY_CAPABILITIES>(),
                null_mut(),
                null(),
            );
            if updated == 0 {
                DeleteProcThreadAttributeList(list);
                return Err("security capabilities attribute".to_string());
            }
            let mut info: STARTUPINFOEXW = std::mem::zeroed();
            info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            info.StartupInfo.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
            info.StartupInfo.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
            info.StartupInfo.hStdError = GetStdHandle(STD_ERROR_HANDLE);
            info.lpAttributeList = list;
            let app = wide(executable);
            let mut command_line = wide(line);
            let dir = wide(cwd);
            let mut process: PROCESS_INFORMATION = std::mem::zeroed();
            let created = CreateProcessW(
                app.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
                null(),
                dir.as_ptr(),
                &info.StartupInfo,
                &mut process,
            );
            DeleteProcThreadAttributeList(list);
            if created == 0 {
                return Err(format!(
                    "could not start the sandboxed command ({})",
                    std::io::Error::last_os_error()
                ));
            }
            let child = Handle(process.hProcess);
            let thread = Handle(process.hThread);
            let job = Handle(CreateJobObjectW(null(), null()));
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if job.0.is_null()
                || SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) == 0
                || AssignProcessToJobObject(job.0, child.0) == 0
            {
                windows_sys::Win32::System::Threading::TerminateProcess(child.0, 1);
                return Err("could not put the sandboxed command in a job".into());
            }
            ResumeThread(thread.0);
            if WaitForSingleObject(child.0, INFINITE) != WAIT_OBJECT_0 {
                return Err("waiting for the sandboxed command failed".into());
            }
            let mut code = 1u32;
            GetExitCodeProcess(child.0, &mut code);
            Ok(code)
        })();
        for cap in caps {
            unsafe { LocalFree(cap as _) };
        }
        drop(attrs);
        result
    }

    /// Entry point for `acpd __acpd-sandbox-exec --profile P --workspace W --cwd C -- exe …`.
    pub fn launcher_main() -> i32 {
        let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
        let value = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .map(PathBuf::from)
        };
        let marker = args.iter().position(|a| a == MARKER);
        let raw = unsafe {
            let p = GetCommandLineW();
            let mut n = 0;
            while *p.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
        };
        let (Some(profile), Some(workspace), Some(cwd), Some(marker), Some(line)) = (
            value("--profile"),
            value("--workspace"),
            value("--cwd"),
            marker,
            tail_after_marker(&raw),
        ) else {
            eprintln!("acpd sandbox: invalid launcher arguments");
            return 126;
        };
        let Some(executable) = args.get(marker + 1).map(PathBuf::from) else {
            eprintln!("acpd sandbox: missing program");
            return 126;
        };
        let profile = profile.to_string_lossy().into_owned();
        let workspace = plain_path(&workspace);
        let code = match run(&profile, &workspace, &plain_path(&cwd), &executable, line) {
            Ok(code) => code as i32,
            Err(error) => {
                eprintln!("acpd sandbox: {error}");
                126
            }
        };
        cleanup(&profile, &workspace);
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_command_line_after_the_marker_is_kept_verbatim() {
        let line = r#""C:\acpd.exe" __acpd-sandbox-exec --profile p --workspace C:\w --cwd C:\w --acpd-sandbox-- C:\Windows\System32\cmd.exe /D /S /C "echo a && echo ""b""""#;
        assert_eq!(
            tail_after_marker(line).unwrap(),
            r#"C:\Windows\System32\cmd.exe /D /S /C "echo a && echo ""b""""#
        );
        // A marker inside an argument does not count.
        assert_eq!(
            tail_after_marker("x --cwd C:\\a--acpd-sandbox--b --acpd-sandbox-- y z"),
            Some("y z")
        );
        assert_eq!(tail_after_marker("no marker here"), None);
        assert_eq!(
            plain_path(Path::new(r"\\?\C:\w\x")),
            PathBuf::from(r"C:\w\x")
        );
        assert_eq!(
            plain_path(Path::new(r"\\?\UNC\s\x")),
            PathBuf::from(r"\\?\UNC\s\x")
        );
        let args = launcher_args(Path::new("W"), Path::new("C"), Path::new("E"), "acpd.sbx.1");
        assert_eq!(args.first().unwrap(), LAUNCHER_ARG);
        assert_eq!(args[args.len() - 2], MARKER);
        assert!(profile_name().starts_with("acpd.sbx.") && profile_name() != profile_name());
    }
}
