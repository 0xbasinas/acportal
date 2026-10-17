use crate::{
    config::Config,
    registry::{AgentDefinition, Registry},
    security::{Device, Role, SecurityStore},
    session::{AcpSession, SessionManager, SessionMetadata},
    userdata::{UserData, plain_path},
};
use acportal_protocol::{error, response, validate_message};
use anyhow::{Context, Result, bail};
use axum::{
    Extension, Json, Router,
    extract::{
        DefaultBodyLimit, Path, Query, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

mod bridge;
mod checks;
mod git;
mod pr;
mod shared;

#[derive(Clone)]
pub struct Host {
    pub config: Arc<Config>,
    pub registry: Arc<Registry>,
    pub sessions: Arc<SessionManager>,
    orchestrator: Arc<crate::orchestrator::Orchestrator>,
    bridge: Arc<bridge::BridgeState>,
    pub security: SecurityStore,
    gateways: Arc<AsyncMutex<HashMap<Uuid, Arc<Gateway>>>>,
    /// Serializes phone-initiated changes to folders (read, change, save, reload).
    user_changes: Arc<AsyncMutex<()>>,
    /// Drafts, titles and prompt history shared by the phone and the desktop.
    shared: Arc<Mutex<shared::Store>>,
    /// Modes, models and config options each agent reported in a probe (`POST /v1/agents/{id}/probe`).
    agent_setups: Arc<Mutex<HashMap<String, Value>>>,
}
struct Gateway {
    /// Current controller: lease generation and whether another client may take it over.
    controller: Mutex<Option<(u64, bool)>>,
    /// Broadcasts the controlling generation (0 = none) so a replaced controller can step down.
    owner: tokio::sync::watch::Sender<u64>,
    requests: Mutex<VecDeque<CachedRequest>>,
    cache_bytes: usize,
    result_reserve: usize,
}
struct CachedRequest {
    id: String,
    input: Value,
    complete: Option<Value>,
    reserved_bytes: usize,
}
fn reserve_cache(requests: &mut VecDeque<CachedRequest>, bytes: usize, limit: usize) -> bool {
    if bytes > limit {
        return false;
    }
    while requests.len() >= 256
        || requests
            .iter()
            .map(|request| request.reserved_bytes)
            .sum::<usize>()
            > limit - bytes
    {
        let Some(index) = requests
            .iter()
            .position(|request| request.complete.is_some())
        else {
            return false;
        };
        requests.remove(index);
    }
    true
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn entry(id: &str, bytes: usize, completed: bool) -> CachedRequest {
        CachedRequest {
            id: id.into(),
            input: json!({"id":id}),
            complete: completed.then(|| json!({"id":id,"result":{}})),
            reserved_bytes: bytes,
        }
    }

    #[test]
    fn byte_pressure_evicts_completed_only_and_keeps_pending_input() {
        let mut requests = VecDeque::from([entry("pending", 60, false), entry("done", 30, true)]);
        assert!(reserve_cache(&mut requests, 40, 100));
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].input, json!({"id":"pending"}));
        assert!(!reserve_cache(&mut requests, 41, 100));
        assert_eq!(requests[0].id, "pending");
        assert!(!reserve_cache(&mut requests, 101, 100));
    }

    #[test]
    fn count_pressure_also_keeps_pending_requests() {
        let mut requests: VecDeque<_> = (0..256)
            .map(|id| entry(&id.to_string(), 1, false))
            .collect();
        assert!(!reserve_cache(&mut requests, 1, 1000));
        requests[100].complete = Some(json!({"result":{}}));
        assert!(reserve_cache(&mut requests, 1, 1000));
        assert_eq!(requests.len(), 255);
        assert!(!requests.iter().any(|request| request.id == "100"));
        assert!(requests.iter().all(|request| request.complete.is_none()));
    }
}
struct Lease {
    gateway: Arc<Gateway>,
    generation: u64,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut controller = self.gateway.controller.lock().unwrap();
        // A lease that was taken over must not release its successor.
        if controller.is_some_and(|(generation, _)| generation == self.generation) {
            *controller = None;
            self.gateway.owner.send_replace(0);
        }
    }
}
static LEASE_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
#[derive(Clone)]
struct Authorized {
    device: Device,
    token: String,
}
#[derive(Debug)]
struct ApiError(StatusCode, &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":{"code":self.1}}))).into_response()
    }
}
type ApiResult<T> = std::result::Result<T, ApiError>;
fn internal(_: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, "host_error")
}
fn input_error(_: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, "invalid_request")
}

impl Host {
    pub fn new(mut config: Config, mut registry: Registry) -> Result<Self> {
        UserData::load(&config.state_directory)?.apply(&mut config, &mut registry);
        config.validate()?;
        let security = SecurityStore::open(&config.state_directory)?;
        let sessions = Arc::new(SessionManager::persistent(
            config.clone(),
            registry.clone(),
            security.directory(),
        )?);
        let orchestrator = crate::orchestrator::Orchestrator::new(Arc::downgrade(&sessions));
        sessions.attach_orchestrator(orchestrator.clone());
        // Tasks an earlier run left open are shown as failed by the restart; nothing is resumed.
        orchestrator.restore(&config.state_directory);
        Ok(Self {
            registry: Arc::new(registry),
            sessions,
            orchestrator,
            bridge: Arc::new(bridge::BridgeState::default()),
            security,
            gateways: Arc::new(AsyncMutex::new(HashMap::new())),
            user_changes: Arc::new(AsyncMutex::new(())),
            shared: Arc::new(Mutex::new(shared::Store::load(&config.state_directory))),
            config: Arc::new(config),
            agent_setups: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    /// The delegation orchestrator, for embedders and tests that drive it directly.
    pub fn orchestrator(&self) -> &Arc<crate::orchestrator::Orchestrator> {
        &self.orchestrator
    }
    /// Start the control channel agents' delegate processes connect to, if delegation is on.
    /// Dropping the returned endpoint stops it. `serve` calls this once; it is public so a
    /// host can be driven without a listener.
    pub fn start_control_channel(&self) -> Result<Option<crate::orchestrator::control::Endpoint>> {
        if !self.config.orchestrator.enabled {
            return Ok(None);
        }
        crate::orchestrator::control::start(self.orchestrator.clone(), &self.config.state_directory)
            .map(Some)
            .context("start the delegation control channel")
    }
    pub fn router(&self) -> Router {
        let protected = Router::new()
            .route("/v1/status", get(status))
            .route("/v1/agents", get(agents).post(add_agent))
            .route("/v1/agents/check", post(check_agent))
            .route("/v1/agents/locate", post(locate_agents))
            .route("/v1/agents/discover", get(discover_agents))
            .route("/v1/scratch", post(new_scratch))
            .route(
                "/v1/agents/{id}",
                get(agent_definition).delete(remove_agent),
            )
            .route("/v1/agents/{id}/probe", post(probe_agent))
            .route(
                "/v1/workspaces",
                get(workspaces).post(add_workspace).delete(remove_workspace),
            )
            .route("/v1/filesystem/directories", get(directories))
            .route("/v1/workspaces/browse", get(browse))
            .route("/v1/presets", get(presets))
            .route("/v1/sessions", get(sessions).post(create_session))
            .route("/v1/sessions/{id}", get(session).delete(delete_session))
            .route("/v1/sessions/{id}/connect", get(connect))
            .route("/v1/sessions/{id}/supervisor", post(set_supervisor))
            .route("/v1/tasks", get(list_tasks))
            .route("/v1/tasks/{id}", get(task_detail))
            .route("/v1/tasks/{id}/cancel", post(cancel_task))
            .route("/v1/tasks/{id}/merge", post(merge_task))
            .route("/v1/tasks/{id}/discard", post(discard_task))
            .route("/v1/tasks/{id}/supervisor/pause", post(pause_supervisor))
            .route("/v1/tasks/{id}/supervisor/resume", post(resume_supervisor))
            .route("/v1/checks", post(checks::run))
            .route("/v1/shared", get(shared::get))
            .route(
                "/v1/shared/drafts/{id}",
                axum::routing::put(shared::put_draft),
            )
            .route(
                "/v1/shared/titles/{id}",
                axum::routing::put(shared::put_title),
            )
            .route("/v1/shared/history", post(shared::add_history))
            .route("/v1/commit-message", post(git::commit_message))
            .route(
                "/v1/pull-requests",
                post(pr::create).layer(DefaultBodyLimit::max(pr::MAX_HTTP_BODY)),
            )
            .route(
                "/v1/git/{op}",
                post(git::run).layer(DefaultBodyLimit::max(git::MAX_INPUT + 64 * 1024)),
            )
            .route("/v1/device", delete(revoke_self))
            .route_layer(middleware::from_fn(phones_only))
            .route_layer(middleware::from_fn_with_state(self.clone(), authenticate));
        // Standard ACP over a WebSocket for paired peer hosts. Streamable HTTP is not offered.
        let bridge = Router::new()
            .route("/v1/peer/info", get(bridge::info))
            .route(
                "/acp",
                get(bridge::agent_required)
                    .post(bridge::not_supported)
                    .delete(bridge::not_supported),
            )
            .route(
                "/acp/{agent_id}",
                get(bridge::upgrade)
                    .post(bridge::not_supported)
                    .delete(bridge::not_supported),
            )
            .route_layer(middleware::from_fn(peers_only))
            .route_layer(middleware::from_fn_with_state(self.clone(), authenticate));
        Router::new()
            .merge(protected)
            .merge(bridge)
            .route("/v1/pair", post(pair))
            .layer(DefaultBodyLimit::max(16 * 1024))
            .layer(middleware::from_fn(no_browser_and_no_cache))
            .with_state(self.clone())
    }
    pub async fn serve(&self) -> Result<()> {
        self.serve_controlled(None, None).await
    }
    pub async fn serve_controlled(
        &self,
        stop: Option<tokio::sync::watch::Receiver<bool>>,
        ready: Option<tokio::sync::oneshot::Sender<()>>,
    ) -> Result<()> {
        self.config.validate()?;
        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        let host = self.clone();
        tokio::spawn(async move {
            match stop {
                Some(mut stop) => {
                    tokio::select! { _ = ctrl_c_or_pending() => {}, _ = stop.changed() => {} }
                }
                None => {
                    ctrl_c_or_pending().await;
                }
            }
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(5)));
            host.sessions.shutdown().await;
        });
        // Experimental delegation: the control endpoint lives exactly as long as the host serves.
        // Enabling it by reload alone is not enough; it is started here, once.
        let _control = self.start_control_channel()?;
        let address = self.config.server.listen;
        let listener = std::net::TcpListener::bind(address)
            .context("bind host listener; check address and port conflicts")?;
        listener.set_nonblocking(true)?;
        tracing::info!(listen = %listener.local_addr()?, tls = self.config.server.tls_certificate.is_some(), "host listening");
        if let (Some(certificate), Some(key)) = (
            &self.config.server.tls_certificate,
            &self.config.server.tls_private_key,
        ) {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(certificate, key)
                .await
                .context("read TLS certificate and private key")?;
            if let Some(ready) = ready {
                let _ = ready.send(());
            }
            axum_server::from_tcp_rustls(listener, tls)?
                .handle(handle)
                .serve(self.router().into_make_service())
                .await?;
        } else {
            if !address.ip().is_loopback() {
                bail!("cleartext listener must be loopback")
            }
            if let Some(ready) = ready {
                let _ = ready.send(());
            }
            axum_server::from_tcp(listener)?
                .handle(handle)
                .serve(self.router().into_make_service())
                .await?;
        }
        self.sessions.shutdown().await;
        Ok(())
    }
}
async fn ctrl_c_or_pending() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            return;
        }
    }
    // Detached Windows processes may have no console signal source.
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}
async fn no_browser_and_no_cache(request: Request, next: Next) -> Response {
    if request.headers().contains_key(header::ORIGIN) {
        return ApiError(StatusCode::FORBIDDEN, "browser_origin_not_supported").into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
/// Phone management routes are for phones only: a peer host's credential stops at `/acp`.
async fn phones_only(request: Request, next: Next) -> Response {
    match request.extensions().get::<Authorized>() {
        Some(auth) if auth.device.role == Role::Phone => next.run(request).await,
        _ => ApiError(StatusCode::FORBIDDEN, "role_not_allowed").into_response(),
    }
}
/// The agent bridge is for peer hosts only.
async fn peers_only(request: Request, next: Next) -> Response {
    match request.extensions().get::<Authorized>() {
        Some(auth) if auth.device.role == Role::Peer => next.run(request).await,
        _ => ApiError(StatusCode::FORBIDDEN, "role_not_allowed").into_response(),
    }
}
async fn authenticate(State(host): State<Host>, mut request: Request, next: Next) -> Response {
    let Some(token) = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string)
    else {
        return ApiError(StatusCode::UNAUTHORIZED, "authentication_required").into_response();
    };
    let security = host.security.clone();
    let candidate = token.clone();
    let authorized = tokio::task::spawn_blocking(move || security.authorize(&candidate)).await;
    match authorized {
        Ok(Ok(device)) => {
            request
                .extensions_mut()
                .insert(Authorized { device, token });
            next.run(request).await
        }
        _ => ApiError(StatusCode::UNAUTHORIZED, "invalid_credentials").into_response(),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairInput {
    code: String,
    device_name: String,
}
async fn pair(State(host): State<Host>, Json(input): Json<PairInput>) -> ApiResult<Json<Value>> {
    let lifetime = host.config.server.token_lifetime_seconds;
    let security = host.security.clone();
    let result = tokio::task::spawn_blocking(move || {
        security.redeem(&input.code, &input.device_name, lifetime)
    })
    .await
    .map_err(internal)?;
    let result = result.map_err(|_| ApiError(StatusCode::UNAUTHORIZED, "pairing_failed"))?;
    Ok(Json(serde_json::to_value(result).map_err(internal)?))
}
async fn status(State(host): State<Host>) -> ApiResult<Json<Value>> {
    // `orchestrator` is listed only while delegation is enabled, so the phone shows the task
    // views only then.
    let mut features = vec![
        "formElicitation",
        "git",
        "sessionWorktree",
        "checks",
        "controlTakeover",
        "sharedDrafts",
        "agentProbe",
        "agentDiscovery",
        "commitMessage",
        "pullRequests",
        "presets",
        "scratchSessions",
    ];
    if host.config.orchestrator.enabled {
        features.push("orchestrator");
    }
    Ok(Json(
        json!({"hostId":host.security.host_id().map_err(internal)?,"name":std::env::var("COMPUTERNAME").or_else(|_|std::env::var("HOSTNAME")).unwrap_or_else(|_|"Development host".into()),
        "version":env!("CARGO_PKG_VERSION"),"protocolVersion":1,"managementVersion":1,
        // Optional host behaviors a phone may opt into; older hosts omit this list.
        "features":features,"gh": if gh_found() { "found" } else { "missing" },
        "activeSessions":host.sessions.active_count().await}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetQuery {
    agent: String,
}
/// Whether `gh` can be started. `/v1/status` is polled every few seconds by every client, so the
/// PATH search is repeated at most every 30 seconds.
fn gh_found() -> bool {
    static CACHE: Mutex<Option<(std::time::Instant, bool)>> = Mutex::new(None);
    const TTL: Duration = Duration::from_secs(30);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, found)) = *cache
        && at.elapsed() < TTL
    {
        return found;
    }
    let found = crate::discovery::resolve("gh", None).is_some();
    *cache = Some((std::time::Instant::now(), found));
    found
}
/// Read-only presets for one agent. Nothing here is stored or applied to a session.
async fn presets(
    State(host): State<Host>,
    Query(query): Query<PresetQuery>,
) -> ApiResult<Json<Value>> {
    if !crate::registry::agent_id_ok(&query.agent) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_parameter"));
    }
    let definition = host
        .registry
        .get(&query.agent)
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "unknown_agent"))?;
    let facts = crate::presets::Facts {
        auto_approve: host.config.auto_approve.get(&query.agent),
        shell_review: host.config.shell_review.get(&query.agent),
        own_tools: definition.own_tools,
        remote: definition.transport == "ws" || definition.url.is_some(),
    };
    Ok(Json(json!({
        "presets": crate::presets::presets_for(&facts, crate::connection::WorkspaceAccess::default())
    })))
}
async fn agents(State(host): State<Host>) -> Json<Value> {
    let active = host.sessions.list().await;
    let settings = host.sessions.launch_settings();
    let mut discovered = discovered_agents(&settings.registry, &active);
    attach_setups(&mut discovered, &active, &host.agent_setups.lock().unwrap());
    // Agents the phone registered can be removed from the phone; the host's own cannot.
    let ours: Vec<String> = UserData::load(&settings.config.state_directory)
        .map(|data| data.agents.into_iter().map(|agent| agent.id).collect())
        .unwrap_or_default();
    if let Some(list) = discovered.as_array_mut() {
        for agent in list {
            agent["removable"] = json!(ours.iter().any(|id| agent["id"] == json!(id)));
        }
    }
    Json(discovered)
}
/// The choices an ACP agent offers for a session (`modes`, `models`, `configOptions`), as
/// reported in its last `session/new` or `session/load` reply. Anything else is left out.
fn setup_choices(setup: &Value) -> Option<Value> {
    let mut out = serde_json::Map::new();
    for key in ["modes", "models", "configOptions"] {
        if let Some(value) = setup.get(key).filter(|value| !value.is_null()) {
            out.insert(key.into(), value.clone());
        }
    }
    (!out.is_empty()).then_some(Value::Object(out))
}
/// Give each agent `setup`: the newest choices from one of its sessions, else from a probe.
fn attach_setups(
    discovered: &mut Value,
    sessions: &[SessionMetadata],
    probed: &HashMap<String, Value>,
) {
    let Some(list) = discovered.as_array_mut() else {
        return;
    };
    for agent in list {
        let Some(id) = agent["id"].as_str().map(str::to_owned) else {
            continue;
        };
        let from_session = sessions
            .iter()
            .rev()
            .filter(|session| session.agent_id == id)
            .find_map(|session| setup_choices(&session.setup));
        if let Some(setup) = from_session.or_else(|| probed.get(&id).cloned()) {
            agent["setup"] = setup;
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProbeInput {
    workspace: std::path::PathBuf,
}
/// Start the agent in a throwaway session (no prompt, no file or terminal access, no MCP
/// servers), keep the modes/models/config options it reports, and stop it again.
async fn probe_agent(
    State(host): State<Host>,
    Path(id): Path<String>,
    Json(input): Json<ProbeInput>,
) -> ApiResult<Json<Value>> {
    let access = crate::connection::WorkspaceAccess {
        read_files: false,
        write_files: false,
        terminal: false,
        form_elicitation: false,
        manual_approval: false,
    };
    let session = host
        .sessions
        .open_configured(&id, &input.workspace, None, Vec::new(), access)
        .await
        .map_err(input_error)?;
    let metadata = session.metadata();
    drop(session);
    let _ = host.sessions.remove(metadata.id).await;
    let setup = setup_choices(&metadata.setup).unwrap_or_else(|| json!({}));
    host.agent_setups.lock().unwrap().insert(id, setup.clone());
    Ok(Json(json!({ "setup": setup })))
}
fn discovered_agents(registry: &Registry, active: &[SessionMetadata]) -> Value {
    let mut discovered = serde_json::to_value(registry.discover()).unwrap();
    if let Some(agents) = discovered.as_array_mut() {
        for agent in agents {
            // Live process health cannot make a missing, disabled or invalid definition usable.
            if agent["enabled"] != true || agent["status"] != "available" {
                continue;
            }
            if active.iter().any(|session| {
                session.agent_id == agent["id"]
                    && matches!(session.status.as_str(), "ready" | "running")
            }) {
                agent["status"] = json!("running");
            } else if active.iter().any(|session| {
                session.agent_id == agent["id"] && session.status == "authentication_required"
            }) {
                agent["status"] = json!("authentication_required");
            }
        }
    }
    discovered
}

#[cfg(test)]
mod discovery_tests {
    use super::*;

    fn session(agent: &str, status: &str) -> SessionMetadata {
        serde_json::from_value(json!({"id":Uuid::new_v4(),"agentId":agent,"acpSessionId":"s","workspace":"/workspace","initialization":{},"setup":{},"status":status})).unwrap()
    }
    #[test]
    fn retained_or_stopped_records_do_not_report_running_and_auth_does_not_hide_a_ready_process() {
        let registry = Registry::parse(
            &json!([{"id":"agent","name":"Agent","command":std::env::current_exe().unwrap()}])
                .to_string(),
        )
        .unwrap();
        let retained = [
            session("agent", "interrupted"),
            session("agent", "exited"),
            session("other", "ready"),
        ];
        assert_eq!(
            discovered_agents(&registry, &retained)[0]["status"],
            "available"
        );
        let mut active = retained.to_vec();
        active.push(session("agent", "authentication_required"));
        assert_eq!(
            discovered_agents(&registry, &active)[0]["status"],
            "authentication_required"
        );
        active.push(session("agent", "ready"));
        assert_eq!(
            discovered_agents(&registry, &active)[0]["status"],
            "running"
        );
    }
    #[test]
    fn active_records_never_override_missing_disabled_or_misconfigured_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().join("agent-cwd");
        std::fs::create_dir(&cwd).unwrap();
        let executable = std::env::current_exe().unwrap();
        let registry=Registry::parse(&json!([
            {"id":"disabled","name":"Disabled","command":executable,"enabled":false},
            {"id":"missing","name":"Missing","command":directory.path().join("missing-agent.exe")},
            {"id":"invalid","name":"Invalid","command":executable,"workingDirectory":cwd}
        ]).to_string()).unwrap();
        std::fs::remove_dir(&cwd).unwrap();
        let active = [
            session("disabled", "ready"),
            session("missing", "running"),
            session("invalid", "ready"),
        ];
        let agents = discovered_agents(&registry, &active);
        let status = |id: &str| {
            agents
                .as_array()
                .unwrap()
                .iter()
                .find(|agent| agent["id"] == id)
                .unwrap()["status"]
                .as_str()
                .unwrap()
        };
        assert_eq!(status("disabled"), "available");
        assert_eq!(status("missing"), "missing");
        assert_eq!(status("invalid"), "misconfigured");
    }
}
/// The agent CLIs this host knows how to start in ACP mode (`registry/known.json`), each with
/// whether it is installed here (on `PATH` or in the usual user install folders), where, and
/// whether it is already registered. Clients fill their Add agent forms from this and keep no list
/// of their own. Nothing is run, registered or installed.
async fn discover_agents(State(host): State<Host>) -> Json<Value> {
    let registry = host.sessions.launch_settings().registry.clone();
    let list = tokio::task::spawn_blocking(move || {
        let extra = crate::discovery::known_tool_directories();
        crate::registry::Registry::known()
            .into_iter()
            .map(|agent| {
                let launch = crate::discovery::resolve_in(&agent.command, None, &extra);
                let mut entry = json!({
                    "id": agent.id, "name": agent.name, "command": agent.command,
                    "args": agent.args, "env": agent.env, "ownTools": agent.own_tools,
                    "found": launch.is_some(),
                    "registered": registry.covers(&agent.id, &agent.command),
                });
                if let Some(launch) = launch {
                    entry["executable"] = json!(plain_path(&launch.found));
                }
                entry
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(json!(list))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocateInput {
    commands: Vec<String>,
}
/// Tells the phone which of the given command names exist on this computer and where (on `PATH` or in
/// the usual user install folders), so it can offer the agents that are really installed. Names only:
/// paths, empty names and anything over 64 names are refused. Nothing is run.
async fn locate_agents(Json(input): Json<LocateInput>) -> ApiResult<Json<Value>> {
    if input.commands.len() > 64 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_request"));
    }
    let extra = crate::discovery::known_tool_directories();
    let mut found = vec![];
    for command in input.commands {
        if command.is_empty() || command.len() > 256 || command.contains(['/', '\\', '\0']) {
            return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_request"));
        }
        let launch = crate::discovery::resolve_in(&command, None, &extra);
        found.push(match launch {
            Some(launch) => {
                json!({"command":command,"found":true,"executable":plain_path(&launch.found)})
            }
            None => json!({"command":command,"found":false}),
        });
    }
    Ok(Json(json!(found)))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CheckAgentInput {
    command: String,
    #[serde(default)]
    env: std::collections::BTreeMap<String, String>,
}
/// Runs `<command> --version` once (literal arguments, no shell, no stdin, 5 s limit, at most
/// 4 KiB read) so the phone can see whether the CLI exists before registering it. The command is
/// resolved the same way a registered agent is: an absolute path, or a bare name found on PATH.
async fn check_agent(Json(input): Json<CheckAgentInput>) -> ApiResult<Json<Value>> {
    use tokio::io::AsyncReadExt;
    if input.command.trim().is_empty() || input.command.len() > 1024 || input.command.contains('\0')
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_request"));
    }
    let Some(launch) =
        crate::discovery::resolve(&input.command, input.env.get("PATH").map(String::as_str))
    else {
        return Ok(Json(json!({"found":false})));
    };
    let mut command = tokio::process::Command::new(&launch.program);
    command
        .args(&launch.prefix_args)
        .arg("--version")
        .envs(&input.env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    if let Some(path) = launch.path_with_dirs(
        input
            .env
            .get("PATH")
            .map(std::ffi::OsString::from)
            .or_else(|| std::env::var_os("PATH")),
    ) {
        command.env("PATH", path);
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn().map_err(internal)?;
    let mut output = Vec::new();
    let mut stdout = child.stdout.take().expect("piped stdout").take(4096);
    let finished = tokio::time::timeout(Duration::from_secs(5), async {
        stdout.read_to_end(&mut output).await?;
        child.wait().await
    })
    .await;
    let (version_ok, version) = match finished {
        Ok(Ok(status)) => {
            let line = String::from_utf8_lossy(&output)
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| {
                    line.chars()
                        .filter(|c| !c.is_control())
                        .take(200)
                        .collect::<String>()
                });
            (status.success(), line)
        }
        _ => {
            let _ = child.kill().await;
            (false, None)
        }
    };
    Ok(Json(
        json!({"found":true,"executable":plain_path(&launch.found),"versionOk":version_ok,"version":version}),
    ))
}
/// Registers an agent CLI the user entered on the phone. It gets the same validation as a
/// registry file. An agent defined by the host's own registry cannot be replaced from the phone.
async fn add_agent(
    State(host): State<Host>,
    Json(definition): Json<AgentDefinition>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let _guard = host.user_changes.lock().await;
    definition.validate().map_err(input_error)?;
    if crate::discovery::resolve(
        &definition.command,
        definition.env.get("PATH").map(String::as_str),
    )
    .is_none()
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "agent_not_found"));
    }
    let settings = host.sessions.launch_settings();
    let state = &settings.config.state_directory;
    let mut data = UserData::load(state).map_err(user_change_error)?;
    let ours = data.agents.iter().any(|agent| agent.id == definition.id);
    if settings.registry.contains(&definition.id) && !ours {
        return Err(ApiError(StatusCode::CONFLICT, "agent_exists"));
    }
    data.agents.retain(|agent| agent.id != definition.id);
    data.agents.push(definition.clone());
    let mut registry = settings.registry.clone();
    registry.upsert(definition.clone()).map_err(input_error)?;
    data.save(state).map_err(user_change_error)?;
    host.sessions
        .reload(settings.config.clone(), registry)
        .map_err(user_change_error)?;
    Ok((StatusCode::CREATED, Json(json!({"id":definition.id}))))
}
/// A new empty folder for a session that belongs to no project, below the host's scratch root
/// (`<state>/scratch/<unix seconds>-<random>`). Sessions may run there even when no workspace
/// root contains it; nothing else is created and the folder is never reused.
async fn new_scratch(State(host): State<Host>) -> ApiResult<(StatusCode, Json<Value>)> {
    let root = host.sessions.launch_settings().config.scratch_root();
    let path = tokio::task::spawn_blocking(move || -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(&root)?;
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or_default();
        let name = format!("{seconds}-{}", &Uuid::new_v4().simple().to_string()[..6]);
        let path = root.join(name);
        std::fs::create_dir(&path)?;
        path.canonicalize()
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"path": plain_path(&path)})),
    ))
}
/// The saved definition of an agent added from an app, so the app can edit it and save it back
/// with `POST /v1/agents`. Agents from the host's own registry file are edited on the host and
/// are refused here.
async fn agent_definition(
    State(host): State<Host>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let settings = host.sessions.launch_settings();
    let data = UserData::load(&settings.config.state_directory).map_err(user_change_error)?;
    let agent = data
        .agents
        .into_iter()
        .find(|agent| agent.id == id)
        .ok_or(ApiError(StatusCode::NOT_FOUND, "not_editable"))?;
    let mut value = serde_json::to_value(&agent).map_err(internal)?;
    // Defaults are left out when saved; an edit form needs them spelled out.
    value["sandbox"] = serde_json::to_value(agent.sandbox).map_err(internal)?;
    value["shell"] = serde_json::to_value(agent.shell).map_err(internal)?;
    value["ownTools"] = json!(agent.own_tools);
    Ok(Json(value))
}
async fn remove_agent(State(host): State<Host>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let _guard = host.user_changes.lock().await;
    let settings = host.sessions.launch_settings();
    let state = &settings.config.state_directory;
    let mut data = UserData::load(state).map_err(user_change_error)?;
    if !data.agents.iter().any(|agent| agent.id == id) {
        return Err(ApiError(StatusCode::NOT_FOUND, "not_removable"));
    }
    let mut registry = settings.registry.clone();
    // A profile of this agent stays registered. Refuse before the saved copy is deleted.
    registry.remove(&id).map_err(input_error)?;
    data.agents.retain(|agent| agent.id != id);
    data.save(state).map_err(user_change_error)?;
    host.sessions
        .reload(settings.config.clone(), registry)
        .map_err(user_change_error)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn workspaces(State(host): State<Host>) -> ApiResult<Json<Value>> {
    let settings = host.sessions.launch_settings();
    let added = UserData::load(&settings.config.state_directory)
        .map_err(internal)?
        .workspace_roots;
    let mut entries = vec![];
    for root in &settings.config.workspace_roots {
        let path = plain_path(&settings.config.workspace(root).map_err(internal)?);
        let removable = added.iter().any(|saved| plain_path(saved) == path);
        entries.push(json!({"path":path,"name":path.file_name().map(|name|name.to_string_lossy()).unwrap_or_default(),"removable":removable}));
    }
    Ok(Json(json!(entries)))
}
#[derive(Deserialize)]
struct PathInput {
    path: PathBuf,
}
fn user_change_error(error: impl std::fmt::Display) -> ApiError {
    tracing::warn!(%error, "phone-initiated change failed");
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, "host_error")
}
/// Adds a folder the paired phone chose. Only an existing directory can be added; the device
/// token already authorizes this phone to start agents on the host.
async fn add_workspace(
    State(host): State<Host>,
    Json(input): Json<PathInput>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let _guard = host.user_changes.lock().await;
    let directory = input.path.canonicalize().map_err(input_error)?;
    if !directory.is_dir() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "not_a_directory"));
    }
    let directory = plain_path(&directory);
    let settings = host.sessions.launch_settings();
    let state = &settings.config.state_directory;
    let mut data = UserData::load(state).map_err(user_change_error)?;
    if !data.workspace_roots.contains(&directory) {
        data.workspace_roots.push(directory.clone());
    }
    let mut config = settings.config.clone();
    if !config.workspace_roots.contains(&directory) {
        config.workspace_roots.push(directory.clone());
    }
    config.validate().map_err(input_error)?;
    data.save(state).map_err(user_change_error)?;
    host.sessions
        .reload(config, settings.registry.clone())
        .map_err(user_change_error)?;
    Ok((
        StatusCode::CREATED,
        Json(
            json!({"path":directory,"name":directory.file_name().map(|name|name.to_string_lossy()).unwrap_or_default(),"removable":true}),
        ),
    ))
}
/// Removes only a folder the phone added; folders from the host's own config stay.
async fn remove_workspace(
    State(host): State<Host>,
    Json(input): Json<PathInput>,
) -> ApiResult<StatusCode> {
    let _guard = host.user_changes.lock().await;
    let settings = host.sessions.launch_settings();
    let state = &settings.config.state_directory;
    let mut data = UserData::load(state).map_err(user_change_error)?;
    // The path may be spelled differently from the stored one (case, short names): match both forms.
    let mut wanted = vec![plain_path(&input.path)];
    if let Ok(canonical) = input.path.canonicalize() {
        wanted.push(plain_path(&canonical));
    }
    let before = data.workspace_roots.len();
    data.workspace_roots
        .retain(|saved| !wanted.contains(&plain_path(saved)));
    if data.workspace_roots.len() == before {
        return Err(ApiError(StatusCode::NOT_FOUND, "not_removable"));
    }
    let mut config = settings.config.clone();
    config
        .workspace_roots
        .retain(|root| !wanted.contains(&plain_path(root)));
    data.save(state).map_err(user_change_error)?;
    host.sessions
        .reload(config, settings.registry.clone())
        .map_err(user_change_error)?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
struct DirectoryInput {
    #[serde(default)]
    path: Option<PathBuf>,
}
/// Lists directories anywhere on the host so the phone can pick a folder to add. Directory names
/// only (no file names or contents); an empty path lists the drives, or `/` on Unix.
async fn directories(Query(input): Query<DirectoryInput>) -> ApiResult<Json<Value>> {
    let requested = input.path.filter(|path| !path.as_os_str().is_empty());
    let Some(requested) = requested else {
        let mut roots = vec![];
        if cfg!(windows) {
            for letter in b'A'..=b'Z' {
                let drive = PathBuf::from(format!("{}:\\", letter as char));
                if drive.exists() {
                    roots.push(json!({"name":format!("{}:", letter as char),"path":drive}));
                }
            }
        } else {
            roots.push(json!({"name":"/","path":"/"}));
        }
        return Ok(Json(json!({"path":null,"parent":null,"entries":roots})));
    };
    let directory = requested.canonicalize().map_err(input_error)?;
    if !directory.is_dir() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "not_a_directory"));
    }
    let mut entries = vec![];
    for entry in std::fs::read_dir(&directory).map_err(internal)?.take(4096) {
        let Ok(entry) = entry else { continue };
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            entries.push(json!({"name":entry.file_name().to_string_lossy(),"path":plain_path(&entry.path())}));
        }
    }
    entries.sort_by_key(|entry| entry["name"].as_str().unwrap_or_default().to_lowercase());
    let parent = directory.parent().map(plain_path);
    Ok(Json(
        json!({"path":plain_path(&directory),"parent":parent,"entries":entries}),
    ))
}
#[derive(Deserialize)]
struct BrowseInput {
    path: PathBuf,
}
async fn browse(
    State(host): State<Host>,
    Query(input): Query<BrowseInput>,
) -> ApiResult<Json<Value>> {
    let settings = host.sessions.launch_settings();
    let directory = settings
        .config
        .workspace(&input.path)
        .map_err(input_error)?;
    let mut entries = vec![];
    for entry in std::fs::read_dir(directory).map_err(internal)?.take(1024) {
        let entry = entry.map_err(internal)?;
        if let Ok(path) = settings.config.workspace(&entry.path()) {
            entries
                .push(json!({"path":plain_path(&path),"name":entry.file_name().to_string_lossy()}));
        }
    }
    entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(Json(json!(entries)))
}
async fn sessions(State(host): State<Host>) -> Json<Value> {
    Json(json!(host.sessions.list().await))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateInput {
    agent_id: String,
    workspace: PathBuf,
    #[serde(default)]
    load_session_id: Option<String>,
    #[serde(default)]
    mcp_servers: Vec<Value>,
    #[serde(default)]
    workspace_access: crate::connection::WorkspaceAccess,
    /// Start on a branch, optionally in its own worktree (feature `sessionWorktree`).
    #[serde(default)]
    branch: Option<git::SessionBranch>,
}
async fn create_session(
    State(host): State<Host>,
    Json(input): Json<CreateInput>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let workspace = match &input.branch {
        Some(branch) => git::prepare_session(&host, &input.workspace, branch).await?,
        None => input.workspace.clone(),
    };
    let session = host
        .sessions
        .open_configured(
            &input.agent_id,
            &workspace,
            input.load_session_id.as_deref(),
            input.mcp_servers,
            input.workspace_access,
        )
        .await
        .map_err(|error| {
            if error.is::<crate::mcp::UnsupportedTransport>() {
                ApiError(StatusCode::BAD_REQUEST, "unsupported_mcp_transport")
            } else {
                input_error(error)
            }
        })?;
    Ok((StatusCode::CREATED, Json(json!(session.metadata()))))
}
async fn session(State(host): State<Host>, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let session = host
        .sessions
        .metadata(id)
        .await
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "session_not_found"))?;
    Ok(Json(json!(session)))
}
async fn delete_session(State(host): State<Host>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    host.sessions
        .remove(id)
        .await
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "session_not_found"))?;
    host.gateways.lock().await.remove(&id);
    shared::forget_session(&host, id);
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SupervisorInput {
    paused: bool,
}
/// Pause or resume supervisor approvals for the delegated tasks of a phone-started session.
/// Pending requests stay pending for the phone either way.
async fn set_supervisor(
    State(host): State<Host>,
    Path(id): Path<Uuid>,
    Json(input): Json<SupervisorInput>,
) -> ApiResult<StatusCode> {
    if host.orchestrator.set_supervisor_paused(id, input.paused) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(StatusCode::NOT_FOUND, "no_delegated_tasks"))
    }
}
/// Every task this host knows about, oldest first.
async fn list_tasks(State(host): State<Host>) -> Json<Value> {
    Json(Value::Array(host.orchestrator.task_list()))
}
fn task_id(id: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| ApiError(StatusCode::NOT_FOUND, "task_not_found"))
}
async fn task_detail(State(host): State<Host>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut view = host
        .orchestrator
        .task_detail(task_id(&id)?)
        .ok_or(ApiError(StatusCode::NOT_FOUND, "task_not_found"))?;
    // What the task's worktree changed, shell-made edits included. While the worktree is on disk
    // that is base..working tree; once it is removed, base..branch in the main repository.
    let w = &view["worktree"];
    let path = w["path"].as_str().map(PathBuf::from);
    let repo = w["repository"].as_str().map(PathBuf::from);
    let base = w["base"].as_str().map(str::to_owned);
    let branch = w["branch"].as_str().map(str::to_owned);
    let target = match (path, repo, branch) {
        (Some(p), _, _) if p.is_dir() => Some((p, None)),
        (_, Some(r), Some(b)) if r.is_dir() => Some((r, Some(b))),
        _ => None,
    };
    if let (Some((folder, branch)), Some(base)) = (target, base) {
        let changes = tokio::task::spawn_blocking(move || {
            git::worktree_changes(&folder, &base, branch.as_deref())
        })
        .await
        .unwrap_or_default();
        view["worktreeChanges"] = json!(changes);
    }
    Ok(Json(view))
}
/// Cancel a task and what is still running below it.
async fn cancel_task(State(host): State<Host>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    use crate::orchestrator::views::CancelRefusal;
    match host.orchestrator.cancel_task(task_id(&id)?).await {
        Ok(cancelled) => Ok(Json(json!({ "cancelled": cancelled }))),
        Err(CancelRefusal::Unknown) => Err(ApiError(StatusCode::NOT_FOUND, "task_not_found")),
        Err(CancelRefusal::NotOpen) => Err(ApiError(StatusCode::CONFLICT, "task_not_open")),
    }
}
/// The phone merges a finished task's worktree branch into the parent's checkout. A refusal is
/// 404 or 409 with a code; a conflict also lists the conflicting files.
async fn merge_task(State(host): State<Host>, Path(id): Path<String>) -> Response {
    let id = match task_id(&id) {
        Ok(id) => id,
        Err(error) => return error.into_response(),
    };
    match host.orchestrator.merge_task(id).await {
        Ok(merged) => Json(merged).into_response(),
        Err(refusal) => {
            let (status, body) = merge_refusal_response(&refusal);
            (status, Json(body)).into_response()
        }
    }
}
/// The phone throws a finished task's work away (its worktree and branch). Refusals as for a merge.
async fn discard_task(State(host): State<Host>, Path(id): Path<String>) -> Response {
    let id = match task_id(&id) {
        Ok(id) => id,
        Err(error) => return error.into_response(),
    };
    match host.orchestrator.discard_task(id).await {
        Ok(done) => Json(done).into_response(),
        Err(refusal) => {
            let (status, body) = merge_refusal_response(&refusal);
            (status, Json(body)).into_response()
        }
    }
}
/// The most conflicting files a refused merge names. A conflict can touch hundreds; the phone
/// shows a handful, and the body stays small enough for its error reader.
const MERGE_CONFLICT_FILES: usize = 20;
/// HTTP status and JSON body for a refused merge.
fn merge_refusal_response(
    refusal: &crate::orchestrator::views::MergeTaskRefusal,
) -> (StatusCode, Value) {
    use crate::orchestrator::{views::MergeTaskRefusal, worktree::MergeRefusal};
    let status = match refusal {
        MergeTaskRefusal::Unknown => StatusCode::NOT_FOUND,
        MergeTaskRefusal::Git(MergeRefusal::GitFailed) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::CONFLICT,
    };
    let mut error = json!({"code": refusal.code()});
    if let MergeTaskRefusal::Conflict {
        files,
        branch,
        repository,
    } = refusal
    {
        if !files.is_empty() {
            error["files"] = json!(files.iter().take(MERGE_CONFLICT_FILES).collect::<Vec<_>>());
        }
        // What the owner needs to merge by hand on the host.
        error["branch"] = json!(branch);
        error["repository"] = json!(repository);
    }
    (status, json!({ "error": error }))
}
fn supervisor(host: &Host, id: &str, paused: bool) -> ApiResult<StatusCode> {
    use crate::orchestrator::views::PauseRefusal;
    match host
        .orchestrator
        .set_supervisor_paused_for_task(task_id(id)?, paused)
    {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(PauseRefusal::Unknown) => Err(ApiError(StatusCode::NOT_FOUND, "task_not_found")),
        Err(PauseRefusal::NotARoot) => Err(ApiError(StatusCode::CONFLICT, "not_a_root_task")),
    }
}
async fn pause_supervisor(
    State(host): State<Host>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    supervisor(&host, &id, true)
}
async fn resume_supervisor(
    State(host): State<Host>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    supervisor(&host, &id, false)
}
async fn revoke_self(
    State(host): State<Host>,
    Extension(auth): Extension<Authorized>,
) -> ApiResult<StatusCode> {
    tokio::task::spawn_blocking(move || host.security.revoke(auth.device.id))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ConnectInput {
    #[serde(default)]
    after: u64,
    #[serde(default)]
    read_only: bool,
    /// Take control from another client app (desktop or phone) that holds the lease. The
    /// replaced client receives `control_taken` and continues as an observer.
    #[serde(default)]
    takeover: bool,
}
async fn connect(
    State(host): State<Host>,
    Path(id): Path<Uuid>,
    Query(input): Query<ConnectInput>,
    Extension(auth): Extension<Authorized>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    if !headers
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|value| value.trim() == "acpd.v1"))
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "websocket_subprotocol_required",
        ));
    }
    let session = host
        .sessions
        .get(id)
        .await
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "session_not_found"))?;
    let gateway = gateway_for(&host, id).await;
    let lease = if input.read_only {
        None
    } else {
        Some(
            acquire_lease(&gateway, true, input.takeover)
                .ok_or(ApiError(StatusCode::CONFLICT, "session_has_controller"))?,
        )
    };
    let limit = host.config.runtime.max_frame_bytes;
    Ok(ws
        .protocols(["acpd.v1"])
        .max_message_size(limit)
        .max_frame_size(limit)
        .on_upgrade(move |socket| {
            socket_session(socket, host, session, gateway, lease, auth, input)
        })
        .into_response())
}
/// The per-session gateway: the single-controller lease and the request de-duplication cache.
async fn gateway_for(host: &Host, id: Uuid) -> Arc<Gateway> {
    host.gateways
        .lock()
        .await
        .entry(id)
        .or_insert_with(|| {
            Arc::new(Gateway {
                controller: Mutex::new(None),
                owner: tokio::sync::watch::Sender::new(0),
                requests: Mutex::new(VecDeque::new()),
                cache_bytes: (8 * 1024 * 1024).max(host.config.runtime.max_frame_bytes * 4 + 4096),
                result_reserve: host.config.runtime.max_frame_bytes * 2 + 1024,
            })
        })
        .clone()
}
/// Take the controller lease, or `None` if someone else (a phone, a peer) already has it.
/// Peer bridge leases never yield to a takeover.
fn take_lease(gateway: &Arc<Gateway>) -> Option<Lease> {
    acquire_lease(gateway, false, false)
}
/// Take the lease. With `takeover`, a yielding holder (a client app) is replaced; a peer
/// bridge holder is not.
fn acquire_lease(gateway: &Arc<Gateway>, yields: bool, takeover: bool) -> Option<Lease> {
    let mut controller = gateway.controller.lock().unwrap();
    match *controller {
        None => {}
        Some((_, true)) if takeover => {}
        Some(_) => return None,
    }
    let generation = LEASE_GENERATION.fetch_add(1, Ordering::Relaxed);
    *controller = Some((generation, yields));
    gateway.owner.send_replace(generation);
    Some(Lease {
        gateway: gateway.clone(),
        generation,
    })
}
async fn send_json(
    socket: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    value: Value,
) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(5),
            socket.send(Message::Text(value.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    )
}
async fn socket_session(
    socket: WebSocket,
    host: Host,
    session: Arc<AcpSession>,
    gateway: Arc<Gateway>,
    mut lease: Option<Lease>,
    auth: Authorized,
    input: ConnectInput,
) {
    let mut owner = gateway.owner.subscribe();
    let mut events = session.connection.subscribe();
    let replay = session.replay(input.after);
    let mut cursor = replay.latest_sequence;
    let (mut outgoing, mut incoming) = socket.split();
    // Send events individually so the bounded history cannot become one giant WS frame.
    if !send_json(&mut outgoing, json!({"type":"replay_start","gap":replay.gap,"latestSequence":cursor,"processing":session.connection.is_busy()})).await { return }
    for event in replay.events {
        if !send_json(&mut outgoing, json!({"type":"event","sequence":event.sequence,"direction":event.direction,"message":event.message})).await { return }
    }
    for permission in replay.pending_permissions {
        if !send_json(
            &mut outgoing,
            json!({"type":"pending_permission","message":permission}),
        )
        .await
        {
            return;
        }
    }
    if !send_json(
        &mut outgoing,
        json!({"type":"replay_complete","latestSequence":cursor}),
    )
    .await
    {
        return;
    }
    let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
    let mut last_received = tokio::time::Instant::now();
    loop {
        tokio::select! {
            event = events.recv() => {
                match event {
                    Ok(event) if event.sequence > cursor => {
                        cursor = event.sequence;
                        if !send_json(&mut outgoing, json!({"type":"event","sequence":event.sequence,"direction":event.direction,"message":event.message})).await { break }
                    }
                    Ok(_) => {},
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let _ = send_json(&mut outgoing, json!({"type":"reconnect_required","reason":"consumer_lagged"})).await; break;
                    }
                    Err(_) => break,
                }
            }
            message = incoming.next() => {
                last_received = tokio::time::Instant::now();
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if lease.is_none() { let _ = send_json(&mut outgoing, json!({"type":"error","code":"read_only"})).await; continue }
                        let value = serde_json::from_str::<Value>(&text);
                        let value = match value { Ok(value) if validate_message(&value).is_ok() => value, _ => { let _ = send_json(&mut outgoing, json!({"type":"error","code":"invalid_acp_message"})).await; continue } };
                        let security = host.security.clone(); let token = auth.token.clone();
                        if !matches!(tokio::task::spawn_blocking(move ||security.authorize(&token)).await, Ok(Ok(_))) {
                            let _ = send_json(&mut outgoing, json!({"type":"error","code":"credentials_revoked_or_expired"})).await; break;
                        }
                        dispatch(session.clone(), gateway.clone(), value);
                    }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Ping(_))) => {},
                    _ => break,
                }
            }
            _ = heartbeat.tick() => {
                let security = host.security.clone(); let token = auth.token.clone();
                if !matches!(tokio::task::spawn_blocking(move || security.authorize(&token)).await, Ok(Ok(_))) {
                    let _ = send_json(&mut outgoing, json!({"type":"error","code":"credentials_revoked_or_expired"})).await; break;
                }
                if last_received.elapsed() > Duration::from_secs(40) { break }
                if !matches!(tokio::time::timeout(Duration::from_secs(5), outgoing.send(Message::Ping(vec![].into()))).await, Ok(Ok(()))) { break }
            }
            changed = owner.changed(), if lease.is_some() => {
                let replaced = changed.is_err() || lease.as_ref().is_some_and(|lease| *owner.borrow_and_update() != lease.generation);
                if replaced {
                    // Step down to an observer; the successor already holds the lease.
                    lease = None;
                    if !send_json(&mut outgoing, json!({"type":"control_taken"})).await { break }
                }
            }
            reason = session.connection.wait_closed() => {
                // All already-published outcomes are replayable after a close.
                let _ = send_json(&mut outgoing, json!({"type":"session_closed","reason":reason})).await; break;
            }
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), outgoing.close()).await;
    // Drain the peer's close acknowledgement (or remaining buffered input) before dropping TCP.
    // On Windows an unread client frame can otherwise turn an authorization close into a reset
    // and discard the explanatory error frame. No closing-state input is dispatched.
    let _ = tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(message) = incoming.next().await {
            if matches!(message, Ok(Message::Close(_)) | Err(_)) {
                break;
            }
        }
    })
    .await;
    // Drop releases only the controller lease. Host session and pending RPC jobs continue.
}
fn dispatch(session: Arc<AcpSession>, gateway: Arc<Gateway>, message: Value) {
    let id = message.get("id").cloned();
    let Some(method) = message["method"].as_str() else {
        tokio::spawn(async move {
            let result = session
                .connection
                .answer(message["id"].clone(), message["result"].clone())
                .await;
            if result.is_ok() {
                session.connection.record(message, "client");
            } else {
                session.connection.record(
                    error(
                        message["id"].clone(),
                        -32602,
                        "Permission response rejected",
                    ),
                    "host",
                );
            }
        });
        return;
    };
    let method = method.to_string();
    let metadata = session.metadata();
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    if method.starts_with("session/") && params["sessionId"] != metadata.acp_session_id {
        if let Some(id) = id {
            session.connection.record(
                error(id, -32602, "Session id does not match attached session"),
                "agent",
            );
        }
        return;
    }
    let allowed = match method.as_str() {
        "session/prompt" | "session/cancel" => !metadata.acp_session_id.is_empty(),
        "session/set_mode" => metadata.setup.get("modes").is_some(),
        "session/set_model" => metadata.setup.get("models").is_some(),
        "session/set_config_option" => metadata.setup.get("configOptions").is_some(),
        "authenticate" => metadata.initialization["authMethods"]
            .as_array()
            .is_some_and(|methods| {
                methods.iter().any(|method| {
                    method["id"] == params["methodId"]
                        && method
                            .get("type")
                            .and_then(Value::as_str)
                            .is_none_or(|kind| kind == "agent")
                })
            }),
        "logout" => metadata.initialization["agentCapabilities"]["auth"]
            .get("logout")
            .is_some_and(Value::is_object),
        method if method.starts_with('_') => true,
        _ => false,
    };
    if !allowed || (id.is_none() && method != "session/cancel") {
        if let Some(id) = id {
            session.connection.record(
                error(id, -32601, "Method unavailable for this session"),
                "agent",
            );
        }
        return;
    }
    if let Some(id) = &id {
        let mut requests = gateway.requests.lock().unwrap();
        let key = id.to_string();
        if let Some(previous) = requests.iter().find(|request| request.id == key) {
            if previous.input != message {
                session.connection.record(
                    error(id.clone(), -32600, "Request id reused with different input"),
                    "agent",
                );
            } else if let Some(result) = &previous.complete {
                session.connection.record(result.clone(), "agent");
            }
            return;
        }
        let reserved_bytes = message.to_string().len() + key.len() + gateway.result_reserve;
        if !reserve_cache(&mut requests, reserved_bytes, gateway.cache_bytes) {
            session.connection.record(
                error(
                    id.clone(),
                    -32603,
                    "Request cache capacity reached; request was not started",
                ),
                "agent",
            );
            return;
        }
        requests.push_back(CachedRequest {
            id: key,
            input: message.clone(),
            complete: None,
            reserved_bytes,
        });
    }
    session.connection.record(message, "client");
    tokio::spawn(async move {
        if let Some(id) = id {
            let result = if method == "authenticate" {
                session.authenticate(params).await
            } else {
                session.connection.request(&method, params).await
            };
            let mut result = match result {
                Ok(value) => response(id.clone(), value),
                // Keep the agent's own code (for example ACP auth_required, -32000) but
                // never its message; host-side failures are internal errors, not -32000.
                Err(failure) => error(
                    id.clone(),
                    failure
                        .downcast_ref::<crate::connection::AgentRpcError>()
                        .map_or(-32603, |failure| failure.0),
                    "Agent request failed",
                ),
            };
            if result.to_string().len() > gateway.result_reserve {
                result = error(
                    id.clone(),
                    -32603,
                    "Agent response exceeds retained response limit",
                );
            }
            let key = id.to_string();
            if let Some(cached) = gateway
                .requests
                .lock()
                .unwrap()
                .iter_mut()
                .find(|request| request.id == key)
            {
                cached.complete = Some(result.clone());
            }
            session.connection.record(result, "agent");
        } else {
            let _ = session.cancel().await;
        }
    });
}

#[cfg(test)]
mod merge_response_tests {
    use super::*;
    use crate::orchestrator::{views::MergeTaskRefusal, worktree::MergeRefusal};

    fn conflict(count: usize) -> MergeTaskRefusal {
        MergeTaskRefusal::Conflict {
            files: (0..count).map(|n| format!("src/file{n}.rs")).collect(),
            branch: "acpd/task-0123456789ab".into(),
            repository: "/home/me/project".into(),
        }
    }

    #[test]
    fn a_conflict_names_its_files_but_never_more_than_twenty() {
        let (status, body) = merge_refusal_response(&conflict(3));
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "merge_conflict");
        assert_eq!(
            body["error"]["files"],
            json!(["src/file0.rs", "src/file1.rs", "src/file2.rs"])
        );
        // Exactly at the cap: all of them. One over, and far over: the first twenty, in order.
        let at = merge_refusal_response(&conflict(20)).1;
        assert_eq!(at["error"]["files"].as_array().unwrap().len(), 20);
        for count in [21, 500] {
            let body = merge_refusal_response(&conflict(count)).1;
            let files = body["error"]["files"].as_array().unwrap();
            assert_eq!(files.len(), 20, "{count} conflicting files");
            assert_eq!(files[0], "src/file0.rs");
            assert_eq!(files[19], "src/file19.rs");
        }
        // The body a phone reads stays far under the 8 KiB it reads, even with long paths.
        let long = MergeTaskRefusal::Conflict {
            files: (0..500)
                .map(|n| format!("{}/{n}.rs", "d".repeat(200)))
                .collect(),
            branch: "acpd/task-0123456789ab".into(),
            repository: "/home/me/project".into(),
        };
        assert!(merge_refusal_response(&long).1.to_string().len() < 8192);
    }

    #[test]
    fn other_refusals_carry_a_code_and_the_right_status_and_no_file_list() {
        for (refusal, status, code) in [
            (
                MergeTaskRefusal::Unknown,
                StatusCode::NOT_FOUND,
                "task_not_found",
            ),
            (
                MergeTaskRefusal::Git(MergeRefusal::GitFailed),
                StatusCode::INTERNAL_SERVER_ERROR,
                "git_failed",
            ),
            (
                MergeTaskRefusal::Git(MergeRefusal::ParentDirty),
                StatusCode::CONFLICT,
                "parent_dirty",
            ),
            (
                MergeTaskRefusal::AlreadyMerged,
                StatusCode::CONFLICT,
                "already_merged",
            ),
            (
                MergeTaskRefusal::AlreadyDiscarded,
                StatusCode::CONFLICT,
                "already_discarded",
            ),
            (
                MergeTaskRefusal::NothingKept,
                StatusCode::CONFLICT,
                "nothing_to_discard",
            ),
        ] {
            let (got, body) = merge_refusal_response(&refusal);
            assert_eq!(got, status, "{code}");
            assert_eq!(body["error"]["code"], code);
            assert!(body["error"].get("files").is_none(), "{code}");
        }
        // A conflict git reported no names for has no list at all (not an empty one), but still
        // says which branch and repository to merge by hand.
        let (_, body) = merge_refusal_response(&conflict(0));
        assert!(body["error"].get("files").is_none());
        assert_eq!(body["error"]["branch"], "acpd/task-0123456789ab");
        assert_eq!(body["error"]["repository"], "/home/me/project");
    }
}
