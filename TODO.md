# Remaining work

Reconciled 11 October 2026 after merged PR #15 main `8f4741b`, with work on `codex/final-acceptance`. Completed implementation and scoped checks are archived in [TODO_DONE.md](TODO_DONE.md); [the current continuation](docs/final-acceptance.md) and [PR #15 review](docs/pr15-review.md) record actual results, failures and limits. These are narrower acceptance gates, not requests to rebuild completed features.

Goose ACP is the local agent. Android uses authenticated REST/WebSocket and the host uses ACP stdio. Preserve production data, never automatically resend prompts or decisions, and follow [HANDOFF](HANDOFF.md) and [task recovery](docs/task-recovery.md).

## Host limits requiring further verification or a containment decision

- [ ] macOS cleanup with real-agent descendants after abrupt host death, particularly when the leader has exited. Linux real-Goose cleanup and Linux/macOS controlled CI tests have passing evidence. macOS leader-less groups and descendants creating a new session remain documented limits; the implemented cleanup is not a sandbox guarantee.

## Android acceptance still open

- [ ] Native TalkBack and actual large-font traversal of remaining loading/empty/offline/error pages, Connection logs return focus, broader scroll/focus restoration, rich-content/thought lifecycle, codecs and document providers. The 24 synthetic page-state cases and four native Markdown TalkBack runs pass to their stated scopes. See [runbook §4](docs/emulator-runbook.md).
- [ ] Real-agent Android own-tools/write warnings and shell approval/review badge outcomes; any broader form lifecycle beyond the verified fixture. Installed Goose/router form recovery and production Submit now pass with one exact typed answer and preserved draft. The batch's one `ask_preference` approval/submission is consumed; do not reuse it. The specifically authorized disposable write/read-only shell part remains unused. See [the current continuation](docs/final-acceptance.md) and [runbook §6](docs/emulator-runbook.md).
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
