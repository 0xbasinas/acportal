# ADR: a host-side egress proxy that keeps provider credentials out of the agent

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Anthropic's secure-deployment guide recommends running a proxy *outside* the agent's boundary that injects credentials into outgoing requests, enforces a domain allowlist and logs requests. The agent then never holds the key. In the container form the agent has `--network none` and its only exit is a mounted Unix socket to that proxy. It also lists the limits: the proxy sees only the hostname of a TLS tunnel, so domain fronting is possible without TLS inspection, and some programs (Node `fetch` by default) ignore `HTTP_PROXY`. See [research notes](adr-ade-index.md#research-notes).

Today the registry passes provider credentials to an agent as plain environment entries (`AgentDefinition.env`, `registry.rs`) or the agent reads its own stored key. Anything the agent runs, including prompt-injected commands, can read and send that key. The existing sandbox covers approved *commands* (`terminal.rs`, `win_sandbox.rs`), not the agent process, and `shell_review.rs` has no network policy. [adr-container-isolation](adr-container-isolation.md) proposes a container launcher; this ADR is the network and credential half of that story and is useful on its own.

## Decision

Add an optional per-agent **egress proxy** run by the host.

1. `[agent.<id>.network]` with `mode = "open"` (default, today's behaviour), `"proxy"` or `"none"`, an `allow = ["api.deepseek.com", "github.com"]` domain list, and `credentials = [{ host = "api.deepseek.com", header = "Authorization", secret = "DEEPSEEK_KEY" }]`.
2. In `proxy` mode the host listens on a loopback port (Unix socket where the sandbox supports it) and sets `HTTPS_PROXY`/`HTTP_PROXY` for the agent. For model providers that accept a base URL (`OPENAI_HOST`-style settings) the host points the agent at a plain-HTTP loopback endpoint, adds the credential header, and forwards over TLS. The secret is read from the host's own environment or secret store and never enters the agent's environment.
3. **Enforcement needs a boundary.** With `open` or without a sandbox the proxy is advisory, because a process can ignore it. Real enforcement requires the agent to have no other route: `--network none` in a container, or a network namespace on Linux. The settings screen and `doctor` say which of the three situations applies and never imply protection that is not there.
4. Blocked requests are reported by hostname only ("api.example.org blocked for agent X"). The phone can allow the domain once or for the agent; allow-for-agent edits host config through the existing config-reload path. Request paths, headers, bodies and secrets are never logged.
5. No TLS interception in the first cut. The ADR states the domain-fronting limit in `docs/security.md` and does not install a CA into the agent's trust store.
6. Windows and macOS: the proxy runs, but network enforcement is not claimed until a sandbox mechanism is verified there.

## Options Considered

### Option A: a small host proxy with allowlist and header injection (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. CONNECT handling, base-URL forwarding, config, doctor checks. |
| Cost | Added latency per request, small. |
| Scalability | One proxy per agent session. |
| Team familiarity | Same listener, config and doctor patterns as the host. |

**Pros:** removes the most valuable secret from the agent's reach; gives the operator a list of domains an agent contacts; useful even in advisory mode as an audit view.
**Cons:** only as strong as the boundary around it; agents that need arbitrary hosts (package installs) need an allowlist that grows.

### Option B: rely on container egress rules or a firewall outside this project

**Pros:** no code. **Cons:** nothing tells the phone user what is blocked; credentials still enter the container.

### Option C: a TLS-terminating proxy with a trusted CA

**Pros:** can inject credentials for any HTTPS service and defeat domain fronting. **Cons:** certificate management, and the host would read all agent traffic. Not for the first cut.

## Consequences

- Secrets for injection live in the host environment or a protected store; `docs/security.md` says who can read them (the host user) and that the agent cannot.
- Tests: a request to an unlisted domain is refused; the credential header appears at the fake upstream and never in the agent's environment or the log; an agent that ignores the proxy is shown as "not enforced" by doctor.
- This overlaps `[preview]` and `/v1/preview` only in name; preview is inbound, this is outbound.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Spike on Linux with a mock agent and a network namespace; confirm a direct connection fails.
3. [ ] Host: proxy, config, doctor wording, tests.
4. [ ] Desktop and Android: blocked-domain prompt and per-agent allowlist editor.
5. [ ] Record which of Goose and one other registry agent honour a base-URL setting.
