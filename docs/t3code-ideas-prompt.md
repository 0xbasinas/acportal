# Prompt: implement T3 Code-inspired improvements

Copy everything below the line into a new agent session started on branch `experiment/t3code-ideas` (cut from `agent-discovery`).

---

You are continuing the ACP Portal project in `C:\Users\basin\acportal` on branch `experiment/t3code-ideas`. Read `AGENTS.md`, `HANDOFF.md` (current section), `TODO.md`, `docs/security.md`, `docs/architecture.md` and `docs/orchestrator-design.md` first. Continue in place; do not scaffold replacements, upgrade dependencies, or branch UI on agent names.

## Background

We reviewed T3 Code (https://github.com/pingdotgg/t3code, MIT), a local-first control surface for coding agents with desktop, web and mobile clients that all talk to a local server. Its README lists: a relay for remote access (`infra/relay`), background service install plus `t3 update`, multiple accounts per agent (Codex, Claude), an MCP surface so outside agents can drive it, permission modes (Full Access / Supervised) and one-click commit/push/PR. Only the README was read, not the source, so verify any claim before relying on it. Do not copy their code; read `docs/internals/` there only for ideas, and respect their license.

## Work, in this order

Do one item per commit. After each, run the relevant checks from `AGENTS.md` (Rust fmt/clippy/tests; Android Gradle task if Android changed; desktop build/tests if `desktop/` changed) and report what actually passed.

1. **Ship action (commit, push, PR).** Extend the scoped `/v1/git` API and the desktop Git view with a "Ship" flow: stage reviewed files, commit with an editable message, push, and open a PR via `gh` or the configured remote. The title/body are suggested using the existing Suggest feature but always editable. It must go through the existing consent flow (nothing pushes without explicit approval), use literal argument arrays (no shell interpolation), stay within the session worktree scope, and never print tokens. Add Rust tests with a local bare remote; PR creation is mocked.
2. **Permission mode presets.** Add named presets over existing settings (for example `supervised`, `auto-review`, `trusted-workspace`) that map onto per-line consent, `[shell_review]` and workspace policy. Presets must only choose among existing behaviours; none may widen allowed roots or skip consent that is mandatory today. Show the active preset in the new-session composer (desktop first, then phone) with the effective rules visible.
3. **Agent profiles (multiple accounts).** Allow several registry entries of the same agent with different env/config (for example two Claude or Codex accounts) to be presented as profiles in the composer. Store only environment variable names or file paths in config, never secret values; keep secrets in the operator environment. No agent-specific branches in the clients.
4. **Background service install.** Add `acpd service install|uninstall|status` (systemd user unit on Linux, launchd agent on macOS, a per-user scheduled task on Windows) built on the existing `daemon.rs`/`detached.rs` and `acpd setup`. Do not require elevation, do not use a stored PID as termination authority, and make uninstall remove only what install created. Self-update is out of scope; write a design note instead.
5. **Host as an MCP server (design first).** Write an ADR in `docs/` for exposing "list sessions", "get diff" and "delegate task" as an MCP server on the existing authenticated listener, built on the orchestrator and peer bridge. Cover authentication (reuse device tokens; no new credential types), scopes, consent and how it interacts with `[orchestrator] enabled`. Implement only after the ADR is written, and stop to ask me before implementing it.
6. **Remote access (design only).** Write an ADR comparing (a) a documented Tailscale/Cloudflare Tunnel recipe in `acpd setup`/`docs/installation.md` and (b) a relay that forwards only the already-authenticated `acpd.v1` stream. Keep TLS requirements, one-use 60-second pairing codes and certificate validation unchanged. Do not implement a relay in this branch.

## Constraints

- Preserve every invariant in `AGENTS.md`: literal argv, TLS on remote listeners, no secrets in logs/saved state, consent for writes and terminals, bounded content, host-owned agent list.
- Isolated fixtures only; do not touch my hosts, saved sessions or MCP definitions. Do not mutate the Windows system settings.
- Do not merge, push or open a PR without asking me. Commit locally on this branch.
- Verify before claiming. Record real results, what was not run, and any limits in `TODO_DONE.md`; leave unfinished work in `TODO.md`. Keep README/AGENTS short and current, and add a dated entry only for finished, verified work.
- Report back with: what changed, checks that passed or were skipped, what remains unverified, and the next useful action.
