# Remaining work

Reconciled 11 October 2026 for PR #15 against merged PR #14 main `5283b55`. Completed implementation and scoped checks are archived in [TODO_DONE.md](TODO_DONE.md); [PR #15 review](docs/pr15-review.md) records actual emulator results, failures and limits. These are narrower acceptance gates, not requests to rebuild completed features.

Goose ACP is the local agent. Android uses authenticated REST/WebSocket and the host uses ACP stdio. Preserve production data, never automatically resend prompts or decisions, and follow [HANDOFF](HANDOFF.md) and [task recovery](docs/task-recovery.md).

## Host limits requiring further verification or a containment decision

- [ ] macOS cleanup with real-agent descendants after abrupt host death, particularly when the leader has exited. Linux real-Goose cleanup and Linux/macOS controlled CI tests have passing evidence. macOS leader-less groups and descendants creating a new session remain documented limits; the implemented cleanup is not a sandbox guarantee.

## Android acceptance still open

- [ ] Allocation-site and retained-object profiling during Sessions refresh/streaming and after closing sessions. JVM load regressions and isolated ART/heap/700-model counters pass; counters do not identify retained object paths. See [runbook §2](docs/emulator-runbook.md).
- [ ] External same-task process-death recovery for folder browsing/error routes, message-cache settings, offline history and prompt history; broader scroll/expanded-state restoration. Four composable restoration checks pass, but do not prove external process death. See [runbook §3](docs/emulator-runbook.md).
- [ ] Native TalkBack and actual large-font traversal of remaining loading/empty/offline/error pages, Connection logs return focus, broader scroll/focus restoration, rich-content/thought lifecycle, codecs and document providers. The 24 synthetic page-state cases and four native Markdown TalkBack runs pass to their stated scopes. See [runbook §4](docs/emulator-runbook.md).
- [ ] Real-agent Android own-tools/write warnings, shell approval/review badge outcomes, and form submission/lifecycle. Host-side batch 7 and earlier pending-form recovery evidence remain valid; the new opt-in Android fixture was not run in this review. Each new paid call or decision needs its own human authorization. See [runbook §6](docs/emulator-runbook.md).
- [ ] Old-build upgrade with a populated live conversation/authoritative approval replay. The actual `cf8fa26` in-place offline upgrade and authenticated empty mock-session replay pass, preserving the draft and cleaning only owned rows. Populated replay remains a broader check, not a failure of those passing phases. See [runbook §7](docs/emulator-runbook.md).
- [ ] Predictive Back gesture progress/cancellation, sheet/dialog/keyboard animation and physical-phone streaming frame behavior. Three Back/input fixtures pass; debug-emulator streaming p95 remains about 36 ms. See [runbook §8](docs/emulator-runbook.md).

## Ongoing contributor requirements

Keep future UI changes represented in Penpot before acceptance. Existing boards 24–31 are completed to their recorded scope; this is not a separate unfinished implementation. Compare original supplied mockups only if their missing source folder becomes available; no fresh original-image comparison is claimed.

## Suspended and user-owned checks

- Physical-phone checks are user-owned: trusted TLS/hostname/issuer trust, Wi-Fi/mobile-data changes, lock/background, token expiry/revocation/renewal and competing controllers, plus full real-agent diff/terminal/permission/recovery workflows.
- User phone retest of the approximately 700-model picker and conversation header, shorter transitions, and successful All sessions creation. These features are implemented; device/provider-specific usability remains unverified.
- User phone QR capture: first-use Google Play module availability, expired codes and camera cancellation. Injected scanner fixtures do not prove camera capture.
- Signed release APK production and verification remain suspended until requested. Debug builds and ordinary implementation checks remain in scope.
- Tablet/landscape acceptance is excluded from current scope.

## Future decisions, outside current completion gates

OS service/boot installation and automatic crash restart are not implemented. URL-mode elicitation and `elicitation/complete` are unsupported and never advertised. Optional delivery-journal persistence, biometric unlock and workspace-specific shell-review allowlist extensions remain future product decisions, not unfinished claims about the implemented features.

Update [acceptance report](docs/acceptance-report.md) when new evidence changes acceptance. Preserve earlier run URLs, failures and limitations in TODO_DONE; move completed checks there at each finished increment.
