# Agentic development environment ideas: index and research notes

**Date:** 18 October 2026
**Status:** All ADRs listed here are *Proposed*. None changes code. Each says "No code from this ADR until they accept it."

## The ADRs

| ADR | Idea | Closest prior art | Builds on |
|---|---|---|---|
| [attention board](adr-attention-board.md) | One "what needs me" view across sessions, tasks and hosts, plus notifications | Superset Workspaces columns, Copilot "My Work" | `/v1/shared`, task views, multi-host desktop |
| [session modes](adr-session-modes.md) | Plan, Interactive, Autopilot as one host-owned picker | Copilot app modes | presets, auto-approve, ACP modes |
| [review comments](adr-review-comments.md) | Inline diff comments sent back as one prompt | Orca | diff viewers, Send to agent |
| [turn checkpoints](adr-turn-checkpoints.md) | Per-turn git snapshots with compare, restore, fork | Cline (unconfirmed elsewhere) | worktrees, Run on several agents |
| [workspace scripts](adr-workspace-scripts.md) | Setup, run, archive scripts and allocated ports per worktree | Conductor scripts and `CONDUCTOR_PORT`, Superset ports | worktrees, terminals, preview, checks |
| [usage dashboard](adr-usage-dashboard.md) | Recorded usage and cost, passive only | Orca usage display | `usage_update`, Run compare |
| [PR watch](adr-pr-watch.md) | CI and review feedback proposed back to the agent | Copilot Agent Merge | `api/pr.rs`, checks |
| [scheduled runs](adr-scheduled-runs.md) | Timed unattended worktree runs with hard limits | Superset automations, Copilot automations | tasks, budgets, service install |
| [registry browser](adr-registry-browser.md) | Browse and add agents from the ACP Registry | Zed and JetBrains registry pickers | Add agent, orchestrator design section 2 |
| [project context](adr-project-context.md) | Show instruction files, optional shared rules, start from a spec | Kiro specs, Roo personas | filesystem containment |
| [container isolation](adr-container-isolation.md) | Optional container launcher for an agent (spike first) | Sculptor, Copilot local sandboxes | launcher, process tree, doctor |
| [editor handoff](adr-editor-handoff.md) | Open a worktree in VS Code, Zed or JetBrains (desktop, local host) | Copilot "Open", Superset | Electron main process |

## Suggested order

1. **Attention board** and **session modes**: they make existing features legible and share one set of states.
2. **Review comments** and **turn checkpoints**: they share the review surface and make auto-approve safer.
3. **Workspace scripts** and **registry browser**: they remove setup friction for new worktrees and new agents.
4. **Usage dashboard** and **editor handoff**: small, independent.
5. **PR watch**, **scheduled runs**, **project context**: depend on earlier work or on a real `gh` check.
6. **Container isolation**: spike only; the largest and most platform-specific.

## Research notes

Read 18 October 2026 through web search and page fetches. Most sources are vendor-adjacent blogs and comparison pages; they disagree in places (agent counts, pricing), so features below are what the pages *said*, not what was tried. Nothing was installed or run. Verify against each product's own docs before copying a behaviour.

- **Conductor** (macOS): worktree per workspace, setup/run/archive scripts, `CONDUCTOR_PORT`, `nonconcurrent` run mode, repository file plus personal override. [scripts docs](https://conductor.build/docs/reference/scripts).
- **Superset**: parallel agents in worktrees with persistent terminals, diff review in-app or in an external editor, port management, in-app browser, remote and cloud workspaces, automations, MCP, a Tasks view over Linear and GitHub Issues with Working, Needs attention, Needs review and Merged columns. Source-available (ELv2). [comparison page](https://superset.sh/compare/superset-vs-vibe-kanban).
- **Vibe Kanban**: board where each card maps to an agent run, parallel or sequential workspaces, 10+ agents, Apache-2.0; the company shut down on 10 April 2026 and the project continues under community maintenance; the docs say agents run with permission-skipping flags by default. [comparison](https://nimbalyst.com/blog/best-agent-management-tools-2026/).
- **GitHub Copilot app** (announced at Microsoft Build, 2 June 2026): "My Work" dashboard, Interactive/Plan/Autopilot sessions, worktree per session, Agent Merge, local and cloud sandboxes, an Open button to VS Code. [overview](https://www.digitalapplied.com/blog/github-copilot-app-agent-native-desktop-orchestration-2026).
- **Orca**: one prompt fanned to several agents in separate worktrees, comments on diff lines sent back to the agent, Claude and Codex usage and rate-limit resets. [repository](https://github.com/zaridan/orca). Its licence was not confirmed.
- **Sculptor**: each agent in its own Docker container; early preview. **Mux**: local, worktree or SSH execution; browser UI. **Claude Squad**: tmux and worktrees in a terminal. **Nimbalyst**: markdown, mockups and code in one workspace, native iOS app. [comparison](https://nimbalyst.com/blog/best-agent-management-tools-2026/).
- **Agentic IDEs**: Cursor (parallel agents; one source says up to eight), Windsurf (automatic context), Google Antigravity (several agents in separate workspaces), Kiro (specs and hooks), Zed (open source, agent panel), Cline and Roo Code (bring your own key; Roo has personas and auto-approval workflows). [DataCamp list](https://www.datacamp.com/blog/best-agentic-ide), [Codecademy comparison](https://www.codecademy.com/article/agentic-ide-comparison-cursor-vs-windsurf-vs-antigravity).
- **ACP**: the Registry went live on 28 January 2026; `session/list`, `session_info_update` and session config options were stabilised in February and March 2026. [Zed announcement](https://zed.dev/blog/acp-registry), [updates page](https://agentclientprotocol.com/updates).
- **Worktree limits**: worktrees share ports and databases, and guides say no tool isolates both. [Upsun guide](https://developer.upsun.com/posts/ai/git-worktrees-for-parallel-ai-coding-agents).

Not found: reliable details on Emdash, Crystal, Warp, Kiro steering files, Claude Code hooks and skills, Zed's review UI, or checkpoints in any orchestrator. Those ideas are marked as unconfirmed in the ADRs that touch them.

## What this project already has

Protocol-uniform ACP hosting, phone and desktop clients with handoff, worktrees with phone merge, "Run on several agents" compare with cost, checks, PR creation, host terminals and preview, browser with element picker, auto-approve and shell review, a sandbox for approved commands, multi-host desktop, and a service installer. The ADRs above avoid repeating these; each names the part it extends.
