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
#[command(
    version,
    about = "Agent-agnostic ACP host core and local client",
    after_help = "Everyday workflow:\n  acpd setup --config PATH --address https://YOUR-HOST\n  acpd start                 Keep this terminal open\n  acpd pair                  Scan in the Android app\n  acpd pair --manual          Manual code only\n  acpd doctor                Check the saved configuration"
)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    registry: Option<PathBuf>,
    /// Saved operator setup. Defaults to launcher.json in the acpd config directory.
    #[arg(long, global = true)]
    profile: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Remember an existing config and HTTPS address for short start/pair commands.
    Setup {
        #[arg(long)]
        address: Option<String>,
        #[arg(long, default_value = "Workstation")]
        name: String,
    },
    /// Start the authenticated host. Remote listeners require configured TLS.
    Start {
        /// Run detached and control it with acpd daemon status/stop/reload.
        #[arg(long)]
        background: bool,
    },
    /// Manage a local host without using phone credentials.
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    #[command(hide = true)]
    DaemonWorker,
    /// Generate a one-use pairing code through the local operator store.
    Pair {
        /// Phone-reachable HTTPS address (for example a Tailscale Serve address).
        #[arg(long)]
        address: Option<String>,
        /// Display a QR code in this terminal. Never redirect pairing output to logs.
        #[arg(long, conflicts_with = "manual")]
        qr: bool,
        /// Print only the manual code even when setup has a saved QR address.
        #[arg(long, conflicts_with = "qr")]
        manual: bool,
        /// Optional connection name included in the QR code.
        #[arg(long, default_value = "", requires = "address")]
        name: String,
    },
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
    /// Diagnose configuration, executables, workspaces, listener, storage, free space, logging and TLS.
    Doctor {
        /// Also start this agent with host callbacks disabled and create one session without a
        /// prompt to check its sign-in state. Requires --workspace.
        #[arg(long, requires = "workspace")]
        agent: Option<String>,
        /// Authorized workspace used for the sign-in check.
        #[arg(long, requires = "agent")]
        workspace: Option<PathBuf>,
        /// Also send one tiny prompt ("reply with OK") so the agent must reach its model
        /// provider. Costs one small model request; off by default.
        #[arg(long, requires = "agent")]
        prompt_check: bool,
    },
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
#[derive(Subcommand)]
enum DaemonAction {
    /// Start the saved host in the background.
    Start,
    /// Check the authenticated local control channel, never just a saved PID.
    Status,
    /// Gracefully stop the managed host and its agent sessions.
    Stop,
    /// Validate and atomically reload settings for future sessions.
    Reload,
}
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let profile_path = cli
        .profile
        .clone()
        .unwrap_or_else(|| default_directory().join("launcher.json"));
    if let Commands::Setup { address, name } = &cli.command {
        let config = match &cli.config {
            Some(path) => path.clone(),
            None => PathBuf::from(operator_input("Existing host config path")?),
        };
        let address = match address {
            Some(value) => value.clone(),
            None => operator_input("Phone-reachable HTTPS address (for example Tailscale Serve)")?,
        };
        acpd::launcher::Launcher::prepare(&config, &address, name)?.save(&profile_path)?;
        println!(
            "Setup saved. Run acpd start, then acpd pair in a second terminal.\nSetup keeps your existing config, workspace roots and credentials unchanged."
        );
        return Ok(());
    }
    let saved = if profile_path.exists()
        && (cli.config.is_none() || matches!(&cli.command, Commands::Pair { address: None, .. }))
    {
        Some(acpd::launcher::Launcher::load(&profile_path)?)
    } else {
        None
    };
    let config_file = cli
        .config
        .as_ref()
        .or_else(|| saved.as_ref().map(|p| &p.config));
    let default_file = default_directory().join("config.toml");
    let selected_file = config_file.cloned().unwrap_or_else(|| default_file.clone());
    let control_path = acpd::daemon::record_path(&selected_file, &profile_path)?;
    if let Commands::Daemon { action } = &cli.command {
        match action {
            DaemonAction::Status => {
                if !control_path.exists() {
                    println!("No managed host for this config.");
                    return Ok(());
                }
                let reply = acpd::daemon::request(&control_path, "status").await?;
                println!("Host {} (PID {}).", reply.status, reply.pid);
                return Ok(());
            }
            DaemonAction::Stop => {
                acpd::daemon::stop(&control_path).await?;
                println!("Host stopped.");
                return Ok(());
            }
            DaemonAction::Reload => {
                let reply = acpd::daemon::request(&control_path, "reload").await?;
                match reply.status.as_str() {
                    "reloaded" => println!(
                        "Reloaded. Existing sessions keep their launch settings; future sessions use the new settings."
                    ),
                    "restart_required" => bail!(
                        "reload rejected: listener, TLS, tokens, storage, frame size or logging changed; restart required, previous settings retained"
                    ),
                    "invalid_configuration" => bail!(
                        "reload rejected: invalid config or registry; previous settings retained"
                    ),
                    _ => bail!("reload was not accepted"),
                }
                return Ok(());
            }
            DaemonAction::Start => {}
        }
    }
    let mut config = match config_file {
        Some(file) => Config::load(file)?,
        None if default_file.exists() => Config::load(&default_file)?,
        None => Config::default(),
    };
    if let Some(path) = &cli.registry {
        config.registry = path.clone();
    }
    if matches!(
        &cli.command,
        Commands::Start { background: true }
            | Commands::Daemon {
                action: DaemonAction::Start
            }
    ) {
        Config::load(&selected_file)?;
        Registry::load(&config.registry)?;
        let pid =
            acpd::daemon::start(&selected_file, cli.registry.as_deref(), &profile_path).await?;
        println!("Background host started (PID {pid}). Use acpd daemon status, reload or stop.");
        return Ok(());
    }
    // Only the long-running host writes the optional bounded file sink; diagnostics and the
    // local client keep stderr-only output and never create log files.
    let log_file = match (&cli.command, &config.logging.file) {
        (Commands::Start { background: false } | Commands::DaemonWorker, Some(_)) => {
            Some(acpd::logging::BoundedLog::open(&config.logging)?)
        }
        _ => None,
    };
    if config.logging.frame_metadata {
        acpd::connection::enable_frame_metadata();
    }
    init_tracing(log_file, config.logging.frame_metadata);
    let registry = Registry::load(&config.registry)?;
    let foreground = matches!(&cli.command, Commands::Start { background: false });
    match cli.command {
        Commands::Setup { .. } => unreachable!("setup handled before host initialization"),
        Commands::Start { background: false } | Commands::DaemonWorker => {
            if foreground {
                println!(
                    "Starting ACP Portal. Keep this terminal open; Ctrl+C stops the host.\nTo connect a phone, run acpd pair in a second terminal."
                );
            }
            let host = Host::new(config, registry)?;
            if selected_file.exists() {
                acpd::daemon::serve(host, selected_file, cli.registry.clone(), control_path).await?
            } else {
                host.serve().await?
            }
        }
        Commands::Start { background: true } | Commands::Daemon { .. } => {
            unreachable!("daemon command handled before host initialization")
        }
        Commands::Pair {
            address,
            qr,
            manual,
            name,
        } => {
            // An explicit different config must never inherit another host's address.
            let profile = saved
                .as_ref()
                .filter(|p| config_file.is_some_and(|f| p.matches_config(f)));
            let from_setup = address.is_none() && profile.is_some();
            let address = address.or_else(|| profile.map(|p| p.address.clone()));
            let name = if from_setup && name.is_empty() {
                profile.unwrap().name.clone()
            } else {
                name
            };
            let qr = !manual && (qr || from_setup);
            if qr && address.is_none() {
                bail!(
                    "QR pairing needs an HTTPS address: run acpd setup or acpd pair --address https://YOUR-HOST --qr"
                )
            }
            if let Some(address) = &address {
                acpd::pairing_input::validate_pairing_address(address)?;
                // Validate all presentation inputs before replacing an unexpired code.
                let preview = acpd::pairing_input::pairing_uri(address, "0000-0000-0000", &name)?;
                if qr {
                    acpd::pairing_input::terminal_qr(&preview)?;
                }
            }
            let store = SecurityStore::open(&config.state_directory)?;
            let code = store.new_pairing_code()?;
            println!("Pairing code: {}\nExpires in 60 seconds.", code);
            if let Some(address) = address {
                println!("Host address: {address}");
                if qr {
                    let uri = acpd::pairing_input::pairing_uri(&address, &code, &name)?;
                    println!("{}", acpd::pairing_input::terminal_qr(&uri)?);
                    println!(
                        "In ACP Portal, choose Scan pairing QR code, review the address, then tap Pair connection."
                    );
                }
            }
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
        Commands::Doctor {
            agent,
            workspace,
            prompt_check,
        } => {
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
            match (agent, workspace) {
                (Some(agent), Some(workspace)) => {
                    let (message, passed) = acpd::doctor::agent_check(
                        &config,
                        registry,
                        &agent,
                        &workspace,
                        prompt_check,
                    )
                    .await;
                    println!("{message}");
                    healthy &= passed;
                }
                _ => println!(
                    "Provider sign-in: not checked. Run doctor --agent <id> --workspace <dir> to start the agent and create one session without a prompt; add --prompt-check to also send one tiny prompt (one small model request)."
                ),
            }
            if !healthy {
                bail!(
                    "diagnostics failed; review listener, storage, free space, log, TLS, executable, workspace and sign-in checks above"
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

fn operator_input(label: &str) -> Result<String> {
    use std::io::{IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        bail!("non-interactive setup requires --config and --address")
    }
    print!("{label}: ");
    std::io::stdout().flush()?;
    let mut value = String::new();
    std::io::stdin().read_line(&mut value)?;
    let value = value.trim();
    if value.is_empty() {
        bail!("setup value cannot be empty")
    }
    Ok(value.into())
}
fn init_tracing(log_file: Option<std::sync::Arc<acpd::logging::BoundedLog>>, frame_metadata: bool) {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};
    let mut filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "acpd=info".into());
    if frame_metadata {
        filter = filter.add_directive("acpd::acp_frames=debug".parse().expect("static directive"));
    }
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr).with_target(false))
        .with(log_file.map(|log| {
            fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_writer(log.writer())
        }))
        .init();
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

#[cfg(test)]
mod pairing_cli_tests {
    use super::*;
    #[test]
    fn original_pair_command_remains_valid() {
        assert!(matches!(
            Cli::try_parse_from(["acpd", "pair"]).unwrap().command,
            Commands::Pair {
                address: None,
                qr: false,
                ..
            }
        ));
    }
    #[test]
    fn qr_can_use_setup_and_manual_is_explicit() {
        assert!(Cli::try_parse_from(["acpd", "pair", "--qr"]).is_ok());
        assert!(Cli::try_parse_from(["acpd", "pair", "--qr", "--manual"]).is_err());
        assert!(
            Cli::try_parse_from(["acpd", "pair", "--address", "https://host.example", "--qr"])
                .is_ok()
        );
        assert!(Cli::try_parse_from(["acpd", "pair", "--name", "Laptop"]).is_err());
    }
}
