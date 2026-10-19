# ADR: a ticket dispatcher (poll, dispatch, reconcile) with hard limits

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Several orchestrators turn a work queue into agent runs without a person starting each one. OpenAI's Symphony is an open-source spec that treats a Linear board as a control plane for Codex: each open task gets its own agent workspace, ticket statuses act as a state machine, and the orchestrator polls the board, picks up new work and restarts agents that crash or stall; one issue can yield several PRs or only an investigation, and the Linear token is kept away from the agents by exposing a dynamic tool call instead of the token. Baton runs a poll-dispatch-reconcile loop over GitHub Issues with one `WORKFLOW.md` file and one worktree per issue. Code Conductor has agents claim tasks from a labelled GitHub Issue queue. Agent Orchestrator routes CI failures, review comments and merge conflicts back to the session that produced them. Orca reads linked ticket context and moves work through workflow states. Backlog.md and Agent Kanban keep tasks as Markdown files in the repository, with acceptance criteria, and Backlog.md describes three review points (spec, plan, code). The research also shows the danger: Gas Town's "limitless mode" merged a PR despite failing integration tests, and its coordinator misreported status. See [research notes](adr-ade-index.md#research-notes).

[adr-issue-tracker](adr-issue-tracker.md) adds a manual "Work on this" for GitHub Issues. Nothing watches a queue. This ADR is the unattended version, and the risky one.

## Decision

An opt-in **dispatcher** in the host that turns tickets in an explicit "ready" state into ordinary tasks, within hard limits, and never merges.

1. **Sources** (one per project, behind an interface): GitHub Issues through the forge adapter ([adr-git-forges](adr-git-forges.md)); a repository backlog in a Markdown-task layout the operator names (for example `backlog/tasks/*.md` with front matter `status` and acceptance criteria), read from the default branch only; later Linear. Each source returns `{id, title, body, labels, status, url}`.
2. **Eligibility**: a ticket is dispatched only if it carries the operator's trigger (a label like `agent-ready`, or status `Ready`), is not assigned to a person, has no open task already, and its body is under the size limit. Everything else is ignored. There is no "claim anything" mode.
3. **Dispatch**: poll every N minutes (default 5, minimum 1). For each eligible ticket, up to `max_concurrent` (default 1, hard maximum 4), create a task from a recipe ([adr-workflow-recipes](adr-workflow-recipes.md)) in a session worktree with the ticket text in a fenced, labelled block ([adr-issue-tracker](adr-issue-tracker.md)). The recipe fixes the agent, profile, budget and checks. No recipe, no dispatch.
4. **Reconcile**: each poll compares tickets with task records. A ticket whose task finished moves to a configured "in review" status (a comment with the task link, and the label change) only with `write_back = true`; a ticket closed by someone else cancels its running task; a task whose agent crashed or stalled is surfaced on the attention board ([adr-stuck-detection](adr-stuck-detection.md)). It is not restarted automatically with the old prompt: restarting means a new session with a handoff note, which needs a person.
5. **Never**: merge, push to a protected branch, open a PR without the existing phone-confirmed actions, or touch tickets without the trigger. A result is a branch and a diff waiting in *needs review*.
6. **Credentials**: forge CLI sign-in stays in the CLI's store; the agents never see it. Write-back happens in the host, not through a tool the agent can call.
7. **Limits and cost**: a daily dispatch cap (default 5), the recipe's budget per task, and a global kill switch (`acpd dispatcher pause`) that stops polling at once. Everything dispatched is recorded in the audit trail with the ticket id.
8. **Visibility**: the attention board shows "Dispatched from #123" and a dispatcher card with last poll time, next poll, and the reason a ticket was skipped.

## Options Considered

### Option A: opt-in poll, dispatch and reconcile with caps (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium to high. Sources, eligibility, reconcile logic, write-back, tests with fake forges. |
| Cost | Provider spend per dispatched task, capped. |
| Scalability | Hard cap of four concurrent tasks and a daily limit. |
| Team familiarity | Uses recipes, tasks, worktrees and the forge adapter. |

**Pros:** closes the loop from ticket to reviewable branch; every limit is explicit; the host keeps the credentials.
**Cons:** unattended spend and prompt-injection exposure through ticket text; mistakes at the label level dispatch wrong work.

### Option B: manual only (the issue-tracker ADR)

**Pros:** safest. **Cons:** a person starts every run.

### Option C: a full multi-agent coordinator (Gas Town style, 20 to 30 agents)

**Pros:** maximum throughput. **Cons:** reported cost around $100 for an hour of work and stress from work moving too fast to follow; the opposite of this project's person-in-the-loop stance.

## Consequences

- `docs/security.md`: ticket text is untrusted input to an agent running unattended; the safeguards are the recipe's profile and budget, worktree isolation, no merge, and the daily cap. Treat it as the highest-risk feature here and ship it last.
- Tests: ineligible tickets never dispatch; the concurrency and daily caps; a closed ticket cancels its task; write-back off by default; the pause switch.
- Depends on [adr-workflow-recipes](adr-workflow-recipes.md), [adr-issue-tracker](adr-issue-tracker.md), [adr-git-forges](adr-git-forges.md), [adr-scheduled-runs](adr-scheduled-runs.md) and [adr-audit-and-trace](adr-audit-and-trace.md).

## Action Items

1. [ ] User reviews this ADR; accept its dependencies first.
2. [ ] Host: GitHub source and dispatch loop with fakes; caps and pause.
3. [ ] Repository-backlog source (read only) after that.
4. [ ] Clients: dispatcher card and skip reasons.
