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
/// Why a phone merge of a task's worktree branch was refused. Nothing was changed.
#[derive(Debug, PartialEq, Eq)]
pub enum MergeTaskRefusal {
    Unknown,
    /// Only a finished task (completed, failed, cancelled, refused) can be merged.
    NotFinished,
    /// The task did not run in a worktree.
    NoWorktree,
    /// The worktree is being released, or its cleanup failed; try again later.
    Busy,
    /// The worktree still has uncommitted work (kept with `worktree_uncommitted = "keep"`).
    WorktreeDirty,
    AlreadyMerged,
    Git(super::worktree::MergeRefusal),
}
impl MergeTaskRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unknown => "task_not_found",
            Self::NotFinished => "task_not_finished",
            Self::NoWorktree => "no_worktree",
            Self::Busy => "worktree_busy",
            Self::WorktreeDirty => "worktree_dirty",
            Self::AlreadyMerged => "already_merged",
            Self::Git(refusal) => refusal.code(),
        }
    }
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

    /// Merge a finished task's worktree branch into the checkout it was made from (the parent's
    /// folder). A completed task's worktree, still kept for follow-ups, is released first (its
    /// uncommitted work is committed to the branch); after a merge the task takes no follow-ups.
    pub async fn merge_task(&self, id: Uuid) -> Result<Value, MergeTaskRefusal> {
        use super::worktree::{self, Outcome};
        let (wt, release_first) = {
            let mut state = self.lock();
            let node = state.tree.get(id).ok_or(MergeTaskRefusal::Unknown)?;
            if !node.state.is_terminal() {
                return Err(MergeTaskRefusal::NotFinished);
            }
            let own = state
                .runtime
                .get(&id)
                .and_then(|runtime| runtime.worktree.clone())
                .ok_or(MergeTaskRefusal::NoWorktree)?;
            match own.outcome {
                Outcome::Active => (
                    Self::take_worktree(&mut state, id).ok_or(MergeTaskRefusal::Busy)?,
                    true,
                ),
                Outcome::BranchKept | Outcome::Snapshotted => {
                    // Claimed while git runs, so two merges cannot race.
                    Self::set_worktree_outcome(&mut state, id, Outcome::Releasing);
                    (own, false)
                }
                Outcome::Merged => return Err(MergeTaskRefusal::AlreadyMerged),
                Outcome::KeptDirty => return Err(MergeTaskRefusal::WorktreeDirty),
                Outcome::Removed => {
                    return Err(MergeTaskRefusal::Git(
                        worktree::MergeRefusal::NothingToMerge,
                    ));
                }
                Outcome::Releasing | Outcome::CleanupFailed => {
                    return Err(MergeTaskRefusal::Busy);
                }
            }
        };
        let root = wt
            .path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        // The worktree of a completed task is still on disk (and on Windows locked while its
        // agent runs): commit its uncommitted work there, merge, and remove the folder after.
        let job = wt.clone();
        let result = tokio::task::spawn_blocking(move || {
            if release_first && !worktree::snapshot_if_dirty(&root, &job) {
                return Err(worktree::MergeRefusal::GitFailed);
            }
            let merged = worktree::merge(&root, &job)?;
            if release_first {
                // Not waited for: on Windows the folder may stay locked until the task's agent
                // process ends.
                std::thread::spawn(move || worktree::discard_merged(&root, &job));
            }
            Ok(merged)
        })
        .await
        .unwrap_or(Err(worktree::MergeRefusal::GitFailed));
        let mut state = self.lock();
        let answer = match result {
            Ok(commit) => {
                if let Some(own) = state.runtime.get_mut(&id).and_then(|r| r.worktree.as_mut()) {
                    own.outcome = Outcome::Merged;
                    own.merged = Some(commit.clone());
                }
                Ok(json!({"merged": commit, "branch": wt.branch, "into": wt.repository}))
            }
            Err(refusal) => {
                // Nothing merged: the worktree (if still on disk) stays as it was.
                Self::set_worktree_outcome(&mut state, id, wt.outcome);
                Err(MergeTaskRefusal::Git(refusal))
            }
        };
        drop(state);
        self.bump();
        answer
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
