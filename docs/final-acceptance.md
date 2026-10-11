# Final acceptance continuation

11 October 2026, branch `codex/final-acceptance`, starting from merged PR #15 main `8f4741b`. This report records new scoped results. It does not replace the earlier acceptance reports or establish production completion.

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

This closes real Android form submission/recovery for this fixture. It does not cover write notices, shell/review outcomes, every form lifecycle or every provider. The remaining write/shell authorization in this batch is still unused. Earlier authorizations remain consumed.

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
