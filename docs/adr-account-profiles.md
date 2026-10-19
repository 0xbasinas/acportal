# ADR: named credential profiles per agent (personal and work accounts)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Orca shows Claude and Codex usage and rate-limit resets and lets you switch accounts without re-logging in. Coder and the gateway guides separate identities per developer so cost and access can be attributed. AcpAgent Client and Harnss let each agent definition carry its own command, arguments, environment and icon. Bellows notes each person keeps their own agent and credentials. See [research notes](adr-ade-index.md#research-notes).

An agent definition here has one command and one environment (`AgentDefinition.env`, `registry.rs`), and provider credentials come from the operator's environment or the agent's own store. A person with a personal and a work account for the same agent, or two API keys with different limits, must edit the registry or restart the host with different variables. The phone's New session screen cannot choose.

## Decision

Let an agent define several **profiles**: named bundles of environment variable *names and sources*, chosen per session.

1. In host config: `[agent.goose.profiles.work] env = { OPENAI_API_KEY = { from = "env:WORK_OPENAI_KEY" }, OPENAI_HOST = "https://api.example.com" }` and a `default_profile`. Values are either plain non-secret strings or a reference to a secret source: a host environment variable, or a secret held in the host's protected store (see below). References are resolved at launch; the resolved value goes only to the agent process environment.
2. The New session screen shows a Profile picker when an agent has more than one, with the profile's *label* and the non-secret fields (host URL, model default). Secret values and even their reference names are never sent to clients beyond the label.
3. A session records its profile name; Sessions, attribution ([adr-usage-dashboard](adr-usage-dashboard.md), [adr-model-governance](adr-model-governance.md)) and scorecards ([adr-agent-scorecards](adr-agent-scorecards.md)) can group by it.
4. Agents with their own login store (a CLI signed in with a subscription) can have a profile that sets a different home or config directory environment variable for that agent, so two sign-ins live side by side. The host does not read those stores.
5. **Secret store**: a host-side protected file (owner-only, encrypted with a key held in the OS credential store where available) for the cases where an environment variable is inconvenient. Entered at the host (CLI or desktop app); the phone can pick a profile but cannot create or read a secret value. Backup excludes it unless explicitly chosen ([adr-data-lifecycle](adr-data-lifecycle.md)).
6. **Rate-limit and usage windows**: only if the agent reports them through ACP (`usage_update` carries no window today). The ADR does not scrape provider dashboards or the agent's private files.
7. Switching profile mid-session is not offered; start a new session or fork. This keeps the audit story simple and avoids restarting an agent under a person.

## Options Considered

### Option A: profiles of env references chosen at session start (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. Config schema, resolution at launch, a picker. |
| Cost | None. |
| Scalability | A few profiles per agent. |
| Team familiarity | Same env handling and launcher code. |

**Pros:** solves the two-accounts case; keeps secrets on the host; compatible with the credential proxy (a profile can name the proxy base URL).
**Cons:** another config concept; a secret store is new security-sensitive code.

### Option B: one agent definition per account (duplicate entries)

**Pros:** works today. **Cons:** clutter, and attribution by agent id mixes accounts and agents.

### Option C: switch accounts by rewriting the agent's own auth files

**Pros:** matches what some tools do. **Cons:** the host would modify another program's credential files.

## Consequences

- `docs/security.md`: profile references are resolved at launch only; clients never receive secret values; the secret store's key handling needs review like the backup cipher.
- Tests: a profile changes the child environment as configured; clients' JSON never contains a resolved secret; an unresolved reference fails the launch with a clear error naming the profile, not the value.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: schema, resolution, tests; environment-variable sources first (no secret store).
3. [ ] Clients: profile picker in New session.
4. [ ] Decide on the secret store separately.
