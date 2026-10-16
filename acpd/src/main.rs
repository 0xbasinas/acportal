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
        /// HTTPS address the phone uses to reach this computer (for example a Tailscale Serve address).
        #[arg(long)]
        address: Option<String>,
        #[arg(long, default_value = "Workstation")]
        name: String,
    },
    /// Start the authenticated host. Remote listeners require configured TLS.
    Start {
        /// Run detached and control it with acpd daemon status/stop/reload.
        #[arg(long, short = 'b', visible_alias = "bg")]
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
        /// `phone` manages this host from the app. `peer` is another acpd host that may only use
        /// the agent bridge (`/acp`); it cannot reach any phone management route.
        #[arg(long, value_enum, default_value = "phone", conflicts_with_all = ["qr", "manual"])]
        role: PairRole,
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
    /// Show the delegated tasks this host has recorded, read from its saved task records.
    ///
    /// Works whether the host is running or not and shows the last state written. After a
    /// restart, tasks that were still open show as failed ("interrupted"); nothing is restarted.
    Tasks {
        /// Print the records as JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Manage the other hosts this computer connects to as a client (agents on another host).
    Peer {
        #[command(subcommand)]
        action: PeerAction,
    },
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
    /// Run the experimental MCP delegation server for an agent session. Started by the host, not by hand.
    #[command(hide = true)]
    McpDelegate,
    /// Print effective non-secret host configuration. Does not overwrite files.
    Config,
    /// Verify ACP initialization without sending a prompt or starting a model request.
    Probe {
        /// Agent id from the registry (see `acpd agents`). There is no default agent.
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Run a local ACP session. Permission choices always require interactive input.
    Chat {
        /// Agent id from the registry (see `acpd agents`). There is no default agent.
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long)]
        load: Option<String>,
    },
}
#[derive(Subcommand)]
enum PeerAction {
    /// Pair with another acpd host, or save the shared secret of a server that is not acpd.
    ///
    /// For acpd, run `acpd pair --role peer` on the other host and give the code it prints here
    /// within 60 seconds. For anything else, name the header and the environment variable that
    /// holds its secret. The credential is stored owner-only and sent only to this address.
    Add {
        /// A short name for this peer (a-z, 0-9, '-' or '_'), used by registry entries.
        #[arg(long)]
        name: String,
        /// The other host's address, such as https://laptop.example:8765.
        #[arg(long)]
        address: String,
        /// The pairing code. Prefer --code-env: a command line is kept in shell history.
        #[arg(long, conflicts_with_all = ["code_env", "header"])]
        code: Option<String>,
        /// The environment variable that holds the pairing code.
        #[arg(long, conflicts_with = "header")]
        code_env: Option<String>,
        /// For a server that is not acpd: the header that carries its shared secret.
        #[arg(long, requires = "secret_env")]
        header: Option<String>,
        /// The environment variable that holds that secret (never put it on the command line).
        #[arg(long, requires = "header")]
        secret_env: Option<String>,
        /// The name the other host shows for this computer (default: this computer's name).
        #[arg(long = "as")]
        device_name: Option<String>,
    },
    /// List the peers: names, addresses and the kind of credential, never the secret.
    List,
    /// Forget a peer. This does not revoke the credential on the other host.
    Remove { name: String },
}
/// "5s ago", "12m ago", "3h ago", "2d ago".
fn ago(unix: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    match now.saturating_sub(unix) {
        seconds if seconds < 60 => format!("{seconds}s ago"),
        seconds if seconds < 3600 => format!("{}m ago", seconds / 60),
        seconds if seconds < 86_400 => format!("{}h ago", seconds / 3600),
        seconds => format!("{}d ago", seconds / 86_400),
    }
}
fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "acpd host".into())
}
#[derive(Clone, Copy, clap::ValueEnum)]
enum PairRole {
    Phone,
    Peer,
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
    #[cfg(windows)]
    if std::env::args_os().nth(1).as_deref() == Some(acpd::win_sandbox::LAUNCHER_ARG.as_ref()) {
        std::process::exit(acpd::win_sandbox::launcher_main());
    }
    acpd::win_sandbox::enable_launcher();
    let cli = Cli::parse();
    let profile_path = cli
        .profile
        .clone()
        .unwrap_or_else(|| default_directory().join("launcher.json"));
    if let Commands::McpDelegate = &cli.command {
        // Only MCP messages may reach stdout; diagnostics go to stderr. The task token is read from
        // the environment (never an argument) and is not printed.
        let path = std::env::var_os("ACPD_CONTROL").context("ACPD_CONTROL is not set")?;
        let token = std::env::var("ACPD_TASK_TOKEN").context("ACPD_TASK_TOKEN is not set")?;
        let stream = acpd::orchestrator::control::connect(std::path::Path::new(&path))
            .await
            .context("cannot reach the host control channel")?;
        let backend = acpd::orchestrator::control::ControlBackend::new(stream, token);
        {
            // Tell the host the delegation tools really started. Failure is not fatal: the tools
            // still work, the host just cannot show that they connected.
            use acpd::orchestrator::delegate::Backend as _;
            let _ = backend.call("_hello", serde_json::json!({})).await;
        }
        acpd::orchestrator::delegate::serve(tokio::io::stdin(), tokio::io::stdout(), &backend)
            .await?;
        return Ok(());
    }
    if let Commands::Setup { address, name } = &cli.command {
        // Without --config the settings live in the standard per-user folder, created on first use.
        let config = match &cli.config {
            Some(path) => path.clone(),
            None => {
                let existed = default_directory().join("config.toml").exists();
                let file = acpd::config::bootstrap_default_config(&default_directory())?;
                if !existed {
                    println!("Created your settings in {}", default_directory().display());
                    println!(
                        "Host data (paired devices) will be kept in {}",
                        acpd::config::default_state_directory().display()
                    );
                }
                file
            }
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
        println!(
            "Background host started (PID {pid}). Pair a phone with acpd pair; manage it with acpd daemon status, reload or stop."
        );
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
        Commands::McpDelegate => unreachable!("mcp-delegate handled before host initialization"),
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
            role,
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
            let role = match role {
                PairRole::Phone => acpd::security::Role::Phone,
                PairRole::Peer => acpd::security::Role::Peer,
            };
            let code = store.new_pairing_code_for(role)?;
            println!("Pairing code: {}\nExpires in 60 seconds.", code);
            if role == acpd::security::Role::Peer {
                println!(
                    "Role: peer. The paired host can use the agent bridge only, not phone management."
                );
            }
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
        Commands::Tasks { json } => {
            let records = acpd::orchestrator::records::load(&config.state_directory)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&records)?);
            } else if records.is_empty() {
                println!("No tasks recorded.");
            } else {
                println!(
                    "{:<9} {:<9} {:<16} {:<14} {:<10} NOTE",
                    "TASK", "PARENT", "AGENT", "STATE", "UPDATED"
                );
                for record in &records {
                    println!(
                        "{:<9} {:<9} {:<16} {:<14} {:<10} {}",
                        &record.id.to_string()[..8],
                        record
                            .parent
                            .map_or("-".into(), |id| id.to_string()[..8].to_owned()),
                        record.agent.chars().take(16).collect::<String>(),
                        record.state,
                        ago(record.updated_at),
                        record.summary.as_deref().unwrap_or("")
                    );
                }
            }
        }
        Commands::Peer { action } => {
            use acpd::peers::{Peer, PeerAuth, PeerStore, origin_of};
            let store = PeerStore::open(&config.state_directory);
            match action {
                PeerAction::List => {
                    println!("{}", serde_json::to_string_pretty(&store.summaries()?)?)
                }
                PeerAction::Remove { name } => {
                    if !store.remove(&name)? {
                        bail!("no peer named {name}")
                    }
                    println!(
                        "Peer {name} removed. The other host still lists this computer as a paired device; revoke it there (acpd revoke <id>) if it should lose access."
                    );
                }
                PeerAction::Add {
                    name,
                    address,
                    code,
                    code_env,
                    header,
                    secret_env,
                    device_name,
                } => {
                    let origin = origin_of(&address)?;
                    // Check everything that can be checked here before a one-use code is spent.
                    let paired_with_code = header.is_none();
                    let auth = match (header, secret_env) {
                        (Some(header), Some(variable)) => PeerAuth::Header {
                            name: header,
                            value: std::env::var(&variable)
                                .ok()
                                .filter(|value| !value.is_empty())
                                .with_context(|| {
                                    format!("the environment variable {variable} is not set")
                                })?,
                        },
                        // A stand-in with the right shape, replaced once the code is spent.
                        _ => PeerAuth::Bearer {
                            token: "0".repeat(64),
                        },
                    };
                    let mut peer = Peer {
                        name: name.clone(),
                        origin: origin.clone(),
                        auth,
                        device_id: None,
                    };
                    store.check_can_add(&peer)?;
                    if paired_with_code {
                        let code = match (code, code_env) {
                            (Some(code), _) => code,
                            (None, Some(variable)) => std::env::var(&variable)
                                .ok()
                                .filter(|value| !value.trim().is_empty())
                                .with_context(|| {
                                    format!("the environment variable {variable} is not set")
                                })?,
                            (None, None) => {
                                use std::io::IsTerminal;
                                if !std::io::stdin().is_terminal() {
                                    bail!("give the pairing code with --code-env (or --code)")
                                }
                                operator_input(
                                    "Pairing code from the other host (acpd pair --role peer)",
                                )?
                            }
                        };
                        let (token, device_id) = acpd::peer_cli::redeem(
                            &origin,
                            code.trim(),
                            &device_name.unwrap_or_else(computer_name),
                        )
                        .await?;
                        peer.auth = PeerAuth::Bearer { token };
                        peer.device_id = device_id;
                    }
                    let device_id = peer.device_id;
                    store.add(peer)?;
                    println!(
                        "Peer {name} added for {origin}. The credential is stored owner-only and is sent only to that address."
                    );
                    if let Some(id) = device_id {
                        println!(
                            "On the other host this computer is device {id}; revoke it there with: acpd revoke {id}"
                        );
                    }
                    println!(
                        "To use an agent there, add a registry entry such as:\n  {{\"id\":\"{name}-agent\",\"name\":\"Agent on {name}\",\"transport\":\"ws\",\"url\":\"{origin}/acp/AGENT_ID\",\"peer\":\"{name}\",\"workspaceAlias\":\"ALIAS\"}}\nAGENT_ID and ALIAS must be shared with peers on the other host (peer_agents and peer_workspaces in its [orchestrator] settings)."
                    );
                }
            }
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
            let agent = required_agent(agent, &registry)?;
            let workspace = config.workspace(&workspace)?;
            let definition = registry.get(&agent)?;
            if !definition.enabled {
                bail!("agent is disabled")
            }
            let cwd = match &definition.working_directory {
                Some(cwd) => config.workspace(cwd)?,
                None => workspace,
            };
            let launch = definition
                .discover()
                .launch
                .context("agent executable not found")?;
            let connection = AcpConnection::spawn(definition, launch, &cwd, config.runtime)?;
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
            let agent = required_agent(agent, &registry)?;
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

/// The `--agent` the operator chose. Without one, the error lists the registered agents instead of
/// picking one.
fn required_agent(agent: Option<String>, registry: &Registry) -> Result<String> {
    if let Some(agent) = agent {
        return Ok(agent);
    }
    let ids: Vec<String> = registry
        .discover()
        .into_iter()
        .map(|agent| agent.id)
        .collect();
    if ids.is_empty() {
        bail!(
            "--agent is required, and no agents are registered; add one from the desktop or phone app, or in agents.json"
        )
    }
    bail!("--agent is required; registered agents: {}", ids.join(", "))
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
