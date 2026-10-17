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
    "log-dag",
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
            // Against a commit or branch instead of the index (a run's changes from its base).
            if let Some(base) = opt(p, "base")? {
                a.push(base.into());
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
        // The history with each commit's parents, for clients that draw the graph themselves:
        // full hash, parent hashes, refs, subject, author, relative time.
        "log-dag" => s(&[
            "log",
            "--all",
            "--date-order",
            "-n",
            "300",
            "--pretty=format:%H\u{1f}%P\u{1f}%d\u{1f}%s\u{1f}%an\u{1f}%cr",
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
        "worktree-remove" => {
            let mut a: Vec<String> = vec!["worktree".into(), "remove".into()];
            if p.get("force").is_some_and(|v| v == "1") {
                a.push("--force".into());
            }
            a.push(param(p, "path")?.into());
            a
        }
        // Stage everything in the folder, new files included (before committing a run).
        "add-all" => s(&["add", "-A"]),
        // Merge a branch into the current checkout as one merge commit; a conflict is
        // reported by git and left for `merge-abort` or resolution.
        "merge" => vec![
            "merge".into(),
            "--no-ff".into(),
            "--no-edit".into(),
            param(p, "branch")?.into(),
        ],
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
pub(super) fn allowed(host: &Host, path: &FsPath) -> Option<PathBuf> {
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

pub(super) fn ref_ok(name: &str) -> bool {
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CommitMessageInput {
    path: PathBuf,
    agent_id: String,
    /// `commit` (default) or `pullRequest`. Older clients omit it.
    #[serde(default)]
    kind: String,
    /// Base branch for a pull-request suggestion. Ignored for a commit message.
    #[serde(default)]
    base: Option<String>,
}

/// Most of the staged diff given to the agent; the file summary always covers everything.
const MESSAGE_DIFF_BYTES: usize = 24 * 1024;

/// Read-only git for the commit-message prompt; `None` when git fails.
async fn git_text(cwd: &FsPath, args: &[&str]) -> Option<String> {
    let mut command = tokio::process::Command::new("git");
    command
        .args(["-c", "core.quotepath=off", "-c", "color.ui=never"])
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
        command.env_remove(name);
    }
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .ok()?
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The text of the prompt that asks for a commit message (also used by tests).
pub(super) fn commit_message_prompt(subjects: &str, files: &str, diff: &str) -> String {
    let mut diff = diff.to_string();
    if diff.len() > MESSAGE_DIFF_BYTES {
        let mut cut = MESSAGE_DIFF_BYTES;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
        diff.push_str("\n… (diff cut; the file summary above lists every change)\n");
    }
    format!(
        "Write a git commit message for the staged changes below. Match the style of this \
         repository's recent commit subjects (for example an `Area: summary` prefix if they use \
         one). First line: an imperative summary of at most 72 characters. Then, only if it \
         helps, a blank line and a short body wrapped at 72 characters that says what changed \
         and why. Reply with only the message: no code fences, no quotes, no tools.\n\n\
         Recent commit subjects:\n{}\n\nStaged files:\n{}\n\nStaged diff:\n{}",
        if subjects.trim().is_empty() {
            "(none yet)"
        } else {
            subjects.trim_end()
        },
        files.trim_end(),
        diff
    )
}

/// Title line, then an optional body, from the same cleaning as a commit message.
pub(super) fn split_pull_request(text: &str) -> (String, String) {
    let text = clean_commit_message(text);
    let mut lines = text.lines();
    let title: String = lines
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .take(256)
        .collect();
    let body = lines.collect::<Vec<_>>().join("\n");
    let body = body.trim().to_string();
    (title, body)
}

/// Prompt for a pull-request title and body. Same bounds as a commit message.
pub(super) fn pull_request_prompt(subjects: &str, files: &str, diff: &str) -> String {
    let mut diff = diff.to_string();
    if diff.len() > MESSAGE_DIFF_BYTES {
        let mut cut = MESSAGE_DIFF_BYTES;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
        diff.push_str("\n… (diff cut; the file summary above lists every change)\n");
    }
    format!(
        "Write a GitHub pull request description for the commits below. First line: an \
         imperative title of at most 72 characters. Then a blank line and a short body wrapped \
         at 72 characters that says what changed and why. Reply with only that text: no code \
         fences, no quotes, no tools.\n\n\
         Commit subjects since the base:\n{}\n\nChanged files:\n{}\n\nDiff:\n{}",
        if subjects.trim().is_empty() {
            "(none yet)"
        } else {
            subjects.trim_end()
        },
        if files.trim().is_empty() {
            "(none)"
        } else {
            files.trim_end()
        },
        diff
    )
}

/// The agent's reply as a commit message: code fences and surrounding quotes removed.
pub(super) fn clean_commit_message(text: &str) -> String {
    let mut text = text.trim();
    if let Some(rest) = text.strip_prefix("```") {
        text = rest.split_once('\n').map_or("", |(_, body)| body);
        text = text.trim_end().strip_suffix("```").unwrap_or(text);
    }
    let text = text.trim();
    let text = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    text.trim().to_string()
}

/// `POST /v1/commit-message` `{"path", "agentId"}`: a commit message for what is staged in
/// `path`, written by that agent in a throwaway session (no file, terminal or MCP access; every
/// permission request is refused; no client sees it). One model request. The host reads the
/// staged diff and recent subjects itself, so nothing large comes from the client.
pub(super) async fn commit_message(
    State(host): State<Host>,
    Json(input): Json<CommitMessageInput>,
) -> ApiResult<Json<Value>> {
    if input.kind == "pullRequest" {
        return pull_request_message(host, input).await;
    }
    if !input.kind.is_empty() && input.kind != "commit" {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_parameter"));
    }
    let cwd =
        allowed(&host, &input.path).ok_or(ApiError(StatusCode::FORBIDDEN, "path_not_allowed"))?;
    let files = git_text(&cwd, &["diff", "--cached", "--stat=200", "--no-ext-diff"])
        .await
        .ok_or(ApiError(StatusCode::CONFLICT, "not_a_repository"))?;
    if files.trim().is_empty() {
        return Err(ApiError(StatusCode::CONFLICT, "nothing_staged"));
    }
    let diff = git_text(&cwd, &["diff", "--cached", "-U3", "--no-ext-diff"])
        .await
        .unwrap_or_default();
    let subjects = git_text(&cwd, &["log", "-n", "10", "--pretty=format:%s"])
        .await
        .unwrap_or_default();
    let access = crate::connection::WorkspaceAccess {
        read_files: false,
        write_files: false,
        terminal: false,
        form_elicitation: false,
        manual_approval: false,
        boolean_options: false,
    };
    let session = host
        .sessions
        .open_configured(&input.agent_id, &cwd, None, Vec::new(), access)
        .await
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "agent_unavailable"))?;
    let id = session.metadata().id;
    let refuser = crate::doctor::refuse_permissions(session.clone());
    let deadline = Duration::from_secs(host.config.runtime.request_timeout_seconds.max(30));
    let prompt = commit_message_prompt(&subjects, &files, &diff);
    let result = tokio::time::timeout(deadline, session.prompt(&prompt)).await;
    refuser.abort();
    let text: String = session
        .connection
        .replay(0)
        .events
        .iter()
        .filter(|event| event.message["params"]["update"]["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|event| {
            event.message["params"]["update"]["content"]["text"]
                .as_str()
                .map(str::to_owned)
        })
        .collect();
    drop(session);
    let _ = host.sessions.remove(id).await;
    let message = clean_commit_message(&text);
    match result {
        Err(_) => Err(ApiError(StatusCode::GATEWAY_TIMEOUT, "agent_timeout")),
        Ok(Err(_)) => Err(ApiError(StatusCode::BAD_GATEWAY, "agent_failed")),
        Ok(Ok(_)) if message.is_empty() => Err(ApiError(StatusCode::BAD_GATEWAY, "empty_reply")),
        Ok(Ok(_)) => Ok(Json(json!({ "message": message }))),
    }
}

/// Same throwaway session as a commit message, describing commits since `base` instead of the index.
async fn pull_request_message(host: Host, input: CommitMessageInput) -> ApiResult<Json<Value>> {
    let cwd =
        allowed(&host, &input.path).ok_or(ApiError(StatusCode::FORBIDDEN, "path_not_allowed"))?;
    let base = input
        .base
        .as_deref()
        .filter(|name| ref_ok(name))
        .ok_or(ApiError(StatusCode::BAD_REQUEST, "invalid_branch_name"))?;
    let subjects = git_text(
        &cwd,
        &["log", &format!("{base}..HEAD"), "--pretty=format:%s"],
    )
    .await
    .ok_or(ApiError(StatusCode::CONFLICT, "base_not_found"))?;
    let files = git_text(
        &cwd,
        &[
            "diff",
            "--stat=200",
            "--no-ext-diff",
            &format!("{base}...HEAD"),
        ],
    )
    .await
    .unwrap_or_default();
    let diff = git_text(
        &cwd,
        &["diff", "-U3", "--no-ext-diff", &format!("{base}...HEAD")],
    )
    .await
    .unwrap_or_default();
    if subjects.trim().is_empty() && files.trim().is_empty() && diff.trim().is_empty() {
        return Err(ApiError(StatusCode::CONFLICT, "nothing_to_describe"));
    }
    let access = crate::connection::WorkspaceAccess {
        read_files: false,
        write_files: false,
        terminal: false,
        form_elicitation: false,
        manual_approval: false,
        boolean_options: false,
    };
    let session = host
        .sessions
        .open_configured(&input.agent_id, &cwd, None, Vec::new(), access)
        .await
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "agent_unavailable"))?;
    let id = session.metadata().id;
    let refuser = crate::doctor::refuse_permissions(session.clone());
    let deadline = Duration::from_secs(host.config.runtime.request_timeout_seconds.max(30));
    let prompt = pull_request_prompt(&subjects, &files, &diff);
    let result = tokio::time::timeout(deadline, session.prompt(&prompt)).await;
    refuser.abort();
    let text: String = session
        .connection
        .replay(0)
        .events
        .iter()
        .filter(|event| event.message["params"]["update"]["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|event| {
            event.message["params"]["update"]["content"]["text"]
                .as_str()
                .map(str::to_owned)
        })
        .collect();
    drop(session);
    let _ = host.sessions.remove(id).await;
    let (title, body) = split_pull_request(&text);
    match result {
        Err(_) => Err(ApiError(StatusCode::GATEWAY_TIMEOUT, "agent_timeout")),
        Ok(Err(_)) => Err(ApiError(StatusCode::BAD_GATEWAY, "agent_failed")),
        Ok(Ok(_)) if title.is_empty() => Err(ApiError(StatusCode::BAD_GATEWAY, "empty_reply")),
        Ok(Ok(_)) => Ok(Json(json!({ "title": title, "body": body }))),
    }
}

/// Files a task's worktree changed against the commit it started from, including edits made by
/// shell commands and new untracked files: `[{"path","status","diff"}]`. Empty when git fails.
/// Files changed from `base` to the working tree, or to `branch` when one is given (the worktree
/// is gone and only its branch is left).
pub(super) fn worktree_changes(folder: &FsPath, base: &str, branch: Option<&str>) -> Vec<Value> {
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
    let mut range = vec![base];
    range.extend(branch);
    let mut names_args = vec!["diff", "--name-status"];
    names_args.extend(&range);
    names_args.push("--");
    if let Some(names) = git(&names_args) {
        for line in names.lines().take(200) {
            let mut parts = line.splitn(2, '\t');
            let (Some(status), Some(path)) = (parts.next(), parts.next()) else {
                continue;
            };
            let mut a = vec!["diff"];
            a.extend(&range);
            a.extend(["--", path]);
            let diff = git(&a).unwrap_or_default();
            files.push(json!({"path":path,"status":&status[..1],"diff":truncate(diff)}));
        }
    }
    if branch.is_some() {
        return files;
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
    fn commit_message_prompt_shows_style_files_and_a_bounded_diff() {
        let prompt = commit_message_prompt(
            "Desktop: keep the bubble
Host: fix x
",
            " a.rs | 2 +
",
            "+line
",
        );
        assert!(
            prompt.contains("Desktop: keep the bubble")
                && prompt.contains("a.rs | 2 +")
                && prompt.contains("+line")
        );
        assert!(prompt.contains("at most 72 characters"));
        assert!(commit_message_prompt("", "f", "d").contains("(none yet)"));
        let big = "é".repeat(MESSAGE_DIFF_BYTES);
        let cut = commit_message_prompt("s", "f", &big);
        assert!(cut.len() < MESSAGE_DIFF_BYTES + 2048 && cut.contains("diff cut"));
    }
    #[test]
    fn the_reply_is_cleaned_of_fences_and_quotes() {
        assert_eq!(
            clean_commit_message(
                "  Host: fix the hang

Body.
 "
            ),
            "Host: fix the hang

Body."
        );
        assert_eq!(
            clean_commit_message(
                "```text
Host: fix it
```"
            ),
            "Host: fix it"
        );
        assert_eq!(
            clean_commit_message(
                "```
A

B
```"
            ),
            "A

B"
        );
        assert_eq!(clean_commit_message("\"Fix it\""), "Fix it");
        assert_eq!(clean_commit_message("```"), "");
    }

    #[test]
    fn pull_request_prompt_uses_subjects_since_base_and_splits_the_reply() {
        let prompt = pull_request_prompt("Fix the hang\n", " a.rs | 2 +\n", "+line\n");
        assert!(prompt.contains("Fix the hang"));
        assert!(prompt.contains("a.rs | 2 +"));
        assert!(prompt.contains("+line"));
        assert!(prompt.contains("at most 72 characters"));
        let big = "é".repeat(MESSAGE_DIFF_BYTES);
        assert!(pull_request_prompt("s", "f", &big).contains("diff cut"));
        assert_eq!(
            split_pull_request("```\nFix it\n\nBecause.\n```"),
            ("Fix it".into(), "Because.".into())
        );
        assert_eq!(split_pull_request("\"Only a title\"").0, "Only a title");
    }

    #[test]
    fn run_operations_build_safe_arguments() {
        let p = |v: &[(&str, &str)]| {
            v.iter()
                .map(|(k, x)| (k.to_string(), x.to_string()))
                .collect::<HashMap<_, _>>()
        };
        let tail = |op: &str, v: &[(&str, &str)]| args(op, &p(v)).map(|a| a[4..].to_vec());
        assert_eq!(
            tail("merge", &[("branch", "run/a")]).unwrap(),
            ["merge", "--no-ff", "--no-edit", "run/a"]
        );
        assert!(tail("merge", &[("branch", "--abort")]).is_err());
        assert_eq!(tail("add-all", &[]).unwrap(), ["add", "-A"]);
        assert_eq!(
            tail("worktree-remove", &[("path", "/w/x"), ("force", "1")]).unwrap(),
            ["worktree", "remove", "--force", "/w/x"]
        );
        assert_eq!(
            tail("diff", &[("base", "main"), ("staged", "1")]).unwrap(),
            ["diff", "-U3", "--no-ext-diff", "--cached", "main", "--"]
        );
        assert!(tail("diff", &[("base", "-p")]).is_err());
    }

    #[test]
    fn changes_come_from_the_branch_once_the_worktree_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        let g = |a: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(a)
                .current_dir(r)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "{a:?}");
        };
        g(&["init", "-q", "-b", "main"]);
        std::fs::write(r.join("a"), "1\n").unwrap();
        g(&["add", "a"]);
        g(&["commit", "-qm", "one"]);
        g(&["switch", "-qc", "task"]);
        std::fs::write(r.join("b"), "hi\n").unwrap();
        g(&["add", "b"]);
        g(&["commit", "-qm", "two"]);
        g(&["switch", "-q", "main"]);
        std::fs::write(r.join("loose"), "x").unwrap();
        let files = worktree_changes(r, "main", Some("task"));
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0]["path"], "b");
        assert!(files[0]["diff"].as_str().unwrap().contains("+hi"));
        assert_eq!(worktree_changes(r, "main", None).len(), 1, "untracked only");
    }
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
