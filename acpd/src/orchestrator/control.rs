//! Client side of the local control channel used by `acpd mcp-delegate` (design section 6.1).
//! One request line, one reply line, newline-delimited JSON. The per-session task token is sent
//! on every call and the host checks it; this side never logs or prints it.
//!
//! Request: `{"token":"…","tool":"task_status","arguments":{…}}`
//! Reply:   `{"ok":true,"result":{…}}` or `{"ok":false,"code":"…","message":"…"}`
use super::delegate::{Backend, ToolFailure, read_line};
use serde_json::{Value, json};
use std::path::Path;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::Mutex,
};

/// Largest reply accepted from the host (design bounds results well below this).
const MAX_REPLY: usize = 4 * 1024 * 1024;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

pub struct ControlBackend<S> {
    stream: Mutex<BufReader<S>>,
    token: String,
}

impl<S: AsyncRead + AsyncWrite + Unpin> ControlBackend<S> {
    pub fn new(stream: S, token: String) -> Self {
        Self {
            stream: Mutex::new(BufReader::new(stream)),
            token,
        }
    }
}

fn unavailable() -> ToolFailure {
    ToolFailure::new(
        "control_unavailable",
        "the host control channel is not available",
    )
}

impl<S: AsyncRead + AsyncWrite + Unpin> Backend for ControlBackend<S> {
    async fn call(&self, tool: &str, arguments: Value) -> Result<Value, ToolFailure> {
        let mut request =
            serde_json::to_vec(&json!({"token": self.token, "tool": tool, "arguments": arguments}))
                .map_err(|_| unavailable())?;
        request.push(b'\n');
        // The lock spans request and reply so concurrent calls cannot interleave their lines.
        let mut stream = self.stream.lock().await;
        stream
            .get_mut()
            .write_all(&request)
            .await
            .map_err(|_| unavailable())?;
        stream.get_mut().flush().await.map_err(|_| unavailable())?;
        let line = read_line(&mut stream, MAX_REPLY)
            .await
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        let reply: Value = serde_json::from_slice(&line).map_err(|_| unavailable())?;
        match reply.get("ok").and_then(Value::as_bool) {
            Some(true) => Ok(reply.get("result").cloned().unwrap_or_else(|| json!({}))),
            Some(false) => Err(ToolFailure::new(
                reply
                    .get("code")
                    .and_then(Value::as_str)
                    .filter(|code| !code.is_empty() && code.len() <= 64)
                    .unwrap_or("control_error"),
                reply
                    .get("message")
                    .and_then(Value::as_str)
                    .map(|m| m.chars().take(500).collect::<String>())
                    .unwrap_or_default(),
            )),
            None => Err(unavailable()),
        }
    }
}

/// Connect to the host's control endpoint: a Unix domain socket, or a Windows named pipe.
#[cfg(unix)]
pub async fn connect(path: &Path) -> std::io::Result<Box<dyn Stream>> {
    Ok(Box::new(tokio::net::UnixStream::connect(path).await?))
}
#[cfg(windows)]
pub async fn connect(path: &Path) -> std::io::Result<Box<dyn Stream>> {
    Ok(Box::new(
        tokio::net::windows::named_pipe::ClientOptions::new().open(path)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, duplex};

    /// A fake host: answers each request line with the next canned reply and records the requests.
    async fn fake_host(server: tokio::io::DuplexStream, replies: Vec<&'static str>) -> Vec<Value> {
        let mut reader = BufReader::new(server);
        let mut seen = vec![];
        for reply in replies {
            let mut line = String::new();
            if reader.read_line(&mut line).await.unwrap() == 0 {
                break;
            }
            seen.push(serde_json::from_str(&line).unwrap());
            let out = reader.get_mut();
            out.write_all(reply.as_bytes()).await.unwrap();
            out.write_all(b"\n").await.unwrap();
        }
        seen
    }

    #[tokio::test]
    async fn sends_the_token_and_returns_the_result() {
        let (client, server) = duplex(64 * 1024);
        let host = tokio::spawn(fake_host(
            server,
            vec![r#"{"ok":true,"result":{"tasks":[]}}"#],
        ));
        let backend = ControlBackend::new(client, "secret-token".into());
        let result = backend.call("task_status", json!({})).await.unwrap();
        assert_eq!(result, json!({"tasks":[]}));
        let seen = host.await.unwrap();
        assert_eq!(seen[0]["token"], "secret-token");
        assert_eq!(seen[0]["tool"], "task_status");
    }

    #[tokio::test]
    async fn host_errors_keep_their_stable_code() {
        let (client, server) = duplex(64 * 1024);
        tokio::spawn(fake_host(
            server,
            vec![
                r#"{"ok":false,"code":"invalid_task_token","message":"expired"}"#,
                r#"{"ok":false}"#,
            ],
        ));
        let backend = ControlBackend::new(client, "t".into());
        let failure = backend.call("agents_list", json!({})).await.unwrap_err();
        assert_eq!(failure, ToolFailure::new("invalid_task_token", "expired"));
        let failure = backend.call("agents_list", json!({})).await.unwrap_err();
        assert_eq!(failure.code, "control_error");
    }

    #[tokio::test]
    async fn closed_or_garbled_channel_reports_control_unavailable() {
        let (client, server) = duplex(1024);
        drop(server);
        let backend = ControlBackend::new(client, "t".into());
        assert_eq!(
            backend
                .call("agents_list", json!({}))
                .await
                .unwrap_err()
                .code,
            "control_unavailable"
        );

        let (client, server) = duplex(1024);
        tokio::spawn(fake_host(server, vec!["not json", r#"{"result":1}"#]));
        let backend = ControlBackend::new(client, "t".into());
        for _ in 0..2 {
            assert_eq!(
                backend
                    .call("agents_list", json!({}))
                    .await
                    .unwrap_err()
                    .code,
                "control_unavailable"
            );
        }
    }

    #[tokio::test]
    async fn end_to_end_mcp_call_through_the_control_channel() {
        let (client, server) = duplex(64 * 1024);
        tokio::spawn(fake_host(
            server,
            vec![r#"{"ok":true,"result":{"agents":[{"ref":"goose"}]}}"#],
        ));
        let backend = ControlBackend::new(client, "t".into());
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"agents_list\"}}\n";
        let mut output = Vec::new();
        super::super::delegate::serve(input.as_bytes(), &mut output, &backend)
            .await
            .unwrap();
        let reply: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            reply["result"]["structuredContent"]["agents"][0]["ref"],
            "goose"
        );
    }
}
