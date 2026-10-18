# ADR: suspend idle agent processes and resume them on demand

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

AcpAgent Client lets active sessions be suspended through `session/close` and resumed later, with the sidebar grouping sessions into Active and History. Perch leaves managed remote sessions running when the app closes. Codeg and Harnss keep many sessions and agents open side by side. Running a Node or Python agent per open session costs memory; the host RSS test (`tests/memory_rss.rs`) bounds the host's own growth, not the agents'. See [research notes](adr-ade-index.md#research-notes).

In this host each interactive session holds a live agent process until it is deleted or the host stops (`max_sessions` defaults to 8). The orchestrator already cancels *delegated tasks* that have been quiet for `idle_timeout_seconds`, and `session.rs` already asks an agent that advertised `session/close` to end the session politely before stopping the process (`supports_close`, `close_politely`). Interrupted sessions after a daemon restart are catalogued and can be reloaded with explicit capability-checked loading. So the three needed pieces exist: close, catalog, load. They are not connected for the "I left this open for two days" case.

## Decision

Add an opt-in **idle suspend** for interactive sessions, using only mechanisms that already exist.

1. `[runtime] suspend_idle_minutes = 0` (off). When set, a session with no turn running, no pending approval or elicitation, no attached terminal it owns and no client connected for that long becomes **suspended**: the host calls `session/close` when advertised, stops the agent process, and keeps the session's catalog record and journal (bounded, as now).
2. Suspended sessions appear with a "Suspended" state, not "Interrupted by restart", and show how long ago. Opening one shows saved history without starting anything (the existing rule).
3. **Resume** is explicit: the person taps Resume, or sends a prompt from the composer, which first triggers a resume. The host uses `session/load` only when the agent advertised the capability; for agents without it the session cannot be resumed with its agent state, and the screen says so before suspending is ever offered (a per-agent "can suspend" flag from `initialize`).
4. The working directory, worktree and branch are untouched. Terminals the session started are not suspended; a session that owns a running terminal is not idle for this purpose.
5. Suspend never applies to sessions with a running automation, a delegated child, or a PR watch ([adr-pr-watch](adr-pr-watch.md)); those have their own lifetimes.
6. The activity panel ([adr-activity-panel](adr-activity-panel.md)) shows idle sessions with a Suspend now action, so the feature is usable by hand before the timer is trusted.

## Options Considered

### Option A: opt-in suspend using close, catalog and load (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A timer in the session manager, a new state, resume-on-prompt. |
| Cost | A little latency on resume; saves memory for each idle agent. |
| Scalability | Raises the practical number of open sessions beyond `max_sessions` of live agents. |
| Team familiarity | Reuses `close_politely`, the catalog and explicit loading. |

**Pros:** frees memory with no new protocol; the session's history stays available; works from the phone with one tap.
**Cons:** resume depends on the agent's `session/load` and may lose in-memory agent state; the first prompt after resume is slower.

### Option B: never suspend

**Pros:** simplest, no surprises. **Cons:** idle agents accumulate; the session cap is reached by sessions nobody is using.

### Option C: delete idle sessions

**Pros:** frees everything. **Cons:** loses the live session; resume becomes "start again".

## Consequences

- Tests with the mock agent: idle session suspends and the process stops; a pending approval prevents suspension; resume restores history through `session/load`; an agent without `session/load` is never auto-suspended; the journal survives.
- Needs a real-agent check of `session/load` after `session/close` with at least Goose and one other agent before the default is changed from off.
- Messages typed while resume is in progress queue behind the resume ([adr-steer-and-queue](adr-steer-and-queue.md) semantics).

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Check how Goose behaves on `session/close` then `session/load`.
3. [ ] Host: state, timer, resume path, tests.
4. [ ] Clients: Suspended state and Resume.
