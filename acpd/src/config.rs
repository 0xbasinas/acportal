use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub registry: PathBuf,
    pub workspace_roots: Vec<PathBuf>,
    pub runtime: RuntimeConfig,
    pub server: ServerConfig,
    pub state_directory: PathBuf,
    pub logging: LoggingConfig,
}

/// Optional bounded file sink for host lifecycle logs. Stderr output is unchanged.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    /// Active log file. Absent keeps logging on stderr only.
    pub file: Option<PathBuf>,
    /// Size at which the active file rotates.
    pub max_file_bytes: u64,
    /// Rotated files kept beside the active file (`name.1` is newest).
    pub retained_files: u32,
    /// Log per-frame ACP metadata (direction, kind, sanitized method, error code, size) at
    /// debug level. Never logs frame content.
    pub frame_metadata: bool,
}
impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            file: None,
            max_file_bytes: 4 * 1024 * 1024,
            retained_files: 3,
            frame_metadata: false,
        }
    }
}
impl LoggingConfig {
    /// Maximum bytes the active plus retained files can occupy.
    pub fn disk_budget(&self) -> u64 {
        self.max_file_bytes * (u64::from(self.retained_files) + 1)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub listen: std::net::SocketAddr,
    pub tls_certificate: Option<PathBuf>,
    pub tls_private_key: Option<PathBuf>,
    pub token_lifetime_seconds: u64,
}
impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:8765".parse().unwrap(),
            tls_certificate: None,
            tls_private_key: None,
            token_lifetime_seconds: 30 * 24 * 3600,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfig {
    pub request_timeout_seconds: u64,
    pub max_frame_bytes: usize,
    pub max_sessions: usize,
    pub history_events: usize,
    pub history_bytes: usize,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            request_timeout_seconds: 120,
            max_frame_bytes: 1024 * 1024,
            max_sessions: 8,
            history_events: 2048,
            history_bytes: 8 * 1024 * 1024,
        }
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            registry: default_directory().join("agents.json"),
            workspace_roots: vec![],
            runtime: RuntimeConfig::default(),
            server: ServerConfig::default(),
            state_directory: default_directory().join("state"),
            logging: LoggingConfig::default(),
        }
    }
}
pub fn default_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("acpd");
    }
    directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().join("acpd"))
        .unwrap_or_else(|| PathBuf::from(".config/acpd"))
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read configuration {}", path.display()))?;
        let mut config: Self = toml::from_str(&raw).context("parse configuration TOML")?;
        let parent = path.parent().unwrap_or(Path::new("."));
        if config.registry.is_relative() {
            config.registry = parent.join(&config.registry);
        }
        for root in &mut config.workspace_roots {
            if root.is_relative() {
                *root = parent.join(&*root);
            }
        }
        if config.state_directory.is_relative() {
            config.state_directory = parent.join(&config.state_directory);
        }
        for path in [
            &mut config.server.tls_certificate,
            &mut config.server.tls_private_key,
            &mut config.logging.file,
        ]
        .into_iter()
        .flatten()
        {
            if path.is_relative() {
                *path = parent.join(&*path);
            }
        }
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        if self.server.tls_certificate.is_some() != self.server.tls_private_key.is_some() {
            bail!("TLS certificate and private key must be configured together")
        }
        if !self.server.listen.ip().is_loopback() && self.server.tls_certificate.is_none() {
            bail!("remote listeners require TLS")
        }
        if !(60..=365 * 24 * 3600).contains(&self.server.token_lifetime_seconds) {
            bail!("token lifetime must be 60 seconds..365 days")
        }
        let runtime = &self.runtime;
        if !(1..=3600).contains(&runtime.request_timeout_seconds) {
            bail!("request timeout must be 1..3600 seconds")
        }
        if !(1024..=16 * 1024 * 1024).contains(&runtime.max_frame_bytes) {
            bail!("frame limit must be 1 KiB..16 MiB")
        }
        if !(1..=128).contains(&runtime.max_sessions) {
            bail!("session limit must be 1..128")
        }
        if !(1..=100_000).contains(&runtime.history_events)
            || runtime.history_bytes < runtime.max_frame_bytes
            || runtime.history_bytes > 256 * 1024 * 1024
        {
            bail!("invalid history limits")
        }
        let logging = &self.logging;
        if !(64 * 1024..=64 * 1024 * 1024).contains(&logging.max_file_bytes) {
            bail!("log file size must be 64 KiB..64 MiB")
        }
        if !(1..=16).contains(&logging.retained_files) {
            bail!("retained log files must be 1..16")
        }
        if logging
            .file
            .as_ref()
            .is_some_and(|file| file.file_name().is_none())
        {
            bail!("log file must name a file")
        }
        Ok(())
    }
    pub fn workspace(&self, requested: &Path) -> Result<PathBuf> {
        let path = requested
            .canonicalize()
            .context("workspace does not exist or is inaccessible")?;
        if !path.is_dir() {
            bail!("workspace must be a directory")
        }
        for root in &self.workspace_roots {
            let root = root
                .canonicalize()
                .context("configured workspace root is inaccessible")?;
            if path.starts_with(root) {
                return Ok(path);
            }
        }
        bail!("workspace is outside configured workspace_roots; configure an explicit root")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn denies_unconfigured_and_outside_workspaces() {
        let allowed = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        assert!(config.workspace(allowed.path()).is_err());
        config.workspace_roots.push(allowed.path().into());
        assert!(config.workspace(allowed.path()).is_ok());
        assert!(config.workspace(outside.path()).is_err());
    }
    #[test]
    fn relative_paths_resolve_against_config_not_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.toml");
        std::fs::write(
            &file,
            "registry = 'agents.json'\nworkspace_roots = ['.']\n[logging]\nfile = 'logs/acpd.log'\nframe_metadata = true\n",
        )
        .unwrap();
        let config = Config::load(&file).unwrap();
        assert_eq!(config.registry, dir.path().join("agents.json"));
        assert_eq!(
            config.logging.file,
            Some(dir.path().join("logs").join("acpd.log"))
        );
        assert!(config.logging.frame_metadata);
        assert_eq!(
            config.workspace(dir.path()).unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }
    #[test]
    fn catches_invalid_limits_and_unknown_keys() {
        let mut config = Config::default();
        config.runtime.max_sessions = 0;
        assert!(config.validate().is_err());
        let mut config = Config::default();
        config.logging.max_file_bytes = 1024;
        assert!(config.validate().is_err());
        let mut config = Config::default();
        config.logging.retained_files = 0;
        assert!(config.validate().is_err());
        let mut config = Config::default();
        config.logging.file = Some(PathBuf::from(".."));
        assert!(config.validate().is_err());
        assert!(toml::from_str::<Config>("typo = true").is_err());
    }
}
