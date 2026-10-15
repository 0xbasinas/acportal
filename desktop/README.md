# acportal desktop

Electron client for `acpd`, the desktop counterpart of the Android app in `android/universal-acp`.
It pairs and talks to a host exactly like the phone does; the Rust host is unchanged.

Based on [electron-vite-starter](https://github.com/0xbasinas/electron-vite-starter)
(Electron + electron-vite + React + TypeScript, sandboxed renderer, context isolation).

## How it talks to the host

All network traffic is in the main process (`electron/host.ts`); the renderer only sees IPC.

| Concern | Rule (same as Android `HostApi.kt` / `WebSocketTransport.kt`) |
| --- | --- |
| Address | Origin only, no path or credentials. HTTPS for remote hosts; plain HTTP only for `localhost` / `127.0.0.1` / `::1`. |
| TLS | Certificate validation always on. Trust = Node's roots plus the operating system store (Android uses the system store). No pinning bypass, no permissive trust. |
| Pairing | `POST /v1/pair {code, deviceName}` with a one-use code from `acpd pair`; the bearer token is stored with Electron `safeStorage` (OS keychain). Without a keychain it stays in memory until the app quits. `Unpair` calls `DELETE /v1/device`. |
| REST | `/v1/status`, `/v1/agents`, `/v1/workspaces`, `/v1/sessions`, `/v1/tasks`, `/v1/tasks/{id}/merge` and `/cancel`; `Authorization: Bearer`, no redirects. |
| Sessions | WebSocket `/v1/sessions/{id}/connect?after=<cursor>`, subprotocol `acpd.v1`, bearer header, JSON text frames only, 1 MiB frame cap. Replay (`replay_start` → events / `pending_permission` → `replay_complete`), then live events. Capped exponential backoff with jitter; 401/403/404/409 are terminal. Prompts are never resent after a disconnect. |
| ACP | Plain ACP messages: `session/prompt`, `session/cancel`, `session/set_mode`, `session/set_config_option`, permission responses. |

## Screens

Pairing, main workspace (hosts/projects/sessions, conversation, task tree + changed files + worktree panel),
shell and file approvals (host consent and agent permission cards with auto-review verdicts),
diff review and task review with *Merge into parent*, ⌘K / Ctrl K command palette, settings
(General, Agents, Hosts, Workspaces, Approvals, Appearance, Keyboard) and dark / light / system themes.
Design: Penpot boards 55-61 (and 62+ for the extra states), tokens from `Theme.kt`.

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

- Form elicitation, attachments, prompt history, MCP editing and adding agents are phone-only for now.
- Delegated-task decisions are answered by opening the task's session (as on the phone).
- No signed installers; Windows/macOS packaging is not set up.
