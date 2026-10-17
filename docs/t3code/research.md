# T3 Code research notes

Source: a shallow clone of https://github.com/pingdotgg/t3code (MIT) read on 17 October 2026, specifically `docs/user/*`, `docs/internals/*`, `docs/orchestration-v2/*` and `infra/relay/README.md`. The code was not built or run, so these are what its documentation says, not verified behaviour. Paths below are inside that repository.

## Shape

- TypeScript monorepo: `apps/{server,desktop,web,mobile,marketing}`, `packages/*` (provider adapters, source-control adapters, `ssh`, `tailscale`, `contracts`, `client-runtime`), `native/*`, `infra/relay`.
- One environment server owns providers, execution, files and durable state (SQLite). Desktop, web and mobile are clients that connect over HTTP and WebSocket. The desktop app can run with no local environment.
- Providers: Claude Code, Codex, Cursor, Grok, OpenCode, Pi, Muse, Antigravity, plus a generic ACP Registry driver. The ACP driver enables session load, fork, models and modes only when the agent advertises them (same rule as ours) and degrades steering to interrupt-and-restart and forking to a text handoff.

## How it compares with ours

| Area | T3 Code | ACP Portal today | Takeaway |
| --- | --- | --- | --- |
| Remote access | Direct, Tailscale Serve (`t3 pair --tailscale`), SSH launch/forward, and a hosted relay ("T3 Connect") for tunnels | Own TLS address only | Routes list and Tailscale recipe are cheap wins; the hosted relay is a large trust decision |
| Routes | Ordered route list per machine; a LAN address is checked against the public descriptor before any credential is sent; learned LAN/tailnet routes; re-preflight earlier routes while on a later one | One address per host | Item 6 in the prompt |
| Auth | Scoped sessions (`access:write`, `terminal:operate`, `orchestration:operate`...), exchange can only narrow scopes, DPoP-bound tokens, short-lived WebSocket tickets, access read model never exposes pairing secrets | `Role::{Phone, Peer}`, device tokens | Scopes are heavier than we need; the "separate audience rejected everywhere but /mcp" rule is worth copying |
| Permission modes | Supervised, Auto-accept edits, Auto (provider review), Full access, per thread | access chips + shell review + auto-approve per agent | Item 2 |
| Background service | `t3 service install/status/restart/uninstall` on Linux (systemd user, lingering) and macOS (launchd); Windows unsupported; `t3 update` | `acpd daemon` only | Item 4 |
| Updates | Stable launcher owns the runtime, stages an immutable version, trial start, SQLite snapshot, commit boundary, rollback | none for the host (app updates only for Android) | Design note only |
| Outside agents | `/mcp` with OAuth (PKCE), read-only or mode-ceiling grants, sessions rejected on non-MCP routes, revocable | stdio delegation for an agent's own tasks | Item 5 |
| Sub-agents | Orchestrator-owned child threads on any provider via the same MCP server; parent history not copied | `orchestrator/` delegation with worktrees and a supervisor | We are ahead on worktree merge and supervisor |
| Source control | GitHub (direct API or `gh`), GitLab, Forgejo/Gitea, Bitbucket, Azure DevOps, GitCafe; PRs, stacked PRs, clone and publish; per-turn diff that ignores pulled-in files | `/v1/git`, compare URL, Suggest commit message, worktrees | Item 1, GitHub only |
| Restart recovery | Durable run records; interrupted runs continue after restart only when provider thread and native resume identity match, never on user stop or pending approval | `session_catalog.rs` restores metadata, explicit Resume | Worth reading `server-updates.md` before changing ours |
| Handoffs | Textual context handoff between providers, 240-char item summaries, fork merge-back | `/v1/shared`, takeover | Could reuse the idea for switching agent mid-task; not in this branch |
| Multi-machine | Load balancing of new threads across machines by CPU/memory preference | Several hosts, no balancing | Not proposed |
| Notifications | Relay delivers APNs/FCM and Live Activities | Desktop notifications, no push | Needs a relay; not proposed |
| Diagnostics | `t3 doctor` repairs and shrinks the database | `acpd doctor` read-only | Not proposed |
| Devices panel | Streams iOS/Android simulators through a loopback-only supervised hub with an allowlisting proxy | none | Interesting security pattern (never expose the hub, proxy allowlists reads); not proposed |

## Security patterns worth keeping in mind

- Advertised endpoints are hints. Only the connecting device can prove a route works; a loopback address must never be offered as shareable.
- Do not put a pairing secret in a query string; T3 puts it in the URL fragment so it never reaches the hosted origin.
- The relay is a trusted broker: it holds signing authority for credential mint requests, so a compromised relay key is not made harmless by DPoP. T3 says this explicitly. Any relay we ever add needs the same honesty in `docs/security.md`.
- A signed asset URL is a bearer credential tied to one file identity (device/inode), rechecked on serve.
- Reusable development credentials must not fall back from a rejected normal credential.

## What we have that T3's docs do not mention

OS sandbox for approved terminal commands (bwrap, AppContainer), shell-line auto-review with a hard-deny list, scope checks on terminal consents, per-line consent shown with the exact shell line, delivery-journal replay with cursor, two-machine peer bridge, and an explicit "opening saved history never starts an agent" rule.

## Not adopted on purpose

Cloud sign-in and a hosted control plane, analytics/telemetry, voice input, browser import, snap-shot tools, Muse/Pi/Antigravity adapters, and the Effect-based server architecture.
