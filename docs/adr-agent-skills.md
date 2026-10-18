# ADR: a host-managed library of Agent Skills

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Agent Skills are directories with a `SKILL.md` file (YAML frontmatter with at least a name and a description, then Markdown instructions), published as an open standard. Reports list support in Claude Code, Codex, GitHub Copilot, Gemini CLI, Cursor, Windsurf, Goose, Amp and OpenCode, but each reads a different folder (`.claude/skills/`, `.agents/skills/`, `.github/skills/`, `.gemini/skills/`, `.cursor/skills/`, `.windsurf/skills/`). Third-party registries claim tens or hundreds of thousands of skills, and one source reports a 13.4% critical-vulnerability rate in scanned skills; both numbers are secondary and conflict between sources. Cursor's marketplace and Claude Code plugins bundle skills, hooks, subagents and MCP servers. The Codex app puts skills in its `@` menu. See [research notes](adr-ade-index.md#research-notes).

This repository has agent definitions, MCP definitions (host-side secrets planned) and project instruction files ([adr-project-context](adr-project-context.md)), but nothing about skills. A keyword search of the host and both clients found no `SKILL.md` handling.

## Decision

Add a read-first, install-second **skills library** on the host.

1. **Discover (read only).** For a workspace and for the home directory, list skills found in the known folders above. Show name, description, location, size, which folder convention it sits in, and "scripts: yes/no" (does the directory contain executables or files referenced as scripts). Bounded count and size; nothing is executed or sent to a model.
2. **Show who can see it.** The host cannot know whether an agent reads a folder, so the label is "present in a folder agent X conventionally reads", taken from a small data table, not from agent-specific code paths.
3. **Install from a path or URL into one chosen folder.** The host copies the directory (no symlinks, size and file-count bounds, no files outside the skill directory), shows the full file list and `SKILL.md` text for review, and writes only after the phone or desktop confirms. Installing is a workspace write and uses the same consent and audit rules as other host writes.
4. **Static check before install.** A bounded scan flags shell scripts, network calls (`curl`, `wget`, URLs), `eval`, base64 blobs, instructions that mention credentials or ask the agent to disable approvals, and files over a size cap. The result is advice, not a verdict; the screen says so. No external scanning service is used.
5. **Cross-install.** "Also install for: Codex, Copilot..." copies the same reviewed directory into each chosen convention folder, so one skill need not be copied by hand into six places.
6. **Remove and update** are explicit, one skill at a time. No automatic updates and no registry sync in the first cut.

## Options Considered

### Option A: discovery, reviewed install, cross-install by copy (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Directory walking, review UI, copy, scan. |
| Cost | None. |
| Scalability | Bounded by the caps. |
| Team familiarity | Reuses workspace containment and consent code. |

**Pros:** makes an invisible, security-relevant surface visible; useful without choosing a marketplace.
**Cons:** the scan can be bypassed by an obfuscated skill; folder conventions keep changing and need upkeep.

### Option B: browse and install from third-party skill marketplaces

**Pros:** more choice. **Cons:** supply-chain risk in a fast-growing ecosystem; a separate trust decision for each marketplace.

### Option C: leave skills to each agent's own tooling

**Pros:** no work. **Cons:** the phone cannot show what a session may be following.

## Consequences

- `docs/security.md`: a skill is instructions plus possibly scripts that an agent may run with the permissions the user already granted; install is a code-trust decision and the screen says so.
- Tests: a skill with a symlink to `/etc` is refused; size caps; cross-install writes exactly the listed files; scan flags a planted `curl | sh`.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Check agentskills.io for the current spec and folder list before coding the table.
3. [ ] Host: discovery route and tests.
4. [ ] Desktop and Android: read-only list first; install flow after review.
