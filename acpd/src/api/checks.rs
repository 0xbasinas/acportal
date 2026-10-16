//! Pre-merge checks: `POST /v1/checks` with `{"path", "commands": ["cargo test", ...]}`.
//!
//! A paired device may already approve any shell command an agent asks for, so running the
//! test and lint lines the user configured for a workspace adds no new authority. The lines run
//! one after another through `/bin/sh -c` in a folder inside the workspace roots or the task
//! worktrees, inside the host OS sandbox when one works here (only that folder writable, home
//! hidden), with a deadline and a bounded output tail. Each answer says whether it was sandboxed.
use super::{ApiError, ApiResult, Host};
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};

const MAX_COMMANDS: usize = 8;
const MAX_LINE: usize = 2000;
const TAIL: usize = 32 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct ChecksInput {
    path: std::path::PathBuf,
    commands: Vec<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

/// Untracked, not-ignored files of the git checkout at `dir` (empty outside git).
fn untracked(dir: &Path) -> std::collections::HashSet<String> {
    std::process::Command::new("git")
        .args([
            "-c",
            "core.quotepath=off",
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            o.stdout
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Remove untracked files the checks created (bytecode, caches, reports), so committing the
/// run afterwards takes only the agent's changes. Returns how many were removed.
fn remove_new_files(dir: &Path, before: &std::collections::HashSet<String>) -> usize {
    let mut removed = 0;
    for path in untracked(dir).difference(before) {
        if path.split('/').any(|c| c == "..") {
            continue;
        }
        let full = dir.join(path);
        if std::fs::remove_file(&full).is_ok() {
            removed += 1;
            let mut parent = full.parent();
            while let Some(p) = parent {
                if p == dir || std::fs::remove_dir(p).is_err() {
                    break;
                }
                parent = p.parent();
            }
        }
    }
    removed
}

/// Run each line in `dir`; stops nothing early so every result is shown. Untracked files the
/// lines create are removed afterwards (see [`remove_new_files`]).
pub async fn run_checks(
    dir: &Path,
    commands: &[String],
    sandbox: Option<&Path>,
    limit: Duration,
) -> Vec<Value> {
    let before = untracked(dir);
    let results = run_lines(dir, commands, sandbox, limit).await;
    let removed = remove_new_files(dir, &before);
    if removed > 0 {
        tracing::info!(
            removed,
            "pre-merge checks left untracked files; removed them"
        );
    }
    results
}

async fn run_lines(
    dir: &Path,
    commands: &[String],
    sandbox: Option<&Path>,
    limit: Duration,
) -> Vec<Value> {
    let mut out = vec![];
    for line in commands {
        let started = std::time::Instant::now();
        let shell = Path::new("/bin/sh");
        let mut command = match sandbox {
            Some(runner) => {
                let mut c = tokio::process::Command::new(runner);
                c.args(crate::terminal::sandbox_args(dir, dir, shell));
                c
            }
            None => tokio::process::Command::new(shell),
        };
        command
            .arg("-c")
            .arg(line)
            .current_dir(dir)
            .env("CI", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let result = match command.spawn() {
            Err(error) => {
                json!({"command":line,"code":null,"output":format!("could not start: {error}"),"ms":0})
            }
            Ok(child) => match tokio::time::timeout(limit, child.wait_with_output()).await {
                Err(_) => {
                    json!({"command":line,"code":null,"output":format!("timed out after {} s", limit.as_secs()),"ms":started.elapsed().as_millis() as u64,"timedOut":true})
                }
                Ok(Err(error)) => {
                    json!({"command":line,"code":null,"output":error.to_string(),"ms":0})
                }
                Ok(Ok(o)) => {
                    let mut bytes = o.stdout;
                    bytes.extend_from_slice(&o.stderr);
                    let start = bytes.len().saturating_sub(TAIL);
                    let text = String::from_utf8_lossy(&bytes[start..]).into_owned();
                    json!({"command":line,"code":o.status.code(),"output":text,"ms":started.elapsed().as_millis() as u64,"truncated":start>0})
                }
            },
        };
        out.push(result);
    }
    out
}

pub(super) async fn run(
    State(host): State<Host>,
    Json(input): Json<ChecksInput>,
) -> ApiResult<Json<Value>> {
    if input.commands.is_empty()
        || input.commands.len() > MAX_COMMANDS
        || input
            .commands
            .iter()
            .any(|c| c.trim().is_empty() || c.len() > MAX_LINE || c.contains('\0'))
    {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_commands"));
    }
    let dir = super::git::allowed(&host, &input.path)
        .ok_or(ApiError(StatusCode::FORBIDDEN, "path_not_allowed"))?
        .canonicalize()
        .map_err(|_| ApiError(StatusCode::FORBIDDEN, "path_not_allowed"))?;
    // The Windows launcher only serves terminal commands; checks there stay unconfined (and say so).
    let sandbox = if cfg!(windows) {
        None
    } else {
        crate::terminal::sandbox_runner()
    };
    let limit = Duration::from_secs(input.timeout_secs.unwrap_or(600).clamp(5, 1800));
    tracing::info!(
        commands = input.commands.len(),
        sandboxed = sandbox.is_some(),
        "pre-merge checks"
    );
    let results = run_checks(&dir, &input.commands, sandbox.as_deref(), limit).await;
    let passed = results.iter().all(|r| r["code"] == json!(0));
    Ok(Json(
        json!({"passed":passed,"sandboxed":sandbox.is_some(),"results":results}),
    ))
}

// Checks run through /bin/sh, so the tests are Unix-only.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn checks_report_each_exit_code_and_stay_in_the_sandbox() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("acpd-checks-{}", std::process::id()));
        let dir = base.join("ws");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(base.join("secret.txt"), "x").unwrap();
        let commands = vec![
            "echo ok".to_string(),
            "echo bad >&2; exit 3".to_string(),
            format!("cat {}", base.join("secret.txt").display()),
        ];
        let plain = run_checks(&dir, &commands, None, Duration::from_secs(20)).await;
        assert_eq!(plain[0]["code"], 0);
        assert!(plain[0]["output"].as_str().unwrap().contains("ok"));
        assert_eq!(plain[1]["code"], 3);
        assert!(plain[1]["output"].as_str().unwrap().contains("bad"));
        assert_eq!(plain[2]["code"], 0);
        let slow = run_checks(&dir, &["sleep 5".into()], None, Duration::from_millis(200)).await;
        assert_eq!(slow[0]["timedOut"], true);
        if let Some(runner) = crate::terminal::sandbox_runner() {
            let boxed = run_checks(&dir, &commands, Some(&runner), Duration::from_secs(20)).await;
            assert_eq!(boxed[0]["code"], 0);
            assert_ne!(boxed[2]["code"], 0, "a file beside the workspace is hidden");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn files_created_by_checks_are_removed_but_agent_files_stay() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        let git = |a: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(a)
                    .current_dir(r)
                    .output()
                    .unwrap()
                    .status
                    .success()
            )
        };
        git(&["init", "-q"]);
        std::fs::write(r.join("agent.py"), "x = 1\n").unwrap();
        let out = run_checks(
            r,
            &["mkdir -p __pycache__ && echo b > __pycache__/a.pyc && echo r > report.txt".into()],
            None,
            Duration::from_secs(20),
        )
        .await;
        assert_eq!(out[0]["code"], 0);
        assert!(
            r.join("agent.py").exists(),
            "the agent's untracked file stays"
        );
        let left: Vec<_> = std::fs::read_dir(r)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(
            !r.join("report.txt").exists() && !r.join("__pycache__").exists(),
            "{left:?} {:?}",
            untracked(r)
        );
    }
}
