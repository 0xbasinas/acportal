# ADR: terminals that survive a dropped connection (and a host restart, later)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

The common remote-agent setup in 2026 is a three-layer stack: a mobile SSH client, Mosh or Eternal Terminal for roaming connections, and tmux so the session lives on the server whatever happens to the link. Guides suggest `ServerAliveInterval 30` and `ServerAliveCountMax 3` to notice dead connections early, and named tmux sessions to reattach. Mosh cannot forward ports; Eternal Terminal keeps scrollback. Coder's AgentAPI runs the agent in an in-memory terminal emulator on the server. See [research notes](adr-ade-index.md#research-notes).

This project's terminals (`pty.rs`, `terminals.rs`, `api/terminals.rs`) already run as host processes and the phone and desktop attach over a WebSocket. TODO lists the gap: "no tmux-style persistence across a host restart, no per-terminal title, no copy-paste helpers on the phone". It also lists untried phone behaviour after a connection drop (Ctrl toggle, arrow keys, Take control, reconnect).

## Decision

Make reconnection a defined, tested behaviour first; make a host restart survivable only if a clear case appears.

1. **Reattach across drops (build now).** A terminal keeps a bounded scrollback on the host (a screen-state-aware ring, default 1 MiB, configurable). A client that reconnects with the terminal id and its last sequence number receives the missing output, or a fresh screen snapshot with a "N bytes skipped" marker if the ring wrapped. The same visible-gap rule as the session journal.
2. **Liveness.** The WebSocket carries a ping every 20 seconds from the host and the clients show "Reconnecting..." after two missed replies, instead of a frozen screen. The phone reconnects with backoff and never replays keystrokes typed while it was disconnected (typed input while offline is shown as discarded, per the no-resend rule).
3. **Terminal metadata.** A per-terminal title (from the shell's title escape, bounded and sanitised), an owner label (which device or agent) and a "last output" time in the list. Named by the user on creation.
4. **Control.** Existing Take control behaviour stays: one writer at a time. A reconnecting writer keeps control for 30 seconds before it is released.
5. **Phone input helpers.** A compact key row (Esc, Tab, Ctrl toggle, arrows, `|`, `~`, `/`) and paste. These are client-only, tested on a device; the web client later reuses them.
6. **Host restart (deferred, option).** Surviving a restart needs a separate supervisor process that owns the PTYs, like tmux's server. This is a real second process with its own security story (who can attach, how it is stopped). This ADR does **not** build it; it records the shape so the decision is explicit: the existing `daemon.rs` instance records and `detached.rs` launch are the starting point, and the supervisor would hold only PTYs, not agents.

## Options Considered

### Option A: bounded host scrollback, sequence reattach, liveness (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A ring buffer, snapshot logic, reconnect handling in three clients. |
| Cost | Memory per terminal, bounded. |
| Scalability | The existing terminal limit applies. |
| Team familiarity | Same pattern as session journal replay. |

**Pros:** fixes the most common failure (phone sleeps, network flips) without a new process.
**Cons:** a snapshot of a full-screen program (an editor) needs terminal-state tracking, not raw bytes.

### Option B: delegate to tmux

**Pros:** mature. **Cons:** an external dependency and not available on Windows; the host would hide a second tool.

### Option C: Mosh-style state synchronisation protocol

**Pros:** best interactive behaviour on bad links. **Cons:** a protocol of its own; much more work.

## Consequences

- `docs/security.md` (Host terminals): scrollback may hold secrets typed or printed; it lives in memory only, is dropped on release and never logged.
- Tests: a client reconnects mid-output and receives the gap exactly once; a wrapped ring produces the skipped marker; discarded offline keystrokes are not sent.
- Verification on a real phone is required for the keyboard row and sleep behaviour; so far only the emulator was used.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: scrollback ring, sequence protocol, ping, tests.
3. [ ] Clients: reconnect UI and the phone key row.
4. [ ] Revisit the PTY supervisor only after real use shows host restarts hurt.
