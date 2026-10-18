//! Interactive shells the person opens from the desktop app or the phone (`/v1/terminals`).
//!
//! Off by default (`[terminal] enabled`). A terminal is a real shell running as the host user,
//! unsandboxed, in a folder the host already allows. Agents cannot see or use these. Output is
//! kept in a bounded ring with byte offsets so a client that reconnects resumes without a gap.
//! The shell and everything it starts end when the terminal is closed, idle with nobody attached,
//! or the host stops. See `docs/security.md`.
use crate::pty::{Pty, Spawn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use uuid::Uuid;

/// `[terminal]` in `config.toml`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct TerminalConfig {
    /// Allow phones to open shells on this computer. Only the host's config file can turn this on.
    pub enabled: bool,
    pub max_terminals: usize,
    /// A terminal nobody is attached to is closed after this many minutes.
    pub idle_minutes: u64,
    /// Output kept for reconnecting clients, in KiB.
    pub scrollback_kib: usize,
    /// The shell program. Absent: `$SHELL` (or `/bin/sh`) on Unix, the `[windows] shell` choice on Windows.
    pub shell: Option<PathBuf>,
    /// Start the shell with the host's whole environment, including any secrets in it.
    pub inherit_env: bool,
}
impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_terminals: 4,
            idle_minutes: 30,
            scrollback_kib: 1024,
            shell: None,
            inherit_env: false,
        }
    }
}
impl TerminalConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=16).contains(&self.max_terminals),
            "terminal max_terminals must be 1..16"
        );
        anyhow::ensure!(
            (1..=24 * 60).contains(&self.idle_minutes),
            "terminal idle_minutes must be 1..1440"
        );
        anyhow::ensure!(
            (64..=16 * 1024).contains(&self.scrollback_kib),
            "terminal scrollback_kib must be 64..16384"
        );
        Ok(())
    }
}

/// Largest single input or output piece.
pub const MAX_FRAME: usize = 64 * 1024;
const READ_CHUNK: usize = 16 * 1024;
/// Input pieces waiting for the shell; more than this and the sender is told to slow down.
const INPUT_QUEUE: usize = 64;
/// A finished terminal stays listed this long so a client can read why it ended.
const EXITED_KEEP: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Eq)]
pub enum TerminalError {
    Disabled,
    TooMany,
    NotFound,
    /// Another device holds the keyboard.
    NotController,
    TooLarge,
    /// Input is arriving faster than the shell takes it.
    Busy,
    Ended,
    Failed(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: Uuid,
    pub folder: String,
    pub shell: String,
    pub rows: u16,
    pub cols: u16,
    pub created_at: u64,
    pub attached: usize,
    pub controller: Option<Uuid>,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Output { offset: u64, data: Arc<Vec<u8>> },
    Exit(i32),
}

/// Output at a byte offset.
pub type Piece = (u64, Arc<Vec<u8>>);

/// A bounded history of output addressed by the number of bytes produced so far.
pub struct Ring {
    chunks: VecDeque<(u64, Arc<Vec<u8>>)>,
    bytes: usize,
    cap: usize,
    end: u64,
}
impl Ring {
    pub fn new(cap: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            bytes: 0,
            cap,
            end: 0,
        }
    }
    /// Adds output and returns its offset.
    pub fn push(&mut self, data: Arc<Vec<u8>>) -> u64 {
        let offset = self.end;
        self.end += data.len() as u64;
        self.bytes += data.len();
        self.chunks.push_back((offset, data));
        while self.bytes > self.cap && self.chunks.len() > 1 {
            let (_, old) = self.chunks.pop_front().expect("non-empty");
            self.bytes -= old.len();
        }
        offset
    }
    pub fn start(&self) -> u64 {
        self.chunks.front().map_or(self.end, |(o, _)| *o)
    }
    pub fn end(&self) -> u64 {
        self.end
    }
    /// Output from offset `after` on. `gap` is true when part of what the client missed is gone.
    pub fn since(&self, after: u64) -> (Vec<Piece>, bool) {
        let after = after.min(self.end);
        let gap = after < self.start();
        let from = after.max(self.start());
        let mut out = Vec::new();
        for (offset, data) in &self.chunks {
            let finish = offset + data.len() as u64;
            if finish <= from {
                continue;
            }
            if *offset >= from {
                out.push((*offset, data.clone()));
            } else {
                let cut = (from - offset) as usize;
                out.push((from, Arc::new(data[cut..].to_vec())));
            }
        }
        (out, gap)
    }
}

struct Live {
    pty: Pty,
    rows: u16,
    cols: u16,
    controller: Option<Uuid>,
    attached: usize,
    last_active: Instant,
    exit_code: Option<i32>,
    exited_at: Option<Instant>,
    ring: Ring,
}
struct Term {
    id: Uuid,
    folder: PathBuf,
    shell: String,
    created_at: u64,
    live: Mutex<Live>,
    events: broadcast::Sender<Event>,
    input: SyncSender<Vec<u8>>,
}

#[derive(Default)]
pub struct Terminals {
    terms: Mutex<HashMap<Uuid, Arc<Term>>>,
    reaper: std::sync::atomic::AtomicBool,
}

/// What a client gets when it attaches: the history it missed and a receiver for the rest.
pub struct Subscription {
    pub replay: Vec<Piece>,
    pub gap: bool,
    pub exit_code: Option<i32>,
    pub rx: broadcast::Receiver<Event>,
    guard: Attachment,
}
impl Subscription {
    /// Keep this while a client is connected; the terminal is not idle meanwhile.
    pub fn attachment(&self) -> &Attachment {
        &self.guard
    }
}
pub struct Attachment(Arc<Term>);
impl Drop for Attachment {
    fn drop(&mut self) {
        let mut live = self.0.live.lock().unwrap();
        live.attached = live.attached.saturating_sub(1);
        live.last_active = Instant::now();
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Terminals {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open(
        self: &Arc<Self>,
        config: &TerminalConfig,
        folder: &Path,
        device: Uuid,
        rows: u16,
        cols: u16,
    ) -> Result<TerminalInfo, TerminalError> {
        if !config.enabled {
            return Err(TerminalError::Disabled);
        }
        let (rows, cols) = (rows.clamp(2, 500), cols.clamp(2, 1000));
        // cmd.exe cannot start in a verbatim (\?) path, which canonicalizing produces on Windows.
        let folder = &crate::userdata::plain_path(folder);
        let shell = choose_shell(config).map_err(|e| TerminalError::Failed(e.to_string()))?;
        let env = shell_env(std::env::vars(), config.inherit_env);
        {
            let terms = self.terms.lock().unwrap();
            let running = terms
                .values()
                .filter(|t| t.live.lock().unwrap().exit_code.is_none())
                .count();
            if running >= config.max_terminals {
                return Err(TerminalError::TooMany);
            }
        }
        let mut pty = Pty::spawn(&Spawn {
            program: &shell.program,
            args: &shell.args,
            cwd: folder,
            env: &env,
            rows,
            cols,
        })
        .map_err(|e| TerminalError::Failed(format!("{e:#}")))?;
        let mut reader = pty.take_reader().expect("fresh pty has a reader");
        let mut writer = pty
            .writer()
            .map_err(|e| TerminalError::Failed(e.to_string()))?;
        let (input, queued) = sync_channel::<Vec<u8>>(INPUT_QUEUE);
        let (events, _) = broadcast::channel(256);
        let term = Arc::new(Term {
            id: Uuid::new_v4(),
            folder: folder.to_path_buf(),
            shell: shell.name,
            created_at: now(),
            live: Mutex::new(Live {
                pty,
                rows,
                cols,
                controller: Some(device),
                attached: 0,
                last_active: Instant::now(),
                exit_code: None,
                exited_at: None,
                ring: Ring::new(config.scrollback_kib * 1024),
            }),
            events,
            input,
        });
        std::thread::Builder::new()
            .name("terminal-input".into())
            .spawn(move || {
                while let Ok(bytes) = queued.recv() {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| TerminalError::Failed(e.to_string()))?;
        let output = term.clone();
        std::thread::Builder::new()
            .name("terminal-output".into())
            .spawn(move || {
                let mut buffer = vec![0u8; READ_CHUNK];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let data = Arc::new(buffer[..n].to_vec());
                            let mut live = output.live.lock().unwrap();
                            let offset = live.ring.push(data.clone());
                            // Sent under the lock so a new subscriber sees every byte once.
                            let _ = output.events.send(Event::Output { offset, data });
                        }
                    }
                }
                finish(&output);
            })
            .map_err(|e| TerminalError::Failed(e.to_string()))?;
        // On Windows the output pipe stays open after the shell exits; notice the exit and close the
        // pseudo-console (after output had time to drain) so the reader sees the end.
        let watched = term.clone();
        std::thread::Builder::new()
            .name("terminal-watch".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(200));
                    let mut live = watched.live.lock().unwrap();
                    if live.exit_code.is_some() {
                        break;
                    }
                    if live.pty.exit_code().is_some() {
                        drop(live);
                        std::thread::sleep(Duration::from_millis(300));
                        watched.live.lock().unwrap().pty.kill();
                        break;
                    }
                }
            })
            .map_err(|e| TerminalError::Failed(e.to_string()))?;
        let info = info_of(&term);
        tracing::info!(terminal = %term.id, device = %device, shell = %term.shell, "terminal opened");
        self.terms.lock().unwrap().insert(term.id, term);
        let weak = Arc::downgrade(self);
        let idle = Duration::from_secs(config.idle_minutes * 60);
        if tokio::runtime::Handle::try_current().is_ok()
            && !self.reaper.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    let Some(this) = weak.upgrade() else { break };
                    this.reap(idle);
                }
            });
        }
        Ok(info)
    }

    /// Closes terminals nobody has attached to for `idle`, and drops finished ones after a minute.
    pub fn reap(&self, idle: Duration) -> usize {
        let mut gone = Vec::new();
        {
            let terms = self.terms.lock().unwrap();
            for (id, term) in terms.iter() {
                let live = term.live.lock().unwrap();
                let expired = match live.exited_at {
                    Some(at) => at.elapsed() >= EXITED_KEEP && live.attached == 0,
                    None => live.attached == 0 && live.last_active.elapsed() >= idle,
                };
                if expired {
                    gone.push(*id);
                }
            }
        }
        for id in &gone {
            tracing::info!(terminal = %id, "terminal closed (idle or finished)");
            self.close(*id);
        }
        gone.len()
    }

    pub fn list(&self) -> Vec<TerminalInfo> {
        let mut all: Vec<_> = self.terms.lock().unwrap().values().map(info_of).collect();
        all.sort_by_key(|t| (t.created_at, t.id));
        all
    }
    pub fn info(&self, id: Uuid) -> Result<TerminalInfo, TerminalError> {
        self.get(id).map(|t| info_of(&t))
    }
    fn get(&self, id: Uuid) -> Result<Arc<Term>, TerminalError> {
        self.terms
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or(TerminalError::NotFound)
    }

    /// Ends the shell and everything it started, and forgets the terminal.
    pub fn close(&self, id: Uuid) -> bool {
        let Some(term) = self.terms.lock().unwrap().remove(&id) else {
            return false;
        };
        kill(&term);
        true
    }
    pub fn close_all(&self) {
        let all: Vec<_> = self.terms.lock().unwrap().drain().map(|(_, t)| t).collect();
        for term in all {
            kill(&term);
        }
    }

    /// Attach, getting output after byte offset `after` (0 = everything still kept).
    pub fn subscribe(&self, id: Uuid, after: u64) -> Result<Subscription, TerminalError> {
        let term = self.get(id)?;
        let mut live = term.live.lock().unwrap();
        let rx = term.events.subscribe();
        let (replay, gap) = live.ring.since(after);
        live.attached += 1;
        let exit_code = live.exit_code;
        drop(live);
        Ok(Subscription {
            replay,
            gap,
            exit_code,
            rx,
            guard: Attachment(term),
        })
    }

    /// Keyboard input from the device that holds the keyboard.
    pub fn input(&self, id: Uuid, device: Uuid, bytes: Vec<u8>) -> Result<(), TerminalError> {
        if bytes.len() > MAX_FRAME {
            return Err(TerminalError::TooLarge);
        }
        let term = self.get(id)?;
        {
            let mut live = term.live.lock().unwrap();
            if live.exit_code.is_some() {
                return Err(TerminalError::Ended);
            }
            if live.controller != Some(device) {
                return Err(TerminalError::NotController);
            }
            live.last_active = Instant::now();
        }
        match term.input.try_send(bytes) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(TerminalError::Busy),
            Err(TrySendError::Disconnected(_)) => Err(TerminalError::Ended),
        }
    }

    pub fn resize(
        &self,
        id: Uuid,
        device: Uuid,
        rows: u16,
        cols: u16,
    ) -> Result<(), TerminalError> {
        let term = self.get(id)?;
        let mut live = term.live.lock().unwrap();
        if live.exit_code.is_some() {
            return Err(TerminalError::Ended);
        }
        if live.controller != Some(device) {
            return Err(TerminalError::NotController);
        }
        let (rows, cols) = (rows.clamp(2, 500), cols.clamp(2, 1000));
        live.pty
            .resize(rows, cols)
            .map_err(|e| TerminalError::Failed(e.to_string()))?;
        live.rows = rows;
        live.cols = cols;
        Ok(())
    }

    /// Hands the keyboard to `device` (the person asked for it on that device).
    pub fn take_control(&self, id: Uuid, device: Uuid) -> Result<(), TerminalError> {
        let term = self.get(id)?;
        let mut live = term.live.lock().unwrap();
        live.controller = Some(device);
        live.last_active = Instant::now();
        Ok(())
    }
}

fn kill(term: &Term) {
    let mut live = term.live.lock().unwrap();
    live.pty.kill();
}

/// The output ended: the shell has gone (or was killed). Record its exit code and tell watchers.
fn finish(term: &Arc<Term>) {
    let mut code = None;
    for _ in 0..40 {
        code = term.live.lock().unwrap().pty.exit_code();
        if code.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut live = term.live.lock().unwrap();
    let code = code.unwrap_or(-1);
    live.exit_code = Some(code);
    live.exited_at = Some(Instant::now());
    let _ = term.events.send(Event::Exit(code));
}

fn info_of(term: &Arc<Term>) -> TerminalInfo {
    let live = term.live.lock().unwrap();
    TerminalInfo {
        id: term.id,
        folder: term.folder.to_string_lossy().into_owned(),
        shell: term.shell.clone(),
        rows: live.rows,
        cols: live.cols,
        created_at: term.created_at,
        attached: live.attached,
        controller: live.controller,
        exit_code: live.exit_code,
    }
}

struct Chosen {
    program: PathBuf,
    args: Vec<String>,
    name: String,
}

fn choose_shell(config: &TerminalConfig) -> anyhow::Result<Chosen> {
    #[cfg(unix)]
    {
        let program = config
            .shell
            .clone()
            .or_else(|| {
                std::env::var_os("SHELL")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute() && p.is_file())
            })
            .unwrap_or_else(|| PathBuf::from("/bin/sh"));
        anyhow::ensure!(program.is_file(), "shell {} not found", program.display());
        let name = file_stem(&program);
        Ok(Chosen {
            program,
            args: Vec::new(),
            name,
        })
    }
    #[cfg(windows)]
    {
        let program = match &config.shell {
            Some(p) => p.clone(),
            None => crate::shell::resolve(None, false)?.executable,
        };
        anyhow::ensure!(program.is_file(), "shell {} not found", program.display());
        let name = file_stem(&program);
        let args = match name.to_ascii_lowercase().as_str() {
            "pwsh" | "powershell" => vec!["-NoLogo".to_string()],
            "bash" => vec!["--login".to_string(), "-i".to_string()],
            _ => Vec::new(),
        };
        Ok(Chosen {
            program,
            args,
            name,
        })
    }
}
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "shell".into())
}

/// The environment the shell starts with: a short list of ordinary variables, never the host's
/// secrets, plus `TERM`. `inherit` passes everything through instead.
pub fn shell_env(
    vars: impl Iterator<Item = (String, String)>,
    inherit: bool,
) -> Vec<(String, String)> {
    const KEEP: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LANGUAGE",
        "SHELL",
        "TMPDIR",
        "TZ",
        "PWD",
        // Windows
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "USERPROFILE",
        "USERNAME",
        "HOMEDRIVE",
        "HOMEPATH",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
        "PROGRAMW6432",
        "COMMONPROGRAMFILES",
        "SYSTEMDRIVE",
        "TEMP",
        "TMP",
        "PATHEXT",
        "OS",
        "PROCESSOR_ARCHITECTURE",
        "NUMBER_OF_PROCESSORS",
        "COMPUTERNAME",
        "USERDOMAIN",
    ];
    let mut out: Vec<(String, String)> = vars
        .filter(|(k, _)| {
            inherit || {
                let up = k.to_ascii_uppercase();
                KEEP.contains(&up.as_str()) || up.starts_with("LC_")
            }
        })
        .filter(|(k, _)| !k.eq_ignore_ascii_case("TERM") && !k.eq_ignore_ascii_case("COLORTERM"))
        .collect();
    out.push(("TERM".into(), "xterm-256color".into()));
    out.push(("COLORTERM".into(), "truecolor".into()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(s: &str) -> Arc<Vec<u8>> {
        Arc::new(s.as_bytes().to_vec())
    }

    #[test]
    fn ring_replays_from_an_offset_and_reports_what_was_lost() {
        let mut ring = Ring::new(10);
        ring.push(bytes("abcde"));
        ring.push(bytes("fghij"));
        let (all, gap) = ring.since(0);
        assert!(!gap);
        assert_eq!(all.len(), 2);
        let (rest, gap) = ring.since(7);
        assert!(!gap);
        assert_eq!(rest.len(), 1);
        assert_eq!((rest[0].0, rest[0].1.as_slice()), (7, "hij".as_bytes()));
        ring.push(bytes("klmno"));
        assert_eq!(ring.start(), 5);
        let (kept, gap) = ring.since(0);
        assert!(gap, "the first five bytes are gone");
        assert_eq!(kept[0].0, 5);
        let (none, gap) = ring.since(99);
        assert!(
            none.is_empty() && !gap,
            "a cursor past the end gets nothing"
        );
    }

    #[test]
    fn the_shell_starts_with_a_short_environment() {
        let host = [
            ("PATH", "/bin"),
            ("HOME", "/home/a"),
            ("LC_ALL", "C"),
            ("OPENAI_API_KEY", "secret"),
            ("GITHUB_TOKEN", "secret"),
            ("TERM", "dumb"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let env = shell_env(host.clone().into_iter(), false);
        let names: Vec<_> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains(&"PATH") && names.contains(&"HOME") && names.contains(&"LC_ALL"));
        assert!(!names.contains(&"OPENAI_API_KEY") && !names.contains(&"GITHUB_TOKEN"));
        assert_eq!(env.iter().filter(|(k, _)| k == "TERM").count(), 1);
        assert!(env.contains(&("TERM".into(), "xterm-256color".into())));
        let all = shell_env(host.into_iter(), true);
        assert!(all.iter().any(|(k, _)| k == "OPENAI_API_KEY"));
    }

    #[test]
    fn limits_are_checked() {
        assert!(TerminalConfig::default().validate().is_ok());
        assert!(!TerminalConfig::default().enabled);
        for bad in [
            TerminalConfig {
                max_terminals: 0,
                ..Default::default()
            },
            TerminalConfig {
                idle_minutes: 0,
                ..Default::default()
            },
            TerminalConfig {
                scrollback_kib: 1,
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }
}
