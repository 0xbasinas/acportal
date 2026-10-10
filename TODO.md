# Remaining work

Updated 10 October 2026. PR #12 is merged on main at `3cd1b9b`; acceptance work continues in draft [PR #13](https://github.com/0xbasinas/acportal/pull/13) on `codex/remaining-acceptance`. PR #13 is not merged. Completed features, test counts and chronological evidence belong in [TODO_DONE.md](TODO_DONE.md).

Goose ACP is the local agent. Android connects through authenticated REST/WebSocket; the host uses ACP stdio. Follow [HANDOFF.md](HANDOFF.md), [requirements audit](docs/requirements-audit.md) and [UI reference](docs/ui-reference.md). Checkmarks are reserved for verified work, and completed items are archived rather than kept here.

## Current host verification

- [ ] Harden/test Unix cleanup after abrupt host termination or deliberate process-group escape, including active real-agent descendants. Real Goose pending-approval/credential/session continuity across reload passes. Windows Job Object breakaway is rejected and abrupt-host cleanup of two descendant generations passes. Unix foreground SIGTERM/control-record/listener restart passes on Linux/macOS. See the exact source and run limits in the acceptance continuation.

## Android verification still open

- [ ] Profile allocation sites/retained objects during Sessions updates and after leaving sessions; repeat with broader provider pressure. Isolated ART counter/sampled heap checks cover binary loading, 100 prompt serializations, four long streaming histories and 20 search/cancel cycles over 700 models. Bounds and actual-screen permission overflow checks pass. Process-wide counters include framework/test overhead. [Profiling procedure](docs/android-acceptance.md).
- [ ] Broaden route-specific recovery to folder browsing, error pages, message-cache settings, offline history and prompt-history restoration. Real Goose process-death with a pending approval, same-task restoration, cached draft before network, exact authoritative approval, two live sessions and zero automatic prompts/decisions now passes. Controlled replaced/removed approvals and exact conversation scroll/disclosure recovery also pass. Never automatically resend prompts or decisions.
- [ ] Finish the page-state and accessibility matrix: remaining loading/empty/offline/error pages, Connection logs failure states and automatic return focus, wider page scroll/focus restoration, rich-content/thought accessibility and lifecycle, and device codecs/document providers. Scope details are in [UI/accessibility](docs/ui-accessibility.md) and [Android acceptance](docs/android-acceptance.md).
- [ ] Check remaining Markdown/link TalkBack outcomes; broaden multiple-connection and delayed-live-host race checks for session creation. Full session-creation form traversal and Start pass in both themes at normal and actual double font. Model cancellation, connection selection, pairing scanner failure, Markdown heading/list/code, saved/error Agents, badges and form-sheet checks are completed to their documented scope.
- [ ] Check the elicitation sheet, own-tools warnings, write notices, shell approval and review badges against real agents on Android. Isolated rendered, keyboard, large-font and TalkBack fixtures are completed; host-side Goose form accept/decline is completed. [Real-agent evidence](docs/real-agent-testing.md).
- [ ] Verify network replay after an old-build upgrade. An actual APK from `cf8fa26` upgraded in place to the current acceptance APK; Sessions, explicit saved-history gap handling, small draft retention and exact original large-column fingerprints pass without clearing data.
- [ ] Broaden predictive-Back, dialogs/sheets and keyboard animation checks; investigate measured streaming frame costs and repeat on the user-owned phone. Actual rendered streaming-window durations are recorded for both themes, with emulator p95 around 35–37 ms. Short page fades and selected-tab behavior have scoped enabled/disabled-animation evidence. [Motion evidence](docs/native-cli-and-motion.md).
- [ ] Keep new UI changes represented in Penpot before acceptance. Existing boards 24–31 cover the four new-screen designs in both themes. Compare original supplied mockups only if their missing source folder becomes available; no fresh original-image comparison is claimed.

## Suspended and user-owned checks

- Physical-phone checks are user-owned: trusted TLS/hostname/issuer trust, Wi-Fi/mobile-data changes, lock/background, token expiry/revocation/renewal and competing controllers, plus full real-agent diff/terminal/permission/recovery workflows.
- User phone retest of the approximately 700-model picker and conversation header, shorter transitions, and successful All sessions creation. These features are implemented; device/provider-specific usability remains unverified.
- User phone QR capture: first-use Google Play module availability, expired codes and camera cancellation. Injected scanner fixtures do not prove camera capture.
- Signed release APK production and verification remain suspended until requested. Debug builds and ordinary implementation checks remain in scope.
- Tablet/landscape acceptance is excluded from current scope.

## Future decisions, outside current completion gates

OS service/boot installation and automatic crash restart are not implemented. URL-mode elicitation and `elicitation/complete` are unsupported and never advertised. Optional delivery-journal persistence, biometric unlock and workspace-specific shell-review allowlist extensions remain future product decisions, not unfinished claims about the implemented features.

Update [acceptance report](docs/acceptance-report.md) when new evidence changes acceptance. Preserve earlier run URLs, failures and limitations in TODO_DONE; move completed checks there at each finished increment.
