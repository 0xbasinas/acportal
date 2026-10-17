# Prompt: implement T3 Code-inspired improvements

Copy everything below the line into a new agent session started on branch `experiment/t3code-ideas` (cut from `agent-discovery`). The research behind it is in [t3code-research.md](t3code-research.md).

---

You are continuing the ACP Portal project in `C:\Users\basin\acportal` on branch `experiment/t3code-ideas`. Continue in place; do not scaffold replacements, upgrade dependencies, or branch UI behaviour on agent names (agents, models and modes come from the host).

## Read first (in this order)

`AGENTS.md`, `HANDOFF.md` (current section), `TODO.md`, `docs/security.md`, `docs/architecture.md`, `docs/orchestrator-design.md`, `docs/daemon-and-reload.md`, `docs/desktop-multi-host.md`, `docs/t3code-research.md` (what T3 Code does and where it differs from us) and `docs/t3code-acp-feasibility.md` (what ACP can and cannot do for each item below). T3 Code is MIT licensed (https://github.com/pingdotgg/t3code). Use it for ideas only; do not copy its code. Treat `docs/t3code-research.md` as notes from reading its docs, not from running it.

## What already exists (do not rebuild it)

- `/v1/git/{op}` in `acpd/src/api/git.rs`: fixed argv builder `args()`, `allowed()` workspace check, `ref_ok()`, `READ_ONLY`, timeouts (push 120 s). It already has `add`, `commit`, `push`, `merge`, `worktree-*`, `branch-*`, `stash-*`, `compare`, `remote-url`. `/v1/commit-message` (Suggest) is in the same file. It is phone-role only (`phones_only` layer in `acpd/src/api.rs`).
- Desktop Git view `desktop/src/Git.tsx` (commit box at the `Commit` card, `suggest` prop, `GitBtn` with a visible command and undo). Composer `desktop/src/NewSession.tsx` has an `access` chip (read/write/terminal/mcp) and a branch/worktree chip.
- Per-agent safety config in `acpd/src/config.rs`: `[shell_review.<id>]`, `[auto_approve.<id>]` (`acpd/src/auto_approve.rs`, off by default, never refuses, `only_in_worktree` default true), `[windows] shell`, registry `sandbox`/`shell`/`env` (`acpd/src/registry.rs`).
- Orchestrator (`acpd/src/orchestrator/`, off by default): stdio MCP server `acpd mcp-delegate` (`delegate.rs`, `control.rs`) injected per session with a task token. It is for the agent's own delegation only, not an external MCP endpoint.
- Auth in `acpd/src/security.rs`: `Role::{Phone, Peer}` fixed at pairing; one-use 60 s codes; device tokens; `phones_only` / peer-only layers in `api.rs`.
- Daemon: `acpd daemon start|status|stop|reload` (`daemon.rs`, `detached.rs`), no OS service registration. `acpd doctor` is in `doctor.rs`.

## Work, in this order

Do each item as its own commit(s). After each, run the checks `AGENTS.md` lists for what you touched (Rust fmt, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace --all-targets`; `desktop/` typecheck and vitest; Android Gradle only if Android changed) and report what actually passed or was skipped. Add tests with each change. Check the existing mutation-check practice in `AGENTS.md` and, for safety-relevant code, show that a test fails when the guard is removed.

### 1. Open a pull request from the Git view

Gap: we push and compare, but only open a browser compare URL.

- Host: add `POST /v1/pull-requests` in a new `acpd/src/api/pr.rs`, registered next to `/v1/git/{op}` under `phones_only`. Body (`deny_unknown_fields`, camelCase): `path`, `base`, `head`, `title`, `body`, `draft`. Validate `path` with `git::allowed`, branch names with `ref_ok`, cap title (256 chars) and body (64 KiB). Run `gh pr create --title <t> --body-file - --base <b> --head <h> [--draft]` with a literal argv, the body on stdin, `GIT_TERMINAL_PROMPT=0`, a 60 s timeout, `kill_on_drop`, output bounded like `git::run`. Return `{ url }` parsed from stdout, or a stable error code (`gh_unavailable`, `gh_not_signed_in`, `no_remote`, `already_exists`). Never echo `gh` stderr verbatim if it could hold a token; map to codes.
- Make `gh` discovery use `discovery::resolve` like agents do, not a bare PATH lookup. Add a `doctor` line: `gh` found/signed in (no network call; use `gh auth status` exit code only).
- Desktop: in `Git.tsx` after a successful push, offer "Open pull request" with the Suggest-generated title and body (reuse `/v1/commit-message` flow with a new prompt in `git.rs`: subjects since base plus changed-file summary, bounded diff, same cleaning). Always editable, always shows the equivalent `gh` command (match the existing `GitBtn learn` behaviour). Disable with a reason when the host reports no `gh` or an older acpd.
- Out of scope: GitLab/Bitbucket/Forgejo (T3 has them via separate packages; ours is GitHub only for now), merging a PR, Android UI.
- Tests: argv construction (no injection via title/body/branch, mirror `options_cannot_be_injected`), size caps, error mapping with a fake `gh` on PATH (a script in a temp dir; skip on Windows only if the mock cannot be made portable, and say so).

### 2. Permission presets in the composer

Gap: T3 lets you choose one named mode per thread (Supervised, Auto-accept edits, Auto, Full access). Ours spreads the same ideas over `access` chips, `[shell_review]` and `[auto_approve]`, so users cannot see the net effect.

- Host: add `GET /v1/presets?agent=<id>` returning, for that agent, a list of presets computed from the current config snapshot, each with `id`, `label`, `effects` (human-readable lines) and the exact `workspaceAccess` it implies. Proposed presets: `supervised` (every write and command asks the phone/desktop), `auto-edits` (only offered when `[auto_approve.<id>]` is enabled; edits inside the session worktree are allowed by the host rules, everything else asks), `auto-review` (only offered when `[shell_review.<id>]` is enabled; effects text must say that undecided lines still ask). Never offer a preset whose backing config is off, and never include a "full access" preset. The preset only selects among existing behaviours. It must not set any config, widen `workspace_roots`, or skip a consent that is mandatory today. If you find a case where it would, stop and report.
- Desktop: `NewSession.tsx` shows the preset as the first item in the access chip; picking one sets `access` and shows the effects list. Keep the custom read/write/terminal toggles as "Custom".
- Remember the last preset per agent in desktop settings. Do not persist anything host-side.
- ACP limit (see the feasibility doc): ACP defines no approval modes; agent `mode` ids are agent-chosen. A preset therefore governs only host-mediated behaviour (access flags, shell review, auto-approve, answers to `session/request_permission`). Never map a preset onto an agent mode id by name. Show the agent's own `mode` option next to the preset. For an agent flagged `ownTools`, the effects text must say its own tools are not covered.
- Tests: JSON shape and that presets disappear when their config is off; a regression test that no preset can enable terminal/write for a session the config would otherwise refuse.

### 3. Agent profiles (several accounts of one agent)

Gap: T3 supports several Codex or Claude accounts. Registry entries are merged by `id` in `registry.rs`, so a second entry with another `id` and different `env` already works, but nothing groups them.

- Add optional `profileOf` (string, an existing agent id) and `profileLabel` to the registry entry schema (`AgentDefinition`, `deny_unknown_fields` applies; keep old files loading). Validate: target exists, not itself, depth 1, same `command`. Reject at load, not at launch.
- `/v1/agents` returns both fields. Desktop composer groups profiles under their base agent in the agent picker (`NewSession.tsx`) and Settings shows them (`Settings.tsx`, `agentSetup.ts`).
- Secrets: `env` values stay in the operator's registry as they are today. Do not add secret storage, and make sure `/v1/agents` and logs still print env names only (check that existing behaviour and add a test).
- Tests: registry parse (valid, missing target, self, chain), merge override, API output has no env values.

### 4. OS service install (Linux and macOS first, then Windows)

Gap: T3 has `t3 service install|status|restart|uninstall`, `t3 update` and `t3 doctor`. We have `acpd daemon` but nothing that survives logout or reboot.

- Add `acpd service install|status|restart|uninstall` in a new `acpd/src/service.rs`. Linux: a systemd user unit written under `~/.config/systemd/user/acpd.service` with `ExecStart` as an absolute literal path plus args from the remembered `acpd setup` config (no shell, no templating of untrusted values; escape per systemd rules and test it). macOS: `~/Library/LaunchAgents/dev.acportal.acpd.plist` (XML-escape every value). Windows: a per-user scheduled task at logon via `schtasks` with literal args, no elevation. All three call `acpd start` (foreground) so that systemd/launchd/Task Scheduler own the process; do not use `detached.rs`.
- `status` reports installed/running/enabled and the log path, and on Linux checks lingering (`loginctl show-user --property=Linger`), printing the recovery command rather than running sudo. `uninstall` removes only the unit/plist/task this command wrote (record a marker in the unit comment and refuse to remove one without it). Leave state, config, tokens and devices untouched.
- Do not implement self-update. Add a short section in `docs/installation.md` on what a safe updater would need (stage, verify checksum, keep previous binary, rollback), and link T3's `docs/internals/server-updates.md` as a reference.
- Tests: unit-file and plist generation (quoting, spaces, unicode), uninstall refusal without marker, `status` parsing with a fake `systemctl`. Real install/start is verified only through the existing Actions or a documented manual run; say which.

### 5. External MCP endpoint (design and ADR only, then stop)

Gap: T3 exposes `/mcp` so outside agents (Claude Code, Codex, ChatGPT) can list and start threads, with OAuth, read-only vs mode-ceiling grants, and a separate audience (`mcp-client`) that is rejected on every non-MCP route. Ours only has per-session stdio delegation.

Write `docs/adr-external-mcp.md` covering: tools (start with read-only `sessions_list`, `session_read`, `tasks_list`; mutating tools only after review); a new `Role::Agent` (or equivalent) that every route except the MCP route rejects, following how `phones_only` and the peer layer work; credential type reuse (device token minted by a pairing code, no OAuth server in this branch, explain why or why not); a ceiling on what a thread started through MCP may do, taken from the same `workspaceAccess` and `[orchestrator]` limits; token expiry and revocation through `acpd revoke`; interaction with `[orchestrator] enabled` and `peer_rights`; threat model (hosted agents on HTTPS redirects were a risk T3 called out). Ask me to review before implementing anything.

### 6. Remote access routes (design and ADR only, then stop)

Gap: T3 supports direct, Tailscale, SSH, and a hosted relay, with an ordered route list per machine, a descriptor check before a credential is sent to a LAN address, and routes "learned" from a connected server. Ours needs the user's own TLS address.

Write `docs/adr-remote-routes.md` comparing: (a) a documented Tailscale Serve and Cloudflare Tunnel recipe plus an `acpd doctor` check for the chosen URL, (b) several saved addresses per host in the desktop and Android hosts store, tried in order with a check that the address answers as the same host id (`security.rs` `host_id`) before any token is sent, and (c) a hosted relay. State what each would require of `docs/security.md`. Recommend (a)+(b) for this branch and record why (c) is deferred. For (b), list exact changes in `desktop/src/Hosts.tsx`, `desktop/src/Pairing.tsx`, and the Android `HostApi.kt`/Room schema (schema 3, additive migration). Do not implement.

### 7. Small ACP gaps (optional, after items 1 to 4)

Only where the agent advertises it, and confirm against a live `goose acp` first (it advertises `sessionCapabilities` list, delete and close; no resume): call `session/close` before killing the agent process on session removal, falling back to today's kill; advertise `session.configOptions.boolean` and render boolean options as switches in the desktop and phone option pickers; add `session/list` as a read-only "Earlier sessions of this agent" view in the composer, filtered to the allowed workspace roots and opened with `session/load`. Do not enable any `unstable_*` schema feature.

## Constraints

- Preserve every invariant in `AGENTS.md`: literal argv, no shell interpolation, TLS for remote listeners, pairing codes one-use and 60 s, no secrets in logs/saved state/reports, consent for writes and terminals, bounded content and cancellation, host-owned agent list, workspace containment.
- Use isolated fixtures and temp directories only. Do not touch my paired hosts, saved sessions, MCP definitions, or Windows system settings. Do not run commands against a real GitHub repository; `gh` is mocked.
- Keep dated notes short. When an item is finished and verified, move it to `TODO_DONE.md` with exact checks and limits; keep open work in `TODO.md`. Do not claim device or real-agent verification you did not do.
- Commit locally on this branch. Do not push, open a PR, merge, or change global git config without asking me.
- When done or blocked, report: what changed (file links), checks that passed/failed/skipped, what is unverified, and the next useful action.
