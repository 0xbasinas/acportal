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
}
impl AgentDefinition {
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            bail!("agent id must contain 1..64 ASCII letters, digits, '-' or '_'")
        }
        if self.name.trim().is_empty() || self.command.trim().is_empty() {
            bail!("name and command are required")
        }
        if self.transport != "stdio" {
            bail!("only stdio agent transport is supported")
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
    pub fn discover(&self) -> Discovery {
        let validation = self.validate().err().map(|error| error.to_string());
        let executable =
            resolve_executable(&self.command, self.env.get("PATH").map(String::as_str));
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
        }
    }
}
impl Registry {
    pub fn parse(raw: &str) -> Result<Self> {
        let definitions: Vec<AgentDefinition> = serde_json::from_str(raw)
            .context("registry must be a JSON array of agent definitions")?;
        let mut agents = BTreeMap::new();
        for definition in definitions {
            definition
                .validate()
                .with_context(|| format!("invalid agent {}", definition.id))?;
            if agents.insert(definition.id.clone(), definition).is_some() {
                bail!("duplicate agent id")
            }
        }
        Ok(Self { agents })
    }
    pub fn load(path: &Path) -> Result<Self> {
        let builtins = Self::parse(include_str!("../registry/builtin.json"))?;
        if !path.exists() {
            return Ok(builtins);
        }
        let mut registry = builtins;
        let custom = Self::parse(&std::fs::read_to_string(path).context("read agent registry")?)?;
        registry.agents.extend(custom.agents);
        Ok(registry)
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
fn executable(path: &Path) -> bool {
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
        let gemini = custom.get("gemini-cli-node").unwrap();
        assert_eq!(gemini.args.last().map(String::as_str), Some("--acp"));
    }
    #[test]
    fn arbitrary_custom_agents_and_literal_arguments() {
        let registry = Registry::parse(r#"[{"id":"future","name":"Future","command":"future-agent","args":["$(touch hacked)","; rm -rf /"],"env":{"KEY":"secret"},"icon":"future.svg"}]"#).unwrap();
        assert_eq!(registry.get("future").unwrap().args[0], "$(touch hacked)");
        assert_eq!(registry.discover()[0].status, AgentStatus::Missing);
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
}
