# ADR: one declarative policy for every agent action, with simulation

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Agent "control plane" products converge on one idea: a policy layer between agents and what they act on. An MCP-gateway article describes four outcomes for every tool call, **allow, deny, require approval, or require justification**, written as declarative YAML that can inspect arguments as well as tool names (for example denying a shell call whose command contains a destructive Terraform operation), usually in CEL or OPA expressions, with denials that return a message the agent can read and retry on. Approvers should see the tool, the arguments, the agent's reasoning, the session and the matched policy. The same source advises keeping the rule set small, each rule tied to a risk someone can name. Google Cloud Next 2026 announced agent identity, registry, gateway, simulation, evaluation and observability. Another guide warns that a model gateway sees model calls only, while tool calls run locally and need separate governance. See [research notes](adr-ade-index.md#research-notes).

Here the decision logic is spread across several places: the host's filesystem and terminal consent, the shell-line lexer and allowlist (`shell_review.rs`), per-agent auto-approve with a risk ceiling (`auto_approve.rs`), the "Ask for everything" and "Auto-approve everything" per-session switches (`presets.rs`), workspace access policy, and the proposed [approval rules](adr-approval-rules.md), [risk-ranked review](adr-risk-ranked-review.md) and [model governance](adr-model-governance.md). Each has its own config and its own explanation. A person cannot ask "why was this allowed?" or "what would change if I tightened this?".

## Decision

Define one **policy** evaluated by the host for every consent-shaped request, and keep the existing mechanisms as its inputs rather than rewriting them.

1. **Request kinds** (closed): `fs.write`, `fs.read_outside`, `terminal.run`, `shell.line`, `net.egress` ([adr-credential-proxy](adr-credential-proxy.md)), `mcp.tool` ([adr-mcp-gateway](adr-mcp-gateway.md)), `git.push`, `git.merge`, `model.select`. Each request carries its attributes (path, command text, domain, tool name, risk label, agent, workspace, device, profile).
2. **Outcomes** (closed): `allow`, `deny`, `ask`, `ask_justified`. The last asks the agent for a one-line reason via the denial path described below; the reason is shown on the approval sheet and recorded. There is no `allow_always` to the agent (see [approval rules](adr-approval-rules.md)).
3. **Rules** are an ordered list in one file: `{id, when, outcome, message}`. `when` is a small, non-Turing-complete predicate language over the request attributes (equality, prefix, glob, regex with a length limit, `in`, `and`/`or`/`not`). I recommend a minimal purpose-built grammar over embedding CEL or OPA; the choice is an action item. First match wins; the default is `ask`. Hard denies are a fixed first group that cannot be overridden by later rules.
4. **Existing mechanisms feed it**: the shell lexer classification, auto-approve's risk ceiling and path checks and remembered approval rules become *predicates and rule sources* ("shell.risk == sensitive", "rule 7 matched"), not parallel decision paths. Migration is incremental: the engine first wraps the current behaviour and reproduces it exactly, verified by running both and comparing on recorded requests.
5. **Messages for the agent**: a `deny` returns the rule's `message` as the ACP error text (bounded, no secrets), for example "Writes to .github/workflows need a person. Ask the user." so the agent can adapt instead of retrying blindly.
6. **Explain**: every decision carries the matched rule id. The approval sheet shows "Asked because: rule `ci-files`" and the audit trail records it ([adr-audit-and-trace](adr-audit-and-trace.md)).
7. **Simulation**: `acpd policy test` replays recorded request metadata (the audit trail's kind, outcome, rule, digests and path-or-domain; not content) against a candidate policy and prints how many past decisions would change and which ones, before the policy is activated. Activation is explicit, validated and soft-reloaded like other config.
8. Rights: only an owner device may edit policy ([adr-device-roles](adr-device-roles.md)).

## Options Considered

### Option A: one policy file, wrapping existing logic, with simulation (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | High. A grammar, an evaluator, migration of five mechanisms, a test replay tool. |
| Cost | None at runtime. |
| Scalability | Rules evaluated per request in microseconds. |
| Team familiarity | Decision outcomes already exist; the new part is unification. |

**Pros:** one place to read and change policy; explainable decisions; safe to change because of the replay; matches how control-plane products frame the problem.
**Cons:** a large refactor in a security-critical area; a new mini-language to maintain and secure.

### Option B: leave the mechanisms separate and add an Explain screen

**Pros:** no refactor. **Cons:** explanations stay per mechanism; no simulation across them.

### Option C: embed OPA or CEL

**Pros:** established languages. **Cons:** a dependency and a larger attack surface; evaluation semantics the article does not specify and that this project would still have to restrict.

## Consequences

- `docs/security.md` is rewritten around the policy as the single source of truth once it lands; until then it is unchanged.
- Tests: golden tests that the engine reproduces current shell-review and auto-approve outcomes on recorded cases; first-match ordering; hard denies immune to later rules; a predicate with a pathological regex hits its limit.
- Build after [approval rules](adr-approval-rules.md) and [audit and trace](adr-audit-and-trace.md); it consumes both.

## Action Items

1. [ ] User reviews this ADR, especially the choice of own grammar versus CEL.
2. [ ] Host: evaluator and a read-only "wrap and compare" mode first.
3. [ ] `acpd policy test` replay.
4. [ ] Clients: matched-rule line on approvals; a read-only policy view.
