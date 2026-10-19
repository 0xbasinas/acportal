# ADR: data lifecycle: retention, export redaction and backup

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Codeg offers encrypted backups with configuration sync. Nimbalyst syncs sessions, titles, models, status and drafts across devices and lets a person choose which projects to sync. Bellows keeps team audit history for 90 days with CSV and JSONL export and states that its audit holds sanitised metadata, not code. Several tools mention that compaction and summaries are opaque. The research also shows the privacy concerns people raise about transcripts and about skills with credentials. See [research notes](adr-ade-index.md#research-notes).

Here, data lives in several places with separate rules: Android Room (sessions, history capped at 1 MiB per session, drafts, hosts), DataStore settings, Keystore ciphertext for tokens and MCP definitions; the desktop's own store; the host's state directory (security file, catalog, task records, worktrees, logs). There is a message-cache preference on Android and a transcript export (`TranscriptExport.kt`, desktop `transcripts.ts`). A keyword search found no age-based retention or purge anywhere, and no way to back up and restore host configuration and rules. As this ADR set grows (recipes, rules, audit, hooks, automations, checkpoints), the amount of valuable and sensitive state grows too.

## Decision

Three small, separate controls, each with a preview before anything is deleted.

1. **Retention (per device and on the host).** Settings to remove, after N days (never by default), finished sessions' saved history, finished task records and worktrees ([adr-warm-worktrees](adr-warm-worktrees.md) stale worktrees follow the same rule), checkpoints ([adr-turn-checkpoints](adr-turn-checkpoints.md)) and content-capture frames. A "Review what will be deleted" screen lists counts and sizes before the first run, and deletion is logged (counts only). Pinned sessions are exempt. The audit trail ([adr-audit-and-trace](adr-audit-and-trace.md)) keeps its own, longer retention.
2. **Redaction on export.** Transcript export and handoff files ([adr-context-handoff](adr-context-handoff.md)) pass through a redaction step with a preview: strings that look like private key blocks, `Authorization`/`Bearer` values, `.env`-style assignments with long values, cloud key prefixes, and any value in the current `env` of the session's agent definition. The preview highlights matches and the person can unmask a line. It is a safety net, not a guarantee, and the export screen says so.
3. **Backup and restore of configuration (host).** `acpd backup --out file` writes one archive containing config, the agent registry, rules ([adr-approval-rules](adr-approval-rules.md)), recipes, hooks, automations and workspace policy, encrypted with a passphrase chosen at backup time (an authenticated cipher with a memory-hard key derivation; library to be chosen after review). It excludes device tokens and the pairing store (devices must pair again), provider keys, logs, task worktrees and session content. `acpd restore file` shows a diff of what changes and applies it atomically, keeping the previous files. Android and desktop can trigger these through the existing owner-only routes; the archive moves as a file the person saves.
4. **Phone and desktop local data**: "Clear saved conversations", "Clear drafts" and "Forget this host" gain a size shown beside them. Their behaviours do not change.
5. Nothing in this ADR syncs data between devices automatically. Sync is a separate decision (the roadmap's shared host settings item covers a host-side version).

## Options Considered

### Option A: retention, redaction and an encrypted configuration backup (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Three modules; the backup cipher choice needs care. |
| Cost | Archive size is small (configuration only). |
| Scalability | n/a |
| Team familiarity | Retention reuses existing delete paths; backup is new. |

**Pros:** answers the obvious "what happens to all this data" questions; makes moving to a new machine possible without leaking tokens; small defaults that change nothing until chosen.
**Cons:** a custom archive format and passphrase handling are new security-sensitive code; redaction can be bypassed by unusual secrets.

### Option B: cloud sync of settings and sessions

**Pros:** seamless. **Cons:** needs a service and a data-handling statement; see [adr-outbound-relay](adr-outbound-relay.md) for the same concerns.

### Option C: leave it to filesystem backups

**Pros:** nothing to build. **Cons:** device tokens and secrets travel with a disk copy; sessions never expire.

## Consequences

- `docs/security.md` gets a short data inventory: what each store holds, how long, who can delete it.
- Tests: retention never deletes pinned or running items; a backup contains no token bytes (search the decrypted archive for a seeded token); restore refuses a tampered archive; redaction matches a set of fixture secrets and ignores ordinary base64.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Choose the archive cipher and KDF with a review.
3. [ ] Host: retention jobs with preview, backup and restore commands.
4. [ ] Clients: retention settings, redaction preview on export.
