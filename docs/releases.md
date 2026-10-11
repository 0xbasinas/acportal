# Host releases

The **Release** workflow publishes the host (`acpd`) as a GitHub release so you can install it without a Rust toolchain.

## What a release contains

| File | For |
| --- | --- |
| `acpd-windows-x86_64.exe` | Windows 10/11, 64-bit |
| `acpd-linux-x86_64` | Linux, 64-bit (built on Ubuntu 22.04, so glibc 2.35 or newer) |
| `SHA256SUMS.txt` | Checksums of the files above |

The binaries are **not code-signed**. Windows SmartScreen may warn about the `.exe` on first run, and downloads should be checked against `SHA256SUMS.txt`:

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

or Actions > **Release** > Run workflow with a tag such as `v0.1.0`. Tags must start with `v` and a digit. The workflow builds each platform with `cargo build --locked --release`, adds the checksums and creates the release with generated notes. A tag that already has a release fails instead of replacing it.

## Using it

1. Download the file for your system and put it somewhere on your PATH (rename it to `acpd` / `acpd.exe` if you like). On Linux run `chmod +x acpd-linux-x86_64` first.
2. `acpd setup --address https://YOUR-HOST` creates the settings in your user folders (see [native setup](native-cli-and-motion.md)).
3. `acpd start --bg`, then `acpd pair`.

macOS builds are not published yet; build from source with `cargo build --release -p acpd`.

## Limits

Not signed or notarized, no installers, no automatic update of the host (the phone app updates itself from the `debug` release; the host is replaced by downloading a new release). The workflow has not run until the first tag or manual dispatch; a first run may need small fixes.
