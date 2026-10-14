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
    _orchestrator: Arc<Orchestrator>,
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
        };
        self.manager
            .open_configured("test-agent", folder, None, vec![], all)
            .await
            .unwrap()
    }
    async fn session_of_task(&self, agent: &Agent, task: &str) -> Arc<AcpSession> {
        let path = eventually(|| async {
            let status = ok(agent, "task_status", json!({"task_id": task})).await;
            status["tasks"][0]["worktree"]["path"]
                .as_str()
                .map(PathBuf::from)
        })
        .await;
        eventually(|| async {
            for metadata in self.manager.list().await {
                if metadata.workspace == path {
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
        _orchestrator: orchestrator,
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
async fn eventually<T, F, Fut>(mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    timeout(Duration::from_secs(20), async {
        loop {
            if let Some(value) = check().await {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("condition not reached in time")
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
/// The worktree folder as the host reports it (no `\\?\` prefix on Windows).
fn plain(path: PathBuf) -> PathBuf {
    let text = path.canonicalize().unwrap().to_string_lossy().into_owned();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned())
}
/// Act as the phone: answer the child's pending request from the given source.
async fn answer(connection: &AcpConnection, host_write: bool, option: &str) {
    let permission = eventually(|| async {
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
    assert_eq!(sa.metadata().workspace, pa);
    assert_eq!(sb.metadata().workspace, pb);

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
    eventually(|| async { (!top_a.exists()).then_some(()) }).await;
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
    ok(&sup, "task_cancel", json!({"task_id": task})).await;
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
    assert_eq!(git(&repo, &["branch", "--list", "acpd/*"]), "");
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
