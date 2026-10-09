//! Operator-only local lifecycle control. Never kills a process by a saved PID.
use crate::{
    api::Host,
    config::Config,
    registry::Registry,
    security::{protect, reject_symlink, write_private_json},
};
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Semaphore, watch},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    address: SocketAddr,
    token: String,
    pid: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    token: String,
    action: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub status: String,
    pub pid: u32,
}

pub fn record_path(config: &Path, profile: &Path) -> Result<PathBuf> {
    let config = std::path::absolute(config)?;
    let digest = Sha256::digest(config.as_os_str().as_encoded_bytes());
    let key: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok(profile
        .parent()
        .unwrap_or(Path::new("."))
        .join("daemons")
        .join(format!("{key}.json")))
}
fn read_record(path: &Path) -> Result<Record> {
    reject_symlink(path)?;
    let mut bytes = Vec::new();
    File::open(path)?.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        bail!("invalid control record")
    }
    let record: Record =
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid control record"))?;
    if !record.address.ip().is_loopback()
        || record.token.len() != 64
        || !record.token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!("invalid control record")
    }
    Ok(record)
}
pub async fn request(path: &Path, action: &str) -> Result<Reply> {
    let record = read_record(path)?;
    tokio::time::timeout(Duration::from_secs(4), async {
        let mut stream = TcpStream::connect(record.address)
            .await
            .context("local control unavailable; the record may be stale")?;
        let bytes = serde_json::to_vec(&Request {
            token: record.token,
            action: action.into(),
        })?;
        stream.write_all(&bytes).await?;
        stream.shutdown().await?;
        let mut reply = Vec::new();
        (&mut stream).take(2049).read_to_end(&mut reply).await?;
        if reply.len() > 2048 {
            bail!("invalid control reply")
        }
        let reply: Reply =
            serde_json::from_slice(&reply).map_err(|_| anyhow::anyhow!("invalid control reply"))?;
        Ok(reply)
    })
    .await
    .context("local control timed out; do not automatically retry a mutation")?
}
pub async fn stop(path: &Path) -> Result<()> {
    let reply = request(path, "stop").await?;
    if reply.status != "stopping" {
        bail!("stop was not accepted")
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    while path.exists() {
        if tokio::time::Instant::now() >= deadline {
            bail!("stop requested; shutdown still pending, check daemon status")
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}
pub async fn start(config: &Path, registry: Option<&Path>, profile: &Path) -> Result<u32> {
    let record = record_path(config, profile)?;
    if record.exists() && request(&record, "status").await.is_ok() {
        bail!("host is already managed; use daemon status or stop")
    }
    let mut args = vec![
        "--config".into(),
        std::path::absolute(config)?.into_os_string(),
        "--profile".into(),
        std::path::absolute(profile)?.into_os_string(),
    ];
    if let Some(registry) = registry {
        args.extend([
            "--registry".into(),
            std::path::absolute(registry)?.into_os_string(),
        ]);
    }
    args.push("daemon-worker".into());
    let mut child = crate::detached::Child::spawn(&std::env::current_exe()?, &args)?;
    let pid = child.id();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if child.exited()? {
            bail!("background host exited; run doctor or foreground start for diagnostics")
        }
        if let Ok(reply) = request(&record, "status").await
            && reply.pid == pid
            && reply.status == "running"
        {
            return Ok(pid);
        }
        if tokio::time::Instant::now() >= deadline {
            // This is our still-owned Child handle, never a PID from a stale record.
            let _ = child.kill();
            let _ = child.wait();
            bail!("background startup timed out; run foreground start for diagnostics")
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
struct Guard {
    record: PathBuf,
    token: String,
    _lock: File,
}
impl Drop for Guard {
    fn drop(&mut self) {
        if read_record(&self.record).is_ok_and(|r| r.token == self.token) {
            let _ = std::fs::remove_file(&self.record);
        }
    }
}
pub async fn serve(
    host: Host,
    config: PathBuf,
    registry: Option<PathBuf>,
    record: PathBuf,
) -> Result<()> {
    let directory = record.parent().context("control directory missing")?;
    reject_symlink(directory)?;
    std::fs::create_dir_all(directory)?;
    protect(directory, true)?;
    let lock_path = record.with_extension("lock");
    reject_symlink(&lock_path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    protect(&lock_path, false)?;
    lock.try_lock_exclusive()
        .context("another managed host owns this config")?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|_| anyhow::anyhow!("secure randomness unavailable"))?;
    let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
    write_private_json(
        &record,
        &Record {
            address: listener.local_addr()?,
            token: token.clone(),
            pid: std::process::id(),
        },
    )?;
    let _guard = Guard {
        record,
        token: token.clone(),
        _lock: lock,
    };
    let ready = Arc::new(AtomicBool::new(false));
    let (stop, mut stopped) = watch::channel(false);
    let stop_rx = stopped.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let flag = ready.clone();
    let readiness = tokio::spawn(async move {
        if ready_rx.await.is_ok() {
            flag.store(true, Ordering::Release);
        }
    });
    let sessions = host.sessions.clone();
    let control = tokio::spawn(async move {
        let slots = Arc::new(Semaphore::new(16));
        loop {
            let accepted = tokio::select! {_ = stopped.changed() => break, accepted = listener.accept() => accepted};
            let Ok((mut stream, _)) = accepted else { break };
            let Ok(slot) = slots.clone().try_acquire_owned() else {
                continue;
            };
            let token = token.clone();
            let ready = ready.clone();
            let stop = stop.clone();
            let sessions = sessions.clone();
            let config = config.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                let _slot = slot;
                let work = async {
                    let mut bytes = Vec::new();
                    (&mut stream).take(4097).read_to_end(&mut bytes).await?;
                    if bytes.len() > 4096 {
                        bail!("control request too large")
                    }
                    let request: Request = serde_json::from_slice(&bytes)?;
                    if !bool::from(request.token.as_bytes().ct_eq(token.as_bytes())) {
                        bail!("unauthorized")
                    }
                    let status = match request.action.as_str() {
                        "status" => {
                            if *stop.borrow() {
                                "stopping"
                            } else if ready.load(Ordering::Acquire) {
                                "running"
                            } else {
                                "starting"
                            }
                        }
                        "stop" => {
                            let _ = stop.send(true);
                            "stopping"
                        }
                        "reload" => {
                            let candidate = tokio::task::spawn_blocking(move || -> Result<_> {
                                let config = Config::load(&config)?;
                                let registry = Registry::load(
                                    registry.as_deref().unwrap_or(&config.registry),
                                )?;
                                Ok((config, registry))
                            })
                            .await;
                            match candidate {
                                Ok(Ok((config, registry))) => {
                                    if sessions.reload(config, registry).is_ok() {
                                        "reloaded"
                                    } else {
                                        "restart_required"
                                    }
                                }
                                _ => "invalid_configuration",
                            }
                        }
                        _ => "unknown_action",
                    };
                    stream
                        .write_all(&serde_json::to_vec(&Reply {
                            status: status.into(),
                            pid: std::process::id(),
                        })?)
                        .await?;
                    Ok::<_, anyhow::Error>(())
                };
                // Malformed/unauthorized input is discarded without logging its payload.
                let _ = tokio::time::timeout(Duration::from_secs(3), work).await;
            });
        }
    });
    let result = host.serve_controlled(Some(stop_rx), Some(ready_tx)).await;
    control.abort();
    readiness.abort();
    result
}
