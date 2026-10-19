# ADR: let agents verify their own UI work through a host browser tool

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Playwright MCP is the main way coding agents check UI work: the agent drives a browser, reads the accessibility tree, takes screenshots and reads the console, then iterates. Reports also say this self-check is ad hoc and point-in-time, so it should be paired with deterministic screenshot diffs. Cursor 3 has an integrated browser the agent can open and prompt against, and cloud agents attach demos and screenshots as evidence of their work. See [research notes](adr-ade-index.md#research-notes).

This project has the human side of this loop: the desktop browser (`Browser.tsx`, `electron/browser.ts`) with an element picker and Send to agent, and the phone preview (`PreviewScreen.kt`). The agent side is missing. The agent cannot open the dev server the host is already previewing, and a finished task carries no screenshot of what the change looks like. A person reviewing from a phone sees a diff, not the page.

## Decision

Offer an optional **browser MCP tool** served by the host to agents that support HTTP MCP, limited to the ports already listed for preview, and attach screenshots to turns as review evidence.

1. Preferred transport, when the agent advertises it: **MCP over ACP** (draft RFD), where the host declares the server with `"type": "acp"` and a `serverId` and the agent sends `mcp/message` requests over the ACP connection it already has, so no network endpoint or token exists at all; ownership of the `serverId` stays with the host, and request ids are routing identifiers, not credentials. Fallback transport: the host exposes an MCP server on a per-session loopback endpoint with a per-session token ([adr-external-mcp](adr-external-mcp.md) solves the token and role question for outside agents; this reuses that decision rather than inventing a second one). Tools: `browser_open(url)`, `browser_snapshot` (accessibility tree, bounded), `browser_screenshot` (viewport PNG, bounded), `browser_console` (last N messages), `browser_click`/`browser_type` (by accessibility reference, not by coordinates), and `browser_close`.
2. **Scope**: URLs must be `http://localhost:<port>` or `127.0.0.1` for a port in `[preview] ports`. Any other origin, `file:`, `data:` and `javascript:` URLs are refused. Cross-origin navigation away from a listed origin is blocked, so a page cannot steer the tool to the open web.
3. The browser is a separate, isolated profile with no cookies or sign-ins, and it is torn down with the session. It is headless Chromium launched by the host (the desktop app can show it live in its browser pane).
4. **Consent**: the first use in a session asks the phone ("Agent wants to open localhost:5173"), then is allowed for that origin for the session. Click and type actions are visible in the timeline as tool calls.
5. **Evidence**: when a turn ends after browser use, the last screenshot is stored with the turn as an attachment-style item the phone and desktop can open. It is stored only in the session journal, bounded, and dropped with the session.
6. Where an agent cannot use HTTP MCP, nothing changes; the host never injects tools into an agent that did not advertise the capability.

## Options Considered

### Option A: a scoped host browser MCP tool (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium to high. A browser driver, an MCP surface, limits, evidence storage. |
| Cost | Browser memory per session (bounded to one). |
| Scalability | One browser per active session, configurable. |
| Team familiarity | Electron browser code and the preview allowlist already exist. |

**Pros:** agents can check their own UI work without a separate Playwright setup; the phone gets screenshots.
**Cons:** adds an automation surface that reads page content (which may include secrets in a dev app); requires a browser binary on the host.

### Option B: tell users to configure Playwright MCP themselves

**Pros:** no code. **Cons:** unscoped (any URL), no consent, no evidence on the phone.

### Option C: screenshots taken by the host after each turn, with no agent control

**Pros:** simple; safe. **Cons:** the agent cannot use them to iterate. This is a good fallback and can ship first.

## Consequences

- `docs/security.md`: the tool can read anything a listed dev server serves, including data in a logged-in dev app; it is off by default and listed ports only.
- Page content is untrusted input to the agent (prompt injection through the page); the tool wraps snapshots in a labelled block.
- Tests: refusal of non-listed ports, `file:` and redirects off-origin; the screenshot size bound; teardown with the session.
- Start with Option C (evidence screenshots) and add control only if the first step is used.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: post-turn screenshot of a listed preview port, bounded, behind `[preview] screenshots`.
3. [ ] Decide the browser driver (system Chromium via CDP vs a bundled one) after checking installed size.
4. [ ] Only then: the MCP tool surface, after [adr-external-mcp](adr-external-mcp.md) is decided.
