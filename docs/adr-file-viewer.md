# ADR: a read-only file explorer and viewer

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Mobvibe has a session-scoped file explorer with code preview, a Tree-sitter outline and an `@` file picker. AgentDeck can browse files, Git changes, diffs and history from the phone, and preview HTML pages and charts the agent produces. Superset has a built-in diff and file editor. Threadlines keeps project files open next to the conversation. Zed's multibuffer review lets changes be accepted or rejected across files. Codeg renders `.pptx`, `.docx` and `.xlsx` files live. See [research notes](adr-ade-index.md#research-notes).

On the phone, the only way to see a file the agent mentioned is to ask the agent to print it, or to open its diff. The app has a folder browser for choosing workspaces (`GET` browse routes in `api.rs`) and a received-content viewer for media the agent sends (`ReceivedContentView.kt`), but no route that reads an arbitrary workspace file for display. The ACP filesystem callbacks (`fs/read_text_file`) are for agents, not for clients.

## Decision

A **read-only** explorer inside authorised roots, scoped to a session's workspace, for viewing only.

1. Host routes under the session: `GET /v1/sessions/{id}/files?path=` lists a directory (names, sizes, modified times, a type hint, `ignored` flag from `.gitignore`), and `GET /v1/sessions/{id}/files/content?path=&range=` returns bounded text (default 256 KiB, line ranges) or a refusal for binaries with a size and type. Both use the same containment as filesystem callbacks: canonical paths inside the session's roots, no following symlinks out, fail closed. Files over the limit return the first and last ranges with a visible "truncated" marker, consistent with the project's truncation rule.
2. **Secrets guard**: a short deny list for obvious secret files (`.env*`, `*.pem`, `*.key`, `id_*`, `.git/config` with credentials) returns "hidden by policy" and is on by default; the owner can allow specific files per workspace. This matters because a paired phone otherwise reads everything the agent can.
3. Clients show: a tree with changed files marked from git status, a file view with syntax highlighting (the Markdown viewer's code styling and a bounded highlighter), line numbers, copy line range and **Add to message** ([adr-context-attachments](adr-context-attachments.md)). No editing in this ADR. "Open in editor" stays desktop-only ([adr-editor-handoff](adr-editor-handoff.md)).
4. An outline for supported languages and file search by name are later steps; they would use ripgrep-like name matching on the host with a bound on results, not a parser in the first cut.
5. **Rich files** (Office documents, PDFs, images): images use the existing bounded media viewer; others show metadata and a download-disabled note. Rendering Office formats is out of scope.
6. Guest and observer rights ([adr-session-sharing](adr-session-sharing.md), [adr-device-roles](adr-device-roles.md)) may read files only at `observer` or above and only within the session's roots; the secrets guard still applies.
7. Reads are audit events with path (not content) for non-owner devices ([adr-audit-and-trace](adr-audit-and-trace.md)).

## Options Considered

### Option A: bounded read-only routes with a secrets deny list (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. Two routes reusing containment code, a tree and viewer on each client. |
| Cost | None. |
| Scalability | Paged listings, bounded reads. |
| Team familiarity | Containment and browse code exist. |

**Pros:** removes a daily phone annoyance; read-only, so low risk; reuses the exact containment rules.
**Cons:** widens what a phone token can read; hence the deny list and rights checks; highlighting adds client code.

### Option B: ask the agent to show files

**Pros:** nothing new. **Cons:** costs tokens and an agent turn for a file read.

### Option C: a full editor

**Pros:** powerful. **Cons:** writes would bypass the agent write consent flow; a separate, larger decision.

## Consequences

- `docs/security.md`: a phone with `operator` rights can now read any file inside the session's roots except the deny list; this is new surface and is called out.
- Tests: symlink and `..` escapes refused; deny-list matches; truncation marker; binary detection; observer rights.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: routes and tests reusing the filesystem containment helper.
3. [ ] Desktop: dock panel; Android: Files screen from the session menu.
