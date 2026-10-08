# Agent registry

Registry files contain a JSON array. The default location is the platform config directory under `acpd/agents.json`; Unix can override its base with `XDG_CONFIG_HOME`. Pass `--registry` to use a different file. Registry entries are trusted operator configuration, not data supplied by a phone.

Built-in definitions are in `acpd/registry/builtin.json`. Custom entries replace built-ins by ID, so an operator can disable an agent or change its executable without changing application code.

```json
[
  {
    "id": "my-agent",
    "name": "My Agent",
    "command": "/usr/local/bin/my-agent",
    "args": ["acp"],
    "transport": "stdio",
    "enabled": true,
    "env": {},
    "workingDirectory": null,
    "icon": null
  }
]
```

Arguments are literal process arguments. There is no shell string parsing. IDs use ASCII letters, digits, underscores, or hyphens, with a maximum length of 64. Invalid entries, unknown keys, unsupported transports, duplicate IDs in a file, and malformed environment keys fail configuration validation.

An explicit working directory must be absolute, exist, and be inside an allowed workspace root. The process launch cwd can differ from the session workspace, but both need host authorization.

## Goose local client

The default local command is `goose acp`. This is Goose's native stdio ACP server, as documented in the [official Goose CLI source](https://github.com/aaif-goose/goose/blob/main/crates/goose-cli/src/cli.rs). It is distinct from running a normal Goose terminal conversation.

```json
{
  "id": "goose",
  "name": "Goose",
  "command": "goose",
  "args": ["acp"],
  "transport": "stdio",
  "enabled": true,
  "env": {"GOOSE_MODE":"approve"}
}
```

Use Goose's own provider configuration and credential storage. The host inherits the operator environment, then applies registry overrides. `approve` is a documented [Goose mode](https://github.com/aaif-goose/goose/blob/main/documentation/docs/guides/config-files.md). Add `--with-builtin`, `developer` to the argument array if that extension is desired. Extension and provider configuration stay on the host. Do not switch Goose into automatic approval mode to bypass the client permission workflow. In `approve` mode an edit asks twice: Goose's own tool permission, then the host's `host-filesystem` write consent with the diff. Goose 1.53.0 often sends a whole shell line as `terminal/create.command` with no `args`; the host treats that as a shell-line consent (`host-shell-command`), shows the exact line on the phone, and only after per-line approval runs it through `/bin/sh -c` (or `cmd.exe /D /S /C` on Windows). Plain argv such as `pwd` still uses the literal `host-terminal` path. To avoid approving every `python3 -m unittest` by hand, enable the opt-in auto-review for Goose only:

```toml
[shell_review.goose]
rules = true
# Optional model reviewer for lines the rules leave undecided:
# [shell_review.goose.model]
# base_url = "https://api.deepseek.com"
# model = "deepseek-flash"
# api_key_env = "DEEPSEEK_API_KEY"
```

Read the limits in [security](security.md#shell-line-auto-review-opt-in) first: test commands run project code. See [real-agent testing](real-agent-testing.md) and the terminal section of [protocol](protocol.md).

## Discovery

Discovery checks configured executable names in absolute PATH directories, or an explicit absolute executable path. On Unix the target must be a file with executable permissions. Windows discovery expands PATHEXT but accepts only `.exe` or `.com` binaries for direct spawning. Batch wrappers would require shell parsing, so configure the actual runtime executable and script arguments instead.

Discovery does not execute unknown binaries to infer ACP support. It reports `available`, `missing`, or `misconfigured`; an executable being present does not prove provider authentication or ACP compatibility. Version is currently null because arbitrary version probes may have side effects. Use `probe` to verify protocol negotiation deliberately. For enabled, available definitions, the management API reports `running` when at least one ready or working live process exists. If only live sessions waiting for authentication exist, it reports `authentication_required`. Interrupted/exited records do not imply a live process, and live status never overrides missing/disabled/misconfigured discovery. Running means a process exists, not necessarily that a prompt is processing; the Sessions page shows turn activity separately. Runtime errors remain session-specific.

Adapters are configured exactly like native agents. For a Node adapter, configure the installed Node executable plus the installed adapter entrypoint. Avoid runtime package downloads through `npx -y` in a production registry. Pin adapters through the host's package tooling, then point the registry at the installed command.

## Custom-agent templates

[examples/agents.custom.json](../examples/agents.custom.json) contains disabled templates: a native Unix agent, a Windows Node adapter, OpenCode (`opencode acp`) and Gemini CLI through Node (`gemini.js --acp`). Paths are placeholders. The OpenCode and Gemini invocations were checked with `acpd doctor` on Linux (OpenCode 1.18.35: `ready`, prompt check passed; Gemini CLI 0.63.0 without credentials: `sign-in required` with its four methods listed); see [real-agent testing](real-agent-testing.md#doctor-against-other-acp-adapters-8-october-2026-linux-about-2257-athens). OpenCode's full session workflow was later run too. It edits files, runs shell commands and reads files with its own tools, so the host's write/terminal consents and workspace containment do not control it. Set `"permission": {"edit": "ask", "bash": "ask", "external_directory": "ask"}` in OpenCode's own `opencode.json` and treat its permission prompts as the real decision (see [security](security.md)). Gemini was checked for negotiation and sign-in only. The other two entries are not a claim of compatibility with a specific adapter. Install and pin your chosen agent first, replace the paths and arguments with its documented ACP stdio invocation, then enable only the configured entry. Do not add quotes inside a path containing spaces; JSON strings and separate array items preserve literal arguments.

Merge chosen entries into your private registry alongside the Goose override. A custom registry merges with built-ins by ID; omitting Gemini does not disable it. Keep an explicit `enabled: false` override for any unwanted built-in. Use `acpd --config /your/config.toml --registry /your/agents.json agents` to inspect discovery, followed by `probe --agent custom-native --workspace /your/allowed/project` with the same global options to test negotiation deliberately. `doctor --agent custom-native --workspace /your/allowed/project` also reports whether the agent needs sign-in; add `--prompt-check` to send one small prompt that shows whether its provider credentials work (it may cost one tiny model request). Restart the daemon after registry changes; it loads configuration at startup.

Avoid secrets in registry examples. Provider authentication belongs in the agent's credential store or the operator environment. A registry `env` object overrides inherited values and can contain sensitive data, so keep any private registry out of version control. An optional `workingDirectory` must already exist as an absolute path inside an allowed root; it does not change the requested session workspace.
