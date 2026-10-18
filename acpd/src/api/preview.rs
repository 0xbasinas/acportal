//! `GET /v1/preview/{port}/{path}`: a dev server on this computer, shown on a paired phone through
//! the phone's own connection to the host (feature `preview`).
//!
//! Off unless the host's config turns `[preview]` on, and then only for the ports it lists. The host
//! connects to `127.0.0.1:{port}` and nowhere else, forwards `GET` and `HEAD` only, never passes the
//! phone's token, cookies or `Origin` on, and drops `Set-Cookie`, so the page keeps nothing. The page
//! is untrusted; the phone shows it in a view with no bridge to the app.
use super::{ApiError, ApiResult, Authorized, Host};
use axum::{
    Extension,
    body::Body,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use std::time::Duration;

/// Largest page or asset passed on.
const MAX_BODY: usize = 32 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

/// Request headers worth passing on. Everything else, above all `Authorization`, stays here.
const FORWARD: &[HeaderName] = &[
    header::ACCEPT,
    header::ACCEPT_LANGUAGE,
    header::ACCEPT_ENCODING,
    header::USER_AGENT,
    header::RANGE,
];
/// Response headers that describe the connection to the dev server, not the page.
fn hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "transfer-encoding"
            | "upgrade"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "content-length"
            | "set-cookie"
            | "set-cookie2"
    )
}

pub(super) async fn proxy_root(
    state: State<Host>,
    Path(port): Path<u16>,
    auth: Extension<Authorized>,
    method: Method,
    query: RawQuery,
    headers: HeaderMap,
) -> ApiResult<Response> {
    proxy(
        state,
        Path((port, String::new())),
        auth,
        method,
        query,
        headers,
    )
    .await
}

pub(super) async fn proxy(
    State(host): State<Host>,
    Path((port, path)): Path<(u16, String)>,
    Extension(auth): Extension<Authorized>,
    method: Method,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let config = host.sessions.launch_settings().config.preview.clone();
    if !config.enabled {
        return Err(ApiError(StatusCode::FORBIDDEN, "preview_disabled"));
    }
    if !config.ports.contains(&port) {
        return Err(ApiError(StatusCode::FORBIDDEN, "port_not_allowed"));
    }
    if method != Method::GET && method != Method::HEAD {
        return Err(ApiError(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
        ));
    }
    if path
        .split('/')
        .any(|part| part == ".." || part.contains('\\'))
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_path"));
    }
    let mut url = format!("http://127.0.0.1:{port}/{path}");
    if let Some(query) = query {
        url.push('?');
        url.push_str(&query);
    }
    let mut request = host
        .preview_client
        .request(method.clone(), &url)
        .timeout(TIMEOUT);
    for name in FORWARD {
        if let Some(value) = headers.get(name) {
            request = request.header(name, value);
        }
    }
    // Method and port only: the path and query can carry tokens of the page's own.
    tracing::debug!(device = %auth.device.id, port, method = %method, "preview request");
    let reply = request.send().await.map_err(|error| {
        if error.is_connect() {
            ApiError(StatusCode::BAD_GATEWAY, "nothing_listening")
        } else if error.is_timeout() {
            ApiError(StatusCode::GATEWAY_TIMEOUT, "preview_timeout")
        } else {
            ApiError(StatusCode::BAD_GATEWAY, "preview_failed")
        }
    })?;
    let status = reply.status();
    let mut out = HeaderMap::new();
    for (name, value) in reply.headers() {
        if hop_by_hop(name) {
            continue;
        }
        if name == header::LOCATION {
            if let Some(text) = value.to_str().ok()
                && let Ok(local) = HeaderValue::from_str(&localize(text, port))
            {
                out.insert(header::LOCATION, local);
            }
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    let body = if method == Method::HEAD {
        Vec::new()
    } else {
        let bytes = reply
            .bytes()
            .await
            .map_err(|_| ApiError(StatusCode::BAD_GATEWAY, "preview_failed"))?;
        if bytes.len() > MAX_BODY {
            return Err(ApiError(StatusCode::BAD_GATEWAY, "preview_too_large"));
        }
        bytes.to_vec()
    };
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    *response.headers_mut() = out;
    Ok(response.into_response())
}

/// A redirect to the dev server's own address becomes a path on the page's origin; any other
/// absolute address is left as it is (the phone refuses to leave the preview).
fn localize(location: &str, port: u16) -> String {
    for prefix in [
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        format!("http://[::1]:{port}"),
    ] {
        if let Some(rest) = location.strip_prefix(&prefix) {
            return if rest.is_empty() {
                "/".into()
            } else {
                rest.into()
            };
        }
    }
    location.to_string()
}

#[cfg(test)]
mod tests {
    use super::localize;

    #[test]
    fn redirects_to_the_dev_server_stay_on_the_page_origin() {
        assert_eq!(localize("http://localhost:5173/login", 5173), "/login");
        assert_eq!(localize("http://127.0.0.1:5173", 5173), "/");
        assert_eq!(localize("/relative", 5173), "/relative");
        assert_eq!(
            localize("https://example.com/", 5173),
            "https://example.com/"
        );
    }
}
