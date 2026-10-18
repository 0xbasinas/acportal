# ADR: warm worktrees, so a new task starts ready

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Ona (formerly Gitpod) uses prebuilds: dependencies are compiled and initial tests run when a commit lands, so a new environment starts ready. Cursor 3 claims sub-second boot goals for agent tasks as a differentiator that Codespaces lacks. Cursor's `/best-of-n` and `/multitask`, and this project's "Run on several agents", all create several isolated workspaces at once. The Codex changelog repeatedly fixes worktree creation and attach problems (September and October 2026). See [research notes](adr-ade-index.md#research-notes).

A new worktree here is a fresh checkout. With [adr-workspace-scripts](adr-workspace-scripts.md), `setup` then installs dependencies, which can take minutes and block the agent. Running three agents on the same repository pays that cost three times.

## Decision

Keep a small pool of **prepared worktrees** per repository and hand one to a new task.

1. Opt in per project: `[warm_worktrees."<root>"] size = 2`, requiring a `setup` script from [adr-workspace-scripts](adr-workspace-scripts.md). Off by default.
2. The host creates up to `size` detached worktrees from the repository's default branch under the state directory, runs `setup` in each (same consent, sandbox and output capture as normal setup), and marks them `ready` with the commit they were built from.
3. A new task takes a ready worktree, checks out its branch (`git switch -c`), fast-forwards to the current default branch when the dependency manifest (lockfile hash) is unchanged, and otherwise discards the warm copy and builds normally. Taking one triggers a refill in the background.
4. Disk and CPU limits: the pool respects a global cap (`[warm_worktrees] max_total`, default 4) and a free-space floor (the doctor's 64 MiB check is far too low here; default 5 GiB free required). Under the floor the pool shrinks to zero and says why.
5. Staleness: a warm worktree older than 24 hours, or built from a commit more than N commits behind, is rebuilt. Idle pools are removed when the host stops.
6. Dependency caches shared by tool (npm, cargo, pip) are left to the tools' own caches. The pool does not hard-link or copy `node_modules` between worktrees.
7. Cleanup uses the existing worktree removal and `archive` script; a crash leaves worktrees under a recognisable name that the next start prunes, as with stale containers in [adr-container-isolation](adr-container-isolation.md).

## Options Considered

### Option A: a small opt-in pool, rebuilt on lockfile change (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A pool manager, staleness rules, cleanup. |
| Cost | Disk (one worktree plus dependencies each) and idle CPU while building. |
| Scalability | Hard caps. |
| Team familiarity | Builds on worktree and scripts code. |

**Pros:** turns minutes of setup into seconds for the common case; helps "Run on several agents".
**Cons:** disk use; a stale pool is a small supply-chain risk if lockfiles changed upstream, hence the hash check.

### Option B: share one dependency directory by link between worktrees

**Pros:** fast and small. **Cons:** a package install in one task can break the others; platform quirks on Windows.

### Option C: do nothing

**Pros:** simple. **Cons:** setup time stays on the critical path.

## Consequences

- The pool only exists when scripts exist, so the trust model is the one in [adr-workspace-scripts](adr-workspace-scripts.md); no new execution path.
- Tests: taking a worktree refills; a changed lockfile invalidates; free-space floor empties the pool; crash recovery removes strays.
- Measure first: time `setup` on a real project before building this. If setup is already seconds, drop the ADR.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Measure setup time on two real projects.
3. [ ] Host: pool manager, tests.
4. [ ] Settings: size and cap, visible pool state.
