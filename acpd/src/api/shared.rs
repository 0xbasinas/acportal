//! Per-session drafts and titles and per-folder prompt history, kept on the host so the phone
//! and the desktop continue from the same place (feature `sharedDrafts`). Stored in an
//! owner-only `shared.json` in the state directory. Drafts are last writer wins; every change
//! bumps `revision`, so a client can skip an unchanged read. An unreadable file is left alone
//! and writes are refused until it is fixed or removed.
use super::{ApiError, ApiResult, Authorized, Host};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path as FsPath};
use uuid::Uuid;

const FILE: &str = "shared.json";
const MAX_DRAFT_BYTES: usize = 64 * 1024;
const MAX_DRAFTS: usize = 512;
const MAX_TITLE_CHARS: usize = 200;
const MAX_TITLES: usize = 2048;
const MAX_HISTORY: usize = 50;
const MAX_PROMPT_BYTES: usize = 16 * 1024;
const MAX_FOLDERS: usize = 256;
const MAX_WORKSPACE_BYTES: usize = 4096;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Shared {
    revision: u64,
    drafts: BTreeMap<String, Draft>,
    titles: BTreeMap<String, String>,
    history: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Draft {
    text: String,
    updated_at: u64,
    device: String,
}

/// The loaded store, or `None` when the file exists but cannot be read.
pub(super) struct Store(Option<Shared>);
impl Store {
    pub(super) fn load(state: &FsPath) -> Self {
        let path = state.join(FILE);
        if !path.exists() {
            return Self(Some(Shared::default()));
        }
        match std::fs::read(&path)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| Ok(serde_json::from_slice(&bytes)?))
        {
            Ok(shared) => Self(Some(shared)),
            Err(error) => {
                tracing::warn!(%error, "shared drafts file is unreadable; drafts are not saved until it is fixed or removed");
                Self(None)
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn unavailable() -> ApiError {
    ApiError(StatusCode::SERVICE_UNAVAILABLE, "shared_store_unreadable")
}
fn too_large() -> ApiError {
    ApiError(StatusCode::PAYLOAD_TOO_LARGE, "shared_value_too_large")
}

/// Apply `change` and save; the in-memory copy changes only when the save succeeds.
fn change(host: &Host, change: impl FnOnce(&mut Shared) -> ApiResult<()>) -> ApiResult<u64> {
    let mut store = host.shared.lock().unwrap();
    let mut next = store.0.clone().ok_or_else(unavailable)?;
    change(&mut next)?;
    next.revision += 1;
    let bytes = serde_json::to_vec(&next).map_err(super::internal)?;
    crate::userdata::save_private(&host.config.state_directory, FILE, &bytes)
        .map_err(super::internal)?;
    let revision = next.revision;
    store.0 = Some(next);
    Ok(revision)
}

pub(super) async fn get(State(host): State<Host>) -> ApiResult<Json<Value>> {
    let store = host.shared.lock().unwrap();
    let shared = store.0.as_ref().ok_or_else(unavailable)?;
    Ok(Json(serde_json::to_value(shared).map_err(super::internal)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftInput {
    text: String,
}
/// `PUT /v1/shared/drafts/{sessionId}`; empty text removes the draft.
pub(super) async fn put_draft(
    State(host): State<Host>,
    Path(id): Path<Uuid>,
    Extension(auth): Extension<Authorized>,
    Json(input): Json<DraftInput>,
) -> ApiResult<Json<Value>> {
    if input.text.len() > MAX_DRAFT_BYTES {
        return Err(too_large());
    }
    let updated_at = now_ms();
    let revision = change(&host, |shared| {
        let key = id.to_string();
        if input.text.is_empty() {
            shared.drafts.remove(&key);
            return Ok(());
        }
        shared.drafts.insert(
            key,
            Draft {
                text: input.text,
                updated_at,
                device: auth.device.name.chars().take(64).collect(),
            },
        );
        while shared.drafts.len() > MAX_DRAFTS {
            let oldest = shared
                .drafts
                .iter()
                .min_by_key(|(_, draft)| draft.updated_at)
                .map(|(key, _)| key.clone())
                .unwrap();
            shared.drafts.remove(&oldest);
        }
        Ok(())
    })?;
    Ok(Json(json!({"revision":revision,"updatedAt":updated_at})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TitleInput {
    title: String,
}
/// `PUT /v1/shared/titles/{sessionId}`; an empty title removes it.
pub(super) async fn put_title(
    State(host): State<Host>,
    Path(id): Path<Uuid>,
    Json(input): Json<TitleInput>,
) -> ApiResult<Json<Value>> {
    let title = input.title.trim();
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(too_large());
    }
    let revision = change(&host, |shared| {
        let key = id.to_string();
        if title.is_empty() {
            shared.titles.remove(&key);
        } else if shared.titles.len() >= MAX_TITLES && !shared.titles.contains_key(&key) {
            return Err(too_large());
        } else {
            shared.titles.insert(key, title.to_string());
        }
        Ok(())
    })?;
    Ok(Json(json!({"revision":revision})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryInput {
    workspace: String,
    text: String,
}
/// `POST /v1/shared/history`: put a sent prompt first in its folder's history (newest first,
/// duplicates moved, at most 50).
pub(super) async fn add_history(
    State(host): State<Host>,
    Json(input): Json<HistoryInput>,
) -> ApiResult<Json<Value>> {
    if input.text.trim().is_empty() || input.workspace.is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_request"));
    }
    if input.text.len() > MAX_PROMPT_BYTES || input.workspace.len() > MAX_WORKSPACE_BYTES {
        return Err(too_large());
    }
    let revision = change(&host, |shared| {
        if shared.history.len() >= MAX_FOLDERS && !shared.history.contains_key(&input.workspace) {
            return Err(too_large());
        }
        let list = shared.history.entry(input.workspace).or_default();
        list.retain(|prompt| prompt != &input.text);
        list.insert(0, input.text);
        list.truncate(MAX_HISTORY);
        Ok(())
    })?;
    Ok(Json(json!({"revision":revision})))
}

/// Forget a removed session's draft and title.
pub(super) fn forget_session(host: &Host, id: Uuid) {
    let key = id.to_string();
    let known = host
        .shared
        .lock()
        .unwrap()
        .0
        .as_ref()
        .is_some_and(|shared| shared.drafts.contains_key(&key) || shared.titles.contains_key(&key));
    if known {
        let _ = change(host, |shared| {
            shared.drafts.remove(&key);
            shared.titles.remove(&key);
            Ok(())
        });
    }
}
