//! `POST /v1/pull-requests`: open a GitHub pull request with the `gh` the operator installed.
//!
//! The body is written to `gh`'s stdin (`--body-file -`). Title, base and head are separate
//! arguments, never a shell string. `gh`'s own text is not returned: it can repeat a token.
//! Sign-in is `gh`'s exit code 4 (`gh help exit-codes` on 2.94.0). `gh auth status` is not used
//! here; that command contacts GitHub (`GetScopes` in cli/cli `pkg/cmd/auth/status/status.go`).

use super::git::{self, ref_ok};
use super::{ApiError, ApiResult, Host};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

/// Largest pull-request body accepted, in bytes.
pub(super) const MAX_BODY: usize = 64 * 1024;
/// HTTP body cap. A 64 KiB body of escaped JSON is larger than the text itself.
pub(super) const MAX_HTTP_BODY: usize = 512 * 1024;
const MAX_TITLE_CHARS: usize = 256;
const MAX_GH_OUTPUT: usize = 64 * 1024;
const GH_TIMEOUT: Duration = Duration::from_secs(60);

/// How long `gh` may run: 60 seconds. `ACPD_GH_TIMEOUT_MS` may shorten it (100 ms to 60 s, never
/// longer) so the timeout path can be tested without waiting a minute.
fn gh_timeout() -> Duration {
    std::env::var("ACPD_GH_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|ms| Duration::from_millis(ms.clamp(100, 60_000)))
        .unwrap_or(GH_TIMEOUT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct PullRequestInput {
    path: PathBuf,
    base: String,
    head: String,
    title: String,
    body: String,
    #[serde(default)]
    draft: bool,
}

/// Arguments after the `gh` program. The body is not one of them.
pub(super) fn pr_argv(title: &str, base: &str, head: &str, draft: bool) -> Vec<String> {
    let mut args = vec![
        "pr".into(),
        "create".into(),
        "--title".into(),
        title.into(),
        "--body-file".into(),
        "-".into(),
        "--base".into(),
        base.into(),
        "--head".into(),
        head.into(),
    ];
    if draft {
        args.push("--draft".into());
    }
    args
}

/// Split a head into the optional fork owner and the branch. `gh pr create --head` takes `branch` or
/// `owner:branch` for a pull request from a fork. The owner must look like a GitHub login: letters,
/// digits and single hyphens, 1 to 39 characters, not starting or ending with a hyphen, so it can
/// never read as an option. The branch is still checked by git.
pub(super) fn split_head(head: &str) -> Option<(Option<&str>, &str)> {
    let Some((owner, branch)) = head.split_once(':') else {
        return Some((None, head));
    };
    let owner_ok = (1..=39).contains(&owner.len())
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && !owner.contains("--");
    (owner_ok && !branch.contains(':')).then_some((Some(owner), branch))
}

fn title_ok(title: &str) -> bool {
    let trimmed = title.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_TITLE_CHARS
        && !title.chars().any(|c| c.is_control())
}

/// Map `gh`'s exit code and output to a stable code or a pull-request URL.
/// The returned error is a fixed string. Stderr is only inspected, never copied out.
pub(super) fn classify_gh(code: i32, stdout: &str, stderr: &str) -> Result<String, &'static str> {
    if code == 4 {
        return Err("gh_not_signed_in");
    }
    if code == 0 {
        return pull_request_url(stdout).ok_or("gh_failed");
    }
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("already exists") {
        return Err("already_exists");
    }
    // "no git remotes" is `context.BaseRepo` in cli/cli v2.94.0 when the list is empty.
    // "already exists" is `createRun` in pkg/cmd/pr/create/create.go of that tag.
    if lower.contains("no git remotes") || lower.contains("no remote") {
        return Err("no_remote");
    }
    Err("gh_failed")
}

fn pull_request_url(stdout: &str) -> Option<String> {
    stdout.lines().map(str::trim).find_map(|line| {
        let rest = line.strip_prefix("https://")?;
        let (host, path) = rest.split_once('/')?;
        let host_ok = !host.is_empty()
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        let path_ok = path.starts_with(|c: char| c.is_ascii_alphanumeric())
            && !path.chars().any(|c| c.is_whitespace() || c.is_control());
        (host_ok && path_ok && line.len() <= 2048).then(|| line.to_string())
    })
}

fn classify_status(code: &str) -> StatusCode {
    match code {
        "gh_not_signed_in" => StatusCode::CONFLICT,
        "already_exists" | "no_remote" => StatusCode::CONFLICT,
        "gh_timeout" => StatusCode::GATEWAY_TIMEOUT,
        "gh_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_GATEWAY,
    }
}

async fn drain(mut reader: impl AsyncRead + Unpin, max: usize) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = match reader.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if kept.len() < max {
            let room = max - kept.len();
            kept.extend_from_slice(&buf[..n.min(room)]);
        }
    }
    kept
}

/// `409 already_exists` carries the existing pull request's URL when `gh` printed one. The URL passes
/// the same check as a created one; no other text from `gh` is returned.
pub(super) async fn create(
    State(host): State<Host>,
    Json(input): Json<PullRequestInput>,
) -> Response {
    let mut existing = None;
    match create_inner(host, input, &mut existing).await {
        Err(ApiError(status, "already_exists")) if existing.is_some() => (
            status,
            Json(json!({"error": {
                "code": "already_exists",
                "message": format!("A pull request already exists: {}", existing.as_deref().unwrap_or_default()),
                "url": existing,
            }})),
        )
            .into_response(),
        Ok(reply) => reply.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn create_inner(
    host: Host,
    input: PullRequestInput,
    existing: &mut Option<String>,
) -> ApiResult<Json<Value>> {
    let cwd = git::allowed(&host, &input.path)
        .ok_or(ApiError(StatusCode::FORBIDDEN, "path_not_allowed"))?;
    if !title_ok(&input.title) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_parameter"));
    }
    if input.body.len() > MAX_BODY || input.body.contains('\0') {
        return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"));
    }
    // `git check-ref-format` is a blocking child process; keep it off the async workers.
    let Some((_, head_branch)) = split_head(&input.head) else {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_branch_name"));
    };
    let (base, head) = (input.base.clone(), head_branch.to_owned());
    let refs_ok = tokio::task::spawn_blocking(move || ref_ok(&base) && ref_ok(&head))
        .await
        .unwrap_or(false);
    if !refs_ok {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_branch_name"));
    }
    let Some(launch) = crate::discovery::resolve("gh", None) else {
        return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, "gh_unavailable"));
    };
    let mut command = tokio::process::Command::new(&launch.program);
    command
        .args(&launch.prefix_args)
        .args(pr_argv(&input.title, &input.base, &input.head, input.draft))
        .current_dir(&cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = launch.path_with_dirs(std::env::var_os("PATH")) {
        command.env("PATH", path);
    }
    // `gh` runs inside the same containment as agents and terminals, so a `git` or credential
    // helper it started cannot outlive it: the tree is stopped when this request ends, on a
    // timeout as well as after a normal exit.
    let (mut child, tree) = crate::process_tree::ProcessTree::spawn(&mut command)
        .map_err(|_| ApiError(StatusCode::SERVICE_UNAVAILABLE, "gh_unavailable"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.body.as_bytes()).await;
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_task = tokio::spawn(async move {
        match stdout {
            Some(pipe) => drain(pipe, MAX_GH_OUTPUT).await,
            None => Vec::new(),
        }
    });
    let err_task = tokio::spawn(async move {
        match stderr {
            Some(pipe) => drain(pipe, MAX_GH_OUTPUT).await,
            None => Vec::new(),
        }
    });
    let status = match tokio::time::timeout(gh_timeout(), child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(_)) => return Err(ApiError(StatusCode::BAD_GATEWAY, "gh_failed")),
        Err(_) => {
            out_task.abort();
            err_task.abort();
            return Err(ApiError(StatusCode::GATEWAY_TIMEOUT, "gh_timeout"));
        }
    };
    // A helper `gh` started (a credential cache, say) inherits its output pipes. Stop the whole
    // tree now, so the pipes close and reading them cannot wait on that helper.
    drop(tree);
    let grace = Duration::from_secs(2);
    let stdout = String::from_utf8_lossy(
        &tokio::time::timeout(grace, out_task)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default(),
    )
    .into_owned();
    let stderr = String::from_utf8_lossy(
        &tokio::time::timeout(grace, err_task)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default(),
    )
    .into_owned();
    let code = status.code().unwrap_or(1);
    match classify_gh(code, &stdout, &stderr) {
        Ok(url) => Ok(Json(json!({ "url": url }))),
        Err(error) => {
            if error == "already_exists" {
                *existing = pull_request_url(&stderr);
            }
            Err(ApiError(classify_status(error), error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_keep_title_and_branch_as_single_words_and_omit_the_body() {
        let title = "Fix $(whoami); rm -rf /";
        let head = "feature$(whoami)";
        let body = "BODY-MARKER not an argument";
        let args = pr_argv(title, "main", head, true);
        assert_eq!(
            args,
            [
                "pr",
                "create",
                "--title",
                title,
                "--body-file",
                "-",
                "--base",
                "main",
                "--head",
                head,
                "--draft",
            ]
        );
        assert!(
            !args
                .iter()
                .any(|arg| arg.contains(body) || arg.contains('\n'))
        );
        assert!(
            !pr_argv(title, "main", head, false)
                .iter()
                .any(|arg| arg == "--draft")
        );
        assert!(!title_ok(""));
        assert!(!title_ok("  "));
        assert!(!title_ok(&"a".repeat(MAX_TITLE_CHARS + 1)));
        assert!(!title_ok("line\n--body"));
        assert!(title_ok(title));
    }

    #[test]
    fn a_fork_head_is_an_owner_and_a_branch_and_nothing_else() {
        assert_eq!(split_head("feature"), Some((None, "feature")));
        assert_eq!(
            split_head("octo-cat:fix/x"),
            Some((Some("octo-cat"), "fix/x"))
        );
        for bad in [
            ":x",
            "-owner:x",
            "owner-:x",
            "a--b:x",
            "own er:x",
            "owner:a:b",
            "--upload-pack=x:y",
            "$(id):x",
        ] {
            assert!(split_head(bad).is_none(), "{bad}");
        }
        let too_long = format!("{}:x", "a".repeat(40));
        assert!(split_head(&too_long).is_none());
    }

    #[test]
    fn gh_output_maps_to_codes_and_never_returns_stderr() {
        let secret = "ghp_SUPERSECRETTOKENVALUE";
        let err = classify_gh(1, "", &format!("failed token {secret}")).unwrap_err();
        assert_eq!(err, "gh_failed");
        assert!(!err.contains(secret));
        assert_eq!(
            classify_gh(4, "", &format!("token {secret}")).unwrap_err(),
            "gh_not_signed_in"
        );
        assert_eq!(
            classify_gh(
                1,
                "",
                "a pull request for branch \"h\" into branch \"b\" already exists:\nhttps://github.com/o/r/pull/3"
            )
            .unwrap_err(),
            "already_exists"
        );
        assert_eq!(
            classify_gh(1, "", "no git remotes found").unwrap_err(),
            "no_remote"
        );
        assert_eq!(
            classify_gh(0, "https://github.com/example/repo/pull/7\n", secret).unwrap(),
            "https://github.com/example/repo/pull/7"
        );
        assert!(classify_gh(0, secret, "").is_err());
        assert!(classify_gh(0, "http://github.com/example/repo/pull/7", "").is_err());
    }
}
