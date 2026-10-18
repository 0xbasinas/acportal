# ADR: one session mode picker (Plan, Interactive, Autopilot)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

The GitHub Copilot app starts every session in one of three modes: *Interactive* (approve each change), *Plan* (the agent proposes a task list first) and *Autopilot* (start to finish without check-ins). Roo Code has personas (Coder, Architect, Debugger) and auto-approval workflows. Kiro separates a spec step from the build step. See [the competitor notes](adr-ade-index.md#research-notes).

Here the same choices exist but are scattered:

- Permission presets, including "Ask for everything" (`presets.rs`, T3 Code ideas in TODO_DONE).
- Host-side auto-approve per agent (`auto_approve.rs`, orchestrator design section 32) and shell auto-review (`shell_review.rs`).
- ACP session modes and config options, which agents advertise and the apps already render as option pickers (`OptionPicker.tsx`).
- Workspace access policy (`WorkspaceAccess`).

A user has to know which of four screens decides how much an agent may do. AGENTS.md also forbids branching on agent names, so the picker cannot be built from product knowledge.

## Decision

Define three **host-owned launch profiles**. A profile is a named bundle that only sets things the host already controls; it never invents agent capability.

| Profile | Permission preset | Auto-approve | ACP mode |
|---|---|---|---|
| Interactive | the operator's default preset | off | the agent's default |
| Plan | Ask for everything | off | a mode whose id or name the operator maps to "plan", if the agent advertises one; otherwise the first prompt is prefixed by a host-configured planning instruction and the profile says so on screen |
| Autopilot | the operator's chosen preset | on only where `[auto_approve.<agent>]` already exists and the workspace is a worktree | the agent's default |

- Profiles live in host config (`[profiles.<name>]`) with the three above as defaults. They are a convenience over existing settings, so changing a profile changes no stored session.
- Each client shows one picker at session creation and in the session header. The header also shows the *effective* state (for example "Autopilot, but approvals still ask: no auto-approve rule for this agent"), taken from the host.
- A session keeps the profile it started with. Switching profile mid-session is a separate action that changes the preset and says so in the timeline.
- Delegated tasks inherit the stricter of the parent's profile and the child's request (already true for Ask for everything).

## Options Considered

### Option A: host-owned profiles over existing controls (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A config table, one resolver, one picker. |
| Cost | None. |
| Scalability | Per host. |
| Team familiarity | Same config and preset code. |

**Pros:** no new permission machinery; the unsafe case (Autopilot) can only use rules the operator wrote.
**Cons:** "Plan" depends on the agent offering a mode; the fallback is only a prompt prefix.

### Option B: a client-only picker that sets the existing controls one by one

**Pros:** no host change. **Cons:** phone and desktop could set different combinations; no single effective-state answer.

### Option C: a new ACP-level capability

**Pros:** clean in theory. **Cons:** needs an RFD and agent support; not available for existing agents.

## Consequences

- `docs/security.md` states that Autopilot adds no permission beyond the existing auto-approve rules.
- Tests: a profile cannot turn on auto-approve without a rule; a delegated child never gets a looser profile.
- Plan mode quality varies by agent. The UI must show when only the prompt-prefix fallback is in use.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: profile table, resolver and effective-state report on session info.
3. [ ] Desktop and Android: picker and header line.
4. [ ] Decide the exact wording of the planning prefix, and whether it may be overridden per agent.
