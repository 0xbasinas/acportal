# Development

## Toolchain and commands

Use Rust 1.88+ and commit Cargo.lock. UUID is pinned to 1.26.1 because 1.27 requires Rust 1.89. The local Windows verification environment has Rust 1.96.0 and Goose 1.53.0. Android uses SDK 36, AGP 9.0.1, Kotlin 2.3.20 and the checked-in Gradle 9.1 wrapper. Run Gradle with JDK 21 on this Windows machine; the configured source/toolchain targets remain in the build files.

```text
cargo fetch --locked
cargo build --locked --workspace --bins
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

Use PowerShell 7 for `scripts/dev.ps1`. Native equivalent commands are in the README. The mock task writes its temporary absolute executable registry under ignored `.local/`, then starts the normal local client with `--agent mock`. Its numbered permission choice is interactive, exactly like a real agent's. Select allow to see simulated execution and a proposed diff. No file is edited.

The Docker recipe is `docker/Dockerfile`. Verify its build through GitHub Actions. Supply your agent runtime and config/workspace mounts to use it, then pass `--config /mounted/config.toml start`. The image defaults to help; networking requires explicit listener, TLS and mounted state configuration. Actions build/help/version/UID checks do not establish mounted agent execution or TLS deployment.

## Supported-build checks on GitHub Actions

`.github/workflows/rust.yml` runs stable formatting/Clippy/tests on Ubuntu 24.04 and Windows Server 2022, locked builds/tests on Rust 1.88.0 on both platforms, the Docker recipe with help/version and UID smoke checks, and two independent Windows release builds pinned to Rust 1.96.0. The Windows script uses fresh target directories, normalized source/output paths and MSVC deterministic linking, then compares SHA-256 hashes of `acpd.exe`. Actions retains toolchain, runner image, commit and hash evidence.

Run these checks through Actions, not locally. A matching hash establishes repeatability on the same runner/toolchain only; it does not establish cross-runner/toolchain, PDB or signed-artifact reproducibility. Container smoke checks do not verify mounted storage, TLS or agent execution. Inspect terminal job results for the exact commit before recording acceptance. Changing this workflow or script requires another Actions run.

## Android checks on GitHub Actions

`.github/workflows/android.yml` runs on pull requests and pushes to main. It sets up Temurin JDK 17 and 21 (Gradle runs on 21; modules use a 17 toolchain), the runner's Android SDK through `android-actions/setup-android` and Gradle caching, then runs `./gradlew :core:protocol:test :app:testDebugUnitTest :app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug` from `android/universal-acp`. Only debug variants are built; no signing secrets are used. Test and lint reports are uploaded as the `android-reports` artifact. This does not run instrumented device tests, which still need an emulator or phone.

## Test coverage

Protocol unit tests check JSON-RPC validation, unknown payload preservation, string IDs and upstream initialization deserialization. Host unit tests check invalid/duplicate registry entries, literal arguments, executable resolution, denied workspaces, config-relative paths, bounded frame parsing, history eviction and retained permissions.

Executable integration tests launch the actual mock binary through pipes. They cover negotiation, creation/load, streaming, upstream update validation, permissions, rejected option IDs, duplicate answers, denial, simulated command output/diffs, concurrent sessions, capacity limits, controller reattachment, cancellation, version mismatch, missing resume capability, crash, invalid/oversized output, timeout, concurrent request IDs, oversized caller input and duplicate prompt rejection. Dedicated filesystem callback tests write only into temporary test workspaces and check consent, changed-file rejection and cancellation.

Network tests start the actual Axum router and connect over loopback sockets. They cover bearer authentication, origin rejection, workspace browsing containment, session creation/deletion, offline permission recovery, replay, duplicate prompt IDs, session ID validation and credential revocation. Pairing tests cover expiration, failed-attempt limits, atomic one-use redemption and token digest storage.

Android JVM tests cover streamed chunks, duplicate events, tool patches, permission acknowledgments, invalid options, unknown updates, diff line numbers, URL validation, WebSocket authorization/subprotocol/cursors, credential expiry and reconnect without prompt replay. Device tests cover page navigation and the explicit permission choices. Run `:app:connectedDebugAndroidTest` with an attached emulator or phone.

## Local verification record

On 7 October, the agent availability correction passed 67 Rust tests, formatting and strict Clippy. New regressions verify that interrupted/exited records cannot imply a live process, configured failures retain priority, waiting sign-in is reported accurately and a usable live process takes precedence over another session's sign-in requirement. Existing authenticated network tests now check availability after actual daemon restart and before/after ACP sign-in.

Verified on Windows on October 6, 2026:

- Workspace and both binaries compile with Rust 1.96.0.
- 57 Rust unit/integration tests pass, including all five terminal callbacks, explicit command consent, reconnect without duplicate execution, retained output after release, responsive exit waiting and quiet commands beyond the prompt inactivity deadline. Tests also cover UTF-8/byte limits, literal arguments, scoped handles, descendant cleanup, authentication before new/load, filesystem containment and permissions, and protected restart catalog recovery/limits.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` passes.
- Installed Goose 1.53.0 negotiated ACP v1 and advertised session loading, image/embedded context, and MCP HTTP support.
- A real Goose session streamed `ACP connection verified.` and returned `end_turn` for a prompt requesting no tools or file operations.
- A new Goose process loaded that existing session through `session/load`, streamed `ACP session resumed.`, and returned `end_turn`.
- After workspace-scoped read support was added, a live Goose ACP session negotiated successfully, returned `ACP filesystem negotiation verified.` and ended with `end_turn` without tools or file modifications.
- With both filesystem capabilities enabled, a live Goose ACP session returned `ACP write capability negotiated.` and ended with `end_turn` without tools or file modifications.
- Android debug APK builds successfully.
- Eleven protocol JVM tests and four Android transport JVM tests pass.
- Android debug lint passes with zero errors; remaining warnings include dependency catalog and translation recommendations.
- The small-phone Android 16 emulator completed the real Android → loopback acpd → mock agent flow: pairing, agent/workspace selection, new session, streamed prompt, explicit permission approval, tool results and expanded diff.
- The same full-device flow passes with agent authentication required. Prompts stay disabled until the advertised mock sign-in succeeds, and a draft survives that transition.
- Both device navigation/permission tests pass with a populated session cache as well as the earlier empty-state run. Dark Settings and pairing screens and the live diff screen were captured for visual inspection.
- Six dedicated permission/sign-in/output/recovery page device tests pass, including the host source label, proposed-diff review, advertised approval options, rejecting terminal-based sign-in choices, terminal truncation/exit status and capability-driven resume/new-session actions.
- The full-device flow also passes with authentication and native terminal callbacks enabled together. It checks explicit Run once consent, retained output after release, exit code 0 and the expanded diff. The captured native terminal screen was inspected visually.
- A real Goose ACP session completed a no-tools prompt with `end_turn` after managed terminal support was enabled. Its text response did not confirm protocol details; capability validation comes from the initialization and callback tests.
- The Android app paired and completed a prompt, then the actual daemon was stopped and restarted with the same state. The interrupted session page offered explicit resume; loading and a new prompt completed through the retained device credential.
- After the user's 16 mockups were adopted, the redesigned navigation test and full authentication/native-terminal lifecycle passed. Updated selection, Settings, permission and conversation screenshots were captured for inspection. Remaining detail-page work is tracked in `ui-reference.md`.
- Three dedicated detail-page tests pass for grouped configuration routing, missing capabilities and disabled disconnected controls. The full authenticated lifecycle also opens Agent details and Session options and returns to the connected conversation. Eleven reducer tests include complete configuration replacement, acknowledged model selection and a configuration error during an unrelated prompt.

The first integration run exposed a race in mock cancellation: a cancelled permission could complete the turn before the cancellation notification arrived. The mock now returns a cancelled prompt result for that outcome. The final suite includes this regression.

The October 6 record above did not verify Linux, Docker or Rust 1.88. Later Actions results are recorded below and in TODO_DONE. Advanced UI/accessibility checks, physical-phone TLS deployment and network switching remain separate acceptance work.

8 October 2026 Actions run [37756534596](https://github.com/0xbasinas/acportal/actions/runs/37756534596), source commit `4fe4f8bbcb14c2570f4c814f376564ac7775981b`: stable formatting/strict Clippy/tests and Rust 1.88 locked builds/tests pass on Ubuntu 24.04 and Windows Server 2022. Each Linux suite passes 70 tests; each Windows suite passes 73, with zero failures/ignored tests. Docker builds and help/version/UID 10001 checks pass. The preceding run exposed UUID 1.27's Rust 1.89 minimum; the corrected run pins 1.26.1. No supported-build check ran locally. Windows hash-comparison evidence is recorded separately after its terminal result.

The same run finishes successfully with matching Windows release hashes `785EC5A7FC92B2646D69FB5162C3BF012C9C255AAC1F655CB39ADE48D570B016`. The inspected artifact identifies Rust 1.96.0 and Windows runner image `20261004.326.1`. This is two clean target directories on one runner/source checkout, not independent-machine reproducibility. PDBs, signed outputs and mounted container agent/TLS workflows remain outside this evidence.

## Device-to-host mock test

On Windows, start `./scripts/mock-host.ps1` in one terminal. It builds the mock and starts a loopback host on port 8767 with this repository as its only workspace. Build/install the debug APK and Android test APK on an emulator. Then generate a fresh code with `./scripts/mock-host.ps1 -Pair` and immediately run:

```text
adb shell am instrument -w -r -e class dev.acportal.presentation.HostLifecycleTest -e pairingCode YOUR-CODE dev.acportal.test/androidx.test.runner.AndroidJUnitRunner
```

The test uses the emulator host address `http://10.0.2.2:8767`. Without a `pairingCode` argument it is skipped. The default mock does not edit files or execute commands. The test saves its screenshot in the target app's external files `screenshots` directory; page tests save Hosts, dark Settings and dark pairing screenshots there too.

To exercise agent authentication, start `./scripts/mock-host.ps1 -RequireAuth`, generate a new pairing code and add `-e expectAuthentication true` to the instrumentation command. This checks disabled prompt submission before sign-in, the agent-provided method, retained draft and transition to a working session.

To exercise native terminal callbacks, add `-Terminal` to the mock host and `-e expectTerminal true` to instrumentation. This launches the mock's own fixture executable only after the phone selects Run once. The fixture prints known output, exits and is released; the phone then expands its retained output and exit status. It does not modify workspace files. Both switches can be used together.
