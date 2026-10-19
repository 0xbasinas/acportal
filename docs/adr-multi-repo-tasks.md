# ADR: tasks that span several repositories

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Cursor 3 has multi-root workspaces, so one agent session can target several folders for cross-repo changes. Vibe Kanban supports several repositories in one workspace, each with independent git state. A community discussion says true multi-repo orchestration is not available from the big vendors and suggests one scoped agent per repo with PRs coordinated in dependency order, or a meta-repo. GitHub put stacked pull requests into public preview on 30 July 2026 with a `gh-stack` extension; it orders branches and cascades rebases but does not decide how to split a change. A case study describes 25 engineers using agents across 75 repositories, and a post argues that a coordinated change must be reviewed with all its parts in view. See [research notes](adr-ade-index.md#research-notes).

Here, ACP carries `cwd` plus workspace roots (the orchestrator design notes `cwd` must be absolute and is part of the effective roots). The host's worktree code creates a worktree per task for one repository, and the PR flow opens one pull request. Submodule repositories get no worktree (orchestrator design section 31). A change across a library and its consumer needs two sessions started by hand.

## Decision

Introduce a **task group**: one parent task with one child task per repository, created and reviewed together. Reuse delegated tasks and worktrees rather than adding a multi-root session type.

1. **Create**: the New session screen lets a person add repositories under one task ("+ add repository"). Each must be inside an authorised workspace root. The host creates one child task and worktree per repository, on branches with a shared name stem (`feature/x` in each).
2. **One agent, many roots, if supported**: ACP's *additional directories* RFD is completed (1 June 2026): `additionalDirectories` (absolute paths) on `session/new`, `load`, `resume` and `fork`, gated by `sessionCapabilities.additionalDirectories`, with `cwd` as the primary root and the effective roots their union; roots are explicit on load and resume, are not inherited on fork, and cannot change mid-session. When the chosen agent advertises it, a single session gets the main worktree as `cwd` and the others as `additionalDirectories` (see [adr-acp-capabilities-roadmap](adr-acp-capabilities-roadmap.md)). When it does not, the group is created as separate child tasks with the same prompt and a short note naming the sibling paths. The screen says which mode is in use.
3. **Order**: the person can mark a dependency ("B depends on A"). The attention board and merge screens show the group in that order, and the phone's merge action is offered for B only after A is merged or the person overrides.
4. **Review as a group**: one screen lists every repository's diff, risk labels ([adr-risk-ranked-review](adr-risk-ranked-review.md)), checks and branch, with a combined "ready" state.
5. **PRs**: opened per repository (the existing flow), each body listing the sibling PRs and the dependency order. A stacked set within one repository uses `gh-stack` only if present and confirmed; this ADR does not require it.
6. Limits: at most 5 repositories per group; no automatic cross-repo conflict resolution; a group never merges anything by itself.

## Options Considered

### Option A: task group over existing tasks and worktrees (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A group record, creation UI, group review, link text in PRs. |
| Cost | One worktree per repository. |
| Scalability | Capped at five repositories. |
| Team familiarity | Same task, worktree, merge and PR code. |

**Pros:** works with agents that only support one root; keeps merges explicit; builds on shipped parts.
**Cons:** separate agent sessions may not see each other's changes; cross-repo consistency depends on the prompt and the person.

### Option B: a single multi-root session only

**Pros:** one agent sees everything. **Cons:** depends on agent support that is uneven; harder to merge or discard one repository.

### Option C: a meta-repository with submodules

**Pros:** common workaround. **Cons:** submodule repositories get no worktree today; a different problem.

## Consequences

- `docs/security.md`: additional roots widen what a single agent can read and write; each must be an authorised root and the session's access is the intersection, as with delegation.
- Tests: a root outside the allowed set is refused; branch-name stems; dependency order gating the merge button; a group with one failing child stays "not ready".
- The protocol side is settled (see item 2); which registry agents advertise the capability is still unchecked and is the first action item. The RFD notes that scope is not a security boundary by itself, so the host's containment checks apply to every root.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Check which two real agents advertise `sessionCapabilities.additionalDirectories`.
3. [ ] Host: group record, creation, order gating.
4. [ ] Desktop: group creation and review; Android: read and merge-order view.
