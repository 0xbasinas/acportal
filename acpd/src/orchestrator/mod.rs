//! Experimental agent orchestrator (see `docs/orchestrator-design.md`).
//!
//! `Orchestrator` owns the task tree and the per-task runtime (token, result, cancellation).
//! `SessionManager` calls [`Orchestrator::begin`] before it starts an agent (admission, and the
//! MCP server that gives the agent its delegation tools), [`Orchestrator::bind`] once the session
//! exists and [`Orchestrator::release_subtree`] when a session is removed. Agents reach the
//! orchestrator only through the control channel in [`control`], one task token per session.
//!
//! Not implemented yet: supervisor approvals (`task_decide`), worktree isolation, peers/bridge.
//! Permission requests from a delegated child go to the phone like any other session's.
pub mod approvals;
pub mod control;
mod decisions;
pub mod delegate;
pub mod tree;

use crate::{
    config::{OrchestratorConfig, SupervisorRisk},
    connection::{AgentRpcError, WorkspaceAccess},
    session::{LaunchSettings, SessionManager},
};
use delegate::ToolFailure;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::{Component, Path},
    sync::{Arc, Mutex, MutexGuard, OnceLock, Weak},
    time::Duration,
};
use tokio::sync::{Notify, watch};
use tree::{AdmissionError, Limits, NewTask, Rights, TaskState, TaskTree};
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
}

struct Runtime {
    session: Option<Uuid>,
    token_hash: Option<String>,
    result: String,
    truncated: bool,
    stop_reason: Option<String>,
    error: Option<String>,
    cancel_requested: bool,
    deadline_hit: bool,
    cancel: Arc<Notify>,
    /// Permission requests of this task still waiting for an answer, by request id.
    requests: BTreeMap<String, decisions::Request>,
    /// Recently answered request ids, so a late duplicate decision is recognized.
    answered: VecDeque<String>,
    /// Set on a root task by the phone: supervisor approvals stop for its whole tree.
    supervisor_paused: bool,
}
impl Runtime {
    fn new() -> Self {
        Self {
            session: None,
            token_hash: None,
            result: String::new(),
            truncated: false,
            stop_reason: None,
            error: None,
            cancel_requested: false,
            deadline_hit: false,
            cancel: Arc::new(Notify::new()),
            requests: BTreeMap::new(),
            answered: VecDeque::new(),
            supervisor_paused: false,
        }
    }
}

struct State {
    tree: TaskTree,
    runtime: HashMap<Uuid, Runtime>,
    /// SHA-256 of a task token to its task. Raw tokens are never stored.
    tokens: HashMap<String, Uuid>,
    by_session: HashMap<Uuid, Uuid>,
}

pub struct Orchestrator {
    manager: Weak<SessionManager>,
    state: Mutex<State>,
    changed: watch::Sender<u64>,
    control_path: OnceLock<String>,
    /// The executable that runs `mcp-delegate`. Defaults to the running host.
    delegate_executable: OnceLock<std::path::PathBuf>,
    cancel_grace: Duration,
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
    match state {
        TaskState::Submitted => "submitted",
        TaskState::Working => "working",
        TaskState::InputRequired => "input_required",
        TaskState::AuthRequired => "auth_required",
        TaskState::Completed => "completed",
        TaskState::Failed => "failed",
        TaskState::Canceled => "canceled",
        TaskState::Rejected => "rejected",
    }
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
        Arc::new(Self {
            manager,
            state: Mutex::new(State {
                tree: TaskTree::new(false, Limits::default()),
                runtime: HashMap::new(),
                tokens: HashMap::new(),
                by_session: HashMap::new(),
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
        let task = state
            .tree
            .admit(NewTask {
                parent,
                agent,
                scope,
                rights: rights_of(access),
                delegable,
            })
            .map_err(AdmissionFailure)?;
        let mut runtime = Runtime::new();
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
        state.runtime.insert(task, runtime);
        Ok(Some(Ticket { task, mcp }))
    }

    /// The session for `task` exists. Phone-started (root) tasks are working from here on.
    pub fn bind(&self, task: Uuid, session: Uuid) {
        let mut state = self.lock();
        state.by_session.insert(session, task);
        if let Some(runtime) = state.runtime.get_mut(&task) {
            runtime.session = Some(session);
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
            }
        }
        drop(state);
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
        for id in &ids {
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
        self.bump();
        sessions
    }

    fn set_state(&self, task: Uuid, to: TaskState) {
        let mut state = self.lock();
        if !state.tree.transition(task, to) && to.is_terminal() {
            state.tree.force(task, to);
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
    /// appearing, being answered by anyone, or becoming undecidable) or the time runs out.
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
        if wait == 0 || all_finished {
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
        let deadline = manager
            .launch_settings()
            .config
            .orchestrator
            .default_deadline_seconds;
        let session = {
            let mut state = self.lock();
            Self::manage(&state, caller, target)?;
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
            runtime.deadline_hit = false;
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
        if arguments.get("isolation").and_then(Value::as_str) == Some("worktree") {
            return Err(ToolFailure::new(
                "isolation_unsupported",
                "worktree isolation is not implemented yet",
            ));
        }
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
        let scope = settings.config.workspace(&requested).map_err(|_| {
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
        let session = manager
            .open_task(agent, &scope, access, caller)
            .await
            .map_err(|error| match error.downcast_ref::<AdmissionFailure>() {
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
        let mut prompting = Box::pin(session.prompt(&prompt));
        let mut interrupted = false;
        let outcome = tokio::select! {
            result = &mut prompting => Some(result),
            _ = tokio::time::sleep(deadline) => {
                if let Some(runtime) = self.lock().runtime.get_mut(&task) {
                    runtime.deadline_hit = true;
                }
                None
            }
            _ = cancel.0.notified() => None,
        };
        let outcome = match outcome {
            Some(result) => result,
            None => {
                interrupted = true;
                let _ = session.cancel().await;
                match tokio::time::timeout(self.cancel_grace, &mut prompting).await {
                    Ok(result) => result,
                    Err(_) => {
                        let hit = self
                            .lock()
                            .runtime
                            .get(&task)
                            .is_some_and(|runtime| runtime.deadline_hit);
                        self.set_state(
                            task,
                            if hit {
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
                let hit = self
                    .lock()
                    .runtime
                    .get(&task)
                    .is_some_and(|runtime| runtime.deadline_hit);
                let (next, truncated) = match reason.as_str() {
                    "end_turn" => (TaskState::Completed, false),
                    "max_tokens" | "max_turn_requests" => (TaskState::Completed, true),
                    "refusal" => (TaskState::Rejected, false),
                    "cancelled" if hit => (TaskState::Failed, false),
                    "cancelled" => (TaskState::Canceled, false),
                    _ => (TaskState::Failed, false),
                };
                let error = (hit && next == TaskState::Failed)
                    .then(|| "the task's deadline passed".to_owned());
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
                None if interrupted => (TaskState::Canceled, None, None, false),
                None => (
                    TaskState::Failed,
                    None,
                    Some("the agent connection failed".to_owned()),
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
}
