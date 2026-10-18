# ADR: fork a conversation, and edit an earlier message

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Claude Code's rewind can roll back code, the conversation, or both, and forking makes a separate session id so another direction can be tried without overwriting the original. Reports disagree on details (one issue says `/rewind` resets a branch pointer inside the same file and creates no new session). After a conversation restore, the original prompt is put back in the input box for editing. The Codex app has chat forking (the changelog mentions fixing it for WSL repositories), and Cursor's `/best-of-n` runs one prompt several ways. See [research notes](adr-ade-index.md#research-notes).

In ACP, `session/fork` is a draft RFD: it creates a new session from an existing one with `cwd` and `mcpServers` like `session/load`, so the old conversation serves as context (for example for a summary or PR description) without adding to the original's history; the capability is `session: { fork: {} }`. A fork point by message id is described as future work. Message ids themselves are completed (optional in v1). See [adr-acp-capabilities-roadmap](adr-acp-capabilities-roadmap.md).

Here, [adr-turn-checkpoints](adr-turn-checkpoints.md) restores *files*; it deliberately does not rewind the agent's conversation and says so. A person who restores a checkpoint then has an agent that still believes its earlier edits exist. A fork closes that gap when the agent supports it.

## Decision

Offer **Fork session** when, and only when, the agent advertises `session.fork`; add **Edit and resend** as a client-side convenience that never rewrites history.

1. **Fork from here**: an action on a session (and, once message ids and a fork point exist in the protocol, on a specific message). The host calls `session/fork` with the same `cwd` and MCP servers, creates a new host session record linked to its parent (`forked_from`), and opens it. The original session is untouched.
2. **Files**: by default the fork shares the working tree with the original, which is only safe if the original is idle. The dialog offers "Fork into a new worktree" (default when the original has uncommitted changes), which uses the existing worktree code and, if [adr-turn-checkpoints](adr-turn-checkpoints.md) exists, a checkpoint as the starting state.
3. **Additional directories**: not inherited by the protocol unless the agent can prove it; the host passes the original session's list explicitly (the RFD says roots are not restored implicitly).
4. **Edit and resend (no protocol support needed)**: tapping a past user message offers "Edit as new prompt". It copies the text into the composer (the draft), and optionally forks first. It never alters the displayed or stored history of the original. This matches how rewind puts the prompt back for editing, without claiming the agent forgot anything.
5. **Without `session.fork`**: the Fork action is replaced by "Continue in a new session" from [adr-context-handoff](adr-context-handoff.md), which starts a new session with a reviewed note instead of the real transcript. The UI states which of the two it is.
6. The session tree: forks appear grouped under their parent in the list, with the parent shown on the detail screen. Deleting a parent does not delete forks.

## Options Considered

### Option A: protocol fork when advertised, edit-and-resend always (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A link field, a dialog, a list grouping. |
| Cost | A new agent session per fork. |
| Scalability | Bounded by `max_sessions`. |
| Team familiarity | Same creation path as `session/new` and load. |

**Pros:** honest about what carries over; uses the standard method; leaves history intact.
**Cons:** draft RFD, so few agents will support it at first; forks cost memory (see [adr-idle-suspend](adr-idle-suspend.md)).

### Option B: rewrite the stored history and replay it to a fresh agent

**Pros:** works with any agent. **Cons:** the host would have to replay prompts to a new agent, which breaks "never resend prompts" and does not reproduce the agent's tool state.

### Option C: only checkpoints and the handoff note

**Pros:** nothing depends on a draft. **Cons:** the agent's belief state diverges after a restore.

## Consequences

- The record stores `forked_from` only; no conversation content is copied by the host.
- Tests with the mock agent (which needs `session/fork` added): a fork gets a new id and the parent's history is unchanged; the capability gate hides the action; forking with different worktree state.
- Do not build until a real agent advertises the capability or the RFD completes; Edit-and-resend can ship first.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Ship Edit-and-resend on both clients.
3. [ ] Add `session/fork` to the mock agent behind the unstable list.
4. [ ] Check which registry agents advertise it.
