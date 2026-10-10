# Android recovery, stress and UI acceptance

Current status, 10 October 2026: draft PR #13 has scoped passing platform Actions, Penpot boards 24–31, native TalkBack including full session creation, real Goose approval/reload/process-death recovery, an actual older-APK offline upgrade and expanded allocator/rendered-frame measurements. The real recovery run found and fixed historical replay clearing an unsent draft. [Acceptance continuation](acceptance-continuation.md) retains exact results and failures; broader routes, real-agent forms/warnings, allocation-site attribution, upgrade replay and motion follow-ups remain in [TODO.md](../TODO.md). Earlier dated checkpoints remain historical.

9 October 2026 phone-feedback increment: conversation status moved into the header and large model/configuration catalogs use a searchable lazy picker. Scoped verification is recorded in [TODO_DONE.md](../TODO_DONE.md). Real-provider phone latency, exact heap profiling and native TalkBack for the picker remain unverified.

9 October 2026 PR #10 review: metadata refresh now updates only metadata, preserving original oversized history/drafts and archive/timestamps instead of writing guarded read placeholders back. Summary invalidation uses a bounded SHA-256 scan, covering equal-length/equal-timestamp changes and metadata hash collisions; production missing-summary rendering no longer decodes history on Main. Verification: 64 core/52 app JVM tests with zero failures/errors/skips, both debug APK builds and lint (0 errors, 16 dependency/toolchain warnings). Two isolated Room regressions and four dark/light Sessions layout/native-keyboard cases pass on small_phone, API 36, 720x1280, density 320, font 1.0. Full old-build upgrade, precise heap profiling, real-agent Android screens and broader route acceptance remain open. Original platform Actions run 37940712823 was inspected, including macOS RSS/process cleanup, container TLS/restart/revocation and identical cross-runner hashes; Rust sources were unchanged by the review and not locally rerun.

Current continuation: PR #10 is merged on main at `6ec49de`, after review at `d4ba9f2`; documentation continuation is `codex/docs-acceptance-reconciliation`. PR #9 recovery work started from merged PR #8 (`ada4798`) and is now historical evidence. This checklist preserves the full work requested on 9 October 2026. Existing scoped evidence is in TODO_DONE.md and ui-accessibility.md; it does not prove these remaining gates.

## Conversation recovery

- [x] Actual MainActivity previous-task restoration after verified process absence: offline history, retained drafts, two sessions and Back navigation. Cached approvals remain unavailable. Passed the isolated OfflineConversation workflow on 9 October 2026; this does not cover live host/replay or conversation scroll/expansion.
- [x] Controlled live conversations after process death, using an external controlled host that survives the Android process. Restore local state before releasing network refresh/replay.
- [x] Pending approvals reconciled against authoritative replay; removed/replaced approvals cannot be answered. No automatic prompts, decisions, cancellation or mutation retries.
- [x] Multiple controlled live sessions remain independently scoped; recovery of one does not overwrite the other.
- [x] Scoped navigation, conversation scroll and independently expanded tool/thought activity across recreation and process death.

Use the external procedures in task-recovery.md. Preparation/restoration methods must run separately; instrumentation must not replace the task between termination and restoration. Record production data fingerprints, close owned stores/scopes and remove only fixture resources.

## New Android screens

- [x] Scoped fixture elicitation: field kinds, exact choice values, empty arrays, validation, explicit Submit/Decline/Cancel, sheet dismissal, replay gating and native keyboards.
  Rendered dark/light form validation, exact padded choices, required empty arrays, disabled controls, explicit answers and native keyboard reachability pass separately at actual font 1.0/2.0. Modal dismissal/recreation and full field interaction now pass in isolated fixtures; real-agent elicitation/replay remains open.
- [x] Scoped fixture own-tools warning and host-write notices: complete explanations, disclosure state, contrast, touch bounds and navigation.
- [x] Scoped dark/light form/warning/notice controls at normal and actual large fonts with native TalkBack. These results do not prove every page or speech audibility.
- [ ] Real-agent Android form replay, own-tools warnings and write notices, including recovery while requests are pending.

## Memory and stress

- [ ] Android heap profiling with long multi-session conversations; measure retained heap and peak allocation separately from encoded-content limits.
- [ ] Attachment loading/serialization under pressure, including temporary allocations and main-thread responsiveness.
- [x] Controlled permission count/byte overflow through actual navigation/screens; retain prior accepted consent, detach, block submissions after dismissal and send no decision.

## Remaining UI and accessibility

- [x] Scoped saved/error Agents discovery and complete automatic-review badge outcomes through native TalkBack.
- [ ] Dark/light loading, empty, offline and error-state page matrix with reachable explicit recovery.
- [ ] Focus/scroll restoration across remaining pages and routes; remaining native form keyboards and large-text controls.

Tablet/landscape acceptance is excluded. Physical-phone testing remains user-owned/suspended; signed APK work is suspended. Supported Rust/Linux/container/reproducibility runs belong in GitHub Actions. Never wipe app/AVD data or infer completion from skipped fixture tests.

## 9 October 2026 offline recovery evidence

`verify-task-recovery.ps1 -MainActivity -Scenario OfflineConversation` passes preparation, external termination/restoration and final verification/cleanup. Two saved interrupted conversations have different histories/drafts and cached pending approvals. The saved-history route returns in the same task (1569) in a distinct process with framework saved state; no instrumentation runs between termination and restoration. Back returns to interrupted-session actions and Sessions, and the second conversation opens independently. Cached approval controls stay hidden. Final verification asserts unchanged fixture histories/drafts and production hosts/sessions/UI preferences/MCP aliases; the fixture task/store is removed.

The original MCP MainActivity workflow separately passes after the harness change (task 1570). Historical task IDs are not reusable process evidence. Logs: `.local/offline-conversation-task-recovery.log` and `.local/mcp-task-recovery-regression.log`. The closed-loopback host cannot run an agent; these runs do not verify live replay, message submission or network mutation counts.

Affected implementation checks pass 59 core/36 app JVM tests with zero failures/errors/skips, both debug APK builds and lint (0 errors,16 warnings, including an additional target-SDK availability notice). No dependency/toolchain upgrades. Emulator small_phone/API36/720×1280/density320, font 1.0, automatic rotation 1/user rotation 0 and services null; no data wipe. Rust unchanged and not rerun.

## Elicitation native keyboard evidence

ElicitationScreenUiTest uses the production form in a scrolling full-window surface with native IME insets. Missing required values produce validation without a callback. Actual taps focus fields, select an advertised padded constant and reach Submit while the native keyboard remains visible. The callback verifies typed integer/number/boolean output and a required empty array. Disabling the form blocks all three answer actions; explicit Decline/Cancel produce only their requested action and no content. The fixture has no repository, transport, vault or persisted inputs.

Two dark/light cases pass at normal font and separately two at actual system font 2.0, without failures/skips. External finally restores and verifies font 1.0. Logs: `.local/elicitation-native-ui.log`, `.local/elicitation-native-system-font-two.log`. Test APK build/lint pass after adding the class. No TalkBack, modal-sheet dismissal/recreation, physical-device or real-agent acceptance is implied.

## 9 October 2026 live recovery and memory fixes (PR #9)

An authenticated Python loopback fixture survives Android termination and holds session metadata separately from WebSocket replay. Actual MainActivity restores cached content before metadata release, then keeps the exact conversation anchor through the saved-to-live transition while Send/Stop and decisions remain blocked. Authoritative replay replaces one pending approval and removes the other session’s old approval. Only the explicit denial of the fresh scoped request is sent. Both sessions keep distinct histories/drafts; offscreen tool/thought expansions survive process death. Count and byte overflow pass through the actual permission sheet and persistent recovery controls with no extra decision. Production fingerprints and owned-store cleanup pass. This verifies controlled REST/WebSocket recovery, not real Goose execution or every route.

Fixes retain route-owned scroll/follow/disclosure choices, with at most 128 hashed disclosure keys; exclude pending approval/model correlation payloads from disk; cap session cache at 1 MiB encoded UTF-8 and show a history gap when reduced to a checkpoint. Byte overflow previously exposed SQLiteBlobTooBigException from cached approval payloads. Existing oversized rows and other draft/metadata columns still need an audit; no database data is wiped.

Prompt JSON is streamed under existing wire budgets on Dispatchers.Default, then ownership/execution state is rechecked before sending. The bounded encoder avoids a complete JSON String/UTF-8 copy before rejection. Disconnected or replaying busy sessions cannot send Cancel. Core 61/app 38 JVM tests, both debug APK builds and lint pass (0 errors,16 warnings); Rust is unchanged/not rerun.

Separate Android stress regressions pass: four timelines each receive 600 updates of 64 KiB (retained growth 10,686,448 bytes, sampled peak growth 81,428,464 bytes); 100 synthetic 256 KiB attachments retain 274,432 bytes with sampled peak 17,846,272 bytes and main heartbeat 101 ticks; 30 real provider 256 KiB binary loads retain 180,224 bytes with sampled peak 19,173,376 bytes. These are runtime heap samples/regression bounds, not exact allocator peaks or a full application heap profile. Logs: `.local/android-final-acceptance-regressions.log`.

Final scoped navigation/field suite passes 16 cases: elicitation full field values/native IME/dismissal/recreation, failed Host/New session recovery, controlled Agents loading/error/dismissal/explicit reload, offscreen disclosures after recreation and repository recovery/cancel guards. Native TalkBack 20 cases pass at normal font, including saved/error Agents, all badge outcomes, own-tools warning, changed/unchanged write notices and elicitation traversal/explicit cancellation. Broader page-state/focus/scroll matrix, speech audibility, real agents/providers and physical acceptance remain open. Normal-font log: `.local/talkback-final-normal-font.log`.

Final held-metadata count and byte scenarios each pass preparation, verified same-task/distinct-process recovery and final preservation/cleanup. Logs: `.local/live-recovery-held-metadata-count.log` and `.local/live-recovery-held-metadata-bytes.log`. Agents navigation additionally passes two dark/light 30-entry catalog cases, checking the returned agent 18 anchor before any scroll action; only GET requests occur. Log: `.local/agent-page-scroll-final.log`. Release manifest excludes debug fixture Activities.

Large-font diagnostic: the first 20-case system-font 2.0 TalkBack run had one hardware-focus stall in dark elicitation (19 pass). The unchanged isolated dark elicitation case then passes at 2.0; `.local/talkback-elicitation-font-two-diagnostic.log`. Preserve this transient failure separately from any later complete rerun; do not combine it into a 20-pass result. Outer font restoration 1.0 and secure-settings restoration execute even on failure.

The unchanged complete large-font repeat had a different single hardware-focus stall (badge heading), while elicitation passed. The fixture now retries only missed focus swipes, at most three times within the same 15-second budget, and still requires changed native accessibility focus. Clicks, answers and mutations are never retried. Earlier failed logs remain historical diagnostics; final runs use the bounded gesture helper.

Final bounded-gesture normal-font TalkBack run passes all 20 cases without failures/skips (170.055 s): `.local/talkback-final-bounded-gestures-normal.log`. This is a separate final run, not an aggregation of earlier failed gestures.

Final bounded-gesture system-font 2.0 TalkBack run separately passes all 20 cases without failures/skips (183.861 s): `.local/talkback-final-bounded-gestures-font-two.log`. Both final runs use real native focus traversal and explicit single callbacks. Earlier failed gesture runs are retained above as diagnostics.

The first final large-font navigation suite exposed two genuine Agents anchor failures (both themes); the other ten cases passed. Returning through AgentRoute cleared fresh details and temporarily inserted an in-list saved notice at large text. Agent navigation now reuses fresh in-memory discovery on details/Back; missing/offline discovery still refreshes, and explicit refresh remains available. This preserves the anchor without turning a saved catalog into fresh launch authority. Final normal-font Agents regression passes two cases: `.local/agent-anchor-final-normal.log`. Full JVM/APK/lint checks pass after the fix: `.local/android-final-agent-anchor-build.log`. Earlier failing 12-case evidence is `.local/android-final-system-font-two.log`.

Final large-font navigation/keyboard/restoration rerun passes all 12 cases without failures/skips after the Agents fix (61.402 s): `.local/android-final-system-font-two-fixed.log`. It uses actual system font 2.0 and the form/lifecycle large-font assertions. Outer finally restores the previous font. This supersedes the two failed anchor cases only for the scoped routes; the wider page/focus matrix remains open.

Offline MainActivity regression passes again on final APKs (task 2079): `.local/offline-recovery-final-regression.log`. MCP follow-up initially stops before process termination because the emulator IME rejects hardware text events despite semantic field focus. Standalone interrupted-fixture cleanup passes each time, asserting production fingerprints and removing only its owned task/store; these cleanup passes are not recovery acceptance. Re-selecting the same keyboard alone did not fix it. Restarting the current emulator keyboard service without clearing data restores typing; the selected IME stays identical. Trial MCP keyboard-handoff/reset helper changes did not make the full harness reliable and were discarded. Earlier input-driver failures remain in .local/mcp-recovery-final-*.log, separate from final recovery evidence.

A final interrupted MCP fixture cleanup passes before a data-preserving emulator reboot. No app/AVD data is wiped, and keyboard selection/font values remain unchanged. The original MCP input steps are retained; the rebooted-device result is recorded separately below.

Final original MCP MainActivity workflow passes after the data-preserving reboot: `.local/mcp-recovery-final-after-emulator-reboot.log`. Preparation, same-task/distinct-process restoration, discarded unsaved draft, Back through chooser/Settings and final production fingerprint/alias preservation/owned cleanup all pass. No instrumentation runs during process absence/restoration. Trial keyboard-helper changes are not shipped. Emulator identity is revalidated as small_phone/API36/720×1280/density320; font 1.0, rotation 1/user rotation 0, services null/accessibility 0/touch exploration 0 and the original selected IME are verified after cleanup. The owned diagnostic hierarchy file is removed. All started builds/instrumentations/fixture servers have terminal outcomes.

## Older-data safety and Sessions memory (PR #10, 9 October 2026)

Current behavior, with JVM tests and two isolated API 36 Room regressions:

- **Reading saved sessions.** The `sessions()` and `session()` queries check each column's size in SQLite (`length(CAST(col AS BLOB))`), so a row can never be larger than Android's 2 MiB CursorWindow:
  - metadata over 256 KiB is read as empty;
  - a saved conversation over 1 MiB is read as `{"historyGap":true}`;
  - a draft over 512 KiB is read as empty.

  Saved agent catalogues over 1 MiB are read as empty. Reading deletes nothing. Metadata refresh updates only metadata; it never writes guarded history/draft placeholders over original stored columns. The oversized copy is not displayed, and reconnect can replay available host history. Before this, one row written by an older version could fail the whole Sessions query with `SQLiteBlobTooBigException`. The schema is unchanged, so no migration is needed.
- **Unreadable or replaced data.** Session details now show as "Details unavailable" until the host refreshes them, instead of crashing the Sessions list, navigation, resume or workspace-access checks. Resume and workspace access treat such a session as unknown. A saved conversation that cannot be read opens empty with a history gap, and the host replays it.
- **Writing.** Drafts are cut at a character boundary to 512 KiB, which is the largest prompt the app can send. Session details first drop the agent's setup choices, which are read from the host again, and then keep only the capabilities the offline pages use. Saving fresh details replaces unavailable metadata only and preserves original history, draft, archive flag and timestamp.
- **Sessions list.** Titles, times and search text are worked out off the main thread. Only rows that changed are decoded again (`SessionListSummaries`). Before this, every save, about twice a second while an agent works, decoded the full saved conversation of every row on the main thread. Search now looks at the workspace, the agent, the title and up to 16k characters of conversation text, not the raw stored JSON. Before, words like `role` or `sequence` matched every session.
- **Timeline retention.** Retention no longer serialises every tool, content or plan item again on each update. Each item's JSON size is computed once and cached in memory, and is not stored (`TimelineRetentionCostTest`).

JVM tests: `StoredLimitsTest` (8), `SessionListSummaryTest` (6) and `TimelineRetentionCostTest` (3).

### Completed isolated storage checks and remaining upgrade

Both `OversizedSessionRowsTest` cases pass on small_phone/API 36: oversized columns return bounded placeholders without failing the list, and metadata refresh preserves raw original history/draft lengths, timestamp and archive flag. The latter uses an owned disposable file database; no production store is read or changed. Four dark/light Sessions compact-layout/native-keyboard tests also pass. These six tests are scoped regressions, not an old-build upgrade.

Still open: install over an isolated older build holding large histories/drafts without clearing data. Check Sessions opens, metadata refresh preserves data, and history-gap/replay behavior through actual navigation.

### Heap profiling (Android Studio)

This has not been run yet. It needs an emulator or device. Use only isolated synthetic conversations and attachments; allocation recordings and heap dumps must contain no production prompts, credentials or provider secrets.

1. Build and install the debug app. Open **View > Tool Windows > Profiler**, choose the app process and pick **Track Memory Consumption (Java/Kotlin Allocations)**.
2. Pair with a host and start a long session. The mock agent with a large output, or a real agent asked to print a large file, will do. Keep the Sessions page open in split screen or switch to it now and then.
3. Record allocations for about 30 seconds while the agent streams. Check that `SessionListSummaries.update` and `sessionListSummary` run on `DefaultDispatcher` threads, not `main`. Also check that `SessionReducer.bounded` no longer allocates `JsonElement.toString` strings for every update.
4. Capture a heap dump after the stream ends and again after leaving the session. Look for retained `SessionState` and `TimelineItem` instances, and for strings of 1 MiB or more beyond the open session.
5. Repeat with 4 attachments of 256 KiB in the composer. The prompt is encoded on `Dispatchers.Default` (PR #9); check that the main thread shows no large `ByteArray` or `String` allocations when you tap Send.

Record the device, the API level, and the peak and retained heap figures in this file.

9 October 2026: the [All sessions creation flow](session-creation-design.md) is implemented. Scoped verification and limits are in [TODO_DONE.md](../TODO_DONE.md); real-phone/native TalkBack acceptance remains open.
