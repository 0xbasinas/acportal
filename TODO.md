# Remaining work

## T3 Code ideas (`experiment/t3code-ideas`, PR #53, 17 October 2026)

Implemented and checked locally on Windows (details in [TODO_DONE](TODO_DONE.md)): pull requests from the Git view (fork heads, timeout cleanup), permission presets with "Ask for everything" (inherited by delegated tasks and kept on resume), agent profiles, OS service install, ACP `session/close`, earlier sessions (`session/list`) and boolean config options on desktop and Android, an Android pull request form. Design only: [external MCP](docs/adr-external-mcp.md) and [remote routes](docs/adr-remote-routes.md). Research notes: [docs/t3code](docs/t3code/findings.md). The agent prompts used for this work are kept outside the repository.

Open:

- [ ] Look at the new desktop screens in the Electron window, both themes: PR form (fork owner, link button), access chip with Ask for everything, Earlier sessions chip, boolean switches.
- [ ] Run the new Android screens on an emulator (only JVM tests, the debug builds and lint ran): Ask for everything switch, grouped agents, boolean option switches, Earlier sessions dialog, Open pull request form. No Android UI tests were added because none can be run here.
- [ ] Try `acpd service install`, `status` and `uninstall` on a real machine (Windows first: does `--no-console` leave no console window?), then Linux and macOS. The Linux and macOS code, the Linux-only shell-review test and the split `service/` modules compile and run only in Actions.
- [ ] A real `gh pr create` against a throwaway repository. Only a fake `gh` was used.
- [ ] Review the two ADRs; implement only what you approve.
- [ ] Delegated tasks inherit Ask for everything, but a supervising agent (`/v1/sessions/{id}/supervisor`) is a separate, explicit opt-in and still answers requests it is given.

Reconciled 11 October 2026 after merged PR #15 main `8f4741b`, with work on `codex/final-acceptance`. Completed implementation and scoped checks are archived in [TODO_DONE.md](TODO_DONE.md); [the current continuation](docs/final-acceptance.md) and [PR #15 review](docs/pr15-review.md) record actual results, failures and limits. These are narrower acceptance gates, not requests to rebuild completed features.

Goose ACP is the local agent. Android uses authenticated REST/WebSocket and the host uses ACP stdio. Preserve production data, never automatically resend prompts or decisions, and follow [HANDOFF](HANDOFF.md) and [task recovery](docs/task-recovery.md).

## Desktop and handoff (merged into `experiment/orchestrator`)

- [ ] Try by hand on a real phone and the desktop: phone/desktop handoff and shared drafts (16 October 2026), implemented and CI-tested: `takeover=true`/`control_taken`, both clients watch a session another device controls with Take control, and drafts, first-prompt titles and folder prompt history are kept on the host (`/v1/shared`). Host tests `takeover_moves_control_between_clients_and_the_old_one_keeps_watching` and `drafts_titles_and_history_are_shared_between_devices_and_survive_restart` pass; desktop typecheck/tests pass. Android compile/JVM tests rely on CI (no SDK or Gradle plugin access in that session); no client tried by hand. An open session on the phone checks the host every 5 s (the desktop every 4 s); a draft from the other device replaces the box only while it is unedited, otherwise local typing wins. The phone shows host titles for sessions it has no copy of and saves a first-prompt title for sessions started there. Not shared: attachments (binary, per-device file grants) and MCP/checks settings (the shared-settings item below, which needs host-side secret storage).

- [ ] Android: show `_meta.acpdScope` (outside-workspace paths, sandboxed, shell and `shellNote`) on the shell approval sheet; no quick approve when paths are listed.
- [ ] Shared settings on the host (checks config, MCP definitions with masked host-side secrets; prompt history and drafts now done) with migration from desktop and Android; plan in [HANDOFF](HANDOFF.md#roadmap).
- [ ] Android: git view, branch/worktree picker, run on several agents, checks, approval notifications and list badges.
- [ ] macOS sandbox for host commands.
- [ ] Desktop: host `[windows] shell` editable from the app (per-agent edit form with sandbox/shell is done, 17 October 2026). Try the new desktop pieces by hand: Markdown, agent edit, project groups, No project, title bar (Windows overlay buttons; macOS traffic lights and Linux untested). Linux has no PNG app icon yet.
- [ ] Windows: test timeout tree-kill under the sandbox; model reviewer prompt should name the resolved shell.
- [ ] Signed desktop installers (only when asked).
- [ ] Host-owned agent list (`GET /v1/agents/discover`, branch `agent-discovery`, 16 October 2026): run `AddAgentScreenUiTest` on the emulator and try both Add agent forms against a real host. Host, desktop and Android JVM/build checks only so far.

## Proposed, not started

- [ ] **Agent orchestrator, experimental on `experiment/orchestrator` (PR #45, ready for review; the desktop work is merged into it), off by default.** Phases 0-5 are implemented, including worktree isolation with the researched defaults and the phone merge (design sections 16-27). Linux, Windows and macOS CI (plus container and Windows reproducibility) pass: [run 38569965505](https://github.com/0xbasinas/acportal/actions/runs/38569965505) and, with the phone merge, [run 38578927776](https://github.com/0xbasinas/acportal/actions/runs/38578927776) at `eb8f409`. Remaining:
  - Protocol-uniform handling (design §28) is implemented on this branch: no Goose-specific production paths; override layer is `ownTools` / shell_review / orchestrate|delegable only.
  - [ ] Two-machine check with real TLS, and `goose serve` as a peer.
  - [ ] Auto-approve ([design section 32](docs/orchestrator-design.md#32-host-side-auto-approve-15-october-2026)) is verified with a real Goose in a worktree on Windows, one run; not on Linux/macOS or a device, and its optional model reviewer only against a local fake server. Windows shell lines still always rate High (shell lines have their own `shell_review`).
  - [ ] Dependency follow-ups from the `tokio-tungstenite` review (design section 30): `cargo audit` runs in CI (`audit.yml`); `cargo deny` not added; decide whether private-CA peers must be reachable (trust roots are the bundled web set only); the journal-lag cut-off and the reply-queue overflow in the bridge are untested.
  - [ ] Run Linux and macOS CI (and an Android run) for `f123159` and the review fixes (design section 29); neither has run on them. Repository-configured filters, merge drivers and textconv are not blocked in worktree git calls (documented; refuse-when-configured is the next step if wanted).
  - [ ] Worktrees: merge conflicts are refused (the phone shows the command to merge by hand), not resolved; submodule repositories get no worktree (section 31).
  - [ ] Phone task views on a device: run [runbook §9](docs/emulator-runbook.md) (task tree, detail, worktree row, merge button, peer row, both themes, font 2.0, TalkBack). Built and JVM-tested only.
  - [ ] Look at Penpot boards 53 and 54 (both themes) once image export works again: their two text flaws were fixed on 15 October and checked by measured geometry only.
  - Decided against: a token budget (design section 23). Client reconnect is now built (section 31); not yet run on Linux/macOS or over real TLS between two machines.
  - [ ] Before merge: ask the user whether to delete test sessions `20261013_2`-`_5` from their Goose history. (PR #45 is out of draft.)

## Host limits requiring further verification or a containment decision

- [ ] macOS cleanup with real-agent descendants after abrupt host death, particularly when the leader has exited. Linux real-Goose cleanup and Linux/macOS controlled CI tests have passing evidence. macOS leader-less groups and descendants creating a new session remain documented limits; the implemented cleanup is not a sandbox guarantee.

## Android acceptance still open

- [ ] Still open: Back-focus on a connected live conversation and at font 2.0 (pairing, connection chooser, new-session form, Settings children and saved-conversation sub-pages now measured); focus returning to the opener (no supported Compose API found; bespoke handoff untried); third-party document providers and native system-picker automation; audio/video codecs beyond the WAV fixture; physical-device media (user-owned). Done: deterministic Back focus, MediaStore provider and PNG/JPEG/WebP decode, Markdown link dialog lifecycle. See [evidence](docs/final-acceptance.md) and [sub-page Back focus](docs/final-acceptance.md#back-focus-on-conversation-sub-pages-11-october-2026).
- [ ] Real-agent Android write notices and shell approval/review badge outcomes; any broader form lifecycle beyond the verified fixture. Installed Goose/router form Submit, exact fresh-file write with separate agent/host consent, own-tools warning and retained drafts now pass. All recorded form/write approvals are consumed and must not be reused; read-only shell scope remains authorized and unused. The optional preexisting-file notice mode is unrun and requires a fresh approval. See [current continuation](docs/final-acceptance.md) and [runbook §6](docs/emulator-runbook.md).
- [ ] (needs hand gestures or a physical phone) Predictive Back gesture progress/cancellation, sheet/dialog/keyboard animation and physical-phone streaming frame behavior. Three Back/input fixtures pass; debug-emulator streaming p95 remains about 36 ms. See [runbook §8](docs/emulator-runbook.md).

## Release follow-ups (v0.1.3 to v0.1.6)

- [ ] Download the v0.1.3 binaries and run them: install with `install.ps1`/`install.sh`, open the tray, try both Uninstall items on a throwaway install, and check the shortcut/PATH are gone.
- [ ] Real Linux check of agent discovery (nvm/fnm Node folders, `~/.local/bin`, snap/Linuxbrew) and the tray uninstall (desktop entry and links).
- [ ] Phone check of the Add agent scan against a real host (user-owned).
- [ ] Windows CI: `windows_shell_choice_runs_lines_in_the_chosen_shell` got empty Git Bash output twice (runs 38907047097, 38910348900). Not reproduced in 300 diagnostic runs on CI or 25 local full-suite runs; Git's `binash.exe` waits for its inner shell, so the output exists before exit. The terminal pump now retries transient read errors (`WouldBlock`, `Interrupted`, Windows `ERROR_NO_DATA` from MSYS2 non-blocking pipes) instead of treating them as end of output, and drains for up to 5 s after exit instead of 1 s. Close after several clean Windows runs.
- [ ] Download the v0.1.6 tray and try "Set up with Tailscale" through a real start-up (restart Tailscale, then set up), a switched-off Tailscale (the one-time `tailscale up` reconnect) and a signed-out one; none of these were exercised, only the scripted wait and a Ready machine. Also Linux and macOS.
- [ ] Install `app-v0.1.5` (or newer) on a phone and try the Update screen's Follow switch, including the one-time uninstall when moving between debug builds and signed releases. Only the emulator against the real GitHub list was checked, and Download and install was not pressed.
- [ ] Launcher icon of the downloaded `.apk` in a file manager: the `mipmap-anydpi-v26` change is unconfirmed on a real phone. Ask which file manager shows the wrong icon if it persists.
- [ ] Re-run the Update screen device tests and look at the smaller buttons (44 dp actions, 40 dp Follow switch) on the emulator; only the build and lint were rerun after that change. Add a Penpot board for the Follow switch.
- [ ] Keep a private backup of the release key (copy of `acportal-release.jks` and its password, outside the repository). Losing it forces everyone to uninstall before updating.
- [ ] Tray has no confirmation dialog beyond the second click, and does not remove Tailscale Serve settings (`tailscale serve reset`); decide whether either is wanted.

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
