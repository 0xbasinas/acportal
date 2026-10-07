use crate::{
    config::Config,
    connection::{AcpConnection, AgentRpcError, Replay, WorkspaceAccess},
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
    sync::Arc,
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
}
pub struct AcpSession {
    metadata: std::sync::Mutex<SessionMetadata>,
    setup_lock: Mutex<()>,
    load_id: Option<String>,
    mcp_servers: Vec<Value>,
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
        let result = self
            .connection
            .request(
                "session/prompt",
                json!({"sessionId":metadata.acp_session_id,
            "prompt":[{"type":"text","text":text}]}),
            )
            .await?;
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
        connection
            .request(
                "session/new",
                json!({"cwd":workspace,"mcpServers":mcp_servers}),
            )
            .await?
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
    config: Config,
    registry: Registry,
    sessions: Mutex<BTreeMap<Uuid, Arc<AcpSession>>>,
    retained: Mutex<BTreeMap<Uuid, SessionMetadata>>,
    catalog: Option<Arc<SessionCatalog>>,
    // Serializes lifecycle creation to enforce limits without holding the sessions lock across IO.
    lifecycle: Mutex<()>,
}
impl SessionManager {
    pub fn new(config: Config, registry: Registry) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            registry,
            sessions: Mutex::new(BTreeMap::new()),
            retained: Mutex::new(BTreeMap::new()),
            catalog: None,
            lifecycle: Mutex::new(()),
        })
    }
    /// Network hosts retain metadata. Standalone local chats stay in memory.
    pub fn persistent(config: Config, registry: Registry, state_directory: &Path) -> Result<Self> {
        let mut manager = Self::new(config, registry)?;
        let catalog = Arc::new(SessionCatalog::open(state_directory)?);
        let records = catalog
            .records()
            .into_iter()
            .filter_map(|(id, mut metadata)| {
                // A changed root policy must not expose old workspace metadata.
                manager.config.workspace(&metadata.workspace).ok()?;
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
        crate::mcp::validate(&mcp_servers, None)?;
        let _lifecycle = self.lifecycle.lock().await;
        if self.sessions.lock().await.len() >= self.config.runtime.max_sessions {
            bail!("session limit reached; remove a session first")
        }
        let definition = self.registry.get(agent_id)?;
        if !definition.enabled {
            bail!("agent is disabled")
        }
        let workspace = self.config.workspace(workspace)?;
        let cwd = match &definition.working_directory {
            Some(path) => self.config.workspace(path)?,
            None => workspace.clone(),
        };
        let discovery = definition.discover();
        let executable = discovery.executable.context("agent executable not found")?;
        let connection = AcpConnection::spawn_with_access(
            definition,
            &executable,
            &cwd,
            self.config.runtime.clone(),
            &workspace,
            access,
        )?;
        let setup_result = async {
            let mut initialization_request = initialize_params();
            initialization_request["clientCapabilities"] = access.capabilities();
            let initialization = connection
                .request("initialize", initialization_request)
                .await?;
            let parsed: acp::InitializeResponse = serde_json::from_value(initialization.clone())
                .context("invalid ACP initialize response")?;
            if parsed.protocol_version != acportal_protocol::ProtocolVersion::V1 {
                bail!("agent does not support ACP protocolVersion {ACP_VERSION}")
            }
            let (setup, acp_session_id) =
                match setup_session(&connection, &initialization, &workspace, load, &mcp_servers)
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
            }),
            setup_lock: Mutex::new(()),
            load_id: load.map(str::to_owned),
            mcp_servers,
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
        let sessions = std::mem::take(&mut *self.sessions.lock().await);
        for session in sessions.values() {
            session.connection.shutdown().await;
        }
    }
}
