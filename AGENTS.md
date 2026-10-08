# ACP Portal contributor context

## Read first

Periodically move completed TODO.md checklist items and their verification notes into TODO_DONE.md, preferably at each finished increment or handoff. Preserve evidence and limits, leave unfinished follow-ups in TODO.md and keep documentation links current. Archive only work actually completed and verified to its stated scope.

This is an existing Android client and Rust host for coding-agent CLIs. Continue the implementation in place. Do not scaffold a replacement or treat the current app as production complete.

Read `TODO.md` for remaining/suspended items and `TODO_DONE.md` for completed work and chronological verification, then `docs/requirements-audit.md` for the original 39-section scope and its evidence. `docs/screen-acceptance.md` records scoped screen checks. `README.md`, `docs/architecture.md`, `docs/protocol.md`, `docs/android.md` and `docs/security.md` explain the boundaries.

The user requested completion of the Connection logs increment and a documentation handoff on 7 October 2026. That handoff is complete, and the user subsequently resumed coding. Read `HANDOFF.md` for the baseline, TODO for remaining work and TODO_DONE for completed progress. Keep TODO, TODO_DONE, README and AGENTS current and distinguish implemented behavior from verified acceptance.

## User decisions

- Scope update on 8 October 2026: tablet/landscape acceptance is excluded from current work. Physical-phone testing is suspended and user-owned. Signed APK production/verification is suspended until requested. Verify minimum Rust, Linux/container and reproducible Windows builds through GitHub Actions, not local supported-build runs. This supersedes older remaining-work statements; historical evidence is unchanged. The user authorized implementation to resume on 8 October 2026.

- Local agent is Goose ACP, invoked as `goose acp`. Agent executable/arguments/environment belong in the host registry, not Android releases.
- Phone connections use authenticated REST and WebSocket. The host communicates with agent processes using ACP stdio. Do not add another remote-shell transport or associated connection settings.
- Use the supplied UI mockups rather than the original visual direction. Their recorded guide is `docs/ui-reference.md`.
- The original images were supplied at `C:\Users\basin\Downloads\Page 1`. That folder was absent at the latest check. Do not claim a fresh image comparison without finding and inspecting the images.
- The original pasted request was at `C:\Users\basin\.codex\attachments\0be47343-c011-4049-8f3f-0276730c574d\Pasted text.txt`. The audit preserves its requirement map and later scope decisions.

## Where code lives

| Path | Purpose |
| --- | --- |
| `acpd/src/` | Rust host, CLI, registry, subprocess lifecycle, authentication, API, filesystem and terminal callbacks |
| `acpd/tests/` | Executable host and lifecycle integration tests |
| `protocol/` | Rust ACP protocol boundary and upstream schema exports |
| `examples/` | Goose registry and host configuration |
| `scripts/` | PowerShell development and mock-host entry points |
| `android/universal-acp/app/` | Compose Android application and instrumented tests |
| `android/universal-acp/core/protocol/` | Independently tested JVM protocol models, reducer, content and diff logic |
| `docs/` | Design decisions, security boundaries, UI guide and acceptance evidence |
| `.local/` | Local runtime fixtures, logs, screenshots and process markers; not authoritative proof a process is alive |

Android packages under `app/src/main/java/dev/acportal/`:

- `presentation`: `PortalNavigation.kt`, `PortalViewModel.kt`, page composables and shared theme.
- `data`: `PortalRepository.kt`, `HostApi.kt`, MCP definitions, attachments and live activity.
- `storage`: Room database/entities/migrations and DataStore settings.
- `security`: `CredentialVault.kt`, Keystore encryption and ciphertext storage.
- `transport`: WebSocket lifecycle, replay and diagnostics.

App JVM tests are in `app/src/test/java/`; device tests are in `app/src/androidTest/java/`. Many device tests use in-memory Room and isolated settings. Tests that call `assumeTrue` require fixtures; a skipped test is not acceptance evidence.

## Architecture and state ownership

The phone owns presentation and local saved copies. The host owns execution, workspace validation, agent configuration and live processes. Agent capabilities, modes, models, authentication choices and configuration options come from discovery/ACP responses. Do not branch Android behavior on agent names or invent capability data to match a mockup.

`PortalApplication.kt` constructs the application-scoped repository, IO coroutine scope, database, vault, settings and API. Repository-owned live sessions can outlive a screen or ViewModel. `PortalViewModel.kt` exposes Room flows, saved UI preferences, live activities and transient page state. Keep execution and persistence out of composables.

| State | Owner and persistence |
| --- | --- |
| Hosts, sessions, drafts, archive flags, recents, agent catalog, main-page/filter preferences | Room via `PortalDao` and `PortalDatabase` |
| Theme, message-cache preference, MCP aliases, workspace policies | `SettingsStore` backed by DataStore |
| Device tokens and MCP definitions | AES-GCM ciphertext under the vault, keyed by Android Keystore; Room/DataStore hold aliases |
| Live transport, attachments, pending submissions, activity and per-session jobs | `LiveSession` / `PortalRepository`, in memory |
| Page loading/error, host details, MCP read results, access read results | `PortalUiState`, transient |
| Unsaved MCP editor fields, including secrets | Host-keyed `McpEditState` / `McpDraft` in `PortalViewModel`; memory only, retained during Activity recreation |
| Folder browse request, path, loading/error/results | Separate `WorkspaceBrowseState`, cancellable job and generation guard |
| ACP updates, tools, thought/text blocks, permissions and replay reduction | JVM `SessionReducer` and `SessionState` |
| Agent subprocesses, controller lease and retained delivery journal | Rust host, in memory |
| Interrupted host session metadata after daemon restart | Protected `session_catalog.rs` storage; no restored running process |

Host session UUID and the agent's ACP session ID are different identifiers. Android storage and API routing use the host/session identity; explicit loading uses the ACP ID only when advertised. Preserve host scoping when indexing sessions, settings and activity.

Main-page and Sessions archive/active/search preferences are durable. Action routes, confirmations, pairing codes and unsent attachment payloads are excluded. Scroll positions and expanded activity do not yet have complete lifecycle acceptance. MCP drafts are retained in the page ViewModel during Activity recreation, keyed by host, and deliberately excluded from saved instance state. Back, successful Save/Remove and forgetting a connection clear the draft. Do not serialize secrets to restore unsaved edits after process death.

The host wrapper speaks `acpd.v1`; agent negotiation uses ACP wire version 1. The pinned schema package version is a separate version. Preserve unknown ACP payload fields and visible fallback content when extending known models.

## Navigation and feature entry points

`PortalNavigation.kt` uses Navigation 3 keys and a back stack. Main pages are Connections, Sessions and Settings. The rail currently starts at 700 dp width; compact main pages use bottom navigation. Detail/action routes hide main navigation. Do not assume all routes are persisted because their keys are serializable.

| Task | Start here |
| --- | --- |
| Pairing and host management | `HostScreens.kt`, `HostApi.kt`, repository pairing/refresh methods; `PairRoute` / `PairHostRoute` |
| Agent discovery/details and cached catalog | `AgentScreens.kt`, `AgentRoute`, repository details/catalog persistence |
| New session, recent choice and folder browser | `HostScreens.kt`, `NewSessionRoute`, `PortalViewModel.browse/cancelBrowse`, workspace filtering helpers |
| Sessions list, filters and live status | `HostScreens.kt`, `SessionActivityTime.kt`, repository `liveActivities`, `StoredUiState` |
| Live conversation and composer | `SessionScreens.kt`, `AttachmentControls.kt`, `PromptHistorySheet.kt`, repository prompt/attachment methods |
| Saved/interrupted conversation and recovery | `SessionRecovery.kt`, `SavedSessionRoute`, `SessionRoute`, `SessionRecoveryRepositoryTest` |
| MCP configuration | `McpScreens.kt`, `McpDefinition.kt`, repository `mcpServers/saveMcpServers`, `McpRoute` |
| Workspace policy | `WorkspaceAccessScreens.kt`, `WorkspaceAccessRoute`, repository workspace/session access methods |
| Diff review | `ChangesScreen.kt`, JVM `Diff.kt`, permission/tool entry points |
| Content viewing/export | `ReceivedContentView.kt`, JVM received-content/transcript helpers and app media/export code |
| Session details and connection logs | `SessionDetailScreens.kt`, `ConnectionScreens.kt`, transport diagnostics |
| Colors/type/system bars | `Theme.kt`, `MainActivity.kt` |

Settings routes choose a connection before reading MCP, agents or workspace access. New session can preselect an agent or recent workspace but must still validate with the host. Saved agent catalogs are dated, read-only discovery evidence and cannot enable launch until fresh discovery succeeds. Interrupted session history opens without execution; Resume creates a new host record through explicit loading.

## Rust host entry points

`main.rs` contains operator commands including start/status/agents/sessions/pair/doctor/config, probe/chat and device revocation. `registry.rs` merges built-ins and configured overrides by ID; discovery resolves configured executables rather than running arbitrary PATH entries. `connection.rs` owns the ACP process actor, JSON-RPC correlation, deadlines, frame reading, callback brokerage and process exit. `session.rs` owns creation/loading, replay and session lifecycle. `api.rs` exposes authenticated management and WebSocket routing.

`filesystem.rs` enforces workspace capability boundaries and consent for writes. `terminal.rs` implements ACP terminal callbacks and bounded output. `process_tree.rs` handles platform process lifetime controls. `security.rs` owns pairing/tokens; `session_catalog.rs` owns restart metadata; `mcp.rs` validates server setup against agent capabilities. Inspect the corresponding executable tests before changing these boundaries.

`examples/config.toml` currently uses loopback port 8765 and paths relative to the config file. The separate mock fixture uses 8767. Empty allowed roots deny all workspaces. Do not widen roots or copy provider credentials into project configuration to make a test pass.

## Keep these invariants

- Execute literal executable paths and argument arrays. Do not introduce shell interpolation for agent or terminal commands.
- Remote host listeners require TLS. Android uses trusted certificates; never bypass certificate validation. Debug cleartext is limited to local development addresses.
- Pairing codes are one-use and expire after 60 seconds. Do not print pairing codes, device tokens, provider credentials or encrypted-definition contents in logs or reports.
- Preserve user hosts, saved sessions, MCP definitions, policies and preferences when testing. Create isolated fixtures and clean up only fixture-owned resources.
- Keep secrets out of saved instance state and backups. Vault aliases identify ciphertext; a missing or unreadable file must not silently become an empty saved configuration.
- Failed MCP writes retain the previous alias/definition and editor draft. Persist a replacement before deleting the previous ciphertext. Cancellation must remain cancellation.
- Reconnect/replay must not resend prompts or decisions. Reconcile pending permissions against the authoritative host snapshot before enabling decisions.
- Restarted/interrupted sessions require explicit capability-checked loading. Opening saved history must not start an agent.
- Filesystem writes and terminal creation require reviewable consent. Preserve workspace containment, bounded content, cancellation and cleanup behavior.
- Preserve full diff/content fidelity and visible truncation/history-gap reporting. A passing small fixture does not prove large-input or lifecycle acceptance.
- Room currently uses schema 3 with additive 1→2→3 migrations. Preserve saved data and migration coverage when changing storage.

## UI work

Follow `docs/ui-reference.md`: near-black surfaces, neutral cards, white primary actions, muted secondary text, restrained green status, 24 dp gutters, compact bottom navigation and a wider navigation rail. Check both dark and light themes.

Verify small windows and large fonts with reachable controls, real keyboard behavior and touch bounds. Synthetic dimensions/font overrides test layout only; separate native keyboard/device checks are needed. Sessions search stays in one sticky header so keyboard height changes do not move its composition and lose focus. At compact height or large text, its All/Active tabs scroll below search. Compact Changes, MCP and Workspace access controls use scrolling content to remain reachable.

Connections host name/status summaries are bounded to two lines with full semantic text. Connection details preserves full values; below 420 dp or above 1.3× text, Reconnect/Disconnect scroll with the body and use minimum heights. Normal windows retain the footer. `ConnectionsPageLayoutUiTest` covers dark/light 320×280 dp, synthetic 2× long names, values/errors and action callbacks; `ConnectionScreensUiTest` covers normal controls and paused logs.

Loading, empty, failed and offline states must have useful copy and explicit recovery. Dismissing an error must not remove the page's persistent recovery action. Do not retry mutations automatically.

## Build and verification

Use the pinned versions already in Gradle/Cargo files. Do not upgrade dependencies as a side effect of unrelated work. Android currently uses AGP 9.0.1, Kotlin 2.3.20, compile/target SDK 36 and min SDK 26. Gradle runs with JDK 21 on this machine; the default JDK 25 caused compatibility issues. Source/toolchain targets remain configured in the build files.

From the repository root, Rust checks:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

From `android/universal-acp`:

```powershell
$env:JAVA_HOME='C:/Program Files/Amazon Corretto/jdk21.0.11_10'
./gradlew.bat :core:protocol:test :app:testDebugUnitTest :app:assembleDebug :app:assembleDebugAndroidTest :app:lintDebug
```

For UI-only changes, run appropriate app checks and targeted device regressions. Broaden testing when the changed boundary warrants it. Record actual results, not commands merely planned or started.

APK paths are `app/build/outputs/apk/debug/app-debug.apk` and `app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk`, relative to the Android project. On this machine adb is `C:/Users/basin/AppData/Local/Android/Sdk/platform-tools/adb.exe`.

After installing both APKs, run selected classes, for example:

```powershell
adb shell am instrument -w -r -e class 'dev.acportal.presentation.McpLoadRecoveryNavigationTest,dev.acportal.presentation.McpSaveRecoveryNavigationTest' dev.acportal.test/androidx.test.runner.AndroidJUnitRunner
```

Revalidate emulator identity and settings before use. The latest device was `emulator-5554`, API 36, physical size 720×1280 at density 320, font 1.0 and automatic rotation enabled. Restore settings changed by tests in `finally` and verify restoration. Do not wipe app/AVD data to fix a debugging connection. The latest authorization issue was recovered with a data-preserving cold boot; the user cannot access the headless emulator prompt. Process IDs from past checkpoints are not reusable evidence.

`scripts/mock-host.ps1 -RequireAuth -Terminal -Media` starts deterministic fixtures on desktop loopback port 8767; the emulator uses `http://10.0.2.2:8767`. `-Pair` requests a fresh code against that host. Check the existing listener/process before starting another. Mock configuration disables Goose/Gemini only for the fixture. Real Goose smoke checks use production registry/configuration separately.

## Test selection and fixture discipline

| Changed behavior | Relevant existing checks |
| --- | --- |
| Sessions layout, native keyboard and summary | `SessionsLayoutUiTest`, `SessionSummaryUiTest`, app `SessionActivityTimeTest` / `LiveSessionActivityTest` |
| MCP read/write recovery and editor | `McpLoadRecoveryNavigationTest`, `McpSaveRecoveryNavigationTest`, `McpLifecycleNavigationTest`, `McpServersUiTest`, `ConfigurationLayoutUiTest`, `McpStorageTest` |
| MCP encrypted cold reopen | `McpColdProcessNavigationTest`; run preparation and restoration methods in separate instrumentations with verified process absence between them |
| Workspace selection and delayed HTTP responses | `WorkspaceLaunchNavigationTest`, `WorkspaceBrowserNetworkTest`, `WorkspaceBrowserUiTest`, `NewSessionLayoutUiTest` |
| Workspace policy and unavailable reads | `WorkspaceAccessLayoutUiTest`, `WorkspaceAccessUiTest`, `WorkspaceAccessStorageTest` |
| Pairing and credential renewal | `PairingLayoutUiTest`, `PairingHostNavigationTest`, app `HostRequestErrorTest`, Rust network/security tests |
| Saved discovery and preferences | `AgentCacheRecoveryNavigationTest`, `AgentCatalogStartupTest`, catalog/UI-state migration tests, `UiStateNavigationTest` |
| Permission/replay/recovery | JVM `PermissionReplayTest`, app transport tests, `SessionRecoveryRepositoryTest`, `LiveSessionActivityHostTest`, Rust network/lifecycle tests |
| Changes rendering and full context | `ChangesLayoutUiTest`, `ChangesUiTest`, JVM `DiffTest` |
| Media, native attachments and export | `ReceivedContentUiTest`, `ReceivedMediaTest`, `AttachmentLoaderTest`, `AttachmentPickerNavigationTest`, `TranscriptExportTest` |
| Host processes, restart, filesystem and terminal | `acpd/tests/lifecycle.rs`, `network.rs`, `restart.rs`, `terminal.rs` and module tests |

Read test fixtures before running them. Some require a paired mock host; others intentionally mutate then restore default-store preferences. Prefer isolated in-memory Room, injected preference files and fixture-owned vault roots for new recovery tests. Keep a record of the production values/resources a fixture promises to preserve and assert that preservation. Cancel fixture scopes, clear ViewModel stores and close databases before deleting their files.

Test-only Activities can differ from `MainActivity` in system-bar behavior. A light screenshot from a fixture with white status icons is not proof of a production theme defect. Inspect the actual app before changing system bars. A semantic node marked displayed can still be covered by a sticky header; verify bounds and the intended callback for important action tests.

For process-death acceptance, prove the app process stopped and check restored state before network refresh can replace it. For delayed-request races, hold/release actual controlled responses and assert older results cannot overwrite newer selections. A configuration recreation and a cold process restart cover different failure modes.

Wait for each started build/instrumentation process to reach terminal output. A tool timeout or transient adb offline state is not evidence the test process stopped. Recheck the same live handle before launching a duplicate run. Installing a test APK alone does not update production code; install the app APK as well when production sources changed.

Connection logs uses wrapping minimum-height tabs. Below 420 dp or above 1.3× text, host/status/filter and pause/footer controls scroll with events; normal windows retain separate controls. `ConnectionLogsLayoutUiTest` checks compact dark/light filters, frozen events, resume and sanitized diagnostics. Copy availability is checked; the new fixture does not write the clipboard.

## Latest verified checkpoint and limits

8 October 2026 installation documentation: docs/installation.md covers private configuration/state, direct TLS PEM paths, certificate renewal requiring daemon restart, debug installation and explicit pairing/revocation. examples/agents.custom.json contains disabled native/Node templates; docs/agents.md explains built-in merge and runtime paths. Existing debug CLI parsed templates/effective example config successfully without launching agents. No new build, certificate deployment or custom adapter acceptance. User authorized implementation resumption; supported-build GitHub Actions work remains next.

8 October 2026 device repository overflow: PermissionOverflowRepositoryTest delivers 129 approvals plus late replay completion over an actual local WebSocket into isolated Room/DataStore/vault. It verifies 128 retained approvals/sequence, detached/disconnected/replaying state, blocked decisions/prompts after error dismissal, zero submissions and no automatic reconnect. Four device tests pass without skips with existing recovery guards; both APK builds/install and lint pass. Unique fixture files/resources are cleaned up. Android-test-only MockWebServer 4.12.0 matches the existing JVM version. Emulator small_phone was started headless without wiping data and verified at 720×1280/density320. No production-source changes or repeated core/JVM/Rust checks this increment. Device byte-overflow, route/UI and physical-host acceptance remain open. User requested pausing after this checkpoint.

8 October 2026 pending permissions: SessionReducer enforces 128 entries per live/replay map and 8 MiB combined conservative content estimate when maps change. PendingPermissionLimitException rejects the complete update. PortalRepository detaches with a protocol error and keeps prior consent; manually detached sessions ignore queued frames until explicit reconnect. No host approval/cancellation is sent. Two reducer tests plus final 45 core/31 app JVM tests, zero failures/skips, debug build/lint pass. Repository/device overflow integration remains unverified; no emulator install. Rust unchanged, not rerun. Model/config correlation fields, broadcasts, waiting callers, allocator profiling and wider lifecycle acceptance remain open.

8 October 2026 auxiliary retention: SessionReducer bounds changed terminal snapshots at 16 entries/2 MiB conservative content estimate, marking historyGap for count/byte eviction. SessionInfo has a separate 2 MiB estimate; oversized additive updates preserve prior accepted fields and report a protocol error. Permissions are untouched. Unchanged auxiliary references avoid rescanning ordinary text updates. Three tests pass for byte/count pressure, metadata preservation and pending consent, within final 43 core/31 app JVM tests, zero failures/skips; debug build/lint pass. No emulator/device review; Rust unchanged, not rerun. Other auxiliary fields and actual heap profiling remain open.

8 October 2026 outgoing queue checkpoint: BoundedSocketSender synchronizes admission and send, caps socket queued content at 4 MiB and validates actual decoded/re-encoded UTF-8 frames at 1 MiB. Capacity failure rejects before sending and permits explicit retry after drain. Three controlled socket tests cover queue/drain/rejection, encoding expansion and concurrent admission. Final 31 app JVM tests, zero failures/skips, debug build/lint pass. Existing repository send-failure paths reset prompt busy state and retain pending decisions. No emulator/physical slow-network acceptance; JVM socket tests are controlled fixtures. Rust/core unchanged, not rerun. Auxiliary-state and total-heap acceptance remain open.

8 October 2026 Android incoming queue checkpoint: IncomingMessageQueue accounts queued byte arrays under a synchronized 8 MiB/256-entry limit. Consumer delivery and undelivered callbacks release bytes; failed trySend rolls reservations back. WebSocket overflow uses existing cancel/reconnect/cursor replay and preserves accepted frames. Two queue tests and one actual MockWebServer nine-frame overflow/reconnect test pass within final 28 app JVM tests, zero failures/skips; debug build and lint pass. No emulator install/device checks. This excludes OkHttp's current frame/String, downstream parsing, outgoing buffers and whole-process heap overhead. Rust/core unchanged, not rerun. Auxiliary state and physical-network/lifecycle acceptance remain open.

8 October 2026 command admission: connection.rs validates borrowed outbound request/notification/permission envelopes through a counting serializer before enqueueing, including newline and escaping. Requests conservatively reserve a full-width u64 ID. Command queue capacity follows the frame-derived stdout budget. Final production source passes 73 Rust tests; subsequently extended oversized notification/answer lifecycle assertions pass in the targeted test, including prompt/cancel afterward. Strict Clippy and formatting pass. Waiting caller inputs, JSON allocator overhead, broadcasts and Android queues remain separate open work. Android unchanged and not rerun.

8 October 2026 reader-queue checkpoint: `connection.rs::frame_queue_capacity` bounds queued maximum-size raw frames plus one reader frame within max(8 MiB, 2 × max_frame_bytes), capped at 64 queue entries. Default queue is seven; 16 MiB frame setting queues one. Backpressure preserves frames. Boundary test and final 72 Rust tests plus strict Clippy pass. Actor-current frame and JSON allocator overhead are outside this bound. Control queue and Android transport/auxiliary budgets remain open; Android unchanged and not rerun. This does not complete memory or platform containment acceptance.

8 October 2026 request-cache checkpoint: `api.rs` charges encoded input/ID plus reserved result bytes before execution within max(8 MiB, 4 × max_frame_bytes + 4,096) per session and 256 entries. Results reserve 2 × max_frame_bytes + 1,024 per entry even after completion. Only completed entries can be evicted. Pending inputs and duplicate handling remain intact; insufficient capacity rejects before execution. Two cache tests and final 71 Rust tests, strict Clippy and formatting pass. Existing network/replay/lifecycle checks pass; dedicated socket byte-pressure and heap measurements remain open. Android unchanged, not rerun. This is not a full memory bound: JSON overhead, actor queues and Android auxiliary/transport state remain open.

8 October 2026 host diagnostics checkpoint: `doctor.rs` reports listener availability and configured runtime limits, and probes state storage with its own unique create-new file, sync and removal. Missing state paths probe their nearest existing parent without creating directories; inaccessible/non-directory ancestors fail. The CLI fails when operational checks fail. An occupied port may simply be an existing host. Two new preservation/conflict tests and the final 69-test Rust suite pass with strict Clippy on Windows. Formatting applied. The restricted run blocked existing filesystem tests; approved execution passes. Use PowerShell `cargo --% clippy --locked --workspace --all-targets -- -D warnings` if lint flags are lost. Android was unchanged and not rerun. These probes do not prove free space, TLS/provider validity or future startup availability. Bounded logging, byte budgets and process containment remain open.

MainActivity recovery checkpoint: the final external workflow passed on actual MainActivity using an owned debug repository. It confirms an unsaved editor value remains present after keyboard dismissal, then saves/backgrounds the task, verifies identity, terminates only its process and restores the same task ID in a distinct process. The MCP host route and Back through chooser/Settings return; the unsaved draft does not. Two standalone JUnit storage phases pass and verify saved fixture data/alias, unchanged production MCP aliases and hashes of production hosts, sessions and UI preferences. Cleanup removes the recorded fixture task from Recents, unregisters debug lifecycle callbacks, closes the owned store and restores the normal repository. Three normal debug regressions pass without skips: one McpLifecycleNavigationTest and two AgentDetailsLayoutUiTest. Final 25 app JVM tests, both APK builds, lint and release manifest generation pass. Release retains PortalApplication and excludes both debug fixture classes. The only production code change opens PortalApplication with a protected setter for debug subclass isolation. Core/Rust sources were unchanged and not rerun. More routes, full-session/pending-permission, physical-device and release acceptance remain open.

MCP previous-task checkpoint: scripts/verify-task-recovery.ps1 passed end to end outside instrumentation. After Settings → MCP → editor, Android saves the background task; adb verifies PID/task and background state, kills only that process and verifies absence. A distinct process restores the same task ID with framework saved state, the MCP host route and Back to chooser/Settings. The unsaved draft is discarded. Two separate JUnit storage phases pass; the final phase verifies the saved definition/alias, unchanged production aliases and owned-storage cleanup. Existing 25 app JVM tests, both APK builds, lint and release manifest generation pass. The debug fixture is absent from release and its external entry requires DUMP permission plus an owned marker. Initial harness attempts failed because fresh instrumentation replaced the task or XML lacked the assumed tab flag; the corrected external workflow passes. See docs/task-recovery.md. Full-session, MainActivity/all-route and physical-device lifecycle acceptance remain open; core/Rust and production navigation were unchanged.

Agents list compact checkpoint: saved notices scroll with compact/large-text lists; empty title/help are separately scrollable. Agent names/status use two-line summaries with full semantic text, and rows expose an Inspect agent button action. Agent keys are namespaced apart from notice keys. Dark/light 320×280 dp, synthetic 2× fixtures verify full-name semantics, two-line rendering, inspection callbacks, empty guidance, explicit Refresh, fresh rows and Back. Nine device checks pass without skips: two list, two details and five discovery regressions. Final 25 app JVM tests, both APK builds and lint pass; both APKs were installed. Fixtures use local state/callbacks and preserve saved data/device settings. Wide/TalkBack/lifecycle and live discovery acceptance remain open. Core/Rust sources were unchanged and not rerun.

Agent details compact checkpoint: below 420 dp or above 1.3× text, the saved-discovery notice and New session action scroll with full details, retaining 24 dp gutters and a minimum 52 dp launch button. Two dark/light 320×280 dp, synthetic 2× tests verify long names/executable/errors, reachable disabled/enabled launch, explicit refresh, busy-state gating and cached-status gating. Five normal discovery regressions also pass, for seven device tests without skips. Final 25 app JVM tests, debug/test APK builds and lint pass; both APKs were installed. Fixtures use local callbacks and leave saved data/device settings untouched. This does not establish live discovery, TalkBack, wide-layout or lifecycle acceptance. Core/Rust sources were unchanged and not rerun.

Connection logs accessibility/wide checkpoint: the Live updates switch has an explicit accessible name and a minimum 48 dp layout target; filters expose tab selection within a selection group. Dark/light compact 320×280 dp, 2× text checks assert selected tab role/bounds and named switch state. Dark/light 800×700 dp fixtures in an actual 800 dp wide emulator window verify pause, filtered empty state, explicit resume, sanitized errors, Copy availability and Back. Six device tests pass without skips, alongside 25 app JVM tests, both APK builds and lint. The first wide run found an undersized switch target; the corrected final run passes. Both APKs were installed. Emulator size returned to physical 720×1280, density 320, font 1.0, automatic rotation 1 and user rotation 0. Saved data was untouched. This is scoped semantics/layout evidence, not a completed TalkBack or physical-network review. Core/Rust sources were unchanged and not rerun.

Latest Connection logs increment: 25 app JVM tests, debug/test APK builds, lint and eight device tests pass without skips. The device run covers two compact logs, four compact Connections and two normal detail/log tests. Both APKs were installed on verified emulator-5554 at physical 720×1280, density 320, font 1.0. Fixtures leave display settings and saved data intact. Core/Rust sources were unchanged and not rerun. Wide, accessibility and physical-network acceptance remain open.

Latest Connections layout increment: 25 app JVM tests, debug/test APK builds, lint and ten device checks pass. Four compact dark/light list/detail fixtures, two normal detail/log checks and four summary regressions pass. This is component evidence; no live connection failure, physical-network or accessibility acceptance is implied. Fixtures leave saved data/display settings intact. Core/Rust sources were unchanged and not rerun.

On 7 October 2026, the resumed MCP increment passed the existing 25 app JVM test task, debug/test APK builds, lint and seven targeted device tests without fixture skips. It covers missing/corrupt ciphertext, explicit load recovery, an actual encrypted-write failure in a fixture-owned vault, retained prior ciphertext/editor draft, explicit successful retry and existing editor/missing-host regressions. The new preference-commit check injects an IOException inside DataStore's update transaction after transformation and before commit. It proves the old alias/ciphertext remains intact and replacement ciphertext is removed, with editor inputs preserved until explicit retry. It does not prove physical disk exhaustion or configuration/process death.

The preceding Sessions increment passed eight targeted device checks for compact dark/light actions, actual dark/light keyboard focus/clear/dismissal and summaries. Changes passed compact long-path/jump checks, complete 1,005-line context expansion/collapse and an actual 800 dp split/unified check. These are scoped results, not the entire device matrix.

The subsequent MCP long-value increment passed 25 app JVM tests, builds/lint and seven UI device checks. List names and endpoints use two-line ellipsis while preserving full semantic/editor text. Dark/light 320×280 dp, synthetic 2× fixtures verify opening, rejecting an oversized header, correcting, saving and reopening a complete definition with a 128-character name, long URL and 4,096-character header. Native-keyboard Save and existing editor regressions also pass. This does not prove encrypted long-value persistence, accessibility or lifecycle behavior.

The latest MCP Activity-recreation increment passes 25 app JVM tests, builds/lint and 13 device checks. Actual Activity recreation retains the full edit in the same PortalViewModel, excludes the fixture secret from serialized saved-state bytes, preserves the previous encrypted data until explicit Save and clears the draft after saving. Existing layout, keyboard, editor and read/write recovery checks also pass. `McpLifecycleFixtureActivity` exists only under debug sources, is non-exported and is absent from the generated release manifest. Its static repository/state capture is test infrastructure; never move it into release code or use it for production ownership. Cold-process acceptance remains open.

The subsequent cold-reopen increment passes test APK build/lint and two separate device phases. `McpColdProcessNavigationTest#prepareSavedDefinitionAndLeaveDraftUnsaved` writes only to `cacheDir/mcp-cold-process-fixture`, containing its own persistent Room/DataStore/vault. After that instrumentation, adb verified the app process was absent. `#restoreSavedDefinitionAfterVerifiedProcessStop` asserts a distinct PID, unchanged alias/ciphertext digest, no retained draft and full long values in the editor, then removes the fixture root. Run each method alone; running the whole class in one process fails the fresh-PID requirement. Preparation closes the Activity/storage before process exit, so this does not prove abrupt foreground OS kill or restored-task behavior. Never overwrite an existing fixture root; restore/clean it first. No production code changed in this increment; prior unit/core/Rust checks were not rerun.

Earlier recorded checks include 40 core protocol JVM tests and 67 Rust tests with formatting/strict Clippy. Those components were unchanged in the recent UI/storage increments and were not rerun for them. Historical test counts in TODO_DONE are chronological, not one combined current suite run. Revalidate affected sources and results for new work.

Timeline retention has a conservative 8 MiB content budget, 2,000 items and a 1 Mi-character streamed-text tail with visible history gaps. Attachment checks currently cover four attachments, 256 KiB per file, 384 KiB combined attachment JSON and 512 KiB prompt JSON. These limits do not finish auxiliary-state, transport-queue or request-cache byte budgets; inspect the current constants before changing them.

Real Goose initialization, a simple prompt and explicit session loading were smoke-tested with Goose 1.53.0. Complete real-agent write/diff/terminal/approval/rejection/cancel/resume acceptance is still open. Rust 1.96.0 was used locally; the declared minimum Rust 1.88, Unix cleanup, Linux/container and release reproducibility remain unverified.

## Remaining work

The resumed foreground-kill check uses `McpColdProcessNavigationTest#prepareVisibleDraftUntilExternalKill`. After draft/storage assertions, it writes `waitingForKill=true` and waits before teardown. Within the 60-second test deadline, verify the checkpoint PID matches `adb shell pidof dev.acportal` and Android 16's `dumpsys activity activities` reports the fixture as `topResumedActivity`. Terminate only that verified PID with `adb shell run-as dev.acportal kill -9 <PID>`. Expected terminal instrumentation output is `Process crashed`; do not count preparation as a JUnit pass. Verify absence, then run the restore method alone. This sequence succeeded and restoration passed with unchanged data and discarded draft. The restore phase removes owned files. Build/lint pass; production/unit/core/Rust checks were unchanged. Previous-task restoration remains open.

Use unchecked TODO items and the audit for the complete scope. Principal remaining areas include page/accessibility/lifecycle acceptance, full-session and production-task process death, complete real Goose tools/permissions/cancel/load workflows, physical-device trusted TLS/network/background/revocation/controller tests, auxiliary state and transport byte limits, runtime/storage/port diagnostics, process containment hardening, supported-platform reproducibility and signed release delivery.

Optional QR, biometrics, IDE fork integration and later transports are not first-release requirements. Existing emulator/mock evidence does not prove physical-device reliability, complete real-agent compatibility or production readiness.

When implementation resumes, follow `HANDOFF.md` for the suggested work order. Scoped Connection logs wide/semantics checks now pass. TalkBack review, physical networking, previous-task/back-stack restoration and the remaining page matrix are open. RegistryAgentsScreen compact saved/empty/long-name checks now pass in AgentsListLayoutUiTest. Wider/TalkBack/lifecycle acceptance remains open. AgentDetailsLayoutUiTest covers compact detail actions; AgentDiscoveryUiTest covers normal launch gating. MCP preference-commit failure, long values, Activity recreation and foreground-kill/fresh-launch recovery now have scoped evidence. Do not broaden fresh-launch recovery into restoring the previous task or component tests into physical-device/real-agent acceptance.

For operational work, inspect `main.rs`/`config.rs` for doctor diagnostics, `connection.rs`/`process_tree.rs` for the Windows launch-to-job assignment gap and detached Unix cleanup, and storage/transport code for remaining byte budgets. Release work needs a signed APK, verified install/host TLS instructions and an acceptance report tied to the original scope.

Debug task recovery now supports actual MainActivity via scripts/verify-task-recovery.ps1 -MainActivity. The debug Application redirects to owned storage only while cache/task-recovery-fixture/main-enabled exists. Normal debug runs use production storage; release keeps PortalApplication. Read docs/task-recovery.md before lifecycle tests. Do not run TaskRecoveryNavigationTest as an ordinary full class; storage setup, external workflow and verification must remain separate. Cleanup restores the normal repository and removes callbacks/owned files. A failed fixture can retain the marker for diagnosis; remove only that marker and restart to return to normal debug storage.

## Working and handoff conventions

Git is initialized on `main`. The user requested the private GitHub repository `0xbasinas/acportal` and repository-local identity `0xBasinas <241305723+0xbasinas@users.noreply.github.com>`. The email is GitHub's account-ID no-reply address because the available authentication cannot read private account email. Use the `0xbasinas` account for this remote and preserve its private visibility. Do not change global Git identity or publish this repository publicly. Verify the current remote/account before future pushes.

Use `rg` for scoped source searches and exclude generated build/target output. Inspect current files and runtime state rather than assuming old checkpoints describe live processes. Local environment paths above are convenience notes, not portable configuration requirements. Do not commit generated APKs, local ciphertext, test credentials or runtime fixture state.

Prefer targeted fixes and the existing architecture. Avoid agent-specific UI branches, dependency upgrades or speculative module migrations. Preserve cancellation through coroutine error handlers and keep friendly errors free of internal credential paths. No approval is needed for ordinary authorized, reversible implementation work; pause/resume and external publishing follow the user's instructions.

For a handoff, record what changed, which checks actually passed or skipped, resources/settings restored, what remains unverified and the next useful action. Do not declare the overall project complete until every required deliverable and acceptance gate in the audit has evidence. Documentation-only requests while implementation is paused do not authorize resuming the broader build task.

At handoff, keep `TODO.md` focused on remaining/suspended items and next actions. Move completed checklist items and exact chronological verification evidence to `TODO_DONE.md`. Update screen/audit evidence when its claims change. Keep final updates short and link relevant files.
