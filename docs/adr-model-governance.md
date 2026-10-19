# ADR: allowed models per agent, and a prompt egress check

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

A gateway field guide for coding agents lists five control surfaces: access control, cost attribution (rolled up by developer, repo, team or cost center, per call or per session), audit logging, DLP on prompt egress, and model governance and routing. Its examples: an allowlist of approved models per agent set in gateway config and locked at install; budgets that start as soft alerts during a canary and become hard caps; routing rules that keep restricted repositories on approved endpoints; and DLP that scans outgoing prompts for secrets, PII and regulated content with pattern matching, noting that regex-only DLP misses novel content. It warns that gateway fingerprints of agents are an analytics aid, not a security boundary, and that cost figures differ from invoices by a few percent. See [research notes](adr-ade-index.md#research-notes).

This project does not proxy model traffic (agents call their providers), but the host does control two relevant things: which model an ACP session uses, through session configuration options and the model picker, and every prompt that goes from a phone or desktop to an agent. A keyword search found neither an allowed-models setting nor any check of outgoing prompts.

## Decision

Two small host-side controls that need no model gateway. Both are off by default and informative before they are restrictive.

1. **Allowed models.** `[agent.<id>] allowed_models = ["deepseek-flash", "gpt-*"]` (glob patterns against the model ids the agent advertises through ACP config options). The host filters the model choices it passes to clients to the allowed ones and refuses a `session/set_config_option` or `session/set_model` call for any other model with a readable error. A recipe or profile can narrow further ([adr-workflow-recipes](adr-workflow-recipes.md), [adr-session-modes](adr-session-modes.md)). Models are ids the agent advertises, so the list needs no agent-specific knowledge. If the agent's *default* model is not on the list, the host reports a configuration warning rather than blocking the session start (the host cannot change a default it does not control).
2. **Honest limits.** An agent can still choose a model by its own settings or environment; this controls only what ACP exposes. `doctor` says so, and the session header shows the model the agent reports.
3. **Prompt egress check.** Before a user prompt (and each attachment) is forwarded, the host scans it with a small set of patterns: private key blocks, cloud and API key prefixes, `Authorization: Bearer ...`, long `NAME=value` lines from `.env` files, and optionally values of the agent's configured environment secrets (exact match). Modes: `off`, `warn` (the sender sees "This looks like a secret: ..." and chooses Send anyway or Edit; default when enabled), `block` (the host refuses with the same message; the person edits). Matches are masked in the message and never logged; the log records only that a check fired and which pattern class.
4. **Attribution fields.** The usage records ([adr-usage-dashboard](adr-usage-dashboard.md)) gain optional dimensions that the host already knows: agent, project, device, recipe and task. No new data is collected from providers; where an agent reports no cost the row says "not reported".
5. **Caps.** The soft alert from the usage ADR is the only budget-like feature here; a hard cap that refuses new prompts is deliberately not proposed (the existing orchestrator budgets for delegated tasks stay as they are).

## Options Considered

### Option A: allowlist at the ACP boundary and a warn-first egress scan (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A filter on config options and a pattern scanner. |
| Cost | None. |
| Scalability | Per prompt; cheap. |
| Team familiarity | The model picker and config options code exist. |

**Pros:** useful for teams and for a careful individual; honest about what it cannot control; no provider integration.
**Cons:** patterns miss secrets and false-positive on test data; an allowlist cannot stop an agent that picks its own model.

### Option B: a full model gateway the host runs

**Pros:** real control and cost data. **Cons:** a proxy for every provider API shape; see [adr-credential-proxy](adr-credential-proxy.md) for the limited version that exists in this set.

### Option C: nothing

**Pros:** no friction. **Cons:** a pasted key goes to a provider without a warning.

## Consequences

- `docs/security.md`: the egress check is a safety net and runs locally; it sends nothing anywhere.
- Tests: glob matching against advertised ids; a refused model change leaves the previous model; fixtures of secrets match and ordinary hex or base64 does not; block mode refuses and masks.
- The check overlaps the chip-tray warning in [adr-context-attachments](adr-context-attachments.md); share one pattern set so the two agree.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: pattern set, scanner, modes; allowed-models filter.
3. [ ] Clients: the warning dialog and the filtered model picker.
