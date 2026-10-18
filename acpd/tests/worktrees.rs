//! Phase 4: delegated tasks in their own git worktrees, with real git repositories in temporary
//! folders, the mock agent and the control channel.
use acpd::{
    config::{Config, DefaultIsolation, OrchestratorConfig, RuntimeConfig},
    connection::{AcpConnection, WorkspaceAccess},
    orchestrator::{
        Orchestrator,
        control::{self, ControlBackend, Endpoint, Stream},
        delegate::{Backend, ToolFailure},
        records::{self, RecordRights, TaskRecord},
        worktree::{self, Outcome},
    },
    registry::Registry,
    session::{AcpSession, SessionManager},
};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};
use tempfile::TempDir;
use tokio::time::timeout;
use uuid::Uuid;

type Agent = ControlBackend<Box<dyn Stream>>;
struct Fixture {
    manager: Arc<SessionManager>,
    orchestrator: Arc<Orchestrator>,
    _endpoint: Endpoint,
    base: TempDir,
}
impl Fixture {
    fn repo(&self) -> PathBuf {
        self.base.path().join("work/repo").canonicalize().unwrap()
    }
    fn plain(&self) -> PathBuf {
        self.base.path().join("work/plain").canonicalize().unwrap()
    }
    fn worktrees(&self) -> PathBuf {
        self.base.path().join("state/worktrees")
    }
    async fn root(&self, folder: &Path) -> Arc<AcpSession> {
        let all = WorkspaceAccess {
            read_files: true,
            write_files: true,
            terminal: true,
            form_elicitation: false,
            manual_approval: false,
            boolean_options: false,
            yolo: false,
        };
        self.manager
            .open_configured("test-agent", folder, None, vec![], all)
            .await
            .unwrap()
    }
    async fn session_of_task(&self, agent: &Agent, task: &str) -> Arc<AcpSession> {
        let path = eventually_named("task worktree path", || async {
            let status = ok(agent, "task_status", json!({"task_id": task})).await;
            status["tasks"][0]["worktree"]["path"]
                .as_str()
                .map(PathBuf::from)
        })
        .await;
        eventually_named("child session in the worktree", || async {
            for metadata in self.manager.list().await {
                if plain(metadata.workspace.clone()) == plain(path.clone()) {
                    return self.manager.get(metadata.id).await.ok();
                }
            }
            None
        })
        .await
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?} {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn fixture(tweak: impl FnOnce(&mut Config)) -> Fixture {
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("work/repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(base.path().join("work/plain")).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    // Windows runners default to core.autocrlf=true, which rewrites "one\n" on checkout.
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("src/lib.rs"), "original\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let state = base.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    let mut config = Config {
        workspace_roots: vec![base.path().join("work")],
        state_directory: state.clone(),
        runtime: RuntimeConfig {
            request_timeout_seconds: 10,
            max_sessions: 8,
            ..Default::default()
        },
        orchestrator: OrchestratorConfig {
            enabled: true,
            orchestrate: vec!["test-agent".into()],
            delegable: vec!["worker".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    tweak(&mut config);
    let agents: Vec<Value> = ["test-agent", "worker"]
        .iter()
        .map(|id| json!({"id":id,"name":id,"command":env!("CARGO_BIN_EXE_mock-acp-agent"),"args":["--echo-mcp"]}))
        .collect();
    let registry = Registry::parse(&serde_json::to_string(&agents).unwrap()).unwrap();
    let manager = Arc::new(SessionManager::new(config, registry).unwrap());
    let orchestrator =
        Orchestrator::with_cancel_grace(Arc::downgrade(&manager), Duration::from_millis(500));
    manager.attach_orchestrator(orchestrator.clone());
    let endpoint = control::start(orchestrator.clone(), &state).unwrap();
    Fixture {
        manager,
        orchestrator,
        _endpoint: endpoint,
        base,
    }
}

fn env_of(definition: &Value, name: &str) -> String {
    definition["env"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap()["value"]
        .as_str()
        .unwrap()
        .to_owned()
}
async fn agent_of(session: &AcpSession) -> Agent {
    let definition = session.metadata().setup["_meta"]["mockMcpFull"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["name"] == "acpd-orchestrator").cloned())
        .expect("delegation tools");
    let stream = control::connect(Path::new(&env_of(&definition, "ACPD_CONTROL")))
        .await
        .unwrap();
    ControlBackend::new(stream, env_of(&definition, "ACPD_TASK_TOKEN"))
}
async fn ok(agent: &Agent, tool: &str, arguments: Value) -> Value {
    match agent.call(tool, arguments).await {
        Ok(value) => value,
        Err(failure) => panic!("{tool} failed: {} {}", failure.code, failure.message),
    }
}
async fn refused(agent: &Agent, tool: &str, arguments: Value) -> ToolFailure {
    match agent.call(tool, arguments).await {
        Ok(value) => panic!("{tool} unexpectedly succeeded: {value}"),
        Err(failure) => failure,
    }
}
async fn eventually<T, F, Fut>(check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    eventually_named("condition", check).await
}
async fn eventually_named<T, F, Fut>(what: &str, mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    // Windows waits for an agent process to let go of a folder before git can remove it, and the
    // hosted runners are slow: the host itself retries for two minutes, so a short wait here only
    // makes the test flaky (it failed on a loaded runner in about half of the runs).
    let wait = if cfg!(windows) { 90 } else { 20 };
    timeout(Duration::from_secs(wait), async {
        loop {
            if let Some(value) = check().await {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} not reached in time"))
}
async fn status(agent: &Agent, task: &str) -> Value {
    ok(agent, "task_status", json!({"task_id": task})).await["tasks"][0].clone()
}
async fn reaches(agent: &Agent, task: &str, wanted: &str) {
    eventually(|| async { (status(agent, task).await["state"] == wanted).then_some(()) }).await;
}
async fn outcome(agent: &Agent, task: &str, wanted: &str) -> Value {
    let found = timeout(Duration::from_secs(20), async {
        loop {
            let view = status(agent, task).await;
            if view["worktree"]["outcome"] == wanted {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;
    match found {
        Ok(view) => view,
        Err(_) => panic!(
            "outcome {wanted} not reached; last view {}",
            status(agent, task).await
        ),
    }
}
/// Windows cannot delete a folder an agent process still runs in, and a cancelled task's mock
/// agent keeps running; end its session so the host's removal retry can finish.
async fn end_on_windows(fx: &Fixture, child: &AcpSession) {
    if cfg!(windows) {
        let _ = fx.manager.remove(child.metadata().id).await;
    }
}
/// The worktree folder as the host reports it (no `\\?\` prefix on Windows).
fn plain(path: PathBuf) -> PathBuf {
    let path = path.canonicalize().unwrap_or(path);
    let text = path.to_string_lossy().replace('/', "\\");
    let text = if cfg!(windows) {
        text
    } else {
        path.to_string_lossy().into_owned()
    };
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned())
}
/// Act as the phone: answer the child's pending request from the given source.
async fn answer(connection: &AcpConnection, host_write: bool, option: &str) {
    let permission = eventually_named("pending permission", || async {
        connection
            .replay(0)
            .pending_permissions
            .into_iter()
            .find(|p| (p["params"]["_meta"]["acpdSource"] == "host-filesystem") == host_write)
    })
    .await;
    connection
        .permission(
            permission["id"].clone(),
            json!({"outcome":"selected","optionId":option}),
        )
        .await
        .unwrap();
}
/// The child asks the host to write a file (as an agent's `fs/write_text_file`).
fn write(
    session: &AcpSession,
    path: &Path,
    content: &str,
) -> tokio::task::JoinHandle<anyhow::Result<Value>> {
    let connection = session.connection.clone();
    let params =
        json!({"sessionId": session.metadata().acp_session_id, "path": path, "content": content});
    tokio::spawn(async move { connection.request("_mock/write_file", params).await })
}
async fn delegate(agent: &Agent, extra: Value) -> String {
    let mut args = json!({"agent":"worker","prompt":"edit","rights":{"read":true,"write":true}});
    args.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    ok(agent, "task_delegate", args).await["task_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn two_workers_get_separate_worktrees_cannot_touch_each_other_and_are_cleaned_up() {
    let fx = fixture(|_| {});
    let repo = fx.repo();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    let a = delegate(&sup, json!({"isolation":"worktree","workspace":"src"})).await;
    let b = delegate(&sup, json!({"isolation":"worktree"})).await;
    let (va, vb) = (status(&sup, &a).await, status(&sup, &b).await);
    let (wa, wb) = (&va["worktree"], &vb["worktree"]);
    assert_eq!(wa["outcome"], "active");
    assert_ne!(wa["branch"], wb["branch"]);
    assert!(wa["branch"].as_str().unwrap().starts_with("acpd/task-"));
    assert_eq!(wa["base"], git(&repo, &["rev-parse", "HEAD"]));
    assert_eq!(wa["base_dirty"], false);
    let (pa, pb) = (
        PathBuf::from(wa["path"].as_str().unwrap()),
        PathBuf::from(wb["path"].as_str().unwrap()),
    );
    let top_a = pa.parent().unwrap().to_path_buf();
    assert!(
        pa.ends_with("src"),
        "the task folder keeps its place in the tree"
    );
    assert!(top_a.starts_with(plain(fx.worktrees())));
    assert!(pb.starts_with(plain(fx.worktrees())));
    assert!(!pa.starts_with(&repo) && !pb.starts_with(&repo));

    let sa = fx.session_of_task(&sup, &a).await;
    let sb = fx.session_of_task(&sup, &b).await;
    assert_eq!(plain(sa.metadata().workspace.clone()), plain(pa.clone()));
    assert_eq!(plain(sb.metadata().workspace.clone()), plain(pb.clone()));

    // A writes inside its own worktree, approved by the phone.
    let own = write(&sa, &pa.join("lib.rs"), "from a\n");
    answer(&sa.connection, true, "acpd-write-allow").await;
    own.await.unwrap().unwrap();
    // Cross-edits: into B's worktree, and into the parent's checkout. Both are outside A's
    // folder, so the host refuses them for a delegated child without asking anyone.
    for target in [pb.join("src/lib.rs"), repo.join("src/lib.rs")] {
        let crossed = write(&sa, &target, "from a\n").await.unwrap();
        assert!(crossed.is_err(), "{target:?}");
    }
    assert_eq!(
        std::fs::read_to_string(pb.join("src/lib.rs")).unwrap(),
        "original\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        "original\n"
    );
    assert_eq!(
        std::fs::read_to_string(pa.join("lib.rs")).unwrap(),
        "from a\n"
    );
    // B writes in its own; A's worktree does not see it.
    let own = write(&sb, &pb.join("notes.txt"), "from b\n");
    answer(&sb.connection, true, "acpd-write-allow").await;
    own.await.unwrap().unwrap();
    assert!(!top_a.join("notes.txt").exists() && !repo.join("notes.txt").exists());

    // A finishes; its worktree stays for a follow-up.
    answer(&sa.connection, false, "allow").await;
    reaches(&sup, &a, "completed").await;
    assert_eq!(status(&sup, &a).await["worktree"]["outcome"], "active");
    // A commits its work (as an agent would with its terminal).
    git(&top_a, &["commit", "-q", "-am", "a's change"]);

    // B is cancelled with uncommitted work: it is committed to B's branch (a snapshot) and the
    // worktree folder goes. Nothing is lost and nothing reaches the parent's checkout.
    ok(&sup, "task_cancel", json!({"task_id": b})).await;
    reaches(&sup, &b, "canceled").await;
    end_on_windows(&fx, &sb).await;
    outcome(&sup, &b, "snapshotted").await;
    assert!(!pb.exists());
    let branch_b = wb["branch"].as_str().unwrap();
    assert_eq!(
        git(&repo, &["show", &format!("{branch_b}:notes.txt")]),
        "from b"
    );
    assert_eq!(git(&repo, &["log", "-1", "--format=%an", branch_b]), "acpd");
    assert!(!repo.join("notes.txt").exists());

    // Ending the root session ends A: the worktree goes, the branch with the commit stays.
    let branch_a = wa["branch"].as_str().unwrap().to_owned();
    fx.manager.remove(root.metadata().id).await.unwrap();
    eventually_named("worktree A removed", || async {
        (!top_a.exists()).then_some(())
    })
    .await;
    assert_eq!(
        git(&repo, &["show", &format!("{branch_a}:src/lib.rs")]),
        "from a"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        "original\n"
    );
    assert_eq!(
        git(&repo, &["status", "--porcelain"]),
        "",
        "the parent checkout is untouched"
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn an_unused_worktree_and_its_branch_are_removed_when_the_task_fails_or_is_refused() {
    let fx = fixture(|config| config.orchestrator.max_children_per_task = 1);
    let repo = fx.repo();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    let task = delegate(&sup, json!({"isolation":"worktree"})).await;
    let path = PathBuf::from(
        status(&sup, &task).await["worktree"]["path"]
            .as_str()
            .unwrap(),
    );
    // A second child is over the fan-out limit: refused, and the worktree made for it is gone.
    let refusal = refused(
        &sup,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","isolation":"worktree","rights":{"read":true,"write":true}}),
    )
    .await;
    assert_eq!(refusal.code, "fan_out_limit");
    let child = fx.session_of_task(&sup, &task).await;
    ok(&sup, "task_cancel", json!({"task_id": task})).await;
    end_on_windows(&fx, &child).await;
    outcome(&sup, &task, "removed").await;
    assert!(!path.exists());
    eventually(|| async {
        let left = std::fs::read_dir(fx.worktrees())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name() != ".no-hooks")
            .count();
        (left == 0).then_some(())
    })
    .await;
    // The refused child's worktree is released in the background: its folder goes first and its
    // branch is deleted just after, with no task outcome to wait on.
    eventually_named("task branches deleted", || async {
        git(&repo, &["branch", "--list", "acpd/*"])
            .is_empty()
            .then_some(())
    })
    .await;
    assert_eq!(git(&repo, &["worktree", "list"]).lines().count(), 1);
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn folders_without_a_usable_repository_are_refused_or_shared_by_default() {
    let fx = fixture(|config| config.orchestrator.default_isolation = DefaultIsolation::Worktree);
    // Not a repository: an explicit worktree is refused before any agent starts.
    let root = fx.root(&fx.plain()).await;
    let sup = agent_of(&root).await;
    let refusal = refused(
        &sup,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","isolation":"worktree","rights":{"read":true,"write":true}}),
    )
    .await;
    assert_eq!(refusal.code, "isolation_unavailable");
    assert!(refusal.message.contains("not in a git repository"));
    assert_eq!(fx.manager.list().await.len(), 1, "no child was started");
    // The configured default falls back to the shared folder.
    let shared = delegate(&sup, json!({})).await;
    let view = status(&sup, &shared).await;
    assert!(view.get("worktree").is_none());
    assert_eq!(view["isolation"], "shared");
    assert!(
        view["isolation_note"]
            .as_str()
            .unwrap()
            .contains("not in a git repository")
    );
    // In a repository, the default gives a writer a worktree and a reader none.
    let root = fx.root(&fx.repo()).await;
    let sup = agent_of(&root).await;
    let writer = delegate(&sup, json!({})).await;
    let view = status(&sup, &writer).await;
    assert_eq!(view["worktree"]["outcome"], "active");
    assert_eq!(view["isolation"], "worktree");
    assert!(view.get("isolation_note").is_none());
    let reader = ok(
        &sup,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","rights":{"read":true}}),
    )
    .await;
    assert!(reader.get("worktree").is_none());
    // `shared` always wins over the default.
    let asked = delegate(&sup, json!({"isolation":"shared"})).await;
    assert!(status(&sup, &asked).await.get("worktree").is_none());
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn a_repository_above_the_delegating_folder_is_refused() {
    let fx = fixture(|_| {});
    // The root works in `src`; a worktree would check out the whole repository.
    let root = fx.root(&fx.repo().join("src")).await;
    let sup = agent_of(&root).await;
    let refusal = refused(
        &sup,
        "task_delegate",
        json!({"agent":"worker","prompt":"x","isolation":"worktree","rights":{"read":true,"write":true}}),
    )
    .await;
    assert_eq!(refusal.code, "isolation_unavailable");
    assert!(
        refusal
            .message
            .contains("outside the delegating task's folder")
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn a_dirty_parent_checkout_is_reported_and_its_edits_stay_out_of_the_worktree() {
    let fx = fixture(|_| {});
    let repo = fx.repo();
    std::fs::write(repo.join("src/lib.rs"), "parent's unsaved edit\n").unwrap();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    let task = delegate(&sup, json!({"isolation":"worktree"})).await;
    let view = status(&sup, &task).await;
    assert_eq!(view["worktree"]["base_dirty"], true);
    let path = PathBuf::from(view["worktree"]["path"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(path.join("src/lib.rs")).unwrap(),
        "original\n"
    );
    fx.manager.shutdown().await;
    assert_eq!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        "parent's unsaved edit\n"
    );
}

#[tokio::test]
async fn after_a_restart_worktrees_of_interrupted_tasks_are_released_and_unknown_clean_ones_removed()
 {
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    // Windows runners default to core.autocrlf=true, which rewrites "one\n" on checkout.
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let repo = repo.canonicalize().unwrap();
    let state = base.path().join("state");
    let root = state.join("worktrees");
    // What an earlier host left: two recorded worktrees (one with uncommitted work) and one it
    // made but never recorded.
    let clean = worktree::create(&root, &repo, &repo).unwrap();
    let dirty = worktree::create(&root, &repo, &repo).unwrap();
    std::fs::write(dirty.path.join("wip.txt"), "x").unwrap();
    let orphan = worktree::create(&root, &repo, &repo).unwrap();
    let record = |wt: &worktree::Worktree| TaskRecord {
        id: Uuid::new_v4(),
        parent: Some(Uuid::new_v4()),
        root: Uuid::new_v4(),
        depth: 1,
        agent: "worker".into(),
        scope: wt.task_folder(),
        rights: RecordRights {
            read: true,
            write: true,
            terminal: false,
        },
        state: "working".into(),
        session: None,
        created_at: 1,
        updated_at: 1,
        summary: None,
        stop_reason: None,
        trace_id: None,
        cost_usd: None,
        worktree: Some(wt.clone()),
    };
    records::save(&state, &[record(&clean), record(&dirty)]).unwrap();

    let manager =
        Arc::new(SessionManager::new(Config::default(), Registry::parse("[]").unwrap()).unwrap());
    let orchestrator = Orchestrator::new(Arc::downgrade(&manager));
    assert_eq!(orchestrator.restore(&state), 2);
    assert!(!clean.path.exists() && !orphan.path.exists());
    // The interrupted task's uncommitted work is now a commit on its branch.
    assert!(!dirty.path.exists());
    assert_eq!(
        git(&repo, &["show", &format!("{}:wip.txt", dirty.branch)]),
        "x"
    );
    let saved = records::load(&state).unwrap();
    let outcomes: Vec<Outcome> = saved
        .iter()
        .map(|r| r.worktree.as_ref().unwrap().outcome)
        .collect();
    assert!(outcomes.contains(&Outcome::Removed) && outcomes.contains(&Outcome::Snapshotted));
    assert!(saved.iter().all(|r| r.state == "failed"));
    assert_eq!(git(&repo, &["worktree", "list"]).lines().count(), 1);
}

#[test]
fn a_branch_that_cannot_be_counted_is_never_reported_removed_while_it_exists() {
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let repo = repo.canonicalize().unwrap();
    let root = base.path().join("state/worktrees");

    // Git cannot count commits against a base it does not know (standing in for any failed
    // count), but the branch is there: that is a failed cleanup, with the branch still present,
    // not "removed".
    let mut wt = worktree::create(&root, &repo, &repo).unwrap();
    wt.base = "0000000000000000000000000000000000000001".into();
    let began = std::time::Instant::now();
    assert_eq!(worktree::release(&root, &wt), Outcome::CleanupFailed);
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "it gave up in good time"
    );
    assert!(!git(&repo, &["branch", "--list", &wt.branch]).is_empty());

    // The same failed count with the branch really gone (deleted by hand) is nothing left to keep.
    let mut gone = worktree::create(&root, &repo, &repo).unwrap();
    git(
        &repo,
        &[
            "worktree",
            "remove",
            "--force",
            &gone.path.to_string_lossy(),
        ],
    );
    git(&repo, &["branch", "-D", &gone.branch]);
    gone.base = "0000000000000000000000000000000000000001".into();
    assert_eq!(worktree::release(&root, &gone), Outcome::Removed);

    // And when git cannot answer at all (here: the repository is not there), whether the branch
    // exists is unknown, which is not "gone" either.
    let mut lost = gone.clone();
    lost.repository = base.path().join("no-such-repository");
    lost.path = base.path().join("no-such-worktree");
    lost.outcome = Outcome::Active;
    assert_eq!(worktree::release(&root, &lost), Outcome::CleanupFailed);
}

#[tokio::test]
async fn a_worktree_that_will_not_go_does_not_hold_up_a_restart_and_is_tried_again_at_the_next() {
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(
        repo.join("a.txt"),
        "a
",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let repo = repo.canonicalize().unwrap();
    let state = base.path().join("state");
    let root = state.join("worktrees");
    // `git worktree remove` refuses a locked worktree on every platform, standing in for a
    // folder a scanner or an editor holds open.
    let stuck = worktree::create(&root, &repo, &repo).unwrap();
    git(&repo, &["worktree", "lock", &stuck.path.to_string_lossy()]);
    let task = Uuid::new_v4();
    records::save(
        &state,
        &[TaskRecord {
            id: task,
            parent: Some(Uuid::new_v4()),
            root: Uuid::new_v4(),
            depth: 1,
            agent: "worker".into(),
            scope: stuck.task_folder(),
            rights: RecordRights {
                read: true,
                write: true,
                terminal: false,
            },
            state: "working".into(),
            session: None,
            created_at: 1,
            updated_at: 1,
            summary: None,
            stop_reason: None,
            trace_id: None,
            cost_usd: None,
            worktree: Some(stuck.clone()),
        }],
    )
    .unwrap();
    let outcome_saved = || {
        records::load(&state).unwrap()[0]
            .worktree
            .as_ref()
            .unwrap()
            .outcome
    };

    // The first start: removal is tried for a few seconds, not the two minutes a running host
    // would allow, then given up and recorded.
    let manager =
        Arc::new(SessionManager::new(Config::default(), Registry::parse("[]").unwrap()).unwrap());
    let orchestrator = Orchestrator::new(Arc::downgrade(&manager));
    let began = std::time::Instant::now();
    assert_eq!(orchestrator.restore(&state), 1);
    let took = began.elapsed();
    assert!(
        took < Duration::from_secs(20),
        "start-up waited {took:?} for a worktree that would not go"
    );
    assert!(stuck.path.exists());
    assert_eq!(outcome_saved(), Outcome::CleanupFailed);

    // Once it can go, the next start removes it (a failed cleanup is not forgotten).
    git(
        &repo,
        &["worktree", "unlock", &stuck.path.to_string_lossy()],
    );
    let again = Orchestrator::new(Arc::downgrade(&manager));
    assert_eq!(again.restore(&state), 1);
    assert!(!stuck.path.exists());
    assert_eq!(outcome_saved(), Outcome::Removed);
    assert_eq!(git(&repo, &["worktree", "list"]).lines().count(), 1);
}

#[tokio::test]
async fn with_keep_a_cancelled_tasks_uncommitted_work_stays_in_its_worktree() {
    let fx = fixture(|config| {
        config.orchestrator.worktree_uncommitted = worktree::Uncommitted::Keep;
    });
    let root = fx.root(&fx.repo()).await;
    let sup = agent_of(&root).await;
    // No isolation given: the default puts a writer in a worktree.
    let task = delegate(&sup, json!({})).await;
    let session = fx.session_of_task(&sup, &task).await;
    let path = session.metadata().workspace;
    let own = write(&session, &path.join("draft.txt"), "wip");
    answer(&session.connection, true, "acpd-write-allow").await;
    own.await.unwrap().unwrap();
    ok(&sup, "task_cancel", json!({"task_id": task})).await;
    outcome(&sup, &task, "kept_dirty").await;
    assert_eq!(
        std::fs::read_to_string(path.join("draft.txt")).unwrap(),
        "wip"
    );
    fx.manager.shutdown().await;
}

/// A delegated writer in its own worktree writes `file` (uncommitted) and completes.
async fn completed_writer(fx: &Fixture, sup: &Agent, file: &str, content: &str) -> String {
    let task = delegate(sup, json!({"isolation":"worktree"})).await;
    let session = fx.session_of_task(sup, &task).await;
    let path = session.metadata().workspace;
    let own = write(&session, &path.join(file), content);
    answer(&session.connection, true, "acpd-write-allow").await;
    own.await.unwrap().unwrap();
    answer(&session.connection, false, "allow").await;
    reaches(sup, &task, "completed").await;
    task
}
fn refusal_code(result: Result<Value, acpd::orchestrator::views::MergeTaskRefusal>) -> String {
    result
        .expect_err("merge should be refused")
        .code()
        .to_owned()
}

#[tokio::test]
async fn the_phone_merges_a_finished_tasks_branch_into_the_parent_checkout() {
    let fx = fixture(|_| {});
    let repo = fx.repo();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    let task = completed_writer(&fx, &sup, "merged.txt", "from the child\n").await;
    let id: Uuid = task.parse().unwrap();
    let branch = status(&sup, &task).await["worktree"]["branch"]
        .as_str()
        .unwrap()
        .to_owned();
    let merged = fx.orchestrator.merge_task(id).await.unwrap();
    let commit = merged["merged"].as_str().unwrap();
    // The child's uncommitted work was committed to its branch, then merged as one commit.
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), commit);
    assert_eq!(
        std::fs::read_to_string(repo.join("merged.txt")).unwrap(),
        "from the child\n"
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%an %p"])
            .split(' ')
            .count(),
        3,
        "a merge commit"
    );
    assert_eq!(git(&repo, &["log", "-1", "--format=%an"]), "acpd");
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    // The folder and branch go after the reply (on Windows once the agent process ends).
    let child = fx.session_of_task(&sup, &task).await;
    end_on_windows(&fx, &child).await;
    eventually_named("merged worktree and branch removed", || async {
        (git(&repo, &["branch", "--list", &branch]).is_empty()
            && git(&repo, &["worktree", "list"]).lines().count() == 1)
            .then_some(())
    })
    .await;
    let view = status(&sup, &task).await;
    assert_eq!(view["worktree"]["outcome"], "merged");
    assert_eq!(view["worktree"]["merged"], commit);
    // Once merged: not again, and no follow-up into a folder that is gone.
    assert_eq!(
        refusal_code(fx.orchestrator.merge_task(id).await),
        "already_merged"
    );
    let follow = refused(
        &sup,
        "task_send",
        json!({"task_id": task, "prompt": "more"}),
    )
    .await;
    assert_eq!(follow.code, "invalid_state");
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn a_merge_is_refused_with_nothing_changed_when_the_parent_is_dirty_or_it_conflicts() {
    let fx = fixture(|_| {});
    let repo = fx.repo();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    let task = completed_writer(&fx, &sup, "src/lib.rs", "child version\n").await;
    let id: Uuid = task.parse().unwrap();
    let before = git(&repo, &["rev-parse", "HEAD"]);

    // Dirty parent: refused; the parent's edit and the branch are untouched.
    std::fs::write(repo.join("src/lib.rs"), "parent edit\n").unwrap();
    assert_eq!(
        refusal_code(fx.orchestrator.merge_task(id).await),
        "parent_dirty"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        "parent edit\n"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), before);
    let view = status(&sup, &task).await;
    assert_eq!(
        view["worktree"]["outcome"], "active",
        "still on disk, nothing merged"
    );

    // The parent commits a conflicting change: refused, the merge is aborted, nothing changed.
    git(&repo, &["commit", "-q", "-am", "parent change"]);
    let parent_head = git(&repo, &["rev-parse", "HEAD"]);
    match fx.orchestrator.merge_task(id).await {
        Err(acpd::orchestrator::views::MergeTaskRefusal::Conflict {
            files,
            branch,
            repository,
        }) => {
            assert_eq!(files, vec!["src/lib.rs".to_owned()]);
            // What the owner needs to merge by hand.
            assert_eq!(
                branch,
                status(&sup, &task).await["worktree"]["branch"]
                    .as_str()
                    .unwrap()
            );
            assert_eq!(
                plain(std::path::PathBuf::from(repository)),
                plain(repo.clone())
            );
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), parent_head);
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    assert!(!repo.join(".git/MERGE_HEAD").exists());
    assert_eq!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        "parent edit\n"
    );
    let branch = view["worktree"]["branch"].as_str().unwrap();
    assert_eq!(
        git(&repo, &["show", &format!("{branch}:src/lib.rs")]),
        "child version"
    );
    assert_eq!(status(&sup, &task).await["worktree"]["outcome"], "active");
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn only_finished_tasks_with_a_worktree_and_new_work_can_be_merged() {
    let fx = fixture(|_| {});
    let repo = fx.repo();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    // Still running.
    let open = delegate(&sup, json!({"isolation":"worktree"})).await;
    let open_id: Uuid = open.parse().unwrap();
    assert_eq!(
        refusal_code(fx.orchestrator.merge_task(open_id).await),
        "task_not_finished"
    );
    // Finished with nothing new: its branch was deleted on release.
    ok(&sup, "task_cancel", json!({"task_id": open})).await;
    let child = fx.session_of_task(&sup, &open).await;
    end_on_windows(&fx, &child).await;
    outcome(&sup, &open, "removed").await;
    assert_eq!(
        refusal_code(fx.orchestrator.merge_task(open_id).await),
        "nothing_to_merge"
    );
    // A read-only task shares the parent's folder: no worktree.
    let shared = delegate(&sup, json!({"rights":{"read":true,"write":false}})).await;
    ok(&sup, "task_cancel", json!({"task_id": shared})).await;
    reaches(&sup, &shared, "canceled").await;
    let shared_id: Uuid = shared.parse().unwrap();
    assert_eq!(
        refusal_code(fx.orchestrator.merge_task(shared_id).await),
        "no_worktree"
    );
    assert_eq!(
        refusal_code(fx.orchestrator.merge_task(Uuid::new_v4()).await),
        "task_not_found"
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn repositories_with_submodules_and_too_many_task_branches_get_no_worktree() {
    let fx = fixture(|config| config.orchestrator.max_task_branches = 2);
    let repo = fx.repo();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    let explicit = json!({"agent":"worker","prompt":"x","isolation":"worktree","rights":{"read":true,"write":true}});

    // Submodules: a worktree would leave them empty, so it is refused, or (the default) the task
    // runs shared and its view says why.
    std::fs::write(
        repo.join(".gitmodules"),
        "[submodule \"lib\"]\n\tpath = lib\n\turl = ./lib\n",
    )
    .unwrap();
    let refusal = refused(&sup, "task_delegate", explicit.clone()).await;
    assert_eq!(refusal.code, "isolation_unavailable");
    assert!(refusal.message.contains("submodules"), "{refusal:?}");
    let shared = delegate(&sup, json!({})).await;
    let view = status(&sup, &shared).await;
    assert!(view.get("worktree").is_none());
    assert!(
        view["isolation_note"]
            .as_str()
            .unwrap()
            .contains("submodules")
    );
    std::fs::remove_file(repo.join(".gitmodules")).unwrap();

    // The branch limit: two unmerged task branches already in the repository (kept from earlier
    // tasks, or made by hand) mean no third. Nothing is deleted to make room.
    git(&repo, &["branch", "acpd/task-aaaaaaaaaaaa"]);
    git(&repo, &["branch", "acpd/task-bbbbbbbbbbbb"]);
    let refusal = refused(&sup, "task_delegate", explicit.clone()).await;
    assert_eq!(refusal.code, "isolation_unavailable");
    assert!(refusal.message.contains("too many unmerged task branches"));
    let shared = delegate(&sup, json!({})).await;
    let note = status(&sup, &shared).await["isolation_note"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(note.contains("too many unmerged task branches"), "{note}");
    assert!(!git(&repo, &["branch", "--list", "acpd/task-aaaaaaaaaaaa"]).is_empty());
    assert!(!git(&repo, &["branch", "--list", "acpd/task-bbbbbbbbbbbb"]).is_empty());

    // Room again once one is gone.
    git(&repo, &["branch", "-D", "acpd/task-bbbbbbbbbbbb"]);
    let task = delegate(&sup, json!({"isolation":"worktree"})).await;
    assert_eq!(status(&sup, &task).await["worktree"]["outcome"], "active");
    fx.manager.shutdown().await;
}

#[test]
fn the_branch_limit_counts_only_task_branches_and_zero_means_no_limit() {
    let base = tempfile::tempdir().unwrap();
    let repo = base.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let repo = repo.canonicalize().unwrap();
    let root = base.path().join("state/worktrees");
    // The user's own branches never count.
    for name in ["feature", "acpd-notes", "main2"] {
        git(&repo, &["branch", name]);
    }
    assert_eq!(worktree::task_branches(&root, &repo), 0);
    git(&repo, &["branch", "acpd/task-111111111111"]);
    assert_eq!(worktree::task_branches(&root, &repo), 1);
    // At the limit: refused. One under: allowed. 0: never refused.
    assert_eq!(
        worktree::create_limited(&root, &repo, &repo, 1).unwrap_err(),
        worktree::Unavailable::TooManyBranches
    );
    let made = worktree::create_limited(&root, &repo, &repo, 3).unwrap();
    assert_eq!(worktree::task_branches(&root, &repo), 2);
    assert!(worktree::create_limited(&root, &repo, &repo, 0).is_ok());
    drop(made);
}

#[tokio::test]
async fn the_phone_throws_a_finished_tasks_work_away_without_touching_the_parent_checkout() {
    let fx = fixture(|_| {});
    let repo = fx.repo();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    let task = completed_writer(&fx, &sup, "unwanted.txt", "nope\n").await;
    let id: Uuid = task.parse().unwrap();
    let branch = status(&sup, &task).await["worktree"]["branch"]
        .as_str()
        .unwrap()
        .to_owned();

    // The agent of a completed task is still running; the discard ends it.
    let done = fx.orchestrator.discard_task(id).await.unwrap();
    assert_eq!(done["discarded"], branch.as_str());
    assert!(git(&repo, &["branch", "--list", &branch]).is_empty());
    assert_eq!(git(&repo, &["worktree", "list"]).lines().count(), 1);
    // The parent's folder never saw it.
    assert!(!repo.join("unwanted.txt").exists());
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    assert_eq!(
        status(&sup, &task).await["worktree"]["outcome"],
        "discarded"
    );
    // Once discarded: not again, not merged, and no follow-up into a folder that is gone.
    assert_eq!(
        refusal_code(fx.orchestrator.discard_task(id).await),
        "already_discarded"
    );
    assert_eq!(
        refusal_code(fx.orchestrator.merge_task(id).await),
        "already_discarded"
    );
    let follow = refused(
        &sup,
        "task_send",
        json!({"task_id": task, "prompt": "more"}),
    )
    .await;
    assert_eq!(follow.code, "invalid_state");
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn a_kept_dirty_worktree_can_be_discarded_and_an_open_or_empty_task_cannot() {
    let fx = fixture(|config| {
        config.orchestrator.worktree_uncommitted = worktree::Uncommitted::Keep;
    });
    let repo = fx.repo();
    let root = fx.root(&repo).await;
    let sup = agent_of(&root).await;
    // A task still running has nothing to discard yet.
    let task = delegate(&sup, json!({})).await;
    let id: Uuid = task.parse().unwrap();
    assert_eq!(
        refusal_code(fx.orchestrator.discard_task(id).await),
        "task_not_finished"
    );
    // Cancelled with uncommitted work, kept on disk: discarding removes it, force and all.
    let session = fx.session_of_task(&sup, &task).await;
    let path = session.metadata().workspace;
    let own = write(&session, &path.join("draft.txt"), "wip");
    answer(&session.connection, true, "acpd-write-allow").await;
    own.await.unwrap().unwrap();
    ok(&sup, "task_cancel", json!({"task_id": task})).await;
    outcome(&sup, &task, "kept_dirty").await;
    let branch = status(&sup, &task).await["worktree"]["branch"]
        .as_str()
        .unwrap()
        .to_owned();
    fx.orchestrator.discard_task(id).await.unwrap();
    assert!(!path.join("draft.txt").exists());
    assert!(git(&repo, &["branch", "--list", &branch]).is_empty());
    assert_eq!(
        status(&sup, &task).await["worktree"]["outcome"],
        "discarded"
    );
    // A task that changed nothing was cleaned up already: there is nothing to discard.
    let idle = delegate(&sup, json!({})).await;
    let idle_session = fx.session_of_task(&sup, &idle).await;
    ok(&sup, "task_cancel", json!({"task_id": idle})).await;
    end_on_windows(&fx, &idle_session).await;
    outcome(&sup, &idle, "removed").await;
    assert_eq!(
        refusal_code(fx.orchestrator.discard_task(idle.parse().unwrap()).await),
        "nothing_to_discard"
    );
    // And an unknown task.
    assert_eq!(
        refusal_code(fx.orchestrator.discard_task(Uuid::new_v4()).await),
        "task_not_found"
    );
    fx.manager.shutdown().await;
}

#[tokio::test]
async fn a_delegated_task_in_a_worktree_is_auto_approved_but_a_shared_one_is_not() {
    // The default policy applies only to sessions in a task worktree: the host knows that from
    // where the folder is, not from anything the agent says.
    let fx = fixture(|config| {
        config.auto_approve.insert(
            "worker".into(),
            acpd::auto_approve::AutoApproveConfig {
                max_risk: acpd::auto_approve::AutoRisk::Medium,
                ..Default::default()
            },
        );
    });
    let root = fx.root(&fx.repo()).await;
    let sup = agent_of(&root).await;

    // In its own worktree: the mock agent's edit request is answered by the host; no person and
    // no supervisor is involved, and the task simply completes.
    let isolated = delegate(&sup, json!({"isolation":"worktree"})).await;
    reaches(&sup, &isolated, "completed").await;
    let view = status(&sup, &isolated).await;
    assert_eq!(view["needs_decision"], json!([]));
    let child = fx.session_of_task(&sup, &isolated).await;
    let reviews: Vec<Value> = child
        .replay(0)
        .events
        .iter()
        .filter_map(|event| {
            let review = &event.message["params"]["update"]["_meta"]["acpdAutoReview"];
            review.is_object().then(|| review.clone())
        })
        .collect();
    assert_eq!(reviews.len(), 1, "{reviews:?}");
    assert_eq!(reviews[0]["layer"], "rules");
    assert_eq!(reviews[0]["decision"], "allow_once");

    // In the shared folder with the same policy: left for the supervisor or the phone.
    let shared = delegate(&sup, json!({"isolation":"shared"})).await;
    reaches(&sup, &shared, "input_required").await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(status(&sup, &shared).await["state"], "input_required");
    let waiting = status(&sup, &shared).await["needs_decision"]
        .as_array()
        .map_or(0, Vec::len);
    assert_eq!(waiting, 1, "the request is still there for a decision");
    fx.manager.shutdown().await;
}
