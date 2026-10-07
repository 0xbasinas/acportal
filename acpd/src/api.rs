use crate::{
    config::Config,
    registry::Registry,
    security::{Device, SecurityStore},
    session::{AcpSession, SessionManager, SessionMetadata},
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
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct Host {
    pub config: Arc<Config>,
    pub registry: Arc<Registry>,
    pub sessions: Arc<SessionManager>,
    pub security: SecurityStore,
    gateways: Arc<AsyncMutex<HashMap<Uuid, Arc<Gateway>>>>,
}
struct Gateway {
    controller: AtomicBool,
    requests: Mutex<VecDeque<CachedRequest>>,
}
struct CachedRequest {
    id: String,
    input: Value,
    complete: Option<Value>,
}
struct Lease {
    gateway: Arc<Gateway>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.gateway.controller.store(false, Ordering::Release);
    }
}
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
    pub fn new(config: Config, registry: Registry) -> Result<Self> {
        config.validate()?;
        let security = SecurityStore::open(&config.state_directory)?;
        let sessions = Arc::new(SessionManager::persistent(
            config.clone(),
            registry.clone(),
            security.directory(),
        )?);
        Ok(Self {
            config: Arc::new(config),
            registry: Arc::new(registry),
            sessions,
            security,
            gateways: Arc::new(AsyncMutex::new(HashMap::new())),
        })
    }
    pub fn router(&self) -> Router {
        let protected = Router::new()
            .route("/v1/status", get(status))
            .route("/v1/agents", get(agents))
            .route("/v1/workspaces", get(workspaces))
            .route("/v1/workspaces/browse", get(browse))
            .route("/v1/sessions", get(sessions).post(create_session))
            .route("/v1/sessions/{id}", get(session).delete(delete_session))
            .route("/v1/sessions/{id}/connect", get(connect))
            .route("/v1/device", delete(revoke_self))
            .route_layer(middleware::from_fn_with_state(self.clone(), authenticate));
        Router::new()
            .merge(protected)
            .route("/v1/pair", post(pair))
            .layer(DefaultBodyLimit::max(16 * 1024))
            .layer(middleware::from_fn(no_browser_and_no_cache))
            .with_state(self.clone())
    }
    pub async fn serve(&self) -> Result<()> {
        self.config.validate()?;
        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        let host = self.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown_handle.graceful_shutdown(Some(Duration::from_secs(5)));
            host.sessions.shutdown().await;
        });
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
            axum_server::from_tcp_rustls(listener, tls)?
                .handle(handle)
                .serve(self.router().into_make_service())
                .await?;
        } else {
            if !address.ip().is_loopback() {
                bail!("cleartext listener must be loopback")
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
    Ok(Json(
        json!({"hostId":host.security.host_id().map_err(internal)?,"name":std::env::var("COMPUTERNAME").or_else(|_|std::env::var("HOSTNAME")).unwrap_or_else(|_|"Development host".into()),
        "version":env!("CARGO_PKG_VERSION"),"protocolVersion":1,"managementVersion":1,"activeSessions":host.sessions.active_count().await}),
    ))
}
async fn agents(State(host): State<Host>) -> Json<Value> {
    let active = host.sessions.list().await;
    Json(discovered_agents(&host.registry, &active))
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
async fn workspaces(State(host): State<Host>) -> ApiResult<Json<Value>> {
    let mut entries = vec![];
    for root in &host.config.workspace_roots {
        let path = host.config.workspace(root).map_err(internal)?;
        entries.push(json!({"path":path,"name":path.file_name().map(|name|name.to_string_lossy()).unwrap_or_default()}));
    }
    Ok(Json(json!(entries)))
}
#[derive(Deserialize)]
struct BrowseInput {
    path: PathBuf,
}
async fn browse(
    State(host): State<Host>,
    Query(input): Query<BrowseInput>,
) -> ApiResult<Json<Value>> {
    let directory = host.config.workspace(&input.path).map_err(input_error)?;
    let mut entries = vec![];
    for entry in std::fs::read_dir(directory).map_err(internal)?.take(1024) {
        let entry = entry.map_err(internal)?;
        if let Ok(path) = host.config.workspace(&entry.path()) {
            entries.push(json!({"path":path,"name":entry.file_name().to_string_lossy()}));
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
}
async fn create_session(
    State(host): State<Host>,
    Json(input): Json<CreateInput>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let session = host
        .sessions
        .open_configured(
            &input.agent_id,
            &input.workspace,
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
    Ok(StatusCode::NO_CONTENT)
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
    let gateway = host
        .gateways
        .lock()
        .await
        .entry(id)
        .or_insert_with(|| {
            Arc::new(Gateway {
                controller: AtomicBool::new(false),
                requests: Mutex::new(VecDeque::new()),
            })
        })
        .clone();
    let lease = if input.read_only {
        None
    } else {
        if gateway
            .controller
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ApiError(StatusCode::CONFLICT, "session_has_controller"));
        }
        Some(Lease {
            gateway: gateway.clone(),
        })
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
    _lease: Option<Lease>,
    auth: Authorized,
    input: ConnectInput,
) {
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
                        if input.read_only { let _ = send_json(&mut outgoing, json!({"type":"error","code":"read_only"})).await; continue }
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
                .permission(message["id"].clone(), message["result"]["outcome"].clone())
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
        if requests.len() >= 256 {
            if let Some(index) = requests
                .iter()
                .position(|request| request.complete.is_some())
            {
                requests.remove(index);
            } else {
                session.connection.record(
                    error(id.clone(), -32000, "Too many pending requests"),
                    "agent",
                );
                return;
            }
        }
        requests.push_back(CachedRequest {
            id: key,
            input: message.clone(),
            complete: None,
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
            let result = match result {
                Ok(value) => response(id.clone(), value),
                Err(_) => error(id.clone(), -32000, "Agent request failed"),
            };
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
