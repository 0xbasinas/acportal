# October acceptance continuation

Work continues in draft [PR #13](https://github.com/0xbasinas/acportal/pull/13), based on merged PR #12 at `3cd1b9b`. The eight requested areas remain a collection of separate acceptance gates; this increment does not establish production completion.

## Penpot and native accessibility

The connected Penpot file `7eed092c-bad6-802e-8008-bea06e03019c`, Page 1, contains editable boards 24–31 for the model picker, pairing scanner failure, Markdown reply and connection selection, each in dark and light themes. Earlier boards are preserved. Fixture names and model labels illustrate layout, not advertised agent capabilities.

Model-picker Cancel moved into its heading row in both design and implementation. The initial native TalkBack run reached 28 model choices before its traversal limit without reaching the former footer action. The corrected header action is reachable before entering the 700-choice list. Opening, traversing and cancelling do not send configuration requests.

Four separate native TalkBack instrumentations pass: dark and light at font 1.0, then dark and light at actual device font 2.0. Each instrumentation has one JUnit method covering four pages. This is 16 scoped screen scenarios, not 16 JUnit tests. Checks cover picker cancellation without mutation, explicit connection selection without launching, scanner failure with disabled pairing, and Markdown heading/list/code traversal. They do not prove QR camera capture, speech audibility, full form completion or every Markdown node/link outcome.

Six existing `SessionDetailsUiTest` regressions pass. The changed app passes 60 JVM tests, both debug APK builds and lint. The emulator is small_phone, API 36, 720×1280, density 320. Font returns to 1.0; the TalkBack harness restores accessibility settings. Production saved data is untouched. Core protocol sources were unchanged and their JVM suite was not rerun.

Local ignored evidence includes `.local/new-screens-talkback-*-fixed.txt`, `.local/new-screens-talkback-*-font2.txt` and a visually inspected `.local/penpot-model-picker-dark.png`. The authenticated Penpot endpoint is not stored in tracked files.

## Platform and containment

Merged-main [platform run 38037739241](https://github.com/0xbasinas/acportal/actions/runs/38037739241) passed all nine jobs, including the isolated testy runner. Independent Windows artifacts contain identical first/second-build hashes on both runners: `51F35338F636C5815DF98956BF2736985B6ECDBA81C859E9AE50D3B7DFC37592`. These artifacts belong to `3cd1b9b`, not later Android changes.

New-test [run 38038118368](https://github.com/0xbasinas/acportal/actions/runs/38038118368) passes Linux stable, Rust 1.88, macOS, container smoke and independent Windows reproducibility. Windows stable fails in the restart fixture because it reused an HTTP connection from the forcibly terminated host. The corrected fixture constructs a fresh client before its single restart read; it does not retry a mutation. All four Windows daemon integration tests, targeted strict Clippy and formatting pass locally after this correction. [Rerun 38039091020](https://github.com/0xbasinas/acportal/actions/runs/38039091020) at `bab0bec` passes Linux stable, Rust 1.88 and Windows stable. Extras and testy are deliberately skipped in this rerun; retain their separate earlier source evidence.

The Unix SIGTERM test checks an owned foreground host, control-record removal and restart on its listener. It does not exercise an active real-agent descendant during SIGTERM. The Windows abrupt-exit fixture checks both descendant generations using handles acquired from the isolated fixture, then credential continuity and interrupted metadata after explicit restart. Deliberate Unix process-group escape and real-provider reload continuity remain open.

## Next checks

An opt-in `-PacportalAcceptance=true` debug build uses `dev.acportal.acceptance` and a separate test-provider authority. The normal debug and release application IDs remain `dev.acportal`. This creates independent storage for profiling and future upgrade fixtures; it does not itself prove an old-build upgrade.

`IsolatedAllocationProfileTest` runs only in that separate package. It warms the loader, reads 20 fixture binary documents of exactly 262144 bytes, validates each 349528-character Base64 blob and reports ART process-wide allocation/GC counters. On API 36 it passes with 47243264 allocated bytes, 47230976 freed bytes and a sampled post-GC heap delta of 12288 bytes. These counters include framework and instrumentation overhead and do not identify exact retained objects or allocation sites. No heap dump, network or production document is read. An ordinary-package run skips this opt-in test and is not profiling evidence.

Run from the Android project after checking that the acceptance package belongs to your fixture:

```powershell
$env:JAVA_HOME='C:/Program Files/Amazon Corretto/jdk21.0.11_10'
./gradlew.bat -PacportalAcceptance=true :app:assembleDebug :app:assembleDebugAndroidTest
# Install both output APKs, then select only the isolated fixture:
adb shell am instrument -w -r -e class dev.acportal.data.IsolatedAllocationProfileTest dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner
```

Opt-in builds share Gradle output paths with ordinary builds. Rebuild without the property before handing the usual APK path to the user. Only remove the separate package if you created and own it; never clear the normal application's data.

Continue isolated heap/attachment/streaming profiling and old-build upgrade without reading or dumping production credentials. Real Goose process-death/pending-approval recovery, broader routes, real-agent Android warning/forms, page-state/focus restoration and motion frame-time acceptance remain in TODO. Physical-phone and signed-release work retain their existing suspended ownership.
