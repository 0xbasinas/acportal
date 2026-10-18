# ADR: per-repository setup, run and archive scripts, with allocated ports

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Conductor runs a **setup script** each time a workspace is created (dependencies, untracked files, `.env`), a **run script** from a Run button, and an **archive script** before a workspace is archived. It passes a port in `CONDUCTOR_PORT` so several workspaces can run side by side, and offers a "nonconcurrent" mode that kills a still-running run script first. Scripts live in `conductor.json`, or in a shared `.conductor/settings.toml`, and personal settings on the machine override the repository file. Superset lists port management. A 2026 guide notes that worktrees isolate code but not ports or databases, and that no tool covers both. See [the competitor notes](adr-ade-index.md#research-notes).

This project creates a worktree per task (`orchestrator/worktree.rs`) but a fresh worktree has no `node_modules`, no `.env` and no running dev server. The host has terminals (`terminals.rs`), a preview proxy for listed ports (`api/preview.rs`, `[preview]` ports are an operator list), and a checks runner (`api/checks.rs`) that runs configured lines on the host. Those pieces are not connected to each other.

## Decision

Add an optional **workspace scripts** block, defined on the host, with an optional repository file as a *suggestion* only.

1. Host config: `[workspace_scripts."<project root>"]` with `setup`, `run`, `archive` (each one shell line, run through the host's fixed shell exactly as terminal shell lines are) and `run_mode = "nonconcurrent" | "concurrent"`.
2. Repository suggestion: if the repository contains `.acportal/scripts.toml`, the apps show it and offer to copy it into host config. The host never runs a repository file's scripts without an operator action. This avoids a malicious repository running code when a task starts, which is the main risk of the Conductor model.
3. When a session worktree is created, run `setup` in it as a host terminal with the existing consent and sandbox rules; show the output in the task view. A failed setup marks the task `needs_attention` and does not start the agent.
4. Ports: the host allocates a free port per worktree from `[preview] port_range` and exports `ACP_PORT` to `run`. The allocated port is added to that worktree's preview list for the lifetime of the run, so Preview and the phone can open it. Nothing else is exposed.
5. `run` starts a terminal in the worktree and is stopped with the task. `archive` runs once on worktree removal; its failure is reported, not retried.
6. Environment: only names listed in the config block are passed through; values never appear in logs, as for terminals today.

## Options Considered

### Option A: host-owned scripts, repository file as a suggestion (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Config, port allocator, glue between worktree, terminals and preview. |
| Cost | None. |
| Scalability | Bounded by terminal and preview limits. |
| Team familiarity | Reuses terminals, checks and preview. |

**Pros:** safe by default; closes the real gap (a fresh worktree can run); no new remote surface.
**Cons:** extra step to adopt a repository's scripts; teams cannot share config through git without that step.

### Option B: run a committed repository script automatically (Conductor's model)

**Pros:** zero setup for a person cloning a repository. **Cons:** a cloned repository runs arbitrary code as the host user; contradicts the terminals and shell-review stance.

### Option C: containerised environment per task

**Pros:** isolates databases and ports properly. **Cons:** large; see [adr-container-isolation](adr-container-isolation.md).

## Consequences

- `docs/security.md` names the new code path: scripts run as the host user (or inside the sandbox where one is configured), only from operator config.
- Windows: scripts run through the configured Windows shell choice; the ADR does not promise POSIX behaviour there.
- Tests: setup failure blocks agent start; two worktrees get different ports; archive runs once; a repository file alone runs nothing.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: config schema, port allocator, runner glue with tests.
3. [ ] Desktop: per-project script editor and a "suggested from repository" prompt.
4. [ ] Android: show setup/run state on the task view; Run and Stop buttons.
