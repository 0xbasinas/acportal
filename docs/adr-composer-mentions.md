# ADR: a smarter composer (slash commands, @ mentions, next-prompt suggestions)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

ACP agents advertise slash commands with `available_commands_update`. In this repository the Android session reducer stores them (`SessionReducer.kt` and its test) and the host logs the update name in frame metadata, but a keyword search of the Android screens and the desktop sources found no command picker. The Codex app has skills in its `@` menu, app and file `@` mentions in the automation composer, and, as of 9 October 2026, a beta that suggests your next message after Codex responds (Tab accepts it as an editable draft; it is never sent automatically). Zed's 1.x notes mention not all Claude Code slash commands working in its agent window yet. Warp and Nimbalyst emphasise fast prompting. See [research notes](adr-ade-index.md#research-notes).

Typing a command name from memory, or a long path on a phone keyboard, is the part of prompting that is hardest on a small screen.

## Decision

Add three composer helpers, each client-side and each filling the draft. None sends anything.

1. **Slash commands.** Typing `/` at the start of the draft shows the commands the *current session's agent* advertised (name, description, hint for input). Choosing one inserts `/name ` into the draft. Commands are agent data; the picker shows nothing for agents that advertise none, and the app never invents commands. Commands that need an argument show the agent's input hint as placeholder text.
2. **@ mentions.** Typing `@` opens a picker over:
   - files and folders in the session workspace (from a bounded host listing that respects workspace containment and `.gitignore`),
   - tasks and sessions (to reference "the result of task X"),
   - skills found by [adr-agent-skills](adr-agent-skills.md) when that ADR exists.
   Choosing a file inserts its relative path as text. Attaching file *contents* remains the existing attachment control with its limits; a mention never reads a file into the prompt by itself. Mentions are plain text so any agent understands them.
3. **Next-prompt suggestions (optional, off by default).** After a turn ends, the composer can show up to three suggested follow-ups that are **deterministic**, not model-written: "Run the checks" (if configured), "Show the diff", "Open a pull request" (if the task is ready), "Continue" (if the agent stopped on a limit). Tapping fills the draft or opens the screen. A model-written suggestion (like Codex's) would need a separate model call and a data-handling decision, so it is not included here.
4. **History and snippets.** The prompt history sheet gains pinned prompts and `Save as recipe` ([adr-workflow-recipes](adr-workflow-recipes.md)). Pinned prompts are local drafts-level data.
5. Accessibility: pickers are lists with standard selection semantics, reachable with a keyboard, and measured at font scale 2.0 like the other sheets.

## Options Considered

### Option A: agent-advertised commands, plain-text mentions, deterministic suggestions (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A picker component used three times, one bounded host listing route. |
| Cost | None. |
| Scalability | Listings are paged and capped (500 entries). |
| Team familiarity | The reducer already carries the commands; file listing reuses folder browse code. |

**Pros:** removes the slowest part of prompting on a phone; accurate because the data comes from the agent and the workspace.
**Cons:** the quality of commands depends on the agent; mentions are only text so an agent may ignore them.

### Option B: model-written next-prompt suggestions

**Pros:** more relevant. **Cons:** cost, latency and sending conversation content to a model; not for the first cut.

### Option C: a host-side prompt-rewriting layer

**Pros:** consistent across agents. **Cons:** hidden changes to what the user typed; against the transparent-prompt rule.

## Consequences

- The workspace listing route returns paths only, bounded, never content, within authorised roots.
- Tests: `/` picker shows nothing when no commands are advertised; commands update when the agent re-sends them; `@` listing refuses a path outside the roots and hides ignored files; suggestions never send.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Check what the desktop does with `available_commands_update` (only the Android reducer was found).
3. [ ] Clients: slash picker first (smallest and uses data already stored).
4. [ ] Host: bounded workspace file-listing route; clients: `@` picker.
