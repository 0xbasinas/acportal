# Development

## Toolchain and commands

Use Rust 1.88+ and commit Cargo.lock. The local Windows verification environment has Rust 1.96.0 and Goose 1.53.0. Android uses SDK 36, JDK 17, AGP 9.0.1, Kotlin 2.3.20 and the checked-in Gradle 9.1 wrapper.

```text
cargo fetch --locked
cargo build --locked --workspace --bins
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

Use PowerShell 7 for `scripts/dev.ps1`. Native equivalent commands are in the README. The mock task writes its temporary absolute executable registry under ignored `.local/`, then starts the normal local client with `--agent mock`. Its numbered permission choice is interactive, exactly like a real agent's. Select allow to see simulated execution and a proposed diff. No file is edited.

The optional Docker build is `docker build -f docker/Dockerfile -t acpd .`. Supply your agent runtime and config/workspace mounts to use it, then pass `--config /mounted/config.toml start`. The image defaults to help; networking requires explicit listener, TLS and mounted state configuration. Docker has not been tested in this environment.

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

Linux and Docker have not been verified. The declared Rust 1.88 minimum has not been exercised locally. Advanced UI/accessibility checks, physical-phone TLS deployment and network switching remain separate acceptance work.

## Device-to-host mock test

On Windows, start `./scripts/mock-host.ps1` in one terminal. It builds the mock and starts a loopback host on port 8767 with this repository as its only workspace. Build/install the debug APK and Android test APK on an emulator. Then generate a fresh code with `./scripts/mock-host.ps1 -Pair` and immediately run:

```text
adb shell am instrument -w -r -e class dev.acportal.presentation.HostLifecycleTest -e pairingCode YOUR-CODE dev.acportal.test/androidx.test.runner.AndroidJUnitRunner
```

The test uses the emulator host address `http://10.0.2.2:8767`. Without a `pairingCode` argument it is skipped. The default mock does not edit files or execute commands. The test saves its screenshot in the target app's external files `screenshots` directory; page tests save Hosts, dark Settings and dark pairing screenshots there too.

To exercise agent authentication, start `./scripts/mock-host.ps1 -RequireAuth`, generate a new pairing code and add `-e expectAuthentication true` to the instrumentation command. This checks disabled prompt submission before sign-in, the agent-provided method, retained draft and transition to a working session.

To exercise native terminal callbacks, add `-Terminal` to the mock host and `-e expectTerminal true` to instrumentation. This launches the mock's own fixture executable only after the phone selects Run once. The fixture prints known output, exits and is released; the phone then expands its retained output and exit status. It does not modify workspace files. Both switches can be used together.
