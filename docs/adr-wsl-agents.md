# ADR: run an agent inside WSL from a Windows host

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Windows is the platform where most of this project has been verified, and many agent CLIs behave best on Linux. Reports in the research: OpenAI recommends WSL2 for people who already work in WSL and labelled native Windows support experimental as of March 2026 (one third-party guide); the Codex desktop app turned a WSL project path `/home/...` into an invalid `C:\home\...` path after an update (June 2026 issue) and the changelog repeatedly fixes worktree creation for WSL repositories (October 2026); an image-paste pull request shows temporary paths and byte transfer assuming one environment. GitKraken's Kepler runs over SSH or WSL. See [research notes](adr-ade-index.md#research-notes).

In this host, `shell.rs` deliberately never picks WSL's `bash.exe` from System32 as the shell for approved commands (it is not Git Bash), and agents are launched as Windows processes by literal executable path. A person who installed Goose or Claude Code inside WSL cannot use it unless they wrap it by hand in a `wsl.exe` command line, which then breaks path assumptions: ACP `cwd` must be an absolute path the *agent* understands, and the host validates workspaces as Windows paths.

## Decision

Specify a **WSL launcher** for agents, with explicit path translation at the one boundary where it is needed, and no promise to translate elsewhere.

1. Agent definition gains `launcher = "wsl"` and `wsl_distro = "Ubuntu"` (required). The host starts the agent as `wsl.exe -d <distro> --cd <linux cwd> -e <literal executable> <literal args...>` with an argument array, never a shell string. stdio is the ACP channel as for any agent.
2. **Workspaces**: a workspace can be a Windows path (`C:\Users\a\proj`), translated to `/mnt/c/Users/a/proj` for `cwd` with `wslpath -u` run once and cached, or a WSL path in the form `\\wsl$\Ubuntu\home\a\proj`, translated to `/home/a/proj`. Authorised roots are still checked on the *Windows-visible* form; a path that resolves outside through a symlink is refused as for any other agent. Performance note shown in the UI: `/mnt/c` is slow for large repositories; WSL-native paths are fast.
3. **Callbacks**: ACP filesystem and terminal callbacks come back with the agent's Linux paths. The host translates them back with the same mapping and checks containment on the translated path. A path with no mapping (a Linux-only path outside the mounted roots) is refused with a clear error. Never translate by string substitution alone; use `wslpath` results.
4. **Terminals** requested by the agent run through the existing shell choice on the Windows side by default; an option `terminal_in_wsl = true` runs them inside the same distro. Approved commands in WSL are not covered by the AppContainer sandbox; the settings screen says so and the shell-review rules apply as for POSIX lines.
5. **Worktrees** for WSL-native repositories are created with the distro's own git (run through `wsl.exe`) so ownership and line endings stay consistent; mixing Windows git and WSL git on one repository is warned about, not attempted.
6. **Doctor**: checks `wsl.exe` exists, the distro is running or startable, the agent executable resolves inside it, `wslpath` works, and which filesystem the workspace is on.

See [adr-launcher-model](adr-launcher-model.md) for the questions every launcher must answer and the shared conformance suite this launcher must pass.

## Options Considered

### Option A: a launcher that wraps `wsl.exe` and translates paths at the boundary (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | High. Path mapping on two sides, two git flavours, process-tree containment across `wsl.exe`. |
| Cost | WSL start time; memory for the distro. |
| Scalability | Per distro. |
| Team familiarity | Launcher, doctor and containment code exist; the WSL parts do not. |

**Pros:** lets Windows users run Linux-first agents without leaving the phone workflow; matches where several tools are heading.
**Cons:** the largest bug surface (the research shows the other tools' pain points); process cleanup through `wsl.exe` needs its own proof.

### Option B: run the whole host inside WSL

**Pros:** everything Linux, no translation. **Cons:** the phone must reach a WSL network address and the desktop app and tray live on Windows; works today without this project's help.

### Option C: document the manual wrapper

**Pros:** no code. **Cons:** the path problems remain the user's.

## Consequences

- `docs/security.md`: WSL agents are outside the Windows sandbox and job-object containment unless proved otherwise; the process-tree cleanup test must kill a descendant started inside the distro before any claim.
- Tests need a Windows machine with WSL; the GitHub Windows runners may not have a distro, so this may be verified by hand only.
- A spike comes first: launch a mock agent through `wsl.exe` and check stdio framing and cleanup.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Spike on a Windows machine with WSL2: stdio framing, `wslpath`, kill behaviour.
3. [ ] If it holds: launcher field, mapping, doctor checks.
4. [ ] Record unsupported combinations (symlinks across the boundary, image paste).
