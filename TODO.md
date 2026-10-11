# Remaining work

Reconciled 11 October 2026 against main `7685fca` (PRs #10–#13 merged). Each open item is split into (a) work an agent can do without a device and (b) checks that need an emulator, a device or the user. Completed features, test counts and chronological evidence belong in [TODO_DONE.md](TODO_DONE.md). The unchecked items below are the remaining narrower acceptance scopes, not requests to implement their already completed parent features again.

Goose ACP is the local agent. Android connects through authenticated REST/WebSocket; the host uses ACP stdio. Follow [HANDOFF.md](HANDOFF.md), [requirements audit](docs/requirements-audit.md) and [UI reference](docs/ui-reference.md). Checkmarks are reserved for verified work, and completed items are archived rather than kept here.

## Current host verification

- [ ] Unix cleanup after abrupt host termination or deliberate process-group escape, including active real-agent descendants.
  - (a) Agent: harden and add Linux/macOS tests (CI) for abrupt-kill and process-group escape cleanup.
  - (b) User: confirm with a real agent whose descendants are running when the host is killed.

## Android verification still open

- [ ] Allocation-site and retained-object profiling.
  - (a) Agent: extend isolated JVM/instrumented allocation regressions for broader provider pressure.
  - (b) Device: Android Studio profiler during Sessions updates and after leaving sessions ([procedure](docs/android-acceptance.md)).
- [ ] Route-specific recovery for folder browsing, error pages, message-cache settings, offline history and prompt-history restoration.
  - (a) Agent: add controlled fixtures for each route. Never automatically resend prompts or decisions.
  - (b) Emulator: run them through same-task process-death recovery ([task recovery](docs/task-recovery.md)).
- [ ] Page-state and accessibility matrix: remaining loading/empty/offline/error pages, Connection logs failure states and return focus, page scroll/focus restoration, rich-content/thought accessibility and lifecycle, device codecs and document providers.
  - (a) Agent: write fixtures and tests ([UI/accessibility](docs/ui-accessibility.md)).
  - (b) Emulator: TalkBack and large-font runs.
- [ ] Session creation with multiple connections and a delayed live host (races).
  - (a) Agent: controlled race fixtures and repository tests.
  - (b) Emulator: run them.
- [ ] Real-agent own-tools warnings, write notices, shell approval/review badge outcomes, and form submission and lifecycle on Android.
  - (a) Agent: prepare fixtures and host-side checks ([real-agent evidence](docs/real-agent-testing.md)).
  - (b) Emulator plus user: each real-agent call or submission needs explicit human authorization.
- [ ] Network replay after an old-build upgrade (the offline upgrade passes; see TODO_DONE).
  - (a) Agent: extend the upgrade fixture with a live controlled host.
  - (b) Emulator: run the in-place upgrade from `cf8fa26` with the network connected.
- [ ] Motion: predictive Back, dialogs/sheets and keyboard animation, plus the cost of streaming frames.
  - (a) Agent: investigate the measured streaming frame cost (emulator p95 around 35–37 ms) and add checks ([motion evidence](docs/native-cli-and-motion.md)).
  - (b) Emulator, then the user's phone: repeat the measurements.

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
