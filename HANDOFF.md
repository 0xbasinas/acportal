# ACP Portal agent handoff

7 October 2026. The user requested completion of the current Connection logs work, current README/TODO/AGENTS documentation and a handoff for the agent continuing the remaining implementation. This checkpoint finishes that request. Implementation is stopped; start the next increment when the user authorizes continuation.

## Start here

Read [AGENTS.md](AGENTS.md), [TODO.md](TODO.md), [requirements audit](docs/requirements-audit.md) and [screen acceptance](docs/screen-acceptance.md). The audit maps all 39 sections of the original request; this handoff is a work order, not a replacement scope. Consult [architecture](docs/architecture.md), [protocol](docs/protocol.md), [Android](docs/android.md) and [security](docs/security.md) for implementation boundaries.

Continue this repository in place. Local agents run through the Rust host, with Goose configured as `goose acp`. Android connects using authenticated REST/WebSocket; agent processes use ACP stdio. Executables, arguments and environment belong in the host registry. Preserve discovery-driven capabilities and unknown ACP payloads. Use the recorded [UI guide](docs/ui-reference.md); the original Downloads mockup folder was absent, so no fresh image comparison has been performed.

## Completed checkpoint

`ConnectionScreens.kt` now wraps Connection logs filters with minimum touch heights. Below 420 dp height or above 1.3× font scale, host/status/filter and Live updates/footer controls scroll with the event list. Normal windows retain separate controls. Host labels use bounded summaries with full semantics. Existing event sanitization and paused-snapshot behavior remain intact.

`ConnectionLogsLayoutUiTest` adds dark/light 320×280 dp, synthetic 2× text checks for Warnings/Errors/All filters, frozen events, explicit resume, sanitized failure copy and Copy availability. These tests do not write the clipboard. Earlier Connections work bounds long host summaries and puts detail recovery actions in scrolling content for compact layouts while preserving full addresses/errors.

The final run completed successfully:

| Check | Result |
| --- | --- |
| `:app:testDebugUnitTest` | 25 tests passed |
| `:app:assembleDebug`, `:app:assembleDebugAndroidTest` | Both APKs built and installed |
| `:app:lintDebug` | Passed |
| `ConnectionLogsLayoutUiTest` | 2 device tests passed |
| `ConnectionsPageLayoutUiTest` | 4 device tests passed |
| `ConnectionScreensUiTest` | 2 device tests passed |

No device tests were skipped. Emulator identity was verified as emulator-5554, physical 720×1280, density 320, font 1.0. The compact fixtures use synthetic dimensions/font scale and do not change device settings or saved profiles. Eight device checks establish component behavior, not trusted-TLS, physical-network, accessibility or wide-screen acceptance. Core protocol and Rust sources were unchanged; earlier 40 JVM and 67 Rust results were not rerun for this increment.

## Earlier work to preserve

MCP storage failures retain prior encrypted data and editor drafts. Missing/corrupt ciphertext produces explicit load recovery. Replacement ciphertext is removed if the DataStore alias commit fails. Saving only deletes the prior ciphertext after successful persistence. Cancellation remains cancellation.

Host-keyed MCP drafts live in PortalViewModel memory. Actual Activity recreation retains full unsaved fields, with fixture secrets excluded from serialized saved state. Cold-reopen and abrupt foreground-kill/fresh-launch checks verify distinct processes, unchanged saved alias/ciphertext, complete long definitions and discarded unsaved drafts. They do not prove restoration of the previous task/back stack. Do not serialize secrets to close that gap.

`McpColdProcessNavigationTest` has separate preparation/restoration methods and a waiting foreground-kill preparation. Do not run the whole class blindly: the waiting method intentionally awaits external termination, and restoration needs a distinct process. Follow AGENTS for verified PID/foreground checks. Preparation's expected instrumentation crash is not a passing JUnit test.

Sessions keyboard focus/filter tests, Changes full-context/wide tests, controlled HTTP workspace-browser races and paired mock-host flows have scoped evidence in TODO. Preserve isolated storage, explicit mutation retries, authoritative permission reconciliation and fixture cleanup.

## Suggested continuation order

1. Close the Connection logs wide-layout and accessibility gaps, then work through the remaining page matrix. Define actual user flows first: reachable touch targets, large fonts, TalkBack semantics, loading/empty/error/offline recovery, native keyboard and compact/wide dark/light controls. Avoid adding tests that merely restate implementation.
2. Verify previous-task/back-stack restoration after process death and full-session recovery. Prove the process stopped, inspect restored state before network refresh replaces it, and verify opening saved history never starts execution. Check pending permission reconciliation before decisions become enabled.
3. Run complete real Goose write/diff/terminal approval, rejection, cancellation and explicit load workflows. Existing Goose 1.53.0 evidence covers initialization, a simple prompt and session loading only. Use isolated workspaces and preserve provider configuration.
4. Cover a physical device with trusted TLS, network loss/return, background/lock, token expiry/revocation and competing controller leases. Emulator mock-host behavior does not close these gates.
5. Finish auxiliary state, transport queue and request-cache byte budgets; profile attachment serialization and large replay/content flows. Current timeline/attachment limits are recorded in AGENTS and source constants.
6. Improve runtime/storage/port diagnostics and process containment. Inspect `main.rs`/`config.rs`, the Windows launch-to-job assignment gap in `connection.rs`/`process_tree.rs`, detached Unix cleanup and filesystem write containment/concurrency.
7. Verify declared Rust 1.88 support, Linux/container behavior and reproducible builds. Prepare a signed APK, trusted-TLS install instructions and an acceptance report tied to the original audit. Do not declare production completion before every required gate has evidence.

This order is a recommendation. The unchecked TODO and requirements audit define the remaining scope. Optional QR, biometrics, IDE fork integration and later transports are outside first-release requirements.

## Verification and fixture operation

Use pinned dependencies. Android Gradle needs JDK 21 here:

```powershell
# From android/universal-acp
$env:JAVA_HOME='C:/Program Files/Amazon Corretto/jdk21.0.11_10'
./gradlew.bat :core:protocol:test :app:testDebugUnitTest :app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug
```

Select affected checks rather than repeatedly rerunning green unrelated suites. Rust boundary changes require the repository-root checks:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

adb is `C:/Users/basin/AppData/Local/Android/Sdk/platform-tools/adb.exe`. Install both latest APKs before testing changed production sources. APKs are under `android/universal-acp/app/build/outputs/apk/debug/` and `androidTest/debug/`. The last selected device command was:

```powershell
adb shell am instrument -w -r -e class 'dev.acportal.presentation.ConnectionLogsLayoutUiTest,dev.acportal.presentation.ConnectionsPageLayoutUiTest,dev.acportal.presentation.ConnectionScreensUiTest' dev.acportal.test/androidx.test.runner.AndroidJUnitRunner
```

Recheck emulator identity/settings before use; historical PIDs and `.local` markers are not liveness evidence. All build/instrumentation handles for this increment reached terminal output. Do not duplicate a live run after a timeout. The user cannot access the headless debugging prompt; preserve app/AVD data when recovering authorization.

The deterministic host uses `scripts/mock-host.ps1 -RequireAuth -Terminal -Media`, desktop loopback 8767 and emulator `http://10.0.2.2:8767`. Check the listener before starting another. Production example configuration uses 8765. Keep fixture registry/configuration separate from Goose production configuration. Never widen allowed roots or copy credentials to make a test pass.

## Git and delivery state

Git is initialized on `main`; origin is the private `https://github.com/0xbasinas/acportal.git`. Initial published commit is `aa7dfb1fcd25098210697d43c819c920e2b6c12f`. Repository-local identity is `0xBasinas <241305723+0xbasinas@users.noreply.github.com>`. Later implementation/documentation changes remain local and uncommitted at handoff; inspect the working tree and preserve them. No commit or push was performed for this handoff.

Verify remote/account/private visibility before future pushes. Preserve local identity, ignore rules and executable Gradle wrapper mode. Never commit generated APKs, runtime fixtures, ciphertext, tokens, provider secrets or signing keys. No PR or release has been created.

For each next increment, update TODO with the actual behavior and terminal verification results, update README/AGENTS when the checkpoint or entry points change, and amend screen/audit evidence when acceptance changes. Record skipped tests, restored resources and remaining limits explicitly.
