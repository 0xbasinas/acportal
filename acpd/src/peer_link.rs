//! Client side of the peer link: a WebSocket to another host's `/acp` bridge, presented to
//! `AcpConnection` as the same newline-framed JSON it gets from an agent's stdio.
//!
//! The actor never learns it is talking to a network. Two in-memory pipes carry the lines and
//! one pump task turns each line into a WebSocket text frame and back. The credential comes from
//! the peer store and is sent only to that peer's own origin. TLS uses the bundled web roots and
//! is never switched off; plain `ws://` exists for loopback only (tests and a local Goose).
//!
//! **Losing the socket.** The far bridge keeps a session for a while after its socket goes, and
//! can replay its journal to a client that comes back with `session/load`. When the far host says
//! (in its `initialize` answer) that it numbers its journal frames, the pump uses that to get a
//! lost link back without the actor noticing: it dials again with backoff, repeats `initialize`
//! and `session/load` itself, drops replayed frames the actor already has (by their sequence
//! number, and by request id for the few that carry none), and then sends what the actor wrote
//! meanwhile. Nothing is done twice. What the actor wrote while the link was down is held and sent
//! once. What it wrote just before the socket died may or may not have arrived, so the far host
//! lists the request and answer ids it received, and only the lines missing from that list are
//! sent again. If the far host evicted journal events the actor never saw, if it refuses the
//! credential, or if the window runs out, the link ends the way it always did: the actor sees
//! the "agent" exit.
use crate::peers::PeerStore;
use anyhow::{Context, Result};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream},
    sync::mpsc,
    time::Instant,
};
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
    protocol::{CloseFrame, WebSocketConfig, frame::coding::CloseCode},
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const SEND_TIMEOUT: Duration = Duration::from_secs(5);
const PING_EVERY: Duration = Duration::from_secs(20);
/// Three missed pings: the link is gone even if TCP has not noticed.
const SILENCE_LIMIT: Duration = Duration::from_secs(70);
/// One step of getting a link back (the `initialize` answer, the `session/load` answer).
const RESUME_STEP: Duration = Duration::from_secs(10);
/// What the actor may write while the link is down; more than this and the link ends.
const MAX_HELD_LINES: usize = 256;
/// Agent-to-client request ids remembered so a replay does not deliver one twice.
const REMEMBERED_REQUESTS: usize = 1024;
const RESUME_INIT_ID: &str = "acpd-resume-initialize";
const RESUME_LOAD_ID: &str = "acpd-resume-load";
/// The close reason that tells an acpd bridge to end the sessions at once instead of keeping
/// them for a reattach.
pub const END_SESSIONS: &str = "end-sessions";

/// How a link behaves when its socket is lost.
#[derive(Clone, Copy, Debug)]
pub struct Resume {
    /// Total time spent getting a lost link back. Zero: a lost link ends the session.
    pub window: Duration,
    /// The wait before the first new attempt; it doubles up to `max_wait`.
    pub first_wait: Duration,
    pub max_wait: Duration,
}
impl Resume {
    /// A lost link ends the session (the behaviour before reconnecting existed).
    pub const OFF: Resume = Resume {
        window: Duration::ZERO,
        first_wait: Duration::ZERO,
        max_wait: Duration::ZERO,
    };
    /// Try for `seconds` (0 turns it off), waiting 1 s, then 2, 4 and at most 8 s between tries.
    pub fn within_seconds(seconds: u64) -> Self {
        if seconds == 0 {
            return Self::OFF;
        }
        Self {
            window: Duration::from_secs(seconds),
            first_wait: Duration::from_secs(1),
            max_wait: Duration::from_secs(8),
        }
    }
    fn enabled(&self) -> bool {
        !self.window.is_zero()
    }
}

/// The two ends handed to `AcpConnection::over_transport`.
pub struct Link {
    pub reader: DuplexStream,
    pub writer: DuplexStream,
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type Sink = SplitSink<Socket, Message>;
type Stream = SplitStream<Socket>;

/// Why a socket could not be opened.
enum DialError {
    /// The credential lookup failed (the peer was removed or its origin changed).
    Credential(anyhow::Error),
    /// The far host answered the upgrade with an HTTP status.
    Http(u16),
    Timeout,
    Failed,
}
impl DialError {
    /// Trying again cannot help: the credential or the route is refused.
    fn permanent(&self) -> bool {
        matches!(self, Self::Credential(_) | Self::Http(401 | 403 | 404))
    }
    fn into_error(self) -> anyhow::Error {
        match self {
            Self::Credential(error) => error,
            // Say what happened, never what the server said: its text is not ours to repeat.
            Self::Http(status) => {
                anyhow::anyhow!("the peer refused the connection (HTTP {status})")
            }
            Self::Timeout => anyhow::anyhow!("the peer did not answer in time"),
            Self::Failed => anyhow::anyhow!("could not connect to the peer"),
        }
    }
}

/// Open a WebSocket to `url` as `peer`. The peer's credential is looked up in the store and
/// refused unless `url` points at the origin the peer record names.
async fn dial(
    store: &PeerStore,
    peer: &str,
    url: &str,
    max_frame: usize,
) -> Result<Socket, DialError> {
    let (store, name, target) = (store.clone(), peer.to_owned(), url.to_owned());
    let (_, (header, value)) =
        match tokio::task::spawn_blocking(move || store.credential_for(&name, &target)).await {
            Ok(Ok(found)) => found,
            Ok(Err(error)) => return Err(DialError::Credential(error)),
            Err(_) => return Err(DialError::Failed),
        };
    let build = || -> Result<_> {
        let mut request = url.into_client_request().context("invalid peer URL")?;
        let mut credential = HeaderValue::from_str(&value).context("invalid credential")?;
        credential.set_sensitive(true);
        request.headers_mut().insert(
            HeaderName::from_bytes(header.as_bytes()).context("invalid credential header")?,
            credential,
        );
        Ok(request)
    };
    let request = build().map_err(DialError::Credential)?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = WebSocketConfig::default()
        .max_message_size(Some(max_frame))
        .max_frame_size(Some(max_frame));
    match tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(request, Some(config), false),
    )
    .await
    {
        Err(_) => Err(DialError::Timeout),
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response))) => {
            Err(DialError::Http(response.status().as_u16()))
        }
        Ok(Err(_)) => Err(DialError::Failed),
        Ok(Ok((socket, _))) => Ok(socket),
    }
}

/// Connect to `url` as `peer`. With `resume` enabled, a lost socket is replaced behind the actor's
/// back where the far host supports it (see the module comment).
pub async fn connect(
    store: &PeerStore,
    peer: &str,
    url: &str,
    max_frame: usize,
    resume: Resume,
) -> Result<Link> {
    let socket = dial(store, peer, url, max_frame)
        .await
        .map_err(DialError::into_error)?;
    let capacity = (max_frame * 2).max(64 * 1024);
    let (to_peer, from_actor) = tokio::io::duplex(capacity);
    let (to_actor, from_peer) = tokio::io::duplex(capacity);
    let target = Target {
        store: store.clone(),
        peer: peer.to_owned(),
        url: url.to_owned(),
        max_frame,
        resume,
    };
    tokio::spawn(pump(socket, from_actor, to_actor, target));
    Ok(Link {
        reader: from_peer,
        writer: to_peer,
    })
}

/// Where a link goes, kept so a lost one can be dialled again.
struct Target {
    store: PeerStore,
    peer: String,
    url: String,
    max_frame: usize,
    resume: Resume,
}

/// Read newline-terminated lines of at most `max` bytes into a channel. A task of its own, because
/// `read_until` is not cancel-safe inside `select!` and a channel receive is.
async fn read_lines(from_actor: DuplexStream, max: usize, lines: mpsc::Sender<Vec<u8>>) {
    let mut reader = BufReader::new(from_actor);
    loop {
        let mut line = Vec::new();
        let mut limited = (&mut reader).take(max as u64 + 1);
        match limited.read_until(b'\n', &mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) if line.len() > max => return, // oversize: end the link rather than buffer it
            Ok(_) => {
                if lines.send(line).await.is_err() {
                    return;
                }
            }
        }
    }
}

/// What a request the actor sent is waiting for.
enum Asked {
    Initialize,
    New(Option<String>),
    Load(String),
}

/// What the pump has to remember to get a lost link back.
#[derive(Default)]
struct Tracker {
    /// The actor's own `initialize` parameters, sent again on a new socket.
    init_params: Option<Value>,
    cwd: Option<String>,
    /// The far host's session id (its bridge id), once the actor has one.
    session: Option<String>,
    /// The far host said it numbers its journal frames, so a replay can be deduplicated.
    numbered: bool,
    pending: HashMap<String, Asked>,
    /// The highest journal sequence number delivered to the actor.
    last_seq: u64,
    delivered: VecDeque<String>,
    delivered_set: HashSet<String>,
    /// What the actor wrote that is still waiting for an answer, oldest first, as `(key, line)`:
    /// `q:<id>` for a request (until its response arrives) and `p:<id>` for an answer to one of
    /// the agent's requests. When the link comes back, the far host says which of these keys it
    /// received; only the ones it did not are sent again, so nothing is ever done twice.
    outstanding: VecDeque<(String, String)>,
}
/// How many unanswered lines are remembered.
const REMEMBERED_LINES: usize = 1024;

enum Verdict {
    Drop,
    /// Deliver; `Some` is the text to deliver when the frame had to be rewritten.
    Deliver(Option<String>),
}

impl Tracker {
    fn can_resume(&self) -> bool {
        self.numbered && self.session.is_some() && self.init_params.is_some()
    }

    /// A line the actor wrote, on its way to the far host. `initialize` is asked to number its
    /// frames; the three requests that name a session are remembered.
    fn outgoing(&mut self, text: String) -> String {
        let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
            return text;
        };
        let id_key = value.get("id").map(Value::to_string);
        let Some(method) = value["method"].as_str().map(str::to_owned) else {
            // An answer to a request of the agent's (a permission decision).
            if let Some(id) = &id_key
                && (value.get("result").is_some() || value.get("error").is_some())
            {
                self.remember_sent(format!("p:{id}"), text.clone());
            }
            return text;
        };
        let key = id_key.clone();
        if let Some(id) = &id_key
            && !matches!(
                method.as_str(),
                "initialize" | "session/new" | "session/load"
            )
        {
            self.remember_sent(format!("q:{id}"), text.clone());
        }
        let asked = match method.as_str() {
            "initialize" => {
                self.init_params = Some(value["params"].clone());
                Some(Asked::Initialize)
            }
            "session/new" => Some(Asked::New(
                value["params"]["cwd"].as_str().map(str::to_owned),
            )),
            "session/load" => value["params"]["sessionId"]
                .as_str()
                .map(|id| Asked::Load(id.to_owned())),
            _ => None,
        };
        let Some(asked) = asked else { return text };
        if let Some(key) = key {
            self.pending.insert(key, asked);
        }
        if method == "initialize" {
            ask_to_number_frames(&mut value["params"]);
            return value.to_string();
        }
        text
    }

    fn remember_sent(&mut self, key: String, line: String) {
        self.outstanding.push_back((key, line));
        while self.outstanding.len() > REMEMBERED_LINES {
            self.outstanding.pop_front();
        }
    }

    /// A frame from the far host, on its way to the actor. `replaying` is true between a resume's
    /// `session/load` and its answer, when the far host sends its whole journal again.
    fn incoming(&mut self, text: &str, replaying: bool) -> Verdict {
        let Ok(mut value) = serde_json::from_str::<Value>(text) else {
            return Verdict::Deliver(None);
        };
        let mut rewritten = false;
        let numbered = value
            .as_object_mut()
            .and_then(|object| object.remove("acpdSeq"));
        if let Some(seq) = numbered.as_ref().and_then(Value::as_u64) {
            if seq <= self.last_seq {
                return Verdict::Drop;
            }
            self.last_seq = seq;
            rewritten = true;
        } else if numbered.is_some() {
            rewritten = true;
        } else if replaying
            && value.get("method").is_some()
            && let Some(id) = value.get("id")
            && self.delivered_set.contains(&id.to_string())
        {
            // A request that was still waiting when the journal window moved on; the actor
            // already has it.
            return Verdict::Drop;
        }
        if value.get("method").is_none()
            && let Some(id) = value.get("id")
        {
            let key = format!("q:{id}");
            self.outstanding.retain(|(k, _)| *k != key);
        }
        if value.get("method").is_none()
            && let Some(id) = value.get("id")
            && let Some(asked) = self.pending.remove(&id.to_string())
        {
            match asked {
                Asked::Initialize => {
                    if value["result"]["_meta"]["acpdReattach"] == true {
                        self.numbered = true;
                        rewritten |= strip_meta(&mut value, "acpdReattach");
                    }
                }
                Asked::New(cwd) => {
                    if let Some(session) = value["result"]["sessionId"].as_str() {
                        self.session = Some(session.to_owned());
                        self.cwd = cwd;
                        if let Some(start) = value["result"]["_meta"]["acpdSeq"].as_u64() {
                            // Forwarding started after this position: the earlier events are
                            // ones the actor was never meant to get.
                            self.last_seq = self.last_seq.max(start);
                            rewritten |= strip_meta(&mut value, "acpdSeq");
                        }
                    }
                }
                Asked::Load(session) => self.session = Some(session),
            }
        }
        if value.get("method").is_some()
            && let Some(id) = value.get("id")
        {
            let key = id.to_string();
            if self.delivered_set.insert(key.clone()) {
                self.delivered.push_back(key);
                if self.delivered.len() > REMEMBERED_REQUESTS
                    && let Some(old) = self.delivered.pop_front()
                {
                    self.delivered_set.remove(&old);
                }
            }
        }
        Verdict::Deliver(rewritten.then(|| value.to_string()))
    }
}

/// Add `_meta.acpdReattach: true` to `initialize` parameters.
fn ask_to_number_frames(params: &mut Value) {
    if !params.is_object() {
        *params = json!({});
    }
    if !params["_meta"].is_object() {
        params["_meta"] = json!({});
    }
    params["_meta"]["acpdReattach"] = json!(true);
}

/// Remove one key from a response's `result._meta`, and `_meta` itself if that empties it.
/// True when something was removed.
fn strip_meta(value: &mut Value, key: &str) -> bool {
    let Some(meta) = value["result"]["_meta"].as_object_mut() else {
        return false;
    };
    let removed = meta.remove(key).is_some();
    if meta.is_empty()
        && let Some(result) = value["result"].as_object_mut()
    {
        result.remove("_meta");
    }
    removed
}

/// Why the connected loop ended.
enum Reason {
    /// The actor shut its end: a deliberate end, and the far host is told so.
    Deliberate,
    /// Something that must not be recovered from: a protocol violation, or the actor gone.
    Violation,
    /// The far host refused the credential (closed with 1008).
    Denied,
    /// The socket went away: closed, errored, silent too long or a write failed.
    Lost,
}

async fn pump(
    socket: Socket,
    from_actor: DuplexStream,
    mut to_actor: DuplexStream,
    target: Target,
) {
    let (mut sink, mut stream) = socket.split();
    let (line_tx, mut lines) = mpsc::channel::<Vec<u8>>(16);
    let reader = tokio::spawn(read_lines(from_actor, target.max_frame, line_tx));
    let mut tracker = Tracker::default();
    let mut held: VecDeque<String> = VecDeque::new();
    let mut heartbeat = tokio::time::interval(PING_EVERY);
    heartbeat.tick().await; // the first tick is immediate
    let mut end_sessions = false;
    loop {
        let reason = connected(
            &mut sink,
            &mut stream,
            &mut lines,
            &mut to_actor,
            &mut tracker,
            &mut heartbeat,
            target.max_frame,
        )
        .await;
        match reason {
            Reason::Deliberate => {
                end_sessions = true;
                break;
            }
            Reason::Violation | Reason::Denied => break,
            Reason::Lost => {
                if !target.resume.enabled() || !tracker.can_resume() {
                    break;
                }
                match resume(&target, &mut tracker, &mut lines, &mut held, &mut to_actor).await {
                    Resumed::Back(new_sink, new_stream) => {
                        sink = new_sink;
                        stream = new_stream;
                    }
                    Resumed::ActorGone => {
                        end_sessions = true;
                        break;
                    }
                    Resumed::Failed => break,
                }
            }
        }
    }
    reader.abort();
    let close = end_sessions.then(|| CloseFrame {
        code: CloseCode::Normal,
        reason: END_SESSIONS.into(),
    });
    let _ = tokio::time::timeout(Duration::from_secs(1), sink.send(Message::Close(close))).await;
    // Dropping `to_actor` closes the actor's reader: the link ending looks like an agent exiting.
}

/// Carry frames both ways until the socket is lost or the actor ends the link.
async fn connected(
    sink: &mut Sink,
    stream: &mut Stream,
    lines: &mut mpsc::Receiver<Vec<u8>>,
    to_actor: &mut DuplexStream,
    tracker: &mut Tracker,
    heartbeat: &mut tokio::time::Interval,
    max_frame: usize,
) -> Reason {
    let mut last_heard = Instant::now();
    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    last_heard = Instant::now();
                    // JSON from a conforming peer never holds a raw newline. One that does would
                    // split into several "frames" in the actor, so the link is dropped instead.
                    if text.contains(['\n', '\r']) || text.len() + 1 > max_frame {
                        return Reason::Violation;
                    }
                    match tracker.incoming(&text, false) {
                        Verdict::Drop => {}
                        Verdict::Deliver(rewritten) => {
                            if !deliver(to_actor, rewritten.as_deref().unwrap_or(&text)).await {
                                return Reason::Violation;
                            }
                        }
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => last_heard = Instant::now(),
                Some(Ok(Message::Binary(_) | Message::Frame(_))) => {} // ignored, as the RFD says
                Some(Ok(Message::Close(Some(frame)))) if frame.code == CloseCode::Policy => {
                    return Reason::Denied; // credentials revoked or rate limited: not coming back
                }
                _ => return Reason::Lost, // close, error or end
            },
            line = lines.recv() => {
                let Some(line) = line else {
                    // The connection was shut down on our side: this is a deliberate end.
                    return Reason::Deliberate;
                };
                let Ok(text) = String::from_utf8(line) else { return Reason::Violation };
                let text = tracker.outgoing(text.trim_end_matches(['\r', '\n']).to_owned());
                if !matches!(tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Text(text.into()))).await, Ok(Ok(()))) {
                    // Whether it got out is unknown: on reattach the far host's list of what it
                    // received decides if this line is sent again.
                    return Reason::Lost;
                }
            },
            _ = heartbeat.tick() => {
                if last_heard.elapsed() > SILENCE_LIMIT { return Reason::Lost }
                if !matches!(tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Ping(vec![].into()))).await, Ok(Ok(()))) {
                    return Reason::Lost;
                }
            }
        }
    }
}

/// Write one line to the actor. False when the actor is gone.
async fn deliver(to_actor: &mut DuplexStream, text: &str) -> bool {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(b'\n');
    matches!(
        tokio::time::timeout(SEND_TIMEOUT, to_actor.write_all(&bytes)).await,
        Ok(Ok(()))
    )
}

enum Resumed {
    Back(Sink, Stream),
    /// The actor shut its end while the link was down.
    ActorGone,
    Failed,
}

/// Get a lost link back, or say it cannot be. The actor's writes meanwhile are held (up to a
/// limit) and sent once after the session is reattached.
async fn resume(
    target: &Target,
    tracker: &mut Tracker,
    lines: &mut mpsc::Receiver<Vec<u8>>,
    held: &mut VecDeque<String>,
    to_actor: &mut DuplexStream,
) -> Resumed {
    let deadline = Instant::now() + target.resume.window;
    let mut wait = target.resume.first_wait;
    loop {
        let wake = (Instant::now() + wait).min(deadline);
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(wake) => break,
                line = lines.recv() => match line {
                    None => return Resumed::ActorGone,
                    Some(bytes) => {
                        let Ok(text) = String::from_utf8(bytes) else { return Resumed::Failed };
                        if held.len() >= MAX_HELD_LINES {
                            return Resumed::Failed;
                        }
                        held.push_back(text.trim_end_matches(['\r', '\n']).to_owned());
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return Resumed::Failed;
        }
        wait = (wait * 2).min(target.resume.max_wait);
        match attempt(target, tracker, held, to_actor).await {
            Attempt::Back(sink, stream) => return Resumed::Back(sink, stream),
            Attempt::Retry => {}
            Attempt::Fatal => return Resumed::Failed,
        }
    }
}

enum Attempt {
    Back(Sink, Stream),
    Retry,
    Fatal,
}

/// One try: dial, `initialize`, `session/load`, deliver what was missed, send what was held.
async fn attempt(
    target: &Target,
    tracker: &mut Tracker,
    held: &mut VecDeque<String>,
    to_actor: &mut DuplexStream,
) -> Attempt {
    let socket = match dial(&target.store, &target.peer, &target.url, target.max_frame).await {
        Ok(socket) => socket,
        Err(error) if error.permanent() => return Attempt::Fatal,
        Err(_) => return Attempt::Retry,
    };
    let (mut sink, mut stream) = socket.split();
    let (Some(session), Some(params)) = (tracker.session.clone(), tracker.init_params.clone())
    else {
        return Attempt::Fatal;
    };
    let mut params = params;
    ask_to_number_frames(&mut params);
    if !send(
        &mut sink,
        json!({"jsonrpc":"2.0","id":RESUME_INIT_ID,"method":"initialize","params":params}),
    )
    .await
    {
        return Attempt::Retry;
    }
    let Some(init) = answer_to(&mut stream, RESUME_INIT_ID, |_| {}).await else {
        return Attempt::Retry;
    };
    if init["result"]["_meta"]["acpdReattach"] != true {
        return Attempt::Fatal; // the far host no longer numbers its frames: it cannot be resumed
    }
    // What the actor had before the replay moves the counter on.
    let seen_before = tracker.last_seq;
    let cwd = tracker.cwd.clone().unwrap_or_else(|| "/".to_owned());
    if !send(
        &mut sink,
        json!({"jsonrpc":"2.0","id":RESUME_LOAD_ID,"method":"session/load",
               "params":{"sessionId":session,"cwd":cwd,"mcpServers":[]}}),
    )
    .await
    {
        return Attempt::Retry;
    }
    // The far host sends its journal first, then answers. Frames the actor already has are
    // dropped; the rest were missed while the link was down.
    let mut undeliverable = false;
    let mut replayed: Vec<String> = vec![];
    let load = answer_to(&mut stream, RESUME_LOAD_ID, |text| {
        match tracker.incoming(text, true) {
            Verdict::Drop => {}
            Verdict::Deliver(rewritten) => {
                replayed.push(rewritten.unwrap_or_else(|| text.to_owned()))
            }
        }
    })
    .await;
    for text in replayed {
        if !deliver(to_actor, &text).await {
            undeliverable = true;
            break;
        }
    }
    if undeliverable {
        return Attempt::Fatal;
    }
    let Some(load) = load else {
        return Attempt::Retry;
    };
    if load.get("error").is_some() {
        // "Session not found" also means the old socket has not been given up yet on the far
        // side (it is attached until it notices); try again until the window ends.
        return if load["error"]["code"] == -32602 {
            Attempt::Retry
        } else {
            Attempt::Fatal
        };
    }
    // Events the far journal evicted before the actor saw them cannot be replayed.
    let missed = load["result"]["_meta"]["acpdGap"]["missed"]
        .as_u64()
        .unwrap_or(0);
    if missed > seen_before {
        return Attempt::Fatal;
    }
    // What the actor wrote just before the socket died may or may not have got there. The far
    // host says which of those it received; only the others are sent again (once), and the
    // answers to the ones it did receive come back in the replay.
    let received: HashSet<&str> = load["result"]["_meta"]["acpdReceived"]
        .as_array()
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let never_arrived: Vec<String> = tracker
        .outstanding
        .iter()
        .filter(|(key, _)| !received.contains(key.as_str()))
        .map(|(_, line)| line.clone())
        .collect();
    for line in never_arrived {
        if !matches!(
            tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Text(line.into()))).await,
            Ok(Ok(()))
        ) {
            return Attempt::Retry; // still known to the tracker: decided again on the next try
        }
    }
    // What the actor wrote while the link was down goes out now, once each.
    while let Some(line) = held.pop_front() {
        let text = tracker.outgoing(line);
        if !matches!(
            tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Text(text.into()))).await,
            Ok(Ok(()))
        ) {
            // Lost again mid-flush: this line may or may not have got out, so it is dropped;
            // the rest stay held for the next attempt.
            return Attempt::Retry;
        }
    }
    Attempt::Back(sink, stream)
}

async fn send(sink: &mut Sink, value: Value) -> bool {
    matches!(
        tokio::time::timeout(
            SEND_TIMEOUT,
            sink.send(Message::Text(value.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    )
}

/// Read frames until the response to request `id`, handing every other text frame to `other`.
/// None on a timeout, a closed socket or a frame that cannot be right.
async fn answer_to(stream: &mut Stream, id: &str, mut other: impl FnMut(&str)) -> Option<Value> {
    let deadline = Instant::now() + RESUME_STEP;
    loop {
        match tokio::time::timeout_at(deadline, stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if text.contains(['\n', '\r']) {
                    return None;
                }
                match serde_json::from_str::<Value>(&text) {
                    Ok(value) if value["id"] == id && value.get("method").is_none() => {
                        return Some(value);
                    }
                    _ => other(&text),
                }
            }
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_)))) => {}
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delivered(verdict: Verdict, original: &str) -> Option<Value> {
        match verdict {
            Verdict::Drop => None,
            Verdict::Deliver(text) => {
                Some(serde_json::from_str(text.as_deref().unwrap_or(original)).unwrap())
            }
        }
    }
    fn incoming(tracker: &mut Tracker, frame: Value, replaying: bool) -> Option<Value> {
        let text = frame.to_string();
        delivered(tracker.incoming(&text, replaying), &text)
    }
    /// A tracker that has been through `initialize` and `session/new` with a numbering far host.
    fn started() -> Tracker {
        let mut tracker = Tracker::default();
        tracker.outgoing(
            json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1}})
                .to_string(),
        );
        incoming(
            &mut tracker,
            json!({"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"_meta":{"acpdReattach":true}}}),
            false,
        );
        tracker.outgoing(
            json!({"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/w","mcpServers":[]}})
                .to_string(),
        );
        incoming(
            &mut tracker,
            json!({"jsonrpc":"2.0","id":1,"result":{"sessionId":"s-1","_meta":{"acpdSeq":4}}}),
            false,
        );
        tracker
    }

    #[test]
    fn initialize_is_asked_to_number_frames_but_the_saved_copy_is_the_actors_own() {
        let mut tracker = Tracker::default();
        let sent: Value = serde_json::from_str(&tracker.outgoing(
            json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1,"_meta":{"x":1}}})
                .to_string(),
        ))
        .unwrap();
        assert_eq!(
            sent["params"]["_meta"],
            json!({"x": 1, "acpdReattach": true})
        );
        assert_eq!(
            tracker.init_params,
            Some(json!({"protocolVersion":1,"_meta":{"x":1}}))
        );
    }

    #[test]
    fn what_the_far_host_says_at_the_start_is_learned_and_kept_from_the_actor() {
        let tracker = started();
        assert!(tracker.numbered && tracker.can_resume());
        assert_eq!(tracker.session.as_deref(), Some("s-1"));
        assert_eq!(tracker.cwd.as_deref(), Some("/w"));
        assert_eq!(
            tracker.last_seq, 4,
            "events before the start are not the actor's to get"
        );
        // The actor sees pristine answers: no numbering marks.
        let mut tracker = Tracker::default();
        tracker.outgoing(
            json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}).to_string(),
        );
        let answer = incoming(
            &mut tracker,
            json!({"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"_meta":{"acpdReattach":true}}}),
            false,
        )
        .unwrap();
        assert!(answer["result"].get("_meta").is_none(), "{answer}");
    }

    #[test]
    fn a_far_host_that_never_said_it_numbers_frames_is_not_resumable() {
        let mut tracker = Tracker::default();
        tracker.outgoing(
            json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}).to_string(),
        );
        incoming(
            &mut tracker,
            json!({"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1}}),
            false,
        );
        tracker.outgoing(
            json!({"jsonrpc":"2.0","id":1,"method":"session/new","params":{"cwd":"/"}}).to_string(),
        );
        incoming(
            &mut tracker,
            json!({"jsonrpc":"2.0","id":1,"result":{"sessionId":"s"}}),
            false,
        );
        assert!(tracker.session.is_some() && !tracker.can_resume());
    }

    #[test]
    fn a_numbered_frame_is_delivered_once_without_its_number_and_older_ones_are_dropped() {
        let mut tracker = started();
        let update = |seq: u64| json!({"jsonrpc":"2.0","method":"session/update","params":{"n":seq},"acpdSeq":seq});
        let first = incoming(&mut tracker, update(5), false).unwrap();
        assert!(first.get("acpdSeq").is_none() && first["params"]["n"] == 5);
        // The same frame again (a replay), and an older one: not delivered.
        assert!(incoming(&mut tracker, update(5), true).is_none());
        assert!(incoming(&mut tracker, update(3), true).is_none());
        // One the actor has not seen: delivered, and the counter moves on.
        assert!(incoming(&mut tracker, update(9), true).is_some());
        assert_eq!(tracker.last_seq, 9);
    }

    #[test]
    fn a_request_without_a_number_is_not_delivered_twice_by_a_replay_but_is_when_new() {
        let mut tracker = started();
        let ask =
            json!({"jsonrpc":"2.0","id":77,"method":"session/request_permission","params":{}});
        assert!(incoming(&mut tracker, ask.clone(), false).is_some());
        // Re-queued by the far host after a reattach because it is still waiting: the actor has it.
        assert!(incoming(&mut tracker, ask.clone(), true).is_none());
        // A different request is new.
        let other =
            json!({"jsonrpc":"2.0","id":78,"method":"session/request_permission","params":{}});
        assert!(incoming(&mut tracker, other, true).is_some());
        // Outside a replay an unnumbered frame is never held back.
        assert!(incoming(&mut tracker, ask, false).is_some());
    }

    #[test]
    fn unanswered_lines_are_remembered_until_answered() {
        let mut tracker = started();
        tracker.outgoing(
            json!({"jsonrpc":"2.0","id":2,"method":"session/prompt","params":{}}).to_string(),
        );
        tracker
            .outgoing(json!({"jsonrpc":"2.0","method":"session/cancel","params":{}}).to_string());
        tracker.outgoing(
            json!({"jsonrpc":"2.0","id":77,"result":{"outcome":"cancelled"}}).to_string(),
        );
        let keys: Vec<&str> = tracker
            .outstanding
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        // The request and the answer to the agent's request; not the notification (it has no
        // id to ask about).
        assert_eq!(keys, vec!["q:2", "p:77"]);
        // The prompt's response arrives: no longer outstanding.
        incoming(
            &mut tracker,
            json!({"jsonrpc":"2.0","id":2,"result":{"stopReason":"end_turn"}}),
            false,
        );
        let keys: Vec<&str> = tracker
            .outstanding
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(keys, vec!["p:77"]);
        // The three requests that open a session are never in the list.
        assert!(started().outstanding.is_empty());
    }

    #[test]
    fn the_remembered_requests_are_bounded() {
        let mut tracker = started();
        for n in 0..(REMEMBERED_LINES + 10) {
            tracker.outgoing(
                json!({"jsonrpc":"2.0","id":n + 100,"method":"x","params":{}}).to_string(),
            );
        }
        assert_eq!(tracker.outstanding.len(), REMEMBERED_LINES);
        assert_eq!(tracker.outstanding.front().unwrap().0, "q:110");
    }

    #[test]
    fn the_resume_window_zero_means_off() {
        assert!(!Resume::within_seconds(0).enabled());
        assert!(!Resume::OFF.enabled());
        assert!(Resume::within_seconds(60).enabled());
    }
}
