//! Text access through an opened directory capability, never an ambient path open.
use anyhow::{Context, Result, bail};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub async fn read_async(
    files: Arc<WorkspaceFiles>,
    params: Value,
    session_id: String,
) -> Result<Value> {
    // A timed-out OS read may still be running. Its permit remains with the worker, bounding
    // stuck network-filesystem threads across all sessions rather than spawning more retries.
    let permit = worker_permit()?;
    let worker = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        files.read(params, &session_id)
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .context("filesystem read timed out")?
        .context("filesystem worker failed")?
}
fn worker_permit() -> Result<tokio::sync::OwnedSemaphorePermit> {
    static WORKERS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    WORKERS
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(8)))
        .clone()
        .try_acquire_owned()
        .context("filesystem service is busy")
}

pub struct WorkspaceFiles {
    root: PathBuf,
    directory: Dir,
    maximum_bytes: usize,
}
pub struct PreparedWrite {
    pub path: PathBuf,
    pub content: String,
    pub old_text: Option<String>,
    relative: PathBuf,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteRequest {
    session_id: String,
    path: PathBuf,
    content: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadRequest {
    session_id: String,
    path: PathBuf,
    line: Option<u32>,
    limit: Option<u32>,
}
impl WorkspaceFiles {
    pub fn workspace(&self) -> &Path {
        &self.root
    }
    pub fn prepare_write(&self, params: Value, session_id: &str) -> Result<PreparedWrite> {
        let request: WriteRequest =
            serde_json::from_value(params).context("invalid text write request")?;
        if request.session_id != session_id
            || !request.path.is_absolute()
            || request.content.len() > self.maximum_bytes / 3
        {
            bail!("invalid or oversized text write")
        }
        let name = request.path.file_name().context("file name required")?;
        #[cfg(windows)]
        {
            let name = name.to_string_lossy();
            let stem = name
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            if name.contains(':')
                || name.ends_with(['.', ' '])
                || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
                || (stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
            {
                bail!("special Windows file names are not allowed")
            }
        }
        let parent = request
            .path
            .parent()
            .context("file parent required")?
            .canonicalize()
            .context("file parent unavailable")?;
        let relative = parent
            .strip_prefix(&self.root)
            .context("file parent is outside workspace")?
            .join(name);
        let old_text = self.existing_text(&relative)?;
        if old_text
            .as_ref()
            .is_some_and(|text| text.len() > self.maximum_bytes / 3)
        {
            bail!("existing file is too large for a consent preview")
        }
        Ok(PreparedWrite {
            path: parent.join(name),
            content: request.content,
            old_text,
            relative,
        })
    }
    fn existing_text(&self, relative: &Path) -> Result<Option<String>> {
        match self.directory.symlink_metadata(relative) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("inspect target file"),
            Ok(metadata) => {
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.len() > self.maximum_bytes as u64
                {
                    bail!("target is not a regular bounded file")
                }
                let mut options = OpenOptions::new();
                options.read(true);
                #[cfg(unix)]
                {
                    use cap_std::fs::OpenOptionsExt;
                    options.custom_flags(libc::O_NONBLOCK);
                }
                let file = self
                    .directory
                    .open_with(relative, &options)
                    .context("open target file")?;
                if !file.metadata()?.is_file() {
                    bail!("target file type changed")
                }
                let mut bytes = Vec::new();
                file.take(self.maximum_bytes as u64 + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > self.maximum_bytes {
                    bail!("target grew beyond limit")
                }
                Ok(Some(
                    String::from_utf8(bytes).context("target is not UTF-8")?,
                ))
            }
        }
    }
    pub fn commit_write(
        &self,
        prepared: &PreparedWrite,
        deadline: std::time::Instant,
    ) -> Result<()> {
        if self.existing_text(&prepared.relative)? != prepared.old_text {
            bail!("file changed since permission was requested")
        }
        let parent = prepared.relative.parent().context("file parent required")?;
        let temporary = parent.join(format!(".acpd-{}.tmp", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.access_mode(
                windows_sys::Win32::Foundation::GENERIC_WRITE
                    | windows_sys::Win32::Storage::FileSystem::WRITE_DAC,
            );
        }
        let mut file = self
            .directory
            .open_with(&temporary, &options)
            .context("create temporary workspace file")?;
        let result = (|| {
            file.write_all(prepared.content.as_bytes())?;
            file.sync_all()?;
            if let Ok(metadata) = self.directory.symlink_metadata(&prepared.relative) {
                if metadata.permissions().readonly() {
                    bail!("target became read-only")
                }
                file.set_permissions(metadata.permissions())?;
                #[cfg(windows)]
                {
                    let source = self
                        .directory
                        .open(&prepared.relative)
                        .context("read existing file access policy")?;
                    preserve_windows_dacl(&source, &file)?;
                }
            }
            drop(file);
            if std::time::Instant::now() >= deadline {
                bail!("write deadline expired before commit")
            }
            if self.existing_text(&prepared.relative)? != prepared.old_text {
                bail!("file changed before commit")
            }
            self.directory
                .rename(&temporary, &self.directory, &prepared.relative)
                .context("commit workspace file")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self.directory.remove_file(&temporary);
        }
        result
    }
    pub fn open(workspace: &Path, maximum_bytes: usize) -> Result<Self> {
        let root = workspace.canonicalize().context("workspace unavailable")?;
        let directory = Dir::open_ambient_dir(&root, ambient_authority())
            .context("open workspace directory capability")?;
        Ok(Self {
            root,
            directory,
            maximum_bytes,
        })
    }
    pub fn read(&self, params: Value, session_id: &str) -> Result<Value> {
        let request: ReadRequest =
            serde_json::from_value(params).context("invalid text read request")?;
        if request.session_id != session_id
            || request.line == Some(0)
            || !request.path.is_absolute()
        {
            bail!("invalid session, path or starting line")
        }
        // Canonicalize only to derive a relative name. The actual open remains capability-scoped,
        // so swapping a parent or symlink after this check cannot escape the opened workspace.
        let canonical = request.path.canonicalize().context("file unavailable")?;
        let relative = canonical
            .strip_prefix(&self.root)
            .context("file is outside workspace")?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = self
            .directory
            .open_with(relative, &options)
            .context("open workspace file")?;
        let metadata = file.metadata().context("inspect workspace file")?;
        if !metadata.is_file() || metadata.len() > self.maximum_bytes as u64 {
            bail!("file is not a regular bounded text file")
        }
        let mut bytes = Vec::new();
        file.take(self.maximum_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .context("read workspace file")?;
        if bytes.len() > self.maximum_bytes {
            bail!("text file exceeded read limit")
        }
        let text = String::from_utf8(bytes).context("file is not UTF-8 text")?;
        let content: String = text
            .split_inclusive('\n')
            .skip(request.line.unwrap_or(1) as usize - 1)
            .take(request.limit.map_or(usize::MAX, |value| value as usize))
            .collect();
        Ok(json!({"content": content}))
    }
}

#[cfg(windows)]
fn with_windows_dacl<T>(
    file: &cap_std::fs::File,
    action: impl FnOnce(*const windows_sys::Win32::Security::ACL, bool) -> Result<T>,
) -> Result<T> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
            DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, SE_DACL_PROTECTED,
        },
    };
    let mut descriptor = std::ptr::null_mut();
    let mut dacl = std::ptr::null_mut();
    // GetSecurityInfo owns the descriptor allocation. The ACL pointer is valid only until
    // LocalFree; no borrowed pointer escapes the callback or this thread.
    unsafe {
        let code = GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        );
        if code != 0 {
            return Err(std::io::Error::from_raw_os_error(code as i32).into());
        }
        let mut control = 0u16;
        let mut revision = 0u32;
        let result = if GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) == 0 {
            Err(std::io::Error::last_os_error().into())
        } else {
            action(dacl, control & SE_DACL_PROTECTED != 0)
        };
        LocalFree(descriptor);
        result
    }
}
#[cfg(windows)]
fn preserve_windows_dacl(source: &cap_std::fs::File, target: &cap_std::fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Security::{
        Authorization::{SE_FILE_OBJECT, SetSecurityInfo},
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        UNPROTECTED_DACL_SECURITY_INFORMATION,
    };
    with_windows_dacl(source, |dacl, protected| {
        // The temporary file was explicitly opened with WRITE_DAC; copy through handles
        // rather than resolving an ambient filename that could be replaced by a symlink.
        let code = unsafe {
            SetSecurityInfo(
                target.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION
                    | if protected {
                        PROTECTED_DACL_SECURITY_INFORMATION
                    } else {
                        UNPROTECTED_DACL_SECURITY_INFORMATION
                    },
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null(),
            )
        };
        if code != 0 {
            return Err(std::io::Error::from_raw_os_error(code as i32).into());
        }
        Ok(())
    })
}

pub async fn prepare_write_async(
    files: Arc<WorkspaceFiles>,
    params: Value,
    session_id: String,
) -> Result<PreparedWrite> {
    let permit = worker_permit()?;
    let worker = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        files.prepare_write(params, &session_id)
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .context("prepare write timed out")?
        .context("prepare write worker failed")?
}
pub async fn commit_write_async(files: Arc<WorkspaceFiles>, prepared: PreparedWrite) -> Result<()> {
    let permit = worker_permit()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let worker = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        files.commit_write(&prepared, deadline)
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .context("write timed out; completion may be ambiguous")?
        .context("write worker failed")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_replacement_does_not_modify_other_hard_links() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let original = outside.path().join("original.txt");
        std::fs::write(&original, "original").unwrap();
        let target = root.path().join("link.txt");
        std::fs::hard_link(&original, &target).unwrap();
        let files = WorkspaceFiles::open(root.path(), 1024).unwrap();
        let proposed = files
            .prepare_write(
                json!({"sessionId":"s","path":target,"content":"replacement"}),
                "s",
            )
            .unwrap();
        files
            .commit_write(
                &proposed,
                std::time::Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(std::fs::read_to_string(target).unwrap(), "replacement");
        assert_eq!(std::fs::read_to_string(original).unwrap(), "original");
    }
    #[test]
    fn parent_moved_after_consent_cannot_redirect_a_write_outside_workspace() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let parent = root.path().join("folder");
        std::fs::create_dir(&parent).unwrap();
        let target = parent.join("file.txt");
        std::fs::write(&target, "original").unwrap();
        let files = WorkspaceFiles::open(root.path(), 1024).unwrap();
        let proposed = files
            .prepare_write(
                json!({"sessionId":"s","path":target,"content":"replacement"}),
                "s",
            )
            .unwrap();
        let moved = outside.path().join("moved");
        std::fs::rename(&parent, &moved).unwrap();
        assert!(
            files
                .commit_write(
                    &proposed,
                    std::time::Instant::now() + std::time::Duration::from_secs(5)
                )
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(moved.join("file.txt")).unwrap(),
            "original"
        );
    }
    #[cfg(windows)]
    #[test]
    fn replacement_preserves_protected_windows_acl_and_rejects_device_names() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("private.txt");
        std::fs::write(&path, "private").unwrap();
        crate::security::protect(&path, false).unwrap();
        let files = WorkspaceFiles::open(root.path(), 1024).unwrap();
        let snapshot = || {
            with_windows_dacl(
                &files.directory.open("private.txt").unwrap(),
                |dacl, protected| {
                    let bytes = if dacl.is_null() {
                        vec![]
                    } else {
                        unsafe {
                            std::slice::from_raw_parts(dacl.cast::<u8>(), (*dacl).AclSize as usize)
                                .to_vec()
                        }
                    };
                    Ok((bytes, protected))
                },
            )
            .unwrap()
        };
        let before = snapshot();
        assert!(before.1);
        let proposed = files
            .prepare_write(
                json!({"sessionId":"s","path":path,"content":"replacement"}),
                "s",
            )
            .unwrap();
        files
            .commit_write(
                &proposed,
                std::time::Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(snapshot(), before);
        for name in ["file.txt:stream", "CON.txt", "LPT1", "trailing."] {
            assert!(
                files
                    .prepare_write(
                        json!({"sessionId":"s","path":root.path().join(name),"content":"new"}),
                        "s"
                    )
                    .is_err()
            );
        }
    }
    #[test]
    fn approved_write_creates_and_atomically_replaces_text_without_temporary_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file.txt");
        let files = WorkspaceFiles::open(root.path(), 1024).unwrap();
        let proposed = files
            .prepare_write(
                json!({"sessionId":"s","path":path,"content":"first\n"}),
                "s",
            )
            .unwrap();
        assert!(proposed.old_text.is_none());
        assert!(!path.exists());
        files
            .commit_write(
                &proposed,
                std::time::Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n");
        let proposed = files
            .prepare_write(
                json!({"sessionId":"s","path":path,"content":"second\n"}),
                "s",
            )
            .unwrap();
        assert_eq!(proposed.old_text.as_deref(), Some("first\n"));
        files
            .commit_write(
                &proposed,
                std::time::Instant::now() + std::time::Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second\n");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    fn changed_file_or_expired_commit_preserves_original_content() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file.txt");
        std::fs::write(&path, "original").unwrap();
        let files = WorkspaceFiles::open(root.path(), 1024).unwrap();
        let proposed = files
            .prepare_write(
                json!({"sessionId":"s","path":path,"content":"proposed"}),
                "s",
            )
            .unwrap();
        std::fs::write(&path, "concurrent edit").unwrap();
        assert!(
            files
                .commit_write(
                    &proposed,
                    std::time::Instant::now() + std::time::Duration::from_secs(5)
                )
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "concurrent edit");
        let proposed = files
            .prepare_write(
                json!({"sessionId":"s","path":path,"content":"proposed"}),
                "s",
            )
            .unwrap();
        assert!(
            files
                .commit_write(&proposed, std::time::Instant::now())
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "concurrent edit");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    fn writes_reject_outside_parents_symlinks_binary_and_other_sessions() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let files = WorkspaceFiles::open(root.path(), 1024).unwrap();
        let target = root.path().join("file.txt");
        for params in [
            json!({"sessionId":"s","path":outside.path().join("new.txt"),"content":"new"}),
            json!({"sessionId":"other","path":target,"content":"new"}),
            json!({"sessionId":"s","path":target,"content":"a".repeat(400)}),
        ] {
            assert!(files.prepare_write(params, "s").is_err());
        }
        std::fs::write(&target, [255, 254]).unwrap();
        assert!(
            files
                .prepare_write(json!({"sessionId":"s","path":target,"content":"new"}), "s")
                .is_err()
        );
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "outside").unwrap();
        let link = root.path().join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&secret, &link).unwrap();
        assert!(
            files
                .prepare_write(json!({"sessionId":"s","path":link,"content":"new"}), "s")
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(secret).unwrap(), "outside");
    }
    #[test]
    fn preserves_text_and_applies_one_based_line_ranges() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file.txt");
        std::fs::write(&path, "first\r\nsecond\nlast").unwrap();
        let files = WorkspaceFiles::open(root.path(), 1024).unwrap();
        assert_eq!(
            files
                .read(json!({"sessionId":"s","path":path}), "s")
                .unwrap()["content"],
            "first\r\nsecond\nlast"
        );
        assert_eq!(
            files
                .read(json!({"sessionId":"s","path":path,"line":2,"limit":1}), "s")
                .unwrap()["content"],
            "second\n"
        );
        assert_eq!(
            files
                .read(json!({"sessionId":"s","path":path,"line":99}), "s")
                .unwrap()["content"],
            ""
        );
    }
    #[test]
    fn rejects_other_sessions_outside_paths_directories_binary_and_large_files() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = root.path().join("file.txt");
        std::fs::write(&path, "12345").unwrap();
        let files = WorkspaceFiles::open(root.path(), 4).unwrap();
        for params in [
            json!({"sessionId":"other","path":path}),
            json!({"sessionId":"s","path":path,"line":0}),
            json!({"sessionId":"s","path":"relative.txt"}),
            json!({"sessionId":"s","path":root.path()}),
            json!({"sessionId":"s","path":outside.path()}),
            json!({"sessionId":"s","path":path}),
        ] {
            assert!(files.read(params, "s").is_err());
        }
        std::fs::write(&path, [255, 254]).unwrap();
        assert!(
            files
                .read(json!({"sessionId":"s","path":path}), "s")
                .is_err()
        );
    }
    #[test]
    fn capability_open_rejects_symlink_escape() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "outside").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(outside.path(), root.path().join("link")).unwrap();
        let files = WorkspaceFiles::open(root.path(), 1024).unwrap();
        assert!(
            files
                .read(
                    json!({"sessionId":"s","path":root.path().join("link/secret.txt")}),
                    "s"
                )
                .is_err()
        );
        // Exercise the capability boundary itself, independently of canonical prefix validation.
        assert!(files.directory.open("link/secret.txt").is_err());
    }
}
