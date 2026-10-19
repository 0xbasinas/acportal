# ADR: show what the host observed next to what the agent claims

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

The most useful lesson in the multi-agent write-ups is about trust in status reports. A hands-on Gas Town article records its coordinator reporting that all bugs were fixed when only two PRs existed, because work was committed locally but not pushed; it also records an autonomous merge despite failing integration tests, and four resulting PRs that were all closed. An analysis of agent code quality reports that reviewers feel better about approving agent changes even though agent changes carry more redundancy. Symphony measures results by landed pull requests rather than agent claims. The agent-orchestrator roundup notes that human gates in most tools sit at PR time. Bernstein runs a "Janitor" (lint, type checks, tests) before work reaches its merge queue. See [research notes](adr-ade-index.md#research-notes).

This host knows many facts that do not depend on what an agent says: which files changed on disk, whether commits exist and how many are ahead of the base, whether the branch was pushed, the result of the configured checks, whether a pull request exists and its state, whether the working tree is clean, what tests changed. The apps show the agent's final message prominently and these facts in separate screens (Git view, checks, task detail). A person approving from a phone mostly reads the message.

## Decision

Put a **verified facts strip** under every agent claim of completion, computed by the host, with explicit mismatch warnings. It reports; it never blocks.

1. **Facts** (all host-observed, each with a timestamp and "unknown" when it cannot be read):
   - changed files and counts (staged, unstaged, untracked, deleted), commits ahead of base, whether HEAD is pushed and to which remote, whether a PR exists and its state ([adr-git-forges](adr-git-forges.md));
   - checks: last run time, result, which commands ran, and whether code changed *after* that run;
   - tests: test files added, modified, removed or skipped (path-based and marker-based, the same signals as [adr-risk-ranked-review](adr-risk-ranked-review.md));
   - working-tree state: clean or dirty, merge or rebase in progress;
   - risk label and the number of `high` files.
2. **Mismatch rules** run on the end of a turn (`end_turn`) against simple claims. Claims are detected from a short, conservative list of phrases in the final agent message ("pushed", "tests pass", "fixed", "all done", "merged", "PR opened"), matched case-insensitively. Examples: the message says tests pass but no checks ran, or code changed after the last run; the message says pushed or opened a PR but nothing was pushed; the message says fixed but the diff is empty; the diff removed or skipped a test; the working tree has uncommitted changes the message does not mention. A match shows an amber line, "Agent says it pushed; the host sees no push", never a red error.
3. **Where**: the end of each turn card on both clients, the task detail, the merge screen and the attention board row. A compact form ("3 files, 2 commits, not pushed, checks passed 4 min ago, 1 high-risk") is the summary line on the Sessions list and in notifications ([adr-attention-board](adr-attention-board.md)).
4. **Gate** (opt-in): the merge confirmation, [merge queue](adr-merge-queue.md) and pipeline gates ([adr-task-pipelines](adr-task-pipelines.md)) can require that no mismatch is open, or that it was acknowledged. The default is advisory.
5. **No model in the loop.** Phrase matching is a plain list and the facts are git and checks results; nothing is sent to a model. The limits are stated on screen ("only phrases from a short list are checked").
6. **Agent feedback**: a one-tap "Tell the agent" turns a mismatch into a prompt ("You said tests pass, but the checks have not run since your last edit. Run them.") sent through the normal prompt path.

## Options Considered

### Option A: host-observed facts with a few conservative mismatch rules (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A facts collector over the git runner and checks, a small rule list, a strip component. |
| Cost | A few git calls at turn end. |
| Scalability | Per turn, bounded. |
| Team familiarity | Same git runner, checks runner and risk classifier. |

**Pros:** counters the failure the research found most often; cheap; explainable; helps a phone reader who cannot scan a diff.
**Cons:** phrase matching is crude and will miss claims; a false "mismatch" is a nuisance, so the wording is soft and rules are conservative.

### Option B: ask a model to audit the agent's claims

**Pros:** semantic. **Cons:** cost, content leaves the host, and the same-model blind spot the research warns about.

### Option C: show only the agent's message

**Pros:** nothing to build. **Cons:** the status quo, where a confident wrong message is the easiest thing to believe.

## Consequences

- No new data leaves the host; facts are metadata the Git view already shows.
- Tests with real repositories: unpushed commits and a "pushed" message produce a mismatch; edits after a green checks run; a removed test file; a clean tree produces no strip warnings.
- The phrase list is data and can be tuned or extended per locale later; start with English.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: facts collector, mismatch rules, a route returning the strip for a turn, with tests.
3. [ ] Clients: the strip and the Tell the agent action.
