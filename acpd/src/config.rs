use crate::shell_review::ShellReviewConfig;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// Opt-in automatic review of agent shell lines, keyed by agent id. Absent = off:
    /// every shell line needs explicit phone approval.
    pub shell_review: BTreeMap<String, ShellReviewConfig>,
    /// Experimental agent-to-agent delegation. Off by default.
    pub orchestrator: OrchestratorConfig,
}

/// Experimental delegation between agents (see `docs/orchestrator-design.md`). Changes apply to
/// sessions and tasks started after a reload; running sessions keep what they were launched with.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OrchestratorConfig {
    pub enabled: bool,
    /// Agent ids whose sessions get the delegation tools (an MCP server added to `session/new`).
    pub orchestrate: Vec<String>,
    /// Agent ids that other agents may delegate to.
    pub delegable: Vec<String>,
    pub max_depth: u8,
    pub max_children_per_task: usize,
    pub max_tasks_total: usize,
    /// Applied when a delegation names no deadline.
    pub default_deadline_seconds: u64,
    /// Highest risk a supervising agent may approve for its sub-tasks. `none` leaves every
    /// permission request to the phone; `high` is not allowed.
    pub supervisor_max_risk: SupervisorRisk,
    /// How long after a request appears the supervisor may still answer it. After that only the
    /// phone can.
    pub supervisor_timeout_seconds: u64,
    /// Largest edit (bytes) still rated medium risk.
    pub max_edit_bytes: usize,
    /// Agent ids that paired peer hosts may use through `/acp/{agent_id}`. Empty: none.
    pub peer_agents: Vec<String>,
    /// Workspaces peers may name, by alias. A peer never supplies or sees a path.
    pub peer_workspaces: BTreeMap<String, PathBuf>,
    /// The most a peer's sessions may be allowed to do on this host. A peer may ask for less.
    pub peer_rights: PeerRights,
    /// How long a bridge session survives its socket, so the peer can reattach with
    /// `session/load`. 0 removes it at once.
    pub bridge_resume_seconds: u64,
    /// Interval between WebSocket pings and credential re-checks on a bridge connection.
    pub bridge_ping_seconds: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PeerRights {
    pub read: bool,
    pub write: bool,
    pub terminal: bool,
}
impl Default for PeerRights {
    fn default() -> Self {
        Self {
            read: true,
            write: false,
            terminal: false,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SupervisorRisk {
    None,
    Low,
    #[default]
    Medium,
}
impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            orchestrate: vec![],
            delegable: vec![],
            max_depth: 2,
            max_children_per_task: 4,
            max_tasks_total: 8,
            default_deadline_seconds: 1800,
            supervisor_max_risk: SupervisorRisk::default(),
            supervisor_timeout_seconds: 60,
            max_edit_bytes: 256 * 1024,
            peer_agents: vec![],
            peer_workspaces: BTreeMap::new(),
            peer_rights: PeerRights::default(),
            bridge_resume_seconds: 120,
            bridge_ping_seconds: 20,
        }
    }
}
impl OrchestratorConfig {
    fn validate(&self) -> Result<()> {
        if !(1..=5).contains(&self.max_depth)
            || !(1..=16).contains(&self.max_children_per_task)
            || !(1..=64).contains(&self.max_tasks_total)
            || !(1..=86_400).contains(&self.default_deadline_seconds)
            || !(1..=600).contains(&self.supervisor_timeout_seconds)
            || !(1024..=16 * 1024 * 1024).contains(&self.max_edit_bytes)
        {
            bail!("invalid orchestrator limits")
        }
        if self.orchestrate.len() > 64 || self.delegable.len() > 64 || self.peer_agents.len() > 64 {
            bail!("too many orchestrator agents")
        }
        if self.bridge_resume_seconds > 3600 || !(5..=120).contains(&self.bridge_ping_seconds) {
            bail!("invalid bridge timers")
        }
        if self.peer_workspaces.len() > 32
            || self.peer_workspaces.keys().any(|alias| {
                alias.is_empty()
                    || alias.len() > 32
                    || !alias.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                    })
            })
        {
            bail!("peer workspace aliases must be 1..32 characters of a-z, 0-9, '-' or '_'")
        }
        Ok(())
    }
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
            state_directory: default_state_directory(),
            logging: LoggingConfig::default(),
            shell_review: BTreeMap::new(),
            orchestrator: OrchestratorConfig::default(),
        }
    }
}
/// Where the host keeps its settings (`config.toml`, `agents.json`, `launcher.json`), in the
/// standard per-user place: `%APPDATA%\acpd` on Windows, `~/Library/Application Support/acpd` on
/// macOS and `~/.config/acpd` on Linux. `XDG_CONFIG_HOME` overrides this on every system.
pub fn default_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("acpd");
    }
    directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().join("acpd"))
        .unwrap_or_else(|| PathBuf::from(".config/acpd"))
}
/// Where the host keeps machine-local data (paired devices, session metadata): `%LOCALAPPDATA%\acpd\state`
/// on Windows (not the roaming profile), `~/Library/Application Support/acpd/state` on macOS and
/// `~/.local/share/acpd/state` on Linux. `XDG_DATA_HOME` overrides this on every system.
pub fn default_data_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("acpd");
    }
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_local_dir().join("acpd"))
        .unwrap_or_else(|| PathBuf::from(".local/share/acpd"))
}
/// The state directory used when a config does not name one. An existing `state` folder next to the
/// config (where earlier versions kept it) is still used, so paired devices are not lost.
pub fn default_state_directory() -> PathBuf {
    state_directory_for(&default_directory(), &default_data_directory())
}
fn state_directory_for(config_directory: &Path, data_directory: &Path) -> PathBuf {
    let legacy = config_directory.join("state");
    if legacy.is_dir() {
        legacy
    } else {
        data_directory.join("state")
    }
}
/// Starter `config.toml`: every setting is its default, so it holds only comments to edit.
const STARTER_CONFIG: &str = concat!(
    "# ACP Portal host settings. Everything here is optional; the values shown are the defaults.\n",
    "# Folders and agents can be added from the phone app, so nothing is required here.\n",
    "#\n",
    "# [server]\n",
    "# listen = \"127.0.0.1:8765\"   # publish it to your phone with: tailscale serve --bg 8765\n",
    "#\n",
    "# workspace_roots = []         # folders agents may use; the phone can add more\n",
    "\n",
    "# Agent definitions, next to this file. The phone can add agents too.\n",
    "registry = \"agents.json\"\n",
);
/// Creates the standard config folder with a starter `config.toml` and an empty `agents.json`
/// when they are missing. Existing files are never changed. Returns the config file path.
pub fn bootstrap_default_config(directory: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(directory).context("create the acpd settings folder")?;
    let config = directory.join("config.toml");
    if !config.exists() {
        std::fs::write(&config, STARTER_CONFIG).context("write the starter config")?;
    }
    let registry = directory.join("agents.json");
    if !registry.exists() {
        std::fs::write(&registry, "[]\n").context("write the empty agent list")?;
    }
    Ok(config)
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
        self.orchestrator.validate()?;
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
        for (agent, review) in &self.shell_review {
            if agent.is_empty()
                || agent.len() > 64
                || !agent
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                bail!("shell_review keys must be agent ids")
            }
            review
                .validate()
                .with_context(|| format!("shell_review.{agent}"))?;
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
    fn state_goes_to_the_data_folder_unless_an_older_one_exists() {
        let config = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        assert_eq!(
            state_directory_for(config.path(), data.path()),
            data.path().join("state")
        );
        std::fs::create_dir(config.path().join("state")).unwrap();
        assert_eq!(
            state_directory_for(config.path(), data.path()),
            config.path().join("state")
        );
    }
    #[test]
    fn bootstrap_creates_a_loadable_default_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("nested").join("acpd");
        let file = bootstrap_default_config(&settings).unwrap();
        assert_eq!(file, settings.join("config.toml"));
        let config = Config::load(&file).unwrap();
        assert_eq!(config.registry, settings.join("agents.json"));
        assert!(config.workspace_roots.is_empty());
        assert_eq!(
            std::fs::read_to_string(settings.join("agents.json"))
                .unwrap()
                .trim(),
            "[]"
        );
        std::fs::write(&file, "workspace_roots = []\n# mine\n").unwrap();
        std::fs::write(settings.join("agents.json"), "[ ]").unwrap();
        bootstrap_default_config(&settings).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("# mine"));
        assert_eq!(
            std::fs::read_to_string(settings.join("agents.json")).unwrap(),
            "[ ]"
        );
    }
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
    fn shell_review_is_off_by_default_and_parses_per_agent() {
        assert!(Config::default().shell_review.is_empty());
        let config: Config = toml::from_str(
            "[shell_review.goose]\nrules = true\n[shell_review.goose.model]\nbase_url = 'https://api.deepseek.com'\nmodel = 'deepseek-flash'\napi_key_env = 'DEEPSEEK_API_KEY'\n",
        )
        .unwrap();
        config.validate().unwrap();
        let goose = &config.shell_review["goose"];
        assert!(goose.rules);
        let model = goose.model.as_ref().unwrap();
        assert_eq!(
            (
                model.timeout_seconds,
                model.max_requests_per_minute,
                model.max_command_bytes
            ),
            (10, 6, 2048)
        );
        let retired: Config = toml::from_str(
            "[shell_review.goose]\nrules = true\n[shell_review.goose.model]\nbase_url = 'https://api.deepseek.com'\nmodel = 'deepseek-chat'\napi_key_env = 'K'\n",
        )
        .unwrap();
        assert!(retired.validate().is_err());
        assert!(toml::from_str::<Config>("[shell_review.goose]\nauto = true\n").is_err());
        let off: Config = toml::from_str("[shell_review.goose]\n").unwrap();
        assert!(!off.shell_review["goose"].rules);
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
