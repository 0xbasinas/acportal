# ADR: an external MCP endpoint for outside agents

**Status:** Proposed
**Date:** 17 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Claude Code, Codex, and ChatGPT can call an MCP server over HTTP. T3 Code exposes `/mcp` for that. A token whose subject is `mcp-client` is accepted only there. The HTTP API and the WebSocket call `rejectMcpClientAudience` and refuse it, so the token cannot reach the rest of the RPC surface. Access is one of `read-only`, `approval-required`, `auto-accept-edits`, `auto`, or `full-access`. Read-only receives the orchestration read scope. The others also receive operate, and full-access adds settings and source-control writes. The session lasts 30 days. OAuth uses authorization-code plus PKCE `S256`. An `https` redirect must match the registered URI exactly. Loopback redirects follow RFC 8252 and may vary the port. Page errors are not turned into redirects. Read in T3 source on 17 October 2026, not executed: `packages/contracts/src/auth.ts`, `apps/server/src/auth/EnvironmentAuth.ts`, `apps/server/src/auth/McpOAuth.ts`.

This host already separates credentials by role. `Role::Phone` may use `/v1`. `Role::Peer` may use `/acp` and `/v1/peer/info`. `phones_only` and `peers_only` return `role_not_allowed` for the other role. Pairing codes are one-use and last 60 seconds. `acpd revoke` deletes a device from the same store. The router rejects a browser `Origin` and sets `Cache-Control: no-store`. The default body limit is 16 KiB. A listener that is not loopback requires TLS. `[orchestrator] peer_rights` is the ceiling for a peer's sessions, and `peer_agents` is the allowlist for `/acp/{agent_id}`.

An outside agent is neither a phone nor a peer. A phone token can merge tasks, change git, and revoke devices. A peer token can open an agent session on this host. Giving either token to Claude Code or ChatGPT would hand over that surface.

## Decision

Add `Role::Agent`, minted by the existing pairing-code path (`acpd pair` grows an agent role next to phone and peer). The redeemed credential is the same device-token shape the store already keeps: a digest, an expiry from `token_lifetime_seconds`, and revocation through `acpd revoke`. The first MCP surface is read-only.

Mount `/mcp` on its own router. `authenticate` still checks the bearer digest. A new `agents_only` middleware allows `Role::Agent` and rejects Phone and Peer. `phones_only` and `peers_only` stay as they are, so an agent token receives `role_not_allowed` on `/v1` and on `/acp`. The MCP route keeps the Origin rejection, `no-store`, and the 16 KiB body limit. TLS rules do not change.

The first tools are `sessions_list`, `session_read`, and `tasks_list`.

- `sessions_list` and `session_read` read the session catalog and, for a live session, the journal the host already holds. They do not call `session/load`, do not spawn an agent, and do not send a prompt.
- `tasks_list` reads task records. When `[orchestrator] enabled` is false, the tool says so and returns an empty list. It does not start a task.
- Tool results are bounded. The token, the pairing code, and environment values are not logged.

No mutating tool ships in the first cut. A later review can add a start or a prompt. That grant would store a `WorkspaceAccess` ceiling on the device. The session's access would be the intersection of that ceiling and the access a phone is already allowed to request. `form_elicitation` stays false, because an MCP client has no phone sheet and URL elicitation is already refused. Orchestrator children started from that session would still have to sit inside `[orchestrator]` limits, and they would not receive `peer_rights` or `peer_agents`. Those fields stay on `Role::Peer`.

Do not build an OAuth authorization server on this branch.

## Options Considered

### Option A: a pairing-code agent token and a read-only `/mcp`

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low. One role, one middleware, three read tools. |
| Cost | No new service. The operator runs `acpd pair` and pastes the bearer token into the MCP client. |
| Scalability | One host, the clients the operator configures. |
| Team familiarity | Same pairing store, same revoke command, same role gate as phones and peers. |

**Pros:** The agent token cannot reach `/v1` or `/acp`. Revocation already exists. There is no redirect URI to register, so a hosted agent's https redirect cannot receive the token. The 16 KiB limit and the Origin ban stay.
**Cons:** Clients that can only sign in through OAuth (the ChatGPT connector flow, and remote Claude Code when it insists on a discovery document) cannot connect until a later ADR. The operator pastes a bearer token.

### Option B: an OAuth server with an `mcp-client` audience, as T3 does

| Dimension | Assessment |
|-----------|------------|
| Complexity | High. Registration, exact redirect matching, PKCE, approval page, token exchange. |
| Cost | A new public HTTP surface on a host that currently refuses browser origins. |
| Scalability | Fits hosted agents. |
| Team familiarity | New. The host has no authorization-server code. |

**Pros:** Claude Code and ChatGPT can use their normal MCP login. The audience split is the same idea as `Role::Agent`.
**Cons:** T3 matches `https` redirects exactly because a loose match sends the authorization code to the wrong site. An approval page is a browser flow, and this host returns 403 for any `Origin`. Building that correctly is a larger trust boundary than the read-only tools.

### Option C: reuse a phone or peer token on `/mcp`

| Dimension | Assessment |
|-----------|------------|
| Complexity | Lowest. |
| Cost | No new role. |
| Scalability | Same as today. |
| Team familiarity | Highest, and wrong. |

**Pros:** Nothing new to pair.
**Cons:** A phone token on `/mcp` is also a phone token on `/v1` unless every management route grows a second check. A peer token can already open `/acp/{agent_id}` for `peer_agents`. One leaked client config would manage the host or start an agent. T3's comment on the audience split exists to prevent that.

## Trade-off Analysis

Option C is smaller and fails the invariant that a credential may do only what its role allows. Option B is what hosted agents want, and it adds the redirect problem T3 had to close. Option A gives the outside agent a credential that both gates reject, uses `acpd revoke` unchanged, and leaves OAuth for a review that can decide redirect rules on purpose.

The read-only tool list is the whole of the first cut. `session_read` must not be implemented as `session/load`. Load starts an agent. The catalog and the live journal are enough to answer "what is in this session".

`[orchestrator] enabled` and `peer_rights` stay independent. An agent token does not become a peer when the orchestrator is on, and turning the orchestrator off does not make `tasks_list` invent tasks.

## Consequences

- A paired agent can list sessions and tasks without a phone in the loop.
- Phone management and the peer bridge stay unreachable with that token.
- Hosted MCP clients that require OAuth stay unsupported until a follow-up ADR.
- `docs/security.md` would need a short role section when this is implemented: agent tokens are for `/mcp` only, pairing is still one-use and 60 seconds, revoke still works, and the host still does not accept a browser origin or an OAuth redirect.
- The global 16 KiB body limit stays. A mutating tool that needs a larger body is a separate decision.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] After acceptance, add `Role::Agent` and `acpd pair`'s agent role. Old security files still load as phones.
3. [ ] Add `/mcp` with `agents_only`. Tests: an agent token receives `role_not_allowed` on `/v1` and `/acp`, and a phone or peer token receives it on `/mcp`.
4. [ ] Implement `sessions_list`, `session_read`, and `tasks_list` without starting a process. A test shows `session_read` does not spawn.
5. [ ] Document the role in `docs/security.md` in that same change.

## Sources

Read 17 October 2026.

- T3 `packages/contracts/src/auth.ts` (`AuthMcpClientAccess`), via `git show`, not executed. https://github.com/pingdotgg/t3code/blob/main/packages/contracts/src/auth.ts
- T3 `apps/server/src/auth/EnvironmentAuth.ts` (`MCP_CLIENT_SUBJECT`, `rejectMcpClientAudience`, `authenticateMcpClient`), via `git show`. https://github.com/pingdotgg/t3code/blob/main/apps/server/src/auth/EnvironmentAuth.ts
- T3 `apps/server/src/auth/McpOAuth.ts` (exact `https` redirect match, loopback port exception, page errors must never redirect), via `git show`. https://github.com/pingdotgg/t3code/blob/main/apps/server/src/auth/McpOAuth.ts
- This host: `acpd/src/security.rs` `Role`, `acpd/src/api.rs` `phones_only` and `peers_only`, `acpd/src/config.rs` `peer_rights` and `peer_agents`.
