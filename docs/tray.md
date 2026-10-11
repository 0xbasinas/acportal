# Tray app

`acpd-tray` is a small system-tray controller for the host. It does what you would otherwise type, from a menu: set up, start, stop and pair. It runs the `acpd` program for every action, so the files, ports and behaviour are exactly those of the command line.

## Menu

| Item | What it does |
| --- | --- |
| Status line | `not set up yet`, `stopped` or `running`. The icon dot is amber, grey or green (red if the host answers something unexpected). Checked every 3 seconds, so `acpd start` typed elsewhere shows up too. |
| Set up with Tailscale | Reads your Tailscale name, runs `tailscale serve --bg <port>` (default 8765) so the phone can reach the host over HTTPS, and saves that address with `acpd setup`. Creates the standard settings folders if they are missing. Needs Tailscale installed and signed in. |
| Start host / Stop host | `acpd start --bg` / `acpd daemon stop`. |
| Pair a phone… | Creates a one-use pairing code (valid 60 seconds) and opens a page in your browser with the QR code, the code, the address and a countdown. In the phone app: Connections > Add connection > Scan pairing QR code. |
| Open settings folder | Opens the per-user settings folder (`%APPDATA%\acpd`, `~/Library/Application Support/acpd` or `~/.config/acpd`). |
| Quit | Closes the tray. The host keeps running; stop it first if you want it off. |

A problem from the last action is shown as a `⚠` line under the status.

## Installing and running

Put `acpd-tray` in the same folder as `acpd` (or have `acpd` on your PATH) and run it. On Windows it has no console window. Autostart at login is not set up; add a shortcut to the Startup folder (Windows) or an autostart entry (Linux) if you want it.

Linux needs the GTK 3 and AppIndicator libraries at run time (`libgtk-3-0`, `libayatana-appindicator3-1`, `libxdo3` on Debian and Ubuntu). Some desktops (for example stock GNOME) need an AppIndicator extension to show tray icons.

## Building

The tray is a separate Cargo workspace so the host's builds do not need GTK:

```bash
cd tray
cargo build --release          # produces tray/target/release/acpd-tray
```

On Debian/Ubuntu install `libgtk-3-dev libxdo-dev libayatana-appindicator3-dev` first. The `Tray` workflow runs fmt, clippy and the tests on Windows and Linux.

## Safety notes

- The pairing page is one local file with no outside scripts or styles (its content security policy blocks every connection). It sits in a temporary folder and is deleted two minutes after the code is made. The code works once and expires after 60 seconds.
- The tray never reads or stores device tokens. It only calls `acpd` and Tailscale's own commands, without a shell, and without console windows on Windows.

## Limits

- Only the default settings folder is used (the one `acpd setup` creates); a custom `--config` setup is not managed from the tray.
- Not signed. Windows SmartScreen may warn on first run.
- macOS is not built or tested.
- The tray menu and icon were not checked on screen here; the parsing, the QR page, the icons and the host calls are covered by unit tests and a real run against an isolated `acpd` host.
