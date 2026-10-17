//! `POST /v1/pull-requests` against a fake `gh`.
//!
//! Windows only starts `.exe` / `.com` (`registry::executable`), so this binary copies itself to
//! `gh.exe` and, when that copy is launched with `ACPD_FAKE_GH`, exits as `gh`. The test process
//! keeps its own name, so the flag does not re-enter it.

use acpd::{api::Host, config::Config, registry::Registry, security::Role};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const SECRET: &str = "ghp_SUPERSECRETTOKENVALUE";
const BODY_MARK: &str = "BODY-MARKER-9f3";

fn main() {
    if let Some(path) = std::env::var_os("ACPD_FAKE_SLEEP_PID") {
        // A helper that `gh` started and left running.
        let _ = fs::write(path, std::process::id().to_string());
        std::thread::sleep(Duration::from_secs(120));
        std::process::exit(0);
    }
    if fake_gh_process() {
        fake_gh();
    }
    let scratch = tempfile::tempdir().expect("scratch");
    let log = scratch.path().join("gh-log.json");
    let mode_file = scratch.path().join("mode.txt");
    fs::write(&mode_file, "success").unwrap();
    install_fake_gh(scratch.path(), &log, &mode_file);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(suite(log, mode_file));
}

fn fake_gh_process() -> bool {
    std::env::var_os("ACPD_FAKE_GH").is_some()
        && std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_name().map(|name| {
                    name.to_string_lossy().eq_ignore_ascii_case("gh.exe")
                        || name.to_string_lossy().eq_ignore_ascii_case("gh")
                })
            })
            .unwrap_or(false)
}

fn fake_gh() -> ! {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    if let Ok(path) = std::env::var("FAKE_GH_LOG") {
        let payload = json!({
            "args": args,
            "stdin": stdin,
            "prompt": std::env::var("GIT_TERMINAL_PROMPT").unwrap_or_default(),
        });
        let _ = fs::write(path, payload.to_string());
    }
    let mode = std::env::var("FAKE_GH_MODE_FILE")
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_default();
    match mode.trim() {
        "auth" => std::process::exit(4),
        "exists" => {
            let _ = writeln!(
                std::io::stderr(),
                "a pull request for branch \"h\" into branch \"b\" already exists:\nhttps://github.com/example/repo/pull/3"
            );
            std::process::exit(1);
        }
        "noremote" => {
            let _ = writeln!(std::io::stderr(), "no git remotes");
            std::process::exit(1);
        }
        "leak" => {
            let _ = writeln!(std::io::stderr(), "token {SECRET}");
            std::process::exit(1);
        }
        "leftover" => {
            let pid_file = std::env::var("FAKE_GH_CHILD_PID").unwrap_or_default();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child.env("ACPD_FAKE_SLEEP_PID", pid_file);
            child.env_remove("ACPD_FAKE_GH");
            let _ = child
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            // Give it time to write its pid before gh exits.
            std::thread::sleep(Duration::from_millis(800));
            println!("https://github.com/example/repo/pull/9");
            std::process::exit(0);
        }
        "http" => {
            println!("http://github.com/example/repo/pull/7");
            std::process::exit(0);
        }
        "secretout" => {
            println!("{SECRET}");
            std::process::exit(0);
        }
        _ => {
            println!("https://github.com/example/repo/pull/7");
            std::process::exit(0);
        }
    }
}

fn install_fake_gh(dir: &Path, log: &Path, mode_file: &Path) {
    let src = std::env::current_exe().expect("current exe");
    let dest = dir.join(if cfg!(windows) { "gh.exe" } else { "gh" });
    fs::copy(&src, &dest).expect("copy fake gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).unwrap();
    }
    let mut paths =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
    paths.insert(0, dir.to_path_buf());
    // SAFETY: main thread, before the runtime starts. Nothing else reads these names.
    unsafe {
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        std::env::set_var("ACPD_FAKE_GH", "1");
        std::env::set_var("FAKE_GH_LOG", log);
        std::env::set_var("FAKE_GH_MODE_FILE", mode_file);
    }
}

struct H {
    addr: String,
    token: String,
    client: reqwest::Client,
    root: PathBuf,
    log: PathBuf,
    mode_file: PathBuf,
    _dir: tempfile::TempDir,
}

impl H {
    async fn start(log: PathBuf, mode_file: PathBuf) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projects");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let repo = root.join("calc");
        fs::create_dir_all(&repo).unwrap();
        let config = Config {
            workspace_roots: vec![root.clone()],
            state_directory: dir.path().join("state"),
            ..Default::default()
        };
        let host = Host::new(config, Registry::parse("[]").unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let router = host.router();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let code = host.security.new_pairing_code_for(Role::Phone).unwrap();
        let reply: Value = client
            .post(format!("http://{addr}/v1/pair"))
            .json(&json!({"code": code, "deviceName": "pr-test"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        H {
            addr,
            token: reply["token"].as_str().unwrap().into(),
            client,
            root,
            log,
            mode_file,
            _dir: dir,
        }
    }

    fn repo(&self) -> PathBuf {
        self.root.join("calc")
    }

    fn mode(&self, mode: &str) {
        fs::write(&self.mode_file, mode).unwrap();
        let _ = fs::remove_file(&self.log);
    }

    async fn post(&self, body: Value) -> (u16, String) {
        let response = self
            .client
            .post(format!("http://{}/v1/pull-requests", self.addr))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, text)
    }

    fn opened(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(&self.log).expect("gh was not started")).unwrap()
    }
}

fn pr(path: &Path, extra: Value) -> Value {
    let mut body = json!({
        "path": path,
        "base": "main",
        "head": "feature/x",
        "title": "Fix the calculator",
        "body": "because the add test failed",
        "draft": false,
    });
    if let (Some(map), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            map.insert(key.clone(), value.clone());
        }
    }
    body
}

async fn suite(log: PathBuf, mode_file: PathBuf) {
    let h = H::start(log, mode_file).await;
    let repo = h.repo();

    let status: Value = h
        .client
        .get(format!("http://{}/v1/status", h.addr))
        .bearer_auth(&h.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        status["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "pullRequests"),
        "{status}"
    );
    assert_eq!(status["gh"], "found", "{status}");
    let gh_dir = h.log.parent().unwrap().to_string_lossy();
    assert!(
        !status.to_string().contains(gh_dir.as_ref()),
        "status must not include the gh path"
    );

    let bare = h
        .client
        .post(format!("http://{}/v1/pull-requests", h.addr))
        .json(&pr(&repo, json!({})))
        .send()
        .await
        .unwrap();
    assert_eq!(bare.status(), 401);

    h.mode("success");
    let (status, text) = h.post(pr(&h.root.join("nope"), json!({}))).await;
    assert_eq!(status, 403, "{text}");
    assert!(text.contains("path_not_allowed"), "{text}");
    assert!(!h.log.exists(), "rejected path must not start gh");

    h.mode("success");
    let (status, text) = h.post(pr(&repo, json!({"head": "--upload-pack=x"}))).await;
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("invalid_branch_name"), "{text}");
    assert!(!h.log.exists(), "a branch flag must not reach gh");

    for bad in [
        "-owner:feature",
        "own er:feature",
        "a--b:feature",
        "owner:a:b",
        ":feature",
        "owner:--upload-pack=x",
    ] {
        let (status, text) = h.post(pr(&repo, json!({"head": bad}))).await;
        assert_eq!(status, 400, "{bad}: {text}");
        assert!(!h.log.exists(), "{bad} reached gh");
    }

    // A pull request from a fork passes `owner:branch` to gh as one argument.
    let (status, text) = h.post(pr(&repo, json!({"head": "octo-cat:fix/x"}))).await;
    assert_eq!(status, 200, "{text}");
    let logged: Value = serde_json::from_str(&fs::read_to_string(&h.log).unwrap()).unwrap();
    let args = logged["args"].as_array().unwrap();
    let at = args.iter().position(|a| a == "--head").unwrap();
    assert_eq!(args[at + 1], "octo-cat:fix/x");
    fs::remove_file(&h.log).unwrap();

    let (status, text) = h.post(pr(&repo, json!({"title": ""}))).await;
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("invalid_parameter"), "{text}");
    let (status, text) = h.post(pr(&repo, json!({"title": "line\n--body"}))).await;
    assert_eq!(status, 400, "{text}");
    let (status, text) = h.post(pr(&repo, json!({"title": "t".repeat(257)}))).await;
    assert_eq!(status, 400, "{text}");
    assert!(!h.log.exists(), "a bad title must not start gh");

    let (status, text) = h
        .post(pr(&repo, json!({"body": "b".repeat(64 * 1024 + 1)})))
        .await;
    assert_eq!(status, 413, "{text}");
    assert!(
        text.contains("body_too_large"),
        "route body limit: {status} {text}"
    );
    assert!(!h.log.exists());

    h.mode("success");
    let wide = "w".repeat(20 * 1024);
    let (status, text) = h.post(pr(&repo, json!({"body": wide}))).await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(h.opened()["stdin"], wide);

    h.mode("success");
    let title = "Fix $(whoami); rm -rf /";
    let head = "feature$(whoami)";
    let (status, text) = h
        .post(pr(
            &repo,
            json!({"title": title, "head": head, "body": BODY_MARK, "draft": true}),
        ))
        .await;
    assert_eq!(status, 200, "{text}");
    let opened = h.opened();
    assert_eq!(
        opened["args"],
        json!([
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
            "--draft"
        ])
    );
    assert_eq!(opened["stdin"], BODY_MARK);
    assert_eq!(opened["prompt"], "0");
    assert!(
        !opened["args"].to_string().contains(BODY_MARK),
        "body leaked into argv: {opened}"
    );
    let reply: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(reply["url"], "https://github.com/example/repo/pull/7");
    assert!(!text.contains(SECRET));
    assert!(!text.contains(BODY_MARK));

    h.mode("auth");
    let (status, text) = h.post(pr(&repo, json!({}))).await;
    assert_eq!(status, 409, "{text}");
    assert!(text.contains("gh_not_signed_in"), "{text}");
    assert!(!text.contains(SECRET));

    h.mode("exists");
    let (status, text) = h.post(pr(&repo, json!({}))).await;
    assert_eq!(status, 409, "{text}");
    assert!(text.contains("already_exists"), "{text}");
    // The address of the existing pull request is returned, and only that line of stderr.
    let reply: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        reply["error"]["url"], "https://github.com/example/repo/pull/3",
        "{text}"
    );
    assert!(!text.contains("a pull request for branch"), "{text}");
    assert!(!text.contains(SECRET), "{text}");

    // A helper that gh started and left running is stopped with it.
    let child_pid_file = h.root.join("child.pid");
    unsafe { std::env::set_var("FAKE_GH_CHILD_PID", &child_pid_file) };
    h.mode("leftover");
    let (status, text) = h.post(pr(&repo, json!({}))).await;
    assert_eq!(status, 200, "{text}");
    let child_pid: u32 = fs::read_to_string(&child_pid_file)
        .expect("the helper wrote its pid")
        .trim()
        .parse()
        .unwrap();
    let mut alive = true;
    for _ in 0..40 {
        alive = process_alive(child_pid);
        if !alive {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!alive, "a process gh started outlived the request");

    h.mode("noremote");
    let (status, text) = h.post(pr(&repo, json!({}))).await;
    assert_eq!(status, 409, "{text}");
    assert!(text.contains("no_remote"), "{text}");

    h.mode("leak");
    let (status, text) = h.post(pr(&repo, json!({}))).await;
    assert_eq!(status, 502, "{text}");
    assert!(text.contains("gh_failed"), "{text}");
    assert!(
        !text.contains(SECRET),
        "gh stderr was returned to the client: {text}"
    );

    h.mode("http");
    let (status, text) = h.post(pr(&repo, json!({}))).await;
    assert_eq!(status, 502, "{text}");
    assert!(!text.contains("http://"));

    h.mode("secretout");
    let (status, text) = h.post(pr(&repo, json!({}))).await;
    assert_eq!(status, 502, "{text}");
    assert!(!text.contains(SECRET), "{text}");
}

/// Whether a process with this id is still running.
fn process_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .expect("tasklist");
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
