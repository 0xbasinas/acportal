# Host releases

The **Release** workflow publishes the host (`acpd`) as a GitHub release so you can install it without a Rust toolchain.

## What a release contains

| File | For |
| --- | --- |
| `acpd-windows-x86_64.exe` | Windows 10/11, 64-bit |
| `acpd-linux-x86_64` | Linux, 64-bit (built on Ubuntu 22.04, so glibc 2.35 or newer) |
| `acpd-tray-windows-x86_64.exe`, `acpd-tray-linux-x86_64` | The [tray app](tray.md); keep it next to `acpd`. On Linux it needs the GTK 3 and AppIndicator libraries |
| `SHA256SUMS.txt` | Checksums of the files above |

The Windows `.exe` files carry the ACP Portal icon. The binaries are **not code-signed**. Windows SmartScreen may warn about the `.exe` on first run, and downloads should be checked against `SHA256SUMS.txt`:

```powershell
(Get-FileHash .\acpd-windows-x86_64.exe -Algorithm SHA256).Hash
```

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing
```

## Making a release

Push a version tag, or run the workflow by hand:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

or Actions > **Release** > Run workflow with a tag such as `v0.1.0`. Tags must start with `v` and a digit. The workflow builds each platform with `cargo build --locked --release`, adds the checksums and creates the release with generated notes. A tag that already has a release fails instead of replacing it. For a test run, start the workflow by hand and untick **Publish**: it builds and checks everything, keeps the files as a workflow artifact for three days, and creates no tag or release.

## Using it

0. Easiest of all, one line: on Windows run `irm https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.ps1 | iex`; on Linux run `curl -fsSL https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.sh | sh`. The scripts download the files below, check them against `SHA256SUMS.txt`, install them in your user folder and add a Start menu or applications-menu entry. To remove them run `scripts/uninstall.ps1` or `scripts/uninstall.sh` (the same one-liner form, from `.../main/scripts/uninstall.*`); your settings and paired devices are kept unless you pass `-Purge` / `--purge`. Windows downloads made this way carry no "from the internet" mark, so SmartScreen does not ask.
1. Or download only `acpd-tray-windows-x86_64.exe` (or the Linux one), run it, and choose **Download host (acpd)…** in its menu. It installs the host in your user folder for you. Or download the host yourself (and keep it next to the tray under its release name, no renaming needed) or put it on your PATH as `acpd`. On Linux run `chmod +x` on downloaded files first.
2. `acpd setup --address https://YOUR-HOST` creates the settings in your user folders (see [native setup](native-cli-and-motion.md)).
3. `acpd start --bg`, then `acpd pair`.

macOS builds are not published yet; build from source with `cargo build --release -p acpd`.

## Limits

Not signed or notarized, no installers, no automatic update of the host (the phone app updates itself from the `debug` release; the host is replaced by downloading a new release). The workflow has not run until the first tag or manual dispatch; a first run may need small fixes.
