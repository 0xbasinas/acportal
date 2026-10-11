# Final acceptance continuation

11 October 2026, branch `codex/final-acceptance`, starting from merged PR #15 main `8f4741b`. This report records new scoped results. It does not replace the earlier acceptance reports or establish production completion.

Published as draft [PR #16](https://github.com/0xbasinas/acportal/pull/16). The draft PR event skips the Android job by design. The full [manual Android Actions run](https://github.com/0xbasinas/acportal/actions/runs/38133735509) completed successfully at implementation head `afd99d6`. That result predates the write/page-state fixture increment below. Rust production sources are unchanged and were not locally rerun.

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

Actual font 2.0 exposes a failure in the offline scenario: the saved-conversation banner, Connect action and composer are traversable, but `No messages saved` is missing from TalkBack traversal. Loading/unavailable and the two empty-list scenarios precede that failure successfully in dark mode. The light 2.0 run was not started after this known failure. Do not claim the four-run matrix passed. Font restores to 1.0, accessibility services to null, accessibility/touch exploration to 0, verified externally. The broader accessibility gate remains open, now including this concrete defect.

The user enabled Penpot, but tool discovery still exposes no Penpot tools, Document Control reports no connected sessions, and the browser inventory has no open design tab. A visible layout fix must be represented in Penpot before acceptance, as requested. No production layout change was made in this increment.

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
