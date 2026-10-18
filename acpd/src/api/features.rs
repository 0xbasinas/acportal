//! `GET`/`PUT /v1/settings/features`: whether this host offers terminals and preview, and the ports
//! preview may show. Changes are stored with the phone-added folders and agents
//! (`user-settings.json`) and take effect at once, without a restart. Like adding a folder or an
//! agent, this is something a paired phone-role device may do; the host's own `config.toml` is
//! never rewritten, and a value stored here wins over it (clear it with `null` to fall back).
use super::{ApiError, ApiResult, Host, user_change_error};
use crate::userdata::UserData;
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

fn describe(host: &Host) -> Value {
    let settings = host.sessions.launch_settings();
    let (terminal, preview) = (&settings.config.terminal, &settings.config.preview);
    let stored = UserData::load(&settings.config.state_directory).unwrap_or_default();
    json!({
        "terminal": {"enabled": terminal.enabled, "setHere": stored.terminal.is_some()},
        "preview": {"enabled": preview.enabled, "ports": preview.ports, "setHere": stored.preview_enabled.is_some() || stored.preview_ports.is_some()},
    })
}

pub(super) async fn get(State(host): State<Host>) -> ApiResult<Json<Value>> {
    Ok(Json(describe(&host)))
}

/// `null` clears a stored value (the host's own config applies again); a missing key leaves it alone.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Change {
    #[serde(default, deserialize_with = "double_option")]
    terminal: Option<Option<bool>>,
    #[serde(default, rename = "previewEnabled", deserialize_with = "double_option")]
    preview_enabled: Option<Option<bool>>,
    #[serde(default, rename = "previewPorts", deserialize_with = "double_option")]
    preview_ports: Option<Option<Vec<u16>>>,
}
fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

pub(super) async fn put(
    State(host): State<Host>,
    Json(change): Json<Change>,
) -> ApiResult<Json<Value>> {
    let _guard = host.user_changes.lock().await;
    let settings = host.sessions.launch_settings();
    let state = &settings.config.state_directory;
    let mut data = UserData::load(state).map_err(user_change_error)?;
    if let Some(value) = change.terminal {
        data.terminal = value;
    }
    if let Some(value) = change.preview_enabled {
        data.preview_enabled = value;
    }
    if let Some(value) = change.preview_ports.clone() {
        data.preview_ports = value;
    }
    // The base is the host's own config with the stored values applied, so clearing one falls back.
    let mut config = settings.config.clone();
    if let Some(value) = change.terminal {
        config.terminal.enabled = value.unwrap_or(host.file_terminal);
    }
    if let Some(value) = change.preview_enabled {
        config.preview.enabled = value.unwrap_or(host.file_preview.0);
    }
    if let Some(value) = change.preview_ports {
        config.preview.ports = value.unwrap_or_else(|| host.file_preview.1.clone());
    }
    config
        .validate()
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_request"))?;
    data.save(state).map_err(user_change_error)?;
    host.sessions
        .reload(config, settings.registry.clone())
        .map_err(user_change_error)?;
    tracing::info!("terminal or preview setting changed by a paired device");
    Ok(Json(describe(&host)))
}
