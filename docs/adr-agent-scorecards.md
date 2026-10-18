# ADR: agent scorecards from your own task history

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Public leaderboards are unreliable for choosing an agent: OpenAI reportedly withdrew its SWE-Bench Pro suite after roughly 30 percent of tasks proved broken, and rankings changed within a month on one aggregator. Databricks ran an internal test built from its own merged pull requests and reported that a cheaper model matched a frontier model on quality at lower cost per completed task (a vendor-adjacent article, single source). Writers argue the useful metric is cost per *accepted* result, not per attempt, and one analysis divides task cost by merge rate. I found no router that picks an agent per task automatically; the advice is to log cost and pass or fail per task type and route from your own data. See [research notes](adr-ade-index.md#research-notes).

This host records what that needs and shows it in pieces: task records with outcome, the checks runner (pass or fail), the merge decision on the phone, `usage_update` cost and tokens, the run-compare screen with side-by-side cost, and review results. No screen answers "over the last month, which of my agents actually ended up merged, at what cost, in this repository?".

## Decision

Show a **scorecard** per agent (and per agent and project) computed from records the host already keeps. It informs a choice; it never makes one.

1. Metrics, each with its count so small samples are obvious:
   - tasks started, finished, failed, cancelled;
   - **accepted rate**: finished tasks whose changes were merged (or whose pull request merged, when the forge adapter knows) divided by finished tasks;
   - **checks pass rate** on the first run;
   - **cost per accepted change** (sum of reported cost for the tasks divided by accepted tasks), per currency, only over tasks where the agent reported cost; "not reported" otherwise;
   - median wall-clock time and median tokens per finished task;
   - approvals asked per task (a measure of friction) and how many the person denied.
2. Slicing: by project, by task kind where recipes ([adr-workflow-recipes](adr-workflow-recipes.md)) give one, and by a 30 or 90 day window. Rows below five tasks display "not enough data".
3. Where: the agent details screen and the Run on several agents screen (as a line under each candidate: "Last 30 days here: 7 tasks, 5 accepted"). A task kind with a clear gap can show a quiet hint, never a default.
4. Storage: computed on read from task records, with a small derived table the host rebuilds if it is missing. Task records are retained under the retention rules in [adr-data-lifecycle](adr-data-lifecycle.md); when they are deleted, so are the figures.
5. **No automatic routing.** A person picks the agent. A later ADR can consider a "suggest an agent" button that uses this data, once there is enough use to judge it.
6. Honest caveats on screen: acceptance depends on how the person reviews, a merged change can be wrong, and cost excludes subscription plans where the agent reports none.

## Options Considered

### Option A: derived scorecards from existing records, advice only (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A query layer and two small views. |
| Cost | None. |
| Scalability | Aggregates over task records, bounded by retention. |
| Team familiarity | Task records and views exist. |

**Pros:** answers a real question with the person's own data; no new collection; private.
**Cons:** needs enough history to say anything; "accepted" is a weak proxy for quality.

### Option B: auto-route tasks to the best-scoring agent

**Pros:** convenient. **Cons:** opaque; a rare bad run biases future choices; hides which agent touched the code.

### Option C: run synthetic benchmarks on the repository

**Pros:** controlled comparison. **Cost:** real money and a benchmark to maintain; the Run on several agents screen already provides ad hoc comparison.

## Consequences

- Scorecard figures are metadata, not content, and are as sensitive as the task list.
- Tests: metric computations on seeded records with missing costs, mixed currencies, cancelled tasks; the "not enough data" threshold.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: derived metrics and `GET /v1/agents/{id}/scorecard`.
3. [ ] Clients: agent details and compare-screen lines.
