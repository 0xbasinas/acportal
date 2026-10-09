# Android recovery, stress and UI acceptance

Active implementation branch: `codex/android-recovery-acceptance`, starting from merged PR #8 (`ada4798`). This checklist preserves the full work requested on 9 October 2026. Existing scoped evidence is in TODO_DONE.md and ui-accessibility.md; it does not prove these remaining gates.

## Conversation recovery

- [x] Actual MainActivity previous-task restoration after verified process absence: offline history, retained drafts, two sessions and Back navigation. Cached approvals remain unavailable. Passed the isolated OfflineConversation workflow on 9 October 2026; this does not cover live host/replay or conversation scroll/expansion.
- [ ] Live conversations after process death, using an external controlled host that survives the Android process. Restore local state before releasing network refresh/replay.
- [ ] Pending approvals reconciled against authoritative replay; removed/replaced approvals cannot be answered. No automatic prompts, decisions, cancellation or mutation retries.
- [ ] Multiple live sessions remain independently scoped; recovery of one does not overwrite the other.
- [ ] Navigation, conversation scroll and independently expanded tool/thought activity across recreation and process death.

Use the external procedures in task-recovery.md. Preparation/restoration methods must run separately; instrumentation must not replace the task between termination and restoration. Record production data fingerprints, close owned stores/scopes and remove only fixture resources.

## New Android screens

- [ ] Elicitation: field kinds, exact choice values, empty arrays, validation, explicit Submit/Decline/Cancel, sheet dismissal, replay gating and native keyboards.
  Rendered dark/light form validation, exact padded choices, required empty arrays, disabled controls, explicit answers and native keyboard reachability pass separately at actual font1.0/2.0. Sheet lifecycle/dismissal, full field interaction and live replay remain open.
- [ ] Own-tools warning and host-write notices: complete explanations, disclosure state, contrast, touch bounds and navigation.
- [ ] Dark/light, small windows and actual large system fonts; native TalkBack traversal/state/focus for these new controls.

## Memory and stress

- [ ] Android heap profiling with long multi-session conversations; measure retained heap and peak allocation separately from encoded-content limits.
- [ ] Attachment loading/serialization under pressure, including temporary allocations and main-thread responsiveness.
- [ ] Permission count/byte overflow through actual navigation/screens; retain prior accepted consent, detach, block submissions after dismissal and send no decision.

## Remaining UI and accessibility

- [ ] Saved/error Agents discovery and complete automatic-review badge outcomes through native TalkBack.
- [ ] Dark/light loading, empty, offline and error-state page matrix with reachable explicit recovery.
- [ ] Focus/scroll restoration across remaining pages and routes; remaining native form keyboards and large-text controls.

Tablet/landscape acceptance is excluded. Physical-phone testing remains user-owned/suspended; signed APK work is suspended. Supported Rust/Linux/container/reproducibility runs belong in GitHub Actions. Never wipe app/AVD data or infer completion from skipped fixture tests.

## 9 October 2026 offline recovery evidence

`verify-task-recovery.ps1 -MainActivity -Scenario OfflineConversation` passes preparation, external termination/restoration and final verification/cleanup. Two saved interrupted conversations have different histories/drafts and cached pending approvals. The saved-history route returns in the same task (1569) in a distinct process with framework saved state; no instrumentation runs between termination and restoration. Back returns to interrupted-session actions and Sessions, and the second conversation opens independently. Cached approval controls stay hidden. Final verification asserts unchanged fixture histories/drafts and production hosts/sessions/UI preferences/MCP aliases; the fixture task/store is removed.

The original MCP MainActivity workflow separately passes after the harness change (task1570). Historical task IDs are not reusable process evidence. Logs: `.local/offline-conversation-task-recovery.log` and `.local/mcp-task-recovery-regression.log`. The closed-loopback host cannot run an agent; these runs do not verify live replay, message submission or network mutation counts.

Affected implementation checks pass59 core/36 app JVM tests with zero failures/errors/skips, both debug APK builds and lint (0 errors,16 warnings, including an additional target-SDK availability notice). No dependency/toolchain upgrades. Emulator small_phone/API36/720×1280/density320, font1.0, automatic rotation1/user rotation0 and services null; no data wipe. Rust unchanged and not rerun.

## Elicitation native keyboard evidence

ElicitationScreenUiTest uses the production form in a scrolling full-window surface with native IME insets. Missing required values produce validation without a callback. Actual taps focus fields, select an advertised padded constant and reach Submit while the native keyboard remains visible. The callback verifies typed integer/number/boolean output and a required empty array. Disabling the form blocks all three answer actions; explicit Decline/Cancel produce only their requested action and no content. The fixture has no repository, transport, vault or persisted inputs.

Two dark/light cases pass at normal font and separately two at actual system font2.0, without failures/skips. External finally restores and verifies font1.0. Logs: `.local/elicitation-native-ui.log`, `.local/elicitation-native-system-font-two.log`. Test APK build/lint pass after adding the class. No TalkBack, modal-sheet dismissal/recreation, physical-device or real-agent acceptance is implied.
