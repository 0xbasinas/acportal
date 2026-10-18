# ADR: pull requests and issues beyond GitHub (GitLab, Gitea, Forgejo)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Codeg's repository panel handles issues, pull requests, checks and merges for GitHub, GitLab, Gitea and Forgejo. GitKraken's Kepler connects to Jira, Linear, Trello, GitHub Issues and GitLab Issues, plus GitHub Enterprise and self-managed GitLab. Copilot's flow is GitHub only. See [research notes](adr-ade-index.md#research-notes).

This host's pull request support (`api/pr.rs`) runs the `gh` CLI, and [adr-pr-watch](adr-pr-watch.md) and [adr-issue-tracker](adr-issue-tracker.md) both propose building on `gh`. A repository on GitLab, Gitea, Forgejo, Bitbucket or a self-managed GitHub Enterprise server gets none of it. People who self-host a coding-agent bridge are disproportionately likely to self-host their forge as well.

## Decision

Put forge operations behind a small host-side **forge adapter** with the same four operations, and keep each adapter a thin wrapper over the forge's own CLI. No forge API clients or stored tokens.

1. Operations: `pr_create`, `pr_status` (checks and review state), `issue_list`, `issue_view`. Everything else the earlier ADRs need can be built from these.
2. Detection: the repository's `origin` remote URL decides the forge (host name match against `github.com` and a configured list `[forges] hosts = { "git.example.com" = "gitlab" }`). Unknown hosts get no forge features and the screens say "no forge CLI for this remote".
3. Adapters, in order of work:
   - **GitHub** via `gh` (exists). GitHub Enterprise Server uses `gh` with its host argument.
   - **GitLab** via `glab` (`mr create`, `mr view`, `ci status`, `issue list/view`), including self-managed hosts.
   - **Gitea and Forgejo** via `tea` where it covers the operation; otherwise the feature is shown as unavailable rather than falling back to raw HTTP.
4. Each adapter runs the CLI with a literal argument array in the repository directory with a timeout, parses JSON output (`--json`/`--output json`) into one internal shape, and fails with a plain message when the CLI is missing or not signed in. Authentication stays in the CLI's own credential store.
5. `doctor` gains a forge check per configured remote: CLI present, version, signed in (a read-only call), host reachable.
6. Terminology in the apps follows the forge ("Merge request" for GitLab) from a label in the adapter.

## Options Considered

### Option A: thin adapters over each forge's CLI (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium, mostly output parsing and tests with fakes. |
| Cost | None. |
| Scalability | Per repository. |
| Team familiarity | The same pattern as `gh` today. |

**Pros:** no secrets stored; behaviour matches the forge's own tools; incremental per forge.
**Cons:** depends on CLI output stability and on installed tools; `tea` covers less than `gh`.

### Option B: direct REST clients with tokens stored on the host

**Pros:** uniform and complete. **Cons:** the host stores third-party tokens; far larger security surface.

### Option C: GitHub only

**Pros:** nothing to build. **Cons:** excludes a sizeable group of the likely users.

## Consequences

- [adr-pr-watch](adr-pr-watch.md) and [adr-issue-tracker](adr-issue-tracker.md) call the adapter, not `gh`, so they work on GitLab once that adapter lands. Update both when this is accepted.
- Tests use fake CLIs like the existing fake `gh` (a real `gh` check is still open in TODO); a real `glab` check against a throwaway project is required before claiming GitLab support.
- Bitbucket and Azure DevOps are left out; their CLIs differ more.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: adapter trait, move `gh` behind it, tests.
3. [ ] GitLab adapter with a fake, then one real run.
4. [ ] Clients: forge-aware labels.
