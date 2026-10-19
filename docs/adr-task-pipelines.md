# ADR: short task pipelines with acceptance gates

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Gold Band (a local-first ACP desktop client) models work as an explicit workflow: each node is one agent execution with its own agent, model, role and result evaluation (acceptance criteria), and edges define transitions after success, failure or manual confirmation. Its AUTO mode splits a goal into subtasks that run in separate worktrees, then a merge node combines them and an accept node validates; the runtime validates proposed nodes and agents cannot change runtime state. Runs can stop, resume and loop on failure, and wait for human confirmation. Kepler offers built-in Actions for implementing issues, reviewing PRs and addressing feedback, with editable prompts, models and skills. Codeg runs to-dos in their own worktree, optionally on a schedule, and waits for review before landing the diff. See [research notes](adr-ade-index.md#research-notes).

This project has the parts: delegated tasks and worktrees, checks, a model-based review prompt in "Run on several agents", merge confirmation, and (proposed) recipes ([adr-workflow-recipes](adr-workflow-recipes.md)) with follow-up *offers*. What is missing is the middle: "implement, then run checks, then have a second agent review, then ask me", defined once and run unattended up to the point where a person must decide.

## Decision

Add **pipelines**: ordered lists of at most five steps, built from recipes, with explicit gates. Deliberately not a general graph.

1. A pipeline is `{id, title, steps[]}`. A step is `{recipe, agent?, on_success, on_failure}`. `on_success` is `next`, `confirm` (wait for a person) or `done`; `on_failure` is `stop`, `retry` (at most 2) or `confirm`. No branches, no loops other than bounded retry, no expressions.
2. **Gates** are the acceptance criteria, evaluated by the host, never by the agent that did the work:
   - `checks`: the configured checks command set exits 0 (the existing checks runner).
   - `diff`: a non-empty diff, optionally within a size limit, and no `high` risk files when the step says so ([adr-risk-ranked-review](adr-risk-ranked-review.md)).
   - `review`: a different agent (or the same with a stricter profile) is asked to review the diff with a fixed prompt and must answer in a requested structure (`APPROVE` or `CHANGES` with reasons). A parse failure counts as `confirm`, not approval. A model's approval alone never lets a pipeline merge; the final merge is always a person (below).
   - `manual`: wait for a tap.
3. All steps of one pipeline run in the same task worktree in sequence (isolation inherited from the first step), so each step sees the previous step's changes. A step that is a review runs read-only (profile with no write approval).
4. **Limits**: the pipeline carries one budget (wall clock, turns, `usage_update` limits where reported) shared by all steps; exceeding it stops the pipeline and marks it `needs_attention`. Stop and resume work at step boundaries; a resumed pipeline repeats no completed step.
5. **Never**: merge, push or open a pull request without a person. The last step may be `confirm` followed by the existing phone-confirmed merge. Scheduled use ([adr-scheduled-runs](adr-scheduled-runs.md)) inherits this.
6. The attention board shows the pipeline as one row with the current step; the activity panel ([adr-activity-panel](adr-activity-panel.md)) nests its steps.

Design note: Bernstein schedules from a task graph with a deterministic scheduler that uses no LLM tokens, and Microsoft's Conductor routes conditionally without an LLM in the loop. The runner here is the same kind of thing: a deterministic state machine over host-evaluated gates; models only do the work inside a step. The [merge queue](adr-merge-queue.md) and [claim verification](adr-claim-verification.md) feed the final gate.

## Options Considered

### Option A: linear pipelines of recipes with host-evaluated gates (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A runner state machine, a gate evaluator, a small editor. |
| Cost | Provider spend per step, bounded by the shared budget. |
| Scalability | Five steps; a few concurrent pipelines. |
| Team familiarity | Reuses task, checks and recipe code. |

**Pros:** covers the common "implement, verify, review, ask me" job; gates are objective where possible; unattended up to a safe point.
**Cons:** limited expressiveness; review by a model is advice, not proof (same-model blind spots).

### Option B: a general node-and-edge workflow engine (Gold Band's model)

**Pros:** powerful. **Cons:** a product of its own, with a visual editor and failure semantics; start narrow.

### Option C: let one orchestrating agent drive the sequence

**Pros:** flexible. **Cons:** the orchestrator is the thing being gated; a host-owned runner keeps decisions out of the agent's hands (the orchestrator design's own stance).

## Consequences

- Pipelines store recipe ids and parameter values only; prompts live in recipes.
- Tests: a failing check stops the pipeline at the gate; retry count enforced; an unparseable review reply becomes `confirm`; a resumed pipeline does not rerun completed steps; budgets shared across steps.
- Parallel fan-out and best-of-n remain with the existing "Run on several agents" screen.

## Action Items

1. [ ] User reviews this ADR; accept [adr-workflow-recipes](adr-workflow-recipes.md) first.
2. [ ] Host: runner, gate evaluator, records, tests.
3. [ ] Desktop: pipeline editor and run view; Android: run view and confirm actions.
