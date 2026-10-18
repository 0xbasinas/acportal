# ADR: an activity panel where nothing runs invisibly

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Threadlines (a T3 Code fork) states a principle, "Nothing runs invisibly", and tracks tasks, subagents and background processes with live state and stop controls. Harnss keeps task agents running after they are spawned and tracks them in a dedicated panel; its tool calls nest subagent work under the parent. GitKraken's Kepler has an *Agent Graph* of live tasks, sessions, turns, tool calls and subagents with status and input for each, plus agent, kanban and list views. AcpAgent Client shows "waiting for you" badges for background sessions. See [research notes](adr-ade-index.md#research-notes).

This host starts many kinds of things: agent processes, host terminals (`terminals.rs`), delegated tasks and their worktrees, dev servers started in terminals and exposed by preview, checks runs, and (if accepted) hooks, automations and PR watches. Today each lives on its own screen. The status bar shows a summary on desktop. A person who wonders "why is my laptop fan on?" has no single list, and a runaway process can only be found by visiting each place. This is related to, but different from, the attention board ([adr-attention-board](adr-attention-board.md)): that answers "what needs me", this answers "what is running".

## Decision

One **Activity** panel per host, built from state the host already owns, with a stop control for each item.

1. `GET /v1/activity` returns bounded rows: `kind` (`agent`, `terminal`, `task`, `check`, `preview`, `hook`, `automation`, `watch`), `id`, `parent` (session or task id, so rows nest), `title`, `state`, `started`, `cpu_ms` and `rss_kb` where the OS gives them for the process tree (best effort, absent on failure), and `stoppable`.
2. The desktop shows it as a docked panel next to the terminal dock and the status bar count ("7 running"); Android shows it from the host menu. Rows nest under their session or task and can be collapsed.
3. **Stop** uses the existing stop paths: cancel turn, kill terminal, cancel task, release preview, cancel a check, end an agent. No new kill mechanism, and the host never uses a stored PID as authority (existing rule); it uses the handles it already holds.
4. **Resource column**: process-tree CPU time and resident memory are read through the same platform code that tracks process lifetime (`process_tree.rs`), so a sort by memory finds the heavy agent. Values are labelled approximate.
5. **Idle and long-running flags**: an agent with no turn for 30 minutes, or a terminal with no output for 2 hours, gets a quiet "idle" tag with a Stop action. Nothing is stopped automatically (see [adr-idle-suspend](adr-idle-suspend.md) for the automatic option).
6. Everything the panel lists is already visible somewhere; the panel adds no new content, only a unified view.

## Options Considered

### Option A: a host projection of running things, nested, with stop (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Collecting rows from several managers, resource sampling, a panel. |
| Cost | A sampling pass every few seconds while the panel is open; none otherwise. |
| Scalability | Bounded by existing session, terminal and task limits. |
| Team familiarity | Same shape as `/v1/shared` and the task views. |

**Pros:** answers "what is running and what is it costing" in one place; makes cleanup safe and quick; matches a principle other tools advertise.
**Cons:** another list to keep consistent with each manager; resource numbers differ by platform.

### Option B: per-screen lists only (status quo)

**Pros:** nothing to build. **Cons:** runaway processes stay hidden.

### Option C: a graph view of agents and tool calls (Kepler's Agent Graph)

**Pros:** rich for orchestration. **Cons:** a visualisation project; the nested list gets most of the value. Revisit when delegated trees are deep.

## Consequences

- Tests: nesting from a task to its terminals and agent; stop on a row ends the right handle only; resource sampling failure leaves the row without numbers rather than failing the route; the row count is bounded.
- On Windows, process-tree memory reading is through the job object already used for containment; on Linux and macOS it reads `/proc` or `ps` output; macOS is unverified, as elsewhere.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: activity collector and route, tests.
3. [ ] Desktop: panel and status bar count; Android: list with Stop.
4. [ ] Add hook, automation and watch rows as those ADRs land.
