# Agentic development environment ideas: index and research notes

Three rounds of research on 18 October 2026: the first gave twelve ADRs, the second twelve more ("credential proxy" to "device rights"), the third eleven more ("workflow recipes" down).

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
| [credential proxy](adr-credential-proxy.md) | Host egress proxy: domain allowlist, provider keys injected outside the agent | Anthropic secure-deployment guide, sandbox-runtime | sandbox, container ADR, doctor |
| [steer and queue](adr-steer-and-queue.md) | Queue, steer (cancel and replace) or interrupt while an agent works, host-owned | Claude Code and Codex steering, Nimbalyst queue | session actor, request-id cache |
| [agent skills](adr-agent-skills.md) | Discover, review, install and cross-install `SKILL.md` skills | Agent Skills standard, Codex `@` skills, Cursor marketplace | workspace containment, consent |
| [audit and trace](adr-audit-and-trace.md) | Permission audit trail and opt-in OpenTelemetry export, no content | Langfuse-style traces, the "no audit trail" gap | `traceparent`, logging sink |
| [session search](adr-session-search.md) | Full-text search over saved sessions per device, metadata search across hosts | Codex find, Nimbalyst voice find | Room (schema 4), session catalog |
| [issue tracker](adr-issue-tracker.md) | Start tasks from GitHub Issues (`gh`) and Linear deep links | Superset Tasks, Copilot and Linear, Linear deeplinks | `api/pr.rs`, tasks |
| [Android integration](adr-android-integration.md) | Share target, dictation, camera, notifications, widget, tile | Nimbalyst mobile, ChatGPT iOS widgets | attachments, attention board |
| [agent browser tools](adr-agent-browser-tools.md) | Post-turn screenshots first, then a scoped browser MCP tool | Playwright MCP, Cursor 3 browser and demos | desktop browser, preview, external MCP |
| [risk-ranked review](adr-risk-ranked-review.md) | Deterministic path-based risk labels, grouping, per-hunk revert | Meta RADAR, risk-gated review, Zed hunks | shell-review rules, checks |
| [multi-repo tasks](adr-multi-repo-tasks.md) | Task groups across up to five repositories, ordered merges | Cursor multi-root, Vibe Kanban multi-repo, `gh-stack` | tasks, worktrees, PR flow |
| [context handoff](adr-context-handoff.md) | Context meter and a reviewed "continue in a new session" note, also cross-host | Compaction behaviour, Cursor cloud handoff | `usage_update`, worktrees |
| [device rights](adr-device-roles.md) | Observer, approver, operator, owner per device with expiry | Coder roles, Copilot org policy | pairing store, route middleware |
| [workflow recipes](adr-workflow-recipes.md) | Saved, parameterised jobs (agent, profile, prompt, checks, budget) as plain data | Goose recipes, Codex automations | prompt history, tasks |
| [session sharing](adr-session-sharing.md) | Guest links scoped to one session, observer or approver, expiring | Happy invites, Coder roles | device rights, remote routes |
| [host web client](adr-host-web-client.md) | Static web client served by the host (iOS, borrowed laptops, guests) | Mux server mode, OpenCode web, Happy web | desktop React code, pairing |
| [terminal persistence](adr-terminal-persistence.md) | Scrollback, sequence reattach, liveness, phone key row | tmux and Mosh stacks, AgentAPI | `pty.rs`, `terminals.rs` |
| [plan review](adr-plan-review.md) | Plan card with history, edit, comment, approve; Mermaid on desktop | Mux plan mode, Copilot Plan, Nimbalyst approvals | ACP `plan`, review comments |
| [overlap warning](adr-overlap-warning.md) | Warn when parallel tasks touch the same files; `merge-tree` conflict probe | Mux divergence view | worktrees, git runner |
| [devcontainer import](adr-devcontainer-import.md) | Read `devcontainer.json` as setup suggestions, never run it | Codespaces, DevPod, Coder, Ona | workspace scripts, container ADR |
| [warm worktrees](adr-warm-worktrees.md) | Pool of prepared worktrees so tasks start ready | Ona prebuilds | workspace scripts |
| [host hooks](adr-host-hooks.md) | Event hooks: run, notify (ntfy or webhook), capped prompt | Claude Code hooks, Kiro hooks, Warp notifications | host event stream, sandbox |
| [composer helpers](adr-composer-mentions.md) | Slash commands, `@` mentions, deterministic next-prompt suggestions | Codex `@` menu and composer predictions | `available_commands_update` |
| [MCP Apps spike](adr-mcp-apps.md) | Written research spike only, before any code | MCP Apps, A2A | form elicitation |

## Suggested order

1. **Attention board**, **session modes** and **device rights**: make existing features legible and safe to share; they share one set of states and routes.
2. **Steer and queue**, **review comments**, **risk-ranked review** and **turn checkpoints**: the review and control loop, built on one review surface.
3. **Android integration** (share target and dictation first) and **session search**: cheap, visible wins on the phone.
4. **Workspace scripts**, **registry browser** and **agent skills**: remove setup friction for worktrees and agents.
5. **Audit and trace**, **usage dashboard** and **context handoff**: observability, ahead of any unattended work.
6. **PR watch**, **issue tracker**, **scheduled runs**, **multi-repo tasks**, **project context**, **editor handoff**: depend on earlier work or on a real `gh` check.
7. **Credential proxy**, **agent browser tools** and **container isolation**: security-sensitive and platform-specific; spike first, and decide [external MCP](adr-external-mcp.md) before browser control.
8. **Third-round additions**: start with the small, high-use ones: **composer helpers** (slash commands use data the Android reducer already holds), **plan review** (extends the desktop's existing plan item), **overlap warning** and **host hooks** (`notify` first: it answers push notifications without a relay). **Host web client** unlocks iPhone and **session sharing**; do it after **device rights**. **Terminal persistence** fixes the biggest known phone-terminal gap. **Workflow recipes** feed scheduled runs and the issue tracker. **Devcontainer import** and **warm worktrees** wait for workspace scripts. The **MCP Apps spike** is reading, not building.

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
- **Mobile clients**: Nimbalyst's iOS app syncs sessions with end-to-end encryption, sends notifications for completion, errors and approvals, shows a Lock Screen and Dynamic Island view ranked by waiting time, supports voice (Alpha), camera attachments and queued prompts; it is iOS-only (an Android icon appeared in a release note). Forge Remote, CC Pocket, Omnara and Port22 are phone apps for Claude Code and Codex; the ChatGPT iOS app added Home Screen widgets for recent tasks on 2 October 2026. Anthropic's Remote Control is Claude Code only. [Nimbalyst mobile docs](https://nimbalyst.com/docs/mobile/mobile-app.md), [mobile app roundup](https://nimbalyst.com/blog/best-mobile-apps-for-claude-code-2026/), [Codex changelog](https://learn.chatgpt.com/docs/changelog).
- **Codex app**: worktrees in the sidebar, automations that run locally or in a worktree with a review queue, skills in the `@` menu, composer predictions (beta, 9 October 2026), a review file tree. Third-party guides disagree on launch dates. [changelog](https://learn.chatgpt.com/docs/changelog), [guide](https://intuitionlabs.ai/articles/openai-codex-app-ai-coding-agents).
- **Cursor 3** (2 April 2026): Agents Window with local, worktree, cloud and SSH agents in one sidebar, cloud handoff, `/worktree`, `/best-of-n`, `/multitask`, integrated browser, multi-root workspaces, marketplace plugins. Sources disagree on Design Mode. [guide](https://www.digitalapplied.com/blog/cursor-3-agents-window-complete-guide).
- **Warp**: management view of all running agents with notifications, agent permission allowlists and denylists, Warp Drive shared knowledge. Mostly June 2025 launch coverage. [SD Times](https://sdtimes.com/ai/warp-2-0-evolves-its-terminal-experience-into-an-agentic-development-environment/).
- **Zed**: parallel agents and a `spawn_agent` subagent tool (0.227.0, March 2026), multibuffer review with per-hunk accept or reject, session history for several external ACP agents, boolean toggles for ACP session configuration (1.11.0, July 2026). [release notes](https://zed.dev/releases/preview/0.227.0).
- **Claude Code**: layers for memory, skills, subagents, hooks, plugins and MCP; checkpoints with rewind; read-only plan mode. Mostly secondary blog sources. [docs](https://code.claude.com/docs/en/features-overview).
- **Skills**: the Agent Skills format is shared across many tools but each uses its own folder; marketplace counts conflict and one source reports a high rate of vulnerable skills. [overview](https://codex.danielvaughan.com/2026/05/09/cross-agent-skills-ecosystem-maturity-npm-analogy-codex-cli-portability/).
- **Sandboxing and credentials**: keep credentials outside the agent and route egress through a proxy; allowlists limit exfiltration, not injection; the proxy sees only hostnames without TLS inspection. [Anthropic secure deployment](https://code.claude.com/docs/en/agent-sdk/secure-deployment), [egress control](https://blaxel.ai/blog/sandbox-egress-control-outbound-allow-listing).
- **Observability**: OpenTelemetry-based tracing with session grouping (Langfuse and others); a Hacker News argument that coding agents lack traces, per-task cost, permission audit trails and replay. [Augment guide](https://www.augmentcode.com/guides/agent-observability-for-ai-coding).
- **Browser verification**: Playwright MCP lets agents check UI work; sources note the check is ad hoc and recommend deterministic screenshot diffs. [Argos](https://argos-ci.com/blog/playwright-mcp-visual-testing).
- **Review at scale**: risk scoring and gating ideas (Meta RADAR, vendor blueprints); warnings about same-model blind spots. [RADAR paper](https://arxiv.org/pdf/2605.30208), [blueprint](https://resources.rework.com/libraries/ai-agents/ai-code-review-agent).
- **Trackers and stacked PRs**: Linear deeplinks open prefilled prompts in agent tools; GitHub stacked PRs entered public preview on 30 July 2026. [Linear changelog](https://linear.app/changelog/2026-02-26-deeplink-to-ai-coding-tools), [gh-stack guide](https://agentpedia.codes/blog/github-stacked-pull-requests-gh-stack-agent-guide.md).
- **Steering**: steer, follow-up and interrupt as separate modes; conflicting reports on Claude Code's current behaviour. [patterns](https://agentpatterns.ai/patterns/agent-design/steering-running-agents/).
- **Compaction and memory**: opaque compaction in some tools, third-party memory servers with unverified benchmarks. [deep dive](https://codex.danielvaughan.com/2026/04/14/context-compaction-deep-dive-codex-cli-claude-code-opencode/).
- **Self-hosted teams**: Coder offers SSO, roles, audit logging and spend limits, partly paid; OpenHands is open source (features unconfirmed). [Coder](https://coder.com/solutions/agents).
- **Toad** (Will McGugan, Textual): a terminal front-end for several agents through ACP, with Markdown rendering, fuzzy file search, a prompt editor and shell passthrough. [announcement](https://simonwillison.net/2025/Jul/23/announcing-toad/), [release post](https://willmcgugan.github.io/toad-released/).
- **OpenCode 2.0** (reported 12 October 2026): terminal, desktop and web clients share one background server; `opencode serve` and `opencode web`; a rebuilt API, SDK and plugin system. Remote workspaces were experimental in March 2026. [report](https://zeli.app/de/story/50094994).
- **Happy** (open source, MIT relay): mobile and web client for Claude Code and Codex with end-to-end encryption, session handoff and inviting others into a session. [repository](https://github.com/slopus/happy). **AgentAPI** (Coder, MIT): HTTP layer that drives agents through an in-memory terminal emulator; its listing warns to bind only to trusted interfaces. [listing](https://docsearch.algolia.com/mcp/docs/repo/coder/agentapi).
- **Mux** (Coder): local, worktree and SSH workspaces, git divergence view, Plan/Exec modes with Mermaid, costs tab, compaction dialog, responsive server mode. [repository](https://github.com/coder/mux).
- **Open-source agents**: Roo Code was archived with a final release on 15 May 2026 per one source and Kilo Code is the suggested successor; Cline is approval-first; OpenHands is autonomous and sandboxed; Aider is git-native; Plandex has a diff sandbox and plan branching. Licences and star counts differ between sources. [comparison](https://aifoss.dev/blog/open-source-coding-agents-state-2026/), [CLI agents compared](https://michaellivs.com/blog/cli-coding-agents-compared/).
- **Goose** (Block, now under the Linux Foundation's Agentic AI Foundation with MCP and AGENTS.md): recipes as portable YAML with parameters and sub-recipes, subagents with parallel or sequential modes, many MCP extensions. Scheduler and `goose serve` details were not found. [overview](https://rywalker.com/research/goose).
- **Cloud agents**: Jules (Google), Devin (Cognition, terminal release reported 9 April 2026) and Factory Droid appear in comparison pages with conflicting prices; no primary source read. [roundup](https://ssojet.com/blog/best-cloud-coding-agents).
- **MCP Apps and A2A**: interactive tool UIs and an Agent Card protocol, both described only in secondary sources. [MCP roadmap summary](https://mcpplaygroundonline.com/blog/mcp-2026-roadmap-whats-changing-for-developers.md).
- **Environments**: `devcontainer.json` is the portable layer (Codespaces, DevPod, Daytona, Coder); Ona (formerly Gitpod) pivoted to agent sandboxes with prebuilds; one source claims an acquisition that I could not corroborate. [comparison](https://www.morphllm.com/comparisons/codespaces-alternative).
- **Phone terminal stacks**: Mosh plus tmux plus a mobile SSH client (Blink, Termius), with keepalive settings and named sessions; Mosh cannot forward ports. [Blink and Tailscale](https://docs.blink.sh/integrations/tailscale+mosh), [phone setup guide](https://getmoshi.app/articles/best-setup-coding-agent-phone).

Not found: reliable details on Emdash, Crystal, Paseo, Happy, Kiro steering files, Jules and Devin tracker integrations, OpenHands multi-user features, Graphite and Greptile, VibeTunnel, a Goose scheduler, `goose serve`, or checkpoints in any orchestrator. Those ideas are marked as unconfirmed in the ADRs that touch them.

## Watch list (no ADR yet)

- **Chat entry points** (start or answer an agent from Slack, Telegram, Discord): Cursor and Devin do this. Needs an inbound or long-polling integration and a security review; [host hooks](adr-host-hooks.md) `notify` covers the outbound half.
- **Local models** (Ollama through Mux; Plandex's 14B+ guidance): agents choose their own models; a doctor check for a reachable local endpoint might help.
- **Team features** (SSO, groups, spend limits): Coder's territory; [device rights](adr-device-roles.md) is the single-owner version.
- **A2A interoperability**: a stated non-goal of the orchestrator design; revisit with the MCP Apps spike.
- **Vim keys and keyboard-first navigation** (Mux): small desktop polish.
- **Wear OS or a watch tile** for approvals waiting: cheap if [Android integration](adr-android-integration.md) lands.

## What this project already has

Protocol-uniform ACP hosting, phone and desktop clients with handoff, worktrees with phone merge, "Run on several agents" compare with cost, checks, PR creation, host terminals and preview, browser with element picker, auto-approve and shell review, a sandbox for approved commands, multi-host desktop, and a service installer. The ADRs above avoid repeating these; each names the part it extends.
