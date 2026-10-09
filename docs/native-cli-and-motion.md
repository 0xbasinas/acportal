# Native host setup and shorter page transitions

9 October 2026, local implementation on `codex/session-model-picker`.

## One-time host setup

The workflow is built into `acpd`, without a wrapper script. Use the updated binary, under the account that runs Goose and owns the existing host config. From the directory containing `acpd.exe`:

```powershell
.\acpd.exe setup --config "$env:LOCALAPPDATA\ACPPortal-PhoneTest\config.toml" --address https://YOUR-TAILSCALE-HOST --name "Workstation"
```

Replace the path/address with your existing deployment. `acpd setup` without these flags asks for them interactively. It validates the existing config and registry, then remembers an absolute config path, HTTPS address and optional connection name in protected `launcher.json` in the normal acpd config directory. It does not rewrite config, widen workspace roots, start agents, create pairing codes, change credentials, or configure Tailscale/TLS/firewalls. It can also be rerun to change the remembered host.

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

Native background start/stop/status and explicit soft reload are implemented. Follow [the command guide and reload rules](daemon-and-reload.md). Foreground start remains available.

## Android motion

Ordinary page push and Back use overlapping 150 ms enter/100 ms exit fades. The whole page does not slide across the screen. Tapping the already selected Connections/Sessions/Settings tab no longer clears and rebuilds its route. Predictive Back retains Navigation 3's existing gesture transition; dialogs/sheets retain Material behavior. Compose's system motion scale remains in effect.

The final enabled-animation run passes dark/light clock-driven push/Back checks within a 240 ms virtual-time window and actual session-creation navigation recovery. Full results and limits are recorded in [TODO_DONE](../TODO_DONE.md). These checks establish transition completion and navigation behavior, not physical-phone frame-time improvement. Wider sheet/keyboard/streaming jank profiling and predictive-Back gesture acceptance remain open. No production-completion claim follows from the scoped tests.
