# Can ACP do what the T3 Code ideas need?

Checked 17 October 2026 against the pinned schema crate `agent-client-protocol-schema` 1.10.2 (source read from the Cargo registry, stable and `unstable_*` feature gates), the ACP site pages for overview, schema and session config options, the `claude-agent-acp` README, and a live `initialize` against the local `goose acp` 1.54.0 (no prompt sent). Not checked: a running Claude Code, Codex or Gemini adapter, and the ACP draft/v2 spec beyond what the crate gates.

## Short answer

ACP is the agent-to-host edge only. Five of the six planned items are host features that ACP neither helps nor blocks. The one place ACP limits us is permission presets: ACP has no standard "approval mode", so a preset can only govern what the host mediates.

## What ACP defines (schema 1.10.2)

| Area | Stable | Gated behind an `unstable_*` feature (off in our build) |
| --- | --- | --- |
| Agent methods | `initialize`, `authenticate`, `logout`, `session/new`, `load`, `prompt`, `cancel` (notification), `set_mode`, `set_config_option`, `list`, `delete`, `resume`, `close` | `session/fork`; `providers/list`, `providers/set`, `providers/disable` |
| Client methods | `session/request_permission`, `fs/read_text_file`, `fs/write_text_file`, `terminal/*` (create, output, wait_for_exit, kill, release), `elicitation/create`, `elicitation/complete` | |
| Session updates | message, thought, tool call and update, `plan`, `available_commands_update`, `current_mode_update`, `config_option_update`, `session_info_update`, `usage_update` | `plan_update`/`plan_removed`, `notice`, compaction, `subagent_update`, `session_message(_chunk)` |
| MCP in `session/new` | `stdio` always; `http` and `sse` only if `mcpCapabilities` says so | `acp` transport (MCP carried over the ACP connection) |
| Config | `configOptions` (select, boolean) with categories `mode`, `model`, `model_config`, `thought_level`; supersedes `modes`; agents must send both during the transition | |
| Auth | `authMethods` (agent type, or `terminal` type the client runs interactively), `authenticate`, `logout` | |
| Permission options | `allow_once`, `allow_always`, `reject_once`, `reject_always` | |
| Stop reasons | `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled` | |

Note `session/resume` and `session/close` are stable in 1.10.2 even though the crate groups the fork gate near them. Verified by reading the `#[cfg]` lines in `src/v1/agent.rs`.

## What we use today

From `acpd/src` and `docs/protocol.md`: `initialize`, `authenticate`, `session/new`, `session/load` (only when `loadSession` is true), `session/prompt`, `session/cancel`, `session/set_mode/model/config_option` (forwarded when the agent advertised the matching setup), `fs/*`, `terminal/*`, `session/request_permission`, `elicitation/create` (form mode, opt-in). No unstable features are enabled (`Cargo.toml`: `default-features = false`). Not used: `session/list`, `session/resume`, `session/close`, `session/delete`, boolean config options (client does not advertise `session.configOptions.boolean`), `additionalDirectories`.

## What real agents advertise

| Agent | Evidence | Capabilities |
| --- | --- | --- |
| Goose 1.54.0 (live, today) | `initialize` reply | `loadSession` true; prompt image and embedded context, no audio; MCP `http` true, `sse` false; `sessionCapabilities` list, delete, close and no `resume`; one auth method `goose-provider` of agent type (it points to `goose configure`) |
| Goose 1.53.0 | `docs/orchestrator-design.md` line 515 | same set |
| Claude Code via `claude-agent-acp` | its README, not run | v1: client MCP servers, terminals, TODO plans, custom slash commands, permission requests; `session/list`, `resume`, `fork`, `close`, `delete` and config-option modes only in experimental v2 behind `CLAUDE_AGENT_ACP_EXPERIMENTAL_V2=1`; subagent sessions only if the client declares a `subagents` capability |
| Codex, Gemini, OpenCode | listed as ACP agents on the ACP site; capabilities not checked | unknown |

## Feature by feature

| Planned item | Does ACP support or block it? | Consequence |
| --- | --- | --- |
| 1. Open a pull request from the Git view | Neither. Git and PRs are not in ACP. Our `/v1/git` runs `git` on the host. Suggest only needs `session/new` plus `session/prompt` in a throwaway session, which every agent has. | Host-only. The PR title/body suggestion works with any agent. |
| 2. Permission presets | Partly. ACP gives the client `session/request_permission` (we can answer allow/reject once or always), plus the host-mediated `fs/*` and `terminal/*` callbacks. It does not define approval modes: a `mode` option has agent-chosen ids and values (the spec's own example is "Ask" and "Code"). An agent that runs its own tools (we flag these as `ownTools`) never sends `fs` or `terminal` requests for them, and in a bypass-style mode may not send `request_permission` either. | A preset can only control the host side (access flags, shell review, auto-approve). It cannot promise "supervised" for an agent whose own mode is permissive. The preset UI must show the agent's own `mode` option next to it, and the effects text must say what is not covered. Do not try to map presets onto agent mode ids by name. |
| 3. Agent profiles / accounts | Not directly. `authMethods` and `authenticate` choose a sign-in method, not an account. `logout` is stable. `providers/*` (unstable) would let a client set an LLM provider endpoint, which is a different thing. Goose's auth method is just "run `goose configure`". | Profiles stay a host registry feature (`env` per entry). Account choice lives in the agent's own config or env vars. Gate on `authMethods` only for the sign-in hint. |
| 4. OS service install | Not an ACP concern. | Host-only. |
| 5. External MCP endpoint | Not defined by ACP. ACP's `mcpServers` is the client telling the agent which MCP servers to use, the opposite direction. `McpServer::Acp` (unstable) carries MCP over the ACP connection, and could replace our stdio `mcp-delegate` later, but only agents that implement it. | The external endpoint is a pure host design. Our current stdio injection works with every agent because stdio is mandatory in ACP. Do not rely on `http` MCP for the delegate, since Goose supports it but others may not. |
| 6. Remote routes | Not ACP. ACP is local stdio; our `acpd.v1` WebSocket wrapper and TLS are ours. | Host and client only. |

## Gaps in ACP for sub-agents and forks

- `session/fork` and `subagent_update` are unstable. We cannot depend on them, which is also why T3 uses its own child threads (orchestrator-owned) and falls back to a text handoff when an agent has no fork. Our orchestrator already does the same.
- Claude's adapter only exposes subagent sessions if the client declares `clientCapabilities.subagents`. Enabling the feature would mean turning on `unstable_subagents` in the schema crate, which `AGENTS.md` says not to do as a side effect.

## Stable ACP features we could use but do not

1. `session/list` (Goose advertises it): show an agent's own earlier sessions, including ones not created through acpd, and open them with `session/load`. Risk: listing exposes sessions outside the host's workspace roots, so filter by `cwd` against allowed roots.
2. `session/close` (Goose advertises it): cleaner shutdown than killing the process when the agent supports it. Fall back to the process kill we do today.
3. `session/resume` (not on Goose): resume without replaying history. Only worth adding where an agent advertises it.
4. Boolean config options: advertise `session.configOptions.boolean` and render toggles. The desktop and phone settings UI would need a switch row.
5. `additionalDirectories`: let the composer add extra folders when `sessionCapabilities.additionalDirectories` is present. Each folder must still pass `workspace_roots`.

## Recommended changes to the implementation prompt

- Item 2: add the ACP limitation above to the requirements. Presets describe host-mediated behaviour only, show the agent's `mode`/`configOptions` beside them, and refuse to offer a "supervised" label for an agent flagged `ownTools` unless the effects text says its own tools are not covered.
- Item 3: use `authMethods` only for hints; no ACP change.
- Item 5: note the `mcp-delegate` stdio choice and why it stays.
- Add a small optional item 7: use `session/list` and `session/close` where advertised, and advertise boolean config options. This is the only ACP-side work, and it needs live checks against Goose first.
