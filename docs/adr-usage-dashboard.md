# ADR: a usage and cost view from ACP `usage_update`

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Orca shows Claude and Codex usage and rate-limit resets. Run-comparison screens in this project already show cost and tokens per run (`Multi.tsx`, `WatchSnap.usage`). ACP lets agents send `usage_update` with required `used` and `size` token counts and an optional `cost {amount, currency}` (orchestrator design, protocol table). The orchestrator enforces wall-clock and turn budgets, and token or cost budgets only from `usage_update`; the design records a token budget for ordinary sessions as "decided against".

So the data arrives but is not kept: usage is visible only on a live session or run, and there is no per-day or per-project total. This ADR does **not** reopen the decision against budgets.

## Decision

Record usage per session on the host and show it passively.

1. The host stores, per session, the last `usage_update` and a small time series (one sample per minute at most, 24 hours of samples, bounded). Cost is kept per currency and never summed across currencies.
2. `GET /v1/usage?since=` returns per-session totals and per-day sums by agent and project. Agents that never send `usage_update` appear as "not reported", not as zero.
3. Clients show: the status bar (current session tokens and cost when reported), the Sessions list (small figure per row), and a usage page (per day, per agent, per project).
4. Context size (`used` out of `size`) gets a warning style at 80% and 95% so a person can start a new session before compaction.
5. No enforcement. A later ADR may add soft limits that only *ask the phone*, never silently stop a prompt.

## Options Considered

### Option A: host-recorded, passive display (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low. A bounded store and one route. |
| Cost | Small memory and disk (a few KiB per session). |
| Scalability | Bounded per session. |
| Team familiarity | Same catalog storage as interrupted-session metadata. |

**Pros:** history survives a client closing; phone and desktop agree.
**Cons:** only as accurate as what the agent reports; some agents report none.

### Option B: client-side only

**Pros:** no host change. **Cons:** lost when the app is closed; phone and desktop differ.

### Option C: ask providers for their own usage APIs

**Pros:** exact numbers and rate-limit windows. **Cons:** needs provider keys in the host, which the project avoids; agent specific.

## Consequences

- Usage numbers are metadata, not prompt content, so the log rules are unchanged.
- The page must say it is the agent's own report, not a bill.
- Tests: update ordering, missing fields, a currency change mid-session, and the bound on samples.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: store and `/v1/usage`, with tests.
3. [ ] Desktop: status bar field and usage page.
4. [ ] Android: row figure and a simple page.
5. [ ] Check what Goose and at least one other registry agent actually send before designing the page.
