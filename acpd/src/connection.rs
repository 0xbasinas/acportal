//! One Tokio actor owns stdin, stdout and the child process. Subscribers never own it.
use crate::filesystem::WorkspaceFiles;
use crate::{config::RuntimeConfig, registry::AgentDefinition};
use acportal_protocol::{acp, error, request, response, validate_message};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{broadcast, mpsc, oneshot, watch},
    time::Instant,
};

/// Preserve only the protocol error code; agent text/data may contain credentials.
#[derive(Debug)]
pub struct AgentRpcError(pub i64);
impl std::fmt::Display for AgentRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "agent JSON-RPC error code {}", self.0)
    }
}
impl std::error::Error for AgentRpcError {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceAccess {
    pub read_files: bool,
    pub write_files: bool,
    pub terminal: bool,
}
impl Default for WorkspaceAccess {
    fn default() -> Self {
        Self {
            read_files: true,
            write_files: true,
            terminal: true,
        }
    }
}
impl WorkspaceAccess {
    pub fn capabilities(self) -> Value {
        json!({"fs":{"readTextFile":self.read_files,"writeTextFile":self.write_files},"terminal":self.terminal})
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub sequence: u64,
    pub message: Value,
    pub direction: &'static str,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Replay {
    pub events: Vec<Event>,
    pub pending_permissions: Vec<Value>,
    pub latest_sequence: u64,
    pub gap: bool,
}
struct Journal {
    sequence: u64,
    bytes: usize,
    events: VecDeque<(Event, usize)>,
    permissions: HashMap<String, Value>,
    max_events: usize,
    max_bytes: usize,
}
impl Journal {
    fn append(&mut self, message: Value) -> Event {
        self.append_direction(message, "agent")
    }
    fn append_direction(&mut self, message: Value, direction: &'static str) -> Event {
        self.sequence += 1;
        let size = message.to_string().len();
        let event = Event {
            sequence: self.sequence,
            message,
            direction,
        };
        self.events.push_back((event.clone(), size));
        self.bytes += size;
        while self.events.len() > self.max_events || self.bytes > self.max_bytes {
            if let Some((_, size)) = self.events.pop_front() {
                self.bytes -= size;
            }
        }
        event
    }
    fn replay(&self, after: u64) -> Replay {
        let oldest = self
            .events
            .front()
            .map_or(self.sequence + 1, |(event, _)| event.sequence);
        Replay {
            events: self
                .events
                .iter()
                .filter(|(event, _)| event.sequence > after)
                .map(|(event, _)| event.clone())
                .collect(),
            pending_permissions: self.permissions.values().cloned().collect(),
            latest_sequence: self.sequence,
            gap: after > self.sequence || after.saturating_add(1) < oldest,
        }
    }
}
#[derive(Clone)]
pub struct AcpConnection {
    frame_limit: usize,
    commands: mpsc::Sender<Control>,
    events: broadcast::Sender<Event>,
    journal: Arc<Mutex<Journal>>,
    closed: watch::Receiver<Option<String>>,
    busy: watch::Receiver<bool>,
}
enum Control {
    Request {
        method: String,
        params: Value,
        reply: oneshot::Sender<Result<Value>>,
    },
    Notify {
        method: String,
        params: Value,
        reply: oneshot::Sender<Result<()>>,
    },
    Permission {
        id: Value,
        outcome: Value,
        reply: oneshot::Sender<Result<()>>,
    },
    Shutdown(oneshot::Sender<()>),
}
struct Pending {
    method: String,
    session_id: Option<String>,
    deadline: Instant,
    reply: oneshot::Sender<Result<Value>>,
}
// Reserve one maximum-size frame for the reader in addition to queued frames.
// Parsed JSON allocation overhead and the actor's current frame are separate.
fn frame_queue_capacity(frame_limit: usize) -> usize {
    let budget = (8 * 1024 * 1024).max(frame_limit * 2);
    (budget / frame_limit).saturating_sub(1).clamp(1, 64)
}
#[derive(Serialize)]
struct OutboundRequest<'a> {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    method: &'a str,
    params: &'a Value,
}
fn check_frame_size(value: &impl Serialize, limit: usize) -> Result<()> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::Error::other("frame limit exceeded"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(limit.saturating_sub(1)), value)
        .map_err(|_| anyhow!("outbound ACP frame exceeds limit"))
}
struct HostWrite {
    callback_id: Value,
    prepared: crate::filesystem::PreparedWrite,
}
struct HostTerminalCreate {
    callback_id: Value,
    prepared: crate::terminal::PreparedTerminal,
}
fn publish_host(events: &broadcast::Sender<Event>, journal: &Arc<Mutex<Journal>>, message: Value) {
    let event = journal.lock().unwrap().append_direction(message, "host");
    let _ = events.send(event);
}
fn publish_terminal(
    events: &broadcast::Sender<Event>,
    journal: &Arc<Mutex<Journal>>,
    session_id: &str,
    terminal_id: &str,
    tool_id: &str,
    snapshot: &Value,
) {
    let status = if let Some(exit) = snapshot.get("exitStatus") {
        if exit["exitCode"] == 0 {
            "completed"
        } else {
            "failed"
        }
    } else {
        "in_progress"
    };
    let mut terminal = snapshot.clone();
    terminal["terminalId"] = json!(terminal_id);
    publish_host(
        events,
        journal,
        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session_id,"update":{"sessionUpdate":"tool_call_update","toolCallId":tool_id,"status":status,"content":[{"type":"terminal","terminalId":terminal_id}],"_meta":{"acpdTerminal":terminal}}}}),
    );
}
fn tool_update(mut tool: Value, kind: &str) -> Value {
    tool["sessionUpdate"] = json!(kind);
    tool
}
struct ActorContext<'a> {
    events: &'a broadcast::Sender<Event>,
    journal: &'a Arc<Mutex<Journal>>,
    busy: &'a watch::Sender<bool>,
    limits: &'a RuntimeConfig,
    files: Option<Arc<WorkspaceFiles>>,
    access: WorkspaceAccess,
}

impl AcpConnection {
    pub fn spawn(
        definition: &AgentDefinition,
        executable: &Path,
        cwd: &Path,
        limits: RuntimeConfig,
    ) -> Result<Self> {
        Self::spawn_inner(
            definition,
            executable,
            cwd,
            limits,
            None,
            WorkspaceAccess {
                read_files: false,
                write_files: false,
                terminal: false,
            },
        )
    }
    pub fn spawn_with_filesystem(
        definition: &AgentDefinition,
        executable: &Path,
        cwd: &Path,
        limits: RuntimeConfig,
        workspace: &Path,
    ) -> Result<Self> {
        Self::spawn_with_access(
            definition,
            executable,
            cwd,
            limits,
            workspace,
            WorkspaceAccess::default(),
        )
    }
    pub fn spawn_with_access(
        definition: &AgentDefinition,
        executable: &Path,
        cwd: &Path,
        limits: RuntimeConfig,
        workspace: &Path,
        access: WorkspaceAccess,
    ) -> Result<Self> {
        // Reserve worst-case JSON escaping plus envelope bytes inside the ACP frame limit.
        let files = Arc::new(WorkspaceFiles::open(
            workspace,
            (limits.max_frame_bytes - 256) / 6,
        )?);
        Self::spawn_inner(definition, executable, cwd, limits, Some(files), access)
    }
    fn spawn_inner(
        definition: &AgentDefinition,
        executable: &Path,
        cwd: &Path,
        limits: RuntimeConfig,
        files: Option<Arc<WorkspaceFiles>>,
        access: WorkspaceAccess,
    ) -> Result<Self> {
        definition.validate()?;
        let mut command = Command::new(executable);
        command
            .args(&definition.args)
            .envs(&definition.env)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let (mut child, mut process_tree) =
            crate::process_tree::ProcessTree::spawn(&mut command)
                .with_context(|| format!("launch agent {}", definition.id))?;
        let stdin = child.stdin.take().context("agent stdin unavailable")?;
        let stdout = child.stdout.take().context("agent stdout unavailable")?;
        let stderr = child.stderr.take().context("agent stderr unavailable")?;
        let (commands, receiver) = mpsc::channel(frame_queue_capacity(limits.max_frame_bytes));
        let (events, _) = broadcast::channel(256);
        let (closed_tx, closed) = watch::channel(None);
        let (busy_tx, busy) = watch::channel(false);
        let (frame_tx, frames) = mpsc::channel(frame_queue_capacity(limits.max_frame_bytes));
        let frame_limit = limits.max_frame_bytes;
        let stdout_task = tokio::spawn(async move {
            let mut stdout = BufReader::new(stdout);
            loop {
                let frame = read_frame(&mut stdout, frame_limit).await;
                let terminal = !matches!(frame, Ok(Some(_)));
                if frame_tx.send(frame).await.is_err() || terminal {
                    break;
                }
            }
        });
        let journal = Arc::new(Mutex::new(Journal {
            sequence: 0,
            bytes: 0,
            events: VecDeque::new(),
            permissions: HashMap::new(),
            max_events: limits.history_events,
            max_bytes: limits.history_bytes,
        }));
        // Drain without logging arbitrary stderr: agent logs may contain credentials or code.
        let stderr_task = tokio::spawn(async move {
            let mut stderr = stderr;
            let mut buffer = [0u8; 4096];
            while matches!(stderr.read(&mut buffer).await, Ok(count) if count > 0) {}
        });
        let journal_task = journal.clone();
        let events_task = events.clone();
        tokio::spawn(async move {
            let reason = actor(
                &mut child,
                stdin,
                frames,
                receiver,
                ActorContext {
                    events: &events_task,
                    journal: &journal_task,
                    busy: &busy_tx,
                    limits: &limits,
                    files,
                    access,
                },
            )
            .await;
            if process_tree.terminate().is_err() {
                tracing::warn!("agent descendant cleanup failed");
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            stderr_task.abort();
            stdout_task.abort();
            journal_task.lock().unwrap().permissions.clear();
            let _ = busy_tx.send(false);
            let _ = closed_tx.send(Some(reason));
        });
        Ok(Self {
            frame_limit,
            commands,
            events,
            journal,
            closed,
            busy,
        })
    }
    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        check_frame_size(
            &OutboundRequest {
                jsonrpc: "2.0",
                id: Some(u64::MAX),
                method,
                params: &params,
            },
            self.frame_limit,
        )?;
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Control::Request {
                method: method.into(),
                params,
                reply,
            })
            .await
            .context("agent connection closed")?;
        result.await.context("agent connection closed")?
    }
    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        check_frame_size(
            &OutboundRequest {
                jsonrpc: "2.0",
                id: None,
                method,
                params: &params,
            },
            self.frame_limit,
        )?;
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Control::Notify {
                method: method.into(),
                params,
                reply,
            })
            .await
            .context("agent connection closed")?;
        result.await.context("agent connection closed")?
    }
    pub async fn permission(&self, id: Value, outcome: Value) -> Result<()> {
        #[derive(Serialize)]
        struct Outcome<'a> {
            outcome: &'a Value,
        }
        #[derive(Serialize)]
        struct Answer<'a> {
            jsonrpc: &'static str,
            id: &'a Value,
            result: Outcome<'a>,
        }
        check_frame_size(
            &Answer {
                jsonrpc: "2.0",
                id: &id,
                result: Outcome { outcome: &outcome },
            },
            self.frame_limit,
        )?;
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Control::Permission { id, outcome, reply })
            .await
            .context("agent connection closed")?;
        result.await.context("agent connection closed")?
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    // Subscribe BEFORE replay, then ignore live events <= latest_sequence to avoid a race.
    pub fn replay(&self, after: u64) -> Replay {
        self.journal.lock().unwrap().replay(after)
    }
    pub fn record(&self, message: Value, direction: &'static str) {
        let event = self
            .journal
            .lock()
            .unwrap()
            .append_direction(message, direction);
        let _ = self.events.send(event);
    }
    pub fn is_busy(&self) -> bool {
        *self.busy.borrow()
    }
    pub fn closed_reason(&self) -> Option<String> {
        self.closed.borrow().clone()
    }
    pub async fn wait_closed(&self) -> String {
        let mut receiver = self.closed.clone();
        loop {
            if let Some(reason) = receiver.borrow().clone() {
                return reason;
            }
            if receiver.changed().await.is_err() {
                return "connection task stopped".into();
            }
        }
    }
    pub async fn shutdown(&self) {
        let (reply, result) = oneshot::channel();
        if self.commands.send(Control::Shutdown(reply)).await.is_ok() {
            let _ = result.await;
        }
        self.wait_closed().await;
    }
}

/// Bounded read without allocating an unbounded line before checking its size.
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    max: usize,
) -> Result<Option<Value>> {
    let mut bytes = Vec::with_capacity(4096.min(max));
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            bail!("unterminated ACP frame")
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |position| position + 1);
        if bytes.len() + count > max {
            bail!("ACP frame exceeds configured limit")
        }
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            break;
        }
    }
    let message: Value = serde_json::from_slice(&bytes).context("invalid ACP JSON")?;
    validate_message(&message)?;
    Ok(Some(message))
}
async fn write(stdin: &mut ChildStdin, message: &Value, max: usize) -> Result<()> {
    validate_message(message)?;
    let mut bytes = serde_json::to_vec(message)?;
    bytes.push(b'\n');
    if bytes.len() > max {
        bail!("outbound ACP frame exceeds configured limit")
    }
    // A blocked child stdin must not prevent cancellation and shutdown forever.
    tokio::time::timeout(Duration::from_secs(5), async {
        stdin.write_all(&bytes).await?;
        stdin.flush().await
    })
    .await
    .context("agent stdin stalled")??;
    Ok(())
}

async fn actor(
    child: &mut Child,
    mut stdin: ChildStdin,
    mut frames: mpsc::Receiver<Result<Option<Value>>>,
    mut commands: mpsc::Receiver<Control>,
    context: ActorContext<'_>,
) -> String {
    let ActorContext {
        events,
        journal,
        busy,
        limits,
        files,
        access,
    } = context;
    let mut pending: HashMap<u64, Pending> = HashMap::new();
    let mut next_id = 0u64;
    let mut file_session_id: Option<String> = None;
    let mut files_ready = false;
    let mut host_writes: HashMap<String, HostWrite> = HashMap::new();
    let mut host_terminals: HashMap<String, HostTerminalCreate> = HashMap::new();
    let mut terminals: Option<crate::terminal::TerminalService> = None;
    let mut terminal_tools: HashMap<String, (String, Value)> = HashMap::new();
    let mut terminal_waits = tokio::task::JoinSet::<(Value, Result<Value>)>::new();
    let mut wait_ids = std::collections::HashSet::new();
    let mut terminal_poll = Instant::now();
    let mut timer = tokio::time::interval(Duration::from_millis(100));
    // A dedicated bounded reader preserves partial frames across select branches.
    let reason = loop {
        tokio::select! {
            received = frames.recv() => {
                let message = match received { Some(Ok(Some(value))) => value,
                    Some(Ok(None)) | None => break "agent stdout closed".into(), Some(Err(_)) => break "malformed or oversized ACP frame".into() };
                if message.get("method").is_none() {
                    if let Some(id) = message["id"].as_u64()
                        && let Some(waiter) = pending.remove(&id) {
                            if message.get("error").is_none() {
                                if waiter.method == "session/new" {file_session_id=message["result"]["sessionId"].as_str().map(str::to_owned);files_ready=file_session_id.is_some();}
                                else if waiter.method == "session/load" {file_session_id=waiter.session_id.clone();files_ready=file_session_id.is_some();}
                                if access.terminal && files_ready && terminals.is_none() && let (Some(files),Some(session_id))=(&files,&file_session_id) {
                                    terminals=crate::terminal::TerminalService::new(session_id,files.workspace(),((limits.max_frame_bytes-512)/6).min(65536)).ok();
                                }
                            }
                            if waiter.method == "session/prompt" { let _ = busy.send(false); }
                            let result = if message.get("error").is_some() {
                                // Do not include agent-provided error text: it may contain secrets.
                                Err(AgentRpcError(message["error"]["code"].as_i64().unwrap_or(-32603)).into())
                            } else { Ok(message["result"].clone()) };
                            let _ = waiter.reply.send(result);
                        }
                    continue;
                }
                if message["method"] == "session/update" {
                    for waiter in pending.values_mut().filter(|waiter| waiter.method == "session/prompt") {
                        waiter.deadline = Instant::now() + Duration::from_secs(limits.request_timeout_seconds);
                    }
                }
                if let Some(id) = message.get("id") {
                    let denied=match message["method"].as_str() {
                        Some("fs/read_text_file")=>!access.read_files,
                        Some("fs/write_text_file")=>!access.write_files,
                        Some(method) if method.starts_with("terminal/")=>!access.terminal,
                        _=>false,
                    };
                    if denied {
                        if write(&mut stdin,&error(id.clone(),-32601,"Client access is disabled for this session"),limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}
                        continue;
                    }
                    if message["method"].as_str().is_some_and(|method|method.starts_with("terminal/")) {
                        if host_terminals.values().any(|entry|entry.callback_id==*id) || host_writes.values().any(|entry|entry.callback_id==*id) || wait_ids.contains(&id.to_string()) {break "duplicate client callback id".into()}
                        let params=&message["params"];
                        if !matches!(message["method"].as_str(),Some("terminal/create"|"terminal/output"|"terminal/wait_for_exit"|"terminal/kill"|"terminal/release")) {
                            if write(&mut stdin,&error(id.clone(),-32601,"Terminal method unsupported"),limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}continue;
                        }
                        if !files_ready || params["sessionId"]!=json!(file_session_id) {
                            if write(&mut stdin,&error(id.clone(),-32602,"Terminal session unavailable"),limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}continue;
                        }
                        let Some(service)=terminals.as_mut() else {
                            if write(&mut stdin,&error(id.clone(),-32602,"Terminal runtime unavailable"),limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}continue;
                        };
                        let result=if message["method"]=="terminal/create" {
                            match service.prepare(params.clone()) {
                                Err(error)=>Err(error),Ok(prepared)=> {
                                    let consent_id=json!(format!("acpd-command-{}",uuid::Uuid::new_v4()));
                                    let operation=prepared.operation();
                                    let tool=json!({"toolCallId":consent_id,"title":format!("Run {}",std::path::Path::new(operation["command"].as_str().unwrap()).file_name().unwrap_or_default().to_string_lossy()),"kind":"execute","status":"pending","rawInput":operation,"locations":[{"path":operation["cwd"]}]});
                                    let consent=json!({"jsonrpc":"2.0","id":consent_id,"method":"session/request_permission","params":{"sessionId":file_session_id,"toolCall":tool,"_meta":{"acpdSource":"host-terminal"},"options":[{"optionId":"acpd-command-deny","name":"Deny","kind":"reject_once"},{"optionId":"acpd-command-allow","name":"Run once","kind":"allow_once"}]}});
                                    let size=consent.to_string().len();
                                    let inserted={let mut history=journal.lock().unwrap();let bytes:usize=history.permissions.values().map(|value|value.to_string().len()).sum();if size+256>limits.max_frame_bytes || history.permissions.len()>=128 || bytes+size>limits.history_bytes {false} else {history.permissions.insert(consent_id.to_string(),consent.clone());true}};
                                    if !inserted {Err(anyhow!("permission capacity exceeded"))} else {
                                        host_terminals.insert(consent_id.to_string(),HostTerminalCreate{callback_id:id.clone(),prepared});
                                        publish_host(events,journal,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":file_session_id,"update":tool_update(tool,"tool_call")}}));
                                        publish_host(events,journal,consent);continue;
                                    }
                                }
                            }
                        } else {
                            let session_id=file_session_id.as_deref().unwrap();let terminal_id=params["terminalId"].as_str().unwrap_or("");
                            match message["method"].as_str().unwrap() {
                                "terminal/output"=>service.output(session_id,terminal_id),
                                "terminal/kill"=>service.kill(session_id,terminal_id).await,
                                "terminal/release"=> {
                                    match service.kill(session_id,terminal_id).await {
                                        Err(error)=>Err(error),Ok(_)=> {
                                            let snapshot=service.output(session_id,terminal_id);
                                            let result=service.release(session_id,terminal_id).await;
                                            if result.is_ok() && let (Some((tool_id,_)),Ok(snapshot))=(terminal_tools.remove(terminal_id),snapshot) {publish_terminal(events,journal,session_id,terminal_id,&tool_id,&snapshot);}
                                            result
                                        }
                                    }
                                },
                                "terminal/wait_for_exit"=> {
                                    match service.exit_receiver(session_id,terminal_id) {
                                        Ok(exit) if terminal_waits.len()<64 && wait_ids.insert(id.to_string())=> {let id=id.clone();terminal_waits.spawn(async move {(id,crate::terminal::TerminalService::wait(exit).await)});continue;},
                                        _=>Err(anyhow!("terminal wait unavailable")),
                                    }
                                },
                                _=>Err(anyhow!("unknown terminal method")),
                            }
                        };
                        let answer=match result {Ok(value)=>response(id.clone(),value),Err(_)=>error(id.clone(),-32602,"Terminal request denied or unavailable")};
                        if write(&mut stdin,&answer,limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}
                        continue;
                    }
                    if message["method"] == "fs/read_text_file" && let Some(files)=&files {
                        let result=if let Some(session_id)=file_session_id.clone() {
                            let files=files.clone();
                            crate::filesystem::read_async(files,message["params"].clone(),session_id).await
                        } else {Err(anyhow!("filesystem session is not ready"))};
                        let reply=match result {Ok(value)=>response(id.clone(),value),Err(_)=>error(id.clone(),-32000,"Workspace text read denied or unavailable")};
                        if write(&mut stdin,&reply,limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}
                        continue;
                    } else if message["method"] == "fs/write_text_file" && let Some(files)=&files {
                        if !files_ready {if write(&mut stdin,&error(id.clone(),-32000,"Filesystem writes require a ready session"),limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}continue;}
                        let Some(session_id)=file_session_id.clone() else {
                            if write(&mut stdin,&error(id.clone(),-32000,"Filesystem session not ready"),limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}
                            continue;
                        };
                        if host_writes.values().any(|entry|entry.callback_id==*id) {break "duplicate filesystem callback id".into()}
                        let prepared=match crate::filesystem::prepare_write_async(files.clone(),message["params"].clone(),session_id.clone()).await {
                            Ok(value)=>value,Err(_)=> {if write(&mut stdin,&error(id.clone(),-32000,"Workspace text write denied or unavailable"),limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()} continue;}
                        };
                        let consent_id=json!(format!("acpd-write-{}",uuid::Uuid::new_v4()));
                        let tool_id=consent_id.as_str().unwrap().to_owned();
                        let tool=json!({"toolCallId":tool_id,"title":format!("Write {}",prepared.path.file_name().unwrap().to_string_lossy()),"kind":"edit","status":"pending","rawInput":{"path":prepared.path},"content":[{"type":"diff","path":prepared.path,"oldText":prepared.old_text,"newText":prepared.content}]});
                        let consent=json!({"jsonrpc":"2.0","id":consent_id,"method":"session/request_permission","params":{"sessionId":session_id,"toolCall":tool,"_meta":{"acpdSource":"host-filesystem"},"options":[{"optionId":"acpd-write-deny","name":"Deny","kind":"reject_once"},{"optionId":"acpd-write-allow","name":"Allow write once","kind":"allow_once"}]}});
                        let consent_size=consent.to_string().len();
                        let inserted={let mut history=journal.lock().unwrap();let bytes:usize=history.permissions.values().map(|value|value.to_string().len()).sum();if consent_size+256>limits.max_frame_bytes || history.permissions.len()>=128 || bytes+consent_size>limits.history_bytes {false} else {history.permissions.insert(consent_id.to_string(),consent.clone());true}};
                        if !inserted {if write(&mut stdin,&error(id.clone(),-32000,"Host permission capacity exceeded"),limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()} continue;}
                        host_writes.insert(consent_id.to_string(),HostWrite {callback_id:id.clone(),prepared});
                        publish_host(events,journal,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session_id,"update":tool_update(tool,"tool_call")}}));
                        publish_host(events,journal,consent);
                        continue;
                    } else if message["method"] == "session/request_permission" {
                        if serde_json::from_value::<acp::RequestPermissionRequest>(message["params"].clone()).is_err() {
                            if write(&mut stdin, &error(id.clone(), -32602, "Invalid permission request"), limits.max_frame_bytes).await.is_err() { break "agent stdin failed".into() }
                            continue;
                        }
                        let inserted = {
                            let mut history = journal.lock().unwrap();
                            let key = id.to_string();
                            let permission_bytes: usize = history.permissions.values().map(|value| value.to_string().len()).sum();
                            if history.permissions.contains_key(&key) || history.permissions.len() >= 128 || permission_bytes + message.to_string().len() > limits.history_bytes { false }
                            else { history.permissions.insert(key, message.clone()); true }
                        };
                        if !inserted { break "duplicate permission id or too many pending permissions".into() }
                    } else {
                        if write(&mut stdin, &error(id.clone(), -32601, "Client capability not supported"), limits.max_frame_bytes).await.is_err() { break "agent stdin failed".into() }
                        continue;
                    }
                }
                let event = journal.lock().unwrap().append(message);
                let _ = events.send(event);
            }
            command = commands.recv() => {
                match command {
                    Some(Control::Request { method, params, reply }) => {
                        if pending.len() >= 64 || (method == "session/prompt" && pending.values().any(|value| value.method == method)) {
                            let _ = reply.send(Err(anyhow!("agent is busy or request limit reached"))); continue;
                        }
                        next_id += 1;
                        let session_id=params["sessionId"].as_str().map(str::to_owned);
                        let message = request(next_id, &method, params);
                        // Oversized input is a caller error; it must not kill a healthy agent.
                        if message.to_string().len() + 1 > limits.max_frame_bytes || validate_message(&message).is_err() {
                            let _ = reply.send(Err(anyhow!("invalid or oversized request"))); continue;
                        }
                        if write(&mut stdin, &message, limits.max_frame_bytes).await.is_err() {
                            let _ = reply.send(Err(anyhow!("agent stdin failed"))); break "agent stdin failed".into();
                        }
                        if method == "session/prompt" { let _ = busy.send(true); }
                        // Loading may make client callbacks before its final response.
                        if method == "session/load" { file_session_id = session_id.clone(); files_ready=false; }
                        pending.insert(next_id, Pending { method, session_id, deadline: Instant::now() + Duration::from_secs(limits.request_timeout_seconds), reply });
                    }
                    Some(Control::Notify { method, params, reply }) => {
                        if method == "session/cancel" {
                            let permissions: Vec<Value> = journal.lock().unwrap().permissions.values().cloned().collect();
                            let mut failed = false;
                            for permission in permissions {
                                let answer=if let Some(host_terminal)=host_terminals.remove(&permission["id"].to_string()) {
                                    publish_host(events,journal,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":file_session_id,"update":{"sessionUpdate":"tool_call_update","toolCallId":permission["params"]["toolCall"]["toolCallId"],"status":"failed"}}}));
                                    error(host_terminal.callback_id,-32800,"Terminal creation cancelled")
                                } else if let Some(host_write)=host_writes.remove(&permission["id"].to_string()) {
                                    publish_host(events,journal,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":file_session_id,"update":{"sessionUpdate":"tool_call_update","toolCallId":permission["params"]["toolCall"]["toolCallId"],"status":"failed"}}}));
                                    error(host_write.callback_id,-32800,"Host file write cancelled")
                                } else {response(permission["id"].clone(), json!({"outcome":{"outcome":"cancelled"}}))};
                                if write(&mut stdin, &answer, limits.max_frame_bytes).await.is_err() { failed = true; break }
                            }
                            journal.lock().unwrap().permissions.clear();
                            if let Some(service)=&terminals {service.stop_all();}
                            for waiter in pending.values_mut().filter(|waiter| waiter.method == "session/prompt") {
                                waiter.deadline = Instant::now() + Duration::from_secs(limits.request_timeout_seconds);
                            }
                            if failed { let _ = reply.send(Err(anyhow!("agent stdin failed"))); break "agent stdin failed".into() }
                        }
                        let result = write(&mut stdin, &json!({"jsonrpc":"2.0","method":method,"params":params}), limits.max_frame_bytes).await;
                        let failed = result.is_err();
                        let _ = reply.send(result);
                        if failed { break "agent stdin failed".into() }
                    }
                    Some(Control::Permission { id, outcome, reply }) => {
                        let permission = journal.lock().unwrap().permissions.get(&id.to_string()).cloned();
                        let Some(permission) = permission else { let _ = reply.send(Err(anyhow!("permission request is not pending"))); continue };
                        let valid = outcome["outcome"] == "cancelled" || (outcome["outcome"] == "selected" && outcome["optionId"].is_string() &&
                            permission["params"]["options"].as_array().is_some_and(|options| options.iter().any(|option| option["optionId"] == outcome["optionId"])));
                        if !valid { let _ = reply.send(Err(anyhow!("invalid permission outcome"))); continue }
                        let answer=if let Some(host_terminal)=host_terminals.remove(&id.to_string()) {
                            let accepted=outcome["outcome"]=="selected" && outcome["optionId"]=="acpd-command-allow";
                            let result=if accepted {terminals.as_mut().unwrap().create_approved(host_terminal.prepared)} else {Err(anyhow!("command denied"))};
                            let tool_id=permission["params"]["toolCall"]["toolCallId"].as_str().unwrap().to_string();
                            if let Ok(created)=&result {terminal_tools.insert(created["terminalId"].as_str().unwrap().to_owned(),(tool_id.clone(),json!({})));}
                            publish_host(events,journal,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":file_session_id,"update":{"sessionUpdate":"tool_call_update","toolCallId":tool_id,"status":if result.is_ok(){"in_progress"}else{"failed"}}}}));
                            match result {Ok(value)=>response(host_terminal.callback_id,value),Err(_)=>error(host_terminal.callback_id,-32602,"Terminal creation denied or unavailable")}
                        } else if let Some(host_write)=host_writes.remove(&id.to_string()) {
                            let accepted=outcome["outcome"]=="selected" && outcome["optionId"]=="acpd-write-allow";
                            let result=if accepted {crate::filesystem::commit_write_async(files.as_ref().unwrap().clone(),host_write.prepared).await} else {Err(anyhow!("write denied"))};
                            publish_host(events,journal,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":file_session_id,"update":{"sessionUpdate":"tool_call_update","toolCallId":permission["params"]["toolCall"]["toolCallId"],"status":if result.is_ok(){"completed"}else{"failed"}}}}));
                            match result {Ok(())=>response(host_write.callback_id,json!({})),Err(_)=>error(host_write.callback_id,-32000,"Host write denied, changed or failed; do not retry automatically")}
                        } else {response(id.clone(), json!({"outcome":outcome}))};
                        let result = write(&mut stdin, &answer, limits.max_frame_bytes).await;
                        let failed = result.is_err();
                        if !failed {
                            journal.lock().unwrap().permissions.remove(&id.to_string());
                            for waiter in pending.values_mut().filter(|waiter| waiter.method == "session/prompt") {
                                waiter.deadline = Instant::now() + Duration::from_secs(limits.request_timeout_seconds);
                            }
                        }
                        let _ = reply.send(result);
                        if failed { break "agent stdin failed".into() }
                    }
                    Some(Control::Shutdown(reply)) => {
                        let _ = reply.send(());
                        break "terminated by host".into();
                    }
                    None => break "host released connection".into(),
                }
            }
            _ = timer.tick() => {
                if terminal_poll.elapsed()>=Duration::from_millis(250) {
                    terminal_poll=Instant::now();
                    if let (Some(service),Some(session_id))=(&terminals,&file_session_id) {for (terminal_id,(tool_id,last)) in &mut terminal_tools {
                        if let Ok(snapshot)=service.output(session_id,terminal_id) && snapshot!=*last {publish_terminal(events,journal,session_id,terminal_id,tool_id,&snapshot);*last=snapshot;
                            for waiter in pending.values_mut().filter(|waiter|waiter.method=="session/prompt") {waiter.deadline=Instant::now()+Duration::from_secs(limits.request_timeout_seconds);}
                        }
                    }}
                }
                let awaiting_permission = !journal.lock().unwrap().permissions.is_empty();
                if pending.values().any(|waiter| waiter.deadline <= Instant::now() && !(waiter.method == "session/prompt" && (awaiting_permission || !terminal_waits.is_empty()))) { break "agent request timed out; connection terminated to prevent ambiguous retries".into() }
                match child.try_wait() {
                    Ok(Some(status)) => break format!("agent exited with {status}"),
                    Err(_) => break "agent process status failed".into(),
                    Ok(None) => {}
                }
            }
            completed=terminal_waits.join_next(),if !terminal_waits.is_empty()=> {
                if let Some(Ok((id,result)))=completed {wait_ids.remove(&id.to_string());let answer=match result {Ok(value)=>response(id,value),Err(_)=>error(id,-32602,"Terminal wait failed")};if write(&mut stdin,&answer,limits.max_frame_bytes).await.is_err() {break "agent stdin failed".into()}
                    for waiter in pending.values_mut().filter(|waiter|waiter.method=="session/prompt") {waiter.deadline=Instant::now()+Duration::from_secs(limits.request_timeout_seconds);}
                }
            }
        }
    };
    terminal_waits.abort_all();
    if let Some(service) = &mut terminals {
        service.shutdown().await;
    }
    for (_, waiter) in pending {
        let _ = waiter.reply.send(Err(anyhow!(reason.clone())));
    }
    reason
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_counts_json_escaping_and_envelope_without_retaining_encoded_bytes() {
        let params = json!({"text":"\n\"é".repeat(100)});
        let request = OutboundRequest {
            jsonrpc: "2.0",
            id: Some(u64::MAX),
            method: "session/prompt",
            params: &params,
        };
        let size = serde_json::to_vec(&request).unwrap().len();
        assert!(check_frame_size(&request, size + 1).is_ok());
        assert!(check_frame_size(&request, size).is_err());
        assert!(check_frame_size(&request, 0).is_err());
    }
    #[test]
    fn frame_queue_reserves_reader_space_across_supported_frame_limits() {
        for limit in [
            1024,
            64 * 1024,
            1024 * 1024,
            3 * 1024 * 1024,
            16 * 1024 * 1024,
        ] {
            let capacity = frame_queue_capacity(limit);
            assert!((1..=64).contains(&capacity));
            assert!((capacity + 1) * limit <= (8 * 1024 * 1024).max(limit * 2));
        }
        assert_eq!(frame_queue_capacity(1024 * 1024), 7);
        assert_eq!(frame_queue_capacity(16 * 1024 * 1024), 1);
    }
    #[tokio::test]
    async fn frame_limits_and_truncated_input() {
        let mut reader = BufReader::new(&b"{\"jsonrpc\":\"2.0\",\"method\":\"x\"}\n"[..]);
        assert!(read_frame(&mut reader, 8).await.is_err());
        let mut reader = BufReader::new(&b"{\"jsonrpc\":\"2.0\",\"method\":\"x\"}"[..]);
        assert!(read_frame(&mut reader, 1024).await.is_err());
        let mut reader = BufReader::new(&b"{\"jsonrpc\":\"2.0\",\"method\":\"x\"}\n"[..]);
        assert!(read_frame(&mut reader, 1024).await.unwrap().is_some());
        assert!(read_frame(&mut reader, 1024).await.unwrap().is_none());
    }
    #[test]
    fn bounded_history_reports_eviction_and_retains_pending_requests() {
        let mut journal = Journal {
            sequence: 0,
            bytes: 0,
            events: VecDeque::new(),
            permissions: HashMap::new(),
            max_events: 1,
            max_bytes: 1024,
        };
        journal.permissions.insert("1".into(), json!({"id":1}));
        journal.append(json!({"first":true}));
        journal.append(json!({"second":true}));
        let replay = journal.replay(0);
        assert!(replay.gap);
        assert_eq!(replay.events.len(), 1);
        assert_eq!(replay.pending_permissions.len(), 1);
        assert!(!journal.replay(1).gap);
    }
}
