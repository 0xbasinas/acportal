# ADR: read `devcontainer.json` as a source of setup suggestions

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

`devcontainer.json` is the portable environment definition supported by GitHub Codespaces, DevPod, Daytona and Coder. Ona (formerly Gitpod) pivoted to agent sandboxes and builds environments with prebuilds triggered by commits. Comparison pages note that Codespaces lacks sub-second boot and an API for programmatic agent sandboxes. See [research notes](adr-ade-index.md#research-notes).

[adr-workspace-scripts](adr-workspace-scripts.md) proposes host-owned setup, run and archive scripts, with a repository file treated only as a *suggestion*. [adr-container-isolation](adr-container-isolation.md) proposes an optional container launcher. Many repositories already describe their environment in `.devcontainer/devcontainer.json`, listing the image or Dockerfile, `postCreateCommand`, forwarded ports and recommended settings. Asking a person to retype that in host config is friction.

## Decision

**Read, never run.** The host parses `devcontainer.json` (JSON with comments) from a workspace and offers its contents as suggestions to the screens that already exist.

1. `GET /v1/devcontainer?workspace=` returns a bounded, validated summary: `image` or `build.dockerfile`, `forwardPorts`, `postCreateCommand`/`postStartCommand` (shown as text, not parsed), `remoteEnv` names (values hidden), `features` names, and `customizations` ignored. Parse errors are returned as a message. Unknown fields are ignored.
2. In the workspace-scripts editor, **Use from devcontainer** fills `setup` from `postCreateCommand` (string, array or object form shown as one reviewed line per command) and ports from `forwardPorts` into the preview port range suggestion. The person edits and saves into host config; nothing runs from the repository file.
3. In container mode ([adr-container-isolation](adr-container-isolation.md)), the image named in the file is shown as a *candidate* the operator may pin. The host never builds a Dockerfile in the first cut and never pulls an image on its own.
4. A `devcontainer.json` that wants privileged mode, extra mounts (`mounts`, `workspaceMount`), `runArgs`, `capAdd`, `securityOpt` or a Docker socket gets a visible warning listing exactly those keys and is not usable as a suggestion for container mode.
5. Dev Container features and lifecycle sequencing (`initializeCommand` runs on the *host* in the spec) are explicitly not implemented. `initializeCommand` is shown with a warning that it is the dangerous one.

## Options Considered

### Option A: parse and suggest, never execute (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low. A tolerant JSON-with-comments parser and a summary route. |
| Cost | None. |
| Scalability | One small file per workspace. |
| Team familiarity | Same shape as other discovery routes. |

**Pros:** saves retyping; teaches the operator what a repository wants before trusting it.
**Cons:** the dev container spec is large and partial support can mislead, hence the narrow field list.

### Option B: run the dev container with the reference CLI

**Pros:** full fidelity. **Cons:** a heavy dependency and a code-execution path from repository content; against the host-owned-config stance.

### Option C: ignore devcontainers

**Pros:** none. **Cons:** misses the most common environment description.

## Consequences

- `docs/security.md` is unchanged until implementation; `initializeCommand` and `postCreateCommand` are treated as untrusted repository text.
- Tests: JSON with comments and trailing commas; array and object command forms; a file with `runArgs: ["--privileged"]` produces the warning; path traversal in `build.dockerfile` is not followed.

## Action Items

1. [ ] User reviews this ADR; accept [adr-workspace-scripts](adr-workspace-scripts.md) first.
2. [ ] Host: parser and route.
3. [ ] Desktop: "Use from devcontainer" button; Android shows the summary read-only.
