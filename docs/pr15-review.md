# PR #15 review and emulator evidence

11 October 2026: review started at `c59a73e`; fixes are published at `3ab4cd2` on `agent-side-checks`. PR #14 main `5283b55` is integrated by the subsequent documentation merge. Source checks below cover `3ab4cd2`; the merge changes Markdown only.

## Review fixes

- Continuous Markdown updates previously cancelled the 80 ms parsing debounce on every chunk, leaving an old rendered reply until streaming stopped. A conflated throttled collector now parses the latest source during continuous updates, off Main. A device regression verifies progress before the stream ends.
- The saved-conversation empty state was clipped at double font. Its body now scrolls within the available height, preserving the existing layout and theme.
- `COLUMNS` and `LINES` inert environment overrides now accept only one to four ASCII digits, matching the documented shell-review boundary. Alphabetic values and `1e3` remain manual-review inputs.
- Route fixtures disambiguate repeated semantic text, and the Connection logs fixture requests keyboard input mode before testing keyboard focus. Session-creation tests clear their ViewModel store before closing their owned database.
- The upgrade runner can install and run all three guarded phases, generating the private one-use pairing code after installation. Python bytecode was removed from the PR and ignored.

## Actual checks

These were separate scoped runs, not a combined full acceptance suite.

- Windows Rust workspace tests passed at the original head. After the shell-review fix, its affected regression, workspace strict Clippy and formatting passed.
- Corrected Android source: 68 core and 65 app JVM tests, zero failures/errors/skips; ordinary app/test debug APKs and lint passed, with 0 errors and 18 warnings. Acceptance app/test APKs also built. A first full-build attempt exhausted JVM native memory; it terminated before a successful retry using two workers, 1536 MiB heap and two active processors. No dependency or project toolchain change was needed.
- Ordinary emulator checks: **35 passed** in 73.96 seconds: RouteRecoveryUiTest (4), PageStateMatrixUiTest (24), MotionInputFixtureTest (3), MarkdownMessageUiTest (4). The initial 31-case run failed eight cases; the separate two-case reproduction also failed before the fixes above.
- Native TalkBack: **four selected Markdown link confirmation/Cancel runs passed**, dark/light at normal and actual system font 2.0. Individual inline-link actions and standalone links reach review without opening a browser.
- Isolated acceptance package: **10 passed** in 59.568 seconds: SessionCreationRaceTest (4), AndroidTimelineHeapStressTest (1), IsolatedAllocationProfileTest (2), ModelCatalogAllocationProfileTest (1), RenderedFrameProfileTest (2). Race checks cover replaced credentials, failed slow reads, independent hosts and duplicate creation suppression.
- Actual old `cf8fa26` acceptance APK → current acceptance APK upgrade: selected preparation, verification and cleanup each passed. A 3 MiB legacy history becomes the visible history-gap placeholder; the draft survives pairing and authenticated WebSocket replay from a real acpd with an empty mock session. This proves reconnect/replay completion, not replay of a populated real-agent conversation or approval after upgrade. No data wipe was performed; only fixture-owned rows and the owned adb reverse mapping were removed.

Device: `small_phone`, `emulator-5554`, API 36, 720×1280, density 320. Motion/profiling used animation scales 1, restored to the original 0 afterward. Native font restored to 1.0; accessibility services restored to null. Normal `dev.acportal` saved data was not seeded or cleared. Raw logs are ignored local evidence under `.local/pr15-*`.

## Profiling measurements and limits

| Fixture | Observed result |
| --- | --- |
| Four histories, 600 × 64 KiB updates each | 11,571,200 retained bytes; 47,677,440 sampled peak growth; 42 retained items each |
| Twenty binary loads of 262,144 bytes | 47,374,432 ART allocated; 47,353,952 freed; 20,480 post-GC heap delta |
| Four streaming histories, measured interval | 837,222,400 allocated; 837,226,496 freed; framework/test overhead included |
| 100 prompt serializations | 437,559,296 allocated; 437,002,240 freed; 286,720 retained; 9,080,832 sampled peak; 150 Main ticks, longest 34 ms |
| 700 models, twenty search/Cancel cycles | 57,032,960 allocated; 55,349,424 freed; 1,683,536 retained heap delta; 9,897,248 sampled peak |
| Light streaming window, 120 updates / 164 samples | p50 30.03 ms; p95 36.55 ms; p99 52.18 ms; max 67.69 ms |
| Dark streaming window, 120 updates / 189 samples | p50 25.86 ms; p95 36.28 ms; p99 40.60 ms; max 51.97 ms |

ART counters include process-wide framework/test work. Sampled heap growth is not an allocation-site report or retained-object graph. Rendered frames remain near the earlier 35–37 ms p95; this does not establish improved frame performance or a physical-phone frame budget.

## Remaining acceptance

RouteRecoveryUiTest restores composable state; it does not perform external Android process death. The page-state matrix uses synthetic font and semantic labels; the four native TalkBack runs above cover Markdown only. Broader native page traversal, return focus, rich content, provider/codec behavior, predictive Back gestures, attribution of allocations and retained objects, populated upgrade replay and real-agent Android warning/badge/form submission remain open in [TODO](../TODO.md). No paid-provider prompt or permission approval was sent during this review. The prior one-time elicitation authorization remains consumed. Existing Penpot designs are retained; no new Penpot board or fresh design comparison is claimed.

## Published-source Actions

At `3ab4cd2`, [full Android 38129827608](https://github.com/0xbasinas/acportal/actions/runs/38129827608) completes successfully. [Rust/platform 38129825230](https://github.com/0xbasinas/acportal/actions/runs/38129825230) completes successfully with all eight selected jobs: Windows stable, Linux stable/strict Clippy, Linux Rust 1.88, macOS tests/process cleanup/RSS, container TLS/state/mock execution, two independent Windows builds and the cross-runner comparison. Both runners produced `acpd.exe` SHA-256 `10A004C6C6BBEBB76A7859C02911F74573B726A198D365661CC4490560E54D3A`. Actual job logs were inspected. Testy was intentionally unselected in this dispatch; its earlier evidence is separate and is not claimed as a current-head run.

After the acceptance runs, ordinary debug app/test APKs were rebuilt without `-PacportalAcceptance=true`, with lint passing. The final main integration and evidence updates change Markdown only; no source result above is represented as a run of that later documentation commit.
