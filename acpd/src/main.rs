use acpd::{
    api::Host,
    config::{Config, default_directory},
    connection::AcpConnection,
    registry::Registry,
    security::SecurityStore,
    session::{AcpSession, SessionManager},
};
use acportal_protocol::{acp, initialize_params};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Parser)]
#[command(version, about = "Agent-agnostic ACP host core and local client")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    registry: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Start the authenticated host. Remote listeners require configured TLS.
    Start,
    /// Generate a one-use pairing code through the local operator store.
    Pair,
    /// List paired device names and expiration without credentials.
    Devices,
    /// Revoke a paired device, including its open sockets at the next heartbeat.
    Revoke { id: uuid::Uuid },
    /// Read authenticated host status with a token supplied through ACPD_TOKEN.
    Status {
        #[arg(long, default_value = "http://127.0.0.1:8765")]
        address: String,
    },
    /// Read authenticated host sessions with a token supplied through ACPD_TOKEN.
    Sessions {
        #[arg(long, default_value = "http://127.0.0.1:8765")]
        address: String,
    },
    /// Discover configured agents without running them or exposing environment values.
    Agents,
    /// Diagnose configuration, executables, workspaces, listener and storage.
    Doctor,
    /// Print effective non-secret host configuration. Does not overwrite files.
    Config,
    /// Verify ACP initialization without sending a prompt or starting a model request.
    Probe {
        #[arg(long, default_value = "goose")]
        agent: String,
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Run a local ACP session. Permission choices always require interactive input.
    Chat {
        #[arg(long, default_value = "goose")]
        agent: String,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long)]
        load: Option<String>,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "acpd=info".into()),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
    let cli = Cli::parse();
    let default_file = default_directory().join("config.toml");
    let mut config = match &cli.config {
        Some(file) => Config::load(file)?,
        None if default_file.exists() => Config::load(&default_file)?,
        None => Config::default(),
    };
    if let Some(path) = cli.registry {
        config.registry = path;
    }
    let registry = Registry::load(&config.registry)?;
    match cli.command {
        Commands::Start => Host::new(config, registry)?.serve().await?,
        Commands::Pair => {
            let store = SecurityStore::open(&config.state_directory)?;
            println!(
                "Pairing code: {}\nExpires in 60 seconds.",
                store.new_pairing_code()?
            );
        }
        Commands::Devices => println!(
            "{}",
            serde_json::to_string_pretty(
                &SecurityStore::open(&config.state_directory)?.devices()?
            )?
        ),
        Commands::Revoke { id } => {
            SecurityStore::open(&config.state_directory)?.revoke(id)?;
            println!("Device revoked.");
        }
        Commands::Status { address } => host_query(&address, "/v1/status").await?,
        Commands::Sessions { address } => host_query(&address, "/v1/sessions").await?,
        Commands::Agents => println!("{}", serde_json::to_string_pretty(&registry.discover())?),
        Commands::Config => println!("{}", toml::to_string_pretty(&config)?),
        Commands::Doctor => {
            config.validate()?;
            let agents = registry.discover();
            let mut healthy = !config.workspace_roots.is_empty();
            for (message, passed) in acpd::doctor::operational_checks(&config) {
                println!("{message}");
                healthy &= passed;
            }
            for root in &config.workspace_roots {
                healthy &= config.workspace(root).is_ok();
            }
            for agent in &agents {
                println!(
                    "{}: {:?}{}",
                    agent.id,
                    agent.status,
                    if agent.enabled { "" } else { " (disabled)" }
                );
                if agent.enabled && !agent.installed {
                    healthy = false;
                }
            }
            println!(
                "Workspace roots configured: {}",
                config.workspace_roots.len()
            );
            println!(
                "Provider authentication is agent-owned. Use probe for ACP negotiation; chat exercises the provider."
            );
            if !healthy {
                bail!(
                    "diagnostics failed; review listener, storage, executables and workspace configuration above"
                )
            }
        }
        Commands::Probe { agent, workspace } => {
            let workspace = config.workspace(&workspace)?;
            let definition = registry.get(&agent)?;
            if !definition.enabled {
                bail!("agent is disabled")
            }
            let cwd = match &definition.working_directory {
                Some(cwd) => config.workspace(cwd)?,
                None => workspace,
            };
            let executable = definition
                .discover()
                .executable
                .context("agent executable not found")?;
            let connection = AcpConnection::spawn(definition, &executable, &cwd, config.runtime)?;
            let result = connection.request("initialize", initialize_params()).await;
            connection.shutdown().await;
            let result = result?;
            let parsed: acp::InitializeResponse = serde_json::from_value(result.clone())?;
            if parsed.protocol_version != acportal_protocol::ProtocolVersion::V1 {
                bail!("agent does not support stable ACP v1")
            }
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::Chat {
            agent,
            workspace,
            prompt,
            load,
        } => {
            let manager = SessionManager::new(config, registry)?;
            let result = async {
                let session = match load {
                    Some(id) => manager.load(&agent, &workspace, &id).await?,
                    None => manager.create(&agent, &workspace).await?,
                };
                println!("ACP session: {}", session.metadata().acp_session_id);
                let mut input = BufReader::new(tokio::io::stdin());
                if let Some(prompt) = prompt { run_prompt(&session, &prompt, &mut input).await?; }
                else {
                    loop {
                        eprintln!("Prompt (/quit to exit):");
                        let mut prompt = String::new();
                        tokio::select! {
                            read = input.read_line(&mut prompt) => { if read? == 0 || prompt.trim() == "/quit" { break; } }
                            _ = tokio::signal::ctrl_c() => break,
                        }
                        if !prompt.trim().is_empty() { run_prompt(&session, prompt.trim(), &mut input).await?; }
                    }
                }
                Ok::<_, anyhow::Error>(())
            }.await;
            manager.shutdown().await;
            result?;
        }
    }
    Ok(())
}
async fn host_query(address: &str, endpoint: &str) -> Result<()> {
    let url = reqwest::Url::parse(address)?;
    if url.scheme() != "https"
        && !(url.scheme() == "http"
            && url.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            }))
    {
        bail!("remote host queries require HTTPS")
    }
    let token = std::env::var("ACPD_TOKEN").context("set ACPD_TOKEN to a paired-device token")?;
    let value: Value = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?
        .get(format!("{}{endpoint}", address.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
async fn run_prompt(
    session: &Arc<AcpSession>,
    text: &str,
    input: &mut BufReader<tokio::io::Stdin>,
) -> Result<()> {
    let mut events = session.connection.subscribe();
    let prompt = session.prompt(text);
    tokio::pin!(prompt);
    loop {
        tokio::select! {
            result = &mut prompt => {
                println!("\nStop: {}", result?["stopReason"]);
                return Ok(())
            }
            _ = tokio::signal::ctrl_c() => {
                session.cancel().await?;
                eprintln!("Cancelling...");
            }
            event = events.recv() => {
                match event {
                    Ok(event) => render_event(session, &event.message, input).await?,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        bail!("local event consumer fell behind; use session replay to recover")
                    }
                    Err(_) => bail!("agent event stream closed"),
                }
            }
        }
    }
}
async fn render_event(
    session: &AcpSession,
    message: &Value,
    input: &mut BufReader<tokio::io::Stdin>,
) -> Result<()> {
    if message["method"] == "session/request_permission" {
        let params = &message["params"];
        eprintln!(
            "\nPermission requested: {}",
            params["toolCall"]["title"]
                .as_str()
                .unwrap_or("Agent operation")
        );
        if let Some(raw) = params["toolCall"].get("rawInput") {
            eprintln!("{raw}");
        }
        let options = params["options"]
            .as_array()
            .context("permission options missing")?;
        for (index, option) in options.iter().enumerate() {
            eprintln!(
                "{}. {}",
                index + 1,
                option["name"].as_str().unwrap_or("Option")
            );
        }
        eprintln!("Choose a number. Enter or EOF cancels permission:");
        let mut choice = String::new();
        let mut cancelled = false;
        tokio::select! {
            result = input.read_line(&mut choice) => { result?; }
            _ = tokio::signal::ctrl_c() => { cancelled = true; }
        }
        if cancelled {
            session.cancel().await?;
            return Ok(());
        }
        let outcome = choice
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| options.get(index))
            .map_or_else(
                || json!({"outcome":"cancelled"}),
                |option| json!({"outcome":"selected","optionId":option["optionId"]}),
            );
        session
            .connection
            .permission(message["id"].clone(), outcome)
            .await?;
    } else if message["method"] == "session/update" {
        let update = &message["params"]["update"];
        match update["sessionUpdate"].as_str() {
            Some("agent_message_chunk") => {
                print!("{}", update["content"]["text"].as_str().unwrap_or(""))
            }
            Some("tool_call") | Some("tool_call_update") => {
                println!(
                    "\n[{}] {} {}",
                    update["kind"].as_str().unwrap_or("tool"),
                    update["title"].as_str().unwrap_or("Activity"),
                    update["status"].as_str().unwrap_or("")
                );
                if let Some(content) = update["content"].as_array() {
                    for item in content {
                        if item["type"] == "diff" {
                            println!(
                                "Diff {}\n- {}\n+ {}",
                                item["path"], item["oldText"], item["newText"]
                            );
                        } else if let Some(text) = item["content"]["text"].as_str() {
                            println!("{text}");
                        }
                    }
                }
            }
            Some("available_commands_update") => {
                let count = update["availableCommands"].as_array().map_or(0, Vec::len);
                println!("\n{count} commands available.");
            }
            Some("plan") => {
                if let Some(entries) = update["entries"].as_array() {
                    for entry in entries {
                        println!(
                            "\n[{}] {}",
                            entry["status"].as_str().unwrap_or("pending"),
                            entry["content"].as_str().unwrap_or("")
                        );
                    }
                }
            }
            Some(
                "usage_update"
                | "session_info_update"
                | "user_message_chunk"
                | "agent_thought_chunk",
            ) => {}
            Some(kind) => println!("\n[{kind}]"),
            None => {}
        }
        use std::io::Write;
        std::io::stdout().flush()?;
    }
    Ok(())
}
