# ADR: host hooks for events, including outbound notifications

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Claude Code hooks run at lifecycle events (session, per turn, async, agent-team events). One guide stresses that they are deterministic: they execute every time, unlike an instruction in a prompt. Common uses are blocking writes outside the repository and running a formatter after edits. Kiro describes hooks as determining when agents act. Warp sends notifications when an agent finishes or needs input. Cursor can start agents from Slack, GitHub, Linear or mobile. Native push for a closed Android app needs a relay ([adr-attention-board](adr-attention-board.md) option C). See [research notes](adr-ade-index.md#research-notes).

This host sees every agent event and decision as the ACP client, and it is the party that performs filesystem writes and terminals. So it can run deterministic commands *for any agent* at the right moments, which agent-side hooks cannot offer to agents that lack them. A keyword search found only internal uses of the word "hooks" in the host.

## Decision

Add host-configured **hooks**: an event, an optional filter, and an action. Actions are limited and always host-controlled.

1. Events (closed list): `turn_started`, `turn_ended`, `approval_needed`, `approval_decided`, `write_applied` (a host-approved write finished), `task_finished`, `task_failed`, `checks_failed`, `pr_opened`, `session_ended`, `budget_exceeded`, `approval_waiting_long`, `stuck_suspected` ([adr-stuck-detection](adr-stuck-detection.md)).
2. Actions (closed list):
   - `run`: a literal executable and argument array (never a shell line), run in the task's worktree with a 60-second timeout, a minimal environment plus `ACP_EVENT`, `ACP_SESSION`, `ACP_TASK`, `ACP_PATH` and `ACP_WORKTREE`, through the existing sandbox where one is configured. Output is captured, bounded and shown in the task. Example: format the written file after `write_applied` with a path filter.
   - `notify`: an HTTPS POST of a small JSON body (event name, session title, host name, a deep link) to a configured URL, or to an [ntfy](https://ntfy.sh)-style topic. Never includes prompt or file content. Works with a self-hosted ntfy or webhook receiver, so a phone gets push without this project operating a relay.
   - `prompt`: send a fixed follow-up prompt to the session (for example after `checks_failed`: "The checks failed. Fix them."), with a per-session cap of three automatic sends per hour. Off unless the operator lists it and it states the cap.
3. Filters: agent id, workspace, path glob, risk label ([adr-risk-ranked-review](adr-risk-ranked-review.md)). A hook with no filter applies to every session and the config screen says so.
4. **Failure policy**: a failing or slow hook never blocks the event, except `run` on `write_applied` when the operator sets `blocking = true`, in which case a nonzero exit marks the write as failed to the agent with a short message (the host owns the write, so it can report it that way). Blocking hooks have a 10-second ceiling.
5. Hooks live in host config and are reloaded with the existing soft-reload; changing them needs the same access as changing agents. A repository cannot define hooks.
6. Every hook run appears in the audit trail ([adr-audit-and-trace](adr-audit-and-trace.md)) with the hook id and exit status, not its output.

## Options Considered

### Option A: a closed event and action list in host config (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. An event bus tap, three action runners, limits. |
| Cost | Process spawns per event; capped. |
| Scalability | Per-event rate cap (default 10 per minute). |
| Team familiarity | Same literal-argument execution as agents and checks. |

**Pros:** deterministic, agent-independent, and the only route to push notifications without a relay; the operator can see everything configured in one place.
**Cons:** `run` is arbitrary code execution as the host user, so the config is a high-trust surface; `prompt` can loop if misused, hence the cap.

### Option B: expose events over a local socket for external tools to react to

**Pros:** flexible. **Cons:** a second API surface with its own auth; hooks are simpler for the common cases.

### Option C: rely on each agent's own hooks

**Pros:** nothing to build. **Cons:** only some agents have them; they are invisible to the phone.

## Consequences

- `docs/security.md`: hooks are operator-configured code; the `notify` body never carries content; the `notify` URL is checked to be `https` or loopback, and DNS rebinding to private ranges is refused unless explicitly allowed.
- Tests: timeouts; env contains only the listed variables; no shell interpretation of arguments; the prompt cap; the body of a notify call contains no secret or prompt text.
- A hook that edits files after a write races with the agent's next read; the docs say to keep hooks idempotent.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: event tap, `notify` first (smallest and answers the push question), then `run`, then `prompt`.
3. [ ] Desktop: hook editor with a dry-run button; Android: read-only list.
