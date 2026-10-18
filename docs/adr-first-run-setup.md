# ADR: a first-run checklist that gets one agent working

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Threadlines has a first-run checklist that installs the agent CLIs and handles sign-in. AgentDeck offers a free built-in agent so a new user can chat without an account or key. AcpAgent Client pulls the ACP registry, downloads a managed Node when it is missing, checks binaries against sha256, and supports two sign-in styles (the agent opens a browser, or a built-in terminal runs the login command). Intelligent Terminal auto-detects agents and falls back to GitHub Copilot CLI installed through WinGet when none is found. Troubleshooting pages show the usual traps: `npx` missing because Node is not on PATH, an old npm without a valid `npx` (versions below 10.16.0 were observed), Node version floors that print `EBADENGINE`, a binary installed with `npx` that never reached PATH, and a fresh install not yet on PATH until a new terminal opens. ACP now has terminal authentication (completed 20 Aug 2026). See [research notes](adr-ade-index.md#research-notes).

This project has the parts: agent discovery (`discovery.rs`, `GET /v1/agents/discover`), `acpd doctor` with sign-in diagnostics, `acpd setup`, an Add agent flow with a scan, and install scripts for the host. New users still meet them as separate tools and error messages. The documented path (`docs/installation.md`) is a long page.

## Decision

A single **Get started** checklist on first launch of the desktop app (and on the phone after pairing, read-only), driven by the host's existing checks, ending when one agent has produced a reply.

1. Steps, each with a status, a "why" line and one action:
   1. **Host running and reachable** (desktop: started by the app; phone: paired). 
   2. **Workspace folder** chosen and authorised (empty allowed roots deny everything, so this is the first thing that blocks).
   3. **An agent is available**: show detected agents; otherwise offer Add agent from the registry ([adr-registry-browser](adr-registry-browser.md)) or by command.
   4. **Runtime prerequisites** for the chosen agent: Node present and new enough, `npx` callable, `uvx` for Python agents, git. Checked by running the binary with `--version` in the *host's* process environment, not a shell, so a freshly installed tool is found without reopening a terminal (the PATH trap). Missing items link to the vendor's installer; the app installs nothing by itself in the first cut.
   5. **Sign-in**: the agent's advertised `authMethods`; "agent auth" opens the agent's browser flow; a login-command method runs in a visible host terminal (terminal authentication). Verified by `doctor --agent` (initialize and `session/new`, no prompt).
   6. **First message**: an optional "Say hello" with the opt-in provider check (`--prompt-check`), one tiny no-tools prompt, showing the reply, with the cost note ("about a fraction of a cent on most providers").
2. The checklist is **resumable and read-only where it cannot act**: reopening it re-runs the checks. It never edits shell profiles, never runs an installer, never stores a provider key. Keys stay with the agent's own store or the operator's environment.
3. Failure messages come from one table keyed by check (for example `node_too_old: Node 22 or newer is needed for this agent; found 18.19`), shared with `doctor` so the CLI and the apps say the same thing.
4. A "skip" is always available; the checklist can be reopened from Settings. No telemetry about completion.
5. Phone: after pairing, shows the same checklist's status for the connected host and a "Fix on the computer" explanation for steps that need the desktop, because a phone cannot install software on the host.

## Options Considered

### Option A: one checklist reusing doctor, discovery and registry (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Check orchestration, a shared message table, a desktop wizard view. |
| Cost | None. |
| Scalability | n/a |
| Team familiarity | All checks exist in some form. |

**Pros:** the largest source of early drop-off gets one guided path; one set of error texts; read-only by design.
**Cons:** vendor installers change; some steps can only be explained.

### Option B: bundle a free built-in agent

**Pros:** works without any account. **Cons:** a model dependency and cost we would owe; against the stance that agents come from the registry.

### Option C: improve `docs/installation.md` only

**Pros:** cheap. **Cons:** still a separate page and separate error strings.

## Consequences

- Tests: each check against fakes (missing Node, old npm, no PATH entry for a fresh install, unsigned agent); message table coverage so no check lacks text.
- The "found without reopening a terminal" claim must be verified on Windows, where PATH changes do not reach running processes; the host may need to re-read the registry environment, which is a platform detail to test.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: shared check table behind `doctor` and a `GET /v1/setup` route.
3. [ ] Desktop: Get started view; Android: status view.
4. [ ] Verify the Windows PATH refresh behaviour.
