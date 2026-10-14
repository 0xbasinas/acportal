//! Supervisor approvals for delegated tasks (design section 8, flow in 8.2).
//!
//! A watcher per delegated child notices each pending permission request, classifies it and
//! either refuses it (denied), leaves it to people (human-only, high risk, over the ceiling) or
//! offers it to the supervising agent as `needs_decision`. The phone always sees every request;
//! the supervisor is one more party who may answer, and the first valid answer wins because the
//! connection answers a request at most once. Every failure path leaves the request pending for
//! the phone: nothing here ever allows by default.
use super::{
    Orchestrator, State,
    approvals::{self, Classified},
    delegate::ToolFailure,
    tree::TaskState,
};
use crate::{config::SupervisorRisk, session::AcpSession};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Most answered request ids remembered per task, so a late duplicate decision is reported as
/// `already_answered` rather than `unknown_request`.
const ANSWERED_KEPT: usize = 256;

pub(super) struct Request {
    pub id: Value,
    pub class: Classified,
    pub digest: String,
    pub seen: Instant,
    pub escalated: bool,
    pub tool_call_id: Option<String>,
}

/// The settings a decision depends on, read once per call from the current launch snapshot.
pub(super) struct Policy {
    pub ceiling: SupervisorRisk,
    pub timeout: Duration,
    pub max_edit: usize,
}

/// A request's id as the supervisor sees and quotes it.
pub(super) fn request_text(id: &Value) -> String {
    match id {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Why the supervisor may not decide this request right now, if it may not.
fn why_not(request: &Request, policy: &Policy, paused: bool) -> Option<&'static str> {
    if request.class.denied.is_some() {
        Some("denied")
    } else if request.class.human_only.is_some() {
        Some("human_only")
    } else if request.escalated {
        Some("escalated")
    } else if policy.ceiling == SupervisorRisk::None {
        Some("supervisor_disabled")
    } else if paused {
        Some("paused")
    } else if !request.class.risk.within(policy.ceiling) {
        Some("risk_too_high")
    } else if request.seen.elapsed() >= policy.timeout {
        Some("expired")
    } else {
        None
    }
}
fn code_for(reason: &str) -> ToolFailure {
    match reason {
        "denied" => ToolFailure::new("not_decidable", "this request is refused by policy"),
        "human_only" => ToolFailure::new("not_decidable", "only a person can answer this request"),
        "escalated" => ToolFailure::new("escalated", "this request was already sent to the phone"),
        "supervisor_disabled" => {
            ToolFailure::new("supervisor_disabled", "supervisor approvals are turned off")
        }
        "paused" => ToolFailure::new("supervisor_paused", "supervisor approvals are paused"),
        "risk_too_high" => ToolFailure::new(
            "risk_too_high",
            "this request is above the supervisor's limit",
        ),
        _ => ToolFailure::new(
            "supervisor_timeout",
            "the supervisor's time to answer has passed; the phone decides",
        ),
    }
}

fn remember(runtime: &mut super::Runtime, key: String) {
    runtime.answered.retain(|known| *known != key);
    runtime.answered.push_back(key);
    while runtime.answered.len() > ANSWERED_KEPT {
        runtime.answered.pop_front();
    }
}

/// Tell the phone what happened to a request, in the shape it already renders for automatic
/// reviews. The reason is bounded, control characters removed; it is never logged.
fn annotate(
    session: &AcpSession,
    tool_call_id: Option<&str>,
    decision: &str,
    layer: &str,
    reason: &str,
) {
    let Some(tool_call_id) = tool_call_id else {
        return;
    };
    session.connection.record(
        json!({"jsonrpc":"2.0","method":"session/update","params":{
            "sessionId": session.metadata().acp_session_id,
            "update": {"sessionUpdate":"tool_call_update","toolCallId":tool_call_id,
                "_meta":{"acpdAutoReview":{"decision":decision,"layer":layer,
                    "reason":approvals::clean(reason, 400)}}}}}),
        "host",
    );
}

impl Orchestrator {
    pub(super) fn policy(&self) -> Result<Policy, ToolFailure> {
        let settings = self.manager()?.launch_settings();
        let config = &settings.config.orchestrator;
        Ok(Policy {
            ceiling: config.supervisor_max_risk,
            timeout: Duration::from_secs(config.supervisor_timeout_seconds),
            max_edit: config.max_edit_bytes,
        })
    }

    /// Stop (or restore) supervisor approvals for the tree rooted at a phone-started session.
    /// Pending requests stay pending for the phone either way. False if the session is not the
    /// root of a task tree.
    pub fn set_supervisor_paused(&self, session: Uuid, paused: bool) -> bool {
        let mut state = self.lock();
        let Some(task) = state.by_session.get(&session).copied() else {
            return false;
        };
        if state
            .tree
            .get(task)
            .is_none_or(|node| node.parent.is_some())
        {
            return false;
        }
        let Some(runtime) = state.runtime.get_mut(&task) else {
            return false;
        };
        runtime.supervisor_paused = paused;
        drop(state);
        self.bump();
        true
    }

    /// The `needs_decision` list for one task: every request still waiting, with whether the
    /// supervisor may decide it now and, if not, why.
    pub(super) fn needs_decision(state: &State, task: Uuid, policy: &Policy) -> Vec<Value> {
        let Some(node) = state.tree.get(task) else {
            return vec![];
        };
        let paused = state
            .runtime
            .get(&node.root)
            .is_some_and(|runtime| runtime.supervisor_paused);
        let Some(runtime) = state.runtime.get(&task) else {
            return vec![];
        };
        runtime
            .requests
            .iter()
            .filter(|(_, request)| request.class.denied.is_none())
            .map(|(id, request)| {
                let reason = why_not(request, policy, paused);
                let remaining = policy.timeout.saturating_sub(request.seen.elapsed());
                let mut view = json!({
                    "request_id": id,
                    "digest": request.digest,
                    "source": request.class.source,
                    "kind": request.class.kind,
                    "risk": request.class.risk.name(),
                    "facts": request.class.facts,
                    "decidable": reason.is_none(),
                    "expires_in_seconds": remaining.as_secs(),
                });
                if let Some(reason) = reason {
                    view["reason"] = json!(reason);
                }
                if let Some(note) = request.class.human_only {
                    view["note"] = json!(note);
                }
                view
            })
            .collect()
    }

    /// Notice a delegated child's permission requests until the task ends or its session closes.
    pub(super) async fn watch(self: Arc<Self>, task: Uuid, session_id: Uuid) {
        let Ok(manager) = self.manager() else { return };
        let Ok(session) = manager.get(session_id).await else {
            return;
        };
        let mut events = session.connection.subscribe();
        loop {
            let finished = {
                let state = self.lock();
                !state.runtime.contains_key(&task)
                    || state.tree.get(task).is_none_or(|node| {
                        matches!(
                            node.state,
                            TaskState::Failed | TaskState::Canceled | TaskState::Rejected
                        )
                    })
            };
            if finished || session.connection.closed_reason().is_some() {
                return;
            }
            self.scan(task, &session).await;
            tokio::select! {
                result = events.recv() => {
                    if matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)) {
                        return;
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    }

    /// Register new pending requests, forget answered ones, keep the task's state honest and
    /// refuse what policy denies.
    async fn scan(&self, task: Uuid, session: &Arc<AcpSession>) {
        let Ok(policy) = self.policy() else { return };
        let pending = session.connection.pending_permissions();
        let metadata = session.metadata();
        let overrides = super::overrides::SessionOverrides::from_registry(metadata.own_tools);
        let remote = metadata.remote_peer.is_some();
        let mut refuse: Vec<(Value, Classified)> = vec![];
        let mut changed = false;
        {
            let mut state = self.lock();
            let Some(node) = state.tree.get(task).cloned() else {
                return;
            };
            let present: BTreeSet<String> = pending
                .iter()
                .map(|p| request_text(&p.message["id"]))
                .collect();
            let Some(runtime) = state.runtime.get_mut(&task) else {
                return;
            };
            let gone: Vec<String> = runtime
                .requests
                .keys()
                .filter(|key| !present.contains(*key))
                .cloned()
                .collect();
            for key in gone {
                runtime.requests.remove(&key);
                remember(runtime, key);
                changed = true;
            }
            for permission in &pending {
                let key = request_text(&permission.message["id"]);
                if runtime.requests.contains_key(&key) {
                    continue;
                }
                let class = approvals::classify(
                    &permission.message,
                    permission.host_origin,
                    &node.scope,
                    overrides.own_tools,
                    remote,
                    policy.max_edit,
                );
                let digest = approvals::digest(task, &key, &class, &permission.message);
                if class.denied.is_some() {
                    refuse.push((permission.message.clone(), class.clone()));
                }
                runtime.requests.insert(
                    key,
                    Request {
                        id: permission.message["id"].clone(),
                        digest,
                        seen: Instant::now(),
                        escalated: false,
                        tool_call_id: permission.message["params"]["toolCall"]["toolCallId"]
                            .as_str()
                            .map(str::to_owned),
                        class,
                    },
                );
                changed = true;
            }
            let waiting = runtime
                .requests
                .values()
                .any(|request| request.class.denied.is_none());
            if waiting && node.state == TaskState::Working {
                state.tree.transition(task, TaskState::InputRequired);
                changed = true;
            } else if !waiting && node.state == TaskState::InputRequired {
                state.tree.transition(task, TaskState::Working);
                changed = true;
            }
        }
        for (message, class) in refuse {
            let outcome = match &class.reject_option {
                Some(option) => json!({"outcome":"selected","optionId":option}),
                None => json!({"outcome":"cancelled"}),
            };
            if session
                .connection
                .permission(message["id"].clone(), outcome)
                .await
                .is_ok()
            {
                annotate(
                    session,
                    message["params"]["toolCall"]["toolCallId"].as_str(),
                    "deny",
                    "rules",
                    class.denied.unwrap_or("refused by policy"),
                );
                tracing::info!(
                    decision = "deny",
                    layer = "rules",
                    risk = class.risk.name(),
                    "delegated task permission refused"
                );
            }
        }
        if changed {
            self.bump();
        }
    }

    pub(super) async fn task_decide(
        self: &Arc<Self>,
        caller: Uuid,
        arguments: &Map<String, Value>,
    ) -> Result<Value, ToolFailure> {
        let target = Self::task_id(arguments)?;
        let request_id = arguments["request_id"].as_str().unwrap_or_default();
        let digest = arguments["digest"].as_str().unwrap_or_default();
        let decision = arguments["decision"].as_str().unwrap_or_default();
        let rationale = approvals::clean(arguments["rationale"].as_str().unwrap_or_default(), 500);
        let policy = self.policy()?;

        struct Plan {
            session: Uuid,
            id: Value,
            option: Option<String>,
            tool_call_id: Option<String>,
            risk: &'static str,
        }
        let plan = {
            let mut state = self.lock();
            Self::manage(&state, caller, target)?;
            let root = state.tree.get(target).map_or(target, |node| node.root);
            let paused = state
                .runtime
                .get(&root)
                .is_some_and(|runtime| runtime.supervisor_paused);
            let runtime = state
                .runtime
                .get_mut(&target)
                .ok_or_else(|| ToolFailure::new("unknown_task", "no such task"))?;
            let session = runtime
                .session
                .ok_or_else(|| ToolFailure::new("invalid_state", "that task has no session"))?;
            let known_answered = runtime.answered.iter().any(|key| key == request_id);
            let Some(request) = runtime.requests.get_mut(request_id) else {
                return Err(if known_answered {
                    ToolFailure::new("already_answered", "that request was already answered")
                } else {
                    ToolFailure::new("unknown_request", "no such pending request")
                });
            };
            if request.digest != digest {
                return Err(ToolFailure::new(
                    "digest_mismatch",
                    "the request changed or the digest is wrong; read task_status again",
                ));
            }
            if let Some(reason) = why_not(request, &policy, paused) {
                return Err(code_for(reason));
            }
            let option = match decision {
                "allow_once" => request.class.allow_option.clone(),
                "deny" => request.class.reject_option.clone(),
                _ => {
                    request.escalated = true;
                    None
                }
            };
            if decision != "escalate" && option.is_none() {
                return Err(ToolFailure::new(
                    "option_unavailable",
                    "the request offers no matching option; the phone decides",
                ));
            }
            Plan {
                session,
                id: request.id.clone(),
                option,
                tool_call_id: request.tool_call_id.clone(),
                risk: request.class.risk.name(),
            }
        };
        let manager = self.manager()?;
        let session = manager
            .get(plan.session)
            .await
            .map_err(|_| ToolFailure::new("invalid_state", "the task's session is gone"))?;
        let Some(option) = plan.option else {
            // Escalate: nothing is answered; the phone gets the supervisor's reasoning.
            annotate(
                &session,
                plan.tool_call_id.as_deref(),
                "escalate",
                "supervisor",
                &rationale,
            );
            tracing::info!(
                decision = "escalate",
                layer = "supervisor",
                risk = plan.risk,
                "supervisor decision"
            );
            self.bump();
            return Ok(json!({"applied": false, "outcome": "escalated"}));
        };
        match session
            .connection
            .permission(
                plan.id.clone(),
                json!({"outcome":"selected","optionId":option}),
            )
            .await
        {
            Ok(()) => {
                {
                    let mut state = self.lock();
                    if let Some(runtime) = state.runtime.get_mut(&target) {
                        runtime.requests.remove(request_id);
                        remember(runtime, request_id.to_owned());
                    }
                }
                annotate(
                    &session,
                    plan.tool_call_id.as_deref(),
                    decision,
                    "supervisor",
                    &rationale,
                );
                tracing::info!(
                    decision,
                    layer = "supervisor",
                    risk = plan.risk,
                    "supervisor decision"
                );
                self.bump();
                Ok(
                    json!({"applied": true, "outcome": if decision == "allow_once" { "allowed" } else { "denied" }}),
                )
            }
            // The phone (or a racing decision) answered first: it won, nothing else happens.
            Err(error) if error.to_string().contains("not pending") => Err(ToolFailure::new(
                "already_answered",
                "that request was answered first by someone else",
            )),
            Err(_) => Err(ToolFailure::new(
                "answer_failed",
                "the answer could not be delivered; the phone decides",
            )),
        }
    }
}
