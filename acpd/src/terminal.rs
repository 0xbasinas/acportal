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
    #[cfg(unix)]
    #[test]
    fn sandbox_arguments_expose_only_the_workspace_writable() {
        let args: Vec<String> = sandbox_args(
            Path::new("/w"),
            Path::new("/w/src"),
            Path::new("/home/u/.cargo/bin/cargo"),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        let joined = args.join(" ");
        assert!(joined.contains("--bind /w /w"));
        assert!(joined.contains("--ro-bind-try /home/u/.cargo/bin/cargo /home/u/.cargo/bin/cargo"));
        assert!(joined.contains("--tmpfs /tmp"));
        assert!(joined.contains("--chdir /w/src"));
        assert!(!joined.contains("--bind /home") && !joined.contains("--bind / "));
        assert_eq!(args.last().unwrap(), "/home/u/.cargo/bin/cargo");
        assert_eq!(args[args.len() - 2], "--");
    }
    /// TEMPORARY diagnostic for the Git Bash empty-output flake; prints a summary and fails.
    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread")]
    async fn diag_git_bash_output() {
        use std::os::windows::process::CommandExt;
        let Some(bash) = crate::shell::windows_find_bash_for_tests() else {
            return;
        };
        let inner = bash
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(r"usr\bin\bash.exe");
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("acpd-diag-{}", std::process::id()));
        let root = base.join("ws");
        std::fs::create_dir_all(&root).unwrap();
        let line = "echo \"bash-$((40+2))\" > out.txt; cat out.txt";
        let mut report = format!(
            "bash={} inner={} exists={}\n",
            bash.display(),
            inner.display(),
            inner.exists()
        );
        let direct = |exe: &Path, flags: u32, line: &str| {
            let t = std::time::Instant::now();
            let out = std::process::Command::new(exe)
                .args(["--noprofile", "--norc", "-c", line])
                .current_dir(&root)
                .stdin(Stdio::null())
                .creation_flags(flags)
                .output()
                .unwrap();
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
                t.elapsed().as_millis(),
            )
        };
        for (name, exe, flags) in [
            ("launcher-direct", bash.clone(), 0u32),
            ("launcher-direct-nowindow", bash.clone(), 0x0800_0000),
            ("inner-direct-nowindow", inner.clone(), 0x0800_0000),
        ] {
            let mut bad = vec![];
            for i in 0..5 {
                let (code, out, err, ms) = direct(&exe, flags, line);
                if !out.contains("bash-42") {
                    bad.push(format!(
                        "#{i} code={code:?} ms={ms} out={out:?} err={err:?}"
                    ))
                }
            }
            report += &format!("{name}: {} bad {:?}\n", bad.len(), bad);
            println!("{report}");
        }
        // Concurrent: several services at once, like parallel tests.
        let mut tasks = vec![];
        for k in 0..6 {
            let dir = base.join(format!("ws{k}"));
            std::fs::create_dir_all(&dir).unwrap();
            let line = line.to_owned();
            tasks.push(tokio::spawn(async move {
                let mut service = TerminalService::new("s", &dir, 8192).unwrap();
                service.set_shell(crate::shell::ShellChoice::Bash);
                let mut bad = vec![];
                for i in 0..25 {
                    let t = std::time::Instant::now();
                    let prepared = service
                        .prepare(serde_json::json!({"sessionId":"s","command":line}))
                        .unwrap();
                    let created = service.create_approved(prepared).unwrap();
                    let id = created["terminalId"].as_str().unwrap().to_owned();
                    let exit = service.exit_receiver("s", &id).unwrap();
                    let status = TerminalService::wait(exit).await.unwrap();
                    let text = service.output("s", &id).unwrap()["output"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned();
                    let _ = service.release("s", &id).await;
                    if !text.contains("bash-42") {
                        bad.push(format!(
                            "k{k}#{i} status={status} ms={} text={text:?}",
                            t.elapsed().as_millis()
                        ));
                    }
                }
                bad
            }));
        }
        let mut bad = vec![];
        for t in tasks {
            bad.extend(t.await.unwrap());
        }
        report += &format!("service-concurrent: {} bad of 150 {:?}\n", bad.len(), bad);
        // Concurrent direct runs (threads), no job object.
        let handles: Vec<_> = (0..6)
            .map(|k| {
                let dir = base.join(format!("ws{k}"));
                let bash = bash.clone();
                let line = line.to_owned();
                std::thread::spawn(move || {
                    let mut bad = vec![];
                    for i in 0..25 {
                        let out = std::process::Command::new(&bash)
                            .args(["--noprofile", "--norc", "-c", &line])
                            .current_dir(&dir)
                            .stdin(Stdio::null())
                            .creation_flags(0x0800_0000)
                            .output()
                            .unwrap();
                        let text = String::from_utf8_lossy(&out.stdout).into_owned();
                        if !text.contains("bash-42") {
                            bad.push(format!(
                                "k{k}#{i} code={:?} out={text:?} err={:?}",
                                out.status.code(),
                                String::from_utf8_lossy(&out.stderr)
                            ));
                        }
                    }
                    bad
                })
            })
            .collect();
        let mut bad = vec![];
        for h in handles {
            bad.extend(h.join().unwrap());
        }
        report += &format!("direct-concurrent: {} bad of 150 {:?}\n", bad.len(), bad);
        let _ = std::fs::remove_dir_all(&base);
        panic!("DIAG\n{report}");
    }
    #[cfg(windows)]
    fn launcher_for_tests() -> Option<PathBuf> {
        let exe = std::env::current_exe().unwrap();
        let launcher = exe.parent()?.parent()?.join("acpd.exe");
        (crate::win_sandbox::available() && launcher.exists()).then_some(launcher)
    }
    #[cfg(windows)]
    async fn run_line(service: &mut TerminalService, line: &str) -> (PreparedInfo, String) {
        let prepared = service
            .prepare(serde_json::json!({"sessionId":"s","command":line}))
            .unwrap();
        let info = PreparedInfo {
            shell: prepared.shell_name().map(str::to_owned),
            note: prepared.shell_note().map(str::to_owned),
        };
        let created = service.create_approved(prepared).unwrap();
        let id = created["terminalId"].as_str().unwrap().to_owned();
        let exit = service.exit_receiver("s", &id).unwrap();
        tokio::time::timeout(Duration::from_secs(60), TerminalService::wait(exit))
            .await
            .unwrap()
            .unwrap();
        let text = service.output("s", &id).unwrap()["output"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        (info, text)
    }
    #[cfg(windows)]
    struct PreparedInfo {
        shell: Option<String>,
        note: Option<String>,
    }
    /// Spike: Git Bash (MSYS2) inside the AppContainer launcher. Expected to fail, which is why
    /// `shell::BASH_IN_SANDBOX` is false; PowerShell must work there.
    #[cfg(windows)]
    #[test]
    fn spike_git_bash_cannot_start_in_the_appcontainer_but_powershell_can() {
        let Some(launcher) = launcher_for_tests() else {
            return;
        };
        let Some(bash) = crate::shell::windows_find_bash_for_tests() else {
            eprintln!("Git Bash not installed; skipping");
            return;
        };
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("acpd-spike-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let ws = base.canonicalize().unwrap();
        let run = |exe: &Path, args: &[&str]| {
            let profile = crate::win_sandbox::profile_name();
            let out = std::process::Command::new(&launcher)
                .args(crate::win_sandbox::launcher_args(&ws, &ws, exe, &profile))
                .args(args)
                .current_dir(&ws)
                .output()
                .unwrap();
            crate::win_sandbox::cleanup(&profile, &ws);
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).to_string()
                    + &String::from_utf8_lossy(&out.stderr),
            )
        };
        let (bash_code, bash_out) = run(&bash, &["--noprofile", "--norc", "-c", "echo bash-ok"]);
        let usr_bash = bash
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(r"usr\bin\bash.exe");
        let (usr_code, usr_out) = run(&usr_bash, &["--noprofile", "--norc", "-c", "echo bash-ok"]);
        assert!(
            crate::shell::is_msys_program(&usr_bash),
            "usr\\bin\\bash.exe should import msys-2.0.dll"
        );
        let ps = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let (ps_code, ps_out) = run(
            &ps,
            &[
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Write-Output ps-ok",
            ],
        );
        let report = format!(
            "bin\\bash: {bash_code:?} {bash_out:?}\nusr\\bin\\bash: {usr_code:?} {usr_out:?}\npowershell: {ps_code:?} {ps_out:?}"
        );
        let _ = std::fs::remove_dir_all(&base);
        assert!(ps_out.contains("ps-ok"), "{report}");
        assert_eq!(
            bash_out.contains("bash-ok") || usr_out.contains("bash-ok"),
            crate::shell::BASH_IN_SANDBOX,
            "{report}"
        );
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn windows_shell_choice_runs_lines_in_the_chosen_shell() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("acpd-shells-{}", std::process::id()));
        let root = base.join("ws");
        std::fs::create_dir_all(&root).unwrap();
        let mut service = TerminalService::new("s", &root, 8192).unwrap();
        // Default (auto): cmd.exe, as before.
        let (info, text) = run_line(&mut service, "echo %OS%").await;
        assert_eq!(info.shell.as_deref(), Some("cmd"));
        assert!(text.contains("Windows_NT"), "{text}");
        service.set_shell(crate::shell::ShellChoice::Pwsh);
        let (info, text) = run_line(&mut service, "Write-Output (\"ps-\" + 'ok')").await;
        assert_eq!(info.shell.as_deref(), Some("powershell"));
        assert!(text.contains("ps-ok"), "{text}");
        if crate::shell::windows_find_bash_for_tests().is_some() {
            service.set_shell(crate::shell::ShellChoice::Bash);
            let (info, text) = run_line(
                &mut service,
                "echo \"bash-$((40+2))\" > out.txt; cat out.txt",
            )
            .await;
            assert_eq!((info.shell.as_deref(), info.note), (Some("bash"), None));
            assert!(text.contains("bash-42"), "{text}");
            if let Some(launcher) = launcher_for_tests() {
                // Sandboxed: bash is replaced by PowerShell, with a note on the card.
                service.sandbox = Some(launcher);
                let (info, text) = run_line(&mut service, "Write-Output sandboxed-ok").await;
                assert_eq!(info.shell.as_deref(), Some("powershell"));
                assert!(info.note.unwrap_or_default().contains("sandbox"));
                assert!(text.contains("sandboxed-ok"), "{text}");
                // An MSYS program run directly is refused with a clear error.
                let grep = crate::shell::windows_find_bash_for_tests()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join(r"usr\bin\grep.exe");
                if grep.exists() {
                    let error = service
                        .prepare(
                            serde_json::json!({"sessionId":"s","command":grep,"args":["x","y"]}),
                        )
                        .err()
                        .unwrap()
                        .to_string();
                    assert!(error.contains("MSYS2"), "{error}");
                }
            }
        }
        let _ = std::fs::remove_dir_all(&base);
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn windows_sandbox_writes_only_the_workspace_and_cleans_up() {
        if !crate::win_sandbox::available() {
            eprintln!("AppContainer unavailable; skipping");
            return;
        }
        // Unit tests run from target/<profile>/deps; the launcher lives in the acpd binary.
        let exe = std::env::current_exe().unwrap();
        let launcher = exe.parent().unwrap().parent().unwrap().join("acpd.exe");
        if !launcher.exists() {
            eprintln!("acpd.exe not built; skipping");
            return;
        }
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("acpd-winsbx-{}", std::process::id()));
        let root = base.join("ws");
        std::fs::create_dir_all(&root).unwrap();
        let outside = base.join("outside.txt");
        let profile = std::env::var("USERPROFILE").unwrap();
        let mut service = TerminalService::new("s", &root, 8192).unwrap();
        service.sandbox = Some(launcher);
        let line = format!(
            "echo in> inside.txt & (echo out> \"{}\") 2>nul & dir /b \"{}\" >nul 2>nul && echo profile-visible || echo profile-hidden & type C:\\Windows\\System32\\drivers\\etc\\hosts",
            outside.display(),
            profile
        );
        let prepared = service
            .prepare(serde_json::json!({"sessionId":"s","command":line}))
            .unwrap();
        assert!(prepared.sandboxed());
        let created = service.create_approved(prepared).unwrap();
        let id = created["terminalId"].as_str().unwrap().to_owned();
        let exit = service.exit_receiver("s", &id).unwrap();
        tokio::time::timeout(Duration::from_secs(60), TerminalService::wait(exit))
            .await
            .unwrap()
            .unwrap();
        let text = service.output("s", &id).unwrap()["output"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(root.join("inside.txt").exists(), "{text}");
        assert!(!outside.exists(), "{text}");
        assert!(text.contains("profile-hidden"), "{text}");
        assert!(text.contains("localhost"), "system dirs readable: {text}");
        let acl = std::process::Command::new("icacls")
            .arg(&root)
            .output()
            .unwrap();
        let acl = String::from_utf8_lossy(&acl.stdout).to_string();
        assert!(
            !acl.contains("S-1-15-2-"),
            "AppContainer grant left behind: {acl}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn sandboxed_shell_cannot_write_outside_or_see_home() {
        if sandbox_runner().is_none() {
            eprintln!("bubblewrap unavailable; skipping");
            return;
        }
        // Not under /tmp, which the sandbox replaces with its own tmpfs.
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("acpd-sandbox-{}", std::process::id()));
        let root = base.join("ws");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(base.join("sibling.txt"), "secret").unwrap();
        let outside = base.join("outside.txt");
        let mut service = TerminalService::new("s", &root, 4096).unwrap();
        assert!(service.enable_sandbox());
        let line = format!(
            "echo in > inside.txt; echo out > {} 2>/dev/null; echo status=$?; test -e {} && echo sibling-visible || echo sibling-hidden",
            outside.display(),
            base.join("sibling.txt").display()
        );
        let prepared = service
            .prepare(serde_json::json!({"sessionId":"s","command":line}))
            .unwrap();
        assert!(prepared.sandboxed());
        assert!(!prepared.outside().is_empty());
        let created = service.create_approved(prepared).unwrap();
        let id = created["terminalId"].as_str().unwrap().to_owned();
        let exit = service.exit_receiver("s", &id).unwrap();
        tokio::time::timeout(Duration::from_secs(20), TerminalService::wait(exit))
            .await
            .unwrap()
            .unwrap();
        let text = service.output("s", &id).unwrap()["output"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(root.join("inside.txt").exists(), "{text}");
        assert!(!outside.exists(), "{text}");
        assert!(text.contains("sibling-hidden"), "{text}");
        let _ = std::fs::remove_dir_all(&base);
    }
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
    /// True when `request.command` is a shell line that runs through the host shell (see `crate::shell`).
    shell: bool,
    /// Paths the command names outside the workspace (see `shell_review::outside_paths`).
    outside: Vec<String>,
    /// Will run inside the OS sandbox.
    sandboxed: bool,
    /// The shell for a shell line.
    shell_spec: Option<crate::shell::Shell>,
}
/// How an agent's host terminals run: OS sandbox (`sandbox: auto`) and shell choice.
#[derive(Clone, Copy, Debug, Default)]
pub struct TerminalPolicy {
    pub sandbox: bool,
    pub shell: crate::shell::ShellChoice,
}
/// Build the command for `executable`, wrapped by the sandbox `runner` when given. On Windows
/// the second value is the AppContainer profile to pass to [`sandbox_cleanup`] afterwards.
pub fn sandboxed_command(
    runner: Option<&Path>,
    workspace: &Path,
    cwd: &Path,
    executable: &Path,
) -> (Command, Option<String>) {
    let Some(runner) = runner else {
        return (Command::new(executable), None);
    };
    let mut command = Command::new(runner);
    #[cfg(windows)]
    {
        let profile = crate::win_sandbox::profile_name();
        command.args(crate::win_sandbox::launcher_args(
            workspace, cwd, executable, &profile,
        ));
        (command, Some(profile))
    }
    #[cfg(not(windows))]
    {
        command.args(sandbox_args(workspace, cwd, executable));
        (command, None)
    }
}
/// Remove what the Windows sandbox granted for `profile` (idempotent; nothing elsewhere).
pub fn sandbox_cleanup(profile: Option<String>, workspace: &Path) {
    #[cfg(windows)]
    if let Some(profile) = profile {
        crate::win_sandbox::cleanup(&profile, workspace);
    }
    #[cfg(not(windows))]
    let _ = (profile, workspace);
}
/// The default shell label, used in the model reviewer's prompt.
#[cfg(unix)]
pub const SHELL_LABEL: &str = "/bin/sh -c";
#[cfg(windows)]
pub const SHELL_LABEL: &str = "cmd.exe /D /S /C";
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
    pub fn outside(&self) -> &[String] {
        &self.outside
    }
    pub fn sandboxed(&self) -> bool {
        self.sandboxed
    }
    /// The exact shell line, when this request runs through the host shell.
    pub fn shell_line(&self) -> Option<&str> {
        self.shell.then_some(self.request.command.as_str())
    }
    /// Canonical working directory inside the workspace.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }
    /// Agent-supplied values can change executable resolution and loader behavior.
    /// Agent-supplied environment overrides as (name, value). For review decisions
    /// only; values can contain credentials and are never logged or shown.
    pub fn environment(&self) -> impl Iterator<Item = (&str, &str)> {
        self.request
            .env
            .iter()
            .map(|variable| (variable.name.as_str(), variable.value.as_str()))
    }
    /// The ACP session id of the request, already checked against this session.
    pub fn session_id(&self) -> &str {
        &self.request.session_id
    }
    pub fn has_environment_overrides(&self) -> bool {
        !self.request.env.is_empty()
    }
    pub fn is_shell(&self) -> bool {
        self.shell
    }
    /// `sh`, `cmd`, `powershell` or `bash` for a shell line.
    pub fn shell_name(&self) -> Option<&'static str> {
        self.shell_spec.as_ref().map(|s| s.name())
    }
    /// Why the line runs in a different shell than configured (shown on the card).
    pub fn shell_note(&self) -> Option<&str> {
        self.shell_spec.as_ref().and_then(|s| s.note.as_deref())
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
            let spec = self.shell_spec.as_ref();
            json!({"shellLine":self.request.command,"shell":spec.map(|s| s.label()),"shellNote":spec.and_then(|s| s.note.clone()),"executable":self.executable,"cwd":self.cwd,"environmentNames":names})
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
    sandbox: Option<PathBuf>,
    shell: crate::shell::ShellChoice,
}

/// Where a working bubblewrap lives, probed once. `None` off Linux, when it is not
/// installed, or when it cannot create namespaces here (e.g. inside some containers).
pub fn sandbox_runner() -> Option<PathBuf> {
    static RUNNER: OnceLock<Option<PathBuf>> = OnceLock::new();
    RUNNER
        .get_or_init(|| {
            #[cfg(windows)]
            if crate::win_sandbox::launcher_enabled() && crate::win_sandbox::available() {
                return std::env::current_exe().ok();
            }
            if !cfg!(target_os = "linux") {
                return None;
            }
            let path = crate::registry::resolve_executable("bwrap", None)?;
            let works = std::process::Command::new(&path)
                .args([
                    "--ro-bind",
                    "/",
                    "/",
                    "--dev",
                    "/dev",
                    "--die-with-parent",
                    "--",
                    "true",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            works.then_some(path)
        })
        .clone()
}

/// bubblewrap arguments that expose system directories read-only, a private `/tmp`, and
/// only `workspace` writable. The home directory and the rest of the filesystem are absent.
pub fn sandbox_args(workspace: &Path, cwd: &Path, executable: &Path) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec![];
    const SYSTEM: [&str; 9] = [
        "/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/etc", "/opt", "/nix",
    ];
    for dir in SYSTEM {
        // Merged-/usr systems make /bin and friends symlinks; recreate them as links.
        match std::fs::read_link(dir) {
            Ok(target) => args.extend(["--symlink".into(), target.into(), dir.into()]),
            Err(_) => args.extend(["--ro-bind-try".into(), dir.into(), dir.into()]),
        }
    }
    if !executable.starts_with(workspace) && !SYSTEM.iter().any(|dir| executable.starts_with(dir)) {
        args.extend(["--ro-bind-try".into(), executable.into(), executable.into()]);
    }
    for item in ["--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp"] {
        args.push(item.into());
    }
    args.extend(["--bind".into(), workspace.into(), workspace.into()]);
    args.extend(["--chdir".into(), cwd.into()]);
    args.extend(["--die-with-parent".into(), "--".into()]);
    args.push(executable.into());
    args
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
            sandbox: None,
            shell: Default::default(),
        })
    }
    /// Run approved commands inside bubblewrap when it works on this host. Returns whether
    /// the sandbox is active.
    pub fn enable_sandbox(&mut self) -> bool {
        self.sandbox = sandbox_runner();
        self.sandbox.is_some()
    }
    pub fn sandboxed(&self) -> bool {
        self.sandbox.is_some()
    }
    /// The agent's shell override for shell lines (Windows; `auto` = host setting).
    pub fn set_shell(&mut self, shell: crate::shell::ShellChoice) {
        self.shell = shell;
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
        let outside = if is_shell_line(&request.command, &request.args) {
            crate::shell_review::outside_paths(&request.command, &cwd, &self.workspace)
        } else {
            let quoted: Vec<String> = request
                .args
                .iter()
                .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
                .collect();
            crate::shell_review::outside_paths(
                &format!("run {}", quoted.join(" ")),
                &cwd,
                &self.workspace,
            )
        };
        if is_shell_line(&request.command, &request.args) {
            let spec = crate::shell::resolve(Some(self.shell), self.sandbox.is_some())?;
            if !spec.allows_multiline() && request.command.contains(['\r', '\n']) {
                bail!("multi-line shell commands are not supported on Windows")
            }
            return Ok(PreparedTerminal {
                request,
                cwd,
                limit,
                executable: spec.executable.clone(),
                shell: true,
                outside,
                sandboxed: self.sandbox.is_some(),
                shell_spec: Some(spec),
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
        if cfg!(windows) && self.sandbox.is_some() && crate::shell::is_msys_program(&executable) {
            bail!(
                "{} is an MSYS2/Cygwin program (Git Bash tools), which cannot run inside the Windows sandbox; set this agent's sandbox to off to run it",
                executable.display()
            )
        }
        Ok(PreparedTerminal {
            request,
            cwd,
            limit,
            executable,
            shell: false,
            outside,
            sandboxed: self.sandbox.is_some(),
            shell_spec: None,
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
        let (mut command, sandbox_profile) = sandboxed_command(
            self.sandbox.as_deref(),
            &self.workspace,
            &cwd,
            &prepared.executable,
        );
        if let Some(spec) = &prepared.shell_spec {
            // The approved line is passed as one argument and never re-parsed by the host.
            spec.push_line(&mut command, &prepared.request.command);
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
        let cleanup = (sandbox_profile, self.workspace.clone());
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
            // A killed Windows launcher cannot clean up; repeat it (idempotent) before reporting.
            if cleanup.0.is_some() {
                let _ = tokio::task::spawn_blocking(move || sandbox_cleanup(cleanup.0, &cleanup.1))
                    .await;
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
