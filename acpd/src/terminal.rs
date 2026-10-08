//! Workspace-scoped terminal runtime. Call create_approved only after explicit consent.
use crate::process_tree::ProcessTree;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command, sync::watch, time::Instant};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRequest {
    session_id: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: Vec<EnvVariable>,
    cwd: Option<PathBuf>,
    output_byte_limit: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_characters_from_different_streams_are_not_corrupted() {
        let mut output = Output {
            bytes: VecDeque::new(),
            limit: 100,
            truncated: false,
        };
        let mut out = Decoder::default();
        let mut err = Decoder::default();
        out.feed(&"🙂".as_bytes()[..2], &mut output);
        err.feed("stderr".as_bytes(), &mut output);
        out.feed(&"🙂".as_bytes()[2..], &mut output);
        assert_eq!(output.snapshot(), ("stderr🙂".into(), false));
        out.feed(&[255, 0xe2], &mut output);
        out.finish(&mut output);
        assert_eq!(output.snapshot().0, "stderr🙂��");
    }
    #[test]
    fn shell_lines_are_detected_only_without_separate_arguments() {
        let none: Vec<String> = vec![];
        for line in [
            "cd /w && python3 -m unittest -v 2>&1",
            "ls | head",
            "echo hi; echo there",
            "cat *.py",
            "python3 -m unittest -v",
            "echo $HOME",
            "a > b",
            "run `x`",
        ] {
            assert!(is_shell_line(line, &none), "{line}");
        }
        for plain in ["pwd", "python3", "/usr/bin/ls", "./tool"] {
            assert!(!is_shell_line(plain, &none), "{plain}");
        }
        // Explicit argv is never reinterpreted, whatever its text contains.
        assert!(!is_shell_line("python3", &["-c".into(), "a && b".into()]));
        assert!(!is_shell_line("bad name", &["x".into()]));
    }
    #[test]
    fn retained_output_starts_at_a_character_boundary() {
        let mut output = Output {
            bytes: VecDeque::new(),
            limit: 5,
            truncated: false,
        };
        output.append("🙂🙂x".as_bytes());
        assert_eq!(output.snapshot(), ("🙂x".into(), true));
        output.limit = 2;
        output.append("🙂".as_bytes());
        assert_eq!(output.snapshot(), ("".into(), true));
    }
}
#[derive(Deserialize)]
struct EnvVariable {
    name: String,
    value: String,
}
pub struct PreparedTerminal {
    request: CreateRequest,
    cwd: PathBuf,
    limit: usize,
    executable: PathBuf,
    /// True when `request.command` is a shell line that runs through [`SHELL_LABEL`].
    shell: bool,
}
/// The fixed shell used for approved shell lines, as shown in consent.
#[cfg(unix)]
pub const SHELL_LABEL: &str = "/bin/sh -c";
#[cfg(windows)]
pub const SHELL_LABEL: &str = "cmd.exe /D /S /C";
#[cfg(unix)]
fn shell_executable() -> Result<PathBuf> {
    let shell = PathBuf::from("/bin/sh");
    anyhow::ensure!(shell.is_file(), "/bin/sh unavailable");
    Ok(shell)
}
#[cfg(windows)]
fn shell_executable() -> Result<PathBuf> {
    // Use the system cmd.exe, never one found through PATH or the agent's environment.
    let root = std::env::var_os("SystemRoot").context("SystemRoot unavailable")?;
    let shell = PathBuf::from(root).join("System32").join("cmd.exe");
    anyhow::ensure!(shell.is_file(), "cmd.exe unavailable");
    Ok(shell)
}
/// A request is a shell line when it has no separate arguments and its command text
/// contains whitespace or shell syntax, so it cannot be a literal program path.
/// Plain program + args requests keep the literal argv path.
pub fn is_shell_line(command: &str, args: &[String]) -> bool {
    const SYNTAX: &[char] = &[
        '&', '|', ';', '<', '>', '(', ')', '$', '`', '"', '\'', '*', '?', '[', ']', '{', '}', '~',
        '!', '#', '%', '^',
    ];
    // Backslash is a path separator on Windows, but escape syntax on Unix shells.
    args.is_empty()
        && (command.chars().any(char::is_whitespace)
            || command.contains(SYNTAX)
            || (cfg!(unix) && command.contains('\\')))
}
impl PreparedTerminal {
    /// The exact shell line, when this request runs through the host shell.
    pub fn shell_line(&self) -> Option<&str> {
        self.shell.then_some(self.request.command.as_str())
    }
    /// Canonical working directory inside the workspace.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }
    /// Agent-supplied values can change executable resolution and loader behavior.
    pub fn has_environment_overrides(&self) -> bool {
        !self.request.env.is_empty()
    }
    pub fn is_shell(&self) -> bool {
        self.shell
    }
    pub fn operation(&self) -> Value {
        // Values can contain credentials; show names in consent and never log the request.
        let names = self
            .request
            .env
            .iter()
            .map(|entry| &entry.name)
            .collect::<Vec<_>>();
        if self.shell {
            // The complete line, verbatim; this exact text is what runs after approval.
            json!({"shellLine":self.request.command,"shell":SHELL_LABEL,"executable":self.executable,"cwd":self.cwd,"environmentNames":names})
        } else {
            json!({"command":self.request.command,"executable":self.executable,"args":self.request.args,"cwd":self.cwd,"environmentNames":names})
        }
    }
}
struct Output {
    bytes: VecDeque<u8>,
    limit: usize,
    truncated: bool,
}
#[derive(Default)]
struct Decoder {
    pending: Vec<u8>,
}
impl Decoder {
    fn feed(&mut self, bytes: &[u8], output: &mut Output) {
        self.pending.extend_from_slice(bytes);
        let mut offset = 0;
        loop {
            match std::str::from_utf8(&self.pending[offset..]) {
                Ok(_) => {
                    output.append(&self.pending[offset..]);
                    offset = self.pending.len();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    output.append(&self.pending[offset..offset + valid]);
                    offset += valid;
                    if let Some(length) = error.error_len() {
                        output.append("�".as_bytes());
                        offset += length;
                    } else {
                        break;
                    }
                }
            }
        }
        self.pending.drain(..offset);
    }
    fn finish(&mut self, output: &mut Output) {
        if !self.pending.is_empty() {
            output.append("�".as_bytes());
            self.pending.clear();
        }
    }
}
impl Output {
    fn append(&mut self, bytes: &[u8]) {
        self.bytes.extend(bytes);
        while self.bytes.len() > self.limit {
            self.bytes.pop_front();
            self.truncated = true;
        }
        // Remove continuation bytes at the truncated beginning, preserving a UTF-8 boundary.
        while self.bytes.front().is_some_and(|byte| byte & 0xc0 == 0x80) {
            self.bytes.pop_front();
            self.truncated = true;
        }
    }
    fn snapshot(&self) -> (String, bool) {
        let bytes: Vec<u8> = self.bytes.iter().copied().collect();
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        let mut truncated = self.truncated;
        if text.len() > self.limit {
            let mut boundary = text.len() - self.limit;
            while !text.is_char_boundary(boundary) {
                boundary += 1;
            }
            text.drain(..boundary);
            truncated = true;
        }
        (text, truncated)
    }
}
struct Terminal {
    output: Arc<Mutex<Output>>,
    exit: watch::Receiver<Option<Value>>,
    stop: watch::Sender<bool>,
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}
pub struct TerminalService {
    session_id: String,
    workspace: PathBuf,
    maximum_output: usize,
    terminals: HashMap<String, Terminal>,
}
impl TerminalService {
    pub fn new(session_id: &str, workspace: &Path, maximum_output: usize) -> Result<Self> {
        if session_id.is_empty() || maximum_output == 0 || maximum_output > 1024 * 1024 {
            bail!("invalid terminal service limits")
        }
        Ok(Self {
            session_id: session_id.into(),
            workspace: workspace.canonicalize()?,
            maximum_output,
            terminals: HashMap::new(),
        })
    }
    /// Canonical workspace root of this session.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }
    pub fn prepare(&self, params: Value) -> Result<PreparedTerminal> {
        if params.to_string().len() > 128 * 1024 {
            bail!("oversized terminal request")
        }
        let request: CreateRequest =
            serde_json::from_value(params).context("invalid terminal creation")?;
        if request.session_id != self.session_id
            || request.command.is_empty()
            || request.command.contains('\0')
            || request.args.iter().any(|arg| arg.contains('\0'))
            || request.env.len() > 128
        {
            bail!("invalid terminal command or session")
        }
        if request.env.iter().any(|entry| {
            entry.name.is_empty() || entry.name.contains(['=', '\0']) || entry.value.contains('\0')
        }) {
            bail!("invalid terminal environment")
        }
        let mut names = std::collections::HashSet::new();
        if request.env.iter().any(|entry| {
            !names.insert(if cfg!(windows) {
                entry.name.to_uppercase()
            } else {
                entry.name.clone()
            })
        }) {
            bail!("duplicate terminal environment name")
        }
        let cwd = request
            .cwd
            .clone()
            .unwrap_or_else(|| self.workspace.clone());
        if !cwd.is_absolute() {
            bail!("terminal working directory must be absolute")
        }
        let cwd = cwd.canonicalize()?;
        if !cwd.is_dir() || !cwd.starts_with(&self.workspace) {
            bail!("terminal working directory outside workspace")
        }
        let limit = request
            .output_byte_limit
            .unwrap_or(self.maximum_output as u64)
            .min(self.maximum_output as u64) as usize;
        if is_shell_line(&request.command, &request.args) {
            // cmd.exe stops at a line break, so a multi-line command could not run as approved.
            if cfg!(windows) && request.command.contains(['\r', '\n']) {
                bail!("multi-line shell commands are not supported on Windows")
            }
            return Ok(PreparedTerminal {
                request,
                cwd,
                limit,
                executable: shell_executable()?,
                shell: true,
            });
        }
        let program =
            if Path::new(&request.command).is_relative() && request.command.contains(['/', '\\']) {
                cwd.join(&request.command).to_string_lossy().into_owned()
            } else {
                request.command.clone()
            };
        let path = request
            .env
            .iter()
            .find(|entry| {
                if cfg!(windows) {
                    entry.name.eq_ignore_ascii_case("PATH")
                } else {
                    entry.name == "PATH"
                }
            })
            .map(|entry| entry.value.as_str());
        let executable = crate::registry::resolve_executable(&program, path)
            .context("terminal executable unavailable")?
            .canonicalize()?;
        Ok(PreparedTerminal {
            request,
            cwd,
            limit,
            executable,
            shell: false,
        })
    }
    pub fn create_approved(&mut self, prepared: PreparedTerminal) -> Result<Value> {
        if prepared.request.session_id != self.session_id {
            bail!("approved terminal belongs to another session")
        }
        if self.terminals.len() >= 8 {
            bail!("terminal capacity reached; release a handle first")
        }
        static PROCESSES: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
        let permit = PROCESSES
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(32)))
            .clone()
            .try_acquire_owned()
            .context("host terminal process limit reached")?;
        // Revalidate after consent. Commands still run with the operator's permissions;
        // cwd containment does not sandbox command arguments or filesystem access.
        let cwd = prepared.cwd.canonicalize()?;
        if cwd != prepared.cwd || !cwd.starts_with(&self.workspace) {
            bail!("terminal directory changed while awaiting consent")
        }
        let mut command = Command::new(&prepared.executable);
        if prepared.shell {
            // The approved line is passed as one argument and never re-parsed by the host.
            #[cfg(unix)]
            command.arg("-c").arg(&prepared.request.command);
            // /S makes cmd strip only the outer quotes and run the inner text unchanged.
            #[cfg(windows)]
            command
                .raw_arg("/D /S /C")
                .raw_arg(format!("\"{}\"", prepared.request.command));
        } else {
            command.args(&prepared.request.args);
        }
        command
            .envs(
                prepared
                    .request
                    .env
                    .iter()
                    .map(|entry| (&entry.name, &entry.value)),
            )
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let (mut child, mut tree) = ProcessTree::spawn(&mut command)?;
        let mut stdout = child.stdout.take().context("terminal stdout unavailable")?;
        let mut stderr = child.stderr.take().context("terminal stderr unavailable")?;
        let output = Arc::new(Mutex::new(Output {
            bytes: VecDeque::new(),
            limit: prepared.limit,
            truncated: false,
        }));
        let (exit_tx, exit) = watch::channel(None);
        let (stop, mut stop_rx) = watch::channel(false);
        let retained = output.clone();
        tokio::spawn(async move {
            // Retain the global slot through process exit and bounded output draining.
            let _permit = permit;
            let mut out = [0u8; 4096];
            let mut err = [0u8; 4096];
            let mut out_closed = false;
            let mut err_closed = false;
            let mut out_decoder = Decoder::default();
            let mut err_decoder = Decoder::default();
            let mut stopping = false;
            let mut status = None;
            let mut deadline = Instant::now() + Duration::from_secs(3600);
            loop {
                if status.is_some() && out_closed && err_closed {
                    break;
                }
                tokio::select! {
                    result=stdout.read(&mut out),if !out_closed => match result {Ok(0)|Err(_)=>{out_closed=true;out_decoder.finish(&mut retained.lock().unwrap());},Ok(count)=>out_decoder.feed(&out[..count],&mut retained.lock().unwrap())},
                    result=stderr.read(&mut err),if !err_closed => match result {Ok(0)|Err(_)=>{err_closed=true;err_decoder.finish(&mut retained.lock().unwrap());},Ok(count)=>err_decoder.feed(&err[..count],&mut retained.lock().unwrap())},
                    result=child.wait(),if status.is_none()=> {
                        status=Some(match result {
                            Ok(status)=> {
                                #[cfg(unix)] let signal={use std::os::unix::process::ExitStatusExt;status.signal().map(|signal|signal.to_string())};
                                #[cfg(windows)] let signal:Option<String>=None;
                                json!({"exitCode":status.code(),"signal":signal})
                            },
                            Err(_)=>json!({"exitCode":null,"signal":null}),
                        });
                        let _=tree.terminate();deadline=Instant::now()+Duration::from_secs(1);
                    },
                    _=stop_rx.changed(),if !stopping && status.is_none()=> {stopping=true;let _=tree.terminate();let _=child.start_kill();},
                    _=tokio::time::sleep_until(deadline)=> {
                        if status.is_some() {break;}
                        stopping=true;let _=tree.terminate();let _=child.start_kill();
                        deadline=Instant::now()+Duration::from_secs(5);
                    },
                }
            }
            let _ = exit_tx.send(status);
        });
        let id = format!("acpd-terminal-{}", uuid::Uuid::new_v4());
        self.terminals
            .insert(id.clone(), Terminal { output, exit, stop });
        Ok(json!({"terminalId":id}))
    }
    fn get(&self, session_id: &str, id: &str) -> Result<&Terminal> {
        if session_id != self.session_id {
            bail!("terminal session mismatch")
        }
        self.terminals
            .get(id)
            .context("terminal handle unavailable")
    }
    pub fn output(&self, session_id: &str, id: &str) -> Result<Value> {
        let terminal = self.get(session_id, id)?;
        let (output, truncated) = terminal.output.lock().unwrap().snapshot();
        let mut value = json!({"output":output,"truncated":truncated});
        if let Some(status) = terminal.exit.borrow().clone() {
            value["exitStatus"] = status;
        }
        Ok(value)
    }
    pub fn exit_receiver(
        &self,
        session_id: &str,
        id: &str,
    ) -> Result<watch::Receiver<Option<Value>>> {
        Ok(self.get(session_id, id)?.exit.clone())
    }
    pub async fn wait(mut exit: watch::Receiver<Option<Value>>) -> Result<Value> {
        loop {
            if let Some(status) = exit.borrow().clone() {
                return Ok(status);
            }
            exit.changed().await.context("terminal runner closed")?;
        }
    }
    pub async fn kill(&self, session_id: &str, id: &str) -> Result<Value> {
        let terminal = self.get(session_id, id)?;
        let _ = terminal.stop.send(true);
        tokio::time::timeout(Duration::from_secs(5), Self::wait(terminal.exit.clone()))
            .await
            .context("terminal termination deadline exceeded")??;
        Ok(json!({}))
    }
    pub async fn release(&mut self, session_id: &str, id: &str) -> Result<Value> {
        self.kill(session_id, id).await?;
        self.terminals.remove(id);
        Ok(json!({}))
    }
    pub async fn shutdown(&mut self) {
        self.stop_all();
        for terminal in self.terminals.values() {
            let _ = terminal.stop.send(true);
        }
        let exits = self
            .terminals
            .values()
            .map(|terminal| Self::wait(terminal.exit.clone()))
            .collect::<Vec<_>>();
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            futures_util::future::join_all(exits),
        )
        .await;
        self.terminals.clear();
    }
    pub fn stop_all(&self) {
        for terminal in self.terminals.values() {
            let _ = terminal.stop.send(true);
        }
    }
}
