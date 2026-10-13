//! What the phone sees of the task tree (the REST routes in `api.rs`), and what it may do to it.
//!
//! The phone is the host's owner: it may look at every task and cancel any of them. That is a
//! different authority from an agent's, which reaches only its own sub-tasks through the control
//! channel, so these methods do not check subtree membership.
use super::{Orchestrator, records::TaskRecord};
use serde_json::{Value, json};
use uuid::Uuid;

/// Why a phone cancel was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum CancelRefusal {
    Unknown,
    /// The task has already ended; there is nothing to cancel.
    NotOpen,
}
/// Why a pause or resume was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum PauseRefusal {
    Unknown,
    /// Only the root of a task tree (a phone-started session) can be paused.
    NotARoot,
}

/// `connected` once an `mcp-delegate` process has said hello for a task whose agent was offered
/// the delegation tools, `not_connected` while it has not (the agent may not have started it yet,
/// or it never will). Nothing for a task that was not offered the tools.
fn delegate_label(runtime: &super::Runtime) -> Option<&'static str> {
    runtime
        .delegate_offered
        .then_some(if runtime.delegate_connected {
            "connected"
        } else {
            "not_connected"
        })
}

/// The agent-facing `needs_decision` entry in the phone's camelCase.
fn phone_request(entry: &Value) -> Value {
    let mut out = json!({
        "requestId": entry["request_id"],
        "digest": entry["digest"],
        "source": entry["source"],
        "kind": entry["kind"],
        "risk": entry["risk"],
        "facts": entry["facts"],
        "decidable": entry["decidable"],
        "expiresInSeconds": entry["expires_in_seconds"],
    });
    for (from, to) in [("reason", "reason"), ("note", "note")] {
        if !entry[from].is_null() {
            out[to] = entry[from].clone();
        }
    }
    out
}

impl Orchestrator {
    fn record_json(record: &TaskRecord) -> Value {
        serde_json::to_value(record).unwrap_or(Value::Null)
    }

    /// Every task, oldest first, each with how many requests are waiting on a decision.
    pub fn task_list(&self) -> Vec<Value> {
        let records = self.tasks();
        let state = self.lock();
        records
            .iter()
            .map(|record| {
                let waiting = state.runtime.get(&record.id).map_or(0, |runtime| {
                    runtime
                        .requests
                        .values()
                        .filter(|request| request.class.denied.is_none())
                        .count()
                });
                let mut view = Self::record_json(record);
                view["pendingDecisions"] = json!(waiting);
                if let Some(label) = state.runtime.get(&record.id).and_then(delegate_label) {
                    view["delegate"] = json!(label);
                }
                view
            })
            .collect()
    }

    /// One task with the requests waiting on a decision, or `None` if there is no such task.
    pub fn task_detail(&self, id: Uuid) -> Option<Value> {
        let policy = self.policy().ok()?;
        let record = self.tasks().into_iter().find(|record| record.id == id)?;
        let state = self.lock();
        let waiting: Vec<Value> = Self::needs_decision(&state, id, &policy)
            .iter()
            .map(phone_request)
            .collect();
        let mut view = Self::record_json(&record);
        view["needsDecision"] = json!(waiting);
        if let Some(label) = state.runtime.get(&id).and_then(delegate_label) {
            view["delegate"] = json!(label);
        }
        Some(view)
    }

    /// Cancel a task and everything below it that is still open. Returns the tasks asked to
    /// stop. For the root of a tree (a phone-started session) that means its open descendants
    /// and the root session's current turn; the root session itself stays.
    pub async fn cancel_task(&self, id: Uuid) -> Result<Vec<Uuid>, CancelRefusal> {
        let (order, root_session) = {
            let mut state = self.lock();
            let node = state.tree.get(id).ok_or(CancelRefusal::Unknown)?.clone();
            let is_root = node.parent.is_none();
            let mut order = state.tree.open_descendants(id);
            if !node.state.is_terminal() && !is_root {
                order.push(id);
            }
            // Nothing open below, and no live root turn to cancel: nothing to do.
            if order.is_empty() && (node.state.is_terminal() || !is_root) {
                return Err(CancelRefusal::NotOpen);
            }
            for task in &order {
                if let Some(runtime) = state.runtime.get_mut(task) {
                    runtime.cancel_requested = true;
                    runtime.cancel.notify_one();
                }
            }
            let root_session = is_root
                .then(|| state.runtime.get(&id).and_then(|runtime| runtime.session))
                .flatten();
            (order, root_session)
        };
        if let (Some(session), Ok(manager)) = (root_session, self.manager())
            && let Ok(session) = manager.get(session).await
        {
            let _ = session.cancel().await;
        }
        self.bump();
        Ok(order)
    }

    /// Stop or restore supervisor approvals for a tree, named by its root task.
    pub fn set_supervisor_paused_for_task(
        &self,
        id: Uuid,
        paused: bool,
    ) -> Result<(), PauseRefusal> {
        let mut state = self.lock();
        let node = state.tree.get(id).ok_or(PauseRefusal::Unknown)?;
        if node.parent.is_some() {
            return Err(PauseRefusal::NotARoot);
        }
        let runtime = state.runtime.get_mut(&id).ok_or(PauseRefusal::Unknown)?;
        runtime.supervisor_paused = paused;
        drop(state);
        self.bump();
        Ok(())
    }
}
