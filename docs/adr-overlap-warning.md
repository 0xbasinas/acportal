# ADR: early warning when parallel tasks touch the same files

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Mux has a git divergence view that flags changes and potential conflicts across its workspaces. The Copilot app's own notes say worktrees do not prevent conflicts when branches merge later. A multi-repo write-up argues that coordinated agent changes are hard to review without seeing the parts together, and another says agent pull requests are flooding review. Superset's "Needs review" column and Vibe Kanban's parallel runs make the same problem likely: two agents each change `package.json`, and the second merge fails after the work is done. See [research notes](adr-ade-index.md#research-notes).

This project creates worktrees per task and merges from the phone, and refuses a conflicting merge with a command to run by hand (orchestrator design section 31). Nothing tells you *before* merge time that two live tasks are heading for the same files.

## Decision

Add a read-only **overlap check** across live tasks and worktrees of the same repository.

1. The host computes, per repository, each active task's changed paths against its merge base: `git diff --name-only <base>...HEAD` plus uncommitted changes (`git status --porcelain`). Cheap, bounded, done at most every 30 seconds per repository and on task state changes.
2. Two tasks **overlap** when they changed the same path. A stronger signal, **conflict likely**, comes from `git merge-tree --write-tree` of the two heads (available in Git 2.38 and later; absent versions fall back to path overlap only and say so). The check writes no refs and touches no working tree.
3. The attention board ([adr-attention-board](adr-attention-board.md)), the task view and the merge screen show a chip: "Overlaps with 'Fix login' (3 files)" and "Conflict likely in package.json". Tapping it opens both diffs for those files side by side.
4. **Merge order hint**: when A and B overlap, the merge screen suggests merging the smaller change first and says why. It never reorders or merges anything.
5. **Heads-up to the agent** (optional, off by default): a one-tap action sends the agent a note, "Another task is changing these files: ... Keep your edits to these files minimal.", as a normal prompt. Never automatic.
6. Also checks the main checkout: if the user's own working tree has uncommitted changes in files a task changed, the merge screen warns before attempting a merge.

## Options Considered

### Option A: periodic path overlap plus `merge-tree` conflict probe (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A scheduler tied to existing git runner calls, a chip, a side-by-side link. |
| Cost | A few git calls every 30 seconds per active repository. |
| Scalability | Bounded by the number of live tasks per repository (default cap 8). |
| Team familiarity | Uses the same git runner as `/v1/git` and merge. |

**Pros:** surfaces the problem when it is cheap to fix; read-only; clear to explain.
**Cons:** path overlap over-reports (different regions of one file); `merge-tree` needs a recent git.

### Option B: only check at merge time

**Pros:** nothing new. **Cons:** the failure is the status quo.

### Option C: file locks that block a second agent from editing a claimed file

**Pros:** prevents conflicts. **Cons:** the host would deny agent writes by policy, changing agent behaviour in ways that confuse the agent; a bigger decision.

## Consequences

- No new write path and no consent change; the check reads refs and diffs under existing git-view rights.
- Tests: two worktrees changing the same file produce the chip; disjoint changes do not; old-git fallback; a deleted file on one side and an edit on the other.
- Lists of changed paths are already visible in the git view.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: overlap computation and route, tests with real repositories.
3. [ ] Desktop and Android: chips and the side-by-side link.
4. [ ] Check git version handling on the Windows runner.
