//! Task tree: lineage, limits, admission checks and the state machine (design sections 5 and 5.1).
//! Pure in-memory logic; nothing here starts a process or touches the network.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    AuthRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
}
impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Canceled | Self::Rejected
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rights {
    pub read: bool,
    pub write: bool,
    pub terminal: bool,
}
impl Rights {
    pub fn within(self, parent: Rights) -> bool {
        (!self.read || parent.read)
            && (!self.write || parent.write)
            && (!self.terminal || parent.terminal)
    }
}

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_depth: u8,
    pub max_children_per_task: usize,
    pub max_tasks_total: usize,
    pub max_sessions: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 2,
            max_children_per_task: 4,
            max_tasks_total: 8,
            max_sessions: 8,
        }
    }
}

/// Stable codes from design section 5.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    OrchestratorDisabled,
    DepthLimit,
    FanOutLimit,
    AgentNotDelegable,
    Cycle,
    ScopeNotAllowed,
    RightsNotAllowed,
    SessionLimit,
    /// The delegating task has already finished.
    ParentNotOpen,
}
impl AdmissionError {
    pub fn code(self) -> &'static str {
        match self {
            Self::OrchestratorDisabled => "orchestrator_disabled",
            Self::DepthLimit => "depth_limit",
            Self::FanOutLimit => "fan_out_limit",
            Self::AgentNotDelegable => "agent_not_delegable",
            Self::Cycle => "cycle",
            Self::ScopeNotAllowed => "scope_not_allowed",
            Self::RightsNotAllowed => "rights_not_allowed",
            Self::SessionLimit => "session_limit",
            Self::ParentNotOpen => "invalid_state",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TaskNode {
    pub id: Uuid,
    pub parent: Option<Uuid>,
    pub root: Uuid,
    pub depth: u8,
    pub agent: String,
    pub scope: PathBuf,
    pub rights: Rights,
    pub state: TaskState,
}

pub struct NewTask<'a> {
    pub parent: Option<Uuid>,
    pub agent: &'a str,
    pub scope: &'a Path,
    pub rights: Rights,
    pub delegable: bool,
}

pub struct TaskTree {
    enabled: bool,
    limits: Limits,
    nodes: HashMap<Uuid, TaskNode>,
    /// Live host sessions not owned by the orchestrator, counted against `max_sessions`.
    other_sessions: usize,
}

impl TaskTree {
    pub fn new(enabled: bool, limits: Limits) -> Self {
        Self {
            enabled,
            limits,
            nodes: HashMap::new(),
            other_sessions: 0,
        }
    }
    /// Limits and the enabled flag apply to admissions from now on; existing tasks keep running.
    pub fn configure(&mut self, enabled: bool, limits: Limits) {
        self.enabled = enabled;
        self.limits = limits;
    }
    pub fn set_other_sessions(&mut self, count: usize) {
        self.other_sessions = count;
    }
    pub fn get(&self, id: Uuid) -> Option<&TaskNode> {
        self.nodes.get(&id)
    }

    fn open_count(&self) -> usize {
        self.nodes
            .values()
            .filter(|node| !node.state.is_terminal())
            .count()
    }
    fn is_ancestor_or_self_of(&self, ancestor: Uuid, mut node: Uuid) -> bool {
        loop {
            if node == ancestor {
                return true;
            }
            match self.nodes.get(&node).and_then(|n| n.parent) {
                Some(parent) => node = parent,
                None => return false,
            }
        }
    }

    /// Run the section 5.1 checks, then insert a `Submitted` node. On error nothing is inserted.
    /// A root task (no parent) is bounded by limits but has no parent scope or rights to inherit.
    pub fn admit(&mut self, new: NewTask<'_>) -> Result<Uuid, AdmissionError> {
        if !self.enabled {
            return Err(AdmissionError::OrchestratorDisabled);
        }
        let parent = match new.parent {
            Some(id) => Some(self.nodes.get(&id).ok_or(AdmissionError::ScopeNotAllowed)?),
            None => None,
        };
        if parent.is_some_and(|p| p.state.is_terminal()) {
            return Err(AdmissionError::ParentNotOpen);
        }
        let depth = parent.map_or(0, |p| p.depth + 1);
        if depth > self.limits.max_depth {
            return Err(AdmissionError::DepthLimit);
        }
        if let Some(parent) = parent {
            let children = self
                .nodes
                .values()
                .filter(|n| n.parent == Some(parent.id) && !n.state.is_terminal())
                .count();
            if children >= self.limits.max_children_per_task {
                return Err(AdmissionError::FanOutLimit);
            }
        }
        if self.open_count() >= self.limits.max_tasks_total {
            return Err(AdmissionError::FanOutLimit);
        }
        if !new.delegable {
            return Err(AdmissionError::AgentNotDelegable);
        }
        if let Some(parent) = parent {
            // Same agent id in the same workspace already on the ancestor chain.
            let mut cursor = Some(parent.id);
            while let Some(id) = cursor {
                let node = &self.nodes[&id];
                if node.agent == new.agent && node.scope == new.scope {
                    return Err(AdmissionError::Cycle);
                }
                cursor = node.parent;
            }
            // `Path::starts_with` compares whole components, so `/a/bc` is not inside `/a/b`.
            if !new.scope.starts_with(&parent.scope)
                || new
                    .scope
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(AdmissionError::ScopeNotAllowed);
            }
            if !new.rights.within(parent.rights) {
                return Err(AdmissionError::RightsNotAllowed);
            }
        }
        if self.other_sessions + self.open_count() >= self.limits.max_sessions {
            return Err(AdmissionError::SessionLimit);
        }
        let id = Uuid::new_v4();
        let root = parent.map_or(id, |p| p.root);
        self.nodes.insert(
            id,
            TaskNode {
                id,
                parent: new.parent,
                root,
                depth,
                agent: new.agent.to_owned(),
                scope: new.scope.to_owned(),
                rights: new.rights,
                state: TaskState::Submitted,
            },
        );
        Ok(id)
    }

    /// Apply a state change if the design's transition table allows it; otherwise return `false`
    /// and leave the node unchanged.
    pub fn transition(&mut self, id: Uuid, to: TaskState) -> bool {
        use TaskState::*;
        let Some(node) = self.nodes.get_mut(&id) else {
            return false;
        };
        let allowed = matches!(
            (node.state, to),
            (Submitted, Working | Rejected)
                | (Working, InputRequired | Completed | Rejected | Failed)
                | (InputRequired, Working | Failed)
                | (Working | InputRequired, AuthRequired | Canceled)
                | (Completed, Working)
        );
        if allowed {
            node.state = to;
        }
        allowed
    }

    /// End an open task regardless of the transition table (cancel watchdog, deadline, session
    /// removal). Only terminal states are accepted; a finished task is left unchanged.
    pub fn force(&mut self, id: Uuid, to: TaskState) -> bool {
        debug_assert!(to.is_terminal());
        match self.nodes.get_mut(&id) {
            Some(node) if !node.state.is_terminal() && to.is_terminal() => {
                node.state = to;
                true
            }
            _ => false,
        }
    }
    /// Direct children of `id`, oldest-agnostic, in no particular order.
    pub fn children(&self, id: Uuid) -> Vec<Uuid> {
        self.nodes
            .values()
            .filter(|n| n.parent == Some(id))
            .map(|n| n.id)
            .collect()
    }
    /// `id` and every descendant in any state, deepest first.
    pub fn subtree(&self, id: Uuid) -> Vec<Uuid> {
        let mut found: Vec<&TaskNode> = self
            .nodes
            .values()
            .filter(|n| self.is_ancestor_or_self_of(id, n.id))
            .collect();
        found.sort_by_key(|n| std::cmp::Reverse(n.depth));
        found.into_iter().map(|n| n.id).collect()
    }
    /// Forget a whole task tree once its root session is gone.
    pub fn remove_tree(&mut self, root: Uuid) {
        self.nodes.retain(|_, node| node.root != root);
    }
    /// Every open descendant of `id` (excluding `id`), deepest first, for cascading cancel.
    pub fn open_descendants(&self, id: Uuid) -> Vec<Uuid> {
        let mut found: Vec<&TaskNode> = self
            .nodes
            .values()
            .filter(|n| {
                n.id != id && !n.state.is_terminal() && self.is_ancestor_or_self_of(id, n.id)
            })
            .collect();
        found.sort_by_key(|n| std::cmp::Reverse(n.depth));
        found.into_iter().map(|n| n.id).collect()
    }

    /// Whether `caller` may act on `target`: the target must be in the caller's own subtree and
    /// not the caller itself (design section 8: no self-approval, no cross-subtree decisions).
    pub fn may_manage(&self, caller: Uuid, target: Uuid) -> bool {
        caller != target && self.is_ancestor_or_self_of(caller, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Rights = Rights {
        read: true,
        write: true,
        terminal: true,
    };
    const READ: Rights = Rights {
        read: true,
        write: false,
        terminal: false,
    };

    fn tree() -> TaskTree {
        TaskTree::new(true, Limits::default())
    }
    fn root_of(t: &mut TaskTree, agent: &str, scope: &str, rights: Rights) -> Uuid {
        t.admit(NewTask {
            parent: None,
            agent,
            scope: Path::new(scope),
            rights,
            delegable: true,
        })
        .unwrap()
    }
    fn root(t: &mut TaskTree) -> Uuid {
        root_of(t, "a", "/w", ALL)
    }
    fn child(
        t: &mut TaskTree,
        parent: Uuid,
        agent: &str,
        scope: &str,
        rights: Rights,
    ) -> Result<Uuid, AdmissionError> {
        t.admit(NewTask {
            parent: Some(parent),
            agent,
            scope: Path::new(scope),
            rights,
            delegable: true,
        })
    }

    #[test]
    fn disabled_orchestrator_rejects_everything() {
        let mut t = TaskTree::new(false, Limits::default());
        let r = t.admit(NewTask {
            parent: None,
            agent: "a",
            scope: Path::new("/w"),
            rights: ALL,
            delegable: true,
        });
        assert_eq!(r, Err(AdmissionError::OrchestratorDisabled));
    }

    #[test]
    fn depth_fan_out_and_total_limits() {
        let mut t = tree();
        let r = root(&mut t);
        let c1 = child(&mut t, r, "b", "/w", ALL).unwrap();
        let c2 = child(&mut t, c1, "c", "/w", ALL).unwrap();
        assert_eq!(
            child(&mut t, c2, "d", "/w", ALL),
            Err(AdmissionError::DepthLimit)
        );

        let mut t = TaskTree::new(
            true,
            Limits {
                max_children_per_task: 2,
                ..Limits::default()
            },
        );
        let r = root(&mut t);
        child(&mut t, r, "b", "/w", ALL).unwrap();
        child(&mut t, r, "c", "/w", ALL).unwrap();
        assert_eq!(
            child(&mut t, r, "d", "/w", ALL),
            Err(AdmissionError::FanOutLimit)
        );

        let mut t = TaskTree::new(
            true,
            Limits {
                max_tasks_total: 2,
                ..Limits::default()
            },
        );
        let r = root(&mut t);
        child(&mut t, r, "b", "/w", ALL).unwrap();
        assert_eq!(
            child(&mut t, r, "c", "/w", ALL),
            Err(AdmissionError::FanOutLimit)
        );
    }

    #[test]
    fn finished_children_free_fan_out_capacity() {
        let mut t = TaskTree::new(
            true,
            Limits {
                max_children_per_task: 1,
                ..Limits::default()
            },
        );
        let r = root(&mut t);
        let c = child(&mut t, r, "b", "/w", ALL).unwrap();
        assert!(t.transition(c, TaskState::Rejected));
        child(&mut t, r, "c", "/w", ALL).unwrap();
    }

    #[test]
    fn not_delegable_cycle_scope_rights_and_session_limit() {
        let mut t = tree();
        let r = root(&mut t);
        let denied = t.admit(NewTask {
            parent: Some(r),
            agent: "x",
            scope: Path::new("/w"),
            rights: READ,
            delegable: false,
        });
        assert_eq!(denied, Err(AdmissionError::AgentNotDelegable));
        // Same agent in the same workspace as an ancestor.
        assert_eq!(
            child(&mut t, r, "a", "/w", READ),
            Err(AdmissionError::Cycle)
        );
        // Same agent, deeper workspace, is allowed.
        child(&mut t, r, "a", "/w/sub", READ).unwrap();
        // Sibling-prefix and parent-dir escapes are outside scope.
        assert_eq!(
            child(&mut t, r, "b", "/wx", READ),
            Err(AdmissionError::ScopeNotAllowed)
        );
        assert_eq!(
            child(&mut t, r, "b", "/w/../etc", READ),
            Err(AdmissionError::ScopeNotAllowed)
        );

        let narrow = root_of(&mut t, "n", "/n", READ);
        assert_eq!(
            child(&mut t, narrow, "b", "/n", ALL),
            Err(AdmissionError::RightsNotAllowed)
        );

        let mut t = TaskTree::new(
            true,
            Limits {
                max_sessions: 2,
                ..Limits::default()
            },
        );
        t.set_other_sessions(1);
        root(&mut t);
        let second = t.admit(NewTask {
            parent: None,
            agent: "z",
            scope: Path::new("/w"),
            rights: ALL,
            delegable: true,
        });
        assert_eq!(second, Err(AdmissionError::SessionLimit));
    }

    #[test]
    fn rejected_admission_inserts_nothing() {
        let mut t = tree();
        let r = root(&mut t);
        assert!(child(&mut t, r, "b", "/other", ALL).is_err());
        assert_eq!(t.nodes.len(), 1);
    }

    #[test]
    fn state_machine_allows_only_the_documented_transitions() {
        use TaskState::*;
        let mut t = tree();
        let r = root(&mut t);
        assert!(!t.transition(r, Completed), "Submitted cannot skip Working");
        assert!(t.transition(r, Working));
        assert!(t.transition(r, InputRequired));
        assert!(t.transition(r, Working));
        assert!(t.transition(r, Completed));
        assert!(t.transition(r, Working), "follow-up on a completed task");
        assert!(t.transition(r, Canceled));
        for to in [
            Working,
            Completed,
            Failed,
            Canceled,
            Rejected,
            InputRequired,
        ] {
            assert!(!t.transition(r, to), "Canceled stays final");
        }
        assert_eq!(t.get(r).unwrap().state, Canceled);
        assert!(!t.transition(Uuid::new_v4(), Working), "unknown task");
    }

    #[test]
    fn cascade_lists_open_descendants_deepest_first() {
        let mut t = tree();
        let r = root(&mut t);
        let c = child(&mut t, r, "b", "/w", ALL).unwrap();
        let g = child(&mut t, c, "c", "/w", ALL).unwrap();
        let other = child(&mut t, r, "d", "/w", ALL).unwrap();
        t.transition(other, TaskState::Rejected);
        assert_eq!(t.open_descendants(r), vec![g, c]);
        assert!(t.open_descendants(g).is_empty());
    }

    #[test]
    fn finished_parent_cannot_delegate_and_force_ends_open_tasks_only() {
        let mut t = tree();
        let r = root(&mut t);
        let c = child(&mut t, r, "b", "/w", ALL).unwrap();
        assert!(t.force(c, TaskState::Failed));
        assert!(
            !t.force(c, TaskState::Canceled),
            "finished tasks stay as they ended"
        );
        assert_eq!(t.get(c).unwrap().state, TaskState::Failed);
        assert_eq!(
            child(&mut t, c, "d", "/w", ALL),
            Err(AdmissionError::ParentNotOpen)
        );
        assert_eq!(AdmissionError::ParentNotOpen.code(), "invalid_state");
    }

    #[test]
    fn subtree_children_and_tree_removal() {
        let mut t = tree();
        let r = root(&mut t);
        let c = child(&mut t, r, "b", "/w", ALL).unwrap();
        let g = child(&mut t, c, "c", "/w", ALL).unwrap();
        let r2 = root_of(&mut t, "q", "/w", ALL);
        assert_eq!(t.children(r), vec![c]);
        assert_eq!(t.subtree(r), vec![g, c, r]);
        t.remove_tree(r);
        assert!(t.get(r).is_none() && t.get(g).is_none());
        assert!(t.get(r2).is_some());
    }

    #[test]
    fn management_is_limited_to_the_callers_subtree() {
        let mut t = tree();
        let r = root(&mut t);
        let c = child(&mut t, r, "b", "/w", ALL).unwrap();
        let g = child(&mut t, c, "c", "/w", ALL).unwrap();
        let r2 = root_of(&mut t, "q", "/w", ALL);
        assert!(t.may_manage(r, g));
        assert!(!t.may_manage(c, c), "no self-approval");
        assert!(!t.may_manage(g, c), "a child cannot manage its parent");
        assert!(!t.may_manage(r2, c), "another root's task is out of reach");
    }
}
