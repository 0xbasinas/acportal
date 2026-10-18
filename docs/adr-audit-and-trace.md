# ADR: a permission audit trail and optional OpenTelemetry export

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

A 2026 Hacker News thread argues coding agents lack four things: a structured trace, cost attribution per task, a permission audit trail and session replay. Claude Code reports cost, an action record and (beta) traces enabled through environment variables. Langfuse and similar tools group OpenTelemetry traces by session; one project gives each session operation its own trace linked to the previous one and exports only when an OTLP endpoint is configured. See [research notes](adr-ade-index.md#research-notes).

This host already makes many security-relevant decisions: write consents, terminal consents, shell auto-review (`_meta.acpdAutoReview`), auto-approve, supervisor decisions, device pairing and revocation. The log keeps "decision plus layer only". The orchestrator already carries a `traceparent` in `_meta` (a keyword search found it in `orchestrator/mod.rs` and `session.rs`). What does not exist is one place to answer "what did this agent do, who allowed it, and when?" after the fact, particularly for unattended runs ([adr-scheduled-runs](adr-scheduled-runs.md)).

## Decision

Two layers, both off the critical path.

1. **Audit trail (always on, local).** A bounded, append-only file in the state directory (owner-only, rotated like the log sink) recording decisions, not content: time, session id, task id, device id (not token), kind (`write`, `terminal`, `shell`, `preset_change`, `auto_approve`, `pair`, `revoke`, `merge`, `checkpoint_restore`), outcome (`allowed`, `denied`, `auto`, `error`), the deciding layer (`phone`, `rules`, `model`, `supervisor`), and a short path or command *digest* (hash plus length, plus the path for filesystem items, which the phone already shows). Prompt text, file contents, command text, environment values and tokens are never written.
2. **Viewer.** `GET /v1/audit?session=&since=` (phone role) and a screen filtered by session, kind and outcome. The viewer is how a person reviews what an unattended run did. `acpd audit tail` gives the same view in a terminal.
3. **OpenTelemetry export (opt-in).** `[telemetry] otlp_endpoint` turns on spans for session, prompt turn, tool call and approval, linked by the existing `traceparent`. Attributes carry the same low-sensitivity fields as the audit trail plus token counts and cost where the agent reports them. Content is never exported. The endpoint must be `https` or loopback, and a failing exporter drops spans rather than blocking a turn.
4. **Tamper evidence.** Each audit line carries the SHA-256 of the previous line (a hash chain, as Bellows describes for its audit trail). `acpd audit verify` recomputes the chain and reports the first break. A chain proves later edits or deletions inside the file, not that the host user did not delete the whole file or the tail; an operator who needs more writes the chain head to an external place with a `notify` hook ([adr-host-hooks](adr-host-hooks.md)) after each day.
5. Retention: the audit file defaults to 30 days or 16 MiB, whichever is smaller, configurable. Export is best effort and not a durable record.

## Options Considered

### Option A: local audit trail now, OTLP export as an option (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A writer with rotation, a viewer, a span exporter. |
| Cost | Small disk; no cost unless an endpoint is configured. |
| Scalability | Bounded by retention. |
| Team familiarity | Reuses the logging sink code and decision types. |

**Pros:** answers the audit question without sending data anywhere; OTLP fits teams that already run Langfuse or a collector.
**Cons:** a digest alone can't tell you *what* a command did; a determined agent can't tamper with the log but the host user can.

### Option B: logs only

**Pros:** nothing new. **Cons:** rotates away; unstructured; no viewer.

### Option C: ship content to an observability platform

**Pros:** full replay. **Cons:** breaks the "never log prompts, replies or file contents" rule and needs a data-handling decision per platform.

## Consequences

- `docs/security.md` names what the audit trail holds and what it omits; it is local state, covered by owner-only permissions.
- Session replay stays the existing journal replay; this ADR does not add content storage.
- Tests: no secret or prompt text in a sample audit file (reuse the existing secret-scan style tests); rotation bound; exporter failure does not block a prompt.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: event type, writer, rotation, `/v1/audit`, `acpd audit tail`.
3. [ ] Desktop and Android: audit screen under session details.
4. [ ] Decide span names and attributes against the OpenTelemetry GenAI conventions after reading their current version.
