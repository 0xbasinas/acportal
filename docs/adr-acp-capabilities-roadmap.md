# ADR: which ACP features our clients should advertise and use, in order

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Many of the ideas in this set can be done *through* the protocol instead of beside it, which keeps the project protocol-uniform (no per-agent branches). I read the ACP RFD updates page and ten RFDs on 18 October 2026. Statuses below are as that page showed them; several are drafts and can change. See [research notes](adr-ade-index.md#research-notes).

| RFD | Status (as read) | What it gives a client | Ideas it supports |
|---|---|---|---|
| Elicitation (`elicitation/create`) | Completed 24 Jul 2026 | Structured questions from the agent | Already built (form elicitation) |
| Session usage (`usage_update`) | Completed 5 Jun 2026 | `used`, `size`, cumulative `cost` with currency | [usage dashboard](adr-usage-dashboard.md), [context handoff](adr-context-handoff.md) |
| End-turn token usage | Draft | Per-turn token accounting | Usage dashboard (later) |
| Session resume / delete / close | Resume completed 22 Apr; delete completed 5 Jun; close present | Resume without replay; delete in the agent's store | [idle suspend](adr-idle-suspend.md); delete from the agent when a session is deleted here |
| Additional directories | Completed 1 Jun 2026 (Preview 21 May) | `additionalDirectories` on `session/new`, `load`, `resume`, `fork`, listed in `session/list` | [multi-repo tasks](adr-multi-repo-tasks.md) |
| Message IDs | Completed 5 Jun 2026 (optional in v1, required in v2) | Stable ids per message | Deduping replay, [conversation fork](adr-conversation-fork.md), [steer and queue](adr-steer-and-queue.md) confirmation |
| Session notices | Completed 9 Oct 2026; Zed, Claude and Codex adapters ship it | `notice` updates with severity, title, description; need `clientCapabilities.session.notices` | A notice strip instead of fake agent messages; warnings such as "MCP server unavailable" |
| Terminal authentication | Completed 20 Aug 2026 | Sign in by running a login command in a terminal | [registry browser](adr-registry-browser.md) sign-in flow |
| Logout; auth state query | Logout completed 22 May; auth/status draft | Sign out and ask for state | First-run setup and doctor |
| Session compaction | Draft | `compaction_update` and summary chunks, agent-initiated; client needs `clientCapabilities.session.compaction` in v1 | Show compaction in the timeline; [context handoff](adr-context-handoff.md) |
| Subagents | Draft | `subagent_update` with ids, titles, state, `cancel`; child activity on its own session id | [activity panel](adr-activity-panel.md), task tree |
| Plan operations | Draft (2 Apr 2026) | `plan_update` for multiple plans by `planId`, as items, file or markdown; `plan_removed` | [plan review](adr-plan-review.md) |
| Client system prompt | Draft, unstable flag | `systemPrompt {mode: append|override, content[]}` on `session/new` | [project context](adr-project-context.md) shared rules |
| Custom LLM endpoint (`providers/set`) | Draft | Point the agent at a base URL with headers, process-scoped, never listed back | [credential proxy](adr-credential-proxy.md) |
| MCP over ACP | Draft | Client provides MCP tools over the existing connection (`type: "acp"`) | [agent browser tools](adr-agent-browser-tools.md) |
| Session fork | Draft | `session/fork` makes a new session from an existing one; message-id fork point is future work | [conversation fork](adr-conversation-fork.md) |
| Proxy chains | Draft | Components between client and agent | Watch |
| Streamable HTTP and WebSocket transport | Draft (22 Apr 2026) | Remote agents without a local process | Orchestrator design phase 4 |
| Agent telemetry export | Removed 2 Jul 2026 | (Removed: relied on client-injected variables that do not work for remote agents) | Supports host-side OTel in [audit and trace](adr-audit-and-trace.md) |
| Next edit suggestions | Removed 30 Sep 2026 | none | none |
| v2 (prompting, enums, plan variants) | Draft | Required message ids and more | Plan a compatibility pass |

## Decision

1. **Advertise what we handle, nothing else.** Clients advertise a capability only after the feature works end to end. Order of work, cheapest first: **session notices** (both clients), **message ids** (use them; keep working without), **additional directories** (host passes it through when the agent advertises it), **terminal authentication** (as a host terminal), **compaction display**, **subagent display**, **plan operations**.
2. **Host mediates.** The host advertises client capabilities to agents for its clients and intersects them with what connected clients can render, so an agent is not told "notices supported" when no client is attached that shows them. Unsupported updates fall back to what the agent would do without the capability (an ordinary message).
3. **Draft features stay behind a flag.** `systemPrompt`, `providers/set`, MCP over ACP and `session/fork` are used only when the agent advertises them, and the host config has an `[acp] unstable = ["system_prompt", "providers"]` list that is empty by default. A draft that changes shape does not break a released build because nothing uses it unless the operator listed it.
4. **Record per-agent support.** `acpd agents` and the Add agent screen show which of these capabilities each agent advertised (read from `initialize`, never assumed), which also feeds [protocol inspector](adr-protocol-inspector.md).
5. **Review this table each quarter.** The ADR is a living index; update the statuses and strike entries that are removed. The orchestrator design's protocol table (`docs/orchestrator-design.md`) has the same drift problem, so link rather than copy.
6. **v2**: do not implement now. Add a compatibility test that replays recorded v1 sessions through the reducer with message ids stripped and present, so the v2 move is a flag, not a rewrite.

## Options Considered

### Option A: a capability roadmap with an unstable list (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low for the plan, varying per feature. |
| Cost | Reading RFDs; no runtime cost. |
| Scalability | n/a |
| Team familiarity | The reducer and `initialize` handling already exist. |

**Pros:** many wanted features come with agent support already (notices ship in three clients' adapters); stays uniform; one place to see what the protocol offers.
**Cons:** drafts move; each feature needs real-agent checks.

### Option B: build features locally and ignore the RFDs

**Pros:** independent of protocol churn. **Cons:** duplicates what agents will provide (compaction, subagents) and diverges from other clients.

### Option C: wait for v2

**Pros:** fewer changes. **Cons:** v2 is a draft with no date; several v1 features are already completed.

## Consequences

- Statuses here are from one page read on 18 October 2026; none of the RFD texts beyond the ten read were checked.
- Tests: reducer tests per update type with and without the capability; the host's capability intersection.
- `docs/protocol.md` gains a pointer to this table.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Read the remaining RFDs (request cancellation, `meta-propagation`, `boolean-config-option`, `model-config-category`, `diff-delete`, `tool-call-name`, `session-notices` details) and add rows.
3. [ ] Implement notices and message ids in the reducer and both clients.
4. [ ] Real-agent check: which of Goose and one other agent advertise each completed capability.
