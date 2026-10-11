# Final acceptance continuation

11 October 2026, branch `codex/final-acceptance`, starting from merged PR #15 main `8f4741b`. This report records new scoped results. It does not replace the earlier acceptance reports or establish production completion.

Published as draft [PR #16](https://github.com/0xbasinas/acportal/pull/16). The draft PR event skips the Android job by design. The full [manual Android Actions run](https://github.com/0xbasinas/acportal/actions/runs/38133735509) completed successfully at implementation head `afd99d6`. That result predates the write/page-state fixture increment below. Rust production sources are unchanged and were not locally rerun.

The later full [Android Actions run 38135646596](https://github.com/0xbasinas/acportal/actions/runs/38135646596) also passes at `b135c9f`, including JVM tests, ordinary debug APK, instrumented-test APK and lint. This compiles the new native fixtures; Actions does not execute their emulator tests. Subsequent documentation changes do not alter the tested sources.

## Recovery and upgrade

Actual external MainActivity process-death checks pass for:

- Message-cache settings: task 2675, distinct restored process, restored dialog and return to Settings without changing the preference.
- Folder browser failure: task 2676, restored selected path/error, fresh capability read, exactly one explicit Retry and return to the connection, with zero host mutations.
- Prompt history: task 2678, restored sheet and submitted-text entry before held metadata/replay was released, retained draft and return to Sessions, with zero host mutations. Both stored session drafts/history and production records are checked during cleanup.
- Offline history: task 2716, exact scroll-anchor bounds after restoration, already-expanded offscreen tool/thought content, guarded stale permissions and two-session Back navigation. Both full stored histories and drafts remain equal to preparation, and production preservation passes.

The implementation now saves the folder-picker visibility and prompt-history visibility. Only presentation booleans are saved. Restoring the folder picker reads current host capabilities through the existing browse cancellation/generation boundary. Retry remains explicit. Prompt history does not select text or send a prompt on restoration.

The first folder test preparation rejected the new scenario; the scenario allowlist was corrected. The first prompt-history run lacked the fixture's submitted-text metadata, so its displayed user message correctly did not qualify as a history entry. The fixture was corrected and the passing run used the corrected metadata. Interrupted-fixture cleanup asserted production preservation before removing owned storage. No app or AVD data was wiped.

The expanded offline check first scrolled on the fixed recovery banner; the external runner now derives gestures from the actual scrollable viewport. Its next locator expected a trailing space removed by native Markdown rendering; the visible anchor comparison now uses the rendered trimmed text. The final run proves actual process death and framework restoration with the exact anchor bounds before any new scroll. Subsequent scrolling proves both disclosures remain expanded without tapping their controls again. These were driver corrections, not production layout changes. Existing Penpot screens remain unchanged.

The populated upgrade uses actual older APK `cf8fa26`, then installs the current APK in place. Preparation retains a user prompt, agent answer, deliberately obsolete cached approval, unsent draft and a separate oversized archived row. Authenticated replay renders the current host approval in the production SessionScreen. Its Allow once action is available but never invoked. All three upgrade preparation/verification/cleanup phases pass. The actual host journal separately proves one original prompt, unchanged pending approval and zero decisions. Oversized-row guarding, archive and draft preservation also pass. This closes the populated mock-session upgrade gate; it does not claim a physical-device upgrade or agent process restoration.

## Real installed Goose form

Goose 1.53.0 uses the machine's configured router/provider and an isolated workspace/host/registry. The human explicitly authorized this batch, including one read-only `ask_preference` fixture approval and submitting colour/count. The helper validated the exact MCP namespace, empty input and advertised allow-once option before consuming the exclusive `.local/final-acceptance-elicitation-consumed` marker. That authorization must never be reused.

The actual form remains pending across host reload and Android process death. The external runner proves process absence, distinct restored process, same task, cached draft before networking, disabled cached decisions, authoritative replay and two-session navigation. Journal inspection confirms no automatic prompts or decisions during recovery.

The separate opt-in `submitTheAuthorizedOwnedGooseForm` test validates the exact colour/count schema, renders the production SessionScreen, chooses green and count 2, then invokes the production repository callback exactly once. A second exclusive marker in acceptance-package cache prevents repeating that submission. The MCP fixture receives exactly one accepted answer with string `green` and integer `2`; the unsent draft remains unchanged. The form turn completes. The fixture host shuts down and guarded cleanup removes its recorded Android rows/token. No production hosts, sessions or credentials are removed.

This closes real Android form submission/recovery for this fixture. It does not cover write notices, shell/review outcomes, every form lifecycle or every provider. Read-only shell authorization remains unused. The write authorizations and their separate outcomes are recorded below; none may be reused.

## Real Goose Android write and consent diagnostics

The specifically authorized corrected attempt passes against installed Goose 1.53.0 and the configured router. The helper validates the exact disposable absolute path and content `fixture`, without a trailing newline, before exposing the request to the independent acceptance APK. The production SessionScreen sends one advertised agent allow-once choice, then one separately validated host-filesystem consent for that same file. Its diff has no prior contents and exact new contents. The actual file is checked after the turn completes. The registry-derived own-tools warning is displayed and the unsent draft stays unchanged. Guarded Android cleanup and host shutdown pass.

Both `.local/final-acceptance-write-3-consumed` and its `-host-consent` marker exist. This authorization is consumed. Two earlier write approvals were also consumed and failed completion: the first lacked second-consent diagnostics; the second stopped before host consent because Windows canonical paths carry a `\\?\` prefix. Its file remained absent. The corrected validator strips only that canonical prefix and checks exact owned path, new content, absent previous content, source and advertised option. A separate no-agent regression passes acceptance of the canonical path and refusal of another file, traversal path, changed content and existing contents. No earlier approval was retried.

This closes the exact fresh-file write path and own-tools warning for this fixture. Normal approved host writes do not emit an unchanged/outside-write notice, so it does not close actual write-notice rendering or shell/review badge outcomes. The optional preexisting-file notice mode is implemented but has not been run. A new live approval requires fresh human authorization. Diagnostics retain exception classes, source locations and boolean outcomes only; provider request values and assertion messages are not copied to reports.

## Native page-state traversal

Two normal-font native TalkBack instrumentations pass dark and light themes, four production page scenarios each: host loading then unavailable with explicit Refresh, empty Sessions, empty Connections, and offline history without cached messages with explicit Connect. Hardware swipes traverse native accessibility focus; callbacks remain absent until the requested explicit click. The fixtures use memory-only state and non-exported debug UI, without production storage writes or new visible designs.

Initial actual font 2.0 traversal failed in the debug accessibility Activity: the saved-conversation banner, Connect action and composer were traversable, but `No messages saved` was missing. Loading/unavailable and both empty-list scenarios preceded that failure successfully in dark mode. The corresponding light run was not started. A new owned MainActivity fixture then reproduced the same failure at actual 2.0, with no native scrollable body. Initial MainActivity attempts also exposed incorrect test locators for the session row and its nested action; those were corrected before reproducing the layout failure.

The supplied Penpot MCP endpoint now connects directly, although it is absent from this session's tool inventory. Editable boards 32–35 in the existing ACP Portal file cover dark/light saved empty history and the large-text scrolled state. Prior boards remain unchanged. All new board children fit their boards, and corrected exports were visually inspected. Initial exports exposed Windows pipe character encoding; icons and subtitle separators were corrected before inspection. No connection credential is saved in project files or reports.

The production fix puts the saved recovery card and empty-history explanation in one scrollable body, preserving the header, composer, editable draft and explicit recovery action. Populated history retains its existing layout. `TalkBackMainActivityUiTest` passes two cases at normal font and separately two at actual 2.0. Native hardware traversal reaches Session actions, No messages saved and the full explanation, then returns explicitly to the interrupted route. The 2× body exposes native scroll bounds `[0,336][720,824]`; at normal font all content fits. Fixture cleanup verifies unchanged production hosts, sessions, preferences and MCP aliases, and starts no live session. Font restores to 1.0, services to null and accessibility/touch exploration to 0. The broader page/focus gate remains open.

The affected build passes 65 app JVM tests with zero failures/errors/skips, both debug APKs and lint with 0 errors/18 existing warnings. Six affected saved-session/route-restoration cases also pass. Native checks are scoped to saved empty history, not every large-font page or physical-phone behavior.

After refactoring debug fixture activation, an external SettingsCache MainActivity process-death regression also passes: same task 2810, distinct process, restored cache dialog and Back to Settings without changing preferences. Separate preparation and final preservation/cleanup phases pass. It is not run as an ordinary full test class.

## Profiling investigation

The API 36 ART DDMS handler no longer registers the old allocation-tracker commands; a legacy tracker probe could handshake but did not provide allocation data. It is not acceptance evidence. The replacement opt-in native JVMTI test passes against the actual PortalRepository, two WebSockets, 100 streamed updates per session, and 20 unchanged Sessions refreshes. Two rows decode once each. It captures sampled stack symbols plus full post-GC inventories and direct owner classes, without object values or message/credential contents.

Before deletion, the heap contains two LiveSession and two SessionState instances. Direct LiveSession references include the repository's LinkedHashMap entries, coroutine ObjectRef captures and the test's explicitly weak probes. After actual repository DELETE, the heap contains zero LiveSession and zero SessionState instances; both weak probes are cleared. Owner records also disappear. The original Java GC polling timed out; replacing polling with the profiler's synchronous JVMTI ForceGarbageCollection gives a passing inventory and weak-reference result. No production leak is inferred from the polling timeout.

The final passing report observes 94,096 VM allocations with 5,881 sampled records, sampling every 16th allocation. Object inventory tags use a separate counter. Largest sampled production sites include SessionReducer.reduceEvent line 121, byte arrays 379,928 bytes across 24 samples and strings 87,664 bytes across 22 samples; boundedJsonBytes output write line 13, byte arrays 36,864 bytes in one sample; PortalRepository event decoding line 152, strings 11,608 bytes across 266 samples; and SessionListSummaries.version line 43, byte arrays 12,336 bytes across three samples. These are sampled byte totals under instrumentation, not extrapolated allocator totals or performance timings. Framework/test allocations are included separately. Existing heap/attachment/700-model stress evidence remains separate.

This closes the requested scoped site/retained-object investigation for Sessions refresh, streaming and explicit session deletion. It is not an exhaustive heap-path proof for every route/provider or a physical-phone memory measurement. Native reports are confined to the independent acceptance package and ignored `.local/native-profile`; no heap dump is produced.

## Still open

Four acceptance areas remain in [TODO](../TODO.md): real-agent write/shell outcomes, native accessibility/page states, predictive gesture motion and real-agent macOS abrupt cleanup. Physical-phone and signed-release work remain suspended/user-owned. Recovery, populated upgrade and scoped site/retained-object profiling above are complete to their recorded limits.

## Affected checks

Ordinary debug app/test APKs and lint pass, with zero errors and 18 existing warnings. The JVM tasks are up-to-date against the existing 68 core/65 app results; they were not freshly executed in this increment. Eight selected WorkspaceBrowserNetworkTest, WorkspaceLaunchNavigationTest and RouteRecoveryUiTest cases pass. Their first run caught two folder request timing failures after moving taps into a deferred effect. The corrected version preserves immediate tap reads and uses a separate restoration guard, then all eight pass. A native profiling test import ambiguity was corrected before its successful build; the probe inventory explicitly fails on JVMTI errors rather than treating unavailable data as zero retained objects.

## Repeating the native profile

This profiler is independent of the shipped APK. Use the independent acceptance APK, not the normal app. Build the test APK with `-PacportalAcceptance=true`, then install both APKs. Do not clear either package's data. The current native binary targets only the verified x86_64 emulator, API 28+.

Copy the JDK 21 `include/jvmti.h` alone into an ignored include directory. Compile `scripts/native/android-allocation-agent.cpp` using the installed NDK's `x86_64-linux-android28-clang++.cmd`, with `-shared -fPIC -std=c++17 -O1 -g -static-libstdc++` and that include directory. The remaining JNI headers must come from the NDK, not the Windows JDK. No dependency or toolchain upgrade is required.

Push the library to an owned temporary adb path, then use `run-as dev.acportal.acceptance` to copy it to `cache/allocation-profile/libacportalalloc.so`. Remove the temporary adb copy. Run only:

```text
adb shell am instrument -w -r -e allocationSites true -e class dev.acportal.data.AllocationSitesLifecycleProfileTest dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner
```

Read `cache/allocation-profile/open.tsv`, `closed.tsv` and `summary-count.txt` through `run-as`. They contain symbols, classes and counts only. The test owns an in-memory database, private fixture vault/settings and local mock sockets, deletes its two sessions, and removes its private storage. A fresh instrumentation process is required for each attachment; never attach a second profiler to a running test process. Remove only this owned profile directory after retaining the class-only reports. Rebuild ordinary APK outputs without the acceptance property before distributing the debug APK.

## CI at 4d5d888

Manual [Android Actions 38137773010](https://github.com/0xbasinas/acportal/actions/runs/38137773010) on `codex/final-acceptance` (4d5d888, the large-font empty-history fix) passes JVM tests, debug builds and lint. The draft-PR runs were skipped by policy. No device or real-agent evidence is added by this run.

## Emulator regression rerun, 11 October 2026

On the small_phone API 36 emulator with the acceptance package at `4d5d888`: `PageStateMatrixUiTest` 24/24, `MotionInputFixtureTest` 3/3, `RouteRecoveryUiTest` 4/4 and `RenderedFrameProfileTest` 2/2 pass. Rendered streaming frames (debug emulator, 120 updates): light p50 22.8 / p95 35.6 / p99 36.3 ms; dark p50 22.8 / p95 34.1 / p99 35.4 ms. This is unchanged from the earlier ~36 ms p95. No new gate is closed: predictive-Back gesture drags, physical-phone frames, codecs/document providers and broader focus restoration remain open and need hand gestures or a real device.

## Native TalkBack: Connection logs return and scroll restoration, 11 October 2026

New `TalkBackShellUiTest` cases on emulator-5554 (API 36, 720×1280, font 1.0), run through `scripts/verify-talkback-ui.ps1` (needs PowerShell 7, `pwsh`; Windows PowerShell 5.1 lacks `ProcessStartInfo.ArgumentList`), dark and light, all four pass:

- `*TalkBackLogsReturnFocus`: traverse Connection details to "View connection logs", activate it, then Back from Connection logs. The details heading is announced and automatic focus returns to "View connection logs", the control that opened the page. Traversal and return cause no action beyond the two expected callbacks.
- `*TalkBackScrollFocusSurvivesRecreation`: TalkBack scrolls a 40-row page to row 24, the Activity is recreated, and the same row is still visible with the earlier rows scrolled away. After recreation no accessibility focus is held (reported as empty); swipe navigation still works and causes no action. Scroll offset survives, focus does not, and that is recorded rather than asserted as restored.

Limits: font 1.0 only; fixture pages (`logs-return`, `scroll`) rather than the production navigation stack; no speech audibility. Settings and the TalkBack notification state were restored by the runner. Debug fixture only; absent from release.

Repeated at actual system font 2.0 (set externally, restored to 1.0 and verified): all four cases pass in dark and light with the same results. Return focus lands on "View connection logs"; after recreation the scroll position survives and no accessibility focus is held. Same fixture-page limits as above.

## Native TalkBack on the production back stack, 11 October 2026

`TalkBackMainActivityUiTest#darkProductionBackStackFocus` and `#lightProductionBackStackFocus` (actual `MainActivity`, owned Room/settings/vault, emulator-5554 API 36, font 1.0, run through `scripts/verify-talkback-ui.ps1`) both pass. Path: Sessions → interrupted session → View saved conversation → global Back → interrupted route → global Back → Sessions. Native focus is reported after each Back, not assumed:

- Back from the saved conversation: focus lands on the interrupted route's "Back" control.
- Back from the interrupted route: focus lands on the "Sessions" heading, not the session row that opened it. TalkBack always has a focus target after Back, but returning to the opener is not implemented here (unlike Agents and Connection logs in the fixtures); this is recorded as behavior, not a defect fix.

Draft stayed `Owned untouched draft`, no live session started, production data hash unchanged, accessibility settings restored. Font 1.0 only; actual 2× font, other routes and speech audibility remain open.

Repeated at actual font 2.0 (restored to 1.0, verified): light passes; dark failed once ("Focus after InterruptedBack": TalkBack held no focus for 15 s after the second Back) and then passed on two immediate reruns with focus on "Sessions". Treat the second-Back focus as intermittent at 2× font: TalkBack can briefly hold no focus. Not root-caused.

## Real provider and codecs, 11 October 2026

`RealProviderAndCodecTest` (2 tests, emulator-5554 API 36) passes: attachments loaded from the platform MediaStore Downloads provider (not the fixture provider) embed text and PNG bytes without leaking `content://` URIs and reject a text file as an image; device preview decodes PNG, JPEG and lossy WebP. Existing `AttachmentLoaderTest` 3, `ReceivedMediaTest` 2, `AttachmentPickerNavigationTest` 1, `ReceivedContentUiTest` 6 and `AttachmentControlsUiTest` 2 also pass. Not covered: third-party document providers (Drive and similar), the system picker UI driven natively, audio/video codecs beyond the existing WAV fixture, and physical-device media.

Dark 2× production back-stack flake rate: 5 reruns pass 4, fail 1 (plus the earlier single failure), about 2 failures in 8 runs. Same symptom (no TalkBack focus for 15 s after the second Back); not root-caused.

## Deterministic focus after Back, 11 October 2026

Root cause of the intermittent empty focus: no production code manages accessibility focus; after a Back the focus target was TalkBack's own timing-dependent choice, so it sometimes chose nothing within 15 s. Change: the shared `ScreenHeader` row now carries `paneTitle` (`HostScreens.kt`), a non-visual semantics change (no layout or colour difference, so no Penpot board changes) that makes each page a TalkBack pane. Result on actual `MainActivity` (emulator-5554, owned storage): dark 2× font 6/6 and light 2× font 2/2 (plus the earlier 8/8 dark run); 1.0 font full `TalkBackMainActivityUiTest` 4/4, `TalkBackShellUiTest` 32/32, `PageStateMatrixUiTest` 24/24, app and core JVM tests and lint pass.

Trade-off, recorded not hidden: focus after Back is now deterministic but lands on the first actionable control of the returned page, not the heading or the opener. Observed: saved conversation → "Back"; Sessions → "New session"; each Settings child (Connections, MCP servers, Workspace access, Agents) → "Connections". Returning to the opener is still not implemented. Settings-route Back focus is reported by the extended `TalkBackMainActivityUiTest` back-stack cases; Settings routes start no session and leave the draft untouched.

Stale test fixed: `AgentThoughtUiTest` expected the exact status "Syncing" but the header shows "Syncing · Restoring conversation" while replaying; it now matches by substring (it already failed before this change). `SavedHistoryNavigationTest` needs the paired mock host on port 8767 and `ConnectionLogsLayoutUiTest` wide cases need an actual 800 dp window (tablet, out of scope); both fail on a plain phone emulator for those reasons and were not changed.

Rich content: `ConversationUiRestorationTest` 2, `AgentThoughtUiTest` 2, `AutoReviewBadgeUiTest` 2, `ElicitationLifecycleUiTest` 2 pass. Markdown's link confirmation dialog uses `remember`, so it closes on Activity recreation; nothing opens afterwards (safe default); this is read from source, not tested.

## Environment-dependent classes resolved, 11 October 2026

`ConnectionLogsLayoutUiTest` passes 4/4 with the emulator set to an actual 1600×1280 window (800 dp), reset to 720×1280 afterward; its wide cases failed before only because the window was a phone. `SavedHistoryNavigationTest` passes 1/1 against the paired mock host (`scripts/mock-host.ps1 -RequireAuth -Terminal -Media`, started for the run and stopped afterward, listener verified gone). Android CI [38143193352](https://github.com/0xbasinas/acportal/actions/runs/38143193352) passes at `e372404`, covering all commits to date.
