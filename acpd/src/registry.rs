use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    /// The program to run. Empty for a remote (`ws`) agent, which runs on another host.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "stdio")]
    pub transport: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub working_directory: Option<PathBuf>,
    #[serde(default)]
    pub icon: Option<String>,
    /// Set when this agent is known to edit files or run commands with its own built-in
    /// tools instead of ACP client callbacks. Those actions bypass host review, so the
    /// phone shows a warning. The host cannot detect this; the registry owner declares it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub own_tools: bool,
    /// `auto` (default) runs host-approved terminal commands in an OS sandbox when the host
    /// supports one (bubblewrap on Linux): only the workspace is writable and the home
    /// directory is hidden. `off` runs them unconfined.
    #[serde(default, skip_serializing_if = "Sandbox::is_auto")]
    pub sandbox: Sandbox,
    /// Windows hosts only: shell for this agent's host shell lines (`auto` = host setting).
    #[serde(default, skip_serializing_if = "crate::shell::ShellChoice::is_auto")]
    pub shell: crate::shell::ShellChoice,
    /// Remote (`ws`) agents only: the other host's bridge, `wss://host:port/acp/<agent>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Remote agents only: the name of the peer record (see `acpd peer`) that holds the
    /// credential. The registry never carries a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
    /// Remote agents only: the workspace alias the other host maps to a folder of its own. A
    /// path is never sent to an acpd peer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_alias: Option<String>,
    /// Remote agents only, for a server that is not acpd: the absolute folder on that server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_cwd: Option<String>,
    /// Another registered agent this entry is an account of. The target must exist, must not
    /// itself be a profile, and must use the same command (and the same url when remote).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_of: Option<String>,
    /// Short name for this account, shown under the base agent. The registry `name` stays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_label: Option<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sandbox {
    #[default]
    Auto,
    Off,
}
impl Sandbox {
    pub fn is_auto(&self) -> bool {
        *self == Sandbox::Auto
    }
}
impl AgentDefinition {
    pub fn is_remote(&self) -> bool {
        self.transport == "ws"
    }
}
/// Whether `path` is absolute on *some* common operating system. A remote folder follows the
/// other host's conventions, not this computer's: `/srv/work` is absolute on a Linux peer even
/// when this host (Windows) would call it relative.
fn looks_absolute(path: &str) -> bool {
    if path.is_empty() || path.contains('\0') || path.len() > 4096 {
        return false;
    }
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}
fn agent_id_ok(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn stdio() -> String {
    "stdio".into()
}
fn enabled() -> bool {
    true
}

#[derive(Clone)]
pub struct Registry {
    agents: BTreeMap<String, AgentDefinition>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Available,
    Missing,
    Misconfigured,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub installed: bool,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub status: AgentStatus,
    pub reason: Option<String>,
    pub own_tools: bool,
    /// Set when this entry is an account of another agent (`profileOf` in the registry).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_of: Option<String>,
    /// Account name shown under the base agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_label: Option<String>,
    /// Whether host-approved commands run in the OS sandbox (`auto`) or unconfined (`off`).
    pub sandbox: Sandbox,
    /// How to start it (an npm shim becomes node plus its script). Never sent to the phone.
    #[serde(skip)]
    pub launch: Option<crate::discovery::Launch>,
}
impl AgentDefinition {
    pub fn validate(&self) -> Result<()> {
        if !agent_id_ok(&self.id) {
            bail!("agent id must contain 1..64 ASCII letters, digits, '-' or '_'")
        }
        if self.name.trim().is_empty() {
            bail!("a name is required")
        }
        if let Some(target) = &self.profile_of {
            if !agent_id_ok(target) {
                bail!("profileOf must be an agent id")
            }
            if target == &self.id {
                bail!("an agent cannot be a profile of itself")
            }
        }
        if let Some(label) = &self.profile_label {
            let count = label.chars().count();
            if count == 0 || count > 64 || label.chars().any(char::is_control) {
                bail!("profileLabel must be 1..64 characters without control characters")
            }
        }
        match self.transport.as_str() {
            "stdio" => {
                if self.command.trim().is_empty() {
                    bail!("name and command are required")
                }
                if self.url.is_some()
                    || self.peer.is_some()
                    || self.workspace_alias.is_some()
                    || self.remote_cwd.is_some()
                {
                    bail!("url, peer and workspace fields belong to remote (ws) agents only")
                }
            }
            "ws" => return self.validate_remote(),
            _ => bail!("agent transport must be stdio or ws"),
        }
        if self.command.contains('\0') || self.args.iter().any(|arg| arg.contains('\0')) {
            bail!("command arguments cannot contain NUL")
        }
        if self
            .env
            .iter()
            .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
        {
            bail!("invalid environment entry")
        }
        if let Some(cwd) = &self.working_directory
            && (!cwd.is_absolute() || !cwd.is_dir())
        {
            bail!("workingDirectory must be an existing absolute directory")
        }
        Ok(())
    }
    /// A remote agent runs on another host: no program here, a bridge URL, a peer that holds
    /// the credential and a workspace the other host will map.
    fn validate_remote(&self) -> Result<()> {
        if !self.command.is_empty()
            || !self.args.is_empty()
            || !self.env.is_empty()
            || self.working_directory.is_some()
        {
            bail!("a remote agent cannot have a command, arguments, environment or folder")
        }
        let url = self.url.as_deref().context("a remote agent needs a url")?;
        let origin = crate::peers::origin_of(url).context("invalid remote agent url")?;
        let path = url
            .parse::<axum::http::Uri>()
            .map(|uri| uri.path().to_owned())
            .unwrap_or_default();
        if !path.starts_with("/acp/") || path.len() <= "/acp/".len() || path[5..].contains('/') {
            bail!("the url path must be /acp/<agent id> on the other host")
        }
        let _ = origin;
        let peer = self
            .peer
            .as_deref()
            .context("a remote agent needs a peer")?;
        if !crate::peers::valid_name(peer) {
            bail!("invalid peer name")
        }
        match (&self.workspace_alias, &self.remote_cwd) {
            (Some(alias), None) if crate::peers::valid_name(alias) => {}
            (None, Some(cwd)) if looks_absolute(cwd) => {}
            (Some(_), Some(_)) => bail!("use workspaceAlias or remoteCwd, not both"),
            _ => bail!("a remote agent needs a workspaceAlias (or an absolute remoteCwd)"),
        }
        Ok(())
    }
    pub fn discover(&self) -> Discovery {
        let validation = self.validate().err().map(|error| error.to_string());
        if self.is_remote() {
            // Nothing to find on this computer. Reachability is only known when connecting.
            return Discovery {
                id: self.id.clone(),
                name: self.name.clone(),
                enabled: self.enabled,
                installed: validation.is_none(),
                executable: None,
                version: None,
                status: if validation.is_some() {
                    AgentStatus::Misconfigured
                } else {
                    AgentStatus::Available
                },
                reason: validation,
                own_tools: self.own_tools,
                profile_of: self.profile_of.clone(),
                profile_label: self.profile_label.clone(),
                sandbox: self.sandbox,
                launch: None,
            };
        }
        let launch =
            crate::discovery::resolve(&self.command, self.env.get("PATH").map(String::as_str));
        let executable = launch.as_ref().map(|launch| launch.found.clone());
        let status = if validation.is_some() {
            AgentStatus::Misconfigured
        } else if executable.is_some() {
            AgentStatus::Available
        } else {
            AgentStatus::Missing
        };
        Discovery {
            id: self.id.clone(),
            name: self.name.clone(),
            enabled: self.enabled,
            installed: executable.is_some(),
            executable,
            version: None,
            status,
            reason: validation,
            own_tools: self.own_tools,
            profile_of: self.profile_of.clone(),
            profile_label: self.profile_label.clone(),
            sandbox: self.sandbox,
            launch,
        }
    }
}
fn read_definitions(raw: &str) -> Result<BTreeMap<String, AgentDefinition>> {
    let definitions: Vec<AgentDefinition> =
        serde_json::from_str(raw).context("registry must be a JSON array of agent definitions")?;
    let mut agents = BTreeMap::new();
    for definition in definitions {
        definition
            .validate()
            .with_context(|| format!("invalid agent {}", definition.id))?;
        if agents.insert(definition.id.clone(), definition).is_some() {
            bail!("duplicate agent id")
        }
    }
    Ok(agents)
}
impl Registry {
    pub fn parse(raw: &str) -> Result<Self> {
        let agents = read_definitions(raw)?;
        let registry = Self { agents };
        registry.validate_profiles()?;
        Ok(registry)
    }
    /// A profile names an agent in this registry, one step deep, with the same command.
    /// A remote profile also keeps that agent's url. Checked after the entries are combined,
    /// so a file can profile an agent that arrived from an earlier file.
    fn validate_profiles(&self) -> Result<()> {
        for agent in self.agents.values() {
            let Some(target_id) = &agent.profile_of else {
                continue;
            };
            let Some(target) = self.agents.get(target_id) else {
                bail!("{} profiles unknown agent {target_id}", agent.id)
            };
            if target.profile_of.is_some() {
                bail!(
                    "{} profiles {target_id}, which is itself a profile",
                    agent.id
                )
            }
            if agent.command != target.command {
                bail!("{} must use the same command as {target_id}", agent.id)
            }
            if agent.url != target.url {
                bail!("{} must use the same url as {target_id}", agent.id)
            }
        }
        Ok(())
    }
    /// Entries from a later file replace the same id, then profiles are checked again.
    /// A failed check leaves this registry as it was.
    fn merge_raw(&mut self, raw: &str) -> Result<()> {
        let extra = read_definitions(raw)?;
        let previous = self.agents.clone();
        self.agents.extend(extra);
        if let Err(error) = self.validate_profiles() {
            self.agents = previous;
            return Err(error);
        }
        Ok(())
    }
    /// Agent CLIs this host knows how to start in ACP mode, in display order (`registry/known.json`,
    /// following the public ACP agent registry). These are suggestions for the Add agent forms and
    /// are never registered or launched on their own.
    pub fn known() -> Vec<AgentDefinition> {
        let known: Vec<AgentDefinition> =
            serde_json::from_str(include_str!("../registry/known.json"))
                .expect("registry/known.json is a JSON array of agent definitions");
        known
    }
    pub fn load(path: &Path) -> Result<Self> {
        let mut registry = Self::parse(include_str!("../registry/builtin.json"))?;
        if !path.exists() {
            return Ok(registry);
        }
        registry
            .merge_raw(&std::fs::read_to_string(path).context("read agent registry")?)
            .context("agent registry")?;
        Ok(registry)
    }
    /// Adds or replaces one agent after the same validation a registry file gets.
    /// A profile that does not fit is not kept.
    pub fn upsert(&mut self, definition: AgentDefinition) -> Result<()> {
        definition.validate()?;
        let id = definition.id.clone();
        let previous = self.agents.insert(id.clone(), definition);
        if let Err(error) = self.validate_profiles() {
            match previous {
                Some(previous) => {
                    self.agents.insert(id, previous);
                }
                None => {
                    self.agents.remove(&id);
                }
            }
            return Err(error);
        }
        Ok(())
    }
    pub fn contains(&self, id: &str) -> bool {
        self.agents.contains_key(id)
    }
    /// Whether an agent with this id, or one that starts this command (by name or by a path
    /// ending in it, `.exe`/`.cmd` ignored), is already registered.
    pub fn covers(&self, id: &str, command: &str) -> bool {
        self.contains(id)
            || self.agents.values().any(|agent| {
                Path::new(&agent.command)
                    .file_stem()
                    .is_some_and(|stem| stem.eq_ignore_ascii_case(command))
            })
    }
    /// Removes one agent. Refuses when another entry is a profile of it, and leaves the
    /// registry unchanged in that case.
    pub fn remove(&mut self, id: &str) -> Result<bool> {
        let Some(previous) = self.agents.remove(id) else {
            return Ok(false);
        };
        if let Err(error) = self.validate_profiles() {
            self.agents.insert(id.to_owned(), previous);
            return Err(error);
        }
        Ok(true)
    }
    pub fn get(&self, id: &str) -> Result<&AgentDefinition> {
        self.agents
            .get(id)
            .with_context(|| format!("unknown agent {id}"))
    }
    pub fn discover(&self) -> Vec<Discovery> {
        self.agents
            .values()
            .map(AgentDefinition::discover)
            .collect()
    }
}

pub fn resolve_executable(command: &str, custom_path: Option<&str>) -> Option<PathBuf> {
    let path = Path::new(command);
    // Relative executable paths containing separators are ambiguous across workspaces.
    if path.is_absolute() {
        return executable(path).then(|| path.to_path_buf());
    }
    if command.contains(['/', '\\']) {
        return None;
    }
    let path_value = custom_path
        .map(std::ffi::OsString::from)
        .or_else(|| std::env::var_os("PATH"))?;
    let mut names = vec![command.to_string()];
    #[cfg(windows)]
    {
        let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.COM;.BAT;.CMD".into());
        if path.extension().is_none() {
            names.extend(
                extensions
                    .split(';')
                    .filter(|ext| !ext.is_empty())
                    .map(|ext| format!("{command}{ext}")),
            );
        }
    }
    #[cfg(not(windows))]
    let _ = &mut names;
    let mut visited = HashSet::new();
    for directory in std::env::split_paths(&path_value) {
        if !directory.is_absolute() || !visited.insert(directory.clone()) {
            continue;
        }
        for name in &names {
            let candidate = directory.join(name);
            if executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}
pub(crate) fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    #[cfg(windows)]
    {
        // Batch launch requires cmd.exe shell parsing. Report missing, never interpolate.
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("com"))
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shipped_example_registries_parse_and_templates_stay_disabled() {
        let goose = Registry::parse(include_str!("../../examples/agents.json")).unwrap();
        assert!(goose.get("goose").unwrap().enabled);
        let custom = Registry::parse(include_str!("../../examples/agents.custom.json")).unwrap();
        assert!(custom.agents.len() >= 4);
        // Templates are placeholders; an operator enables only what they installed.
        assert!(custom.agents.values().all(|agent| !agent.enabled));
        let opencode = custom.get("opencode").unwrap();
        assert_eq!(opencode.args, ["acp"]);
        // OpenCode edits and runs commands with its own tools (verified with 1.18.35).
        assert!(opencode.own_tools && opencode.discover().own_tools);
        assert!(!custom.get("custom-native").unwrap().own_tools);
        let gemini = custom.get("gemini-cli-node").unwrap();
        assert_eq!(gemini.args.last().map(String::as_str), Some("--acp"));
    }
    #[test]
    fn known_agents_are_valid_unique_and_never_registered() {
        let known = Registry::known();
        assert!(!known.is_empty());
        let ids: HashSet<&str> = known.iter().map(|agent| agent.id.as_str()).collect();
        assert_eq!(ids.len(), known.len());
        for agent in &known {
            agent.validate().unwrap();
            assert!(
                !agent.command.contains(['/', '\\']),
                "{} is a bare command",
                agent.id
            );
        }
        // Suggestions only: nothing is built in.
        assert!(
            Registry::parse(include_str!("../registry/builtin.json"))
                .unwrap()
                .agents
                .is_empty()
        );
    }
    #[test]
    fn arbitrary_custom_agents_and_literal_arguments() {
        let registry = Registry::parse(r#"[{"id":"future","name":"Future","command":"future-agent","args":["$(touch hacked)","; rm -rf /"],"env":{"KEY":"secret"},"icon":"future.svg"}]"#).unwrap();
        assert_eq!(registry.get("future").unwrap().args[0], "$(touch hacked)");
        assert_eq!(registry.discover()[0].status, AgentStatus::Missing);
        // The flag defaults off, is reported as `ownTools` and is omitted when off.
        let discovered = serde_json::to_value(&registry.discover()[0]).unwrap();
        assert_eq!(discovered["ownTools"], false);
        let saved = serde_json::to_value(registry.get("future").unwrap()).unwrap();
        assert!(saved.get("ownTools").is_none());
        let flagged =
            Registry::parse(r#"[{"id":"own","name":"Own","command":"own","ownTools":true}]"#)
                .unwrap();
        assert_eq!(
            serde_json::to_value(&flagged.discover()[0]).unwrap()["ownTools"],
            true
        );
    }
    #[test]
    fn rejects_duplicate_and_invalid_definitions() {
        for raw in [
            r#"[{"id":"x","name":"X","command":"x"},{"id":"x","name":"Y","command":"y"}]"#,
            r#"[{"id":"../x","name":"X","command":"x"}]"#,
            r#"[{"id":"x","name":"X","command":"x","transport":"http"}]"#,
            r#"[{"id":"x","name":"X","command":"x","env":{"BAD=KEY":"x"}}]"#,
        ] {
            assert!(Registry::parse(raw).is_err());
        }
    }
    #[test]
    fn explicit_binary_and_custom_path_discovery() {
        let binary = std::env::current_exe().unwrap();
        assert_eq!(
            resolve_executable(binary.to_str().unwrap(), None),
            Some(binary.clone())
        );
        assert!(resolve_executable("./untrusted", None).is_none());
        assert_eq!(
            resolve_executable(
                binary.file_name().unwrap().to_str().unwrap(),
                binary.parent().unwrap().to_str()
            ),
            Some(binary)
        );
    }
    #[test]
    fn profiles_require_one_step_and_the_same_command() {
        let ok = Registry::parse(
            r#"[{"id":"goose","name":"Goose","command":"goose","env":{"OPENAI_API_KEY":"preset-marker-value"}},{"id":"goose-work","name":"Goose work","command":"goose","args":["--profile","work"],"profileOf":"goose","profileLabel":"Work","env":{"OPENAI_API_KEY":"other-marker"}}]"#,
        )
        .unwrap();
        assert_eq!(
            ok.get("goose-work").unwrap().profile_of.as_deref(),
            Some("goose")
        );
        let listed = serde_json::to_string(&ok.discover()).unwrap();
        assert!(listed.contains("\"profileOf\":\"goose\""));
        assert!(listed.contains("\"profileLabel\":\"Work\""));
        assert!(!listed.contains("preset-marker-value"));
        assert!(!listed.contains("other-marker"));
        assert!(!listed.contains("OPENAI_API_KEY"));
        // An old file, with neither field, still loads.
        Registry::parse(r#"[{"id":"goose","name":"Goose","command":"goose"}]"#).unwrap();
        for raw in [
            r#"[{"id":"a","name":"A","command":"tool","profileOf":"missing"}]"#,
            r#"[{"id":"a","name":"A","command":"tool","profileOf":"a"}]"#,
            r#"[{"id":"a","name":"A","command":"tool"},{"id":"b","name":"B","command":"tool","profileOf":"a"},{"id":"c","name":"C","command":"tool","profileOf":"b"}]"#,
            r#"[{"id":"a","name":"A","command":"tool"},{"id":"b","name":"B","command":"other","profileOf":"a"}]"#,
        ] {
            assert!(Registry::parse(raw).is_err(), "{raw}");
        }
    }
    #[test]
    fn a_later_file_can_add_a_profile_and_cannot_change_the_base_command() {
        let mut registry =
            Registry::parse(r#"[{"id":"goose","name":"Goose","command":"goose"}]"#).unwrap();
        registry
            .merge_raw(
                r#"[{"id":"goose-work","name":"Work","command":"goose","profileOf":"goose","profileLabel":"Work"}]"#,
            )
            .unwrap();
        let err = registry
            .merge_raw(r#"[{"id":"goose","name":"Goose","command":"other-goose"}]"#)
            .unwrap_err();
        assert!(err.to_string().contains("same command"), "{err}");
        assert_eq!(registry.get("goose").unwrap().command, "goose");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.json");
        std::fs::write(
            &path,
            r#"[{"id":"goose","name":"Goose","command":"goose"},{"id":"work","name":"Work","command":"goose","profileOf":"goose","profileLabel":"Work"}]"#,
        )
        .unwrap();
        assert_eq!(
            Registry::load(&path)
                .unwrap()
                .get("work")
                .unwrap()
                .profile_label
                .as_deref(),
            Some("Work")
        );
        std::fs::write(
            &path,
            r#"[{"id":"work","name":"Work","command":"goose","profileOf":"missing"}]"#,
        )
        .unwrap();
        assert!(Registry::load(&path).is_err());
    }
    #[test]
    fn a_remote_profile_keeps_the_same_url() {
        let url = "wss://example.test/acp/far";
        let raw = format!(
            r#"[{{"id":"far","name":"Far","transport":"ws","url":"{url}","peer":"box","workspaceAlias":"work"}},{{"id":"far-two","name":"Far two","transport":"ws","url":"{url}","peer":"box","workspaceAlias":"work","profileOf":"far","profileLabel":"Two"}}]"#
        );
        assert!(Registry::parse(&raw).is_ok());
        let other = r#"[{"id":"far","name":"Far","transport":"ws","url":"wss://example.test/acp/far","peer":"box","workspaceAlias":"work"},{"id":"far-two","name":"Far two","transport":"ws","url":"wss://example.test/acp/other","peer":"box","workspaceAlias":"work","profileOf":"far"}]"#;
        let err = match Registry::parse(other) {
            Ok(_) => panic!("a remote profile with a different url was accepted"),
            Err(error) => error,
        };
        assert!(err.to_string().contains("same url"), "{err}");
    }
    #[test]
    fn removing_a_base_agent_keeps_its_profiles() {
        let mut registry = Registry::parse(
            r#"[{"id":"goose","name":"Goose","command":"goose"},{"id":"work","name":"Work","command":"goose","profileOf":"goose"}]"#,
        )
        .unwrap();
        assert!(registry.remove("goose").is_err());
        assert!(registry.contains("goose"));
        assert!(registry.remove("work").unwrap());
        assert!(registry.remove("goose").unwrap());
    }
}
