# ADR: detect stuck, looping and crashed agents

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Gas Town describes watchdog roles (Witness and Deacon in the search summaries; the DoltHub article did not cover them, so details are unverified) and a coordinator that "prods" agents that stop short. Symphony's orchestrator polls and restarts agents that crash or stall. Agent Orchestrator runs a local daemon that polls for crashes and restarts, and retries a flaky CI failure before escalating. The Gas Town write-up records a coordinator reporting bugs fixed when only two PRs existed, because work was committed locally but not pushed. See [research notes](adr-ade-index.md#research-notes).

This host already cancels *delegated tasks* that produce no event for `idle_timeout_seconds` (default 300; waiting for a person is not idle; `orchestrator/mod.rs`). Interactive sessions have no such watch: an agent can repeat the same failing tool call for an hour, or spin in a loop spending tokens, while the person sees only a long-running turn. A person on a phone cannot easily tell "thinking" from "stuck".

## Decision

Detect the common stuck patterns from events the host already sees and surface them; do not stop or restart anything automatically.

1. **Signals**, computed per running turn from the journal (no content inspected beyond names and statuses):
   - **Silence**: no event for N seconds while not waiting for a person (default 300 for a prompt turn, matching the task idle timeout).
   - **Repeat**: the same tool kind with the same title or command and the same outcome repeated 5 times in a row, or 8 times within the last 12 calls (a tool title is already visible on the card; comparison uses a hash, not content).
   - **Thrash**: the same file written 6 or more times in one turn, or a write followed by a revert of the same path (seen by the host as the agent's own consecutive writes).
   - **Failure streak**: 5 consecutive failed tool calls or non-zero exits.
   - **Budget pace**: tokens or cost per minute far above the session's median, only where `usage_update` reports them.
   - **Crash**: the agent process exited or the connection dropped with a turn open (already detected; it becomes a visible state).
2. **Surface**: a "May be stuck: repeated the same command 5 times" chip on the session, the attention board row ([adr-attention-board](adr-attention-board.md)) and the activity panel ([adr-activity-panel](adr-activity-panel.md)), and optionally a `stuck_suspected` hook event ([adr-host-hooks](adr-host-hooks.md)) with a notification. Thresholds are configurable per agent; a signal can be dismissed for the rest of the turn.
3. **Actions, all explicit**: Interrupt (cancel the turn), Steer ("You have repeated X several times; try a different approach", sent as a normal prompt via [adr-steer-and-queue](adr-steer-and-queue.md)), or Start fresh with a handoff note ([adr-context-handoff](adr-context-handoff.md)). A crashed session offers Resume (`session/load` when advertised) or Start fresh. None runs without a tap. Restarting with the old prompt is never automatic, because that would resend a prompt.
4. **Delegated tasks** keep the existing idle timeout, which cancels; this ADR's signals also appear on their task rows so the person sees why a task was cancelled.
5. **False positives** are expected (a long build is silent). The chip says "possibly" and the thresholds exclude time while a host terminal command the turn started is still running.

## Options Considered

### Option A: host-side signals, explicit actions (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. Counters over the event stream and a chip. |
| Cost | Negligible. |
| Scalability | Per running turn. |
| Team familiarity | Same stream as the idle-timeout logic. |

**Pros:** catches expensive loops early; works for every agent; no hidden side effects.
**Cons:** heuristics misfire; a quiet agent in a long legitimate step looks stuck.

### Option B: auto-interrupt on a signal

**Pros:** saves money. **Cons:** kills legitimate work; contradicts the person-in-the-loop rule.

### Option C: a supervising model reads the transcript

**Pros:** semantic. **Cons:** cost, content leaves the host, injection risk; the orchestrator's supervisor already exists as an explicit opt-in.

## Consequences

- Tests with the mock agent scripted to repeat a tool call, thrash a file and go silent; a terminal-running turn does not trip the silence signal.
- Thresholds are heuristics and documented as such.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: detectors and a `stuck` field on session info, with tests.
3. [ ] Clients: chip and actions.
