use acportal_protocol::acp;
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::{collections::HashSet, path::Path};

#[derive(Debug)]
pub struct UnsupportedTransport;
impl std::fmt::Display for UnsupportedTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("agent does not support requested MCP transport")
    }
}
impl std::error::Error for UnsupportedTransport {}

/// Validate before spawning, and again against the initialized agent's capabilities.
/// Definitions stay in process memory; metadata and diagnostics never contain their secrets.
pub fn validate(servers: &[Value], initialization: Option<&Value>) -> Result<()> {
    ensure!(servers.len() <= 16, "too many MCP servers");
    ensure!(
        serde_json::to_vec(servers)?.len() <= 12_000,
        "MCP definitions too large"
    );
    let mut names = HashSet::new();
    for server in servers {
        let object = server
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("invalid MCP definition"))?;
        let name = object.get("name").and_then(Value::as_str).unwrap_or("");
        ensure!(
            !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control),
            "invalid MCP name"
        );
        ensure!(names.insert(name), "duplicate MCP name");
        let kind = server
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("stdio");
        let allowed: &[&str] = match kind {
            "stdio" if server.get("type").is_none() => &["name", "command", "args", "env"],
            "http" | "sse" => &["type", "name", "url", "headers"],
            _ => bail!("unsupported MCP transport"),
        };
        ensure!(
            object.keys().all(|key| allowed.contains(&key.as_str())),
            "invalid MCP fields"
        );
        let _: acp::McpServer = serde_json::from_value(server.clone())?;
        if kind == "stdio" {
            let command = server["command"].as_str().unwrap_or("");
            ensure!(
                Path::new(command).is_absolute()
                    && command.len() <= 4096
                    && !command.contains('\0'),
                "MCP executable must be absolute"
            );
            let args = server["args"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid MCP arguments"))?;
            ensure!(
                args.len() <= 128
                    && args.iter().all(|arg| arg
                        .as_str()
                        .is_some_and(|value| value.len() <= 4096 && !value.contains('\0'))),
                "invalid MCP arguments"
            );
            validate_pairs(&server["env"], false)?;
        } else {
            let url = reqwest::Url::parse(server["url"].as_str().unwrap_or(""))?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none(),
                "invalid MCP URL"
            );
            validate_pairs(&server["headers"], true)?;
            if let Some(initialization) = initialization
                && initialization["agentCapabilities"]["mcpCapabilities"][kind] != true
            {
                return Err(UnsupportedTransport.into());
            }
        }
    }
    Ok(())
}

fn validate_pairs(value: &Value, headers: bool) -> Result<()> {
    let pairs = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid MCP values"))?;
    ensure!(pairs.len() <= 32, "too many MCP values");
    let mut names = HashSet::new();
    for pair in pairs {
        let name = pair["name"].as_str().unwrap_or("");
        let value = pair["value"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid MCP value"))?;
        ensure!(
            !name.is_empty()
                && name.len() <= 128
                && name.bytes().all(|byte| byte.is_ascii_alphanumeric()
                    || if headers {
                        b"!#$%&'*+-.^_`|~".contains(&byte)
                    } else {
                        byte == b'_'
                    }),
            "invalid MCP value name"
        );
        ensure!(
            names.insert(if headers || cfg!(windows) {
                name.to_ascii_lowercase()
            } else {
                name.to_owned()
            }),
            "duplicate MCP value"
        );
        ensure!(
            value.len() <= 4096
                && !value.contains('\0')
                && (!headers || !value.contains(['\r', '\n'])),
            "invalid MCP value"
        );
        ensure!(
            pair.as_object().is_some_and(|object| object.len() == 2),
            "invalid MCP value fields"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn capabilities_and_hostile_values_are_rejected() {
        let http =
            json!({"type":"http","name":"docs","url":"https://example.com/mcp","headers":[]});
        assert!(validate(std::slice::from_ref(&http), None).is_ok());
        assert!(validate(std::slice::from_ref(&http), Some(&json!({}))).is_err());
        assert!(
            validate(
                std::slice::from_ref(&http),
                Some(&json!({"agentCapabilities":{"mcpCapabilities":{"http":true}}}))
            )
            .is_ok()
        );
        assert!(validate(&[http.clone(), http], None).is_err());
        assert!(validate(&[json!({"type":"sse","name":"docs","url":"https://user:secret@example.com","headers":[]})], None).is_err());
        assert!(validate(&[json!({"type":"http","name":"docs","url":"https://example.com","headers":[{"name":"Authorization","value":"secret\r\nInjected: x"}]})], None).is_err());
        assert!(validate(&[json!({"type":"unknown","name":"tools","command":"/bin/tool","args":[],"env":[]})], None).is_err());
    }
}
