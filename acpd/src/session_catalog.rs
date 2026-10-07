//! Protected metadata only. Agent processes, prompts and permissions are never restored.
use crate::{
    security::{protect, reject_symlink, write_private_json},
    session::SessionMetadata,
};
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_RECORDS: usize = 256;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Database {
    version: u32,
    sessions: BTreeMap<Uuid, SessionMetadata>,
}

pub(crate) struct SessionCatalog {
    path: PathBuf,
    database: Mutex<Database>,
    // One daemon owns this catalog for its entire lifetime.
    _lease: File,
}

impl SessionCatalog {
    pub fn open(directory: &Path) -> Result<Self> {
        let lock_path = directory.join("sessions.lock");
        reject_symlink(&lock_path)?;
        let lease = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        protect(&lock_path, false)?;
        lease
            .try_lock_exclusive()
            .context("another host owns the session catalog")?;
        let path = directory.join("sessions.json");
        reject_symlink(&path)?;
        let database = if path.exists() {
            let file = File::open(&path)?;
            if file.metadata()?.len() > MAX_BYTES as u64 {
                bail!("session catalog exceeds size limit")
            }
            serde_json::from_reader(file).context("invalid session catalog")?
        } else {
            Database {
                version: 1,
                sessions: BTreeMap::new(),
            }
        };
        if database.version != 1
            || database.sessions.len() > MAX_RECORDS
            || database
                .sessions
                .iter()
                .any(|(id, metadata)| *id != metadata.id)
        {
            bail!("unsupported or invalid session catalog")
        }
        // Repair permissions on existing catalogs as well as new writes.
        if path.exists() {
            protect(&path, false)?;
        }
        Ok(Self {
            path,
            database: Mutex::new(database),
            _lease: lease,
        })
    }
    pub fn records(&self) -> BTreeMap<Uuid, SessionMetadata> {
        self.database.lock().unwrap().sessions.clone()
    }
    fn change(&self, operation: impl FnOnce(&mut Database)) -> Result<()> {
        let mut current = self.database.lock().unwrap();
        let mut next = current.clone();
        operation(&mut next);
        if next.sessions.len() > MAX_RECORDS || serde_json::to_vec(&next)?.len() > MAX_BYTES {
            bail!("session catalog is full; delete a retained session first")
        }
        write_private_json(&self.path, &next)?;
        *current = next;
        Ok(())
    }
    pub fn save(&self, mut metadata: SessionMetadata) -> Result<()> {
        metadata.status = "interrupted".into();
        self.change(|database| {
            database.sessions.insert(metadata.id, metadata);
        })
    }
    pub fn remove(&self, id: Uuid) -> Result<()> {
        self.change(|database| {
            database.sessions.remove(&id);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::SecurityStore;
    use serde_json::json;
    fn record() -> SessionMetadata {
        SessionMetadata {
            id: Uuid::new_v4(),
            agent_id: "mock".into(),
            acp_session_id: "saved-id".into(),
            workspace: PathBuf::from("workspace"),
            initialization: json!({}),
            setup: json!({}),
            status: "ready".into(),
            workspace_access: Default::default(),
        }
    }
    #[test]
    fn catalog_capacity_failure_preserves_records_and_deletion_frees_a_slot() {
        let directory = tempfile::tempdir().unwrap();
        let security = SecurityStore::open(directory.path()).unwrap();
        let records: BTreeMap<_, _> = (0..MAX_RECORDS)
            .map(|_| {
                let metadata = record();
                (metadata.id, metadata)
            })
            .collect();
        let first = *records.keys().next().unwrap();
        let path = security.directory().join("sessions.json");
        write_private_json(
            &path,
            &Database {
                version: 1,
                sessions: records,
            },
        )
        .unwrap();
        let store = SessionCatalog::open(security.directory()).unwrap();
        let previous = std::fs::read(&path).unwrap();
        let additional = record();
        assert!(store.save(additional.clone()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        assert_eq!(store.records().len(), MAX_RECORDS);
        store.remove(first).unwrap();
        store.save(additional.clone()).unwrap();
        drop(store);
        let store = SessionCatalog::open(security.directory()).unwrap();
        assert!(store.records().contains_key(&additional.id));
        assert!(!store.records().contains_key(&first));
    }
    #[test]
    fn oversized_catalog_is_rejected_before_deserialization() {
        let directory = tempfile::tempdir().unwrap();
        let security = SecurityStore::open(directory.path()).unwrap();
        let path = security.directory().join("sessions.json");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_BYTES as u64 + 1).unwrap();
        drop(file);
        assert!(SessionCatalog::open(security.directory()).is_err());
    }
}
