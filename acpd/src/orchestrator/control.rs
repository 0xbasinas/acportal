//! Client side of the local control channel used by `acpd mcp-delegate` (design section 6.1).
//! One request line, one reply line, newline-delimited JSON. The per-session task token is sent
//! on every call and the host checks it; this side never logs or prints it.
//!
//! Request: `{"token":"…","tool":"task_status","arguments":{…}}`
//! Reply:   `{"ok":true,"result":{…}}` or `{"ok":false,"code":"…","message":"…"}`
use super::{
    Orchestrator,
    delegate::{Backend, ToolFailure, read_line},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
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
    use tokio::net::windows::named_pipe::ClientOptions;
    // ERROR_PIPE_BUSY: the server is between accepting one client and creating the next pipe
    // instance. Waiting briefly and retrying is the documented way to connect.
    const PIPE_BUSY: i32 = 231;
    let mut attempts = 0;
    loop {
        match ClientOptions::new().open(path) {
            Ok(client) => return Ok(Box::new(client)),
            Err(error) if error.raw_os_error() == Some(PIPE_BUSY) && attempts < 100 => {
                attempts += 1;
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

// ---- Host side ---------------------------------------------------------------------------

/// Largest request line the host accepts. The delegate caps its own input at 1 MiB and wraps it
/// with a token and tool name, so this leaves room for that.
const MAX_REQUEST: usize = 1024 * 1024 + 8 * 1024;
const MAX_CONNECTIONS: usize = 64;

/// Answer one request line. Never panics on hostile input: anything malformed is an error reply.
pub async fn process(orchestrator: &Arc<Orchestrator>, line: &[u8]) -> Value {
    #[derive(serde::Deserialize)]
    struct Request {
        token: String,
        tool: String,
        #[serde(default)]
        arguments: Value,
    }
    let Ok(request) = serde_json::from_slice::<Request>(line) else {
        return json!({"ok": false, "code": "invalid_request", "message": "malformed request"});
    };
    match orchestrator
        .call(&request.token, &request.tool, request.arguments)
        .await
    {
        Ok(result) => json!({"ok": true, "result": result}),
        Err(failure) => json!({"ok": false, "code": failure.code, "message": failure.message}),
    }
}

async fn serve_connection<S: AsyncRead + AsyncWrite + Unpin>(
    orchestrator: Arc<Orchestrator>,
    stream: S,
) {
    let mut stream = BufReader::new(stream);
    while let Ok(Some(line)) = read_line(&mut stream, MAX_REQUEST).await {
        let mut reply = serde_json::to_vec(&process(&orchestrator, line.trim_ascii()).await)
            .expect("JSON values serialize");
        reply.push(b'\n');
        if stream.get_mut().write_all(&reply).await.is_err()
            || stream.get_mut().flush().await.is_err()
        {
            break;
        }
    }
}

/// A running control endpoint. Dropping it stops accepting and removes the socket file.
pub struct Endpoint {
    path: PathBuf,
    task: tokio::task::JoinHandle<()>,
}
impl Endpoint {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.task.abort();
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Start the control endpoint and tell the orchestrator where it is. Must run inside a Tokio
/// runtime. Unix: a socket in a private (0700) directory under `state_directory`. Windows: a
/// named pipe with an unguessable name that refuses remote clients. In both cases the task
/// token, not the endpoint's reachability, is what authorizes a call.
pub fn start(orchestrator: Arc<Orchestrator>, state_directory: &Path) -> std::io::Result<Endpoint> {
    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    #[cfg(unix)]
    let (path, task) = {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let directory = state_directory.join("orchestrator");
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let path = directory.join("control.sock");
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() || meta.is_dir() => {
                return Err(std::io::Error::other(
                    "unexpected file at the control socket path",
                ));
            }
            Ok(_) => std::fs::remove_file(&path)?, // a stale socket from a previous run
            Err(_) => {}
        }
        let listener = tokio::net::UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let accepting = orchestrator.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let Ok(permit) = permits.clone().acquire_owned().await else {
                    break;
                };
                let orchestrator = accepting.clone();
                tokio::spawn(async move {
                    serve_connection(orchestrator, stream).await;
                    drop(permit);
                });
            }
        });
        (path, task)
    };
    #[cfg(windows)]
    let (path, task) = {
        use tokio::net::windows::named_pipe::ServerOptions;
        let _ = state_directory;
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| std::io::Error::other("randomness unavailable"))?;
        let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let name = format!(r"\\.\pipe\acpd-orchestrator-{suffix}");
        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&name)?;
        let accepting = orchestrator.clone();
        let pipe = name.clone();
        let task = tokio::spawn(async move {
            loop {
                if server.connect().await.is_err() {
                    break;
                }
                // The next instance must exist before this client is handed off, or a client
                // arriving in between would find no pipe.
                let Ok(next) = ServerOptions::new()
                    .reject_remote_clients(true)
                    .create(&pipe)
                else {
                    break;
                };
                let connected = std::mem::replace(&mut server, next);
                let Ok(permit) = permits.clone().acquire_owned().await else {
                    break;
                };
                let orchestrator = accepting.clone();
                tokio::spawn(async move {
                    serve_connection(orchestrator, connected).await;
                    drop(permit);
                });
            }
        });
        (PathBuf::from(name), task)
    };
    orchestrator.set_control_path(path.to_string_lossy().into_owned());
    Ok(Endpoint { path, task })
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
