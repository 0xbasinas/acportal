# Updating the app from GitHub releases

Settings > **Update app** installs newer builds of the app without a computer or `adb`.

## How it works

1. The **Build APK** workflow publishes every build from `main` as one pre-release with the static tag `debug`. The release name is `Debug build 0.1.0-debug.<run>+<sha>`, and its single asset is `acportal-debug-<run>-<sha>.apk`. The run number is also the app's `versionCode`, so builds are ordered.
2. The screen reads `https://api.github.com/repos/0xbasinas/acportal/releases/tags/debug` (fixed in `UpdateSource`; the app installs from no other address), compares the build number with the installed `versionCode`, and offers an update only for a strictly higher build.
3. **Download and install** downloads the APK into the app's cache. The download is refused unless the final address is `https` on a GitHub host (after redirects), the size matches the release, and its SHA-256 matches the `digest` GitHub publishes for the asset (when a release has no digest the screen says so). A failed or cancelled download leaves no file behind.
4. The APK is handed to Android's installer. Android verifies the signature (an update must be signed with the same key as the installed app), shows its own **Update this app?** confirmation, and may show a Play Protect prompt. Your data is kept. Nothing is installed without that confirmation.

Nothing is checked, downloaded or installed until you open the screen and press the button; there is no background updating.

## First-time setup on a phone

- Android asks once to allow ACP Portal to install apps ("Install unknown apps"). The screen explains this and opens the right Settings page; return to the app and press **Download and install** again.
- Debug builds from CI are signed with the shared debug key stored in the repository secrets (`DEBUG_KEYSTORE_BASE64`, `DEBUG_KEYSTORE_PASSWORD`). A build installed from your own computer uses a different key, so Android refuses to update it: uninstall once (saved connections are removed) and install a CI build. After that, every update installs over the previous one.
- If the signature does not match, the screen says so in plain words and keeps the **Try again** button.

## Limits

- Only debug builds exist; signed release delivery is not set up.
- The check needs the public GitHub API (60 unauthenticated requests per hour per address).
- Verified on the emulator through the real release: check, permission step, a 22 MB download with checksum verification, Android's confirmation and Play Protect prompt, and a declined install reported as a failure. Not verified: a completed update (the emulator's installed app was signed with a different key), or any physical phone.
