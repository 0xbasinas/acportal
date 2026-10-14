//! Experimental agent orchestrator (see `docs/orchestrator-design.md`).
//!
//! `Orchestrator` owns the task tree and the per-task runtime (token, result, cancellation).
//! `SessionManager` calls [`Orchestrator::begin`] before it starts an agent (admission, and the
//! MCP server that gives the agent its delegation tools), [`Orchestrator::bind`] once the session
//! exists and [`Orchestrator::release_subtree`] when a session is removed. Agents reach the
//! orchestrator only through the control channel in [`control`], one task token per session.
//!
//! Worktree isolation for delegated tasks is in `worktree.rs` (phase 4).
//! Permission requests from a delegated child go to the phone like any other session's.
pub mod approvals;
pub mod control;
mod decisions;
pub mod delegate;
pub mod records;
pub mod tree;
pub mod views;
pub mod worktree;

use crate::{
    config::{OrchestratorConfig, SupervisorRisk},
    connection::{AgentRpcError, WorkspaceAccess},
    session::{LaunchSettings, SessionManager},
};
use delegate::ToolFailure;
use records::TaskRecord;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::{Component, Path},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, watch};
use tree::{AdmissionError, Limits, NewTask, Rights, TaskNode, TaskState, TaskTree};
use uuid::Uuid;

/// Most agent text kept per task. More is dropped and reported as `truncated`.
const RESULT_LIMIT: usize = 256 * 1024;
const DEFAULT_RESULT_BYTES: usize = 32 * 1024;

/// Admission refused a task before any process started. Carried through `anyhow` so the session
/// manager can return it unchanged and the orchestrator can turn it back into a stable code.
#[derive(Debug)]
pub struct AdmissionFailure(pub AdmissionError);
impl std::fmt::Display for AdmissionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "delegation refused: {}", self.0.code())
    }
}
impl std::error::Error for AdmissionFailure {}

/// Returned by [`Orchestrator::begin`]; `mcp` is the stdio MCP server to add to `session/new`.
#[derive(Debug)]
pub struct Ticket {
    pub task: Uuid,
    pub mcp: Option<Value>,
    /// The W3C `traceparent` for this task: the tree's trace id and this task's own span.
    pub traceparent: Option<String>,
}

struct Runtime {
    session: Option<Uuid>,
    token_hash: Option<String>,
    result: String,
    truncated: bool,
    stop_reason: Option<String>,
    error: Option<String>,
    cancel_requested: bool,
    /// Which limit ended (or is ending) this task, if one did.
    limit: Option<Limit>,
    /// Cumulative USD cost the agent reported; `None` means unmetered.
    cost_usd: Option<f64>,
    /// Prompts this task has been given: the first, plus each accepted follow-up.
    turns: u32,
    /// The delegation tools were offered to this task's agent (a token was issued)...
    delegate_offered: bool,
    /// ...and its `mcp-delegate` process has since said hello over the control channel.
    delegate_connected: bool,
    cancel: Arc<Notify>,
    /// Permission requests of this task still waiting for an answer, by request id.
    requests: BTreeMap<String, decisions::Request>,
    /// Recently answered request ids, so a late duplicate decision is recognized.
    answered: VecDeque<String>,
    /// Set on a root task by the phone: supervisor approvals stop for its whole tree.
    supervisor_paused: bool,
    /// Unix seconds, for the task records.
    created_at: u64,
    updated_at: u64,
    /// The W3C trace id the whole tree shares.
    trace_id: Option<String>,
    /// The task's own git worktree, when it was delegated with worktree isolation.
    worktree: Option<worktree::Worktree>,
    /// Why a writer that would have had a worktree by default runs in the shared folder.
    isolation_note: Option<&'static str>,
}
/// A bound a task went past.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Limit {
    Deadline,
    /// No event for this many seconds.
    Idle(u64),
    Cost,
}
/// What the task's summary says, in the host's own words.
fn limit_text(limit: Limit) -> String {
    match limit {
        Limit::Deadline => "the task's deadline passed".into(),
        Limit::Idle(seconds) => format!("the task made no progress for {seconds} seconds"),
        Limit::Cost => "the task went over its cost budget".into(),
    }
}
/// The cumulative USD cost in a `usage_update`, if it carries one. Another currency is not
/// converted: that agent is unmetered for a USD budget.
fn usd_cost(message: &Value) -> Option<f64> {
    let update = &message["params"]["update"];
    (message["method"] == "session/update"
        && update["sessionUpdate"] == "usage_update"
        && update["cost"]["currency"] == "USD")
        .then(|| update["cost"]["amount"].as_f64())
        .flatten()
        .filter(|amount| amount.is_finite() && *amount >= 0.0)
}
/// Random lowercase hex, `bytes` long, never all zero (an all-zero trace or span id is invalid).
fn random_hex(bytes: usize) -> String {
    loop {
        let mut buffer = vec![0u8; bytes];
        if getrandom::fill(&mut buffer).is_ok() && buffer.iter().any(|b| *b != 0) {
            return buffer.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
/// What an agent and the phone are told about a task's worktree.
fn worktree_view(wt: &worktree::Worktree) -> Value {
    json!({
        "branch": wt.branch,
        "base": wt.base,
        "path": wt.task_folder(),
        "base_dirty": wt.base_dirty,
        "outcome": wt.outcome,
    })
}
impl Runtime {
    fn new() -> Self {
        let stamp = now();
        Self {
            session: None,
            token_hash: None,
            result: String::new(),
            truncated: false,
            stop_reason: None,
            error: None,
            cancel_requested: false,
            limit: None,
            cost_usd: None,
            turns: 1,
            delegate_offered: false,
            delegate_connected: false,
            cancel: Arc::new(Notify::new()),
            requests: BTreeMap::new(),
            answered: VecDeque::new(),
            supervisor_paused: false,
            created_at: stamp,
            updated_at: stamp,
            trace_id: None,
            worktree: None,
            isolation_note: None,
        }
    }
}

struct State {
    tree: TaskTree,
    runtime: HashMap<Uuid, Runtime>,
    /// SHA-256 of a task token to its task. Raw tokens are never stored.
    tokens: HashMap<String, Uuid>,
    by_session: HashMap<Uuid, Uuid>,
    /// Worktrees made for a delegation whose session is starting, by the folder the task will
    /// run in, with the folder it was delegated in (what admission checks).
    placements: HashMap<std::path::PathBuf, (std::path::PathBuf, worktree::Worktree)>,
}

pub struct Orchestrator {
    manager: Weak<SessionManager>,
    state: Mutex<State>,
    changed: watch::Sender<u64>,
    control_path: OnceLock<String>,
    /// The executable that runs `mcp-delegate`. Defaults to the running host.
    delegate_executable: OnceLock<std::path::PathBuf>,
    cancel_grace: Duration,
    me: Weak<Orchestrator>,
    /// Where task records are kept, once `restore` has been called. Without it nothing is saved.
    state_dir: OnceLock<std::path::PathBuf>,
    persist_pending: AtomicBool,
    write_lock: Mutex<()>,
    janitor_started: AtomicBool,
}

fn limits(config: &OrchestratorConfig) -> Limits {
    Limits {
        max_depth: config.max_depth,
        max_children_per_task: config.max_children_per_task,
        max_tasks_total: config.max_tasks_total,
        // The session manager enforces `max_sessions` itself when it starts the process.
        max_sessions: usize::MAX,
    }
}
fn rights_of(access: WorkspaceAccess) -> Rights {
    Rights {
        read: access.read_files,
        write: access.write_files,
        terminal: access.terminal,
    }
}
fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn state_name(state: TaskState) -> &'static str {
    records::state_name(state)
}
fn failure(error: AdmissionError) -> ToolFailure {
    let message = match error {
        AdmissionError::OrchestratorDisabled => "delegation is disabled on this host",
        AdmissionError::DepthLimit => "delegation depth limit reached",
        AdmissionError::FanOutLimit => "too many open tasks",
        AdmissionError::AgentNotDelegable => "that agent cannot be delegated to",
        AdmissionError::Cycle => "that agent is already working in this workspace above you",
        AdmissionError::ScopeNotAllowed => "workspace is outside your task's scope",
        AdmissionError::RightsNotAllowed => "requested rights exceed your task's rights",
        AdmissionError::SessionLimit => "session limit reached",
        AdmissionError::ParentNotOpen => "your task has already finished",
    };
    ToolFailure::new(error.code(), message)
}
fn clip(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// Agent message text from journal events: `session/update` with `agent_message_chunk`.
fn agent_text(events: &[Arc<crate::connection::Event>]) -> String {
    let mut text = String::new();
    for event in events {
        let update = &event.message["params"]["update"];
        if event.message["method"] == "session/update"
            && update["sessionUpdate"] == "agent_message_chunk"
            && update["content"]["type"] == "text"
            && let Some(chunk) = update["content"]["text"].as_str()
        {
            text.push_str(chunk);
        }
    }
    text
}

impl Orchestrator {
    pub fn new(manager: Weak<SessionManager>) -> Arc<Self> {
        Self::with_cancel_grace(manager, Duration::from_secs(15))
    }
    pub fn with_cancel_grace(manager: Weak<SessionManager>, cancel_grace: Duration) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            state_dir: OnceLock::new(),
            persist_pending: AtomicBool::new(false),
            write_lock: Mutex::new(()),
            janitor_started: AtomicBool::new(false),
            manager,
            state: Mutex::new(State {
                tree: TaskTree::new(false, Limits::default()),
                runtime: HashMap::new(),
                tokens: HashMap::new(),
                by_session: HashMap::new(),
                placements: HashMap::new(),
            }),
            changed: watch::channel(0).0,
            control_path: OnceLock::new(),
            delegate_executable: OnceLock::new(),
            cancel_grace,
        })
    }
    /// The control endpoint agents' delegate processes connect to. Without it no agent gets the
    /// delegation tools.
    pub fn set_control_path(&self, path: String) {
        let _ = self.control_path.set(path);
    }
    /// Use this executable for `mcp-delegate` instead of the running one. Only needed when the
    /// host is embedded in something that is not `acpd` (tests, other programs).
    pub fn set_delegate_executable(&self, path: std::path::PathBuf) {
        let _ = self.delegate_executable.set(path);
    }
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("orchestrator state")
    }
    fn bump(&self) {
        self.changed.send_modify(|generation| *generation += 1);
        self.schedule_persist();
    }

    /// Every task this host knows about, oldest first: what the phone, `acpd tasks` and the
    /// saved records show.
    pub fn tasks(&self) -> Vec<TaskRecord> {
        let state = self.lock();
        let mut records: Vec<TaskRecord> = state
            .tree
            .nodes()
            .map(|node| {
                let runtime = state.runtime.get(&node.id);
                TaskRecord {
                    id: node.id,
                    parent: node.parent,
                    root: node.root,
                    depth: node.depth,
                    agent: node.agent.clone(),
                    scope: node.scope.clone(),
                    rights: node.rights.into(),
                    state: records::state_name(node.state).into(),
                    session: runtime.and_then(|r| r.session),
                    created_at: runtime.map_or(0, |r| r.created_at),
                    updated_at: runtime.map_or(0, |r| r.updated_at),
                    summary: runtime.and_then(|r| r.error.clone()),
                    stop_reason: runtime.and_then(|r| r.stop_reason.clone()),
                    trace_id: runtime.and_then(|r| r.trace_id.clone()),
                    cost_usd: runtime.and_then(|r| r.cost_usd),
                    worktree: runtime.and_then(|r| r.worktree.clone()),
                }
            })
            .collect();
        records.sort_by_key(|record| (record.created_at, record.depth));
        records
    }

    /// Write the task records soon. Bursts of changes become one write, and the snapshot is
    /// taken only when the writer runs, so the file always ends up matching the last change.
    fn schedule_persist(&self) {
        if self.state_dir.get().is_none() || self.persist_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        match (self.me.upgrade(), tokio::runtime::Handle::try_current()) {
            (Some(me), Ok(handle)) => {
                handle.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    me.persist_pending.store(false, Ordering::Release);
                    let writer = me.clone();
                    let _ = tokio::task::spawn_blocking(move || writer.persist_now()).await;
                });
            }
            _ => {
                self.persist_pending.store(false, Ordering::Release);
                self.persist_now();
            }
        }
    }
    /// Write the task records now. The snapshot is taken under the write lock, so a slower
    /// earlier writer can never overwrite a later one.
    pub fn persist_now(&self) {
        let Some(directory) = self.state_dir.get() else {
            return;
        };
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let records = records::prune(self.tasks());
        if records.is_empty() && !records::path(directory).exists() {
            return; // no file for nothing
        }
        if records::save(directory, &records).is_err() {
            tracing::warn!("task records could not be saved");
        }
    }
    /// Remember where task records live, load what an earlier run left, and show every task that
    /// was still open as failed because the host restarted. Nothing is restarted and no prompt
    /// is sent again. An unreadable file is set aside rather than stopping the host. Returns the
    /// number of records loaded.
    pub fn restore(&self, directory: &std::path::Path) -> usize {
        let _ = self.state_dir.set(directory.to_path_buf());
        let mut loaded = match records::load(directory) {
            Ok(loaded) => loaded,
            Err(_) => {
                tracing::warn!("task records were unreadable and have been set aside");
                records::set_aside(directory);
                vec![]
            }
        };
        let interrupted = records::interrupt(&mut loaded, now());
        {
            let mut state = self.lock();
            for record in &loaded {
                let Some(task_state) = records::parse_state(&record.state) else {
                    continue;
                };
                state.tree.restore_node(TaskNode {
                    id: record.id,
                    parent: record.parent,
                    root: record.root,
                    depth: record.depth,
                    agent: record.agent.clone(),
                    scope: record.scope.clone(),
                    rights: Rights {
                        read: record.rights.read,
                        write: record.rights.write,
                        terminal: record.rights.terminal,
                    },
                    state: task_state,
                });
                let mut runtime = Runtime::new();
                runtime.session = record.session;
                runtime.created_at = record.created_at;
                runtime.updated_at = record.updated_at;
                runtime.error = record.summary.clone();
                runtime.stop_reason = record.stop_reason.clone();
                runtime.trace_id = record.trace_id.clone();
                runtime.cost_usd = record.cost_usd;
                runtime.worktree = record.worktree.clone();
                state.runtime.insert(record.id, runtime);
            }
        }
        // Every restored task is finished, so its worktree is done with: release what is still
        // there (dirty ones are kept), then remove clean worktrees no record knows about.
        let mut released = false;
        let mut known = vec![];
        let policy = self.uncommitted_policy();
        {
            let mut state = self.lock();
            for runtime in state.runtime.values_mut() {
                if let Some(wt) = runtime.worktree.as_mut() {
                    known.push(wt.path.clone());
                    if matches!(
                        wt.outcome,
                        worktree::Outcome::Active | worktree::Outcome::Releasing
                    ) {
                        let mut pending = wt.clone();
                        pending.outcome = worktree::Outcome::Active;
                        let root = wt.path.parent().map(Path::to_path_buf).unwrap_or_default();
                        wt.outcome = worktree::release_with(&root, &pending, policy);
                        released = true;
                    }
                }
            }
        }
        let root = directory.join("worktrees");
        if root.is_dir() {
            let left = worktree::sweep_unknown(&root, &known);
            if left > 0 {
                tracing::warn!(left, "task worktrees without a record were left in place");
            }
        }
        if interrupted > 0 || released {
            self.persist_now();
        }
        loaded.len()
    }
    fn manager(&self) -> Result<Arc<SessionManager>, ToolFailure> {
        self.manager
            .upgrade()
            .ok_or_else(|| ToolFailure::new("host_stopping", "the host is shutting down"))
    }

    /// Admission, before any process starts. `Ok(None)` means this session is not part of the
    /// orchestrator (a phone-started session of an agent that does not delegate).
    pub fn begin(
        &self,
        parent: Option<Uuid>,
        agent: &str,
        scope: &Path,
        access: WorkspaceAccess,
        settings: &LaunchSettings,
    ) -> Result<Option<Ticket>, AdmissionFailure> {
        let config = &settings.config.orchestrator;
        let orchestrates = config.enabled && config.orchestrate.iter().any(|id| id == agent);
        if parent.is_none() && !orchestrates {
            return Ok(None);
        }
        let delegable = parent.is_none()
            || (config.delegable.iter().any(|id| id == agent)
                && settings.registry.get(agent).is_ok_and(|a| a.enabled));
        let mut state = self.lock();
        state.tree.configure(config.enabled, limits(config));
        let placement = parent.and_then(|_| state.placements.remove(scope));
        let task = state.tree.admit(NewTask {
            parent,
            agent,
            scope: placement
                .as_ref()
                .map_or(scope, |(origin, _)| origin.as_path()),
            rights: rights_of(access),
            delegable,
        });
        let task = match task {
            Ok(task) => task,
            Err(refused) => {
                drop(state);
                if let Some((_, worktree)) = placement {
                    self.release_worktrees(vec![worktree]);
                }
                return Err(AdmissionFailure(refused));
            }
        };
        let mut runtime = Runtime::new();
        if let Some((_, worktree)) = placement {
            state.tree.set_scope(task, scope.to_path_buf());
            runtime.worktree = Some(worktree);
        }
        let executable = match self.delegate_executable.get() {
            Some(path) => Ok(path.clone()),
            None => std::env::current_exe(),
        };
        let mcp = match (orchestrates, self.control_path.get(), executable) {
            (true, Some(path), Ok(exe)) => {
                let mut bytes = [0u8; 32];
                getrandom::fill(&mut bytes).ok();
                let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                let digest = hash(&token);
                state.tokens.insert(digest.clone(), task);
                runtime.token_hash = Some(digest);
                runtime.delegate_offered = true;
                Some(json!({
                    "name": "acpd-orchestrator",
                    "command": exe,
                    "args": ["mcp-delegate"],
                    "env": [
                        {"name": "ACPD_CONTROL", "value": path},
                        {"name": "ACPD_TASK_TOKEN", "value": token}
                    ]
                }))
            }
            _ => None,
        };
        // One trace per tree: a root starts it, children join it with a span of their own.
        let trace = parent
            .and_then(|parent| state.runtime.get(&parent).and_then(|r| r.trace_id.clone()))
            .unwrap_or_else(|| random_hex(16));
        let traceparent = format!("00-{trace}-{}-01", random_hex(8));
        runtime.trace_id = Some(trace);
        state.runtime.insert(task, runtime);
        Ok(Some(Ticket {
            task,
            mcp,
            traceparent: Some(traceparent),
        }))
    }

    /// The session for `task` exists. Phone-started (root) tasks are working from here on.
    pub fn bind(&self, task: Uuid, session: Uuid) {
        let mut state = self.lock();
        state.by_session.insert(session, task);
        if let Some(runtime) = state.runtime.get_mut(&task) {
            runtime.session = Some(session);
            runtime.updated_at = now();
        }
        if state.tree.get(task).is_some_and(|n| n.parent.is_none()) {
            state.tree.transition(task, TaskState::Working);
        }
        drop(state);
        self.bump();
    }

    /// The session never started: undo admission. A child stays visible as `rejected`.
    pub fn abort(&self, task: Uuid, reason: &str) {
        let mut state = self.lock();
        let Some(node) = state.tree.get(task).cloned() else {
            return;
        };
        let hash = state
            .runtime
            .get_mut(&task)
            .and_then(|runtime| runtime.token_hash.take());
        if let Some(hash) = hash {
            state.tokens.remove(&hash);
        }
        if node.parent.is_none() {
            state.tree.remove_tree(node.root);
            state.runtime.remove(&task);
        } else {
            state.tree.transition(task, TaskState::Rejected);
            if let Some(runtime) = state.runtime.get_mut(&task) {
                runtime.error = Some(clip(reason, 300).0.to_owned());
                runtime.updated_at = now();
            }
        }
        let worktree = Self::take_worktree(&mut state, task);
        drop(state);
        self.release_worktrees(worktree.into_iter().collect());
        self.bump();
    }

    /// A session is being removed. Ends its task and every task below it, revokes their tokens
    /// and returns the descendants' sessions (deepest first) for the manager to remove.
    pub fn release_subtree(&self, session: Uuid) -> Vec<Uuid> {
        let mut state = self.lock();
        let Some(task) = state.by_session.remove(&session) else {
            return vec![];
        };
        let Some(node) = state.tree.get(task).cloned() else {
            return vec![];
        };
        let ids = state.tree.subtree(task);
        let mut sessions = vec![];
        let mut worktrees = vec![];
        for id in &ids {
            worktrees.extend(Self::take_worktree(&mut state, *id));
            state.tree.force(*id, TaskState::Canceled);
            let (hash, child_session, cancel) = match state.runtime.get_mut(id) {
                Some(runtime) => {
                    runtime.cancel_requested = true;
                    (
                        runtime.token_hash.take(),
                        if *id == task {
                            None
                        } else {
                            runtime.session.take()
                        },
                        Some(runtime.cancel.clone()),
                    )
                }
                None => (None, None, None),
            };
            if let Some(hash) = hash {
                state.tokens.remove(&hash);
            }
            if let Some(child) = child_session {
                state.by_session.remove(&child);
                sessions.push(child);
            }
            if let Some(cancel) = cancel {
                cancel.notify_one();
            }
        }
        if node.parent.is_none() {
            state.tree.remove_tree(node.root);
            for id in &ids {
                state.runtime.remove(id);
            }
        }
        drop(state);
        self.release_worktrees(worktrees);
        self.bump();
        sessions
    }

    /// Finish with worktrees off the caller's thread (git can take a moment) and record what
    /// became of each. Removal never touches a worktree with uncommitted changes.
    fn release_worktrees(&self, worktrees: Vec<worktree::Worktree>) {
        if worktrees.is_empty() {
            return;
        }
        let me = self.me.clone();
        let policy = self.uncommitted_policy();
        let work = move || {
            for wt in worktrees {
                let root = wt.path.parent().map(Path::to_path_buf).unwrap_or_default();
                let outcome = worktree::release_with(&root, &wt, policy);
                tracing::info!(branch = %wt.branch, outcome = ?outcome, "task worktree released");
                if let Some(me) = me.upgrade() {
                    let mut state = me.lock();
                    for runtime in state.runtime.values_mut() {
                        if let Some(own) =
                            runtime.worktree.as_mut().filter(|own| own.path == wt.path)
                        {
                            own.outcome = outcome;
                        }
                    }
                    drop(state);
                    me.bump();
                }
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn_blocking(work);
            }
            Err(_) => work(),
        }
    }
    fn uncommitted_policy(&self) -> worktree::Uncommitted {
        self.manager
            .upgrade()
            .map(|manager| {
                manager
                    .launch_settings()
                    .config
                    .orchestrator
                    .worktree_uncommitted
            })
            .unwrap_or_default()
    }
    /// Take the task's worktree for release if it is still in use.
    fn take_worktree(state: &mut State, task: Uuid) -> Option<worktree::Worktree> {
        let own = state
            .runtime
            .get_mut(&task)
            .and_then(|runtime| runtime.worktree.as_mut())
            .filter(|wt| wt.outcome == worktree::Outcome::Active)?;
        let taken = own.clone();
        own.outcome = worktree::Outcome::Releasing;
        Some(taken)
    }
    fn set_state(&self, task: Uuid, to: TaskState) {
        let mut state = self.lock();
        if !state.tree.transition(task, to) && to.is_terminal() {
            state.tree.force(task, to);
        }
        // A failed, cancelled or refused task takes no follow-up, so its worktree is done with.
        // A completed one keeps it until its session ends (a follow-up may still come).
        let finished = if matches!(
            to,
            TaskState::Failed | TaskState::Canceled | TaskState::Rejected
        ) {
            Self::take_worktree(&mut state, task)
        } else {
            None
        };
        if let Some(runtime) = state.runtime.get_mut(&task) {
            runtime.updated_at = now();
            // Metadata only: the ids and the state's name, nothing the task said or did.
            tracing::info!(
                trace_id = runtime.trace_id.as_deref().unwrap_or(""),
                task = %task,
                state = records::state_name(to),
                "task state"
            );
        }
        if matches!(
            to,
            TaskState::Failed | TaskState::Canceled | TaskState::Rejected
        ) && let Some(hash) = state
            .runtime
            .get_mut(&task)
            .and_then(|runtime| runtime.token_hash.take())
        {
            state.tokens.remove(&hash);
        }
        drop(state);
        self.release_worktrees(finished.into_iter().collect());
        self.bump();
    }

    fn resolve(&self, token: &str) -> Result<Uuid, ToolFailure> {
        let state = self.lock();
        state
            .tokens
            .get(&hash(token))
            .copied()
            .filter(|task| {
                state.tree.get(*task).is_some_and(|node| {
                    !matches!(
                        node.state,
                        TaskState::Failed | TaskState::Canceled | TaskState::Rejected
                    )
                })
            })
            .ok_or_else(|| ToolFailure::new("invalid_task_token", "unknown or expired task token"))
    }

    /// One tool call from the control channel. The caller is whoever holds a live task token;
    /// arguments are validated again here because a client could bypass `mcp-delegate`.
    pub async fn call(
        self: &Arc<Self>,
        token: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<Value, ToolFailure> {
        let caller = self.resolve(token)?;
        if tool == "_hello" {
            // Sent by `acpd mcp-delegate` as it starts: the agent really did launch the tools.
            // It is not a tool an agent can call (`tools/list` never offers it).
            if let Some(runtime) = self.lock().runtime.get_mut(&caller) {
                runtime.delegate_connected = true;
            }
            self.bump();
            return Ok(json!({}));
        }
        let Value::Object(arguments) = arguments else {
            return Err(ToolFailure::new(
                "invalid_arguments",
                "arguments must be an object",
            ));
        };
        delegate::validate(tool, &arguments)?;
        match tool {
            "agents_list" => self.agents_list(caller),
            "task_delegate" => self.task_delegate(caller, &arguments).await,
            "task_status" => self.task_status(caller, &arguments).await,
            "task_result" => self.task_result(caller, &arguments),
            "task_send" => self.task_send(caller, &arguments).await,
            "task_cancel" => self.task_cancel(caller, &arguments),
            "task_decide" => self.task_decide(caller, &arguments).await,
            _ => Err(ToolFailure::new("unknown_tool", "unknown tool")),
        }
    }

    fn agents_list(&self, _caller: Uuid) -> Result<Value, ToolFailure> {
        let settings = self.manager()?.launch_settings();
        let config = &settings.config.orchestrator;
        let agents: Vec<Value> = settings
            .registry
            .discover()
            .into_iter()
            .filter(|agent| agent.enabled && config.delegable.contains(&agent.id))
            .map(|agent| {
                json!({
                    "ref": agent.id,
                    "name": agent.name,
                    "origin": "local",
                    "available": agent.installed,
                    "own_tools": agent.own_tools,
                    "delegable": true
                })
            })
            .collect();
        Ok(json!({"agents": agents}))
    }

    fn task_id(arguments: &Map<String, Value>) -> Result<Uuid, ToolFailure> {
        arguments["task_id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or_else(|| ToolFailure::new("invalid_arguments", "task_id must be a UUID"))
    }
    /// `target` must exist and be in the caller's own subtree (never the caller itself).
    fn manage(state: &State, caller: Uuid, target: Uuid) -> Result<(), ToolFailure> {
        if state.tree.get(target).is_none() {
            return Err(ToolFailure::new("unknown_task", "no such task"));
        }
        if !state.tree.may_manage(caller, target) {
            return Err(ToolFailure::new(
                "not_authorized_for_task",
                "that task is not one of your sub-tasks",
            ));
        }
        Ok(())
    }
    fn view(state: &State, id: Uuid, policy: &decisions::Policy) -> Value {
        let node = state.tree.get(id).expect("task exists");
        let mut view = json!({
            "task_id": id,
            "state": state_name(node.state),
            "agent": node.agent,
            "needs_decision": Self::needs_decision(state, id, policy)
        });
        if let Some(error) = state.runtime.get(&id).and_then(|r| r.error.as_deref()) {
            view["summary"] = json!(error);
        }
        let runtime = state.runtime.get(&id);
        if let Some(wt) = runtime.and_then(|r| r.worktree.as_ref()) {
            view["isolation"] = json!("worktree");
            view["worktree"] = worktree_view(wt);
        } else if node.parent.is_some() {
            view["isolation"] = json!("shared");
        }
        if let Some(note) = runtime.and_then(|r| r.isolation_note) {
            view["isolation_note"] = json!(note);
        }
        view
    }
    fn result_view(state: &State, id: Uuid, max_bytes: usize, policy: &decisions::Policy) -> Value {
        let mut view = Self::view(state, id, policy);
        if let Some(runtime) = state.runtime.get(&id) {
            let (text, clipped) = clip(&runtime.result, max_bytes);
            view["result"] = json!(text);
            view["truncated"] = json!(clipped || runtime.truncated);
            view["untrusted"] = json!(true);
            if let Some(reason) = &runtime.stop_reason {
                view["stop_reason"] = json!(reason);
            }
        }
        view
    }

    /// The tasks asked about, their views, and a signature that changes when a state or the set
    /// of waiting requests does (not when only a countdown ticks).
    fn status_snapshot(
        &self,
        caller: Uuid,
        arguments: &Map<String, Value>,
        policy: &decisions::Policy,
    ) -> Result<(Value, String), ToolFailure> {
        let state = self.lock();
        let ids = if arguments.contains_key("task_id") {
            let target = Self::task_id(arguments)?;
            Self::manage(&state, caller, target)?;
            vec![target]
        } else {
            state.tree.children(caller)
        };
        let mut signature = String::new();
        let mut tasks = vec![];
        for id in ids {
            let view = Self::view(&state, id, policy);
            signature.push_str(&format!(
                "{id}:{}:{}:{};",
                view["state"],
                view["needs_decision"]
                    .as_array()
                    .map_or(String::new(), |list| list
                        .iter()
                        .map(|r| format!("{}{}", r["digest"], r["decidable"]))
                        .collect::<Vec<_>>()
                        .join(",")),
                view["summary"]
            ));
            tasks.push(view);
        }
        Ok((json!({"tasks": tasks}), signature))
    }

    /// With `wait_seconds`, hold until something about these tasks changes (a state, a request
    /// appearing, being answered by anyone, or becoming undecidable) or the time runs out. It
    /// returns at once when every task has finished or a request the caller can decide is
    /// already waiting.
    async fn task_status(
        &self,
        caller: Uuid,
        arguments: &Map<String, Value>,
    ) -> Result<Value, ToolFailure> {
        let wait = arguments
            .get("wait_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let policy = self.policy()?;
        let mut changes = self.changed.subscribe();
        let (mut current, first) = self.status_snapshot(caller, arguments, &policy)?;
        // Nothing left to wait for once every task asked about has finished (or there are none).
        let all_finished = current["tasks"].as_array().is_none_or(|tasks| {
            tasks.iter().all(|task| {
                matches!(
                    task["state"].as_str(),
                    Some("completed" | "failed" | "canceled" | "rejected")
                )
            })
        });
        // A request the caller may decide right now is something to act on, not to wait out:
        // holding here would let its time run out (a real Goose supervisor lost a Medium write
        // that way when its previous poll returned a moment before the request appeared).
        // Requests it cannot decide keep the hold, or a supervisor would spin on them.
        let actionable = current["tasks"].as_array().is_some_and(|tasks| {
            tasks.iter().any(|task| {
                task["needs_decision"]
                    .as_array()
                    .is_some_and(|list| list.iter().any(|r| r["decidable"] == true))
            })
        });
        if wait == 0 || all_finished || actionable {
            return Ok(current);
        }
        let end = tokio::time::Instant::now() + Duration::from_secs(wait);
        while tokio::time::timeout_at(end, changes.changed())
            .await
            .is_ok()
        {
            let (view, signature) = self.status_snapshot(caller, arguments, &policy)?;
            current = view;
            if signature != first {
                break;
            }
        }
        Ok(current)
    }

    fn task_result(
        &self,
        caller: Uuid,
        arguments: &Map<String, Value>,
    ) -> Result<Value, ToolFailure> {
        let target = Self::task_id(arguments)?;
        let max = arguments
            .get("max_bytes")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_RESULT_BYTES, |n| n as usize);
        let policy = self.policy()?;
        let state = self.lock();
        Self::manage(&state, caller, target)?;
        Ok(Self::result_view(&state, target, max, &policy))
    }

    fn task_cancel(
        &self,
        caller: Uuid,
        arguments: &Map<String, Value>,
    ) -> Result<Value, ToolFailure> {
        let target = Self::task_id(arguments)?;
        let mut state = self.lock();
        Self::manage(&state, caller, target)?;
        let mut order = state.tree.open_descendants(target);
        if state
            .tree
            .get(target)
            .is_some_and(|n| !n.state.is_terminal())
        {
            order.push(target);
        }
        for id in &order {
            if let Some(runtime) = state.runtime.get_mut(id) {
                runtime.cancel_requested = true;
                runtime.cancel.notify_one();
            }
        }
        let policy = self.policy()?;
        let view = Self::view(&state, target, &policy);
        drop(state);
        Ok(json!({"state": view["state"], "cancelled": order}))
    }

    async fn task_send(
        self: &Arc<Self>,
        caller: Uuid,
        arguments: &Map<String, Value>,
    ) -> Result<Value, ToolFailure> {
        let target = Self::task_id(arguments)?;
        let prompt = arguments["prompt"].as_str().unwrap_or_default().to_owned();
        let manager = self.manager()?;
        let config = manager.launch_settings().config.orchestrator.clone();
        let deadline = config.default_deadline_seconds;
        let session = {
            let mut state = self.lock();
            Self::manage(&state, caller, target)?;
            // Checked before the claim, so a refused follow-up leaves the task completed.
            if config.max_turns_per_task > 0
                && state
                    .runtime
                    .get(&target)
                    .is_some_and(|r| r.turns >= config.max_turns_per_task)
            {
                return Err(ToolFailure::new(
                    "turn_limit",
                    "that task has used all the prompts it is allowed",
                ));
            }
            let session = state.runtime.get(&target).and_then(|r| r.session);
            let Some(session) = session else {
                return Err(ToolFailure::new(
                    "invalid_state",
                    "that task has no session",
                ));
            };
            // Completed -> Working is the single atomic claim, so two sends cannot both run.
            if !state.tree.transition(target, TaskState::Working) {
                return Err(ToolFailure::new(
                    "invalid_state",
                    "follow-ups are only accepted once a task has completed",
                ));
            }
            let runtime = state.runtime.get_mut(&target).expect("runtime");
            runtime.result.clear();
            runtime.truncated = false;
            runtime.stop_reason = None;
            runtime.error = None;
            runtime.cancel_requested = false;
            runtime.limit = None;
            runtime.turns += 1;
            session
        };
        self.bump();
        if manager.get(session).await.is_err() {
            self.set_state(target, TaskState::Failed);
            return Err(ToolFailure::new(
                "invalid_state",
                "the task's session is gone",
            ));
        }
        tokio::spawn(self.clone().run_prompt(
            target,
            session,
            prompt,
            Duration::from_secs(deadline),
            false,
        ));
        let policy = self.policy()?;
        let state = self.lock();
        Ok(Self::view(&state, target, &policy))
    }

    async fn task_delegate(
        self: &Arc<Self>,
        caller: Uuid,
        arguments: &Map<String, Value>,
    ) -> Result<Value, ToolFailure> {
        let manager = self.manager()?;
        let settings = manager.launch_settings();
        let agent = arguments["agent"].as_str().unwrap_or_default();
        let prompt = arguments["prompt"].as_str().unwrap_or_default().to_owned();
        // Refuse before looking anything up, so an agent cannot probe which ids exist.
        let delegable = settings
            .config
            .orchestrator
            .delegable
            .iter()
            .any(|id| id == agent)
            && settings
                .registry
                .get(agent)
                .is_ok_and(|definition| definition.enabled);
        if !delegable {
            return Err(failure(AdmissionError::AgentNotDelegable));
        }
        let (parent_scope, parent_rights) = {
            let state = self.lock();
            let node = state
                .tree
                .get(caller)
                .ok_or_else(|| ToolFailure::new("invalid_task_token", "unknown task"))?;
            (node.scope.clone(), node.rights)
        };
        let requested = match arguments.get("workspace").and_then(Value::as_str) {
            None | Some("") => parent_scope.clone(),
            Some(relative) => {
                let relative = Path::new(relative);
                if !relative
                    .components()
                    .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
                {
                    return Err(failure(AdmissionError::ScopeNotAllowed));
                }
                parent_scope.join(relative)
            }
        };
        let scope = settings.config.task_workspace(&requested).map_err(|_| {
            ToolFailure::new(
                "workspace_unavailable",
                "workspace does not exist or is not allowed",
            )
        })?;
        let rights = match arguments.get("rights").and_then(Value::as_object) {
            Some(rights) => Rights {
                read: rights.get("read").and_then(Value::as_bool).unwrap_or(false),
                write: rights
                    .get("write")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                terminal: rights
                    .get("terminal")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            None => Rights {
                read: true,
                ..Rights::default()
            },
        };
        if !rights.within(parent_rights) {
            return Err(failure(AdmissionError::RightsNotAllowed));
        }
        let access = WorkspaceAccess {
            read_files: rights.read,
            write_files: rights.write,
            terminal: rights.terminal,
            form_elicitation: false,
        };
        let deadline = arguments
            .get("deadline_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(settings.config.orchestrator.default_deadline_seconds);
        let wait = arguments
            .get("wait_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        // Worktree isolation: asked for explicitly, or the configured default for a writer.
        let explicit = arguments.get("isolation").and_then(Value::as_str) == Some("worktree");
        let isolate = explicit
            || (arguments.get("isolation").is_none()
                && rights.write
                && settings.config.orchestrator.default_isolation
                    == crate::config::DefaultIsolation::Worktree);
        let mut run_in = scope.clone();
        let mut placed = None;
        let mut note = None;
        if isolate {
            let root = settings.config.worktree_root();
            let (folder, within) = (scope.clone(), parent_scope.clone());
            let made =
                tokio::task::spawn_blocking(move || worktree::create(&root, &folder, &within))
                    .await
                    .unwrap_or(Err(worktree::Unavailable::GitFailed));
            match made {
                Ok(wt) => {
                    let folder = wt.task_folder();
                    run_in = folder.canonicalize().unwrap_or(folder);
                    self.lock()
                        .placements
                        .insert(run_in.clone(), (scope.clone(), wt.clone()));
                    placed = Some(wt);
                }
                Err(why) if explicit => {
                    return Err(ToolFailure::new("isolation_unavailable", why.reason()));
                }
                // The default only: a folder that cannot have a worktree runs shared, and the
                // task view says why.
                Err(why) => note = Some(why.reason()),
            }
        }
        let opened = manager.open_task(agent, &run_in, access, caller).await;
        if let (Err(_), Some(wt)) = (&opened, placed) {
            // Refused or failed to start: the worktree was never used.
            let unused = self.lock().placements.remove(&run_in).is_some();
            if unused {
                self.release_worktrees(vec![wt]);
            }
        }
        let session = opened.map_err(|error| match error.downcast_ref::<AdmissionFailure>() {
            Some(refused) => failure(refused.0),
            None if error.to_string().contains("session limit reached") => {
                failure(AdmissionError::SessionLimit)
            }
            None => ToolFailure::new("launch_failed", "the agent could not be started"),
        })?;
        let session_id = session.metadata().id;
        let task = self
            .lock()
            .by_session
            .get(&session_id)
            .copied()
            .ok_or_else(|| ToolFailure::new("launch_failed", "task was not registered"))?;
        if let Some(runtime) = self.lock().runtime.get_mut(&task) {
            runtime.isolation_note = note;
        }
        tokio::spawn(self.clone().run_prompt(
            task,
            session_id,
            prompt,
            Duration::from_secs(deadline),
            true,
        ));
        if settings.config.orchestrator.supervisor_max_risk != SupervisorRisk::None {
            tokio::spawn(self.clone().watch(task, session_id));
        }
        self.start_janitor();
        self.wait(task, wait).await;
        let policy = self.policy()?;
        let state = self.lock();
        Ok(Self::result_view(
            &state,
            task,
            DEFAULT_RESULT_BYTES,
            &policy,
        ))
    }

    /// Hold up to `seconds` for the task to finish or need a human, then return either way.
    async fn wait(&self, task: Uuid, seconds: u64) {
        if seconds == 0 {
            return;
        }
        let mut changes = self.changed.subscribe();
        let end = tokio::time::Instant::now() + Duration::from_secs(seconds);
        loop {
            let settled = self.lock().tree.get(task).is_none_or(|node| {
                node.state.is_terminal()
                    || matches!(
                        node.state,
                        TaskState::InputRequired | TaskState::AuthRequired
                    )
            });
            if settled
                || tokio::time::timeout_at(end, changes.changed())
                    .await
                    .is_err()
            {
                return;
            }
        }
    }

    /// Drive one prompt on a task's session and record how it ended. The task's deadline and
    /// cancellation both end the prompt with `session/cancel`; an agent that ignores it for
    /// `cancel_grace` has its session removed so no process outlives its task.
    async fn run_prompt(
        self: Arc<Self>,
        task: Uuid,
        session_id: Uuid,
        prompt: String,
        deadline: Duration,
        first: bool,
    ) {
        let Ok(manager) = self.manager() else { return };
        let Ok(session) = manager.get(session_id).await else {
            self.set_state(task, TaskState::Failed);
            return;
        };
        let cancel = {
            let state = self.lock();
            match state.runtime.get(&task) {
                Some(runtime) => (runtime.cancel.clone(), runtime.cancel_requested),
                None => return,
            }
        };
        if cancel.1 && first {
            // Cancelled before the prompt was ever sent: nothing to wait for.
            self.set_state(task, TaskState::Rejected);
            let _ = manager.remove(session_id).await;
            return;
        }
        if first {
            self.set_state(task, TaskState::Working);
        }
        let baseline = session.replay(u64::MAX).latest_sequence;
        let (idle_seconds, budget) = {
            let settings = manager.launch_settings();
            let config = &settings.config.orchestrator;
            (config.idle_timeout_seconds, config.task_max_cost_usd)
        };
        let idle = Duration::from_secs(idle_seconds);
        let mut events = session.connection.subscribe();
        let mut events_open = true;
        let mut prompting = Box::pin(session.prompt(&prompt));
        let mut interrupted = false;
        let deadline_at = tokio::time::Instant::now() + deadline;
        let mut idle_at = tokio::time::Instant::now() + idle;
        let outcome = loop {
            tokio::select! {
                result = &mut prompting => break Some(result),
                _ = tokio::time::sleep_until(deadline_at) => {
                    self.note_limit(task, Limit::Deadline);
                    break None;
                }
                _ = cancel.0.notified() => break None,
                _ = tokio::time::sleep_until(idle_at) => {
                    // A task waiting for a person is not idle.
                    if session.connection.pending_permissions().is_empty() {
                        self.note_limit(task, Limit::Idle(idle_seconds));
                        break None;
                    }
                    idle_at = tokio::time::Instant::now() + idle;
                }
                event = events.recv(), if events_open => match event {
                    Ok(event) => {
                        idle_at = tokio::time::Instant::now() + idle;
                        if let Some(spent) = usd_cost(&event.message) {
                            self.note_cost(task, spent);
                            if budget > 0.0 && spent > budget {
                                self.note_limit(task, Limit::Cost);
                                break None;
                            }
                        }
                    }
                    // Missed events still mean the agent was talking.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        idle_at = tokio::time::Instant::now() + idle;
                    }
                    Err(_) => events_open = false,
                },
            }
        };
        let outcome = match outcome {
            Some(result) => result,
            None => {
                interrupted = true;
                let _ = session.cancel().await;
                match tokio::time::timeout(self.cancel_grace, &mut prompting).await {
                    Ok(result) => result,
                    Err(_) => {
                        // The agent ignored the cancel: end its session and say why.
                        let limit = self.limit_of(task);
                        {
                            let mut state = self.lock();
                            if let Some(runtime) = state.runtime.get_mut(&task) {
                                runtime.error = limit.map(limit_text);
                            }
                        }
                        self.set_state(
                            task,
                            if limit.is_some() {
                                TaskState::Failed
                            } else {
                                TaskState::Canceled
                            },
                        );
                        drop(prompting);
                        let _ = manager.remove(session_id).await;
                        return;
                    }
                }
            }
        };
        drop(prompting);
        let replay = session.replay(baseline);
        let text = agent_text(&replay.events);
        let (stored, clipped) = clip(&text, RESULT_LIMIT);
        let stored = stored.to_owned();
        let (next, stop_reason, error, truncated) = match outcome {
            Ok(value) => {
                let reason = value["stopReason"].as_str().unwrap_or("").to_owned();
                let limit = self.limit_of(task);
                let (next, truncated) = match reason.as_str() {
                    "end_turn" => (TaskState::Completed, false),
                    "max_tokens" | "max_turn_requests" => (TaskState::Completed, true),
                    "refusal" => (TaskState::Rejected, false),
                    "cancelled" if limit.is_some() => (TaskState::Failed, false),
                    "cancelled" => (TaskState::Canceled, false),
                    _ => (TaskState::Failed, false),
                };
                let error = limit.filter(|_| next == TaskState::Failed).map(limit_text);
                (next, Some(reason), error, truncated)
            }
            Err(error) => match error.downcast_ref::<AgentRpcError>() {
                Some(AgentRpcError(-32000, _)) => (
                    TaskState::AuthRequired,
                    None,
                    Some("the agent needs authentication; sign in from the phone".to_owned()),
                    false,
                ),
                Some(AgentRpcError(code, _)) => (
                    TaskState::Failed,
                    None,
                    Some(format!("agent error {code}")),
                    false,
                ),
                None if interrupted => (
                    // Cancelled on purpose, or stopped by a limit when the connection ended.
                    match self.limit_of(task) {
                        Some(_) => TaskState::Failed,
                        None => TaskState::Canceled,
                    },
                    None,
                    self.limit_of(task).map(limit_text),
                    false,
                ),
                None => (
                    TaskState::Failed,
                    None,
                    Some(
                        if session.metadata().remote_peer.is_some() {
                            // The other host keeps the session for a while, but this prompt is
                            // never sent again on its own: it may already have had effects.
                            "the link to the other host was lost; the prompt was not resent"
                        } else {
                            "the agent connection failed"
                        }
                        .to_owned(),
                    ),
                    false,
                ),
            },
        };
        {
            let mut state = self.lock();
            if let Some(runtime) = state.runtime.get_mut(&task) {
                runtime.result = stored;
                runtime.truncated = clipped || truncated || replay.gap;
                runtime.stop_reason = stop_reason;
                runtime.error = error;
            }
        }
        self.set_state(task, next);
    }

    fn note_limit(&self, task: Uuid, limit: Limit) {
        if let Some(runtime) = self.lock().runtime.get_mut(&task) {
            runtime.limit = Some(limit);
        }
    }
    fn limit_of(&self, task: Uuid) -> Option<Limit> {
        self.lock()
            .runtime
            .get(&task)
            .and_then(|runtime| runtime.limit)
    }
    fn note_cost(&self, task: Uuid, cost: f64) {
        if let Some(runtime) = self.lock().runtime.get_mut(&task) {
            runtime.cost_usd = Some(cost);
        }
    }

    /// Start the sweep that ends the sessions of finished child tasks once they have sat for
    /// `finished_child_keep_seconds`, so they stop counting against `max_sessions`. A task keeps
    /// its record; only its session goes, so a later follow-up is refused as "session gone".
    /// Started once, by the first delegation.
    fn start_janitor(self: &Arc<Self>) {
        if self.janitor_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let me = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let Some(this) = me.upgrade() else { return };
                let Ok(manager) = this.manager() else { return };
                let keep = manager
                    .launch_settings()
                    .config
                    .orchestrator
                    .finished_child_keep_seconds;
                if keep > 0 {
                    for session in this.expired_children(keep) {
                        let _ = manager.remove(session).await;
                    }
                }
                let tick = if keep == 0 {
                    10
                } else {
                    (keep / 2).clamp(1, 10)
                };
                drop((this, manager));
                tokio::time::sleep(Duration::from_secs(tick)).await;
            }
        });
    }
    /// Sessions of child tasks that ended at least `keep` seconds ago and have nothing open
    /// below them (ending the session would end that too).
    fn expired_children(&self, keep: u64) -> Vec<Uuid> {
        let state = self.lock();
        let now = now();
        state
            .tree
            .nodes()
            .filter(|node| {
                node.parent.is_some()
                    && node.state.is_terminal()
                    && state.tree.open_descendants(node.id).is_empty()
            })
            .filter_map(|node| {
                let runtime = state.runtime.get(&node.id)?;
                let session = runtime.session?;
                (state.by_session.contains_key(&session) && runtime.updated_at + keep <= now)
                    .then_some(session)
            })
            .collect()
    }
}
