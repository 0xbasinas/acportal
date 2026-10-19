# ADR: per-turn checkpoints with undo and fork

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Cline is known for checkpoints that let a person compare and restore the workspace to an earlier step. My searches did not confirm checkpoints in Conductor, Superset or Vibe Kanban, so this may be a gap in the whole category rather than something to copy. See [the competitor notes](adr-ade-index.md#research-notes).

Why it matters here: host auto-approve and shell auto-review let an agent change many files without a person looking. Today a bad turn is undone by hand with git, and only if the workspace is a git repository. Session worktrees (`orchestrator/worktree.rs`) give a safe place to experiment, but only per task, not per turn.

## Decision

Take a **checkpoint** at the start of each prompt turn in a session that runs in a git repository, and let the person restore or fork from it.

1. Mechanism: `git stash create`-style snapshot objects stored under a private ref namespace (`refs/acportal/checkpoints/<session>/<n>`). The snapshot covers tracked changes and untracked, non-ignored files. It never touches the index, the user's branches or the stash list, and it is never pushed.
2. Triggered by the host when it forwards `session/prompt`, so it also covers a prompt sent from the phone. It adds no latency beyond one `git` call, bounded by a timeout; if the snapshot fails the prompt still goes, and the timeline says "no checkpoint".
3. Actions: **Compare** (diff between a checkpoint and now), **Restore** (reset the working tree to the checkpoint, after confirmation, and only when no write consent or terminal is mid-flight), **Fork** (create a new worktree and session from the checkpoint, reusing the existing Run on several agents plumbing).
4. Restore does not rewrite the agent's own conversation. The timeline gets a marker and the next prompt is prefixed with a note that files were restored. This is stated in the UI, because the agent may still believe its earlier edits exist.
5. Retention: keep the last 50 checkpoints per session and drop them with the session or worktree. Refs are removed by the same cleanup that removes worktrees.
6. Non-git workspaces get no checkpoints; the UI says so instead of copying directories.

## Options Considered

### Option A: git snapshot refs, host-driven (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Git plumbing, retention, UI. |
| Cost | Disk for objects; git deduplicates. |
| Scalability | Bounded per session. |
| Team familiarity | Same `git` runner as `/v1/git` and worktrees. |

**Pros:** cheap, exact, uses the repository's own object store.
**Cons:** large untracked trees make snapshots slow; ignored files (build output, `.env`) are not restored.

### Option B: copy-on-write directory snapshots

**Pros:** works without git. **Cons:** platform-specific, large, and copying a tree that may hold secrets.

### Option C: leave it to worktrees and manual git

**Pros:** no work. **Cons:** the cost of a bad autopilot turn stays with the user.

## Consequences

- `docs/security.md`: checkpoints live in the repository's own `.git` and may contain files the agent just wrote, including secrets; they are local only.
- Restore is a write to the workspace made by the host, so it needs the same consent path as other host writes and an audit line (decision only, no content) in the host log.
- Tests: snapshot with untracked and ignored files; restore leaves unrelated user changes made after the checkpoint untouched only if the person chose "restore selected files"; refs removed on session delete.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Spike: time a snapshot on a large repository; decide the timeout.
3. [ ] Host: checkpoint create, list, compare, restore, with tests on Linux and Windows.
4. [ ] Desktop and Android: timeline markers and the three actions.
5. [ ] Document the "agent still remembers its edits" limit next to the Restore button.
