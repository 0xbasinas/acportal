# ADR: a protocol inspector for agent traffic

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Two ACP clients make the wire visible. ACP Inspector (newioapp) is a debugger: it shows every JSON-RPC message with direction indicators, filters by method, searches text, groups streaming chunks into colour-coded blocks, shows the agent's capabilities, prompt types, MCP transports and auth methods, and lets you send prompts, cancel and answer permission requests inline. AcpAgent Client (Windows, Flutter) has a traffic panel of redacted raw JSON-RPC lines, written also to a daily log file, with length caps for expansion but a "copy line" that copies the original. See [research notes](adr-ade-index.md#research-notes).

When an agent misbehaves here (a Goose prompt fails with `-32603`, a tool call never completes, a session update is dropped), the tools are `frame_metadata` logging in the host (direction, kind, method name, error code, byte size, never content), `acpd doctor --agent` and `acpd probe/chat`. All are command-line, and the host log deliberately has no content. The apps cannot show what an agent actually sent. The Android app has Connection logs, but those are about the transport, not ACP messages.

## Decision

Add an opt-in **inspector** with two levels, so the safe one is always available and the sensitive one is a deliberate act.

1. **Metadata view (always available to the owner).** A per-session timeline built from the same fields as `frame_metadata`: time, direction, kind, method or `sessionUpdate` name, error code, byte size, and the latency between a request and its response. Filters by method and by error. Exposed as `GET /v1/sessions/{id}/frames` (phone role, owner rights only), reading a bounded ring the host keeps per session (default 2,000 entries, a few hundred KiB). Useful for "which call is slow or failing" with no content at all.
2. **Content view (explicit, per session, off by default).** The owner turns on "Capture frame content for this session" from the inspector. The host then keeps up to a bounded number of redacted frames in memory only (8 MiB, same ring discipline as the journal): known secret fields (`env` values, `Authorization`-like headers, token-shaped strings) are replaced before storage, and file contents and prompt text are shown as stored since the person asked to see them. Capture ends with the session or on a switch-off, is never written to disk and never goes to the log. Viewing it is an audit event ([adr-audit-and-trace](adr-audit-and-trace.md)).
3. **Groups and summaries**: consecutive `agent_message_chunk` or `tool_call_update` frames collapse into one row; expand shows the frames. Capabilities from `initialize` are shown in a card ("this agent supports load, images, HTTP MCP").
4. **Export**: a redacted JSONL export of the metadata view (or of captured frames, with a second confirmation) so a failing session can be attached to a bug report. The export screen lists what it contains.
5. **No injection.** The inspector observes; it does not send raw frames. Prompts, cancel and decisions use the normal controls.

## Options Considered

### Option A: metadata ring always, content capture on request (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A ring buffer tap in `connection.rs`, one route, a desktop panel. |
| Cost | A few hundred KiB per session; content capture bounded to 8 MiB. |
| Scalability | Bounded per session and by the existing session cap. |
| Team familiarity | Reuses the frame-summary code behind `frame_metadata`. |

**Pros:** debugging without log files; the default view holds no content; capture is a visible, time-limited choice.
**Cons:** redaction by pattern can miss a secret; memory cost while capturing.

### Option B: log raw frames to a file (AcpAgent Client style)

**Pros:** simplest, durable. **Cons:** writes prompts, replies and file contents to disk; the project declined raw frame logging.

### Option C: a separate standalone inspector tool

**Pros:** no host changes. **Cons:** cannot see a running session's traffic without proxying the stdio pipe; adds a second thing to run.

## Consequences

- `docs/security.md` documents the two levels; the metadata view is as sensitive as the existing log (names and sizes only).
- Tests: redaction of `env` and header-like fields; content ring bound; capture off by default after a restart; the frames route refuses non-owner rights ([adr-device-roles](adr-device-roles.md)).
- It doubles as a compatibility tool for [adr-registry-browser](adr-registry-browser.md): "does this agent send `plan` and `available_commands_update`?" becomes visible.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: metadata ring, route, tests.
3. [ ] Desktop: inspector panel (a Developer section in Settings or the session menu).
4. [ ] Decide whether Android gets the metadata view only.
