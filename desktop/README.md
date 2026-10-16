# acportal desktop

Electron client for `acpd`, the desktop counterpart of the Android app in `android/universal-acp`.
It pairs and talks to hosts exactly like the phone does. Branch `experiment/desktop` (PR #46); see [HANDOFF](../HANDOFF.md) for the full feature map.

Based on [electron-vite-starter](https://github.com/0xbasinas/electron-vite-starter)
(Electron + electron-vite + React + TypeScript, sandboxed renderer, context isolation).

## How it talks to the host

All network traffic is in the main process (`electron/host.ts`); the renderer only sees IPC.

| Concern | Rule (same as Android `HostApi.kt` / `WebSocketTransport.kt`) |
| --- | --- |
| Address | Origin only, no path or credentials. HTTPS for remote hosts; plain HTTP only for `localhost` / `127.0.0.1` / `::1`. |
| TLS | Certificate validation always on. Trust = Node's roots plus the operating system store (Android uses the system store). No pinning bypass, no permissive trust. |
| Pairing | `POST /v1/pair {code, deviceName}` with a one-use code from `acpd pair`; the bearer token is stored with Electron `safeStorage` (OS keychain). Without a keychain it stays in memory until the app quits. `Unpair` calls `DELETE /v1/device`. |
| REST | `/v1/status`, `/v1/agents`, `/v1/workspaces`, `/v1/sessions`, `/v1/tasks`, `/v1/tasks/{id}/merge` and `/cancel`, `/v1/git/{op}`, `/v1/checks`; `Authorization: Bearer`, no redirects. |
| Sessions | WebSocket `/v1/sessions/{id}/connect?after=<cursor>`, subprotocol `acpd.v1`, bearer header, JSON text frames only, 1 MiB frame cap. Replay (`replay_start` → events / `pending_permission` → `replay_complete`), then live events. Capped exponential backoff with jitter; 401/403/404/409 are terminal. Prompts are never resent after a disconnect. |
| ACP | Plain ACP messages: `session/prompt`, `session/cancel`, `session/set_mode`, `session/set_config_option`, permission responses. |

## Screens

Pairing, main workspace (hosts/projects/sessions, conversation, task tree + changed files + worktree panel),
shell and file approvals (host consent and agent permission cards with auto-review verdicts),
diff review and task review with *Merge into parent*, ⌘K / Ctrl K command palette, settings
(General, Agents, Hosts, Workspaces, Approvals, Appearance, Keyboard) and dark / light / system themes.
Also: several hosts at once (grouped sidebar, host switcher, live approvals from every host, desktop notifications with Approve once), form questions (elicitation), attachments, prompt history, MCP server editing (secrets sealed on this computer), adding agents (with sandbox and Windows shell choice), git view with branch picker and Learn mode, run on several agents with compare/merge, and the pre-merge checklist (host checks plus an AI review).
New session is an inline composer in the main area (Ctrl N, sidebar *New session*, project rows): a prompt box with attach, agent chip (with *Run on several agents…*), remembered model/mode, and a context row of chips for host, folder, branch (search, existing, new from base, own worktree), access rights and a Worktree switch, each opening an anchored, keyboard-navigable dropdown. Recent prompts sit below as cards. Sending docks the composer to the bottom (220 ms, none with reduced motion), shows the session in the sidebar straight away, and sends the first prompt once the agent is ready. Details: [desktop composer](../docs/desktop-composer.md).
Approval cards show the host's scope check (paths outside the workspace), whether the command runs sandboxed, and which shell runs it.
Design: Penpot boards 55-99, tokens from `Theme.kt`. Details: [desktop multi-host](../docs/desktop-multi-host.md), [runs and checks](../docs/desktop-runs-and-checks.md).

## Develop

```bash
cd desktop
npm install
npm run dev        # electron-vite dev server with HMR
npm test           # reducer, diff and address-rule unit tests
npm run build      # typecheck + production build into out/
npm run package:linux   # AppImage (local only; CI does not package)
```

For a local host: run `acpd --config <config> start`, then `acpd --config <config> pair` and enter
`http://127.0.0.1:<port>` and the code.

## Known gaps

- Checks config, MCP definitions and prompt history are stored on this computer only; a plan to share them through the host is in [HANDOFF](../HANDOFF.md#roadmap).
- The host-wide `[windows] shell` setting is edited in the host's `config.toml`; the app sets only the per-agent override when adding an agent (no edit form for existing agents).
- Delegated-task decisions are answered by opening the task's session (as on the phone).
- Composer model/mode choices are the values the agent last reported (it reports them only after starting); they are applied right after start, before the first prompt.
- No signed installers; Windows/macOS packaging is not set up.
