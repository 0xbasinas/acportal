# ADR: "add to next message" from anywhere, with image annotation

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Threadlines lets you attach selected terminal lines directly to your next message, attach selected agent replies or code to the next message, and send the exact browser page state back to the agent. Harnss has an annotation tool for freehand drawing and highlighting on a screenshot before sending, embedded-browser page context passed to the agent, and attachments of images. Intelligent Terminal (Microsoft) shows a status-bar indicator on a failed command and loads the error context into the agent pane on request. Pilion Browser gives agents control of its tabs. See [research notes](adr-ade-index.md#research-notes).

This project has the pieces in separate places: the desktop browser's element picker and *Send to agent* (selected text, element), attachments on the composer (text, images, limits of four attachments, 256 KiB per file), a terminal dock, diff views, tool output cards and the preview screenshot on Android. Each has its own "send" path. There is no common way to collect several pieces of context and review them before one message.

## Decision

Introduce **context chips**: a tray above the composer that holds pieces of context collected from anywhere in the app, reviewed together and sent as part of one prompt.

1. **Sources** (an "Add to message" action on each): selected terminal lines (with the command that produced them when known), a diff hunk or a file line range, a tool call's output, an agent message or a selection from one, the browser's picked element or selected text or page snapshot (existing), a preview screenshot (existing on Android), and the last failed check's output.
2. **A chip** shows its source and size (for example "terminal: 38 lines, 2.1 KiB"), can be opened to read the full text, edited down or removed. The tray is part of the draft, so it survives screen changes and is stored with drafts.
3. **Sending**: chips become fenced blocks in the prompt, each labelled with its source and, for files, path and line range, in the order shown. Images use the existing image attachment path and the "agent does not accept images" check. Existing limits apply to the total; going over a limit marks the offending chip rather than silently dropping it.
4. **Annotation**: any image chip can be opened in a small editor with freehand pen, arrow, box and text, undo, and a crop, producing a new PNG within the existing size limits. Local only; the original is kept until sent. Android uses a Compose canvas; desktop uses a canvas element.
5. **Ask about an error**: on a terminal tab whose last command exited nonzero, a one-tap "Ask the agent" adds the command, exit code and the last 60 lines as a chip and focuses the composer. It never sends.
6. Redaction hint: before sending, chips that contain strings shaped like secrets (private key headers, `Authorization:` values, `.env`-style assignments with long values) show a warning icon so the person can edit them out. This is a hint, not a guarantee.

## Options Considered

### Option A: a chip tray in the composer, one send path (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A tray component used by several sources, an annotation editor. |
| Cost | None. |
| Scalability | Bounded by attachment limits. |
| Team familiarity | Builds on existing attachment and draft code. |

**Pros:** one mental model for supplying context; supports multi-source prompts, which phones need most; makes review before send natural.
**Cons:** touches many screens; chips can bloat prompts, hence the visible sizes.

### Option B: keep source-specific Send to agent buttons

**Pros:** nothing new. **Cons:** each source sends its own prompt immediately, so a person cannot combine terminal output with a screenshot in one message.

### Option C: let agents pull context through a host MCP tool

**Pros:** no UI. **Cons:** the person does not choose or see what the agent reads; see [adr-agent-browser-tools](adr-agent-browser-tools.md) for the controlled version.

## Consequences

- The existing *Send to agent* actions become "Add to message" with an optional "Send now" for compatibility.
- Chips and annotated images are prompt content: stored with drafts, never logged.
- Tests: chip ordering in the built prompt; limit handling; a failed-command chip content; annotation produces a bounded PNG; secret-shaped content shows the warning.
- Accessibility: the tray and editor must work with TalkBack and at font scale 2.0; the annotation editor needs a non-drawing alternative (text description field).

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Desktop: tray, terminal selection and failed-command sources first.
3. [ ] Android: tray; diff and tool-output sources.
4. [ ] Annotation editor on both after the tray lands.
