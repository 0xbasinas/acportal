# ADR: optional container isolation per agent task

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Sculptor runs each agent in its own Docker container. The Copilot app offers "Local Sandboxes" with restricted filesystem, network and system access. A 2026 guide says worktrees isolate code but not ports or databases. See [the competitor notes](adr-ade-index.md#research-notes).

This host already sandboxes *approved terminal commands* (`terminal.rs` with bubblewrap, `win_sandbox.rs` AppContainer, per-agent `sandbox`). The **agent process itself** runs as the host user with the host's environment. A container for the whole agent would also contain the agent's own network access, its tool use that bypasses ACP terminals, and its dependency installs. It is also the largest and most platform-specific change in this set.

## Decision

Do not build a general container runtime yet. Decide the **seam** and a spike.

1. Add an optional per-agent `launcher` field: `"host"` (default) or `"container"`. With `"container"` the host launches the agent as `docker run` / `podman run` with a literal argument array, never a shell line.
2. Fixed container policy: the task's worktree mounted read-write at the same absolute path (so ACP `cwd` stays valid), nothing else from the host mounted, a non-root user, `--network` set from `[agent.<id>] network = "none" | "default"`, CPU and memory limits from config, and `--rm`.
3. ACP stdio passes through `docker run -i` unchanged, so the host's actor code does not change. Callbacks (filesystem, terminal) still go through the host, which already enforces workspace containment.
4. Credentials: provider keys enter the container only as names listed in config, as environment values, never copied into an image or a log.
5. Cleanup: the existing process-tree machinery stops the client process; the host also runs `docker rm -f` by a container name it generated and recorded. A stale-container sweep at startup removes only names with the host's prefix.
6. Windows and macOS support the same field only if the engine is installed; `doctor` reports engine, version, and a `--network none` probe.
7. First scope is a spike on Linux with Podman or Docker, one image supplied by the operator. Building or pulling images is out of scope.

## Options Considered

### Option A: a thin container launcher behind the existing process abstraction (chosen, spike first)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium to high; most risk is cleanup and path mapping. |
| Cost | Container start time per task; image maintenance by the operator. |
| Scalability | Limited by host CPU and memory. |
| Team familiarity | Reuses the launcher, process-tree and doctor modules. |

**Pros:** real network and filesystem containment for the agent itself, which no host-side callback check can give.
**Cons:** path and uid mapping on Windows and macOS; the agent needs a working image with its own tooling.

### Option B: tighten the existing OS sandbox to cover the agent process

**Pros:** no container engine. **Cons:** the Linux and Windows mechanisms were built for short commands; applying them to a long-lived agent that needs its own network and package caches is untested.

### Option C: leave isolation to the user (run the host in a VM)

**Pros:** no work. **Cons:** the project says nothing to help them; the phone user can't tell how contained a session is.

## Consequences

- `docs/security.md` gains a section: container mode reduces what a misbehaving agent can reach but does not make it safe to run untrusted code; the engine socket is never mounted.
- The session header shows the launcher and network setting from the host, so the person can see how contained a session is.
- Not decided: remote container hosts, GPU access, image building.

## Action Items

1. [ ] User reviews this ADR; the decision is the seam and the spike.
2. [ ] Spike on Linux: launch a mock agent and then Goose in a container; measure start time; test cleanup after a host kill.
3. [ ] If the spike holds, implement the field, `doctor` probe and header line.
4. [ ] Record Windows and macOS findings separately; do not claim them from Linux.
