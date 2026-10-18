# ADR: a merge queue for finished tasks

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

When several agents finish at once, merging them one after another by hand repeats the same failure: the second merge conflicts or breaks tests that the first one changed. Gas Town is described as having a "Refinery" role for integration (my fetch of the DoltHub write-up did not confirm it, so treat the name as unverified); Bernstein runs a "Janitor" that executes lint, type checks and tests before work reaches its merge queue; Agent Orchestrator routes merge conflicts back to the session that produced them; Braidkeep computes a safe merge order. GitHub has a merge queue and put stacked pull requests into preview in July 2026. The Gas Town write-up also records the opposite failure: a PR merged despite failing integration tests. See [research notes](adr-ade-index.md#research-notes).

This host merges one task at a time from the phone and refuses conflicts. [adr-overlap-warning](adr-overlap-warning.md) warns early, [adr-conflict-resolution](adr-conflict-resolution.md) helps with a conflict, and [adr-task-pipelines](adr-task-pipelines.md) gates a single task. None serialises several tasks into one target branch with checks between merges.

## Decision

A per-repository **queue** of task branches waiting to be integrated, processed one at a time, with a person confirming each landing.

1. **Enqueue**: from a task's merge screen, "Add to merge queue" (target branch default: the repository default branch). A task enters only if its own checks passed and its risk label gate was ticked ([adr-risk-ranked-review](adr-risk-ranked-review.md)).
2. **Order**: by default the smaller diff first ([adr-overlap-warning](adr-overlap-warning.md)'s hint), and the person can drag to reorder. Order never changes silently after enqueue except to move a task to "needs attention".
3. **Processing the head**: in a temporary integration worktree, the host merges the target's current tip with the task branch (`git merge --no-ff`, or rebase if the repository's setting says so), runs the checks, and records the result. Clean and green: status *ready to land*, waiting for the person's confirmation on the phone. Conflict: *needs attention* with a link to [adr-conflict-resolution](adr-conflict-resolution.md). Checks red: *needs attention* with the output. Either way the queue moves to the next entry only after the head is landed, skipped or removed.
4. **Landing** is the existing phone-confirmed fast-forward of the target to the integration commit. After a landing, later entries are re-based on the new tip and re-checked (the point of a queue: each entry is tested against what actually landed before it).
5. **Never**: land without confirmation, push to a remote without the existing confirmed push, or skip checks. A "land without checks" override exists only when no checks command is configured at all, and the screen says so.
6. **Cancel and pause**: removing an entry, pausing the queue, and "stop on first failure" (default on) are always available; a host restart leaves the queue paused with its entries intact.
7. Integration worktrees are named for pruning and removed after landing or failure, as in other ADRs here.

## Options Considered

### Option A: a serial queue with per-entry re-check and human confirm (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Queue records, an integration worktree runner, re-check logic, a view. |
| Cost | Check time per entry. |
| Scalability | One queue per repository; typical queue depth under 10. |
| Team familiarity | Worktrees, checks and merge confirmation exist. |

**Pros:** removes the manual re-test cycle; each landing is tested against the real tip; a person still decides.
**Cons:** slow when checks are slow; a long queue goes stale if the target moves outside the queue.

### Option B: rely on GitHub's merge queue after PRs

**Pros:** mature. **Cons:** needs PRs on GitHub and branch protection; unavailable to local-only repositories and other forges.

### Option C: merge in parallel and fix breakage afterwards

**Pros:** fast. **Cons:** the Gas Town failure mode.

## Consequences

- Tests with real repositories: three branches that touch the same file land in order; a green-then-red case after the first landing; a restart pauses the queue; no landing without confirmation.
- When a forge adapter is present and the repository uses the forge's own queue, this one is hidden.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: queue records, integration worktree runner, tests.
3. [ ] Desktop: queue view; Android: queue status and confirm.
