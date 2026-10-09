# Remaining work

Updated 9 October 2026 after auditing completed implementation and scoped verification. Current local work is on `codex/session-model-picker`, based on merged PR #10; local changes are not assumed published. Completed features, test counts and chronological evidence belong in [TODO_DONE.md](TODO_DONE.md).

Goose ACP is the local agent. Android connects through authenticated REST/WebSocket; the host uses ACP stdio. Follow [HANDOFF.md](HANDOFF.md), [requirements audit](docs/requirements-audit.md) and [UI reference](docs/ui-reference.md). Checkmarks are reserved for verified work, and completed items are archived rather than kept here.

## Current host verification

- [ ] Run GitHub Actions against the current native setup, QR dependency and daemon/reload sources: Linux stable/minimum Rust, macOS detachment and SIGTERM, container TLS/mounted-state/agent execution, and independent Windows reproducibility. Earlier platform runs do not cover these local changes. [Daemon guide](docs/daemon-and-reload.md).
- [ ] Check daemon reload continuity with a real provider and harden/test cleanup after abrupt host termination or deliberate process-group/job escape. Windows isolated lifecycle and pending-approval replay fixtures already pass; graceful stop is implemented.

## Android verification still open

- [ ] Profile exact heap/allocator behavior during streaming, Sessions updates, attachment serialization and after leaving sessions; repeat with broader provider pressure and the large model catalog. Sampled heap, bounds and actual-screen permission overflow checks are completed. [Profiling procedure](docs/android-acceptance.md).
- [ ] Verify process-death/replay with a real Goose conversation and pending approval. Broaden route-specific recovery to folder browsing, error pages, message-cache settings, offline history and prompt-history restoration. Controlled two-session recovery, cached-before-network state, authoritative replaced/removed approvals and exact conversation scroll/disclosure recovery already pass. Never automatically resend prompts or decisions.
- [ ] Finish the page-state and accessibility matrix: remaining loading/empty/offline/error pages, Connection logs failure states and automatic return focus, wider page scroll/focus restoration, rich-content/thought accessibility and lifecycle, and device codecs/document providers. Scope details are in [UI/accessibility](docs/ui-accessibility.md) and [Android acceptance](docs/android-acceptance.md).
- [ ] Verify native TalkBack for the new searchable model picker, All sessions creation flow, QR pairing and Markdown replies; broaden multiple-connection and delayed-live-host race checks for session creation. Existing saved/error Agents, badge outcomes and form-sheet TalkBack checks are completed to their documented scope.
- [ ] Check the elicitation sheet, own-tools warnings, write notices, shell approval and review badges against real agents on Android. Isolated rendered, keyboard, large-font and TalkBack fixtures are completed; host-side Goose form accept/decline is completed. [Real-agent evidence](docs/real-agent-testing.md).
- [ ] Install over an older app build containing a large saved session without wiping data, then check Sessions, history-gap handling and replay. Guarded Room reads, original-column preservation and both isolated oversized-row device regressions already pass.
- [ ] Broaden predictive-Back, dialogs/sheets, keyboard and streaming animation checks; profile frame times if sluggishness persists. Short page fades and selected-tab behavior are implemented and have scoped enabled/disabled-animation device evidence. [Motion evidence](docs/native-cli-and-motion.md).
- [ ] Compare implemented pages against the supplied mockups if the original files become available; the recorded folder is absent. Keep the remaining contrast, layout and route checks scoped, without claiming a fresh comparison.

## Suspended and user-owned checks

- Physical-phone checks are user-owned: trusted TLS/hostname/issuer trust, Wi-Fi/mobile-data changes, lock/background, token expiry/revocation/renewal and competing controllers, plus full real-agent diff/terminal/permission/recovery workflows.
- User phone retest of the approximately 700-model picker and conversation header, shorter transitions, and successful All sessions creation. These features are implemented; device/provider-specific usability remains unverified.
- User phone QR capture: first-use Google Play module availability, expired codes and camera cancellation. Injected scanner fixtures do not prove camera capture.
- Signed release APK production and verification remain suspended until requested. Debug builds and ordinary implementation checks remain in scope.
- Tablet/landscape acceptance is excluded from current scope.

## Future decisions, outside current completion gates

OS service/boot installation and automatic crash restart are not implemented. URL-mode elicitation and `elicitation/complete` are unsupported and never advertised. Optional delivery-journal persistence, biometric unlock and workspace-specific shell-review allowlist extensions remain future product decisions, not unfinished claims about the implemented features.

Update [acceptance report](docs/acceptance-report.md) when new evidence changes acceptance. Preserve earlier run URLs, failures and limitations in TODO_DONE; move completed checks there at each finished increment.
