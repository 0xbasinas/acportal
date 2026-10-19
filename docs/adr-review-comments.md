# ADR: inline review comments on diffs, sent back to the agent as one batch

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Orca's description includes dropping comments on any diff line and shipping them back to the agent. The 2026 orchestrator roundups say the review surface usually offers side-by-side diffs, inline comments and one-click commit or PR. Conductor is known for a strong diff viewer. See [the competitor notes](adr-ade-index.md#research-notes).

This project has diffs in several places: the session timeline (`DiffBlock` in `desktop/src/acp.ts`), the Git view (`Git.tsx`, `gitparse.ts`), the Run compare (`Multi.tsx`, `splitDiff`) and Android `ChangesScreen.kt`. The browser can already *Send to agent* an element or text. A reviewer can read a diff but can only react by typing a free prompt that names the file and line by hand.

## Decision

Add **review comments**: a comment is `{path, side, line_start, line_end, text}` against a specific diff the person is looking at. Comments are held on the client until the person sends them.

1. Where: the Git view and Run compare on desktop first; the Android changes screen after.
2. A comment draft is local state, like prompt drafts. It is never sent on reconnect or replay.
3. "Send review" builds one prompt: a short header, then each comment with a fenced excerpt of the lines it refers to (so the agent is not relying on line numbers that may have moved). The host sees an ordinary `session/prompt`; no new ACP method and no host route.
4. The prompt is bounded (the 64 KiB prompt limit already used for delegation). If comments exceed it, the client says how many were not sent and keeps them.
5. After a send, comments are marked sent and kept for the session, so a second review round shows what was already raised.
6. A comment on a worktree diff is anchored to the commit it was made against; if the file has changed since, the client shows it as outdated rather than moving it.

## Options Considered

### Option A: client-side drafts, one structured prompt (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. Selection UI and a prompt builder. |
| Cost | None. |
| Scalability | Per session. |
| Team familiarity | Same as drafts and Send to agent. |

**Pros:** works with every ACP agent; no host or protocol change; cannot approve or write anything.
**Cons:** the agent sees text, not a structured object; drafts are lost with the app unless saved like prompt drafts.

### Option B: host-stored review threads shared between phone and desktop

**Pros:** a comment started on one device continues on the other. **Cons:** a new store and API, and security review of stored code excerpts. Could follow shared drafts later.

### Option C: post comments to a GitHub PR and let the agent read them

**Pros:** durable, visible to teammates. **Cons:** needs a PR and network; pairs well with [adr-pr-watch](adr-pr-watch.md) instead of replacing this.

## Consequences

- Excerpts of workspace files go into the prompt text; they are already visible on the paired device.
- Tests: the prompt builder is pure and unit-tested for ordering, size limit, and multi-line ranges. A Compose and a React test cover selecting a range and removing a draft.
- Large-diff behaviour follows the existing "visible truncation" rule: a comment cannot be attached to a line the viewer truncated.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Shared prompt builder with tests (desktop TypeScript first).
3. [ ] Desktop: gutter control, comment list, Send review in Git and Run compare.
4. [ ] Android: same on `ChangesScreen.kt`.
5. [ ] Optional later: save drafts alongside prompt drafts on the host (the "shared settings" roadmap).
