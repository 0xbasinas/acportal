# T3 Code ideas: what was checked

Read 17 October 2026. "Documented" is a page. "Read in source" is a file that was fetched or opened. "Run locally" is a command on this machine. "Tested" is a project test that passed.

## 1. Open a pull request from the Git view

Decision: `POST /v1/pull-requests` runs the operator's `gh` with a literal argv and the body on stdin. Doctor reports only whether `gh` was found. The desktop form is editable, shows the `gh` line, and stays off when the host has no `gh` or no `pullRequests` feature.

Confirmed from the earlier notes: the flags match `gh pr create` (`--title`, `--body-file -`, `--base`, `--head`, `--draft`). Exit code 4 is authentication required.

Corrected: `gh auth status` on 2.94.0 does contact GitHub (`GetScopes` in `pkg/cmd/auth/status/status.go`). Doctor does not run it. A missing `gh` does not fail doctor. Sign-in is the exit code of `gh pr create`. `already exists` is not its own exit code; it is exit 1 plus that phrase in stderr (`create.go`). An empty remote list is the string `no git remotes` (`context/context.go`). `git check-ref-format --branch` rejects `owner:feature` and a leading `-`, so a cross-fork head is not accepted. On Windows the host will not start a `.cmd`; the test copies an `.exe`.

Sources:

- Run locally: `gh` 2.94.0 (`https://github.com/cli/cli/releases/tag/v2.94.0`), `gh pr create --help`, `gh help exit-codes` (0 success, 1 error, 2 cancelled, 4 authentication required).
- Read in source, 17 October 2026: `https://raw.githubusercontent.com/cli/cli/v2.94.0/pkg/cmd/pr/create/create.go` (already-exists error; `--head` skips fork and push). `https://raw.githubusercontent.com/cli/cli/v2.94.0/context/context.go` (`no git remotes`). `https://raw.githubusercontent.com/cli/cli/v2.94.0/pkg/cmd/auth/status/status.go` (`GetScopes`).
- Run locally: `git check-ref-format --branch` accepts `feature$(whoami)` and rejects `--upload-pack=x` and `owner:feature`.

Tested: `acpd/tests/pull_request.rs` against an isolated host and a fake `gh`. A 20 KiB body returned 200, so the route body limit won over the router's 16 KiB limit. Putting stderr into the JSON made the secret test fail; that change was reverted.

Unverified: the Electron Git screen in both themes. A real `gh pr create` (no write to GitHub). The exact stderr of a repository whose remotes are not GitHub hosts, beyond the `no git remotes` and `no remote` phrases.

## 2. Permission presets in the composer

Decision: `GET /v1/presets?agent=` is read-only. A preset names `readFiles`, `writeFiles` and `terminal`, the same flags the client already sends on `POST /v1/sessions`. It does not change config, widen workspace roots, or answer a consent. There is no per-session switch for auto-approve or shell review, and this item does not add one. The desktop shows the preset first in the access chip, keeps the toggles as Custom, shows the agent's own mode beside the preset, and remembers the last preset id per agent in `localStorage` (`acportal.preset.<id>`). A remembered id that the host no longer offers is ignored.

Confirmed from the earlier notes: ACP has no approval-mode method. Auto-approve and shell review are host config, off until configured. `ownTools` means the host cannot see that agent's own edits.

Corrected: an empty `[auto_approve.<id>]` has `max_risk = none`, so auto-edits is not offered. Shell review is offered only when `rules = true` (`ShellReviewer::new` returns none otherwise). Neither applies to a remote (`ws`) agent: auto-approve's watcher skips remote sessions, and shell review is passed only to the local spawn. When auto-approve is on, supervised does not say that every write asks. It says edits inside a task worktree are allowed once, unless `only_in_worktree` is false, in which case the session folder is covered. Deletes, fetches, commands and paths outside the folder still ask. T3's Full access and its Auto mode are not offered. Auto-review is the shell-review preset, and its text says undecided lines still ask.

Sources:

- Read in source, 17 October 2026: `acpd/src/auto_approve.rs` (`should_watch`, `only_in_worktree` default true), `acpd/src/session.rs` (shell review only on the local spawn; `auto_approve::spawn` after create), `acpd/src/shell_review.rs` (`ShellReviewer::new` requires `rules`), `acpd/src/connection.rs` (`WorkspaceAccess` gates `fs/write_text_file` and `terminal/`).
- Documented, 17 October 2026: https://agentclientprotocol.com/protocol/tool-calls — `session/request_permission` options are allow or reject, once or always. Clients may auto-allow from their own settings. No approval-mode method.
- Read in source, 17 October 2026, not executed: T3 `docs/user/permission-modes.md` at `https://github.com/pingdotgg/t3code` (`git show` of a blobless clone). Supervised asks; Auto-accept edits approves file edits; Auto uses provider review; Full access does not ask. Full access is not copied.

Tested: `acpd/src/presets.rs` unit tests and `acpd/tests/presets.rs` against an isolated host (401 without a token, 403 for a peer, presets follow config, a model URL and an env value are absent from the JSON, `POST /v1/sessions` with a `preset` field does not create a session). Replacing the ceiling intersection (`&&`) with `||` made `a_preset_cannot_turn_a_refused_write_or_terminal_back_on` fail (auto-edits and auto-review appeared). The `&&` was restored before the suite. Desktop vitest covers the chip, a remembered id, an agent mode named Full access leaving the preset flags alone, and switching agents applying the next agent's preset flags.

Unverified: the Electron window in both themes. Saved hosts were not opened. Individual Custom toggles are not restored on the next visit; only a preset id the host still offers is applied, and otherwise Supervised is selected. Android is unchanged.

## 3. Agent profiles

Decision: optional `profileOf` and `profileLabel` on a registry entry. The target must already be in the merged registry, must not itself be a profile, and must use the same `command`. A remote profile must also use the same `url`, because a remote entry has an empty command and the url is the program. Arguments and `env` may differ. Checked when the registry is loaded or an entry is added, and a failed add or a failed later file is rolled back. Removing a base that still has a profile is refused. `GET /v1/agents` adds the two fields through discovery. The desktop composer and Settings list a profile under its base. A profile whose base is missing stays visible on its own. `env` values stay in the operator's registry file. No secret store was added.

Confirmed: T3 keeps several accounts of one Claude or Codex executable by giving each account its own config directory, and routes work by instance so the accounts do not share mutable state (`docs/internals/providers.md`, `docs/user/providers-claude.md`, `docs/user/providers-codex.md`).

Corrected: discovery JSON does not print environment names either. It omits the map. `GET /v1/agents/{id}` still returns the saved `env` for an agent the phone added, so the edit form can write it back. That route is not the list.

Sources, 17 October 2026, read with `git show` and not executed:

- https://github.com/pingdotgg/t3code/blob/main/docs/internals/providers.md
- https://github.com/pingdotgg/t3code/blob/main/docs/user/providers-claude.md
- https://github.com/pingdotgg/t3code/blob/main/docs/user/providers-codex.md

Tested: registry parse accepts a profile and an old file without the fields, and rejects a missing target, a self profile, a chain, a different command, and a remote profile with a different url. A later file can add a profile of an agent already loaded, and replacing that agent's command is refused with the previous command kept. `discover()` JSON contains `profileOf` and `profileLabel` and does not contain the fixture env value or the name `OPENAI_API_KEY`. Disabling the same-command check made the different-command fixture parse; the check was restored before the suite. Desktop vitest groups Work under Goose, leaves a profile of a missing base in the list, and Settings renders the profile row.

Unverified: the Electron window. Android decodes `GET /v1/agents` with `ignoreUnknownKeys`, so the new fields do not need an Android change; Android was not rebuilt and does not group profiles. The known-agent catalog (`GET /v1/agents/discover`) still returns template `env` from `known.json` (`GOOSE_MODE=approve`). That is the existing catalog, not the agent list.

## 4. OS service install

Decision: `acpd service install|status|restart|uninstall` registers the current binary to run in the foreground as `<absolute acpd> --profile <absolute launcher.json> start`. Linux writes a systemd user unit, macOS writes a LaunchAgent, and Windows writes a per-user logon task. The definition stores those two paths. It does not store the shell environment, the pairing address, or the launcher name. Install requires a saved setup profile. Restart and uninstall do not. A unit, plist, or task without the marker `acpd-service-marker: acportal` is left in place. Uninstall removes only that definition. Config, state, tokens, and paired devices stay. Self-update is not implemented.

Confirmed: T3 starts a Node supervisor (`apps/server/src/serviceLauncher.ts`) from systemd or launchd and keeps update state in that supervisor (`docs/internals/server-updates.md`). The supervisor stages a runtime, runs a trial, and rolls back to the previous version. That is why this host, which has no separate launcher, only documents the updater and links that note.

Corrected: the service manager here starts `acpd start` directly. `acpd daemon` remains the detached process for the current login. User units want `default.target`; that target is the user session's main target. `Type=exec` is used because `Type=simple` reports the unit started even when the binary cannot be invoked. The `:` prefix stops `$` expansion and does not stop `%` specifiers, so each `%` is doubled. `schtasks` has no description flag; the marker is the XML `Description`, written as UTF-16 LE with a BOM. A query for a missing task named `acpd` on this machine printed `The system cannot find the file specified.` and exited 1. That query was read-only.

Sources, read 17 October 2026:

- systemd.service(5), Debian testing, systemd 262: https://manpages.debian.org/testing/systemd/systemd.service.5.en.html (command lines, `Type=exec`)
- systemd.syntax(7), Debian trixie, published 2026-09-22: https://manpages.debian.org/trixie/systemd/systemd.syntax.7.en.html (quoting)
- systemd.special(7), user `default.target`: https://manpages.debian.org/bookworm/systemd/systemd.special.7.en.html and https://manpages.debian.org/unstable/systemd/systemd.special.7.en.html
- loginctl(1) `show-user` with no user argument: https://manpages.debian.org/testing/systemd/loginctl.1.en.html
- launchd.plist(5), Keith mirror, page date 2026-09-28: https://keith.github.io/xcode-man-pages/launchd.plist.5.html
- `schtasks /create /?` and a read-only `schtasks /Query /TN acpd` on this Windows machine
- https://github.com/pingdotgg/t3code/blob/main/apps/server/src/serviceLauncher.ts (`git show`, not executed)
- https://github.com/pingdotgg/t3code/blob/main/docs/internals/server-updates.md (`git show`, not executed)

Tested: unit and plist generation with spaces, `$`, `%`, quotes, backslashes, and unicode; task XML is least privilege, quotes arguments with `CommandLineToArgvW` rules, and never passes `/TN *`, `HIGHEST`, `/RU`, or `/RP`. Uninstall of a foreign unit, plist, and task makes no disable or delete call. Forcing the systemd marker check to succeed made that test remove the foreign unit (`Removed ...\acpd.service`); the check was restored before the suite. A copy of the test binary named `systemctl.exe` answered `systemctl --user show`. Status text includes the resolved log path and omits the fixture TLS key path, certificate path, pairing address, and launcher name. On this Windows machine, `cargo fmt --all -- --check`, strict Clippy, and `cargo test --locked --workspace --all-targets` passed (exit 0, real Goose trials ignored).

Unverified: a real `systemctl --user enable`, `launchctl bootstrap`, or `schtasks /Create` on a machine. Existing GitHub Actions run `cargo test` and do not install a user service. Lingering was not changed. Desktop and Android were not rebuilt for this item.
