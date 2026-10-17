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
