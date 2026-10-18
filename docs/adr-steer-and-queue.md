# ADR: queue, steer or interrupt while an agent is working

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Tools split input sent to a running agent into modes. A guide describes **steer** (applied at the next step of the agent loop), **follow-up/queue** (sent after the current turn ends) and **interrupt** (cancel only). Claude Code users have asked for a choice between steering and queuing, and reports of its present behaviour conflict. A commenter says Codex has a turn-steer API since February 2026, which I could not verify. Nimbalyst's phone app queues prompts to run after the current task, and the Codex changelog mentions fixes for "queued messages stuck on Sending" (8 October 2026). See [research notes](adr-ade-index.md#research-notes).

In this project the baseline ACP calls are `session/prompt` and `session/cancel`; I found no mid-turn input call in the protocol notes in `docs/orchestrator-design.md`, and a keyword search of the session screens found no queue or steer control. A phone user watching an agent head the wrong way can cancel, then type a new prompt. A user who just thought of a follow-up must wait. The project also has a hard rule: reconnect and replay must never resend a prompt.

## Decision

Offer three explicit actions while a turn is running, owned by the host so every client sees the same state.

1. **Queue**: the prompt is stored by the host on the session as `queued` with the client's request id. When the current turn ends with `end_turn`, the host sends the oldest queued prompt once. If the turn ends with `cancelled`, an error, or the agent exits, the queue is *held*, not sent, and the clients show "N queued prompts waiting. Send, edit or discard." This keeps the no-resend-after-failure rule.
2. **Steer**: sends `session/cancel`, waits for the cancelled stop reason, then sends the new text as a normal prompt prefixed by a host-configurable note ("The previous turn was interrupted by the user. Continue with this instead."). Labelled as cancel-and-replace, because baseline ACP has no true mid-turn injection. If an agent later advertises a standard capability for it, the label changes without changing the UI flow.
3. **Interrupt**: today's cancel.
4. The queue is bounded (10 prompts, 64 KiB each), lives in memory with the session, and is lost with the session. A host restart drops it and the clients say so.
5. A phone that is offline keeps its text as a **draft**. Drafts never become queued prompts on reconnect without a tap. This preserves the invariant that nothing is sent on reconnect without a user action.
6. Queued items and their order are published in the session journal as `_meta` entries so reconnecting clients see them.

## Options Considered

### Option A: host-owned queue, cancel-and-replace steer (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Queue state in the session actor, a few journal items, UI controls. |
| Cost | None. |
| Scalability | Per session, bounded. |
| Team familiarity | Same actor and request-id cache as prompts today. |

**Pros:** works with every ACP agent; one source of truth for phone and desktop; failure holds the queue.
**Cons:** steer loses the agent's in-flight step; the replacement prompt costs tokens to re-establish context.

### Option B: client-side queue only

**Pros:** no host change. **Cons:** the phone and desktop disagree; closing the app drops it; two clients can both send.

### Option C: wait for a protocol-level steering method

**Pros:** cleanest. **Cons:** unknown timing and agent support.

## Consequences

- `docs/protocol.md` documents the queue item and that a held queue needs a person.
- The host-side request-id cache already de-duplicates prompts, so a repeated client request cannot queue twice.
- Tests: queue drains in order after `end_turn`; held after `cancelled`; steer sends cancel before the new prompt; reconnect does not send drafts.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Check what Goose and one other agent do with a prompt sent while a turn is running; record in `docs/real-agent-testing.md`.
3. [ ] Host: queue state, journal items, tests.
4. [ ] Desktop and Android: composer controls and the held-queue banner.
