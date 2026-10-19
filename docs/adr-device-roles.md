# ADR: per-device rights (observer, approver, operator) and expiring access

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Coder, the self-hosted platform, lists SSO, roles, groups, audit logging, per-user spend limits and centralised model approval among its governance features (some behind a paid tier, according to its pricing page). Cloud sandboxes in the Copilot app "enforce org policy". The Claude Code docs describe permission rules that organisations can set across users. See [research notes](adr-ade-index.md#research-notes).

This host separates credentials by role: `Role::Phone` for `/v1`, `Role::Peer` for `/acp`, and [adr-external-mcp](adr-external-mcp.md) proposes `Role::Agent`. Within `Role::Phone` every paired device can do everything a phone can do: prompt, approve writes and terminals, merge tasks, change git, and revoke other devices (per that ADR's own context text). Tokens already expire (`token_lifetime_seconds`) and `acpd revoke` exists. The host is single-user by design; this ADR does not add accounts or SSO.

The real situations: showing a colleague what an agent is doing without letting them approve a write; a spare tablet that should only watch; a device that should approve but not merge; lending a phone for a day.

## Decision

Keep one owner and add **rights per paired device**, chosen when pairing and changeable by the owner.

1. Rights levels (a closed set, each a superset of the previous):
   - `observer`: read sessions, tasks, diffs, logs and the audit trail; no prompts, no decisions.
   - `approver`: observer plus approve or deny pending consents and cancel a running turn. Cannot send a new prompt, change settings, merge, push or revoke.
   - `operator`: today's phone rights, minus managing devices.
   - `owner`: today's phone rights including pair and revoke. The first paired device and any device made owner by an owner.
2. `acpd pair --rights observer --expires 24h` and the New device screen choose rights and an optional hard expiry. Existing paired devices load as `owner` so nothing changes on upgrade, and the security file format stays compatible (an absent field means `owner`).
3. Enforcement is in the host's route middleware, beside `phones_only`: each route declares the minimum rights, the middleware compares, and a refusal returns `insufficient_rights` with the required level. Clients read the device's rights at connect and hide controls that would be refused, but hiding is a convenience; the host decides.
4. The Devices screen (owner only) lists name, rights, created, last seen and expiry, with Change rights and Revoke. Changing rights takes effect on the next request, including on an open WebSocket.
5. Control handoff (`takeover`) respects rights: an observer cannot take control of a session.
6. Every decision a non-owner makes appears in the audit trail with the device name ([adr-audit-and-trace](adr-audit-and-trace.md)).

## Options Considered

### Option A: four fixed levels per device (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A field, a route table, middleware, a screen. |
| Cost | None. |
| Scalability | A handful of devices. |
| Team familiarity | Extends `Role` and the pairing store. |

**Pros:** answers the real cases; small enough to test exhaustively; backwards compatible.
**Cons:** fixed levels will not suit every case; no per-workspace rights.

### Option B: free-form per-route permissions

**Pros:** maximum control. **Cons:** a configuration surface nobody will review; easy to get wrong.

### Option C: accounts, groups and SSO

**Pros:** what teams expect. **Cons:** a different product (a multi-user server), and a large security surface; reconsider only if the project targets teams.

## Consequences

- `docs/security.md` gets a rights table; a test enumerates every route and asserts its minimum level (a new route without a level fails the test).
- Observers still see prompts and file contents in sessions; this is *read access to everything the owner sees*, and the pairing screen says so.
- Tests: an observer's prompt, approve and revoke calls are refused over REST and WebSocket; an approver cannot merge; expiry refuses a token without revoking the device record; a downgrade applies to an open socket.

## Action Items

1. [ ] User reviews this ADR, especially the rights table.
2. [ ] Host: `rights` field with default `owner`, route table, middleware, CLI flags.
3. [ ] Desktop and Android: pairing option and Devices screen; hide refused controls.
4. [ ] Coordinate numbering with `Role::Agent` in [adr-external-mcp](adr-external-mcp.md) so roles and rights stay separate (role = which API, rights = what within it).
