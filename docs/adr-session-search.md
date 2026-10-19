# ADR: search across all sessions and hosts

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

The Codex app added Cmd/Ctrl+F that starts from the selected text, a file tree in review, and skills in the `@` menu; Nimbalyst's phone voice mode can "find sessions by topic". Cursor 3 and Warp show every agent in one sidebar. The research found no tool with a good cross-session search, and the observability platforms (Langfuse and similar) offer searchable trace explorers aimed at traces, not at a developer asking "which session changed the retry logic?". See [research notes](adr-ade-index.md#research-notes).

Here, Android has Sessions search (title and summary, per AGENTS.md) and keeps saved history per session in Room; the desktop keeps its own copy. The host holds a bounded journal for live sessions and metadata for interrupted ones (`session_catalog.rs`). No client can search the *content* of past conversations, tool calls or diffs, and nothing spans hosts.

## Decision

Search on each device over data it already stores, and add a host index only for what the host owns.

1. **Phone and desktop: local full-text search** over saved session text, tool titles, file paths touched and prompt history. Android uses an SQLite FTS table next to the existing session rows (additive Room migration, schema 3 to 4). Desktop uses the same fields in its store. Results show the session, time, and a snippet, and open the session at that item.
2. **Scope filters**: host, project, agent, date, "touched file", "has pending approval", "has failed check". Filters use fields the apps already hold.
3. **Cross-host**: the desktop queries every paired host's session list and merges results; the host exposes `GET /v1/sessions/search?q=` only over the metadata the catalog already holds (title, workspace, agent, files touched, summary), never full text. Full text stays on the device that saved it.
4. **Index bounds**: only already-retained text is indexed (the existing 1 MiB history cap on Android applies), so the index cannot grow past what is stored. Deleting a session deletes its index rows; a "clear cached messages" setting clears both.
5. **No semantic or embedding search** in this ADR. Plain tokenised search first; embeddings would need a model call or a local model, which is a separate decision.
6. Search never loads or resumes an agent; it reads saved history only, consistent with "opening saved history must not start an agent".

## Options Considered

### Option A: device-local FTS plus host metadata search (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A migration, an index writer, a results screen. |
| Cost | Extra storage (typically tens of percent of text size). |
| Scalability | Bounded by existing caps. |
| Team familiarity | Room and the host catalog are already in place. |

**Pros:** works offline; no new data leaves a device; consistent with the storage rules.
**Cons:** a session saved on the desktop is not searchable from the phone; two indexes to maintain.

### Option B: a host-side full-text index of everything

**Pros:** one index for all clients. **Cons:** the host would store conversation content it deliberately keeps only in memory; a new sensitive store.

### Option C: ask a model to search

**Pros:** answers by meaning. **Cons:** sends history to a provider; cost; not private.

## Consequences

- Room schema moves to 4 with an additive migration and a migration test, per the storage invariant. The index is rebuildable, so a failed rebuild must not lose sessions.
- Tests: FTS query on a seeded database; deletion removes rows; oversized or unreadable rows (the existing guarded-read cases) are skipped without a crash; no main-thread decoding.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Android: FTS table, migration test, search screen.
3. [ ] Desktop: same fields; merged cross-host results.
4. [ ] Host: metadata search route, bounded.
