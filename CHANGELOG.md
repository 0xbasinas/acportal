# Changelog

All notable changes to ACP Portal. The host and tray are versioned releases (`v0.1.x`, see [releases](https://github.com/0xbasinas/acportal/releases)); the Android app is published as numbered debug builds on the [`debug` release](https://github.com/0xbasinas/acportal/releases/tag/debug). The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Verification evidence and open work live in [TODO.md](TODO.md), [TODO_DONE.md](TODO_DONE.md) and the [acceptance report](docs/acceptance-report.md); this file is the readable summary. Dated development notes from before the first release are kept at the bottom.

## [Unreleased]

## [0.1.2] - 2026-10-11

The host and tray programs are unchanged from 0.1.1; this release adds the installers and documentation.

### Added
- One-line installers: `scripts/install.ps1` (Windows) and `scripts/install.sh` (Linux). They download the host and tray from the latest release, verify them against `SHA256SUMS.txt`, install into the user folder (Start menu or applications-menu entry, PATH or `~/.local/bin` links) and support update and uninstall. Downloads made this way carry no "from the internet" mark.
- Uninstall scripts: `scripts/uninstall.ps1` and `scripts/uninstall.sh` stop the host and tray and remove the programs, links, shortcut and PATH entry; your settings and paired devices are kept unless `-Purge` / `--purge` is given (it asks you to type `DELETE`, or needs `-Yes` / `--yes`).
- This changelog.

### Changed
- README rewritten as a short user guide (install, quick start, where files live, build from source); the dated status paragraphs moved here.

## [0.1.1] - 2026-10-11

### Added
- **Tray app finds the host under its release name** (`acpd-windows-x86_64.exe`, `acpd-linux-x86_64`), then next to itself, the install folder and `PATH`; it no longer exits when the host is missing.
- **Tray can download and install the host** ("Download host" / "Update host"): latest GitHub release only, https GitHub addresses only, size and SHA-256 checked against `SHA256SUMS.txt`, installed to `%LOCALAPPDATA%\acpd\bin` or `~/.local/share/acpd/bin`.
- Tray icon is drawn from the logo in a status colour (blue not installed, amber not set up, grey stopped, green running, red problem).
- Windows executables carry the ACP Portal icon and file properties.

## [0.1.0] - 2026-10-11

First public release of the host and tray.

### Host (`acpd`)
- **Standard per-user folders.** `acpd setup --address https://YOUR-HOST` creates the settings (`%APPDATA%\acpd`, `~/Library/Application Support/acpd`, `~/.config/acpd`) and keeps paired devices and sessions in the local data folder (`%LOCALAPPDATA%\acpd\state`, `~/.local/share/acpd/state`); nothing is written to the folder you run it from. An existing `state` folder next to the settings is still used.
- `acpd start --bg` (also `-b`) runs the host in the background.
- **Add folders from the phone:** list host folder names and add or remove workspace roots (`/v1/filesystem/directories`, `/v1/workspaces`). Saved in `user-settings.json`; they survive restarts and reloads.
- **Register agents from the phone:** `POST /v1/agents/check` runs `<command> --version` once (no shell, 5 s, 4 KiB), `POST/DELETE /v1/agents` registers and removes agents with the same validation as a registry file.
- Windows paths are returned without the `\\?\` prefix.

### Breaking
- **No built-in agents.** The Goose and Gemini entries that used to ship with the host are gone; register the agents you want from the phone or in `agents.json`.

### Tray app (`acpd-tray`)
- Menu: status, set up with Tailscale (publishes the host with `tailscale serve --bg` and saves the address), start/stop host, **Pair a phone** (a self-contained local page with the QR code, code, address and countdown, deleted after it expires), open the settings folder.

### Release workflow
- A tag or manual run builds and publishes Windows x86_64 and Linux x86_64 binaries with `SHA256SUMS.txt`; a build-only test mode publishes nothing.

### Android app (debug builds)
- **Add agent** screen with a `--version` check and a row of popular agents that only prefill the form.
- **Add folder from this computer** in New session, with removal of folders added from the phone.
- **Settings → Update app**: installs newer builds from the `debug` release with a checksum check; Android confirms the install.
- **Jump to latest** button when you have scrolled up in a conversation; following the newest text now works for replies taller than the screen.
- Smoother page changes: fade-through transitions, an animated bottom bar and a loading bar that no longer shifts the page.
- Smaller composer and message bubbles; folders show their name first and Windows paths in a readable form.
- New launcher icon and splash.
- The `debug` release is rebuilt on every change to `main`, with run-numbered versions and a shared debug signing key so builds update over each other.

### Design
- Logo as SVG in `assets/logo` (ring and prompt chevron); Penpot boards 36–50 for the new screens and the logo.

[Unreleased]: https://github.com/0xbasinas/acportal/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/0xbasinas/acportal/releases/tag/v0.1.2
[0.1.1]: https://github.com/0xbasinas/acportal/releases/tag/v0.1.1
[0.1.0]: https://github.com/0xbasinas/acportal/releases/tag/v0.1.0

---

## Development history before the first release

The paragraphs below are the dated status notes that used to open the README, unchanged. Newest first; each is a snapshot of that day, so later entries and the sections above supersede them.

11 October saved empty-history accessibility fix: the recovery message and explanation share a scrolling body, with the header and draft composer preserved. Native actual MainActivity checks pass both themes at normal and 2× font. Penpot boards 32–35 record the design. See [current evidence](docs/final-acceptance.md#native-page-state-traversal); broader acceptance remains in [TODO](TODO.md).

11 October write/page-state increment: installed Goose/router Android exact-file write, separate host consent, own-tools warning and draft preservation pass. Native page states pass dark/light at normal font; actual 2× offline empty-state traversal fails and remains open. Earlier Android Actions at `afd99d6` pass. See [current evidence](docs/final-acceptance.md) for consumed authorizations and precise limits.

11 October 2026 continuation on `codex/final-acceptance`, from merged PR #15 main `8f4741b`: actual folder-error, message-cache settings and prompt-history process-death checks pass. Populated older-APK approval replay and installed-Goose Android form Submit also pass to their isolated scopes. See [current evidence](docs/final-acceptance.md) and [TODO](TODO.md) for the finite remaining gates. Older dated checkpoints below are historical.

Offline-history process death and allocation-site/retained-object profiling also pass their recorded scopes. The remaining acceptance areas are real-agent write/shell outcomes, native accessibility/page states, predictive motion and macOS real-agent abrupt cleanup. Physical-phone and signed-release checks remain user-owned/suspended.

11 October 2026 PR #15 review: fixes at `3ab4cd2` address continuous Markdown starvation, large-font empty-history clipping, inert dimension validation and failing fixtures. 35 UI tests, four native Markdown TalkBack runs, ten isolated race/profiling tests and three actual older-APK upgrade phases pass. Full Android and all eight selected platform Actions jobs pass at `3ab4cd2`; see [review evidence](docs/pr15-review.md), [TODO](TODO.md) and [TODO_DONE](TODO_DONE.md) for exact scopes and remaining gates. PR #14 main `5283b55` is integrated; PR #15 remains open. Earlier checkpoints below are historical.

The latest increment verifies real Goose pending form recovery across reload and Android process death, plus native TalkBack review/Cancel for standalone and individual inline Markdown links in both themes at normal/double font. [Acceptance continuation](docs/acceptance-continuation.md) records exact source/run evidence and remaining limits. PR #13 is merged on main at `e6a3460`.

10 October acceptance continuation adds real Goose pending-approval reload, an actual older-APK offline upgrade, streaming/attachment allocator counters, rendered frame measurements and full session-creation TalkBack. Scope and measurements are in [the continuation report](docs/acceptance-continuation.md); [TODO](TODO.md) remains the acceptance checklist.

Current status, 11 October 2026: [PR #13](https://github.com/0xbasinas/acportal/pull/13) is merged on main at `e6a3460`. Real Goose approval/reload/process-death recovery, an actual older-APK offline upgrade and expanded profiling/full-form TalkBack have scoped passing evidence. [TODO.md](TODO.md) tracks remaining acceptance; [the continuation report](docs/acceptance-continuation.md) retains exact measurements, failures and limits. Earlier dated statements below are historical.

10 October 2026 PR #13 acceptance increment: Penpot boards 24–31 cover the new screens in both themes. Model-picker Cancel is in the header after a native traversal failure exposed its footer placement. Four TalkBack runs pass across four pages at normal/actual 2× font; six options regressions and 60 app JVM tests pass, with both debug APKs and lint. See [acceptance continuation](docs/acceptance-continuation.md) for exact platform results, fixture failures and remaining limits.

9 October 2026 daemon/reload increment: native background mode and operator-only local status/stop/reload are implemented. SessionManager publishes coherent launch snapshots; discovery/browse/future sessions use new settings while current sessions and approvals remain unchanged. Listener/TLS/token/storage/frame/logging changes require restart. See [daemon commands and reload rules](docs/daemon-and-reload.md), the ADR and TODO_DONE.md for actual verification and limits.

9 October 2026 native CLI/motion increment: `acpd setup` remembers an existing config and HTTPS address; `acpd start`, `acpd pair` and `acpd doctor` then use that setup. No wrapper script is required. Android ordinary push/Back now use brief overlapping fades, and selected-tab taps no longer rebuild the route. See [native CLI and motion](docs/native-cli-and-motion.md) and TODO_DONE.md for scoped evidence and remaining phone profiling.

9 October 2026 easier pairing and Markdown on `codex/session-model-picker`: Android offers QR scanning and simpler manual entry; `acpd pair --address HTTPS-ADDRESS --qr --name NAME` prints a terminal QR without changing authentication. Scans fill a reviewable form and never pair automatically. Agent replies render native Markdown with bounded off-Main parsing, literal HTML, no remote image fetch and confirmed web links. See [pairing and Markdown](docs/pairing-and-markdown.md) and TODO_DONE.md for verification and limits.

9 October 2026 All sessions creation implemented on `codex/session-model-picker`: + opens connection selection, then the approved connection/workspace/agent form. Fresh discovery gates Start; pairing, unavailable reads and explicit recovery reuse existing boundaries. Isolated checks pass 14 component regressions, one actual-navigation/failed-creation case and four new-screen cases at actual font 2.0. 52 app JVM tests, both debug APKs and lint pass (0 errors, 16 warnings). Real-phone and native TalkBack verification of this flow remain open. See TODO_DONE.md and docs/session-creation-design.md.

9 October 2026 phone-feedback fixes on `codex/session-model-picker`: catalogs over 40 choices use a searchable lazy dialog; smaller catalogs keep their dropdown. Choice mappings are remembered, advertised IDs remain exact, and opening/filtering/cancelling sends no configuration request. Conversation status now aligns with the subtitle inside the header, above the divider. See TODO_DONE.md for scoped verification and remaining phone checks.

9 October 2026 PR #10 review: metadata refresh now updates only metadata, preserving original oversized history/drafts and archive/timestamps instead of writing guarded read placeholders back. Summary invalidation uses a bounded SHA-256 scan, covering equal-length/equal-timestamp changes and metadata hash collisions; production missing-summary rendering no longer decodes history on Main. Verification: 64 core/52 app JVM tests with zero failures/errors/skips, both debug APK builds and lint (0 errors, 16 dependency/toolchain warnings). Two isolated Room regressions and four dark/light Sessions layout/native-keyboard cases pass on small_phone, API 36, 720x1280, density 320, font 1.0. Full old-build upgrade, precise heap profiling, real-agent Android screens and broader route acceptance remain open. Original platform Actions run 37940712823 was inspected, including macOS RSS/process cleanup, container TLS/restart/revocation and identical cross-runner hashes; Rust sources were unchanged by the review and not locally rerun.

Historical PR #10 checkpoint, 9 October 2026: PR #10 merged on main at `6ec49de`, after review at `d4ba9f2`. Local checks pass 64 core/52 app JVM tests, both debug APK builds and lint; six isolated emulator storage/Sessions tests pass. Current-head [Android Actions](https://github.com/0xbasinas/acportal/actions/runs/37949743658) and [Rust Actions](https://github.com/0xbasinas/acportal/actions/runs/37949743959) pass. This does not establish production completion.
[UI/accessibility work](docs/ui-accessibility.md) is active. Scoped emulator checks cover review badges, conversation restoration, conversation/history, pairing and MCP keyboards, and native TalkBack including automatic recovery/Agent focus at normal and actual 2.0 font scale. Broader pages, forms and route focus-restoration acceptance remain open. [HANDOFF.md](HANDOFF.md) records the current continuation checkpoint.
Merged `main` at `981a97a` passes [Rust Actions verification](https://github.com/0xbasinas/acportal/actions/runs/37818450569) and [Android Actions verification](https://github.com/0xbasinas/acportal/actions/runs/37818455265), including the private-mount testy scenarios, Linux shell-review regressions and 47 core/35 app JVM tests. See [verification history](TODO_DONE.md) for coverage and remaining acceptance limits.
Current scope, 8 October 2026: tablet/landscape acceptance is excluded for now. Physical-phone testing is suspended and user-owned; signed APK production/verification is also suspended. Supported-build verification will run in GitHub Actions rather than locally. TODO lists active and suspended work separately; suspended checks remain unverified.
