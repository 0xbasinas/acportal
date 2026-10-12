# Tray app

`acpd-tray` is a small system-tray controller for the host. It does what you would otherwise type, from a menu: set up, start, stop and pair. It runs the `acpd` program for every action, so the files, ports and behaviour are exactly those of the command line.

## Menu

| Item | What it does |
| --- | --- |
| Status line | `host not installed`, `not set up yet`, `stopped` or `running`. The icon colour is blue, amber, grey or green (red if the host answers something unexpected). Checked every 3 seconds, so `acpd start` typed elsewhere shows up too. |
| Download host (acpd)… | Shown first when the host program is missing. Downloads the latest `acpd` from this project's GitHub release, checks it against the release's checksums, and installs it (see below). Once installed it becomes **Update host** (disabled while the host is running, because the program cannot be replaced then). |
| Set up with Tailscale | Reads your Tailscale name, runs `tailscale serve --bg <port>` (default 8765) so the phone can reach the host over HTTPS, and saves that address with `acpd setup`. Creates the standard settings folders if they are missing. Needs Tailscale installed and signed in. |
| Start host / Stop host | `acpd start --bg` / `acpd daemon stop`. |
| Pair a phone… | Creates a one-use pairing code (valid 60 seconds) and opens a page in your browser with the QR code, the code, the address and a countdown. In the phone app: Connections > Add connection > Scan pairing QR code. |
| Open settings folder | Opens the per-user settings folder (`%APPDATA%\acpd`, `~/Library/Application Support/acpd` or `~/.config/acpd`). |
| Uninstall ACP Portal… | Click twice (second click within 10 seconds). Stops the host, deletes the host and tray programs, the Start menu shortcut or desktop entry, the PATH entry (Windows) and leftover pairing pages, then closes the tray. Your settings and paired phones are kept. On Windows a hidden helper finishes deleting the tray after it exits. |
| Uninstall and delete my settings… | The same, and also deletes the `acpd` settings and data folders (paired phones, saved sessions, folders and agents added from the phone). Folders are only deleted when literally named `acpd`. |
| Quit | Closes the tray. The host keeps running; stop it first if you want it off. |

A problem from the last action is shown as a `⚠` line under the status.

## Installing and running

Download `acpd-tray` from the release and run it; that is all you need. The tray finds the host program in this order: next to the tray as `acpd` / `acpd.exe`, next to the tray under its release name (`acpd-windows-x86_64.exe`, `acpd-linux-x86_64`, so a plain download works without renaming), in the folder the tray installs into, then `acpd` on your PATH. If none is found the menu says **host not installed** and offers **Download host (acpd)…**.

**Where the host is installed:** `%LOCALAPPDATA%\acpd\bin\acpd.exe` on Windows and `~/.local/share/acpd/bin/acpd` on Linux (next to the host's own `state` folder; `XDG_DATA_HOME` overrides it). The download is limited to this project's repository on https GitHub addresses (also after redirects), must match the size and the SHA-256 in the release's `SHA256SUMS.txt` (and GitHub's own digest when published), and is moved into place only when everything matches; a failed download leaves nothing behind. The checksums come from the same release as the file, so they protect against corruption and tampering in transit, not against a compromised release. Only Windows x86_64 and Linux x86_64 builds are published.

 On Windows it has no console window. Autostart at login is not set up; add a shortcut to the Startup folder (Windows) or an autostart entry (Linux) if you want it.

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
- The only network access is the optional host download above (and Tailscale's own commands); nothing is downloaded until you choose it.
- The tray never reads or stores device tokens. It only calls `acpd` and Tailscale's own commands, without a shell, and without console windows on Windows.

## Limits

- Only the default settings folder is used (the one `acpd setup` creates); a custom `--config` setup is not managed from the tray.
- Not signed. Windows SmartScreen may warn on first run.
- macOS is not built or tested.
- The tray menu and icon were not checked on screen here; the parsing, the QR page, the icons and the host calls are covered by unit tests and a real run against an isolated `acpd` host.
