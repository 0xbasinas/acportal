//! `/v1/terminals`: interactive shells for a paired phone or the desktop app (see `terminals.rs`).
//!
//! Off unless the host's config turns `[terminal]` on. Same checks as Git: a phone-role device
//! and a folder inside the host's workspace roots or task worktrees. Output goes to the client as
//! binary WebSocket frames (8-byte big-endian byte offset, then the bytes); keystrokes come back
//! as binary frames; resize and "take the keyboard" are small JSON text frames.
use super::{ApiError, ApiResult, Authorized, Host};
use crate::terminals::{Event, MAX_FRAME, TerminalError};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

const SUBPROTOCOL: &str = "acpd.terminal.v1";

fn error(e: TerminalError) -> ApiError {
    match e {
        TerminalError::Disabled => ApiError(StatusCode::FORBIDDEN, "terminal_disabled"),
        TerminalError::TooMany => ApiError(StatusCode::TOO_MANY_REQUESTS, "too_many_terminals"),
        TerminalError::NotFound => ApiError(StatusCode::NOT_FOUND, "terminal_not_found"),
        TerminalError::NotController => ApiError(StatusCode::CONFLICT, "not_controller"),
        TerminalError::TooLarge => ApiError(StatusCode::PAYLOAD_TOO_LARGE, "input_too_large"),
        TerminalError::Busy => ApiError(StatusCode::TOO_MANY_REQUESTS, "terminal_busy"),
        TerminalError::Ended => ApiError(StatusCode::CONFLICT, "terminal_ended"),
        TerminalError::Failed(_) => ApiError(StatusCode::INTERNAL_SERVER_ERROR, "terminal_failed"),
    }
}

pub(super) async fn list(State(host): State<Host>) -> ApiResult<Json<Value>> {
    let enabled = host.sessions.launch_settings().config.terminal.enabled;
    Ok(Json(
        json!({"enabled":enabled,"terminals":host.terminals.list()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Open {
    folder: PathBuf,
    #[serde(default = "rows")]
    rows: u16,
    #[serde(default = "cols")]
    cols: u16,
}
fn rows() -> u16 {
    24
}
fn cols() -> u16 {
    80
}

pub(super) async fn open(
    State(host): State<Host>,
    Extension(auth): Extension<Authorized>,
    Json(input): Json<Open>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let config = host.sessions.launch_settings().config.terminal.clone();
    if !config.enabled {
        return Err(error(TerminalError::Disabled));
    }
    let folder = super::git::allowed(&host, &input.folder)
        .ok_or(ApiError(StatusCode::FORBIDDEN, "path_not_allowed"))?;
    let terminals = host.terminals.clone();
    let device = auth.device.id;
    let (rows, cols) = (input.rows, input.cols);
    let info =
        tokio::task::spawn_blocking(move || terminals.open(&config, &folder, device, rows, cols))
            .await
            .map_err(super::internal)?
            .map_err(|e| {
                if let TerminalError::Failed(why) = &e {
                    tracing::warn!(error = %why, "terminal could not start");
                }
                error(e)
            })?;
    tracing::info!(terminal = %info.id, device = %device, folder = %info.folder, shell = %info.shell, "terminal opened by device");
    Ok((StatusCode::CREATED, Json(json!(info))))
}

pub(super) async fn close(State(host): State<Host>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let terminals = host.terminals.clone();
    let closed = tokio::task::spawn_blocking(move || terminals.close(id))
        .await
        .map_err(super::internal)?;
    if closed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(TerminalError::NotFound))
    }
}

#[derive(Deserialize)]
pub(super) struct Attach {
    #[serde(default)]
    after: u64,
}

pub(super) async fn connect(
    State(host): State<Host>,
    Path(id): Path<Uuid>,
    Query(query): Query<Attach>,
    Extension(auth): Extension<Authorized>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    if !headers
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|p| p.trim() == SUBPROTOCOL))
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "websocket_subprotocol_required",
        ));
    }
    if !host.sessions.launch_settings().config.terminal.enabled {
        return Err(error(TerminalError::Disabled));
    }
    let sub = host.terminals.subscribe(id, query.after).map_err(error)?;
    Ok(ws
        .protocols([SUBPROTOCOL])
        .max_message_size(MAX_FRAME + 1024)
        .max_frame_size(MAX_FRAME + 1024)
        .on_upgrade(move |socket| run(socket, host, id, auth.device.id, sub))
        .into_response())
}

fn frame(offset: u64, data: &[u8]) -> Message {
    let mut out = Vec::with_capacity(8 + data.len());
    out.extend_from_slice(&offset.to_be_bytes());
    out.extend_from_slice(data);
    Message::Binary(out.into())
}
fn text(value: Value) -> Message {
    Message::Text(value.to_string().into())
}

async fn run(
    mut socket: WebSocket,
    host: Host,
    id: Uuid,
    device: Uuid,
    mut sub: crate::terminals::Subscription,
) {
    let info = host.terminals.info(id).ok();
    let you = info.as_ref().is_some_and(|i| i.controller == Some(device));
    let hello = json!({"type":"hello","terminal":info,"gap":sub.gap,"you":you});
    if socket.send(text(hello)).await.is_err() {
        return;
    }
    let mut sent = 0u64;
    for (offset, data) in std::mem::take(&mut sub.replay) {
        for (i, piece) in data.chunks(MAX_FRAME).enumerate() {
            let at = offset + (i * MAX_FRAME) as u64;
            if socket.send(frame(at, piece)).await.is_err() {
                return;
            }
            sent = at + piece.len() as u64;
        }
    }
    if let Some(code) = sub.exit_code {
        let _ = socket.send(text(json!({"type":"exit","code":code}))).await;
    }
    loop {
        tokio::select! {
            event = sub.rx.recv() => match event {
                Ok(Event::Output { offset, data }) => {
                    let end = offset + data.len() as u64;
                    if end <= sent { continue }
                    let skip = sent.saturating_sub(offset) as usize;
                    let mut at = offset + skip as u64;
                    for piece in data[skip..].chunks(MAX_FRAME) {
                        if socket.send(frame(at, piece)).await.is_err() { return }
                        at += piece.len() as u64;
                    }
                    sent = end;
                }
                Ok(Event::Exit(code)) => {
                    let _ = socket.send(text(json!({"type":"exit","code":code}))).await;
                }
                Err(_) => {
                    // Fell behind: the client reconnects with `?after=` from what it has.
                    let _ = socket.send(text(json!({"type":"reconnect_required"}))).await;
                    return;
                }
            },
            message = socket.recv() => {
                let Some(Ok(message)) = message else { return };
                let failure = match message {
                    Message::Binary(bytes) => host.terminals.input(id, device, bytes.to_vec()).err(),
                    Message::Text(body) => match serde_json::from_str::<Value>(&body) {
                        Ok(value) => match value["type"].as_str() {
                            Some("resize") => {
                                let rows = value["rows"].as_u64().unwrap_or(24).min(500) as u16;
                                let cols = value["cols"].as_u64().unwrap_or(80).min(1000) as u16;
                                host.terminals.resize(id, device, rows, cols).err()
                            }
                            Some("take") => {
                                let result = host.terminals.take_control(id, device);
                                if result.is_ok() {
                                    let _ = socket.send(text(json!({"type":"controller","you":true}))).await;
                                }
                                result.err()
                            }
                            _ => None,
                        },
                        Err(_) => None,
                    },
                    Message::Close(_) => return,
                    _ => None,
                };
                if let Some(e) = failure {
                    let code = match e {
                        TerminalError::NotController => "not_controller",
                        TerminalError::Busy => "terminal_busy",
                        TerminalError::TooLarge => "input_too_large",
                        TerminalError::Ended => "terminal_ended",
                        TerminalError::NotFound => "terminal_not_found",
                        _ => "terminal_failed",
                    };
                    if socket.send(text(json!({"type":"error","code":code}))).await.is_err() { return }
                }
            }
        }
    }
}
