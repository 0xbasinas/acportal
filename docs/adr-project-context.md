# ADR: project rules and specs shared across agents

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Kiro (AWS) is described as spec-driven: specs say what to build and hooks say when agents act. Roo Code has personas (Coder, Architect, Debugger) with fine-grained context control. Windsurf's Cascade loads relevant context automatically. See [the competitor notes](adr-ade-index.md#research-notes). My search for Kiro's steering files and for Claude Code hooks and skills returned nothing usable, so those details are not asserted here.

Different ACP agents read different instruction files (this repository itself has `AGENTS.md`). The roadmap lists "shared settings on the host" (MCP definitions with masked secrets, prompt history, drafts), but not project instructions. An operator running three agents on one repository maintains three files by hand, and a phone user cannot see which instructions a session started with.

## Decision

Treat project instructions as **visible, optional host-managed context**, never as hidden injection.

1. Discovery only: for a workspace, the host lists the instruction files it finds from a closed list (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `.github/copilot-instructions.md`, `.cursor/rules/*`). Read-only, bounded to 64 KiB each, inside the workspace, using the same containment as filesystem reads.
2. Show them in the New session screen and the session header as "Instructions: AGENTS.md (read by this agent?)". Whether an agent reads a given file is not known to the host, so the label says "present", not "used".
3. **Common rules** (optional): one host-stored markdown file per project (`[project."<root>"] rules`). When enabled and the agent advertises ACP's draft `systemPrompt` session capability, the host sends it as `systemPrompt {mode: "append", content: [text]}` on `session/new` (and again on `load`, `resume` and `fork`, as the RFD asks); `override` is never used. Without the capability, the host adds it to the first prompt of a session as a clearly delimited block the timeline shows in full. Either way the text is shown to the person. It is off by default and can be turned off per session.
4. **Refresh after compaction**: Agent Kanban re-injects an `AGENTS.md` sentinel on every agent turn to keep long sessions on track. Here, when a compaction completes ([adr-context-handoff](adr-context-handoff.md)), the session offers "Re-send pinned instructions" (the common rules and any instruction file the person pinned) as one explicit prompt. It is never automatic.
5. **Specs**: a spec is a markdown file the person picks (or the agent writes) in `specs/` or `docs/`. Phase one adds only a *Start from this spec* action that sends the file's content as the first prompt, with the profile Plan ([adr-session-modes](adr-session-modes.md)). No spec parser, no task generation, no hooks.
6. No agent-specific branches: the list of filenames is data, and the label never names how an agent behaves.

## Options Considered

### Option A: discovery, one optional shared block, start-from-spec (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low. Listing, one prompt block, one action. |
| Cost | None. |
| Scalability | Per workspace. |
| Team familiarity | File containment and prompt code already exist. |

**Pros:** useful on day one and honest about what is known; no new execution.
**Cons:** does not synchronise the per-agent files; the shared block adds tokens to every first prompt.

### Option B: write and sync instruction files into each agent's own location

**Pros:** one source of truth. **Cons:** the host would edit files in the repository; needs consent, conflict handling and per-agent knowledge, against the protocol-uniform rule.

### Option C: a full spec-driven workflow with hooks (Kiro style)

**Pros:** strongest process. **Cons:** a product of its own; revisit after phase one shows demand.

## Consequences

- Shared rules text is prompt content; it follows the rule that prompts are never logged.
- Tests: containment (a symlink outside the workspace is refused), the size bound, and the block appearing exactly once, never on reconnect or replay.
- Hooks (run something when an event happens) are deferred; they overlap with [adr-workspace-scripts](adr-workspace-scripts.md) and [adr-scheduled-runs](adr-scheduled-runs.md).

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: instruction-file discovery route with tests.
3. [ ] Desktop and Android: show them in New session; Start from spec.
4. [ ] Decide whether the common-rules block is worth building before seeing phase-one use.
