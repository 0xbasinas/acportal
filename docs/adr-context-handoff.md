# ADR: context meter, and "continue in a new session" on this or another host

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Long sessions degrade as the context fills. A deep dive on compaction reports that Claude Code summarises older history near its limit (so early instructions can disappear if they exist only in chat), and that Codex first reuses structured session memory and otherwise calls a server-side compaction endpoint that returns an encrypted blob a user cannot read. The same source says developers often know better than the tool when it is safe to summarise. Third-party memory servers (for example agentmemory, a self-described benchmark winner I could not verify) try to carry knowledge between sessions. Cursor 3 moves an agent between local and cloud, Codex remote connections keep the selected computer when it reconnects, and Claude Code supports rewind. See [research notes](adr-ade-index.md#research-notes).

ACP agents send `usage_update` with `used` and `size`, which this host already receives ([adr-usage-dashboard](adr-usage-dashboard.md) records it). There is no "I am about to run out" warning, and no way to start a fresh session that carries the useful state, on this host or another one that has a faster machine or the right toolchain.

## Decision

0. **Compaction is the agent's act, shown by us.** ACP's draft compaction RFD has the *agent* send `compaction_update` (with `compactionId`, `status` of in progress, completed, failed or cancelled, and an optional summary) and optional summary chunks; clients only display them and cannot trigger compaction. Once a client advertises `clientCapabilities.session.compaction`, the timeline shows a compaction card with the summary expandable, so a person can see what the agent kept. This ADR's handoff note is the *client-side* alternative when the person wants to start fresh (see [adr-acp-capabilities-roadmap](adr-acp-capabilities-roadmap.md)). Where the agent supports `session/fork`, see [adr-conversation-fork](adr-conversation-fork.md).
1. **Context meter**: the session header shows `used / size` where the agent reports it, with warning styles at 80% and 95% and the words "reported by the agent". If it reports nothing, the meter is absent.
2. **Continue in a new session** (action in the session menu and at the 80% warning): the host builds a **handoff note** from state it can state with certainty, with no model call:
   - the original task prompt and the last N user prompts (bounded),
   - the branch and worktree path, a `git status` summary and the list of changed files,
   - the last check results (checks runner (`api/checks.rs`)),
   - pending or declined approvals,
   - the agent's own last message, shown as "the agent's last message".
   The person sees the note, can edit it, and it becomes the first prompt of a new session in the same worktree. Nothing is sent until they confirm. The old session stays open and readable.
3. **Optional agent-written summary**: a button asks the *current* session for "a short handoff summary of decisions, open problems and next steps", shown for review and pasted into the note. It costs one prompt and is labelled as the agent's words.
4. **Another host**: if the worktree has a pushed branch (a push is an explicit, existing phone-confirmed action), the note offers "continue on host B": the other host checks out that branch into a new worktree and starts a session with the note. Without a remote branch, the option is hidden; no files are copied between hosts by this feature. Peers use the existing peer bridge when the hosts are paired as peers, otherwise the desktop or phone drives both hosts, since it already holds both connections.
5. **Export as a file**: the note can be saved as a single `.acp-handoff.json` file (note text, branch and base commit, changed-file list, check results, no tokens and no host address) so a teammate on another machine can import it into their own host and continue with their own agent. Import shows the whole note for review and creates nothing until confirmed; it never copies files, so the teammate needs the branch. Bellows offers the same idea as handing a session over as one file.
6. Agent-side conversation state is *not* moved. A new session on another agent or host starts with the note, not with the old transcript. The screen says so.

## Options Considered

### Option A: meter plus a reviewed handoff note, optional agent summary (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A builder, a review screen, optional push and checkout. |
| Cost | Zero without the summary, one prompt with it. |
| Scalability | Per session. |
| Team familiarity | Task, worktree and git runner are present. |

**Pros:** works across agents; transparent about what carries over; useful before compaction hurts.
**Cons:** a note is lossy compared with the full transcript; nothing resumes the agent's own memory.

### Option B: replay the whole transcript into the new session

**Pros:** nothing lost in theory. **Cons:** uses the context you were trying to escape.

### Option C: a cross-session memory service

**Pros:** knowledge persists. **Cons:** third-party storage of conversation content or a local model; a separate ADR if wanted.

## Consequences

- The handoff note includes prompts, which are already stored on the device; it is never logged.
- Tests: the note builder with a missing branch, an oversized prompt history and a dirty worktree; the cross-host option hidden without a pushed branch.
- `session/load` is not used; "load" only restores the same agent's own session and is unrelated to this feature.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Desktop and Android: context meter from `usage_update`.
3. [ ] Host: handoff note builder; New session pre-filled from it.
4. [ ] Cross-host continue after [adr-remote-routes](adr-remote-routes.md) is decided.
