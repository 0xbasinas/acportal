//! Inbound agent bridge: standard ACP JSON-RPC over a WebSocket at `/acp/{agent_id}`, for paired
//! peer hosts (design section 7.1).
//!
//! A bridge session is an ordinary host session started with `SessionManager`, so process
//! containment, workspace checks, consent for writes and terminals, byte budgets and the journal
//! all apply unchanged, and the phone sees it like any other session. Client messages go through
//! the same `dispatch` the phone uses (method allow-list, request de-duplication, permission
//! answering); the session's journal is the source of everything sent back.
//!
//! What a peer may and may not do: it names a workspace by alias, never by path; it cannot pass
//! MCP servers (a stdio MCP server is a command to run on this host); its sessions get at most
//! the configured `peer_rights` (read-only by default); only the agents in `peer_agents` exist
//! for it. A dropped socket does not end a session straight away: it stays for
//! `bridge_resume_seconds` so the same peer can reattach with `session/load` and read what
//! happened meanwhile. Nothing is ever re-sent to the agent on reattach.
use super::*;
use crate::connection::{Event, WorkspaceAccess};
use axum::extract::ws::CloseFrame;
use std::sync::atomic::AtomicU64;
use tokio::sync::{Notify, mpsc};

/// Frames queued for one peer socket. A peer that cannot keep up is disconnected, not buffered.
const OUTBOX: usize = 256;
/// Messages a peer may send per second on one connection. A legitimate peer sends a few
/// (prompts, answers, cancels); this only stops a loop. Over it, the socket is closed with 1008.
const MAX_MESSAGES_PER_SECOND: u32 = 100;
static EPOCH: AtomicU64 = AtomicU64::new(1);

/// Which bridge sessions exist, who owns them, and whether a socket is currently attached.
#[derive(Default)]
pub(super) struct BridgeState {
    sessions: Mutex<HashMap<Uuid, Retained>>,
}
struct Retained {
    peer: Uuid,
    attached: bool,
    /// Bumped on every attach and detach, so a stale removal timer does nothing.
    epoch: u64,
    /// The ids of the requests (`q:<id>`) and answers (`p:<id>`) this host received from the peer
    /// for the session, newest last. A peer that reattaches reads them to learn which of the
    /// things it wrote just before the socket died never arrived, and sends only those again.
    received: std::collections::VecDeque<String>,
}
/// How many received ids are kept per session.
const RECEIVED_KEPT: usize = 1024;

/// What a paired peer may use here, so it can check before it opens a session: the agents shared
/// with peers (only ones that exist and are enabled), workspace aliases (never their paths), the
/// rights ceiling and the resume window. Nothing about agents that are not shared.
pub(super) async fn info(State(host): State<Host>) -> ApiResult<Json<Value>> {
    let settings = host.sessions.launch_settings();
    let orchestrator = &settings.config.orchestrator;
    if !orchestrator.enabled {
        return Err(ApiError(StatusCode::NOT_FOUND, "bridge_not_enabled"));
    }
    let agents: Vec<Value> = orchestrator
        .peer_agents
        .iter()
        .filter_map(|id| {
            let definition = settings.registry.get(id).ok().filter(|d| d.enabled)?;
            Some(json!({"id": id, "name": definition.name}))
        })
        .collect();
    Ok(Json(json!({
        "hostId": host.security.host_id().map_err(internal)?,
        "version": env!("CARGO_PKG_VERSION"),
        "protocolVersion": 1,
        "agents": agents,
        "workspaces": orchestrator.peer_workspaces.keys().collect::<Vec<_>>(),
        "rights": orchestrator.peer_rights,
        "resumeSeconds": orchestrator.bridge_resume_seconds,
    })))
}

pub(super) async fn agent_required() -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, "agent_required")
}
pub(super) async fn not_supported() -> ApiError {
    ApiError(StatusCode::NOT_IMPLEMENTED, "transport_not_supported")
}

pub(super) async fn upgrade(
    State(host): State<Host>,
    Path(agent_id): Path<String>,
    Extension(auth): Extension<Authorized>,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    // A browser-supplied `Origin` never gets this far: the router's `no_browser_and_no_cache`
    // layer refuses it for every route. (An Origin check here was unreachable, which a mutation
    // run showed; `routes_roles_and_origin_are_enforced_before_any_session_exists` now pins the
    // behavior for `/acp`.)
    let settings = host.sessions.launch_settings();
    let orchestrator = &settings.config.orchestrator;
    let known = orchestrator.enabled
        && orchestrator.peer_agents.contains(&agent_id)
        && settings
            .registry
            .get(&agent_id)
            .is_ok_and(|definition| definition.enabled);
    if !known {
        // The same answer whether the agent does not exist or is not shared with peers.
        return Err(ApiError(StatusCode::NOT_FOUND, "agent_not_found"));
    }
    let limit = host.config.runtime.max_frame_bytes;
    Ok(ws
        .max_message_size(limit)
        .max_frame_size(limit)
        .on_upgrade(move |socket| serve(socket, host, auth, agent_id))
        .into_response())
}

/// Replace every `sessionId` equal to `from` with `to`, anywhere in the message. Bridge session
/// ids are the host's UUIDs: the agent's own ids collide between processes (every mock session
/// is `mock-1`) and are not the peer's to see.
fn swap_session_id(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                if key == "sessionId" && inner.as_str() == Some(from) {
                    *inner = Value::String(to.to_owned());
                } else {
                    swap_session_id(inner, from, to);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|i| swap_session_id(i, from, to)),
        _ => {}
    }
}

/// What a journal event looks like on the wire to a peer, or `None` if the peer never sees it.
/// Agent messages (notifications, requests and the responses the host synthesizes) go through;
/// host-published updates go through; echoes of what a client sent do not.
///
/// With `tag` (a client that asked for reattach support in `initialize`), the frame carries the
/// journal sequence number as a top-level `acpdSeq`, so a client that reattaches can tell the
/// frames it already has from the ones it missed. Other clients never see the field.
fn outbound(event: &Event, acp_id: &str, bridge_id: &str, tag: bool) -> Option<Value> {
    let forward = match event.direction {
        "agent" => true,
        "host" => event.message.get("method").is_some(),
        _ => false,
    };
    if !forward {
        return None;
    }
    let mut message = event.message.clone();
    swap_session_id(&mut message, acp_id, bridge_id);
    if tag {
        message["acpdSeq"] = json!(event.sequence);
    }
    Some(message)
}

struct Owned {
    session: Arc<AcpSession>,
    acp_id: String,
    forwarder: tokio::task::JoinHandle<()>,
    _lease: Lease,
}

struct Peer {
    host: Host,
    auth: Authorized,
    agent_id: String,
    initialized: bool,
    /// The client asked, in `initialize`, for journal sequence numbers on forwarded frames.
    seq_tags: bool,
    owned: HashMap<String, Owned>,
    outbox: mpsc::Sender<Value>,
    overflow: Arc<Notify>,
}

fn reply_error(id: Value, code: i64, message: &str) -> Value {
    error(id, code, message)
}

impl Peer {
    async fn queue(&self, frame: Value) {
        // Replies are small and ordered with the journal's frames; a full outbox here means the
        // peer stopped reading, which the overflow signal turns into a disconnect.
        if self.outbox.try_send(frame).is_err() {
            self.overflow.notify_one();
        }
    }

    /// Forward a session's journal to the peer from `cursor` on.
    fn start_forwarding(
        &self,
        session: &Arc<AcpSession>,
        acp_id: &str,
        bridge_id: &str,
        mut cursor: u64,
    ) -> tokio::task::JoinHandle<()> {
        let mut events = session.connection.subscribe();
        let (acp_id, bridge_id) = (acp_id.to_owned(), bridge_id.to_owned());
        let outbox = self.outbox.clone();
        let overflow = self.overflow.clone();
        let tag = self.seq_tags;
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) if event.sequence > cursor => {
                        cursor = event.sequence;
                        if let Some(frame) = outbound(&event, &acp_id, &bridge_id, tag)
                            && outbox.try_send(frame).is_err()
                        {
                            overflow.notify_one();
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        overflow.notify_one();
                        return;
                    }
                    Err(_) => return,
                }
            }
        })
    }

    async fn handle(&mut self, text: &str) -> Result<(), &'static str> {
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(_) => {
                self.queue(reply_error(Value::Null, -32700, "Parse error"))
                    .await;
                return Ok(());
            }
        };
        if value.is_array() {
            // The RFD asks for 501 on batches over HTTP; on a socket the same refusal is a
            // JSON-RPC error. Some servers process batches: a peer must not rely on it.
            self.queue(reply_error(
                Value::Null,
                -32600,
                "Batch requests are not supported",
            ))
            .await;
            return Ok(());
        }
        let Some(object) = value.as_object() else {
            self.queue(reply_error(Value::Null, -32600, "Invalid request"))
                .await;
            return Ok(());
        };
        let id = object.get("id").cloned();
        let method = object
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let Some(method) = method else {
            // A response: the peer answering a permission or elicitation request.
            return self.route_response(value).await;
        };
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        match method.as_str() {
            "initialize" => {
                let Some(id) = id else { return Ok(()) };
                if self.initialized {
                    self.queue(reply_error(id, -32600, "Already initialized"))
                        .await;
                } else {
                    self.initialized = true;
                    self.seq_tags = params["_meta"]["acpdReattach"] == true;
                    // Version 1 only. A different version asked for gets 1 back, as the spec
                    // says, and the peer decides whether to continue.
                    let mut result = json!({"protocolVersion": 1,
                        "agentCapabilities": {
                            "loadSession": true,
                            "promptCapabilities": {"image": false, "audio": false, "embeddedContext": false},
                            "mcpCapabilities": {"http": false, "sse": false}},
                        "agentInfo": {"name": "acpd", "version": env!("CARGO_PKG_VERSION")},
                        "authMethods": []});
                    if self.seq_tags {
                        result["_meta"] = json!({"acpdReattach": true});
                    }
                    self.queue(response(id, result)).await;
                }
            }
            _ if !self.initialized => {
                if let Some(id) = id {
                    self.queue(reply_error(
                        id,
                        -32600,
                        "initialize must be the first request",
                    ))
                    .await;
                }
            }
            "session/new" => {
                let Some(id) = id else { return Ok(()) };
                let reply = self.session_new(id.clone(), &params).await;
                self.queue(reply).await;
            }
            "session/load" => {
                let Some(id) = id else { return Ok(()) };
                let reply = self.session_load(id.clone(), &params).await;
                self.queue(reply).await;
            }
            _ => self.forward(value, &params).await,
        }
        Ok(())
    }

    /// A response frame from the peer: find the session whose pending request it answers.
    async fn route_response(&self, value: Value) -> Result<(), &'static str> {
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        for owned in self.owned.values() {
            let pending = owned.session.connection.pending_permissions();
            if pending.iter().any(|p| p.message["id"] == id) {
                self.note_received(owned.session.metadata().id, format!("p:{id}"));
                let gateway = gateway_for(&self.host, owned.session.metadata().id).await;
                dispatch(owned.session.clone(), gateway, value);
                return Ok(());
            }
        }
        Ok(()) // answers to nothing pending are dropped
    }

    /// Any other method belongs to one of this connection's sessions.
    async fn forward(&self, mut message: Value, params: &Value) {
        let id = message.get("id").cloned();
        let Some(bridge_id) = params["sessionId"].as_str() else {
            if let Some(id) = id {
                self.queue(reply_error(id, -32602, "sessionId is required"))
                    .await;
            }
            return;
        };
        let Some(owned) = self.owned.get(bridge_id) else {
            if let Some(id) = id {
                self.queue(reply_error(id, -32602, "Unknown session")).await;
            }
            return;
        };
        swap_session_id(&mut message, bridge_id, &owned.acp_id);
        if let Some(id) = &id {
            self.note_received(owned.session.metadata().id, format!("q:{id}"));
        }
        let gateway = gateway_for(&self.host, owned.session.metadata().id).await;
        dispatch(owned.session.clone(), gateway, message);
    }

    /// Remember that `key` arrived for this session (see [`Retained::received`]).
    fn note_received(&self, host_id: Uuid, key: String) {
        if let Some(entry) = self.host.bridge.sessions.lock().unwrap().get_mut(&host_id) {
            entry.received.push_back(key);
            while entry.received.len() > RECEIVED_KEPT {
                entry.received.pop_front();
            }
        }
    }

    async fn session_new(&mut self, id: Value, params: &Value) -> Value {
        if !params["cwd"].is_string() {
            return reply_error(id, -32602, "cwd is required");
        }
        // A peer must not be able to make this host run a command: an MCP server is one.
        if params["mcpServers"]
            .as_array()
            .is_some_and(|servers| !servers.is_empty())
            || (params.get("mcpServers").is_some() && !params["mcpServers"].is_array())
        {
            return reply_error(id, -32602, "MCP servers are not accepted from peers");
        }
        let settings = self.host.sessions.launch_settings();
        let config = &settings.config;
        let Some(alias) = params["_meta"]["acpdWorkspace"].as_str() else {
            return reply_error(
                id,
                -32602,
                "_meta.acpdWorkspace (a workspace alias) is required",
            );
        };
        let Some(path) = config.orchestrator.peer_workspaces.get(alias) else {
            return reply_error(id, -32602, "Unknown workspace alias");
        };
        let Ok(workspace) = config.workspace(path) else {
            return reply_error(id, -32602, "Workspace is not available");
        };
        let allowed = config.orchestrator.peer_rights;
        let wanted = &params["_meta"]["acpdRights"];
        let ask = |key: &str, limit: bool| match wanted.get(key) {
            None | Some(Value::Null) => Ok(limit),
            Some(Value::Bool(false)) => Ok(false),
            Some(Value::Bool(true)) if limit => Ok(true),
            _ => Err(()),
        };
        let (Ok(read), Ok(write), Ok(terminal)) = (
            ask("read", allowed.read),
            ask("write", allowed.write),
            ask("terminal", allowed.terminal),
        ) else {
            return reply_error(
                id,
                -32602,
                "Requested rights exceed what this host allows peers",
            );
        };
        let access = WorkspaceAccess {
            read_files: read,
            write_files: write,
            terminal,
            form_elicitation: false,
            manual_approval: false,
            boolean_options: false,
            yolo: false,
        };
        let session = match self
            .host
            .sessions
            .open_configured(&self.agent_id, &workspace, None, Vec::new(), access)
            .await
        {
            Ok(session) => session,
            Err(_) => return reply_error(id, -32603, "The agent could not be started"),
        };
        let metadata = session.metadata();
        if metadata.acp_session_id.is_empty() {
            let _ = self.host.sessions.remove(metadata.id).await;
            return reply_error(id, -32000, "The agent needs authentication on this host");
        }
        let bridge_id = metadata.id.to_string();
        let gateway = gateway_for(&self.host, metadata.id).await;
        let Some(lease) = take_lease(&gateway) else {
            let _ = self.host.sessions.remove(metadata.id).await;
            return reply_error(id, -32603, "The session is already controlled");
        };
        let cursor = session.replay(u64::MAX).latest_sequence;
        let forwarder =
            self.start_forwarding(&session, &metadata.acp_session_id, &bridge_id, cursor);
        self.host.bridge.sessions.lock().unwrap().insert(
            metadata.id,
            Retained {
                peer: self.auth.device.id,
                attached: true,
                epoch: EPOCH.fetch_add(1, Ordering::Relaxed),
                received: Default::default(),
            },
        );
        self.owned.insert(
            bridge_id.clone(),
            Owned {
                session,
                acp_id: metadata.acp_session_id.clone(),
                forwarder,
                _lease: lease,
            },
        );
        // The agent's own answer (modes, config options, ...) with the session id swapped.
        let mut result = metadata.setup.clone();
        swap_session_id(&mut result, &metadata.acp_session_id, &bridge_id);
        result["sessionId"] = json!(bridge_id);
        if self.seq_tags {
            // Forwarding starts after this position; a reattach replays from the start, and the
            // client must know the earlier events are ones it was never meant to get.
            if !result["_meta"].is_object() {
                result["_meta"] = json!({});
            }
            result["_meta"]["acpdSeq"] = json!(cursor);
        }
        response(id, result)
    }

    /// Reattach to a session this peer started on an earlier connection. The host's journal is
    /// replayed (what the agent did meanwhile, including a prompt that finished); nothing is
    /// sent to the agent.
    async fn session_load(&mut self, id: Value, params: &Value) -> Value {
        let missing = || reply_error(id.clone(), -32602, "Session not found");
        let Some(bridge_id) = params["sessionId"].as_str() else {
            return missing();
        };
        let Ok(host_id) = Uuid::parse_str(bridge_id) else {
            return missing();
        };
        if self.owned.contains_key(bridge_id) {
            return reply_error(id, -32602, "Session is already attached to this connection");
        }
        let epoch = EPOCH.fetch_add(1, Ordering::Relaxed);
        {
            let mut sessions = self.host.bridge.sessions.lock().unwrap();
            match sessions.get_mut(&host_id) {
                Some(entry) if entry.peer == self.auth.device.id && !entry.attached => {
                    entry.attached = true;
                    entry.epoch = epoch;
                }
                _ => return missing(),
            }
        }
        let detach = |host: &Host| {
            if let Some(entry) = host.bridge.sessions.lock().unwrap().get_mut(&host_id) {
                entry.attached = false;
            }
        };
        let Ok(session) = self.host.sessions.get(host_id).await else {
            self.host.bridge.sessions.lock().unwrap().remove(&host_id);
            return missing();
        };
        let gateway = gateway_for(&self.host, host_id).await;
        let Some(lease) = take_lease(&gateway) else {
            detach(&self.host);
            return reply_error(id, -32603, "The session is already controlled");
        };
        let acp_id = session.metadata().acp_session_id;
        let replay = session.replay(0);
        let first = replay.events.first().map_or(0, |event| event.sequence);
        let mut sent_ids = HashSet::new();
        for event in &replay.events {
            if let Some(frame) = outbound(event, &acp_id, bridge_id, self.seq_tags) {
                if frame.get("method").is_some()
                    && let Some(request_id) = frame.get("id")
                {
                    sent_ids.insert(request_id.to_string());
                }
                self.queue(frame).await;
            }
        }
        // A request still waiting for an answer may have been evicted from the replayed window.
        for pending in session.connection.pending_permissions() {
            if !sent_ids.contains(&pending.message["id"].to_string()) {
                let mut frame = pending.message.clone();
                swap_session_id(&mut frame, &acp_id, bridge_id);
                self.queue(frame).await;
            }
        }
        let forwarder = self.start_forwarding(&session, &acp_id, bridge_id, replay.latest_sequence);
        self.owned.insert(
            bridge_id.to_owned(),
            Owned {
                session,
                acp_id,
                forwarder,
                _lease: lease,
            },
        );
        let mut result = json!({});
        if replay.gap {
            // Events were evicted from the bounded journal: say how many, in `_meta` only.
            result["_meta"] = json!({"acpdGap": {"missed": first.saturating_sub(1)}});
        }
        if self.seq_tags {
            // What this host received from the peer for the session, so the peer can tell what
            // it wrote just before the socket died never got here (and send only that again).
            let received: Vec<String> = self
                .host
                .bridge
                .sessions
                .lock()
                .unwrap()
                .get(&host_id)
                .map(|entry| entry.received.iter().cloned().collect())
                .unwrap_or_default();
            if !result["_meta"].is_object() {
                result["_meta"] = json!({});
            }
            result["_meta"]["acpdReceived"] = json!(received);
        }
        response(id, result)
    }
}

use std::collections::HashSet;

async fn still_authorized(host: &Host, token: &str) -> bool {
    let security = host.security.clone();
    let token = token.to_owned();
    matches!(
        tokio::task::spawn_blocking(move || security.authorize(&token)).await,
        Ok(Ok(device)) if device.role == Role::Peer
    )
}

/// End every session a peer owns (revoked or expired credentials): attached or waiting.
async fn end_peer_sessions(host: &Host, peer: Uuid) {
    let ids: Vec<Uuid> = {
        let mut sessions = host.bridge.sessions.lock().unwrap();
        let ids: Vec<Uuid> = sessions
            .iter()
            .filter(|(_, entry)| entry.peer == peer)
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            sessions.remove(id);
        }
        ids
    };
    for id in ids {
        let _ = host.sessions.remove(id).await;
        host.gateways.lock().await.remove(&id);
    }
}

/// Write one JSON frame, giving up (and so closing the connection) after `timeout`.
async fn send_frame(
    socket: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    value: Value,
    timeout: Duration,
) -> bool {
    matches!(
        tokio::time::timeout(
            timeout,
            socket.send(Message::Text(value.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    )
}

async fn serve(socket: WebSocket, host: Host, auth: Authorized, agent_id: String) {
    let settings = host.sessions.launch_settings();
    let resume = Duration::from_secs(settings.config.orchestrator.bridge_resume_seconds);
    let ping = Duration::from_secs(settings.config.orchestrator.bridge_ping_seconds);
    let send_timeout =
        Duration::from_secs(settings.config.orchestrator.bridge_send_timeout_seconds);
    let (outbox, mut queued) = mpsc::channel::<Value>(OUTBOX);
    let overflow = Arc::new(Notify::new());
    let peer_id = auth.device.id;
    let token = auth.token.clone();
    let mut peer = Peer {
        host: host.clone(),
        auth,
        agent_id,
        initialized: false,
        seq_tags: false,
        owned: HashMap::new(),
        outbox,
        overflow: overflow.clone(),
    };
    let (mut outgoing, mut incoming) = socket.split();
    let mut heartbeat = tokio::time::interval(ping);
    heartbeat.tick().await; // the first tick is immediate
    let mut last_received = tokio::time::Instant::now();
    let mut revoked = false;
    let mut ended = false;
    let mut limited = false;
    let mut close_code = 1000u16;
    let mut window_start = tokio::time::Instant::now();
    let mut in_window = 0u32;
    loop {
        tokio::select! {
            frame = queued.recv() => {
                let Some(frame) = frame else { break };
                if !send_frame(&mut outgoing, frame, send_timeout).await { break }
            }
            _ = overflow.notified() => {
                // The peer is not keeping up (or the journal outran it). It reconnects and
                // reloads; unlike the phone socket there is no cursor to resume from.
                close_code = 1013;
                break;
            }
            message = incoming.next() => {
                last_received = tokio::time::Instant::now();
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if window_start.elapsed() >= Duration::from_secs(1) {
                            window_start = tokio::time::Instant::now();
                            in_window = 0;
                        }
                        in_window += 1;
                        if in_window > MAX_MESSAGES_PER_SECOND { limited = true; break }
                        if !still_authorized(&host, &token).await { revoked = true; break }
                        if peer.handle(&text).await.is_err() { break }
                    }
                    // Binary frames are ignored, as the RFD requires.
                    Some(Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_))) => {}
                    // A peer closing on purpose says so, and its sessions end now.
                    Some(Ok(Message::Close(frame))) => {
                        ended = frame
                            .as_ref()
                            .is_some_and(|f| f.reason.as_str() == crate::peer_link::END_SESSIONS);
                        break;
                    }
                    _ => break,
                }
            }
            _ = heartbeat.tick() => {
                if !still_authorized(&host, &token).await { revoked = true; break }
                if last_received.elapsed() > ping * 3 { break }
                if !matches!(
                    tokio::time::timeout(send_timeout, outgoing.send(Message::Ping(vec![].into()))).await,
                    Ok(Ok(()))
                ) { break }
            }
        }
    }
    let frame = CloseFrame {
        code: if revoked || limited { 1008 } else { close_code },
        reason: if revoked {
            "credentials_revoked_or_expired"
        } else if limited {
            "rate_limited"
        } else {
            ""
        }
        .into(),
    };
    let _ = tokio::time::timeout(
        Duration::from_secs(1),
        outgoing.send(Message::Close(Some(frame))),
    )
    .await;

    for owned in peer.owned.values() {
        owned.forwarder.abort();
    }
    if ended {
        // Ended on purpose by the peer: nothing to wait for.
        let ids: Vec<Uuid> = peer
            .owned
            .values()
            .map(|owned| owned.session.metadata().id)
            .collect();
        drop(peer);
        for id in ids {
            host.bridge.sessions.lock().unwrap().remove(&id);
            let _ = host.sessions.remove(id).await;
            host.gateways.lock().await.remove(&id);
        }
        return;
    }
    if revoked {
        // A revoked peer's sessions end now, including any waiting to be reattached.
        drop(peer);
        end_peer_sessions(&host, peer_id).await;
        return;
    }
    // Keep the sessions for a while so the same peer can reattach; then remove what nobody took.
    let detached: Vec<Uuid> = peer
        .owned
        .values()
        .map(|owned| owned.session.metadata().id)
        .collect();
    drop(peer);
    for id in detached {
        let epoch = EPOCH.fetch_add(1, Ordering::Relaxed);
        if let Some(entry) = host.bridge.sessions.lock().unwrap().get_mut(&id) {
            entry.attached = false;
            entry.epoch = epoch;
        }
        let host = host.clone();
        tokio::spawn(async move {
            tokio::time::sleep(resume).await;
            let expired = {
                let mut sessions = host.bridge.sessions.lock().unwrap();
                match sessions.get(&id) {
                    Some(entry) if !entry.attached && entry.epoch == epoch => {
                        sessions.remove(&id);
                        true
                    }
                    _ => false,
                }
            };
            if expired {
                let _ = host.sessions.remove(id).await;
                host.gateways.lock().await.remove(&id);
            }
        });
    }
}
