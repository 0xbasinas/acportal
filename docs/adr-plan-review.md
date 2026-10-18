# ADR: plans as reviewable objects

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Mux has Plan and Exec modes and renders Mermaid diagrams so a proposal can be reviewed visually. The Copilot app's Plan mode has the agent propose a task list for approval before it works, and the user can edit, reorder or redirect work on the same surface. Nimbalyst's phone app lets you respond to plan approvals and commit proposals. Claude Code's plan mode is read-only and proposes changes before edits. Kiro puts a spec before the build. See [research notes](adr-ade-index.md#research-notes).

ACP has a `plan` session update: a list of entries with content, priority and status that an agent re-sends as it works. The desktop already handles it: `acp.ts` keeps only the latest plan as one item and `Session.tsx` renders it, with no version history and no actions. I did not check how Android renders it. [adr-session-modes](adr-session-modes.md) decides how a Plan session *starts*; this ADR decides what the person can *do* with the plan an agent produces.

## Decision

Show the ACP plan as a first-class, pinned object, and add a small set of actions that turn into ordinary prompts. Nothing here needs a new protocol call.

1. **Plan card**: pinned at the top of the conversation and in the task view, showing entries with status (pending, in progress, completed) and priority. It updates live from `plan` updates and keeps a history of versions, so a reviewer sees what changed between versions.
2. **Actions**, each producing one reviewable prompt:
   - **Approve and continue**: "The plan is approved. Proceed." Used in Plan mode.
   - **Edit**: the person edits entries (reword, delete, reorder, add), and the edited list is sent as "Use this revised plan:" with the entries as a numbered list.
   - **Comment on an entry**: same mechanism as [review comments](adr-review-comments.md), sent as a batch.
   - **Skip an entry**: sent as an instruction, shown as a note on the entry.
3. **Plan text vs structured plan**: many agents write a plan as Markdown prose, not a `plan` update. If no structured plan arrives, the apps offer **Pin as plan** on a message, which turns a Markdown list in that message into a local plan card (client-side; marked "pinned by you").
4. **Diagrams**: Markdown code fences tagged `mermaid` render as a diagram only on desktop, in a sandboxed, script-free renderer with a size limit, and show source otherwise. Android keeps source in the first cut (its native Markdown parser is bounded and remote content is not fetched). Rendering is a view of text the agent sent; it fetches nothing.
5. **Gate**: in the Plan profile ([adr-session-modes](adr-session-modes.md)), the attention board shows "plan ready, needs your approval" as a `needs_review` item; a task does not move to "working" until Approve is tapped, if the profile asks the agent to stop for approval. The host cannot force an agent to stop, so the card says what the profile asked for, not what is guaranteed.
6. Completed plans stay with the session, are included in the context handoff note ([adr-context-handoff](adr-context-handoff.md)), and can be saved as a recipe ([adr-workflow-recipes](adr-workflow-recipes.md)) by one tap.

## Options Considered

### Option A: render and act on the ACP plan, plus a local pin (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A card, version history, four actions. |
| Cost | None. |
| Scalability | Per session. |
| Team familiarity | Uses existing timeline and prompt code. |

**Pros:** works with any agent that sends a plan or writes a list; pure client behaviour.
**Cons:** an agent may ignore an edited plan; "approval" is a prompt, not an enforced gate.

### Option B: enforce plan approval in the host (block writes until approved)

**Pros:** a real gate. **Cons:** the host cannot tell which tool calls belong to a plan; a Plan profile with Ask for everything gives most of the effect already.

### Option C: leave plans as text

**Pros:** nothing to build. **Cons:** long plans scroll away on a phone.

## Consequences

- Tests: version diffing when entries are added or removed; edit produces the exact numbered prompt; pinned plans survive reload; Mermaid source with scripts or HTML is rendered inert.
- The desktop already renders the latest plan; the card extends that item (history and actions) rather than adding a second one. Android's rendering is unchecked.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Check how Android renders the plan; reuse the desktop's single plan item.
3. [ ] Clients: card, history, actions, Pin as plan.
4. [ ] Desktop: sandboxed Mermaid renderer (check bundle size first).
