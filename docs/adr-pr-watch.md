# ADR: watch a pull request and feed CI and review feedback to the agent

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

The GitHub Copilot app's Agent Merge watches CI and review state and, depending on team settings, reruns CI, addresses review comments or completes the merge, while branch protection still applies. See [the competitor notes](adr-ade-index.md#research-notes).

This project can already open a pull request from the Git view (`api/pr.rs`, using the `gh` CLI; only a fake `gh` has been tried) and run configured checks (`api/checks.rs`). After the PR exists the loop ends: a failing CI job or a review comment is something the person must copy back to the agent.

## Decision

Add a **PR watch** that turns new feedback into a *proposed* prompt. It never sends one by itself.

1. A watch is created from a session or task that opened a PR. State: PR number, repository, last seen check-run and review ids. Stored with the task record.
2. The host polls through `gh` on a long interval (default 60 seconds, backing off to 10 minutes when nothing changes; stops when the PR is merged or closed). It uses `gh`'s own authentication; no token is stored here.
3. **Route to the origin**: feedback is attached to the session or task that produced the branch (Agent Orchestrator routes CI failures, review comments and conflicts back to the producing session), not to a new one; if that session is gone, the item offers Start fresh with a handoff note ([adr-context-handoff](adr-context-handoff.md)).
4. On new failing checks or new review comments, the host builds a bounded summary (check name, first lines of the failing log, comment text with path and line) and publishes it as a `pr_feedback` item on the session, using the `_meta` convention already used for host items.
5. The client shows the item with **Send to agent**, **Dismiss** and **Always send for this task**. Only an explicit tap, or the per-task opt-in, sends a prompt. The opt-in has a ceiling of three automatic rounds per watch, then asks again.
6. A watch never merges, approves, pushes or edits the PR. Merge stays with the existing phone-confirmed merge action and branch protection.
7. Comment and log text comes from other people or from CI; it is shown as untrusted quoted content and wrapped in a fenced block in the prompt, so it is not presented to the agent as the user's own instruction.

## Options Considered

### Option A: host poll through `gh`, human sends (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Poller, summariser, one item type. |
| Cost | A few `gh` calls per minute per watch. |
| Scalability | Bounded watches per host (default 8). |
| Team familiarity | Same `gh` runner as PR creation. |

**Pros:** no webhooks or public address; keeps a person in the loop.
**Cons:** polling latency; depends on `gh` being installed and signed in.

### Option B: GitHub webhooks to the host

**Pros:** immediate. **Cons:** needs an inbound public route, which the host does not expose.

### Option C: fully automatic fix loop

**Pros:** least effort. **Cons:** prompt injection through review comments and unbounded spending; rejected for now.

## Consequences

- `docs/security.md`: feedback text is untrusted input to the agent; the fenced wrapper reduces but does not remove injection risk, which is why the automatic path is capped and opt-in.
- If [adr-git-forges](adr-git-forges.md) is accepted, the poller calls the forge adapter instead of `gh`, so GitLab works too.
- Only the fake `gh` has been exercised so far; a real `gh pr create` against a throwaway repository (already in TODO) is a prerequisite.
- Tests: a fake `gh` that returns a failing check then a pass; the cap of three rounds; the watch ends on merge.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Finish the real `gh` check from TODO.
3. [ ] Host: watch store, poller, summariser, with fake-`gh` tests.
4. [ ] Desktop and Android: the `pr_feedback` item and its three actions.
