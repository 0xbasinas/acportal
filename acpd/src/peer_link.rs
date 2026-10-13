//! Client side of the peer link: a WebSocket to another host's `/acp` bridge, presented to
//! `AcpConnection` as the same newline-framed JSON it gets from an agent's stdio.
//!
//! The actor never learns it is talking to a network. Two in-memory pipes carry the lines and
//! one pump task turns each line into a WebSocket text frame and back. The credential comes from
//! the peer store and is sent only to that peer's own origin. TLS uses the bundled web roots and
//! is never switched off; plain `ws://` exists for loopback only (tests and a local Goose).
use crate::peers::PeerStore;
use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream},
    sync::mpsc,
    time::Instant,
};
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
    protocol::WebSocketConfig,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const SEND_TIMEOUT: Duration = Duration::from_secs(5);
const PING_EVERY: Duration = Duration::from_secs(20);
/// Three missed pings: the link is gone even if TCP has not noticed.
const SILENCE_LIMIT: Duration = Duration::from_secs(70);
/// The close reason that tells an acpd bridge to end the sessions at once instead of keeping
/// them for a reattach.
pub const END_SESSIONS: &str = "end-sessions";

/// The two ends handed to `AcpConnection::over_transport`.
pub struct Link {
    pub reader: DuplexStream,
    pub writer: DuplexStream,
}

/// Connect to `url` as `peer`. The peer's credential is looked up in the store and refused
/// unless `url` points at the origin the peer record names.
pub async fn connect(store: &PeerStore, peer: &str, url: &str, max_frame: usize) -> Result<Link> {
    let (store, name, target) = (store.clone(), peer.to_owned(), url.to_owned());
    let (_, (header, value)) =
        tokio::task::spawn_blocking(move || store.credential_for(&name, &target))
            .await
            .context("peer lookup stopped")??;
    let mut request = url.into_client_request().context("invalid peer URL")?;
    let mut credential = HeaderValue::from_str(&value).context("invalid credential")?;
    credential.set_sensitive(true);
    request.headers_mut().insert(
        HeaderName::from_bytes(header.as_bytes()).context("invalid credential header")?,
        credential,
    );
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = WebSocketConfig::default()
        .max_message_size(Some(max_frame))
        .max_frame_size(Some(max_frame));
    let socket = match tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(request, Some(config), false),
    )
    .await
    {
        Err(_) => bail!("the peer did not answer in time"),
        // Say what happened, never what the server said: its text is not ours to repeat.
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response))) => {
            bail!(
                "the peer refused the connection (HTTP {})",
                response.status()
            )
        }
        Ok(Err(_)) => bail!("could not connect to the peer"),
        Ok(Ok((socket, _))) => socket,
    };
    let capacity = (max_frame * 2).max(64 * 1024);
    let (to_peer, from_actor) = tokio::io::duplex(capacity);
    let (to_actor, from_peer) = tokio::io::duplex(capacity);
    tokio::spawn(pump(socket, from_actor, to_actor, max_frame));
    Ok(Link {
        reader: from_peer,
        writer: to_peer,
    })
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

async fn pump(
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    from_actor: DuplexStream,
    mut to_actor: DuplexStream,
    max_frame: usize,
) {
    let (mut sink, mut stream) = socket.split();
    let (line_tx, mut lines) = mpsc::channel::<Vec<u8>>(16);
    let reader = tokio::spawn(read_lines(from_actor, max_frame, line_tx));
    let mut heartbeat = tokio::time::interval(PING_EVERY);
    heartbeat.tick().await; // the first tick is immediate
    let mut last_heard = Instant::now();
    let mut end_sessions = false;
    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    last_heard = Instant::now();
                    // JSON from a conforming peer never holds a raw newline. One that does would
                    // split into several "frames" in the actor, so the link is dropped instead.
                    if text.contains(['\n', '\r']) || text.len() + 1 > max_frame {
                        break;
                    }
                    let mut bytes = text.as_bytes().to_vec();
                    bytes.push(b'\n');
                    let delivered = tokio::time::timeout(SEND_TIMEOUT, to_actor.write_all(&bytes)).await;
                    if !matches!(delivered, Ok(Ok(()))) {
                        break;
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => last_heard = Instant::now(),
                Some(Ok(Message::Binary(_) | Message::Frame(_))) => {} // ignored, as the RFD says
                _ => break, // close, error or end
            },
            line = lines.recv() => {
                let Some(line) = line else {
                    // The connection was shut down on our side: this is a deliberate end.
                    end_sessions = true;
                    break;
                };
                let Ok(text) = String::from_utf8(line) else { break };
                let text = text.trim_end_matches(['\r', '\n']).to_owned();
                if !matches!(tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Text(text.into()))).await, Ok(Ok(()))) {
                    break;
                }
            },
            _ = heartbeat.tick() => {
                if last_heard.elapsed() > SILENCE_LIMIT { break }
                if !matches!(tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Ping(vec![].into()))).await, Ok(Ok(()))) {
                    break;
                }
            }
        }
    }
    reader.abort();
    let close = end_sessions.then(|| tokio_tungstenite::tungstenite::protocol::CloseFrame {
        code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
        reason: END_SESSIONS.into(),
    });
    let _ = tokio::time::timeout(Duration::from_secs(1), sink.send(Message::Close(close))).await;
    // Dropping `to_actor` closes the actor's reader: the link ending looks like an agent exiting.
}
