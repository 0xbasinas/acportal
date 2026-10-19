# ADR: open a worktree in an external editor

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

The Copilot app has an "Open" button that hands a worktree to VS Code. Superset's review can happen inside the app or in an external editor such as VS Code or JetBrains. Zed and JetBrains are both ACP clients, so a person may already use them beside this app. See [the competitor notes](adr-ade-index.md#research-notes).

This project has diffs, a Git view and a terminal but no way to open the files an agent changed in the person's own editor. The desktop app runs on the same machine as a local host, but may also be paired with a remote host, where "open a path" has no meaning on the person's machine.

## Decision

Add **Open in editor** to the desktop app only, for a host that is the same machine.

1. Desktop settings hold a short list of editors: id, label, and a literal executable plus argument template with a `{path}` placeholder (VS Code `code`, Zed `zed`, JetBrains launchers, and a custom entry). Defaults are suggestions the app checks for on PATH; none is run until the person clicks.
2. The button appears on a session, task and worktree when (a) the host is local (its address is loopback and its process belongs to this desktop app's user), and (b) the path is inside a workspace the host reports as authorised.
3. Launch uses `execFile` with an argument array. The path is passed as one argument, never interpolated into a shell line. Paths beginning with `-` are passed after `--` or rejected, whichever the editor supports.
4. For a remote host the button is replaced by "Copy checkout command" (`git fetch` and `git worktree add` for the branch) and does not guess an SSH remote.
5. The phone gets nothing from this ADR.

## Options Considered

### Option A: a configurable local launcher (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low. Settings list, one IPC call. |
| Cost | None. |
| Scalability | n/a |
| Team familiarity | The Electron app already launches helpers for the browser and tray. |

**Pros:** small, useful on day one, no host or protocol change.
**Cons:** only for a local host; each editor's flags differ.

### Option B: `vscode://` and `zed://` URL schemes

**Pros:** no executable list. **Cons:** support varies, they cannot open an arbitrary worktree reliably, and some editors prompt on untrusted folders in confusing ways.

### Option C: remote editing over SSH or dev containers

**Pros:** works for remote hosts. **Cons:** needs credentials and configuration outside this project's model.

## Consequences

- The Electron main process, not the renderer, performs the launch; the renderer sends an editor id and a session id, not a command or a path. The main process looks up the path from host data.
- Tests: an argument containing spaces, quotes or a leading `-` reaches the editor intact; a remote host never shows the button.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Desktop: editor settings, IPC handler and button, with tests for argument handling.
3. [ ] Windows `.cmd` launchers need care (`code.cmd`): decide whether to resolve the real executable or reject `.cmd` entries.
