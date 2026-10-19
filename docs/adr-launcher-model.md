# ADR: one launcher model for where an agent runs

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Comparable tools let work run in several places. Cursor 3 shows local, worktree, cloud sandbox and remote SSH agents in one sidebar. Orca runs agents on remote machines over SSH with file editing, git, terminals, auto-reconnect and port forwarding. Mux has local, worktree and SSH runtimes. The OpenHands SDK is built around *optional isolation*: local workspaces run in-process on the host and remote workspaces run in a container the backend spawns, with the same code either way. The Kubernetes Agent Sandbox project models a per-session pod, can use gVisor or Kata, and keeps a `SandboxWarmPool` for sub-second starts (the sources disagree on API group and version, and one says it is not yet production-ready for all workloads). See [research notes](adr-ade-index.md#research-notes).

Three ADRs here each add a way to start an agent somewhere other than "a local process": [container isolation](adr-container-isolation.md) (`launcher = "container"`), [WSL agents](adr-wsl-agents.md) (`launcher = "wsl"`), and the existing peer bridge (agents on other `acpd` hosts). Each invents its own answers to the same questions, most importantly: **where do the ACP callbacks run?** In ACP the *client* executes `fs/read_text_file`, `fs/write_text_file` and terminal calls on the agent's behalf. If the agent runs in a container, a WSL distro or another machine, the files it asks about are in that environment, not necessarily on the host that spawned it.

## Decision

Define the launcher as one interface with explicit answers, and require every launcher kind to state them.

1. **Interface** (host-internal): `start(agent, workspace, env, limits) -> {stdio, handle}`, `stop(handle)`, `describe() -> LauncherInfo`. Kinds: `host` (today), `container`, `wsl`, `peer` (an agent on another acpd, existing), and `ssh` (specified only, below).
2. **Questions every kind must answer in `describe()`**, shown on the agent screen and in `doctor`:
   - **Filesystem locality**: do `fs/*` callbacks act on the host's filesystem, the launcher's environment, or a mapped view? A launcher is acceptable only if the host's containment check is applied to the *path the callback will actually touch* (after any translation), failing closed when mapping is ambiguous.
   - **Terminal locality**: do terminal callbacks run on the host, in the launcher's environment, or not at all (refused with a clear error)?
   - **Network posture**: open, proxied ([adr-credential-proxy](adr-credential-proxy.md)) or none, and whether the host can enforce it.
   - **Process cleanup**: how a stuck descendant is guaranteed to stop, and the proof (a named test).
   - **Persistence**: what survives a host restart.
   - **Cost and startup**: typical start time, warm-pool support.
3. **SSH is a peer, not a pipe.** A plain `ssh host agent acp` stdio pipe would make the host run callbacks on the wrong machine. The supported remote form is an `acpd` on the remote machine, reached by the existing peer bridge, which executes callbacks locally. A bare SSH launcher is out unless a remote helper does the callbacks; Perch and Orca implement a remote bridge for the same reason.
4. **Warm pools and cloud sandboxes** (Kubernetes Agent Sandbox, hosted sandboxes) are `container`-like launchers with a remote engine. Not in scope until a use appears; the interface leaves room.
5. **Default stays `host`.** Nothing changes for existing agents. A launcher kind appears in the UI only when configured.
6. **One test suite**: a shared conformance test every launcher passes with the mock agent: start, prompt, filesystem read and write inside and outside the workspace (including a symlink escape), terminal run and kill, host crash and cleanup, restart behaviour. The container, WSL and peer launchers each add their platform-specific checks.

## Options Considered

### Option A: an explicit launcher interface with a conformance suite (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Mostly refactoring the existing launcher code and writing the suite. |
| Cost | None at runtime. |
| Scalability | n/a |
| Team familiarity | `launcher.rs`, `process_tree.rs`, `peers.rs` already exist. |

**Pros:** stops three ADRs from diverging; forces the callback-locality question to be answered once, which is the main correctness trap for remote and containerised agents; gives a checklist for any later backend.
**Cons:** an abstraction before the second and third implementations exist can be wrong; keep it thin.

### Option B: let each feature add its own launcher code

**Pros:** no up-front design. **Cons:** divergent answers on containment and cleanup, the exact areas that have needed the most test work.

### Option C: only `host` and `peer`

**Pros:** smallest. **Cons:** ignores containers and WSL, which the research shows other tools prioritise.

## Consequences

- [adr-container-isolation](adr-container-isolation.md) and [adr-wsl-agents](adr-wsl-agents.md) should be re-read against this ADR when it is accepted; their action items gain "pass the conformance suite".
- The conformance suite lives with the host tests and runs on every platform CI has; platform-specific launchers run where they can.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: extract the interface from the current launcher code without behaviour change; write the conformance suite for `host` and `peer`.
3. [ ] Make container and WSL spikes report through `describe()`.
