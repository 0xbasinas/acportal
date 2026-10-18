# Changelog

All notable changes to ACP Portal. The host and tray are versioned releases (`v0.1.x`, see [releases](https://github.com/0xbasinas/acportal/releases)); the Android app is published as numbered debug builds on the [`debug` release](https://github.com/0xbasinas/acportal/releases/tag/debug). The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Verification evidence and open work live in [TODO.md](TODO.md), [TODO_DONE.md](TODO_DONE.md) and the [acceptance report](docs/acceptance-report.md); this file is the readable summary. Dated development notes from before the first release are kept at the bottom.

## [Unreleased]

18 October 2026, shell picker: the host lists the shells it can start (`shells` in `GET /v1/terminals`) and `POST /v1/terminals` takes a `shell` id; the desktop dock has a menu to open a terminal in a chosen shell.

18 October 2026, later: Settings → Terminal in the desktop app turns terminals and the phone preview (and its ports) on or off on the host (`GET`/`PUT /v1/settings/features`, applied at once, kept in `user-settings.json`).

18 October 2026, on `main` (PRs #57–#61):

- Host terminals: interactive shells over `/v1/terminals` (Unix `openpty`, Windows ConPTY in a kill-on-close job), a resumable output ring, one keyboard owner, idle close. Off by default (`[terminal]`), phone role only, allowed folders only, clean environment.
- Host preview: `GET /v1/preview/{port}/…` shows a local dev server on a paired phone (`[preview]`, off by default, listed ports only, GET/HEAD only, no token or cookies passed on).
- Desktop: a terminal dock, an in-app browser (isolated, Send to agent, tabs, find in page), and a status bar with Terminal and Browser toggles.
- Android: Terminal and Preview screens and the session menu with Terminal, Preview and Git.

On `experiment/orchestrator` (PR #45), not released yet:

- Agent orchestrator: delegated tasks, supervisor, peer bridge, worktree isolation and phone merge. Off by default.
- Experimental Electron desktop app (`desktop/`): several hosts, approvals with scope check, git view, session branches and worktrees, run on several agents, pre-merge checks, and an inline new-session composer.
- Host: scoped `/v1/git`, `POST /v1/checks`, a scope check on terminal consents, an opt-in OS sandbox for approved commands (bubblewrap, AppContainer), and a Windows shell choice (cmd, PowerShell, Git Bash).
- Phone/desktop handoff: Take control (`takeover`), with the replaced client watching. Shared drafts, titles and prompt history (`/v1/shared`).
- `POST /v1/agents/{id}/probe` and `setup` on `GET /v1/agents`, for an agent's models and modes before its first session.
- No agent defaults in the apps or CLI: the host owns the one list of known agent CLIs and their ACP arguments (`registry/known.json`, `GET /v1/agents/discover`, feature `agentDiscovery`). The phone and desktop Add agent forms show only that list; their own copies (which disagreed on Gemini's arguments) are gone. `acpd probe` and `acpd chat` need `--agent` instead of falling back to Goose. When the desktop composer cannot read an agent's models it says "Models unknown" with Retry instead of showing a default.
- Desktop session options work like the phone's: every option the agent reports (model, provider, mode, effort, ...), one row each, a list opened per option with search for long lists. When one option changes another (a provider brings other models), the session header shows the agent's updated list from its reply. The new-session form sets nothing on the agent before the session starts; picks are applied in order at start. Generic ACP only; no agent-specific code.
- Desktop: agent replies render as GitHub-flavoured Markdown (nested lists, task lists, tables, quotes, highlighted code with Copy); raw HTML stays text, images are never loaded, links open only after confirming.
- Desktop: agents added from the apps can be edited in Settings → Agents (command, arguments, environment, own tools, shell, sandbox on/off, enabled); host `GET /v1/agents/{id}` returns the saved definition and `GET /v1/agents` reports each agent's `sandbox`.
- Desktop: the sidebar lists sessions under their project (collapsible, most recent project first, **+** for a new session there).
- Sessions outside every project: the new-session folder chip offers **No project**; the host makes a new empty folder under `<state>/scratch` (`POST /v1/scratch`, feature `scratchSessions`) and allows sessions only below that root.
- Desktop: the app draws its own title bar with the acportal logo (as on the phone) and the open session's title; the system window buttons follow the theme. The Windows window icon is `assets/logo/logo.ico`.
- Desktop Git view: the History tab draws the commit graph (coloured lanes, curves for branches and merges, hollow merge dots, a ring on HEAD) from the new read-only `log-dag` git operation, instead of git's ASCII `--graph`; older hosts keep the ASCII view.
- Desktop Git view: **Suggest** writes the commit message without an open session. `POST /v1/commit-message` (feature `commitMessage`) has the chosen agent write it in a throwaway session (no file, terminal or MCP access) from the staged diff and the repository's recent subjects; it used to paste the diff into the current conversation.
- Windows sandbox: approved commands no longer hang. Each command used to grant and revoke the AppContainer's access on the whole workspace, which Windows applies file by file (~23 s per step in this repository), so the exit came after a minute or more and agents gave up. The grant is now made once per workspace, in the background when a sandboxed session opens, and removed when the host stops; the exit is reported before any cleanup.
- Session options are grouped as the ACP spec suggests, on desktop and phone: Model (with `model_config` and `thought_level` options next to it), Mode, then More options in the agent's order.

## [0.1.8] - 2026-10-12

### Fixed
- The Android app is unchanged from 0.1.7; this release is for the host and tray.
- **Agents no longer open a terminal window on Windows.** The host started agent programs (and host terminal commands) without the "no window" flag, so when it had no console of its own and Windows Terminal was the default terminal app, every agent launch popped open a visible terminal window. They now start with `CREATE_NO_WINDOW`; their input and output are piped, so nothing is lost.

## [0.1.7] - 2026-10-12

### Changed
- The host, tray and Android app are unchanged from 0.1.6; this release only carries updated documentation (README, TODO).

## [0.1.6] - 2026-10-12

### Fixed
- The host program and the Android app are unchanged from 0.1.5; this release is for the tray.
- **Set up with Tailscale no longer fails while Tailscale is still starting.** A Tailscale that was connected but had just started (state `NoState`) was reported as "not started or not signed in". The tray now waits up to about 30 seconds with progress in the menu, tells a real sign-out from a switched-off or still-starting Tailscale, and reconnects a switched-off one with `tailscale up`. Problems that need you are still reported at once.

## [0.1.5] - 2026-10-12

### Added
- The host and tray programs are unchanged from 0.1.4; this release is for the Android app.
- **Update app can follow debug builds or signed releases.** A new Follow switch on the Update app screen chooses between the rolling `debug` pre-release and the newest `app-v*` signed release. It defaults to the kind that is installed and remembers the choice. Because the two use different signing keys, a note explains that switching needs one uninstall.

### Changed
- The buttons on the Update app screen are smaller.

## [0.1.4] - 2026-10-12

### Added
- **Release APK workflow.** Actions > Release APK builds the signed release APK and publishes it as its own release (`app-v<version>`, with a SHA-256 file), never as the "latest" release. Needs the `RELEASE_KEYSTORE_BASE64` and `RELEASE_KEYSTORE_PASSWORD` secrets; see [releases](docs/releases.md#app-releases-signed-apk).

### Fixed
- The launcher icon is now in `mipmap-anydpi-v26`, so tools that read the APK without a recent Android version (older file managers, installers) pick the plain per-density icons instead of the adaptive icon.

### Changed
- `uninstall.ps1` and `uninstall.sh` now show a small menu when run in a terminal with no options: everything (keep settings), everything and delete settings, only the host, or only the tray. Options and `ACPD_ONLY` still skip the menu, and without a terminal the default is unchanged (both, keep settings). Deleting settings still asks you to type DELETE.
- **Tailscale in the tray detects more.** A new status line tells apart: not installed, not started/signed out, turned off, waiting for approval, HTTPS certificates not enabled, no MagicDNS name, no permission (Linux operator), and ready. Tailscale is found in more places (Program Files, `%LOCALAPPDATA%`, `/usr/sbin`, snap, Linuxbrew, Homebrew). When `tailscale serve` waits for Serve/HTTPS to be enabled, the tray shows its real message and opens the enable page instead of a generic timeout.

## [0.1.3] - 2026-10-12

### Added
- **Uninstall from the tray.** Two new menu items (confirmed by clicking twice) stop the host, remove the programs, shortcut or desktop entry, PATH entry and leftovers, and quit the tray; the second one also deletes your settings and data. Same behavior as `uninstall.ps1` / `uninstall.sh`, without needing the script.
- **Better agent discovery on Windows and Linux.** The host now also searches the usual user install folders (npm, pnpm, Cargo, Bun, Go, Volta, nvm/fnm Node versions, WinGet, Scoop, Homebrew, snap, `~/.local/bin`...) after `PATH`, and puts a found program's folder first on the `PATH` of the agent it starts. On Windows an npm `.cmd` shim is understood and started as `node script.js` with literal arguments (no shell), so agents installed with `npm -g` are no longer "not found". New `POST /v1/agents/locate` (names only) lets the phone show which popular agents are installed here, with their path; the **Add agent** screen lists them first and has a **Scan again** button.
- `-Only host|tray` (Windows) and `--only host|tray` (Linux), or `ACPD_ONLY`, on the install and uninstall scripts: install or remove just the host or just the tray. PATH is only touched for the host, the shortcut and desktop entry only for the tray, and removing the host removes the PATH entry only when it was the last program in the folder. `-Purge` / `--purge` cannot be combined with it.

### Fixed
- The host returned Windows folder paths with the `\\?\` prefix (the check that strips it had a typo), which also broke starting a Node script by such a path. v0.1.0–v0.1.2 have this on Windows.

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
- Windows paths are returned without the `\\?\` prefix. (The first release still returned it on Windows because of a typo in the prefix check; fixed in Unreleased.)

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

[Unreleased]: https://github.com/0xbasinas/acportal/compare/v0.1.7...HEAD
[0.1.7]: https://github.com/0xbasinas/acportal/releases/tag/v0.1.7
[0.1.6]: https://github.com/0xbasinas/acportal/releases/tag/v0.1.6
[0.1.5]: https://github.com/0xbasinas/acportal/releases/tag/v0.1.5
[0.1.4]: https://github.com/0xbasinas/acportal/releases/tag/v0.1.4
[0.1.3]: https://github.com/0xbasinas/acportal/releases/tag/v0.1.3
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
