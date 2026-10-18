# ADR: browse and install agents from the ACP Registry

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

The ACP Registry went live on 28 January 2026 (JetBrains and Zed). An agent registers once and every ACP client can offer it. It lists only agents that return valid `authMethods` in the ACP handshake. JetBrains IDEs have an "Install From ACP Registry" picker. See [the competitor notes](adr-ade-index.md#research-notes).

[orchestrator-design.md](orchestrator-design.md) section 2 already records the index (`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`, 41 agents when read), the manifest shape (`distribution` of `binary`, `npx` or `uvx`) and says import copies manifest `env` as plain values. It plans an import. The host-owned **Add agent** flow (`GET /v1/agents/discover`) finds agents already installed. There is no screen where a person sees the registry's agents and adds one, and I did not confirm whether the import described in the design is implemented.

## Decision

Add a registry browser to Add agent on desktop and Android, backed by the host.

1. The host fetches the index on request (`GET /v1/agents/registry`), caches it in memory for an hour, and returns a bounded list with id, name, description, version, and the distribution kinds it can run on this platform.
2. The host does the fetch, not the client, so a phone never contacts the CDN and the operator's egress policy applies. Hosts without outbound access show "registry unavailable" and still offer the manual form.
3. **Adding** an entry writes a normal disabled registry entry (`enabled: false`) with a literal executable and argument array. For `npx`/`uvx` the executable is the resolved `npx`/`uvx` path and the package spec is an argument; for `binary` the archive must match the manifest `sha256` before it is unpacked into a host-owned directory. Nothing is run at add time.
4. A person enables it explicitly, then runs the existing `doctor --agent` sign-in check. An entry is marked "from registry, version X" so an update can be offered later; updates are never automatic.
5. `env` values from a manifest are shown for review and copied as plain, non-secret values only.
6. **Upgrades install beside the old version**: the new distribution unpacks to a new directory, runs `doctor --agent` (initialize and `session/new`, no prompt), and only then switches the registry entry; a failed check leaves the old version untouched (AcpAgent Client does this). Binary downloads are checked against the manifest `sha256`. If `npx` needs Node and none is found, the screen says so and offers the install instructions; the host does not download a runtime on its own in the first cut.
7. **Sign-in flows** from `authMethods` are shown as the agent advertises them. ACP's *terminal authentication* RFD is completed (20 August 2026), as are logout (22 May); an auth-state query is still a draft. "Agent auth" (the agent opens a browser) is started by the agent on the host's machine; a method that needs a login command is run in a *host terminal* the person sees and controls, never silently. The result is checked by the same sign-in diagnostics doctor already has.
8. Unknown manifest fields are ignored; a manifest that fails validation is listed as unavailable with the reason.

## Options Considered

### Option A: host-side browse, add as a disabled entry (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Fetch, validate, platform filter, checksum, UI. |
| Cost | One small download per hour at most. |
| Scalability | One index; bounded list. |
| Team familiarity | Same registry file, discovery and doctor. |

**Pros:** follows the project rule that agent configuration belongs on the host; keeps a person in control of execution.
**Cons:** `npx` and `uvx` agents download code at first launch, outside this project's control; the UI must say so.

### Option B: bundle a snapshot of the registry in each release

**Pros:** works offline. **Cons:** goes stale; the registry is meant to be live.

### Option C: let the client fetch and send a ready-made entry to the host

**Pros:** no host fetch. **Cons:** the host would trust client-supplied launch lines for a new executable; weaker than validating on the host.

## Consequences

- `docs/agents.md` and `docs/security.md`: adding from the registry is trusting the registry and the package source; enabling is a separate, deliberate step.
- Tests use a local fixture index, including a bad checksum, an unsupported platform and a manifest with extra fields.
- This builds on what the orchestrator design already describes; reconcile section 2 when implementing so the two do not diverge.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Check how much of the import in orchestrator-design section 2 is already coded.
3. [ ] Host: route, cache, validation, checksum, add-as-disabled.
4. [ ] Desktop and Android: registry tab in Add agent.
