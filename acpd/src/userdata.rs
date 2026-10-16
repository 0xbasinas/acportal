//! Folders and agent CLIs the paired phone adds. Kept apart from the hand-written config and agent
//! registry (which stay untouched and keep their comments) in a protected file in the state directory.
use crate::{
    config::Config,
    registry::{AgentDefinition, Registry},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

const FILE: &str = "user-settings.json";

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct UserData {
    pub workspace_roots: Vec<PathBuf>,
    pub agents: Vec<AgentDefinition>,
}

/// A path without the Windows verbatim prefix (`\?\C:\x` becomes `C:\x`), for display and storage.
/// Other platforms and paths are returned unchanged.
pub fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\")
        && rest.len() >= 2
        && rest.as_bytes()[1] == b':'
    {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

/// Replace `state_directory/file` atomically through a unique owner-only temporary file.
pub(crate) fn save_private(state_directory: &Path, file_name: &str, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(state_directory).context("create state directory")?;
    let target = state_directory.join(file_name);
    let temporary = state_directory.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let result = (|| -> Result<()> {
        let mut file = options.open(&temporary).context("create settings file")?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, &target).context("replace settings file")
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

impl UserData {
    pub fn load(state_directory: &Path) -> Result<Self> {
        let path = state_directory.join(FILE);
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&std::fs::read(&path).context("read user settings")?)
            .context("parse user settings")
    }
    /// Atomic replace through a unique owner-only temporary file in the same directory.
    pub fn save(&self, state_directory: &Path) -> Result<()> {
        save_private(state_directory, FILE, &serde_json::to_vec_pretty(self)?)
    }
    /// Adds the phone-added folders and agents. A saved agent that no longer validates is skipped
    /// with a warning so one bad entry cannot stop the host starting.
    pub fn apply(&self, config: &mut Config, registry: &mut Registry) {
        for root in &self.workspace_roots {
            if !config.workspace_roots.contains(root) {
                config.workspace_roots.push(root.clone());
            }
        }
        for agent in &self.agents {
            if let Err(error) = registry.upsert(agent.clone()) {
                tracing::warn!(agent = %agent.id, %error, "skipping invalid phone-added agent");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verbatim_prefix_is_removed_only_for_windows_forms() {
        // Compared as text: on Windows `\\?\C:\x` and `C:\x` are different path prefixes.
        let text = |path: PathBuf| path.to_string_lossy().into_owned();
        assert_eq!(
            text(plain_path(Path::new(r"\\?\C:\Users\me"))),
            r"C:\Users\me"
        );
        assert_eq!(
            text(plain_path(Path::new(r"\\?\UNC\srv\share"))),
            r"\\srv\share"
        );
        assert_eq!(plain_path(Path::new("/home/me")), Path::new("/home/me"));
        assert_eq!(plain_path(Path::new(r"C:\x")), Path::new(r"C:\x"));
    }
    #[test]
    fn save_and_load_round_trip_and_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            UserData::load(dir.path())
                .unwrap()
                .workspace_roots
                .is_empty()
        );
        let data = UserData {
            workspace_roots: vec![dir.path().to_path_buf()],
            agents: vec![],
        };
        data.save(dir.path()).unwrap();
        data.save(dir.path()).unwrap();
        assert_eq!(
            UserData::load(dir.path()).unwrap().workspace_roots,
            data.workspace_roots
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temporary file is left"
        );
    }
}
