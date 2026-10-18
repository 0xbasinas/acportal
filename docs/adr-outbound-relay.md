# ADR: an optional self-hostable relay with end-to-end encryption

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

[adr-remote-routes](adr-remote-routes.md) chooses Tailscale Serve or a Cloudflare tunnel in front of a loopback host (option A) and an ordered address list (option B), and declines a relay operated by this project (option C) because it would add a party who can read traffic. Its own context notes the gap: a phone not on a tailnet and with no Cloudflare account has no route.

The research for the ACP client list shows the other approach is the common one among phone clients. AgentDeck (Android) has a daemon (`agentdeckd`) that dials *outbound* to a relay over WebSocket so no open ports or VPN are needed; the relay forwards only ciphertext, the pairing ID in the QR code derives the encryption keys, and the relay can be self-hosted. Mobvibe has a gateway relay with end-to-end encryption ("routes events but cannot read your content"). Reemoat's machines dial out to a relay that "forwards bytes it holds no key for", with the control plane and relay self-hostable. Happy publishes an MIT encrypted-sync server. Bellows has a hosted relay that forwards sealed frames. Nimbalyst syncs with AES-256-GCM and keys exchanged by QR code. See [research notes](adr-ade-index.md#research-notes).

This project's pairing today is a one-use, 60-second code that yields a bearer token for a direct HTTPS connection. TLS terminates at the host.

## Decision

Specify, but do not build yet, an **outbound relay mode** whose security does not depend on trusting the relay. Build only if Serve and tunnels prove insufficient in real use.

1. **Roles.** The host dials out to a relay over WebSocket and registers a random host id. Phones and desktops connect to the same relay and ask for that host id. The relay matches the two and forwards opaque frames. The relay holds no secret that opens frames.
2. **Keys.** During pairing the host shows a QR code containing the relay URL (if not the default), the host id and a **pairing secret** with at least 128 bits of entropy. Both sides run a standard authenticated key exchange seeded by that secret (for example a Noise `XX`-style handshake, final choice after review), producing per-device session keys. The relay never sees the secret or the keys. A device that lacks the pairing secret cannot produce a valid handshake, so a malicious relay cannot add itself as a peer.
3. **Frames.** The existing REST and WebSocket calls ride inside the encrypted channel as request and response frames, unchanged in meaning, so the host's authorisation, rights and audit logic are untouched. Frame sizes and counts stay bounded by existing limits.
4. **What the relay still learns:** that a host and a device are connected, when, how much data flows, and from which network addresses. This metadata goes into `docs/security.md`. It never learns prompts, replies, files, tokens or workspace names.
5. **Operator model.** The project ships the relay as a small separate binary or container the operator can run on a VPS. There is no project-operated relay in this ADR. Pointing the host at someone else's relay is a configuration choice that the doctor output states plainly.
6. **Push.** Notifications for a closed phone app go through a platform push service carrying only an opaque host and session id, as AgentDeck's Firebase forwarder does, and are optional. The payload carries no titles or content.
7. **Revocation.** Revoking a device deletes its session key on the host; the relay can no longer complete a handshake for it. The relay can drop a host id on request.

## Options Considered

### Option A: self-hosted E2EE relay, host dials out (specified here)

| Dimension | Assessment |
|-----------|------------|
| Complexity | High. A relay service, a handshake, a transport adapter in all clients, failure handling. |
| Cost | A small VPS for people who need it. |
| Scalability | Many hosts per relay. |
| Team familiarity | None; there is no relay or Noise code in this repository. |

**Pros:** works behind carrier-grade NAT with no tailnet or tunnel account; the model several comparable clients use, so users may expect it; keeps TLS and tokens off the relay.
**Cons:** a custom cryptographic protocol is the riskiest thing in this whole list; metadata exposure; ongoing operation.

### Option B: stay with tunnels and the address list (the accepted direction in the other ADR)

**Pros:** no new protocol; reuses vetted infrastructure. **Cons:** needs an account with a tunnel provider; WebSocket through some of them is unverified (noted in that ADR).

### Option C: a hosted relay run by this project

**Pros:** easiest for users. **Cons:** a new operator in the threat model, as already decided.

## Consequences

- A cryptographic design review by someone other than the author is a precondition; this ADR names the handshake family, not an implementation.
- Tests would include a relay that records everything and a test that the recording contains no plaintext, a relay that replays or drops frames, and a rogue relay attempting its own handshake.
- This is the largest item in the set and should be last; all other ADRs work without it.

## Action Items

1. [ ] User reviews this ADR and [adr-remote-routes](adr-remote-routes.md) together.
2. [ ] Try Serve and a tunnel with the real phone first; record what fails.
3. [ ] If they fail: a threat-model document and an external crypto review before any code.
