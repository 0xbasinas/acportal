# ADR: scheduled and triggered agent runs

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Superset schedules agent sessions for unattended runs. The Copilot app has cloud automations that run agents on a schedule or on GitHub events. See [the competitor notes](adr-ade-index.md#research-notes).

This host already starts sessions and tasks, has permission presets, worktree isolation, auto-approve rules and a resident worker (`acpd service install` is built, see the T3 Code section of TODO). A scheduled run needs only a timer and a stored recipe, but it is the highest-risk idea in this set: nobody is watching.

## Decision

Add **automations**, host-side, off by default (`[automations] enabled`), with the narrowest rights.

1. An automation is `{id, schedule, agent, workspace, prompt, profile, max_runs_per_day, budget}`. Schedules are a simple repeat (every N minutes/hours, or daily/weekly at a time) with the host's time zone. No cron expressions in the first cut.
2. Every run is a normal task in a **session worktree** (isolation `worktree` is mandatory; a workspace that is not a git repository cannot be scheduled). The run never writes to the user's checked-out branch.
3. Profile (see [adr-session-modes](adr-session-modes.md)): only `Interactive`-class permissions plus the operator's existing auto-approve rules. An action that would need a person produces a `needs_approval` row on the attention board ([adr-attention-board](adr-attention-board.md)) and the run waits until `budget.wall_clock` expires, then is cancelled.
4. Budgets are mandatory: wall-clock, turns, and `max_runs_per_day`. Token or cost limits apply only where the agent reports `usage_update`, as in the orchestrator design.
5. A missed run (host off) is skipped and noted, not replayed in a burst.
6. Results: the finished task appears as *needs review* with its diff and checks. Nothing is merged or pushed. Pushing a branch or opening a PR needs the existing phone-confirmed actions.
7. Management is by the phone or desktop over the normal authenticated API (phone role), and by `acpd automations list|run|pause`.

## Options Considered

### Option A: host timer creating worktree tasks (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Scheduler, store, limits, CLI. |
| Cost | Provider cost only when a run happens. |
| Scalability | Bounded runs per day and concurrent automation runs (default 1). |
| Team familiarity | Same task/worktree/budget code as delegation. |

**Pros:** reuses isolation and budgets; safe failure mode (nothing merges).
**Cons:** the host must be running; time-zone and daylight-saving edge cases.

### Option B: use the operating system's scheduler to call the CLI

**Pros:** no scheduler to maintain. **Cons:** three platforms, each different; harder to show and pause from the apps.

### Option C: event triggers (new issue, new commit)

**Pros:** closer to what competitors advertise. **Cons:** needs a webhook or poller; leave until [adr-pr-watch](adr-pr-watch.md) shows the polling pattern works.

## Consequences

- `docs/security.md`: an automation can spend money and run an agent while the owner is away; the limits above are the controls, and they must be enforced by the host, not the client.
- A host restart reloads automations but never resumes a half-finished run.
- Tests: schedule math across a daylight-saving change; skip-on-missed; the daily cap; a run in a non-git workspace is refused.

## Action Items

1. [ ] User reviews this ADR, especially item 3 (what an unattended run may do).
2. [ ] Host: store, scheduler, limits, CLI, with fake clock tests.
3. [ ] Desktop and Android: list, create from a past prompt, pause.
