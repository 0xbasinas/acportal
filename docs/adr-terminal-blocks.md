# ADR: command blocks in host terminals (OSC 133 shell integration)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

OSC 133 "semantic prompt" sequences mark where a prompt starts (A), where typed input begins (B), where command output starts (C) and where the command ends with its exit code (D). Terminals use them to jump between prompts, select a whole command's output and copy the last output: WezTerm defines Input, Output and Prompt zones; Zellij has a copy-last-command-output action; Konsole tracks the mode of each character. Fish emits them by default, bash and zsh need a snippet. VS Code uses a different, vendor protocol (OSC 633). Microsoft's Intelligent Terminal shows a status indicator on a failed command and loads its error context into the agent pane, and sees output from PowerShell and Bash/WSL shells. Warp is built around command blocks. Threadlines lets selected terminal lines be attached to the next message. See [research notes](adr-ade-index.md#research-notes).

This project's terminals (`pty.rs`, `terminals.rs`, xterm.js on desktop and in the Android WebView) are raw: the apps cannot tell where a command begins or ends, so "ask the agent about this error" ([adr-context-attachments](adr-context-attachments.md)) has to guess from rendered lines. A search of the desktop and host sources found no OSC 133 handling.

## Decision

Add optional **shell integration** for terminals the host starts, and use its markers to build command blocks.

1. **Injection**: when the host starts an interactive shell it can add a small integration script for the shells it launches: bash (a `PROMPT_COMMAND` and `PS0` snippet), zsh (`precmd`/`preexec`), fish (built in), and PowerShell (a `prompt` wrapper, subject to execution policy). The script is a literal file shipped with the host, passed by flag or environment variable (for example `--rcfile`, `ZDOTDIR`, `-NoExit -Command`), never appended to the user's rc files. It is off for shells the host cannot integrate (cmd.exe), and the UI says "no command blocks in this shell".
2. **Markers**: the script emits OSC 133 A, B, C and D with the exit code. The host does not interpret them; they pass through the pty to the clients as part of the stream.
3. **Clients**: xterm.js registers an OSC 133 handler and tracks marker positions per line (decorations). Features, in order: a gutter mark coloured by exit status; **Jump to previous/next command** (keyboard and a phone control); **Copy last output**; **Select command output**; and **Add to message** for one block ([adr-context-attachments](adr-context-attachments.md)). A command that exits nonzero gets a quiet "Ask the agent" action.
4. **Bounds**: marker tracking keeps the last 500 commands and the scrollback ring from [adr-terminal-persistence](adr-terminal-persistence.md); block text is read from the client's buffer, so the host stores nothing extra.
5. **Spoof resistance**: program output can print OSC 133 sequences. The markers are treated as hints for UI only; they never affect permissions, approvals, audit entries or any decision in the host. An attacker-controlled output can at worst mislead the block boundaries on screen.
6. The agent's own terminal tool calls (ACP terminal callbacks) already have a known command and exit status from the host; they do not need markers.

## Options Considered

### Option A: shipped integration scripts, markers passed through, UI-only use (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Four small scripts, per-shell launch flags, an xterm.js handler on two clients. |
| Cost | None at runtime. |
| Scalability | Per terminal. |
| Team familiarity | The shell choice code (`shell.rs`) already decides how shells launch. |

**Pros:** exact command boundaries without parsing text; makes the phone terminal navigable; supports "ask about this error" cleanly.
**Cons:** shell-specific snippets break on unusual setups; PowerShell policy can block them; markers can be spoofed (hence UI-only).

### Option B: parse prompts from rendered text

**Pros:** no shell changes. **Cons:** unreliable across prompt themes and multi-line prompts.

### Option C: host-side command tracking by wrapping each command

**Pros:** exact. **Cons:** the host would have to own the shell's line editor; not for an interactive shell.

## Consequences

- `docs/security.md` (Host terminals): the integration script is host-supplied code run inside the user's shell, loaded explicitly; it contains no network calls and logs nothing.
- Tests: the scripts emit the expected sequences in a real bash, zsh and PowerShell (CI runners have bash and PowerShell; zsh and fish may need a package); the client handler's block detection on recorded streams; spoofed markers do not change any host state.
- Verification of the phone side needs a real phone, as for the other terminal items.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Spike: xterm.js OSC 133 handler and one bash script on desktop.
3. [ ] Remaining shells; Android WebView.
4. [ ] Hook into the context chip tray.
