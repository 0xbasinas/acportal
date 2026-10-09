# Android recovery, stress and UI acceptance

Active implementation branch: `codex/android-recovery-acceptance`, starting from merged PR #8 (`ada4798`). This checklist preserves the full work requested on 9 October 2026. Existing scoped evidence is in TODO_DONE.md and ui-accessibility.md; it does not prove these remaining gates.

## Conversation recovery

- [x] Actual MainActivity previous-task restoration after verified process absence: offline history, retained drafts, two sessions and Back navigation. Cached approvals remain unavailable. Passed the isolated OfflineConversation workflow on 9 October 2026; this does not cover live host/replay or conversation scroll/expansion.
- [x] Controlled live conversations after process death, using an external controlled host that survives the Android process. Restore local state before releasing network refresh/replay.
- [x] Pending approvals reconciled against authoritative replay; removed/replaced approvals cannot be answered. No automatic prompts, decisions, cancellation or mutation retries.
- [x] Multiple controlled live sessions remain independently scoped; recovery of one does not overwrite the other.
- [x] Scoped navigation, conversation scroll and independently expanded tool/thought activity across recreation and process death.

Use the external procedures in task-recovery.md. Preparation/restoration methods must run separately; instrumentation must not replace the task between termination and restoration. Record production data fingerprints, close owned stores/scopes and remove only fixture resources.

## New Android screens

- [ ] Elicitation: field kinds, exact choice values, empty arrays, validation, explicit Submit/Decline/Cancel, sheet dismissal, replay gating and native keyboards.
  Rendered dark/light form validation, exact padded choices, required empty arrays, disabled controls, explicit answers and native keyboard reachability pass separately at actual font 1.0/2.0. Modal dismissal/recreation and full field interaction now pass in isolated fixtures; real-agent elicitation/replay remains open.
- [ ] Own-tools warning and host-write notices: complete explanations, disclosure state, contrast, touch bounds and navigation.
- [ ] Dark/light, small windows and actual large system fonts; native TalkBack traversal/state/focus for these new controls.

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
