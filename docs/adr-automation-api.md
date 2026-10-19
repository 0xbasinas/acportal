# ADR: a documented automation interface (OpenAPI, CLI parity, CI use)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Several comparable tools expose scripting surfaces. Orca has a CLI for workflows (`orca worktree create` and similar). Codeg ships `codeg-cli` "for scripts and CI". Goose recipes are described as runnable in CI/CD. The OpenHands Software Agent SDK offers integrated REST and WebSocket services and local-to-remote portability; OpenCode 2.0 has a rebuilt API and an embeddable SDK. Symphony and Baton drive agents from outside through APIs. See [research notes](adr-ade-index.md#research-notes).

This host has a REST and WebSocket API used by its own clients, an operator CLI (`acpd start/status/agents/sessions/pair/doctor/config/probe/chat/revoke`), and documentation in `docs/protocol.md`. There is no machine-readable description of the API, no stable-versioning promise for scripts, no scoped token for automation (device tokens are for phones and peers; [adr-external-mcp](adr-external-mcp.md) proposes a read-only role for outside agents), and `acpd probe/chat` are diagnostic rather than designed for CI. A search found no OpenAPI document.

## Decision

Publish a small, versioned **automation surface** for scripts and CI. It is the same API with an honest contract, not a second API.

1. **OpenAPI description** of the `/v1` routes that are intended for automation, generated from or checked against the Rust handlers in CI so it cannot drift. Routes not listed are "internal" and may change. The version is `/v1` with additive changes only; a breaking change means `/v2` side by side.
2. **Automation tokens**: a device record with a distinct kind (`automation`), created with `acpd pair --automation --rights operator --expires 30d --scope project:/path`. It uses the rights levels from [adr-device-roles](adr-device-roles.md), may be restricted to a project root, has a mandatory expiry (maximum 90 days) and is shown with that label in Devices and in the audit trail ([adr-audit-and-trace](adr-audit-and-trace.md)). It cannot manage devices.
3. **CLI parity for common jobs**, all with `--json` output and stable exit codes (0 success, 2 usage, 3 not found, 4 refused by policy, 5 timed out, 6 task failed):
   - `acpd run --recipe NAME --param k=v --wait --timeout 30m` starts a task from a recipe ([adr-workflow-recipes](adr-workflow-recipes.md)), streams progress to stderr, and exits by outcome. Without `--wait` it prints the task id.
   - `acpd task list|show|cancel|diff|merge --yes`, `acpd approve --list|--allow ID` for approvals that policy allows scripts to answer (default: none), and `acpd checks run`.
   - These talk to the running host over the same API with an automation token; they do not start agents themselves.
4. **Client SDKs are generated, not hand-written**: a TypeScript client (used by the desktop app and the web client) and a Kotlin client (Android) from the OpenAPI file, so the apps and scripts use the same types. Hand-written protocol code (WebSocket frames, reducers) stays where it is.
5. **Webhooks out** use the hook mechanism in [adr-host-hooks](adr-host-hooks.md); this ADR adds no inbound webhook surface.
6. **Safety**: automation cannot do more than its rights allow; destructive actions (merge, push, delete) require the flag `--yes` and the corresponding right; CI use is expected to run against a throwaway worktree. Scripts never see agent credentials.
7. **Examples** under `docs/automation/`: a nightly dependency-update job, a PR-review job, and a GitHub Actions snippet that triggers a recipe on a self-hosted host reachable through the operator's own route.

## Options Considered

### Option A: OpenAPI contract, automation tokens, CLI parity, generated clients (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Spec generation or checking, a token kind, several CLI commands. |
| Cost | None. |
| Scalability | n/a |
| Team familiarity | The CLI and API exist; the contract and token kind are new. |

**Pros:** turns the host into something other tools can drive; removes duplicated client types; gives dispatcher, scheduler and CI one safe entry point.
**Cons:** a public contract is a maintenance promise; limiting "internal" routes needs discipline.

### Option B: keep the API private

**Pros:** freedom to change. **Cons:** scripters scrape the clients' behaviour anyway.

### Option C: an SDK first, no spec

**Pros:** friendlier for one language. **Cons:** one language only, and the spec is what keeps clients honest.

## Consequences

- `docs/protocol.md` becomes the human companion to the OpenAPI file.
- Tests: a CI job fails if a route marked public is missing from the spec or differs; exit-code tests on every CLI command; an automation token cannot reach device management; expiry is enforced.
- Security-sensitive: the automation token is a long-lived credential in CI. The docs recommend short expiry, project scope and a self-hosted runner.

## Action Items

1. [ ] User reviews this ADR; accept [adr-device-roles](adr-device-roles.md) and [adr-workflow-recipes](adr-workflow-recipes.md) first.
2. [ ] Decide generation versus checking for the OpenAPI file.
3. [ ] Host: token kind, `acpd run` and `acpd task`, with tests.
4. [ ] Generate the TypeScript client and replace hand-written types in the desktop app.
