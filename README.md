# ACP Portal

Use the coding agents on your computer from your phone. The **host** (`acpd`) runs on your laptop, starts the agent CLIs you register, and keeps them contained to folders you allow. The **Android app** pairs with it over HTTPS and shows the conversation, tool output, diffs and approval requests, so you can steer an agent from the couch and approve every file write and shell command from your pocket. An experimental **desktop app** (Electron, `desktop/`) does the same from Windows, macOS or Linux, for several hosts at once; a session can move between phone and desktop with **Take control**.

It speaks the open [Agent Client Protocol](https://agentclientprotocol.com) (ACP), so it works with any agent that does. **You register the agents you want; none are built in.**

## Install

### On your computer

**Windows** (PowerShell, no admin rights):

```powershell
irm https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.ps1 | iex
```

**Linux** (x86_64, no root):

```sh
curl -fsSL https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.sh | sh
```

Each script downloads the host and the tray app from the [latest release](https://github.com/0xbasinas/acportal/releases/latest), checks them against the release's `SHA256SUMS.txt`, and installs them in your user folder (a Start menu or applications-menu entry is added). Run it again to update. By default they install both the host and the tray; to install just one, set `ACPD_ONLY` to `tray` or `host` before the one-liner (`$env:ACPD_ONLY='tray'; irm ... | iex` on Windows, `curl ... | ACPD_ONLY=tray sh` on Linux), or save the script and pass `-Only tray` / `--only host`. A tray-only install can download the host from its menu. Both scripts are short, so read them first if you prefer: [`install.ps1`](scripts/install.ps1), [`install.sh`](scripts/install.sh).

**To remove it** (your settings and paired devices are kept unless you add the purge option):

```powershell
irm https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/uninstall.ps1 | iex
```

```sh
curl -fsSL https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/uninstall.sh | sh
```

Run from a terminal with no options, each script shows a menu: everything (keeping your settings), everything and delete my settings, only the host, or only the tray. Options skip the menu: `-Only host` / `--only tray` (and so on) removes just one of the two. Save the script and add `-Purge` (Windows) or pass `--purge` (Linux, `| sh -s -- --purge`) to delete the settings and data too; it asks you to type `DELETE` first unless you add `-Yes` / `--yes`. See [`uninstall.ps1`](scripts/uninstall.ps1) and [`uninstall.sh`](scripts/uninstall.sh). Or download the files yourself from the [releases page](https://github.com/0xbasinas/acportal/releases) and compare them with `SHA256SUMS.txt`; see [releases](docs/releases.md). The files are not code-signed, so Windows may show a SmartScreen warning for a file downloaded in a browser (the install script avoids it).

### On your phone

Install the newest signed APK from the [`app-v…` releases](https://github.com/0xbasinas/acportal/releases) (look for "ACP Portal app"), or a rolling build from the [`debug` release](https://github.com/0xbasinas/acportal/releases/tag/debug); allow installs from your browser or file manager once. After that, **Settings → Update app** installs newer builds for you and has a **Follow** switch for debug builds or signed releases; see [app updates](docs/app-updates.md). Debug builds and signed releases use different signing keys, as does a build installed from a computer, so switching between them needs one uninstall first (saved connections are removed).

## Quick start

1. Run **ACP Portal** from the Start menu (the tray icon). In its menu choose **Set up with Tailscale**, then **Start host**, then **Pair a phone…**. (The tray can also download the host itself if it is missing.) See [the tray app](docs/tray.md).
2. In the phone app, open Connections → Add connection → Scan pairing QR code.
3. Agents → **Add agent**: pick a popular one or type your own command, tap **Check it is installed**, then **Save agent**.
4. New session → **Add folder from this computer**, choose a project folder, start the session.

The same thing from a terminal:

```sh
acpd setup --address https://YOUR-HOST     # creates your settings in the standard per-user folder
acpd start --bg                            # runs the host in the background (-b also works)
acpd pair                                  # prints a QR code and a one-use code (valid 60 seconds)
acpd daemon status | reload | stop
```

The phone reaches your computer over HTTPS; [Tailscale Serve](https://tailscale.com/kb/1242/tailscale-serve) gives you a trusted address with no certificates to manage (`tailscale serve --bg 8765`). Remote connections require TLS and the app never bypasses certificate checks. See [installation and trusted TLS](docs/installation.md).

## Where things are stored

| What | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Settings: `config.toml`, `agents.json`, `launcher.json` | `%APPDATA%\acpd` | `~/Library/Application Support/acpd` | `~/.config/acpd` |
| Paired devices and session metadata (`state`) | `%LOCALAPPDATA%\acpd\state` | `~/Library/Application Support/acpd/state` | `~/.local/share/acpd/state` |
| Programs installed by the scripts or the tray | `%LOCALAPPDATA%\acpd\bin` | | `~/.local/share/acpd/bin` |

Folders and agents you add from the phone are kept in `user-settings.json` in the state folder, apart from your hand-written config. `XDG_CONFIG_HOME` and `XDG_DATA_HOME` override these places. Details: [native setup](docs/native-cli-and-motion.md).

## What it does

- **Any ACP agent.** A registry of commands you control; the app never needs an update to learn a new agent. Agent sign-in choices, models, modes and options come from the agent itself.
- **Review before anything happens.** File writes show a diff and wait for your approval; terminal commands show the exact line. Opt-in rules can auto-approve safe lines such as running tests. See [security](docs/security.md).
- **Contained.** Agents only get folders you allow, with symlink and path-escape checks; you can add or remove folders from the phone.
- **Survives your phone sleeping.** Sessions keep running on the computer; the phone reconnects and replays what it missed, never repeating a prompt or a decision. Saved conversations open offline.
- **Phone and desktop together.** One device controls a session at a time; the other watches live and can take control. Drafts, titles and prompt history are shared through the host.
- **Sandboxed commands (opt-in).** Approved commands can run in bubblewrap (Linux) or an AppContainer (Windows); on Windows you can pick cmd, PowerShell or Git Bash. See [agents](docs/agents.md) and [security](docs/security.md).
- **A terminal and a browser, opt-in.** With `[terminal] enabled = true` on the host, the desktop app and the phone can open a real shell in the session's folder (xterm.js; it runs as you, not sandboxed, and agents cannot see it). The desktop app also has an isolated in-app browser (tabs, find, Send to agent). With `[preview] enabled = true`, the phone can show a dev server running on your computer through the host. Both are off by default; see [security](docs/security.md#host-terminals).
- **A real conversation view.** Streaming Markdown, tool cards, thoughts, plans, attachments, a searchable model picker, light and dark themes, and TalkBack support.

## Build from source

Requires Rust 1.88 or newer for the host, JDK 21 for the Android app, Node 22 for the desktop app.

```sh
cargo build --locked --release -p acpd            # the host
cargo build --locked --release --manifest-path tray/Cargo.toml    # the tray (needs GTK 3 dev packages on Linux)
cd android/universal-acp && ./gradlew :app:assembleDebug           # the app (gradlew.bat on Windows)
cd desktop && npm ci && npm test && npm run build                  # the desktop app (Node 22)
cargo test --locked --workspace --all-targets
```

[Development](docs/development.md) lists the checks, the emulator workflow and the CI jobs. `scripts/dev.ps1` and the `Makefile` wrap the common commands, and `scripts/mock-host.ps1` starts a deterministic mock host for UI testing.

## Repository

```text
acpd/                  Rust host library, command line, mock agent and tests
protocol/              Shared Rust protocol types (ACP v1)
tray/                  System-tray controller for the host (its own Cargo workspace)
android/universal-acp/ Compose Android app and the independently tested JVM protocol module
desktop/               Electron desktop client (React + TypeScript), experimental
scripts/               install.ps1 and install.sh, developer and test scripts
assets/logo/           Logo SVGs
docs/                  Architecture, protocol, security, UI guide and acceptance evidence
examples/              Example configuration and disabled agent templates
.github/workflows/     CI, the Build APK (debug release) and Release workflows
```

## Documentation

- [Architecture](docs/architecture.md), [ACP forwarding and host API](docs/protocol.md), [security boundaries](docs/security.md)
- [Registering agents](docs/agents.md), [the tray app](docs/tray.md), [host releases](docs/releases.md), [app updates](docs/app-updates.md)
- [Installation and TLS](docs/installation.md), [native setup and background mode](docs/native-cli-and-motion.md), [reload rules](docs/daemon-and-reload.md), [pairing](docs/pairing-and-markdown.md)
- [Android app](docs/android.md) (including Terminal and Preview), [UI reference](docs/ui-reference.md), [accessibility](docs/ui-accessibility.md)
- [Development and testing](docs/development.md), [real-agent testing](docs/real-agent-testing.md), [acceptance report](docs/acceptance-report.md)
- [Orchestrator experiment (delegation, supervisor, peer bridge, worktree isolation and phone merge; off by default, PR #45)](docs/orchestrator-design.md), [status](docs/orchestrator-design.md#16-implementation-status-branch-experimentorchestrator)
- Desktop app (experimental): [desktop/README](desktop/README.md), [new-session composer](docs/desktop-composer.md), [several hosts](docs/desktop-multi-host.md), [runs and pre-merge checks](docs/desktop-runs-and-checks.md), [agent handoff](HANDOFF.md)
- [Contributor context](AGENTS.md), [open work](TODO.md), [completed work and evidence](TODO_DONE.md), [user-reported issues](USER_ISSUES.md)

## Status

The project is not yet production complete. The orchestrator, the desktop client, phone/desktop handoff, host scope checks, the OS sandbox for approved commands (Linux bubblewrap, Windows AppContainer) and the Windows shell choice are on `experiment/orchestrator` (PR #45, not yet on `main`); see [HANDOFF](HANDOFF.md). The host and the Android app work end to end and pass their automated tests, but several acceptance checks have not been done: real-phone testing (trusted TLS, networking changes, token expiry), gesture-driven Back animation, macOS cleanup after an abrupt host death, and signed releases. [TODO.md](TODO.md) has the exact open items; the dated history of what changed and what was verified is in the [CHANGELOG](CHANGELOG.md).

ACP v1 details follow the [official protocol repository](https://github.com/agentclientprotocol/agent-client-protocol). This project pins `agent-client-protocol-schema` 1.10.2 and negotiates wire version 1.
