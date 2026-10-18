# ADR: resolve merge conflicts with an agent, reviewed by a person

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Parallel agents will produce conflicting changes; the research found several approaches. Zed added a feature to ask its agent to resolve merge conflicts (March 2026 release). Ralph extracts the base, ours and theirs versions of each conflicted file from git's index stages and sends them to an agent with context about both tasks. The Hermes merge-reconciler skill argues that an agent resolving a conflict against a peer's work tends to overwrite the peer or abandon its own change, so a neutral third party gets both diffs and both sides' stated intents. Braidkeep predicts conflicts with `git merge-tree` and computes a safe merge order. A Virginia Tech study (Java only, 67 scenarios) found LLM solvers matched the developer's resolution on 55 to 62 percent of true conflicts against 37 percent for the best structured tool, validating output with checks such as no leftover conflict markers and a clean parse. Codeg has a three-pane merge editor. See [research notes](adr-ade-index.md#research-notes).

This project refuses a conflicting merge from the phone and shows the command to run by hand (orchestrator design section 31); `api/git.rs` can list conflicted paths (`--diff-filter=U`). [adr-overlap-warning](adr-overlap-warning.md) predicts conflicts; nothing helps resolve one.

## Decision

Add a **Resolve with agent** action to a refused merge. The agent works in a throwaway merge worktree and produces a normal merge commit proposal that a person reviews before it lands.

1. **Prepare**: the host creates a temporary worktree at the target branch and runs `git merge --no-commit <task branch>` there. If it conflicts, the worktree holds conflict markers; nothing touches the person's checkout or the task's worktree.
2. **Neutral resolver**: a new session starts in that worktree with the Plan-then-edit profile ([adr-session-modes](adr-session-modes.md)), by default the agent the person picks (the screen suggests a different agent from the one that wrote either side, because the same model may repeat the same misunderstanding). Its prompt is built by the host: the conflicted file list, for each file the base/ours/theirs versions (bounded), both tasks' original prompts and final messages, the rule "preserve both intents; do not delete either side's tests; if you cannot reconcile, say so", and the checks command.
3. **Validate** (host-evaluated, never by the resolver): no conflict markers remain (`git diff --check` and a search for `<<<<<<<`), `git ls-files -u` is empty, the checks pass, and the diff against each parent shows no file removed that neither side removed.
4. **Review**: the person sees the resolution as a three-way view (ours, theirs, result) per file with risk labels ([adr-risk-ranked-review](adr-risk-ranked-review.md)) and the validation results. Only then does a **Commit merge** button create the merge commit in the temporary worktree; the existing phone-confirmed merge then fast-forwards the target to it.
5. **Failure is normal**: if validation fails after two resolver attempts, the screen shows the manual command as today and offers the temporary worktree path for the person to finish by hand. Nothing is merged and the worktree is removed on discard.
6. **Never** auto-merge, auto-push, or resolve across more than 10 files in one run (above that the person is shown a warning and the list).

## Options Considered

### Option A: neutral resolver in a throwaway merge worktree (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. Worktree orchestration, a prompt builder, validation, a three-way view. |
| Cost | One resolver session per conflict. |
| Scalability | One conflict resolution at a time per repository. |
| Team familiarity | Uses worktrees, sessions, checks and the merge confirm. |

**Pros:** no risk to existing work; objective validation; a person approves the actual result.
**Cons:** conflicts that need business knowledge will fail validation or pass it wrongly; resolution quality is the agent's.

### Option B: ask one of the two original agents to rebase onto the other

**Pros:** simple. **Cons:** the failure mode the Hermes notes describe (overwrite or abandon).

### Option C: keep refusing and show a command

**Pros:** safe. **Cons:** the status quo leaves the person with the hardest part.

## Consequences

- Needs a git version with `merge-tree`/worktree behaviour verified on the Windows runner.
- Tests: a conflicting pair of branches in a real repository; markers detected; a resolver that deletes a test is flagged; the temporary worktree removed on discard and after a crash (named for pruning, as in [adr-warm-worktrees](adr-warm-worktrees.md)).
- Study numbers are from one Java-only paper; do not quote them as expected success rates here.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: temporary merge worktree, prompt builder, validation, tests with real repositories.
3. [ ] Desktop: three-way view; Android: summary and confirm.
