# Emulator and device runbook

One place for the emulator, device and user checks that follow the agent-side work in PR `agent-side-checks`. Each section matches the numbered open item in [TODO.md](../TODO.md). The selected PR #15 review runs now pass: route restoration (4), page states (24), session-creation races (4), Back/input (3), streaming Markdown (4), native Markdown TalkBack (four runs), six profiling checks and three guarded upgrade phases. [Review evidence](pr15-review.md) records exact results and limits; real-agent Android and broader manual checks remain open. Record results (pass/fail, emulator image, date) in [TODO_DONE.md](../TODO_DONE.md) and update [the acceptance report](acceptance-report.md).

## Before you start

- Android project: `android/universal-acp`. On Windows use `gradlew.bat` and PowerShell quoting; the commands below use `./gradlew`.
- Use an API 34+ emulator (predictive Back needs 34+) with animations on. Run one class at a time unless a section says otherwise.
- Fixture runs that seed or read app storage use the isolated acceptance package so the normal app's data is never touched:

  ```sh
  ./gradlew -PacportalAcceptance=true :app:installDebug :app:installDebugAndroidTest
  # then, per class:
  adb shell am instrument -w -r -e class <Class> dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner
  ```

  Rebuild without `-PacportalAcceptance=true` before handing out an ordinary debug APK ([development notes](development.md)).
- Pure Compose fixtures (no storage) can run through Gradle directly:

  ```sh
  ./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=<Class>
  ```

- Host commands run from the repository root after `cargo build -p acpd`.
- **Real-agent runs make paid model calls.** Run them only after you decide to, one command at a time. Nothing in this runbook runs one by default.

## §1 Unix cleanup after abrupt host kill (host, no emulator)

What changed: agents start in their own session (`setsid`), the host records each launched group in `state/process-groups.json` (group, leader start time and boot id, recording host and its start time), and the next start with the same state directory stops groups left by a dead host. On Linux it also stops descendants that left the agent's process group (same session) both on normal removal and after an abrupt kill, even when the agent itself already exited. A group is never signalled unless the recording host is gone and the leader's recorded identity still matches; on Linux, a leader-less session is stopped only for members on the same boot that started no earlier than the recorded leader.

1. Automated (Linux, already in CI): `cargo test -p acpd --test daemon unix_abrupt_host_kill_is_cleaned_up_by_the_next_start` and `cargo test -p acpd --test lifecycle unix_descendant_that_leaves_the_agent_group`.
2. macOS: Actions → *Rust* → *Run workflow* with `extras` on runs the macOS job (all acpd tests, including lifecycle). Expected: escaped-group test passes. macOS limitation: after an abrupt kill, only groups whose leader is still alive are swept (no session ids without `/proc`).
3. Real agent: passed on Linux on 11 October with Goose + DeepSeek ([batch 7](real-agent-testing.md#batch-7-run-11-october-2026-linux-1200-athens)). To repeat (paid): start the host, open a Goose session from the phone, ask for a long-running command (for example `sleep 600` through the shell tool), then `kill -9 <acpd pid>`. Expected: `ps -o pid,pgid,sid,args -s <agent sid>` still lists the command; `acpd start` again → within a second the list is empty and the host log warns "stopped agent process groups left by an earlier host".

## §2 Memory: allocation sites and retained objects

Agent side: `HeavyProviderLoadTest` (core) and `ConversationRenderTest` (app) cover long streams, 3,000 tool calls, 5,000/40,000-model lists, 300 saved sessions, and allocation per streamed chunk.

Device steps (Android Studio profiler, acceptance package):

1. `./gradlew -PacportalAcceptance=true :app:installDebug :app:installDebugAndroidTest`.
2. Run the existing stress fixtures one at a time: `dev.acportal.data.AndroidTimelineHeapStressTest`, `dev.acportal.presentation.ModelCatalogAllocationProfileTest`, `dev.acportal.data.IsolatedAllocationProfileTest`.
3. In Android Studio: *Profiler* → attach to `dev.acportal.acceptance` → *Track memory consumption (Java/Kotlin allocations)* during a Sessions list refresh and a streamed reply (use the fixture host from §7 with `_mock` prompts, or a real session only if you approve the model call).
4. Leave the session, force GC, *Capture heap dump*. Expected: no `LiveSession`/`SessionState` retained for closed sessions; a single active conversation retains at most about 8 MiB of timeline (the reducer budget); `SessionListSummaries` holds one summary per visible row.
5. Record the top allocation sites per update (expected leaders: text concatenation of the streamed message and one item-list copy per chunk).

## §3 Route recovery

```sh
./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=dev.acportal.presentation.RouteRecoveryUiTest
```

Expected: 4 tests pass — folder-browse error keeps Retry and choices; the "History on this device" dialog survives state restoration (fixed in this PR: it used plain `remember`) and reports the Save messages choice; offline history and the unsent draft stay after restoration; prompt-history selection places text without sending. Then repeat the same-task process-death procedure from [task recovery](task-recovery.md) by hand on: workspace browser, Settings → History on this device, a saved conversation offline, and the prompt-history sheet. Never resend a prompt or decision during recovery.

## §4 Page states and accessibility

```sh
./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=dev.acportal.presentation.PageStateMatrixUiTest
```

Expected: 24 cases (6 pages × dark/light × font 1.0/2.0) pass; every actionable node has a spoken label. Then native TalkBack: enable TalkBack, set system font to 200%, and walk Sessions (empty/archived/search-empty), Connections (none), host loading → unavailable, the error banner, a saved conversation without messages and Connection logs with a failure. Expected: each heading and button is announced, Back returns focus to the row that opened the page. The existing `scripts/verify-talkback-ui.ps1` automates the TalkBack switch on Windows.

## §5 Session-creation races

```sh
adb shell am instrument -w -r -e class dev.acportal.data.SessionCreationRaceTest dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner
```

Expected: 4 tests pass — out-of-order creations on two loopback hosts are saved under their own hosts; a slow host read no longer restores a credential replaced by a concurrent re-pair (fixed in this PR); a failed slow read marks the host offline without restoring the old credential; the view model ignores a repeated Start session for the same host/agent/folder and keeps the loading state until the slow creation finishes (fixed in this PR). Uses an in-memory database and loopback fixtures only.

## §6 Real-agent Android checks (paid, explicit go-ahead each run)

Host side: `scripts/real-agent-check.py` now refuses to run unless `ACPD_REAL_AGENT_OK=1` is set for that command ([real-agent testing](real-agent-testing.md)). The host-side checks for this item already passed with real Goose + DeepSeek on 11 October ([batch 7](real-agent-testing.md#batch-7-run-11-october-2026-linux-1200-athens)); only the Android screens remain.

Android side, opt-in instrumented check that renders the live session with the production screen and **never answers a permission or submits a form**:

1. Pair the host in the acceptance app and create the session you want to observe (for own tools, use an agent marked `ownTools`).
2. Run (omit `realPrompt` to only observe; with it, exactly one prompt is sent):

   ```sh
   adb shell am instrument -w -r \
     -e class dev.acportal.presentation.RealAgentAndroidCheckTest \
     -e acportalRealAgent 1 -e realHostId <host id> -e realSessionId <session id> \
     -e expect ownTools,hostWrite,autoReview \
     -e realPrompt "Edit calc.py to fix add" \
     dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner
   ```

   The DeepSeek key never goes into the fixture, the test APK or instrumentation arguments: the phone only talks to your host, and the key lives in the host's environment. On your PC, set it for the host process only, for example (PowerShell) `$env:OPENAI_API_KEY = Get-Content $HOME\.config\deepseek-api-key; $env:DEEPSEEK_API_KEY = $env:OPENAI_API_KEY; acpd start`, with the Goose registry entry using `GOOSE_PROVIDER=openai`, `GOOSE_MODEL=deepseek-flash`, `OPENAI_HOST=https://api.deepseek.com` ([setup](real-agent-testing.md#setup-used-for-the-recorded-linux-run)). Close the shell afterwards.

   `expect` accepts `ownTools`, `hostWrite`, `autoReview`, `permission`, `form`. Expected: pass once every listed notice is visible; a still-running turn is cancelled at the end. Form *submission* stays a manual step: answer the form yourself in the app and confirm the agent receives it.

## §7 Network replay after an old-build upgrade

The automated runner is preferred: `python3 scripts/upgrade-replay-host.py --acpd <acpd binary> --mock <mock binary> --listen 127.0.0.1:<unused port> --adb <adb binary> --serial <verified serial> --old-apk <cf8fa26 acceptance APK> --current-apk <current acceptance APK> --test-apk <current acceptance test APK>`. It runs selected prepare/verify/cleanup phases, creates a fresh pairing code after installation without printing it, and restores its owned reverse mapping. Never run these classes as an ordinary whole-suite instrumentation. The review used this mode successfully with an empty host session; populated conversation/approval replay is still open.

Manual alternative below: generate a **fresh** phone pairing code in Terminal A after step 4 installation, because codes expire after 60 seconds. Do not save codes in logs. Remove your owned reverse mapping after cleanup.

1. Terminal A: `python3 scripts/upgrade-replay-host.py` (real acpd + mock agent, no model calls). It prints `replayHostId`, `replaySessionId`, `replayAddress` and a one-use phone pairing code. Leave it running.
2. `adb reverse tcp:8767 tcp:8767`.
3. Install the older APK built from `cf8fa26` with the acceptance ID (as in [acceptance continuation](acceptance-continuation.md)) and the current test APK, then prepare:

   ```sh
   adb shell am instrument -w -r -e class dev.acportal.data.UpgradeReplayPreparationTest -e upgradeReplay prepare \
     -e replayHostId <id> -e replaySessionId <id> dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner
   ```

4. `adb install -r` the current acceptance APK (do not clear data), keep the network on, then verify:

   ```sh
   adb shell am instrument -w -r -e class dev.acportal.data.UpgradeReplayVerificationTest -e upgradeReplay verify \
     -e replayCode <code> dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner
   ```

   Expected: the legacy 3 MiB state became the history-gap placeholder, the draft survives, pairing returns the same host id, the session connects and the host replay completes without errors, and the draft is still there afterwards.
5. Cleanup (host still running): `-e class dev.acportal.data.UpgradeReplayCleanupTest -e upgradeReplay cleanup -e replayHostId <id>`. Then Ctrl-C terminal A.

## §8 Motion and streaming cost

Agent-side changes: the conversation screen no longer compares the whole item list (`LaunchedEffect(state.items)`) or the full last message on each streamed update, the title no longer copies the timeline per frame, Markdown keeps its previous rendering while text is appended instead of swapping to plain text and back on every chunk, and the reducer copies the item list once per chunk instead of twice. `ConversationRenderTest` measures the per-frame helpers.

1. Frame cost: rerun `dev.acportal.presentation.RenderedFrameProfileTest` with the same emulator as the earlier 35–37 ms p95 measurement ([motion evidence](native-cli-and-motion.md)); compare p50/p95/p99 and record both.
2. Back/dialog/keyboard:

   ```sh
   ./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=dev.acportal.presentation.MotionInputFixtureTest
   ```

   Expected: 3 tests pass — Back closes the prompt-history sheet without selecting, Back closes the Appearance dialog and keeps Settings, the first Back with the keyboard open hides it and keeps the typed address.
3. By hand: with gesture navigation, drag the predictive Back gesture slowly on a pushed page, a sheet and a dialog; cancel halfway and complete once. Expected: preview follows the finger, cancel restores, completion follows the current short production transition. Then repeat 1–3 on your phone.
