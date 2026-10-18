# ADR: start tasks from GitHub Issues and Linear

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Superset's Tasks view shows Linear, GitHub Issues and its own tasks as a list or board, with "Run in workspace" starting an agent in a new workspace. The Copilot app can start sessions from open issues, and GitHub's Linear integration (reported generally available on 23 July 2026, from one source) lets a person assign a Linear issue to Copilot like a teammate. Linear's February 2026 changelog added a deeplink that opens a prefilled prompt, with the issue id and context, in Claude Code, Codex, Cursor and Copilot, and Linear has an MCP server. See [research notes](adr-ade-index.md#research-notes).

This project can create tasks and pull requests (`api/pr.rs`, using the `gh` CLI) but a task always starts from typed text. There is no way to say "work on issue 123" and have the issue body, labels and linked context become the prompt, then link the resulting PR back.

## Decision

Phase one is GitHub Issues through `gh`; Linear is a deep-link import. No tracker credentials are stored in this project.

1. **Browse**: `GET /v1/issues?repo=&state=open&label=` (phone role) runs `gh issue list --json …` in the project's repository and returns a bounded list (number, title, labels, assignee, updated). Uses `gh`'s own sign-in, like PR creation.
2. **Start from an issue**: a "Work on this" action reads `gh issue view --json title,body,labels,comments` (bounded: body 16 KiB, last 10 comments 4 KiB each), builds a prompt where the issue text is inside a fenced block labelled as untrusted issue content, then creates a task in a session worktree on a branch named from the issue number. The person sees and can edit the prompt before it is sent.
3. **Link back**: the task record stores `{tracker: "github", repo, number}`. When the PR is opened ([PR creation exists](adr-pr-watch.md) and the PR body gets `Closes #N`), the issue and PR are linked. The host posts no comments to the issue on its own.
4. **Linear (import only)**: the apps accept `acportal://new-task?...` and a pasted Linear issue URL or deeplink payload, show the text for review, and create a task. No Linear API token is stored and the host never calls Linear. A Linear MCP server can still be configured as an agent MCP definition under existing rules.
5. **Board hook**: tasks with a tracker link show the issue number and state on the attention board ([adr-attention-board](adr-attention-board.md)).

## Options Considered

### Option A: GitHub via `gh`, Linear via import (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. Two routes, a prompt builder, a deep-link handler. |
| Cost | None. |
| Scalability | Bounded lists. |
| Team familiarity | Same `gh` runner as PR creation. |

**Pros:** no new secrets; makes issue-driven work possible; fits the existing PR flow.
**Cons:** no Linear browsing; depends on `gh`; issue text is untrusted input to an agent.

### Option B: native Linear and GitHub API clients with stored tokens

**Pros:** richer browsing and two-way sync. **Cons:** the host would store third-party tokens; far more surface.

### Option C: let the agent fetch the issue through an MCP server

**Pros:** no host code. **Cons:** the person never reviews what the agent will read, and no task or PR linking.

## Consequences

- Prompt-injection risk: issue and comment text can contain instructions. The fenced, labelled block and the review step reduce but do not remove this; the same reasoning as [adr-pr-watch](adr-pr-watch.md).
- Tests: a fake `gh` returning an oversized body; fence handling when the body contains triple backticks; branch-name sanitising.
- The `acportal://` scheme must be registered by the desktop installer and the Android manifest; both treat it as untrusted input.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Finish the real `gh` check from TODO first.
3. [ ] Host: routes, prompt builder, task link field.
4. [ ] Desktop and Android: issue list, Work on this, deep-link import.
