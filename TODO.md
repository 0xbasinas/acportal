# Remaining work

Updated 9 October 2026 after PR #10 review. PR #10 is merged on main at `6ec49de`; reviewed code commit: `d4ba9f2`. Documentation continuation branch: `codex/docs-acceptance-reconciliation`. [Acceptance report](docs/acceptance-report.md) records current results; [TODO_DONE.md](TODO_DONE.md) preserves completed increments and historical limitations.

Goose runs as `goose acp` through the host registry. Android uses authenticated REST/WebSocket; the host uses ACP stdio. Preserve production data, use isolated fixtures, and never automatically resend prompts, retry mutations or answer approvals. Follow [AGENTS.md](AGENTS.md) and [HANDOFF.md](HANDOFF.md).

## Active priorities

- [ ] Real-agent Android workflows. Drive real Goose from Android through read/write, full diff review, approval/rejection, terminal execution/kill, cancel, reconnect and explicit load. Check shell-line scrolling/review badges and form elicitation accept/decline/cancel/replay. Verify own-tools warnings with a declared own-tools agent and host-write notices on actual events. Host-side Goose workflows and isolated Android screens already pass.
- [ ] Real-agent conversation recovery. Repeat process death during active output and pending approvals with more than one real session. Verify authoritative replaced/removed approvals, independent histories/drafts, navigation, exact scroll and expanded tools/thoughts. Controlled mock-host live/offline two-session recovery and count/byte overflow already pass; do not repeat those as unfinished implementation.
- [ ] Android heap and provider pressure. Profile retained heap and allocation peaks separately during long multi-session streams and attachment loading/serialization. Check Sessions summary work and prompt encoding stay off Main, provider failures/cancellation release payloads, and UI responsiveness survives pressure. Sampled timeline/attachment regressions already pass; exact allocations and wider providers remain open. Use only synthetic fixture content in profiling artifacts, never production secrets.
- [ ] Full old-build upgrade. Seed an isolated older app build with large histories/drafts, then install the new build without wiping data. Verify Sessions opens, metadata refresh preserves original columns, history gaps remain visible and reconnect explicitly replays. Both isolated oversized-row Room regressions already pass; this full upgrade has not run.
- [ ] Remaining page states and accessibility. Prioritize MCP, workspace policy/browser, Changes and Connection logs. Cover dark/light loading, empty, offline and error states; persistent explicit recovery after dismissal; native keyboards; actual large fonts; TalkBack traversal, speech audibility, focus return and reachable actions. Saved/error Agents, all review badges and the elicitation/warning/notice fixtures already have scoped native TalkBack evidence.
- [ ] Remaining route recovery. Check scroll/focus and safe navigation after Activity recreation and verified process death for the remaining detail/action routes, folder browsing, prompt history and cache-enabled/disabled offline histories. Preserve the policy that unsaved secrets/form inputs and attachment payloads are not restored from saved state. Existing conversation and MCP recovery results are scoped, not app-wide acceptance.
- [ ] Rich content and providers. Broaden media codec/content/thought/diff accessibility, export and document-provider cancellation/failure coverage, including lifecycle changes and large repository diffs. Keep visible truncation/history-gap reporting and full available diff fidelity.
- [ ] Custom-agent and remaining diagnostics. Verify documented custom registry templates through actual adapter negotiation and cross-platform diagnostic presentation. Gemini currently has negotiation/sign-in evidence, not a full workflow. Check actual recent-workspace launch and removed-folder errors; audit still records undisplayed registry icon metadata and incomplete runtime health reporting.
- [ ] Remaining host containment and developer entry points. Assess agents that deliberately leave Unix process groups or abrupt daemon termination; document or implement the intended containment guarantee without claiming an OS sandbox. Ordinary Unix/macOS cleanup already passes. Verify Make-specific commands through GitHub Actions. Executable reproducibility is verified across two Windows runners; PDB and signed-artifact reproducibility are not established by that check.
- [ ] Final screen comparison and evidence reconciliation. Compare all implemented pages with the supplied mockups when the original images can be found; they were absent at the recorded path. Keep [requirements audit](docs/requirements-audit.md), [screen acceptance](docs/screen-acceptance.md) and [acceptance report](docs/acceptance-report.md) aligned with actual results. Do not equate scoped passing checks with production completion.

## Decisions and future features

Optional delivery-journal persistence, per-workspace shell-review allowlist extensions, biometric unlock, QR pairing, slash commands, fork and URL-mode elicitation are future decisions, not requirements to silently add. URL elicitation and `elicitation/complete` are not advertised. Unsent attachments must be selected again after process death.

## Suspended or excluded

- Physical-phone acceptance is suspended and user-owned: trusted TLS deployment/issuer/hostname trust, Wi-Fi/mobile switching, lock/unlock/backgrounding, offline access, credential renewal/expiry/revocation and competing controllers.
- Signed release APK production and verification are suspended until requested. Normal debug builds and implementation checks remain in scope.
- Tablet and landscape acceptance are excluded.
- Minimum Rust, Linux/container and reproducible Windows supported-build checks must run through GitHub Actions. Their current scoped checks passed; rerun when affected code changes.

Archive only completed and verified work into TODO_DONE.md. Keep failed historical attempts and limitations intact; mark them historical rather than presenting them as current blockers.
