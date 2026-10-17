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
