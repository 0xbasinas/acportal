# Native host setup and shorter page transitions

9 October 2026, local implementation on `codex/session-model-picker`.

## One-time host setup

The workflow is built into `acpd`, without a wrapper script. Run it from any folder; nothing is written to the folder you run it in.

```powershell
acpd setup --address https://YOUR-TAILSCALE-HOST --name "Laptop"
```

With no `--config`, `acpd` keeps its files in the standard per-user places and creates them on first use:

| What | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Settings: `config.toml`, `agents.json`, `launcher.json` | `%APPDATA%\acpd` | `~/Library/Application Support/acpd` | `~/.config/acpd` |
| Host data: paired devices, session metadata (`state`) | `%LOCALAPPDATA%\acpd\state` | `~/Library/Application Support/acpd/state` | `~/.local/share/acpd/state` |

`XDG_CONFIG_HOME` and `XDG_DATA_HOME` override these on every system. A `state` folder that already exists next to the settings (where earlier versions kept it) is still used, so paired devices are not lost. Setup writes a starter `config.toml` (every setting is its default; nothing is required) and an empty `agents.json` only when they are missing, and never changes existing files. Add agents and folders from the phone; you do not have to edit these files.

`acpd setup` without `--address` asks for it. It remembers the config path, HTTPS address and connection name in protected `launcher.json` in the settings folder. To use a config somewhere else, pass `--config PATH` (it must already exist). Setup does not widen workspace roots, start agents, create pairing codes, change credentials, or configure Tailscale/TLS/firewalls. It can be rerun to change the remembered address. The default listener is `127.0.0.1:8765`; publish it with `tailscale serve --bg 8765`.

Everyday use:

```powershell
.\acpd.exe start
```

Keep that terminal open. In a second terminal:

```powershell
.\acpd.exe pair
```

Scan the terminal QR in Android, review the imported address and tap Pair connection. Codes expire in 60 seconds. `acpd pair --manual` prints the code without a QR; `acpd doctor` diagnoses the remembered config. Ctrl+C stops the foreground host and interrupts its live sessions. If the binary is installed on PATH, omit `.\` and `.exe`.

For this checkout, the current development binary is `C:/Users/basin/acportal/target/debug/acpd.exe`. Build updates with `cargo build --locked -p acpd --bin acpd`. Installed copies must be updated separately.

Existing `--config` commands still work. An explicit different config cannot inherit another host's saved QR address. The selected config parent is preserved for relative registry/workspace/state paths; even the same file through a different parent does not inherit that address. `--profile PATH` chooses a separate launcher file for another deployment. No profile means the original default-config/manual-code behavior. An invalid saved profile fails visibly; explicit `--config` plus an explicit pairing address can bypass it. Only paths/address/name are saved, never codes or tokens. Do not log or screenshot pairing output.

## Background mode and reload

Native background start/stop/status and explicit soft reload are implemented. `acpd start --bg` (also `-b` or `--background`) runs the host detached so the terminal is free; stop it with `acpd daemon stop`. Follow [the command guide and reload rules](daemon-and-reload.md). Foreground start remains available.

## Android motion

Ordinary page push, Back and predictive Back use one 90 ms fade-in over the outgoing page, which stays opaque until covered (no fade-out, so nothing dips through the background); the whole page does not slide or scale. (11 October 2026 change; the earlier 150/100 ms fades and the default predictive-Back slide are superseded.) Tapping the already selected Connections/Sessions/Settings tab no longer clears and rebuilds its route. Predictive Back retains Navigation 3's existing gesture transition; dialogs/sheets retain Material behavior. Compose's system motion scale remains in effect.

The final enabled-animation run passes dark/light clock-driven push/Back checks within a 240 ms virtual-time window and actual session-creation navigation recovery. Full results and limits are recorded in [TODO_DONE](../TODO_DONE.md). These checks establish transition completion and navigation behavior, not physical-phone frame-time improvement. Wider sheet/keyboard/streaming jank profiling and predictive-Back gesture acceptance remain open. No production-completion claim follows from the scoped tests.
