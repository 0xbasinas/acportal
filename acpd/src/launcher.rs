//! Non-secret operator preferences for short native CLI commands.
use crate::{config::Config, pairing_input, registry::Registry, security::write_private_json};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launcher {
    pub config: PathBuf,
    pub address: String,
    pub name: String,
}
impl Launcher {
    pub fn prepare(config: &Path, address: &str, name: &str) -> Result<Self> {
        // Preserve the selected config's parent: a symlinked file can intentionally
        // resolve relative registry/workspace/state paths against that parent.
        let config = std::path::absolute(config).context("resolve host config path")?;
        let loaded = Config::load(&config)?;
        Registry::load(&loaded.registry)?;
        // Shared presentation validation, including name bounds. No code is created.
        pairing_input::pairing_uri(address, "0000-0000-0000", name)?;
        Ok(Self {
            config,
            address: pairing_input::validate_pairing_address(address)?
                .as_str()
                .trim_end_matches('/')
                .into(),
            name: name.into(),
        })
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if path.exists() && path.canonicalize()? == self.config.canonicalize()? {
            bail!("launcher profile must be separate from the host config")
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        write_private_json(path, self)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(16_385)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16_384 {
            bail!("launcher profile is too large")
        }
        let saved: Self = serde_json::from_slice(&bytes)
            .context("read launcher profile; run acpd setup to repair it")?;
        if !saved.config.is_absolute() {
            bail!("launcher config path must be absolute")
        }
        pairing_input::pairing_uri(&saved.address, "0000-0000-0000", &saved.name)?;
        Ok(saved)
    }
    pub fn matches_config(&self, config: &Path) -> bool {
        // Same file through a different parent can mean different relative state paths.
        std::path::absolute(config).is_ok_and(|p| p == self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saves_reference_without_rewriting_config_or_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let original = "registry = 'agents.json'\nworkspace_roots = ['.']\n";
        std::fs::write(&config, original).unwrap();
        std::fs::write(dir.path().join("agents.json"), "[]").unwrap();
        let profile = Launcher::prepare(&config, "https://host.example/", "Laptop").unwrap();
        let path = dir.path().join("launcher.json");
        profile.save(&path).unwrap();
        let loaded = Launcher::load(&path).unwrap();
        assert!(loaded.matches_config(&config));
        assert!(!loaded.matches_config(&dir.path().join("other.toml")));
        let other_parent = dir.path().join("other");
        std::fs::create_dir(&other_parent).unwrap();
        std::fs::hard_link(&config, other_parent.join("config.toml")).unwrap();
        assert!(!loaded.matches_config(&other_parent.join("config.toml")));
        assert_eq!(loaded.address, "https://host.example");
        assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
        assert!(!dir.path().join("state").exists());
        assert!(profile.save(&config).is_err());
    }
    #[test]
    fn invalid_setup_and_untrusted_profile_fail_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "registry = 'agents.json'\n").unwrap();
        std::fs::write(dir.path().join("agents.json"), "[]").unwrap();
        assert!(Launcher::prepare(&config, "http://host.example", "Laptop").is_err());
        assert!(Launcher::prepare(&config, "https://host.example", "bad\nname").is_err());
        let path = dir.path().join("launcher.json");
        std::fs::write(&path, "x".repeat(16_385)).unwrap();
        assert!(Launcher::load(&path).is_err());
        std::fs::write(
            &path,
            r#"{"config":"relative.toml","address":"https://host.example","name":"Laptop"}"#,
        )
        .unwrap();
        assert!(Launcher::load(&path).is_err());
    }
}
