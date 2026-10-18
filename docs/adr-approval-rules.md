# ADR: turn an approval into a reviewable standing rule

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Bellows (a desktop ACP client) lets you turn an answer into a *local rule* so harmless actions stop prompting, shows the same permission prompt whichever agent asks, and writes every allow, deny and saved rule to a hash-linked audit trail. Saving a rule requires being at the machine even when a prompt can be answered from the phone. Harnss offers three levels (Ask First, Accept Edits, Allow All) switchable mid-session. Warp has command allowlists and denylists. Reports on Cline and Roo Code mention auto-approval workflows. See [research notes](adr-ade-index.md#research-notes).

Here the host has strong automation (rules and model reviewer for shell lines, per-agent auto-approve with a risk ceiling, "Ask for everything", "Auto-approve everything" per session) but the code deliberately never uses the ACP `allow_always` option: a supervisor or auto-approve decision "is one request only" (`orchestrator/approvals.rs`, `auto_approve.rs`). So when a person gets the same prompt for the fortieth time ("run `cargo test`") the only options are to approve again or to widen auto-approve in host config on a computer. The phone cannot create a rule.

## Decision

Offer **Allow and remember** on a pending request. It creates a *host-side rule object*, never an agent-side permanent grant.

1. The prompt UI gains a second button, "Allow and remember...", which opens a short rule editor prefilled from the request: kind (`shell`, `write`, `read-outside`), agent, workspace, and a **pattern** the person can only *narrow*, not widen (for a shell line: the exact command and arguments; for a write: the exact path or its directory; never "anything").
2. Scope is chosen from a closed list: this session, this task, this workspace for this agent (default), or all workspaces for this agent. The widest scope needs a second confirmation. Expiry: session, 24 hours, 30 days (default), or never.
3. A rule is stored host-side (in the user data store beside recipes), evaluated by the same code as auto-approve *after* hard-deny rules and the risk ceiling, so a remembered allow can never override a hard deny (`sudo`, recursive delete of system paths, writes to `high` risk paths if the operator set that, secrets files). Shell lines that the lexer marks sensitive cannot be remembered at all.
4. Only the owner device (rights `owner` or `operator`, see [adr-device-roles](adr-device-roles.md)) may save a rule. An approver-level device can allow once.
5. A **Rules** screen lists every rule with agent, scope, pattern, creation device and time, hit count and last used, with Revoke. Rules that have not matched in 30 days are flagged. Each use is an audit event with the rule id ([adr-audit-and-trace](adr-audit-and-trace.md)); the audit trail is where "who allowed this" is answered.
6. The ACP option sent to the agent for a rule-approved request is `allow_once`, as with auto-approve today. The agent never receives `allow_always`, so removing the rule takes effect on the next request even for agents that cache grants.

[adr-policy-engine](adr-policy-engine.md) proposes making these rules, auto-approve and shell review inputs to one policy evaluation with simulation; rules stored here would become `ask`-class overrides there.

## Options Considered

### Option A: host-side rules with narrow patterns, one scope list (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A rule type, a matcher shared with auto-approve, an editor and a list. |
| Cost | None. |
| Scalability | Hundreds of rules; matching is linear and cheap. |
| Team familiarity | Same decision layers as `auto_approve.rs` and `shell_review.rs`. |

**Pros:** ends repeated prompts without handing agents a permanent grant; reviewable and revocable from the phone; consistent across agents.
**Cons:** rules accumulate; a narrow pattern still lets prompt-injected commands that match it run, which is why it is exact by default.

### Option B: forward ACP `allow_always` to the agent

**Pros:** trivial. **Cons:** the agent owns the grant, the host cannot list or revoke it, and agents differ on what it means.

### Option C: edit config files on the computer only

**Pros:** safest. **Cons:** the status quo; the phone user stays stuck in prompt loops.

## Consequences

- `docs/security.md`: a remembered rule is an operator decision made from a phone; list its limits (exact patterns, hard denies win, owner-only, expiry).
- Tests: a rule never matches outside its scope; widening a pattern is impossible in the editor and the API; hard deny wins; revoked rule stops matching immediately; approver devices cannot save.
- A hash chain over the audit entries (see the amendment to [adr-audit-and-trace](adr-audit-and-trace.md)) makes rule history tamper-evident.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: rule store, matcher integration, endpoints, tests.
3. [ ] Desktop and Android: the editor on the approval sheet and the Rules screen.
4. [ ] Decide default expiry (30 days suggested).
