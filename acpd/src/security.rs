//! Operator-only pairing creation and a protected, cross-process token digest store.
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use uuid::Uuid;

#[derive(Clone)]
pub struct SecurityStore {
    directory: PathBuf,
}
#[derive(Deserialize, Serialize)]
struct Database {
    host_id: Uuid,
    devices: Vec<Device>,
    pairing: Option<Pairing>,
    failed_attempts: u32,
    blocked_until: u64,
}
impl Default for Database {
    fn default() -> Self {
        Self {
            host_id: Uuid::new_v4(),
            devices: vec![],
            pairing: None,
            failed_attempts: 0,
            blocked_until: 0,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: Uuid,
    pub name: String,
    pub expires_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
}
#[derive(Deserialize, Serialize)]
struct Pairing {
    digest: String,
    expires_at: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingResult {
    pub host_id: Uuid,
    pub device_id: Uuid,
    pub token: String,
    pub expires_at: u64,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random_hex() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| anyhow::anyhow!("secure randomness unavailable"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
fn digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn equal(a: &str, b: &str) -> bool {
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

impl SecurityStore {
    pub fn open(directory: &Path) -> Result<Self> {
        if directory
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            bail!("state directory cannot be a symlink")
        }
        std::fs::create_dir_all(directory).context("create protected state directory")?;
        protect(directory, true)?;
        let store = Self {
            directory: directory.canonicalize()?,
        };
        store.transaction(true, |_| Ok(()))?;
        Ok(store)
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    fn transaction<T>(
        &self,
        save: bool,
        operation: impl FnOnce(&mut Database) -> Result<T>,
    ) -> Result<T> {
        let lock_path = self.directory.join("security.lock");
        reject_symlink(&lock_path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        protect(&lock_path, false)?;
        lock.lock_exclusive()?;
        let path = self.directory.join("security.json");
        reject_symlink(&path)?;
        let mut database = if path.exists() {
            let file = File::open(&path)?;
            if file.metadata()?.len() > 1024 * 1024 {
                bail!("security store exceeds size limit")
            }
            serde_json::from_reader(file).context("invalid security store")?
        } else {
            Database::default()
        };
        let result = operation(&mut database);
        // Failed pairing attempts also commit rate-limit state.
        if save {
            write_private_json(&path, &database)?;
        }
        FileExt::unlock(&lock)?;
        result
    }
    pub fn host_id(&self) -> Result<Uuid> {
        self.transaction(false, |database| Ok(database.host_id))
    }
    pub fn new_pairing_code(&self) -> Result<String> {
        self.new_pairing_code_at(now())
    }
    fn new_pairing_code_at(&self, timestamp: u64) -> Result<String> {
        let random = random_hex()?;
        let code = format!("{}-{}-{}", &random[0..4], &random[4..8], &random[8..12]).to_uppercase();
        self.transaction(true, |database| {
            database.pairing = Some(Pairing {
                digest: digest(&normalize_code(&code)),
                expires_at: timestamp + 60,
            });
            database.failed_attempts = 0;
            database.blocked_until = 0;
            Ok(code)
        })
    }
    pub fn redeem(&self, code: &str, name: &str, lifetime: u64) -> Result<PairingResult> {
        self.redeem_at(code, name, lifetime, now())
    }
    fn redeem_at(
        &self,
        code: &str,
        name: &str,
        lifetime: u64,
        timestamp: u64,
    ) -> Result<PairingResult> {
        if name.trim().is_empty() || name.len() > 80 || code.len() > 32 {
            bail!("invalid pairing input")
        }
        self.transaction(true, |database| {
            if database.blocked_until > timestamp {
                bail!("pairing temporarily rate limited")
            }
            let accepted = database.pairing.as_ref().is_some_and(|pairing| {
                pairing.expires_at > timestamp
                    && equal(&pairing.digest, &digest(&normalize_code(code)))
            });
            if !accepted {
                database.failed_attempts += 1;
                if database.failed_attempts >= 5 {
                    database.blocked_until = timestamp + 60;
                    database.failed_attempts = 0;
                }
                bail!("pairing code invalid or expired")
            }
            database
                .devices
                .retain(|device| device.expires_at > timestamp);
            if database.devices.len() >= 128 {
                bail!("paired device limit reached; revoke a device")
            }
            let token = random_hex()?;
            let device_id = Uuid::new_v4();
            let expires_at = timestamp + lifetime;
            database.devices.push(Device {
                id: device_id,
                name: name.trim().into(),
                expires_at,
                digest: Some(digest(&token)),
            });
            database.pairing = None;
            database.failed_attempts = 0;
            Ok(PairingResult {
                host_id: database.host_id,
                device_id,
                token,
                expires_at,
            })
        })
    }
    pub fn authorize(&self, token: &str) -> Result<Device> {
        self.authorize_at(token, now())
    }
    fn authorize_at(&self, token: &str, timestamp: u64) -> Result<Device> {
        if token.len() != 64 {
            bail!("invalid credentials")
        }
        let token_digest = digest(token);
        self.transaction(false, |database| {
            let mut device = database
                .devices
                .iter()
                .find(|device| {
                    device.expires_at > timestamp
                        && device
                            .digest
                            .as_ref()
                            .is_some_and(|stored| equal(stored, &token_digest))
                })
                .cloned()
                .context("invalid or expired credentials")?;
            device.digest = None;
            Ok(device)
        })
    }
    pub fn devices(&self) -> Result<Vec<Device>> {
        self.transaction(false, |database| {
            Ok(database
                .devices
                .iter()
                .cloned()
                .map(|mut device| {
                    device.digest = None;
                    device
                })
                .collect())
        })
    }
    pub fn revoke(&self, id: Uuid) -> Result<()> {
        self.transaction(true, |database| {
            database.devices.retain(|device| device.id != id);
            Ok(())
        })
    }
}
fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|character| *character != '-' && !character.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect()
}
pub fn reject_symlink(path: &Path) -> Result<()> {
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!("protected state file cannot be a symlink")
    }
    Ok(())
}
pub fn write_private_json(path: &Path, value: &impl Serialize) -> Result<()> {
    reject_symlink(path)?;
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        protect(&temporary, false)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok::<_, anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
pub fn protect(path: &Path, directory: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{
                Authorization::{
                    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                },
                DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
            },
        };
        let sddl = if directory {
            "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"
        } else {
            "D:P(A;;FA;;;OW)(A;;FA;;;SY)"
        };
        let descriptor_text: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
        let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut descriptor = std::ptr::null_mut();
        // The descriptor owns an allocation returned by Win32, released once below.
        unsafe {
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor_text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let applied = SetFileSecurityW(
                wide_path.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            );
            let failure = std::io::Error::last_os_error();
            LocalFree(descriptor);
            if applied == 0 {
                return Err(failure.into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_is_one_use_and_tokens_are_digests_only() {
        let directory = tempfile::tempdir().unwrap();
        let store = SecurityStore::open(directory.path()).unwrap();
        let code = store.new_pairing_code_at(100).unwrap();
        let result = store.redeem_at(&code, "Phone", 600, 110).unwrap();
        assert!(store.redeem_at(&code, "Phone", 600, 110).is_err());
        assert!(store.authorize_at(&result.token, 120).is_ok());
        assert!(store.authorize_at(&result.token, 710).is_err());
        let raw = std::fs::read_to_string(directory.path().join("security.json")).unwrap();
        assert!(!raw.contains(&result.token));
        assert!(!raw.contains(&code));
        store.revoke(result.device_id).unwrap();
        assert!(store.authorize_at(&result.token, 120).is_err());
    }
    #[test]
    fn pairing_expiry_rate_limit_and_cross_process_visibility() {
        let directory = tempfile::tempdir().unwrap();
        let operator = SecurityStore::open(directory.path()).unwrap();
        let server = SecurityStore::open(directory.path()).unwrap();
        let code = operator.new_pairing_code_at(100).unwrap();
        assert!(server.redeem_at(&code, "Phone", 600, 160).is_err());
        let code = operator.new_pairing_code_at(200).unwrap();
        for _ in 0..5 {
            assert!(server.redeem_at("wrong", "Phone", 600, 210).is_err());
        }
        assert!(server.redeem_at(&code, "Phone", 600, 211).is_err());
        let code = operator.new_pairing_code_at(220).unwrap();
        assert!(server.redeem_at(&code, "Phone", 600, 221).is_ok());
    }
    #[test]
    fn concurrent_pairing_exchanges_have_one_winner() {
        let directory = tempfile::tempdir().unwrap();
        let store = SecurityStore::open(directory.path()).unwrap();
        let code = store.new_pairing_code().unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let tasks: Vec<_> = (0..4)
            .map(|_| {
                let store = store.clone();
                let code = code.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store.redeem(&code, "Phone", 600).is_ok()
                })
            })
            .collect();
        assert_eq!(
            tasks
                .into_iter()
                .map(|task| usize::from(task.join().unwrap()))
                .sum::<usize>(),
            1
        );
    }
}
