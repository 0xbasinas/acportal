# ADR: a host MCP gateway in front of the agent's tool servers

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

A 2026 gateway guide notes that a model gateway sees model calls only: tool calls, such as an agent's MCP calls, run locally and need separate governance. MCP gateways sit between the agent and its tools and check every call, with outcomes such as allow, deny, require approval or require justification, can inspect arguments, and keep an audit record with the matched policy, inputs, approver and outcome. Symphony keeps its Linear token away from the agent by exposing a narrow dynamic tool call instead of the token. Anthropic's secure-deployment guide recommends custom tools that route to a service outside the agent's boundary so the agent never sees credentials, and warns that read-only access to a code directory can still expose `.env` and key files. The ACP *MCP over ACP* draft lets a client provide MCP tools over the existing connection. See [research notes](adr-ade-index.md#research-notes).

Here, MCP servers are configured on the host and handed to agents at session start (`mcp.rs` validates definitions against agent capabilities; definitions and secrets are stored encrypted on Android, with host-side masked secrets planned). After that the agent talks to each MCP server itself. The host cannot see, approve, limit or log those calls, and each server's secrets (headers, tokens) are given to the agent's process.

## Decision

Offer an optional **gateway**: instead of N servers, the agent is given one MCP endpoint served by the host, which forwards to the real servers under policy.

1. **Shape**: for a session with `gateway = true` for an agent, the host presents a single MCP server named for the session, preferably via MCP over ACP when the agent advertises it (no network endpoint), otherwise as a loopback HTTP server with a per-session token or a stdio shim the host launches. The agent sees the union of tools from the configured upstream servers, namespaced by server (`github.create_issue`).
2. **Policy per tool call** ([adr-policy-engine](adr-policy-engine.md)): request kind `mcp.tool` with attributes server, tool, arguments (bounded), agent, workspace, profile. Outcomes allow, deny, ask, ask with justification. Tool lists can be filtered so denied tools are simply not offered, reducing prompt-injection surface and token use.
3. **Credentials stay in the host**: upstream headers, tokens and OAuth state live in the host's protected store and are applied by the gateway when it calls the server. The agent never receives them. This also fixes the desktop and Android secret-storage mismatch for MCP definitions, because the host becomes the single secret holder (the roadmap's "shared settings" item).
4. **Audit**: each call is recorded with server, tool, an argument digest, outcome, deciding rule and latency, never arguments or results. Results are streamed through unchanged within a size bound (default 1 MiB), with truncation shown.
5. **Rate and size limits**: calls per minute per tool, argument and result size caps, a timeout per call, and a concurrency cap per upstream.
6. **Untrusted results**: tool results can carry injected instructions. The gateway marks results as tool output (the agent's own framing), and a policy can require approval for the next write after a result from a chosen server (a "taint" rule), described as a policy predicate, not as protection against injection.
7. **Transparency**: the session shows "Tools via host gateway" with a tool list and which are blocked, so a person can see what the agent can call.

## Options Considered

### Option A: a host-served gateway under the shared policy (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | High. An MCP server and client in the host, namespacing, streaming, auth, policy hooks. |
| Cost | Added latency per tool call. |
| Scalability | Per session; bounded concurrency. |
| Team familiarity | `mcp.rs` knows definitions; the server side is new. |

**Pros:** the only way to govern and log tool calls and to keep MCP secrets away from agents; one place for limits; reads well beside the credential proxy.
**Cons:** the gateway becomes a critical path and a large new surface; agents that rely on MCP features beyond tools (resources, prompts, sampling, elicitation) need pass-through rules; MCP over ACP is a draft.

### Option B: keep handing servers to agents directly

**Pros:** nothing to build; works with every agent today. **Cons:** no governance of tool calls, secrets exposed to the agent.

### Option C: deny all MCP and allow only built-in agent tools

**Pros:** smallest surface. **Cons:** removes a major capability.

## Consequences

- `docs/security.md`: the gateway is a trusted component; a bug there could expose credentials or let denied calls through, so it needs its own tests and review. It must bind loopback only, check Origin, and never rely on a random port for authentication (as the MCP over ACP draft says for its HTTP adapter).
- Tests: a denied tool is absent from the offered list and refused if called; a credential never appears in anything sent to the agent; size and rate limits; an upstream timeout leaves the session usable.
- Start with read-only MCP servers and a stdio shim to learn the failure modes before widening.

## Action Items

1. [ ] User reviews this ADR; accept [adr-policy-engine](adr-policy-engine.md) and [adr-acp-capabilities-roadmap](adr-acp-capabilities-roadmap.md) first.
2. [ ] Spike: a stdio shim in front of one HTTP MCP server with header injection.
3. [ ] Decide transport (MCP over ACP versus loopback) after checking real agents.
4. [ ] Tool filtering and audit; then call-time policy.
