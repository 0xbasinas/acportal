use crate::{
    config::Config,
    connection::{AcpConnection, AgentRpcError, Replay, WorkspaceAccess},
    orchestrator::Orchestrator,
    registry::Registry,
    session_catalog::SessionCatalog,
};
use acportal_protocol::{ACP_VERSION, acp, initialize_params};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub id: Uuid,
    pub agent_id: String,
    pub acp_session_id: String,
    pub workspace: PathBuf,
    pub initialization: Value,
    pub setup: Value,
    pub status: String,
    #[serde(default)]
    pub workspace_access: WorkspaceAccess,
    /// Copied from the registry's `ownTools` when the session starts, so the phone can warn
    /// inside the conversation.
    #[serde(default)]
    pub own_tools: bool,
    /// For an agent that runs on another host: the name of the peer it runs on. Absent for
    /// agents started on this computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_peer: Option<String>,
}

/// What `session/new` carries for an agent on another host, kept so signing in later repeats it.
#[derive(Clone)]
struct RemoteSetup {
    cwd: String,
    meta: Option<Value>,
}

pub struct AcpSession {
    metadata: std::sync::Mutex<SessionMetadata>,
    setup_lock: Mutex<()>,
    load_id: Option<String>,
    mcp_servers: Vec<Value>,
    remote: Option<RemoteSetup>,
    /// The W3C `traceparent` of the task this session belongs to, sent in `_meta`.
    trace: Option<String>,
    catalog: Option<Arc<SessionCatalog>>,
    pub connection: AcpConnection,
}
impl AcpSession {
    pub fn metadata(&self) -> SessionMetadata {
        let mut metadata = self.metadata.lock().unwrap().clone();
        metadata.status = if self.connection.closed_reason().is_some() {
            "exited"
        } else if metadata.acp_session_id.is_empty() {
            "authentication_required"
        } else if self.connection.is_busy() {
            "running"
        } else {
            "ready"
        }
        .into();
        metadata
    }
    pub async fn prompt(&self, text: &str) -> Result<Value> {
        let metadata = self.metadata();
        if metadata.acp_session_id.is_empty() {
            bail!("authentication required")
        }
        let mut params = json!({"sessionId":metadata.acp_session_id,
            "prompt":[{"type":"text","text":text}]});
        if let Some(trace) = &self.trace {
            params["_meta"] = json!({ "traceparent": trace });
        }
        let result = self.connection.request("session/prompt", params).await?;
        let _: acp::PromptResponse =
            serde_json::from_value(result.clone()).context("invalid ACP prompt result")?;
        Ok(result)
    }
    pub async fn cancel(&self) -> Result<()> {
        self.connection
            .notify(
                "session/cancel",
                json!({"sessionId":self.metadata().acp_session_id}),
            )
            .await
    }
    pub fn replay(&self, after: u64) -> Replay {
        self.connection.replay(after)
    }
    pub async fn authenticate(&self, params: Value) -> Result<Value> {
        let _setup = self.setup_lock.lock().await;
        let metadata = self.metadata();
        let valid = metadata.initialization["authMethods"]
            .as_array()
            .is_some_and(|methods| {
                methods.iter().any(|method| {
                    method["id"] == params["methodId"]
                        && method
                            .get("type")
                            .and_then(Value::as_str)
                            .is_none_or(|kind| kind == "agent")
                })
            });
        if !valid {
            bail!("authentication method unavailable")
        }
        let result = self.connection.request("authenticate", params).await?;
        let _: acp::AuthenticateResponse =
            serde_json::from_value(result.clone()).context("invalid authentication response")?;
        if metadata.acp_session_id.is_empty() {
            let (setup, session_id) = setup_session(
                &self.connection,
                &metadata.initialization,
                &metadata.workspace,
                self.load_id.as_deref(),
                &self.mcp_servers,
                self.remote.as_ref(),
                self.trace.as_deref(),
            )
            .await?;
            let mut metadata = self.metadata.lock().unwrap();
            metadata.setup = setup;
            metadata.acp_session_id = session_id;
        }
        if let Some(catalog) = &self.catalog {
            let catalog = catalog.clone();
            let metadata = self.metadata();
            tokio::task::spawn_blocking(move || catalog.save(metadata)).await??;
        }
        Ok(result)
    }
}

async fn setup_session(
    connection: &AcpConnection,
    initialization: &Value,
    workspace: &Path,
    load: Option<&str>,
    mcp_servers: &[Value],
    remote: Option<&RemoteSetup>,
    trace: Option<&str>,
) -> Result<(Value, String)> {
    crate::mcp::validate(mcp_servers, Some(initialization))?;
    let setup = if let Some(session_id) = load {
        if initialization["agentCapabilities"]["loadSession"] != true {
            bail!("agent does not advertise loadSession")
        }
        connection
            .request(
                "session/load",
                json!({"sessionId":session_id,"cwd":workspace,"mcpServers":mcp_servers}),
            )
            .await?
    } else {
        let mut params = match remote {
            Some(remote) => json!({"cwd":remote.cwd,"mcpServers":mcp_servers}),
            None => json!({"cwd":workspace,"mcpServers":mcp_servers}),
        };
        if let Some(meta) = remote.and_then(|remote| remote.meta.as_ref()) {
            params["_meta"] = meta.clone();
        }
        if let Some(trace) = trace {
            if !params["_meta"].is_object() {
                params["_meta"] = json!({});
            }
            params["_meta"]["traceparent"] = json!(trace);
        }
        connection.request("session/new", params).await?
    };
    let session_id = if let Some(session_id) = load {
        let _: acp::LoadSessionResponse =
            serde_json::from_value(setup.clone()).context("invalid ACP session/load response")?;
        session_id.to_string()
    } else {
        let parsed: acp::NewSessionResponse =
            serde_json::from_value(setup.clone()).context("invalid ACP session/new response")?;
        parsed.session_id.to_string()
    };
    if session_id.is_empty() {
        bail!("agent returned an empty session id")
    }
    Ok((setup, session_id))
}
pub struct SessionManager {
    launch_settings: RwLock<Arc<LaunchSettings>>,
    sessions: Mutex<BTreeMap<Uuid, Arc<AcpSession>>>,
    retained: Mutex<BTreeMap<Uuid, SessionMetadata>>,
    catalog: Option<Arc<SessionCatalog>>,
    // Serializes lifecycle creation to enforce limits without holding the sessions lock across IO.
    lifecycle: Mutex<()>,
    orchestrator: std::sync::OnceLock<Arc<Orchestrator>>,
}

/// Undoes orchestrator admission if the session never finishes starting.
struct TicketGuard<'a> {
    orchestrator: &'a Orchestrator,
    task: Uuid,
    armed: bool,
}
impl Drop for TicketGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.orchestrator
                .abort(self.task, "the agent could not be started");
        }
    }
}
pub struct LaunchSettings {
    pub config: Config,
    pub registry: Registry,
}
impl SessionManager {
    pub fn launch_settings(&self) -> Arc<LaunchSettings> {
        self.launch_settings
            .read()
            .expect("launch settings lock")
            .clone()
    }
    /// Existing and in-flight sessions retain their original, coherent launch snapshot.
    pub fn reload(&self, config: Config, registry: Registry) -> Result<()> {
        config.validate()?;
        let old = self.launch_settings();
        if config.state_directory != old.config.state_directory
            || config.runtime.max_frame_bytes != old.config.runtime.max_frame_bytes
            || serde_json::to_value(&config.server)? != serde_json::to_value(&old.config.server)?
            || serde_json::to_value(&config.logging)? != serde_json::to_value(&old.config.logging)?
        {
            bail!(
                "restart_required: listener, TLS, tokens, storage, frame size and logging cannot be hot-reloaded"
            )
        }
        *self.launch_settings.write().expect("launch settings lock") =
            Arc::new(LaunchSettings { config, registry });
        Ok(())
    }
    pub fn new(config: Config, registry: Registry) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            launch_settings: RwLock::new(Arc::new(LaunchSettings { config, registry })),
            sessions: Mutex::new(BTreeMap::new()),
            retained: Mutex::new(BTreeMap::new()),
            catalog: None,
            lifecycle: Mutex::new(()),
            orchestrator: std::sync::OnceLock::new(),
        })
    }
    /// Attach the orchestrator once, right after construction. Sessions started before this are
    /// never part of a task tree.
    pub fn attach_orchestrator(&self, orchestrator: Arc<Orchestrator>) {
        let _ = self.orchestrator.set(orchestrator);
    }
    /// Start a delegated child session for `parent`. Admission runs before any process starts and
    /// refusals surface as [`crate::orchestrator::AdmissionFailure`].
    pub async fn open_task(
        &self,
        agent_id: &str,
        workspace: &Path,
        access: WorkspaceAccess,
        parent: Uuid,
    ) -> Result<Arc<AcpSession>> {
        self.open_inner(agent_id, workspace, None, Vec::new(), access, Some(parent))
            .await
    }
    /// Network hosts retain metadata. Standalone local chats stay in memory.
    pub fn persistent(config: Config, registry: Registry, state_directory: &Path) -> Result<Self> {
        let mut manager = Self::new(config, registry)?;
        let catalog = Arc::new(SessionCatalog::open(state_directory)?);
        // After an abrupt host kill, stop agent groups the earlier host left running.
        #[cfg(unix)]
        crate::process_tree::records::enable(state_directory);
        let records = catalog
            .records()
            .into_iter()
            .filter_map(|(id, mut metadata)| {
                // A changed root policy must not expose old workspace metadata.
                manager
                    .launch_settings()
                    .config
                    .workspace(&metadata.workspace)
                    .ok()?;
                metadata.status = "interrupted".into();
                Some((id, metadata))
            })
            .collect();
        manager.retained = Mutex::new(records);
        manager.catalog = Some(catalog);
        Ok(manager)
    }
    pub async fn create(&self, agent_id: &str, workspace: &Path) -> Result<Arc<AcpSession>> {
        self.open(
            agent_id,
            workspace,
            None,
            Vec::new(),
            WorkspaceAccess::default(),
        )
        .await
    }
    pub async fn load(
        &self,
        agent_id: &str,
        workspace: &Path,
        session_id: &str,
    ) -> Result<Arc<AcpSession>> {
        if session_id.is_empty() {
            bail!("ACP session id is required")
        }
        self.open(
            agent_id,
            workspace,
            Some(session_id),
            Vec::new(),
            WorkspaceAccess::default(),
        )
        .await
    }
    pub async fn open_with_mcp(
        &self,
        agent_id: &str,
        workspace: &Path,
        load: Option<&str>,
        mcp_servers: Vec<Value>,
    ) -> Result<Arc<AcpSession>> {
        if load.is_some_and(str::is_empty) {
            bail!("ACP session id is required")
        }
        self.open(
            agent_id,
            workspace,
            load,
            mcp_servers,
            WorkspaceAccess::default(),
        )
        .await
    }
    pub async fn open_configured(
        &self,
        agent_id: &str,
        workspace: &Path,
        load: Option<&str>,
        mcp_servers: Vec<Value>,
        access: WorkspaceAccess,
    ) -> Result<Arc<AcpSession>> {
        if load.is_some_and(str::is_empty) {
            bail!("ACP session id is required")
        }
        self.open(agent_id, workspace, load, mcp_servers, access)
            .await
    }
    async fn open(
        &self,
        agent_id: &str,
        workspace: &Path,
        load: Option<&str>,
        mcp_servers: Vec<Value>,
        access: WorkspaceAccess,
    ) -> Result<Arc<AcpSession>> {
        self.open_inner(agent_id, workspace, load, mcp_servers, access, None)
            .await
    }
    async fn open_inner(
        &self,
        agent_id: &str,
        workspace: &Path,
        load: Option<&str>,
        mut mcp_servers: Vec<Value>,
        access: WorkspaceAccess,
        parent: Option<Uuid>,
    ) -> Result<Arc<AcpSession>> {
        crate::mcp::validate(&mcp_servers, None)?;
        let _lifecycle = self.lifecycle.lock().await;
        let settings = self.launch_settings();
        let config = &settings.config;
        if self.sessions.lock().await.len() >= config.runtime.max_sessions {
            bail!("session limit reached; remove a session first")
        }
        let definition = settings.registry.get(agent_id)?;
        if !definition.enabled {
            bail!("agent is disabled")
        }
        // An agent on another host: no program to find or start here, and nothing local to
        // hand it (an MCP server is a command to run, and the other host will not run ours).
        let remote = definition.is_remote();
        if remote && (!mcp_servers.is_empty() || load.is_some()) {
            bail!("remote agents do not take MCP servers or load saved sessions yet")
        }
        // A delegated task may run in its own worktree under the state directory.
        let workspace = if parent.is_some() {
            config.task_workspace(workspace)?
        } else {
            config.workspace(workspace)?
        };
        let cwd = match &definition.working_directory {
            Some(path) => config.workspace(path)?,
            None => workspace.clone(),
        };
        let launch = if remote {
            None
        } else {
            Some(
                definition
                    .discover()
                    .launch
                    .context("agent executable not found")?,
            )
        };
        // Admission before any process starts; the guard undoes it if startup fails below.
        let mut ticket = None;
        let mut trace = None;
        if let Some(orchestrator) = self.orchestrator.get() {
            if let Some(issued) = orchestrator
                .begin(parent, agent_id, &workspace, access, &settings)
                .map_err(anyhow::Error::new)?
            {
                if let Some(mcp) = issued.mcp.clone()
                    && !remote
                {
                    mcp_servers.push(mcp);
                    crate::mcp::validate(&mcp_servers, None)?;
                }
                trace = issued.traceparent.clone();
                ticket = Some(TicketGuard {
                    orchestrator,
                    task: issued.task,
                    armed: true,
                });
            }
        } else if parent.is_some() {
            bail!("delegation is not available on this host")
        }
        let connection = match launch {
            Some(launch) => AcpConnection::spawn_with_access(
                definition,
                launch,
                &cwd,
                config.runtime.clone(),
                &workspace,
                access,
                config.shell_review.get(agent_id),
            )?,
            None => {
                let peer = definition
                    .peer
                    .as_deref()
                    .context("remote agent has no peer")?;
                let url = definition
                    .url
                    .as_deref()
                    .context("remote agent has no url")?;
                let store = crate::peers::PeerStore::open(&config.state_directory);
                let link =
                    crate::peer_link::connect(&store, peer, url, config.runtime.max_frame_bytes)
                        .await?;
                AcpConnection::over_transport(
                    link.reader,
                    link.writer,
                    config.runtime.clone(),
                    access,
                )
            }
        };
        let remote_setup = remote.then(|| RemoteSetup {
            cwd: definition
                .remote_cwd
                .clone()
                .unwrap_or_else(|| "/".to_owned()),
            // An acpd peer maps an alias to its own folder and grants at most what it allows;
            // asking for less is always fine. A path is never sent to it.
            meta: definition.workspace_alias.as_ref().map(|alias| {
                json!({"acpdWorkspace": alias, "acpdRights": {
                    "read": access.read_files,
                    "write": access.write_files,
                    "terminal": access.terminal}})
            }),
        });
        let setup_result = async {
            let mut initialization_request = initialize_params();
            // This host serves no file or terminal callbacks for an agent that runs elsewhere:
            // that host enforces and asks for those itself, and sends us only the permissions.
            initialization_request["clientCapabilities"] = if remote {
                json!({})
            } else {
                access.capabilities()
            };
            let initialization = connection
                .request("initialize", initialization_request)
                .await?;
            let parsed: acp::InitializeResponse = serde_json::from_value(initialization.clone())
                .context("invalid ACP initialize response")?;
            if parsed.protocol_version != acportal_protocol::ProtocolVersion::V1 {
                bail!("agent does not support ACP protocolVersion {ACP_VERSION}")
            }
            let (setup, acp_session_id) = match setup_session(
                &connection,
                &initialization,
                &workspace,
                load,
                &mcp_servers,
                remote_setup.as_ref(),
                trace.as_deref(),
            )
            .await
            {
                Ok(result) => result,
                Err(error)
                    if error
                        .downcast_ref::<AgentRpcError>()
                        .is_some_and(|error| error.0 == -32000)
                        && initialization["authMethods"]
                            .as_array()
                            .is_some_and(|methods| !methods.is_empty()) =>
                {
                    (json!({}), String::new())
                }
                Err(error) => return Err(error),
            };
            Ok::<_, anyhow::Error>((initialization, setup, acp_session_id))
        }
        .await;
        let (initialization, setup, acp_session_id) = match setup_result {
            Ok(result) => result,
            Err(error) => {
                connection.shutdown().await;
                return Err(error);
            }
        };
        let session = Arc::new(AcpSession {
            metadata: std::sync::Mutex::new(SessionMetadata {
                id: Uuid::new_v4(),
                agent_id: agent_id.into(),
                acp_session_id,
                workspace,
                initialization,
                setup,
                status: "ready".into(),
                workspace_access: access,
                own_tools: definition.own_tools,
                remote_peer: remote.then(|| definition.peer.clone()).flatten(),
            }),
            setup_lock: Mutex::new(()),
            load_id: load.map(str::to_owned),
            mcp_servers,
            remote: remote_setup,
            trace,
            catalog: self.catalog.clone(),
            connection,
        });
        if let Some(catalog) = &self.catalog {
            let catalog = catalog.clone();
            let metadata = session.metadata();
            let result = tokio::task::spawn_blocking(move || catalog.save(metadata)).await;
            if let Err(error) = result
                .map_err(anyhow::Error::from)
                .and_then(|result| result)
            {
                session.connection.shutdown().await;
                return Err(error);
            }
            let mut metadata = session.metadata();
            metadata.status = "interrupted".into();
            self.retained.lock().await.insert(metadata.id, metadata);
        }
        self.sessions
            .lock()
            .await
            .insert(session.metadata().id, session.clone());
        if let Some(mut guard) = ticket {
            guard.armed = false;
            guard.orchestrator.bind(guard.task, session.metadata().id);
        }
        tracing::info!(session_id = %session.metadata().id, "session created");
        Ok(session)
    }
    pub async fn get(&self, id: Uuid) -> Result<Arc<AcpSession>> {
        self.sessions
            .lock()
            .await
            .get(&id)
            .cloned()
            .context("session not found")
    }
    pub async fn list(&self) -> Vec<SessionMetadata> {
        let mut records = self.retained.lock().await.clone();
        for session in self.sessions.lock().await.values() {
            let metadata = session.metadata();
            records.insert(metadata.id, metadata);
        }
        records.into_values().collect()
    }
    pub async fn metadata(&self, id: Uuid) -> Result<SessionMetadata> {
        if let Ok(session) = self.get(id).await {
            return Ok(session.metadata());
        }
        self.retained
            .lock()
            .await
            .get(&id)
            .cloned()
            .context("session not found")
    }
    pub async fn active_count(&self) -> usize {
        self.sessions.lock().await.len()
    }
    pub async fn remove(&self, id: Uuid) -> Result<()> {
        // Delegated children go first, so none outlives the session that started it.
        if let Some(orchestrator) = self.orchestrator.get() {
            for child in orchestrator.release_subtree(id) {
                let _ = self.remove_one(child).await;
            }
        }
        self.remove_one(id).await
    }
    async fn remove_one(&self, id: Uuid) -> Result<()> {
        let _lifecycle = self.lifecycle.lock().await;
        let session = self.get(id).await.ok();
        let _setup = if let Some(session) = &session {
            Some(session.setup_lock.lock().await)
        } else {
            None
        };
        self.metadata(id).await?;
        if let Some(catalog) = &self.catalog {
            let catalog = catalog.clone();
            tokio::task::spawn_blocking(move || catalog.remove(id)).await??;
        }
        self.retained.lock().await.remove(&id);
        self.sessions.lock().await.remove(&id);
        if let Some(session) = &session {
            session.connection.shutdown().await;
        }
        Ok(())
    }
    pub async fn shutdown(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        if let Some(orchestrator) = self.orchestrator.get() {
            // Leave the task records as complete as they can be; whatever is open is shown as
            // interrupted after a restart either way.
            orchestrator.persist_now();
        }
        let sessions = std::mem::take(&mut *self.sessions.lock().await);
        for session in sessions.values() {
            session.connection.shutdown().await;
        }
    }
}

#[cfg(test)]
mod reload_tests {
    use super::*;
    #[test]
    fn restart_only_changes_reject_the_whole_candidate() {
        let config = Config::default();
        let registry = Registry::parse("[]").unwrap();
        let manager = SessionManager::new(config.clone(), registry.clone()).unwrap();
        for kind in 0..6 {
            let mut candidate = config.clone();
            candidate.runtime.max_sessions = 3;
            match kind {
                0 => candidate.server.listen = "127.0.0.1:9876".parse().unwrap(),
                1 => candidate.state_directory = config.state_directory.join("other"),
                2 => candidate.runtime.max_frame_bytes *= 2,
                3 => candidate.logging.frame_metadata = true,
                4 => candidate.server.token_lifetime_seconds += 1,
                _ => {
                    candidate.server.tls_certificate = Some("certificate.pem".into());
                    candidate.server.tls_private_key = Some("key.pem".into());
                }
            }
            assert!(manager.reload(candidate, registry.clone()).is_err());
            assert_eq!(
                manager.launch_settings().config.runtime.max_sessions,
                config.runtime.max_sessions
            );
        }
    }
    #[test]
    fn future_launch_settings_swap_as_one_snapshot() {
        let config = Config::default();
        let registry = Registry::parse("[]").unwrap();
        let manager = SessionManager::new(config.clone(), registry.clone()).unwrap();
        let before = manager.launch_settings();
        let mut candidate = config.clone();
        candidate.runtime.max_sessions = 1;
        candidate.runtime.request_timeout_seconds = 60;
        candidate.workspace_roots.clear();
        manager.reload(candidate, registry).unwrap();
        assert_eq!(manager.launch_settings().config.runtime.max_sessions, 1);
        assert_eq!(
            before.config.runtime.max_sessions,
            config.runtime.max_sessions
        );
        assert_eq!(
            before.config.runtime.request_timeout_seconds,
            config.runtime.request_timeout_seconds
        );
    }
}
