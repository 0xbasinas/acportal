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

0. Easiest of all, one line: on Windows run `irm https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.ps1 | iex`; on Linux run `curl -fsSL https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.sh | sh`. The scripts download the files below, check them against `SHA256SUMS.txt`, install them in your user folder and add a Start menu or applications-menu entry. To remove them run `scripts/uninstall.ps1` or `scripts/uninstall.sh` (the same one-liner form, from `.../main/scripts/uninstall.*`); your settings and paired devices are kept unless you pass `-Purge` / `--purge`. Install or remove just one of the two with `-Only tray` / `-Only host` (Windows) or `--only tray` / `--only host` (Linux), or the `ACPD_ONLY` environment variable; `-Purge` / `--purge` cannot be combined with it. Windows downloads made this way carry no "from the internet" mark, so SmartScreen does not ask.
1. Or download only `acpd-tray-windows-x86_64.exe` (or the Linux one), run it, and choose **Download host (acpd)…** in its menu. It installs the host in your user folder for you. Or download the host yourself (and keep it next to the tray under its release name, no renaming needed) or put it on your PATH as `acpd`. On Linux run `chmod +x` on downloaded files first.
2. `acpd setup --address https://YOUR-HOST` creates the settings in your user folders (see [native setup](native-cli-and-motion.md)).
3. `acpd start --bg`, then `acpd pair`.

macOS builds are not published yet; build from source with `cargo build --release -p acpd`.

## Limits

Not signed or notarized, no installers, no automatic update of the host (the phone app updates itself from the `debug` release; the host is replaced by downloading a new release). The workflow has not run until the first tag or manual dispatch; a first run may need small fixes.

## App releases (signed APK)

The **Release APK** workflow (Actions > Release APK > Run workflow, with a version such as `0.1.3`) builds the Android app with the **release** key and publishes it as its own GitHub release, tag `app-v<version>`, with `acportal-<version>.apk` and a `.sha256` file. It is never marked "latest", so the install scripts keep finding the host and tray release. `versionCode` is `major*10000 + minor*100 + patch` (0.1.3 is 103). The workflow refuses a tag that already exists, checks the signature with `apksigner`, and has a "publish" tick box (untick it to build and keep the files as a test artifact).

One-time setup, never committed:

1. Create a release key and keep a private copy of it (an installed app can only be updated by an APK signed with the same key; losing it means users must uninstall):
   `keytool -genkeypair -keystore acportal-release.jks -alias acportal -keyalg RSA -keysize 4096 -validity 10000`
2. Add repository secrets `RELEASE_KEYSTORE_BASE64` (the file, base64 encoded) and `RELEASE_KEYSTORE_PASSWORD`. Optional: `RELEASE_KEY_ALIAS` (default `acportal`) and `RELEASE_KEY_PASSWORD` (default the keystore password).

The release key is different from the shared debug key, so a debug build cannot be updated to a release build (or the other way round); uninstall once when switching. The in-app **Update app** screen still reads only the rolling `debug` pre-release (see [app updates](app-updates.md)); it does not offer `app-v*` releases yet. Without the secrets the workflow stops with a clear message, and a local `assembleRelease` stays unsigned.

