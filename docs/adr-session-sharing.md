# ADR: share one session with a guest

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Happy (an open-source mobile and web client for Claude Code and Codex) advertises that you can invite a colleague or friend into a session, where both can watch the agent and steer it. Its README says content between devices is end-to-end encrypted through a relay server it also publishes (MIT). Nimbalyst and Superset target solo use; Coder targets teams with roles and audit. See [research notes](adr-ade-index.md#research-notes).

[adr-device-roles](adr-device-roles.md) proposes per-device rights for devices the owner pairs. That fits a spare tablet. It is the wrong shape for "let a colleague watch this one session for an hour": a full pairing gives access to every session and workspace on the host, and the 60-second one-use code was designed for the owner's own devices.

## Decision

Add **guest links**: access to exactly one session, at `observer` or `approver` level, for a limited time. This builds on the rights levels in [adr-device-roles](adr-device-roles.md) and the remote-route work in [adr-remote-routes](adr-remote-routes.md); it adds no relay.

1. The owner picks **Share session**, a level (`observer` default, `suggester`, or `approver` after an extra confirmation), and an expiry (15 minutes to 24 hours, default 1 hour). The host creates a guest credential scoped to that session id and shows a one-use link or QR code. The code lasts 60 seconds to redeem, as pairing codes do today.
2. The guest credential is a device token with `scope = session:<id>` and the chosen rights. The route middleware refuses any route not tied to that session, including the session list, other sessions, hosts, settings, git, terminals, preview and the audit trail of other sessions. A guest cannot see workspace paths outside the session's own.
3. **Reach**: the guest connects to the host address the link names. The host operator must already expose that address (a tailnet, a published HTTPS route, per the remote-routes ADR). This ADR does not make a host reachable.
4. The guest's web client is the host-served page in [adr-host-web-client](adr-host-web-client.md), so no app install is needed.
5. The owner sees who is connected (guest label and level) in the session header and can **remove the guest** at once. Ending the session, the expiry or revocation closes open sockets.
6. Every guest action that changes anything (approver decisions, control requests) is written to the audit trail with the guest label ([adr-audit-and-trace](adr-audit-and-trace.md)). A guest never gains control handoff; at `approver` they can approve or deny and cancel, and cannot prompt.
7. **Suggester level** (Bellows offers watch, suggest and type roles): a suggester can submit text suggestions. Each lands in the session as a *proposed prompt* attributed to the guest, queued but never sent. Only the owner can accept (it is then sent once like any prompt), edit or discard it. This lets a colleague help without being able to drive the agent. Bellows also lets a session be handed over as a single file for a teammate to continue with their own agent; see the handoff export in [adr-context-handoff](adr-context-handoff.md).
8. Content: a guest sees everything in the session, including file contents and command output. The share screen says this in plain words and repeats it on the guest's page.

## Options Considered

### Option A: single-session guest credentials on the existing token model (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A scope field, middleware, a share screen, expiry handling. |
| Cost | None. |
| Scalability | A few guests per session (default limit 3). |
| Team familiarity | Same pairing store and revoke path. |

**Pros:** least privilege for the common case; no new infrastructure; every check is on the host.
**Cons:** needs a reachable address; the guest sees the whole session.

### Option B: full pairing for guests

**Pros:** already works. **Cons:** exposes every session; relies on the owner remembering to revoke.

### Option C: a hosted relay with end-to-end encryption (Happy's design)

**Pros:** works without an open address. **Cons:** someone must operate a relay and the security notes would have to say what it can read; deferred as in the remote-routes ADR.

## Consequences

- `docs/security.md`: guest scope, the content warning and the fact that a guest token is bearer-authenticated like any other.
- Tests: a guest token cannot read another session or list sessions over REST or WebSocket; expiry closes a socket; an observer cannot send a prompt; removing the guest takes effect on an open connection.

## Action Items

1. [ ] User reviews this ADR; accept [adr-device-roles](adr-device-roles.md) first.
2. [ ] Host: scoped credential, middleware, share and remove endpoints.
3. [ ] Desktop and Android: share screen, connected-guests list.
4. [ ] Depends on [adr-host-web-client](adr-host-web-client.md) for the guest's side.
