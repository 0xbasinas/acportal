# Remaining work

Reconciled 11 October 2026 against main `7685fca` (PRs #10–#13 merged). Each open item is split into (a) work an agent can do without a device and (b) checks that need an emulator, a device or the user. The agent parts below were delivered in PR `agent-side-checks` (marked done, details in TODO_DONE); the device parts are ready to run with exact commands in the [emulator runbook](docs/emulator-runbook.md). Instrumented tests from that PR compile but have not been run. Completed features, test counts and chronological evidence belong in [TODO_DONE.md](TODO_DONE.md). The unchecked items below are the remaining narrower acceptance scopes, not requests to implement their already completed parent features again.

Goose ACP is the local agent. Android connects through authenticated REST/WebSocket; the host uses ACP stdio. Follow [HANDOFF.md](HANDOFF.md), [requirements audit](docs/requirements-audit.md) and [UI reference](docs/ui-reference.md). Checkmarks are reserved for verified work, and completed items are archived rather than kept here.

## Current host verification

- [ ] Unix cleanup after abrupt host termination or deliberate process-group escape, including active real-agent descendants.
  - [x] (a) Agent: `setsid` agent sessions, recorded-group sweep on the next start, Linux session-member cleanup for escaped groups; Linux tests. macOS sweeps only groups whose leader is alive.
  - [x] (b, Linux) Real Goose descendants killed with the host were stopped by the next start (batch 7, 11 October).
  - (b) macOS: rerun the dispatched macOS job after the fix in this PR — runbook §1.

## Android verification still open

- [ ] Allocation-site and retained-object profiling.
  - [x] (a) Agent: JVM load tests for long streams, 3,000 tools, 5,000/40,000-model lists, 300 saved sessions and per-chunk allocation.
  - (b) Device: profiler allocation sites and retained objects — ready, runbook §2.
- [ ] Route-specific recovery for folder browsing, error pages, message-cache settings, offline history and prompt-history restoration.
  - [x] (a) Agent: `RouteRecoveryUiTest` (state-restoration fixtures for each route; nothing is resent); Settings dialogs now survive recreation.
  - (b) Emulator: run it, then same-task process death by hand — ready, runbook §3.
- [ ] Page-state and accessibility matrix: remaining loading/empty/offline/error pages, Connection logs failure states and return focus, page scroll/focus restoration, rich-content/thought accessibility and lifecycle, device codecs and document providers.
  - [x] (a) Agent: `PageStateMatrixUiTest` (loading/empty/offline/error and Connection logs failure/focus, dark/light × font 1.0/2.0, labelled-action check). Rich-content/codec/document-provider cases are not covered.
  - (b) Emulator: run it plus native TalkBack and 200% font — ready, runbook §4.
- [ ] Session creation with multiple connections and a delayed live host (races).
  - [x] (a) Agent: two race bugs fixed (stale credential restore on slow host reads; loading cleared early / duplicate Start session); `SessionCreationRaceTest` with loopback fixture hosts.
  - (b) Emulator: run it — ready, runbook §5.
- [ ] Real-agent own-tools warnings, write notices, shell approval/review badge outcomes, and form submission and lifecycle on Android.
  - [x] (a) Agent: `real-agent-check.py` gated by `ACPD_REAL_AGENT_OK=1`; opt-in `RealAgentAndroidCheckTest` that observes notices and never answers or submits.
  - [x] Host side with real Goose + DeepSeek: write deny/cancel, auto-review deny badge, form accept/decline, reconnect/load (batch 7). Auto-allow and model-review outcomes were not observed (Goose sends terminal environment overrides).
  - (b) Emulator plus user: Android screens; each real-agent call or submission needs explicit human authorization — ready, runbook §6.
- [ ] Network replay after an old-build upgrade (the offline upgrade passes; see TODO_DONE).
  - [x] (a) Agent: `scripts/upgrade-replay-host.py` (real acpd + mock agent) and prepare/verify/cleanup phases in `LegacyUpgradeReplayTest.kt`.
  - (b) Emulator: run the in-place upgrade from `cf8fa26` with the network connected — ready, runbook §7.
- [ ] Motion: predictive Back, dialogs/sheets and keyboard animation, plus the cost of streaming frames.
  - [x] (a) Agent: per-update streaming work cut (no full-list/last-item comparisons, no timeline copy for the title, Markdown no longer flips to plain text per chunk, one list copy per chunk); `ConversationRenderTest`; `MotionInputFixtureTest` for Back/dialog/keyboard. Frame-time effect unmeasured.
  - (b) Emulator, then the user's phone: repeat the frame measurement and the predictive-Back gesture — ready, runbook §8.

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
