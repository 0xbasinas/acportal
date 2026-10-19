# ADR: an attention board across sessions, tasks and hosts

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Superset's Workspaces page groups work into Working, Needs attention, Needs review and Merged. The GitHub Copilot app opens on "My Work": active sessions, issues, pull requests and automations in one view. Vibe Kanban maps each board card to an agent run. See [the competitor notes](adr-ade-index.md#research-notes).

This project already holds the data such a view needs, but shows it in separate places:

- Sessions and their pending approvals (`/v1/sessions`, the `SessionManager` journal).
- Task records and the task tree (`orchestrator/records.rs`, `orchestrator/views.rs`, `desktop/src/Tasks.tsx`).
- Worktrees and merge state (`orchestrator/worktree.rs`).
- "Run on several agents" compare, checks and review (`desktop/src/Multi.tsx`, `api/checks.rs`).
- Several paired hosts (`desktop/src/Hosts.tsx`, [desktop-multi-host](desktop-multi-host.md)).

Nothing answers the question a person asks first: *what is waiting on me, on any host?* The TODO already lists "approval notifications and list badges" for Android.

## Decision

Add one read-only **attention** projection and one board view that uses it. No new execution path.

1. Host: `GET /v1/attention` returns bounded rows, one per session or task, each with `state`, `since`, `host`, `workspace`, `agent`, and a short `reason`. States are fixed: `working`, `needs_approval`, `needs_input` (form elicitation), `needs_review` (a finished task with an unmerged worktree or failing checks), `failed`, `merged`, `idle`. The host derives them from data it already owns; it does not call an agent.
2. Clients combine the rows from every paired host. Desktop shows columns (Needs you, Working, Needs review, Done) and a flat list. Android shows the same states as sections on the Sessions page, plus badges.
3. Notifications: desktop uses the OS notification API; Android uses a notification channel fed by the existing WebSocket while the app runs. A push service is out of scope (see Options).
4. **Unread state** (Orca can mark threads unread to return later): each row has a seen/unread flag per device, synced through the host, so "finished" items you already looked at on the desktop do not badge the phone. Marking unread is a manual action.
5. **Long waits**: an item in `needs_approval` for longer than a configurable time (default off) raises an `approval_waiting_long` event for [host hooks](adr-host-hooks.md), so the operator can add a second notification channel. The agent keeps waiting; nothing is auto-approved or auto-denied.
6. Acting from the board only opens the existing session, approval sheet or merge screen. The board never approves, merges or resends a prompt by itself.

## Options Considered

### Option A: a host-computed attention projection (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. One route, one state function, one client view each. |
| Cost | None beyond memory for a bounded list. |
| Scalability | Per host; clients merge hosts. |
| Team familiarity | Same pattern as `/v1/shared` and task views. |

**Pros:** one definition of "needs you" for phone and desktop; testable in Rust; works with the existing journal.
**Cons:** the host must keep state names stable; a new route to secure under `Role::Phone`.

### Option B: each client derives state from session lists

**Pros:** no host change. **Cons:** two implementations drift; the desktop and phone already disagreed about badges in earlier work.

### Option C: push notifications through a relay

**Pros:** works with the app closed. **Cons:** needs an operated service and a statement of what it can read (see [adr-remote-routes](adr-remote-routes.md) option C). Not decided here.

## Trade-off Analysis

A is the smallest change that gives both clients the same answer. B is cheaper today and costlier at every later feature. C is the only way to notify a closed Android app, so it stays a separate decision.

## Consequences

- `docs/protocol.md` gains the route and the closed list of states.
- Rows contain titles and workspace names, which are already visible to a paired phone. They contain no prompt text beyond the existing session title.
- Android notifications work only while the process holds a connection; say so in the settings copy.
- A board is a view of existing records. Deleting a row deletes nothing.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: state function with unit tests for each state, including a task whose worktree is dirty.
3. [ ] Host: `GET /v1/attention`, phone-role only, bounded to 200 rows.
4. [ ] Desktop: board and list views; OS notification on `needs_approval`.
5. [ ] Android: sections and badges from the same rows; notification channel.
6. [ ] Real-device check of notifications (not possible in the emulator-only environment used so far).
