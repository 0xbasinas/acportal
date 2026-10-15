//! Git for a paired client: `POST /v1/git/{op}` with `{"path", "params", "input"}`.
//!
//! Only a fixed list of operations runs, without a shell, in a folder inside the host's
//! workspace roots or one of its task worktrees. Values that become git arguments may not start
//! with `-`, so a client cannot smuggle options in. The answer is `{"code","out","err","args"}`;
//! a failing git command is still a 200 with its exit code so the client can show git's words.
use super::{ApiError, ApiResult, Host};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path as FsPath, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncWriteExt;

/// Largest output returned; more is cut with a note.
const MAX_OUTPUT: usize = 2 * 1024 * 1024;
/// Largest stdin (a patch for hunk staging).
pub(super) const MAX_INPUT: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GitInput {
    path: PathBuf,
    #[serde(default)]
    params: HashMap<String, String>,
    #[serde(default)]
    input: Option<String>,
}

/// Operations that only read. Everything else may change the repository.
pub(super) const READ_ONLY: &[&str] = &[
    "status",
    "diff",
    "log",
    "worktree-list",
    "stash-list",
    "branch-list",
    "conflicts",
    "rev-parse",
    "show",
    "reflog",
    "merging",
    "compare",
    "remote-url",
];

fn param<'a>(p: &'a HashMap<String, String>, key: &str) -> Result<&'a str, ApiError> {
    let value = p
        .get(key)
        .map(String::as_str)
        .ok_or(ApiError(StatusCode::BAD_REQUEST, "missing_parameter"))?;
    if value.is_empty() || value.starts_with('-') || value.contains('\0') {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_parameter"));
    }
    Ok(value)
}
fn opt<'a>(p: &'a HashMap<String, String>, key: &str) -> Result<Option<&'a str>, ApiError> {
    match p.get(key) {
        None => Ok(None),
        Some(v) if v.is_empty() => Ok(None),
        Some(_) => param(p, key).map(Some),
    }
}

/// The git arguments for an operation, or `None` for an unknown one.
pub(super) fn args(op: &str, p: &HashMap<String, String>) -> Result<Vec<String>, ApiError> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let mut out = match op {
        "status" => s(&["status", "--porcelain=v1", "-b", "-uall"]),
        "diff" => {
            let mut a = s(&["diff", "-U3", "--no-ext-diff"]);
            if p.get("staged").is_some_and(|v| !v.is_empty()) {
                a.push("--cached".into());
            }
            a.push("--".into());
            if let Some(f) = opt(p, "file")? {
                a.push(f.into());
            }
            a
        }
        "log" => s(&[
            "log",
            "--graph",
            "--all",
            "--date-order",
            "-n",
            "120",
            "--pretty=format:%h\u{1f}%d\u{1f}%s\u{1f}%an\u{1f}%cr",
        ]),
        "worktree-list" => s(&["worktree", "list", "--porcelain"]),
        "stash-list" => s(&["stash", "list", "--pretty=format:%gd\u{1f}%s\u{1f}%cr"]),
        "branch-list" => s(&[
            "branch",
            "--all",
            "--format=%(refname:short)\u{1f}%(objectname:short)\u{1f}%(HEAD)\u{1f}%(upstream:track)\u{1f}%(committerdate:relative)",
        ]),
        "conflicts" => s(&["diff", "--name-only", "--diff-filter=U"]),
        "rev-parse" => s(&[
            "rev-parse",
            "--show-toplevel",
            "HEAD",
            "--abbrev-ref",
            "HEAD",
        ]),
        "merging" => s(&["rev-parse", "--git-path", "MERGE_HEAD"]),
        "show" => vec![
            "show".into(),
            "--stat".into(),
            "--format=%H%n%an%n%cr%n%B".into(),
            param(p, "rev")?.into(),
        ],
        "reflog" => s(&["reflog", "-n", "15", "--pretty=format:%h\u{1f}%gs"]),
        "compare" => vec![
            "log".into(),
            "--left-right".into(),
            "--pretty=format:%m\u{1f}%h\u{1f}%s\u{1f}%cr".into(),
            format!("{}...HEAD", param(p, "base")?),
        ],
        "remote-url" => s(&["remote", "get-url", "origin"]),
        "add" => vec!["add".into(), "--".into(), param(p, "file")?.into()],
        "restore-staged" => vec![
            "restore".into(),
            "--staged".into(),
            "--".into(),
            param(p, "file")?.into(),
        ],
        "apply-cached" => s(&["apply", "--cached", "--recount", "-"]),
        "apply-cached-reverse" => s(&["apply", "--cached", "--recount", "-R", "-"]),
        "commit" => {
            let message = p
                .get("message")
                .filter(|m| !m.trim().is_empty())
                .ok_or(ApiError(StatusCode::BAD_REQUEST, "missing_parameter"))?;
            vec!["commit".into(), "-m".into(), message.clone()]
        }
        "reset-soft" => vec![
            "reset".into(),
            "--soft".into(),
            opt(p, "rev")?.unwrap_or("HEAD~1").into(),
        ],
        "stash-push" => {
            let mut a = s(&["stash", "push", "--include-untracked"]);
            if let Some(m) = p.get("message").filter(|m| !m.is_empty()) {
                a.extend(["-m".into(), m.clone()]);
            }
            a
        }
        "stash-pop" => {
            let mut a = s(&["stash", "pop"]);
            if let Some(r) = opt(p, "ref")? {
                a.push(r.into());
            }
            a
        }
        "stash-apply" => vec!["stash".into(), "apply".into(), param(p, "ref")?.into()],
        "stash-drop" => vec!["stash".into(), "drop".into(), param(p, "ref")?.into()],
        "worktree-add" => {
            let mut a = vec![
                "worktree".into(),
                "add".into(),
                "-b".into(),
                param(p, "branch")?.into(),
                param(p, "path")?.into(),
            ];
            if let Some(base) = opt(p, "base")? {
                a.push(base.into());
            }
            a
        }
        "worktree-remove" => vec!["worktree".into(), "remove".into(), param(p, "path")?.into()],
        "checkout-ours" | "checkout-theirs" => vec![
            "checkout".into(),
            if op == "checkout-ours" {
                "--ours"
            } else {
                "--theirs"
            }
            .into(),
            "--".into(),
            param(p, "file")?.into(),
        ],
        "merge-abort" => s(&["merge", "--abort"]),
        "checkout" => vec!["switch".into(), param(p, "branch")?.into()],
        "branch-create" => {
            let mut a = vec!["switch".into(), "-c".into(), param(p, "branch")?.into()];
            if let Some(base) = opt(p, "base")? {
                a.push(base.into());
            }
            a
        }
        "branch-rename" => vec![
            "branch".into(),
            "-m".into(),
            param(p, "from")?.into(),
            param(p, "to")?.into(),
        ],
        "branch-delete" => vec![
            "branch".into(),
            if p.get("force").is_some_and(|v| v == "1") {
                "-D"
            } else {
                "-d"
            }
            .into(),
            param(p, "branch")?.into(),
        ],
        "push" => vec![
            "push".into(),
            "--set-upstream".into(),
            "origin".into(),
            param(p, "branch")?.into(),
        ],
        _ => return Err(ApiError(StatusCode::NOT_FOUND, "unknown_git_operation")),
    };
    out.insert(0, "-c".into());
    out.insert(1, "core.quotepath=off".into());
    out.insert(2, "-c".into());
    out.insert(3, "color.ui=never".into());
    Ok(out)
}

/// The folder if it is inside a workspace root or the host's task worktrees.
fn allowed(host: &Host, path: &FsPath) -> Option<PathBuf> {
    let settings = host.sessions.launch_settings();
    if let Ok(dir) = settings.config.workspace(path) {
        return Some(dir);
    }
    let real = path.canonicalize().ok()?;
    let worktrees = settings
        .config
        .state_directory
        .join("worktrees")
        .canonicalize()
        .ok()?;
    (real.is_dir() && real.starts_with(worktrees)).then_some(real)
}

fn ref_ok(name: &str) -> bool {
    std::process::Command::new("git")
        .args(["check-ref-format", "--branch", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub(super) async fn run(
    State(host): State<Host>,
    Path(op): Path<String>,
    Json(input): Json<GitInput>,
) -> ApiResult<Json<Value>> {
    let cwd =
        allowed(&host, &input.path).ok_or(ApiError(StatusCode::FORBIDDEN, "path_not_allowed"))?;
    let argv = args(&op, &input.params)?;
    if op == "worktree-add" {
        let target = PathBuf::from(&input.params["path"]);
        let parent = target
            .parent()
            .ok_or(ApiError(StatusCode::BAD_REQUEST, "invalid_parameter"))?;
        if !target.is_absolute() || target.exists() || allowed(&host, parent).is_none() {
            return Err(ApiError(StatusCode::FORBIDDEN, "path_not_allowed"));
        }
    }
    for key in ["branch", "to"] {
        if matches!(
            op.as_str(),
            "worktree-add" | "branch-create" | "branch-rename"
        ) && let Some(name) = input.params.get(key)
            && !ref_ok(name)
        {
            return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_branch_name"));
        }
    }
    if input.input.as_ref().is_some_and(|i| i.len() > MAX_INPUT) {
        return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE, "input_too_large"));
    }
    let mut command = tokio::process::Command::new("git");
    command
        .args(&argv)
        .current_dir(&cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(if input.input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
        command.env_remove(name);
    }
    let mut child = command
        .spawn()
        .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, "git_unavailable"))?;
    if let (Some(text), Some(mut stdin)) = (input.input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes()).await;
    }
    let limit = Duration::from_secs(if op == "push" { 120 } else { 60 });
    let output = tokio::time::timeout(limit, child.wait_with_output())
        .await
        .map_err(|_| ApiError(StatusCode::GATEWAY_TIMEOUT, "git_timeout"))?
        .map_err(super::internal)?;
    let text = |b: &[u8]| {
        let mut t = String::from_utf8_lossy(&b[..b.len().min(MAX_OUTPUT)]).into_owned();
        if b.len() > MAX_OUTPUT {
            t.push_str("\n… output cut by the host\n");
        }
        t
    };
    let mut out = text(&output.stdout);
    if op == "merging" {
        let f = PathBuf::from(out.trim());
        out = if cwd.join(f).exists() {
            "yes".into()
        } else {
            String::new()
        };
    }
    let mut shown: Vec<String> = vec!["git".into()];
    shown.extend(argv.into_iter().skip(4));
    Ok(Json(json!({
        "code": output.status.code().unwrap_or(1),
        "out": out,
        "err": text(&output.stderr),
        "args": shown,
        "readOnly": READ_ONLY.contains(&op.as_str()),
    })))
}

/// Files a task's worktree changed against the commit it started from, including edits made by
/// shell commands and new untracked files: `[{"path","status","diff"}]`. Empty when git fails.
pub(super) fn worktree_changes(folder: &FsPath, base: &str) -> Vec<Value> {
    let git = |a: &[&str]| -> Option<String> {
        let o = std::process::Command::new("git")
            .args(["-c", "core.quotepath=off"])
            .args(a)
            .current_dir(folder)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(Stdio::null())
            .output()
            .ok()?;
        o.status
            .success()
            .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    // Untracked files join the diff through an intent-to-add-free path: list them separately.
    let mut files = vec![];
    if let Some(names) = git(&["diff", "--name-status", base, "--"]) {
        for line in names.lines().take(200) {
            let mut parts = line.splitn(2, '\t');
            let (Some(status), Some(path)) = (parts.next(), parts.next()) else {
                continue;
            };
            let diff = git(&["diff", base, "--", path]).unwrap_or_default();
            files.push(json!({"path":path,"status":&status[..1],"diff":truncate(diff)}));
        }
    }
    if let Some(names) = git(&["ls-files", "--others", "--exclude-standard"]) {
        for path in names.lines().take(200) {
            let text = std::fs::read(folder.join(path))
                .ok()
                .filter(|b| b.len() <= 256 * 1024 && !b.contains(&0))
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            files.push(json!({"path":path,"status":"A","newText":text}));
        }
    }
    files
}
fn truncate(mut s: String) -> String {
    if s.len() > 256 * 1024 {
        let mut cut = 256 * 1024;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("\n… cut by the host\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_cannot_be_injected() {
        let mut p = HashMap::new();
        p.insert("branch".to_string(), "--upload-pack=x".to_string());
        assert!(args("checkout", &p).is_err());
        p.insert("branch".to_string(), "feature/x".to_string());
        assert_eq!(args("checkout", &p).unwrap()[4..], ["switch", "feature/x"]);
        assert!(args("rm", &p).is_err());
        let mut f = HashMap::new();
        f.insert("file".to_string(), "-rf".to_string());
        assert!(args("add", &f).is_err());
    }
}

/// Where a new session starts: on a branch in the shared folder, or in its own worktree.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct SessionBranch {
    /// The branch to work on.
    pub name: String,
    /// Create the branch from this commit or branch (default: the current HEAD).
    #[serde(default)]
    pub base: Option<String>,
    /// Make the branch (otherwise switch to an existing one).
    #[serde(default)]
    pub create: bool,
    /// Give the session its own worktree beside the repository instead of switching the folder.
    #[serde(default)]
    pub worktree: bool,
}

/// Prepares the folder for a session that asked for a branch and returns the folder to start in.
pub(super) async fn prepare_session(
    host: &Host,
    workspace: &FsPath,
    want: &SessionBranch,
) -> ApiResult<PathBuf> {
    let bad = |c| ApiError(StatusCode::BAD_REQUEST, c);
    let folder =
        allowed(host, workspace).ok_or(ApiError(StatusCode::FORBIDDEN, "path_not_allowed"))?;
    if want.name.starts_with('-') || !ref_ok(&want.name) {
        return Err(bad("invalid_branch_name"));
    }
    if want
        .base
        .as_deref()
        .is_some_and(|b| b.is_empty() || b.starts_with('-'))
    {
        return Err(bad("invalid_parameter"));
    }
    let run = |args: Vec<String>, dir: PathBuf| async move {
        let out = tokio::process::Command::new("git")
            .args(&args)
            .current_dir(&dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, "git_unavailable"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            tracing::info!(stderr = %String::from_utf8_lossy(&out.stderr), "session branch setup failed");
            Err(ApiError(StatusCode::CONFLICT, "branch_failed"))
        }
    };
    let top = PathBuf::from(
        run(
            vec!["rev-parse".into(), "--show-toplevel".into()],
            folder.clone(),
        )
        .await
        .map_err(|_| bad("not_a_repository"))?,
    );
    let sub = folder
        .strip_prefix(&top)
        .map(PathBuf::from)
        .unwrap_or_default();
    if !want.worktree {
        let mut a = vec!["switch".to_string()];
        if want.create {
            a.push("-c".into());
        }
        a.push(want.name.clone());
        if want.create
            && let Some(b) = &want.base
        {
            a.push(b.clone());
        }
        run(a, folder.clone()).await?;
        return Ok(folder);
    }
    let slug: String = want
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = top
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let target = top.with_file_name(format!("{name}-{slug}"));
    let parent = target.parent().ok_or(bad("invalid_parameter"))?;
    if target.exists() || allowed(host, parent).is_none() {
        return Err(ApiError(StatusCode::CONFLICT, "worktree_path_unavailable"));
    }
    let mut a: Vec<String> = vec!["worktree".into(), "add".into()];
    if want.create {
        a.extend(["-b".into(), want.name.clone(), target.display().to_string()]);
        if let Some(b) = &want.base {
            a.push(b.clone());
        }
    } else {
        a.extend([target.display().to_string(), want.name.clone()]);
    }
    run(a, top.clone()).await?;
    Ok(target.join(sub))
}
