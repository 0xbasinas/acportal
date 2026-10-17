# ADR: how a saved host is reached away from one address

**Status:** Proposed
**Date:** 17 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

A saved desktop host and a saved Android host each store one address. The next request unseals the device token and sends it there. `GET /v1/status` cannot be the check that happens first. It sits on the phone router, behind `authenticate` and `phones_only`, and the JSON includes `hostId`, the computer name, version, feature list, whether `gh` was found, and `activeSessions`. Android `PortalRepository.details` compares `status.hostId` with the saved id only after `HostApi.status` has already called `vault.read`. Desktop `hosts:api` does the same with `unseal` and `request(h.address, ...)`.

`desktop/electron/host.ts` already rejects an HTTP 3xx response and keeps `rejectUnauthorized: true`. Android `HostApi.client()` sets `followRedirects(false)` and `followSslRedirects(false)`. `validateAddress` on both clients allows `http` only for loopback. A listener that is not loopback still requires TLS in the host.

`SecurityStore::host_id` is a UUID created once in the security file. A successful `POST /v1/pair` returns it, and an authenticated status call returns it. Anyone who can open the address today does not receive it until they redeem a pairing code or present a bearer token.

T3 keeps identity separate from the route. A saved environment holds an ordered list. The driver tries the first route that works. Each direct route is checked with a public descriptor before a credential is sent, so a LAN address that a different machine answers receives none. A paired token learns a plain HTTP address off the tailnet only when that token was itself paired over plain HTTP. Routes the user saved are left in place. Tailscale is an endpoint for ordinary pairing, and authentication stays with the environment. Read from `docs/internals/remote.md` with `git show` on 17 October 2026. That file was not executed, and its source is not copied here.

## Decision

Do (a) and (b). Leave (c) for a later review.

(a) is a recipe the operator runs, plus one doctor check. This repository does not install Tailscale, `cloudflared`, or a tunnel. `acpd` stays bound to `127.0.0.1`. Loopback cleartext is already allowed. If the operator binds any other address, that listener still needs the host's own TLS.

Tailscale Serve, read 17 October 2026 from the feature page last validated 20 January 2026 and the CLI page last validated 26 January 2026. Serve routes tailnet traffic to a local service and terminates HTTPS with the tailnet certificate. HTTPS certificates must be enabled. Access-control rules apply to Serve as they do to any other tailnet service. The CLI reverse-proxy target that this recipe uses is `http://127.0.0.1` only. The command shape is `tailscale serve --https=<port> http://127.0.0.1:<acpd-port>`, or `tailscale serve <acpd-port>`, which prints a tailnet URL proxying to `http://127.0.0.1:<acpd-port>`. The phone and the desktop then use `https://<machine>.<tailnet>.ts.net`. The host process sees localhost. Do not pass `https+insecure`. That pseudo-protocol skips certificate checks on a local HTTPS server. Do not run `tailscale funnel` for this port. Funnel publishes the same port to the public internet, and the Serve page says the most recent of `serve` and `funnel` on a port wins. The consent page enables Funnel by default. Turn Funnel off when this port should stay inside the tailnet. Serve adds `Tailscale-User-Login`, `Tailscale-User-Name`, and `Tailscale-User-Profile-Pic` on tailnet requests. Those headers are not a device token. The host keeps requiring the bearer, and it must not authorize a request from those headers.

Cloudflare Tunnel, read 17 October 2026. A published application maps a public hostname to a local service. The published-applications page, last updated 15 October 2026, says anyone who can open that hostname reaches the origin. The routing page, last updated 16 October 2026, shows the dashboard creating a CNAME to `<UUID>.cfargotunnel.com`. The get-started page, last updated 30 September 2026, requires a catch-all ingress rule, `http_status:404`, at the end of a config file. The recipe is a public HTTPS hostname, service URL `http://127.0.0.1:<acpd-port>`, and Cloudflare Access in front. Access applications deny by default. A user must match an Allow policy. Enable Protect with Access so `cloudflared` checks the Access token. The host bearer is still required after that. Leave origin certificate verification on. A hostname without Access is a wider exposure than Serve, and this recipe does not use it.

Doctor grows `acpd doctor --url https://...`. It connects with the process default TLS verifier, does not send `Authorization`, and does not follow redirects. It `GET`s the new `/v1/host` below and compares `hostId` to the local `SecurityStore::host_id`. A mismatch, a certificate error, an HTTP error, or any extra JSON field fails the check. A non-loopback `http` URL is refused. The check prints pass or fail. It does not print a token or a TLS key path.

(b) stores several addresses on one saved host, in the order the operator set. Before any token is unsealed or read from the vault, the client `GET`s `/v1/host` on that address with no `Authorization`. A matching `hostId` selects the address, and only then is the token sent. A different `hostId` skips that address and never receives the token. A connection failure, including a certificate failure, skips to the next address. If one or more addresses answered as a different host and none matched, the client shows repair. If every address was silent, the client shows offline. In both cases the token stays local.

The new route is `GET /v1/host`, mounted beside `POST /v1/pair`, outside `authenticate` and `phones_only`, and still under `no_browser_and_no_cache` and the 16 KiB body limit. The body is exactly `{"hostId":"<uuid>"}` with unknown fields rejected. It does not include the computer name, features, session counts, or any token. A browser `Origin` still receives 403.

The client does not learn addresses from the host. An address saved as `https` is not rewritten to `http`. T3 will learn a plain HTTP LAN address only for a token that was paired over plain HTTP. Copying the learning half without that rule is how a token paired over HTTPS would start travelling in cleartext. Learning, and the T3 habit of preflighting an earlier route and moving a live session onto it, stay out of this cut. Each connection attempt walks the list once and keeps the first match. It does not bounce a socket that is already up.

Pairing still posts the code to the single address the operator typed. That origin becomes the first saved address. Further addresses are added on the saved host afterwards. Repair starts from the saved list. The code is posted only to the address the operator confirms.

(c) a relay this project operates, is deferred. It repeats what Serve and Tunnel already forward, and the relay operator would see the requests unless the bytes were end-to-end encrypted. That is a new party in `docs/security.md`. It is not part of this branch.

## Options Considered

### Option A: Serve or Tunnel in front of loopback, checked by doctor

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low for this repository. The operator installs the proxy. The host adds one unauthenticated GET and one doctor flag. |
| Cost | Tailscale and Cloudflare accounts stay outside this repo. No process is added to `acpd`. |
| Scalability | One machine, the tailnet or the Access policy the operator already has. |
| Team familiarity | Same loopback bind and the same TLS rule for every non-loopback listener. |

**Pros:** The device token still ends at this host. Serve stays inside the tailnet when Funnel is left off. Access is deny-by-default in front of a public hostname. Doctor fails when the public name reaches a different `host_id` or a bad certificate.
**Cons:** The operator has to install Tailscale or `cloudflared` themselves. Official Serve and Cloudflare HTTP pages describe an HTTP reverse proxy and do not say that a WebSocket upgrade is forwarded. The phone and the desktop open a WebSocket on the same origin after REST. That upgrade has to be confirmed on a real proxy before the recipe is called done. Identity headers from Serve must be ignored.

### Option B: an ordered address list, probed before the token

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. One route, a desktop walk in the existing request path, an Android migration 3 to 4. |
| Cost | No new service. |
| Scalability | The list the operator types. A handful of addresses, tried in order. |
| Team familiarity | Same `host_id`, same redirect refusal, same vault. |

**Pros:** A stale LAN address that a different `acpd` answers does not receive this device token. Old saved rows with a single `address` still load. Pairing stays one address and one code.
**Cons:** Anyone who can open the address learns the host UUID, which today is returned only after a pairing code or with a bearer. The UUID authorizes nothing. All-silent still means offline, so a host that is down is not treated as a wrong machine. Learned routes are absent, so an address the operator forgot to type is not tried.

### Option C: a hosted relay operated here

| Dimension | Assessment |
|-----------|------------|
| Complexity | High. A public service, accounts, and a trust write-up. |
| Cost | A machine this project would run, and the traffic it would see. |
| Scalability | Many hosts behind one name. |
| Team familiarity | None. There is no relay in this repository. |

**Pros:** One hostname for phones that are not on a tailnet and do not have a Cloudflare account.
**Cons:** Serve and Tunnel already provide that hostname. A relay we operate sees requests unless the tunnel is end-to-end. Device tokens would have to be kept off that machine on purpose. `docs/security.md` would gain a new operator.

## Trade-off Analysis

Option C duplicates A and adds a party who can read traffic. It waits.

A alone leaves the client with one saved string. When that string is stale, or when the operator wants both a tailnet name and a LAN address, the token still goes to whichever string is stored. B is the check that stops the token. B alone does not tell the operator how to put TLS in front of a loopback host. The two fit: the public name from A is one entry in the list from B, and doctor checks that name against this machine's `host_id` without a token.

The probe is a new route because status is authenticated and too talkative. Putting the UUID on an unauthenticated GET is the cost of checking before the bearer is sent. The route stays next to pair, keeps the Origin ban, and returns one field.

Skipping a silent address, and refusing to send the token when every address is silent, is stricter than T3. T3 still tries a route that never answered. Here a silent address is offline. A different `hostId` is the only positive proof that the token must not be sent. Both outcomes keep the token local.

## What docs/security.md would say

`docs/security.md` is unchanged until this ADR is accepted. The implementation change would add the following, and would not weaken the existing sentences.

For A. Tailscale Serve, with Funnel off, and a Cloudflare Tunnel whose hostname has Access in front, are reverse proxies in front of the loopback listener. Funnel, and a Cloudflare hostname without Access, publish that listener more widely than the recipe. The host's own bearer and the TLS rule for a non-loopback bind stay. Doctor checks a URL with the default verifier and sends no device token.

For B. Clients send no bearer until `GET /v1/host` on that exact address returns the saved host id. The route is unauthenticated, rejects a browser `Origin`, and returns only `hostId`. The id is the same UUID a successful pair and an authenticated status call already return. It is not a credential. Clients do not follow redirects before or after the probe. Clients do not store an address the host advertises, and they do not replace an `https` address with `http`.

For C. A relay operated by this project is not introduced. If a later ADR adds one, security.md has to name what that operator can see and that device tokens are not stored there.

## Exact client changes

These are the edits for a later implementation. They are not done in this change.

Desktop, `desktop/src/types.ts`. `Host.address` becomes `addresses: string[]`, ordered. `desktop/electron/main.ts` `StoredHost` matches. `loadHosts` accepts an old row that still has `address` and wraps it as a one-element list. `publicHost` continues to omit `secret`.

Desktop, `desktop/electron/main.ts`. The pair handler still posts `/v1/pair` to the one validated origin and stores that origin as `addresses[0]`. The replace match on line 132 today is `replaceId`, or `hostId` plus the same `address`. After the change, one saved host per `hostId` is enough: `replaceId` or `hostId`. Adding an address updates that row. It does not create a second host. The walk is one helper used by `hosts:api`, the watcher credential callback, `session:attach`, `hosts:forget` when it revokes, and the git registration in the same file. The helper probes `/v1/host` with no token, and calls `unseal` only after a match. `SessionSocket` and `registerGit` then receive the matched address.

Desktop, `desktop/electron/host.ts`. The probe uses `request` with no token. Redirects stay rejected. `rejectUnauthorized` stays true. No flag disables certificate checks.

Desktop, `desktop/src/Hosts.tsx`. `addr` displays `h.address`. It should display the address last used, and the host row should offer the ordered list so the operator can add, remove, and reorder. The host does not rewrite that list.

Desktop, `desktop/src/Pairing.tsx`. The form keeps one address field, the placeholder `https://studio.tailnet.ts.net:8765`, and `hosts.pair(address, code, name, replaceId)`. Pasting `acpd://pair?address=…&code=…` still fills that one field. Extra addresses are not collected on this form. Repair opens with the first saved address filled in, and the operator can change it before the code is posted.

Companions that read `host.address` and have to follow the type. `desktop/src/App.tsx` passes `initialAddress` into `Pairing` and tests `host.address` for a loopback git fallback. `desktop/electron/git.ts` takes an address callback. `desktop/electron/watch.ts` opens sockets with the resolved address and token. `desktop/electron/preload.ts` keeps `pair` as one address.

Android. Room is version 3 in `PortalDatabase.kt`, with auto migrations 1 to 2 and 2 to 3, and `exportSchema = true`. Do not edit schema 3 in place. Add version 4 with an explicit migration. New table `host_addresses` with `hostId`, `position`, and `address`, foreign key to `hosts.id` and `ON DELETE CASCADE`. Copy `hosts.address` into position 0. Add `android/universal-acp/app/schemas/dev.acportal.storage.PortalDatabase/4.json` beside `3.json`. `room.schemaLocation` is already set in `app/build.gradle.kts`.

Keep `HostProfile.address` as the address that last matched, so the existing `execute(host.address, ...)` calls stay. The ordered list lives in the child table. `HostApi` gains `probe(address)` which `GET`s `/v1/host` with a null token. The resolver runs before `vault.read`. On a match it writes that address back to `HostProfile.address` and only then reads the credential. `PortalRepository.details` today calls `api.status`, which reads the vault, and then checks `status.hostId`. That check moves in front of the vault read. A mismatched probe does not call `status`. `pair` still takes one address and stores it at position 0.

## Consequences

- An operator can put a tailnet name or an Access-protected hostname in front of a loopback host, and doctor can tell whether that URL is this host.
- A saved second address is tried only after the first fails the probe or the connection. A different host id never receives the token.
- The host UUID becomes readable without a pairing code by anyone who can open the address. It still authorizes nothing.
- WebSocket upgrade through Serve and through a Cloudflare HTTP published application is not established by the pages read for this ADR.
- `docs/security.md` stays as it is until the implementation change, which adds the sentences in the section above.
- No relay is added.

## Action Items

1. [ ] User reviews this ADR and [adr-external-mcp.md](adr-external-mcp.md).
2. [ ] After acceptance, add `GET /v1/host` and `acpd doctor --url`, with tests that a redirect is not followed, a mismatched id fails, and no token is sent.
3. [ ] Desktop address list and the probe-before-unseal walk, including old rows that still have `address`.
4. [ ] Android migration 3 to 4 and the probe before `vault.read`.
5. [ ] Confirm a session WebSocket on a real Tailscale Serve URL and a real Cloudflare hostname before documenting the recipe as working.
6. [ ] Add the security.md sentences in the same implementation change. Do not describe Funnel or an open hostname as the recipe.

## Sources

Read 17 October 2026.

- Tailscale Serve, last validated 20 January 2026. HTTPS certificates required, access-control rules apply, Funnel is the public alternative, the latest of `serve` and `funnel` on a port wins. https://tailscale.com/docs/features/tailscale-serve.md
- `tailscale serve` CLI, last validated 26 January 2026. `--https` terminates TLS, proxy targets use `http://127.0.0.1`, `https+insecure` skips certificate checks. https://tailscale.com/docs/reference/tailscale-cli/serve
- Tailscale Funnel, last validated 20 January 2026. The public internet, not the tailnet. https://tailscale.com/docs/features/tailscale-funnel.md
- Cloudflare published applications, last updated 15 October 2026. A public hostname reaches the origin for anyone who can open it. https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/routing-to-tunnel/
- Cloudflare Tunnel routing, last updated 16 October 2026. Hostname to local service, CNAME to `<UUID>.cfargotunnel.com`. https://developers.cloudflare.com/tunnel/concepts/routing/
- Cloudflare Tunnel get started, last updated 30 September 2026. Ingress ends with `http_status:404`. Leave origin certificate verification on when the origin is HTTPS. https://developers.cloudflare.com/tunnel/get-started/
- Cloudflare Access for a self-hosted public app, last updated 17 April 2026. Deny by default until an Allow policy matches. Protect with Access so `cloudflared` checks the token. https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/self-hosted-public-app/
- T3 `docs/internals/remote.md`, via `git show`, not executed. https://github.com/pingdotgg/t3code/blob/main/docs/internals/remote.md
- This host: `acpd/src/api.rs` router and `status`, `acpd/src/security.rs` `host_id`, `desktop/electron/main.ts`, `desktop/electron/host.ts`, `desktop/src/Hosts.tsx`, `desktop/src/Pairing.tsx`, `android/.../HostApi.kt`, `android/.../PortalDatabase.kt` version 3, `android/.../PortalRepository.kt` `details`.
