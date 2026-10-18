# ADR: research spike on MCP Apps (interactive tool UIs) in the clients

**Status:** Proposed (spike only; no implementation proposed)
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

MCP Apps (reported as launched in January 2026) let a tool return a sandboxed interactive UI that the client renders. A 2026-07-28 MCP specification release candidate is described as the biggest revision since launch, adding stateless servers, MCP Apps and a Tasks extension. Client and server negotiate which extensions both support at connect time, so a client must check capabilities rather than assume support. A2A (Agent2Agent) has an Agent Card at a well-known URL and task states, and `docs/orchestrator-design.md` already lists A2A interoperability as a non-goal for the first release. Goose, MCP and AGENTS.md now sit under the Linux Foundation's Agentic AI Foundation. The sources I found are secondary blogs; none is the specification. See [research notes](adr-ade-index.md#research-notes).

This project already renders agent-driven UI in one place: form elicitation (`ElicitationScreens.kt` on Android; a form sheet on desktop), which shows an agent's request for structured input as a native form, with exact schema handling. MCP Apps would let an MCP *tool* return richer UI (a chart, a picker, a diff widget) inside a session. It is also the largest new attack surface in this list: remote-authored UI running inside the client.

## Decision

Do not build MCP Apps support now. Do a bounded spike and answer these questions in writing, then decide.

1. **Does ACP carry it?** MCP servers reach agents through the agent's MCP client; the host configures them but does not sit in the tool-call path for tools the agent calls directly. Check whether an MCP App resource would ever reach the host/clients through ACP `session/update` content, or whether it stays inside the agent. If it cannot reach this client, stop; the feature is out of scope.
2. **Which spec?** Read the actual MCP specification and the MCP Apps extension text, not summaries. Record the version, the capability names and the sandbox requirements.
3. **What would the sandbox be?** Desktop: a sandboxed `iframe` or `webview` with no node integration, a strict CSP, no network and no access to the preload API. Android: a `WebView` with JavaScript on only for the app content, file access off, no `addJavascriptInterface` beyond a minimal message channel, loaded from a data or `loadData` origin. State what the app can ask the host to do (probably only "send this text as the next prompt" after a visible confirmation).
4. **Consent and provenance.** An App comes from whatever MCP server the operator configured. The UI must show the server name, label it as agent-supplied, and never let it appear to be the host's own sheet (no spoofing of approval dialogs).
5. **Decision criteria for building:** a real configured MCP server that returns an App and benefits a phone user; a design that cannot touch tokens or the host API; and an accessibility story (TalkBack, font scale) for content the project does not author.
6. **A2A** stays on the watch list; revisit when the orchestrator design's peer bridge needs interoperability with a non-ACP peer.

## Options Considered

### Option A: a written spike before any code (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low (reading and one prototype in a throwaway branch). |
| Cost | A day or two. |
| Scalability | n/a |
| Team familiarity | n/a |

**Pros:** avoids building a large untrusted-UI feature on secondary sources; answers whether it even reaches this client.
**Cons:** delays any benefit; competitors may ship first.

### Option B: render MCP Apps now in a WebView

**Pros:** feature parity with the leading clients. **Cons:** unknown protocol path, security surface and accessibility cost.

### Option C: ignore it

**Pros:** nothing to maintain. **Cons:** the extension may become common; a recorded decision is better than drift.

## Consequences

- The spike outcome is added to this ADR as a dated section, and the status becomes Accepted (build), Rejected, or Deferred with reasons.
- Form elicitation stays the supported structured-input path in the meantime.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Read the MCP specification and the Apps extension; answer questions 1 and 2 in this file.
3. [ ] Only if question 1 is yes: sandbox prototype on the mock host, one screen per client.
