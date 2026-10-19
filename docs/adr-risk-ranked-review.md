# ADR: risk-ranked review of agent changes, with per-hunk keep and revert

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Review is the bottleneck: one benchmark cited by a source says AI-assisted pull requests are 2.6 times larger than unassisted ones (an association, not a causal result), and another study summary says agent changes carry more redundancy and technical debt. Meta's RADAR pipeline scores each diff for risk instead of blanket-approving AI changes. A vendor blueprint suggests letting low-risk changes pass and gating anything touching auth, payments, secrets or infrastructure for a human. One commentary warns that a second model can repeat the first model's misunderstanding. Harness groups a diff logically rather than by file. Zed lets a person accept or reject individual hunks in a multibuffer. See [research notes](adr-ade-index.md#research-notes).

This project already has a rules layer (`shell_review.rs`: lexer, allowlist, hard-deny list) and an optional model reviewer, a checks runner (`api/checks.rs`) and diff views in four places. Review order today is the order the agent wrote files.

## Decision

Add a **deterministic risk label** per changed file and per task, shown on every diff surface, and a per-hunk **revert**. The label advises; it never approves anything.

1. **Rules, not a model, decide the label.** A path classifier with three levels (`high`, `medium`, `low`) from a data table the operator can extend: `high` for credentials and secrets files, CI/deploy configuration, auth or permission code paths named by pattern, lockfiles and dependency manifests changes of any kind, migrations, shell scripts and executable bits; `medium` for configuration, build files and tests removed or skipped; `low` for docs and tests added. Extra signals: file mode change, a deleted file, a new binary, a diff above a size threshold, a changed test with no changed source, a skipped or disabled test marker added.
2. **Order and grouping**: diff lists sort by label then size, and group by the directory so related files appear together. The agent's own order stays available.
3. **Explain, optionally**: a "Summarise changes" action sends the diff (bounded) to the *same agent session* as a normal prompt, labelled as a summary request. The summary is shown as agent text, not as a verdict. No separate model or provider key is introduced here. A second-reviewer pass is possible through the existing `Run on several agents` review prompt, with the diversity warning shown.
4. **Gate**: the existing pre-merge checklist gains one line, "N high-risk files reviewed", that the person must tick for a task whose label is `high`. Auto-approve rules ([orchestrator design section 32](orchestrator-design.md#32-host-side-auto-approve-15-october-2026)) can opt in to refuse auto-approving a write whose path classifies `high`.
5. **Per-hunk revert** on a worktree diff: a button restores one hunk to the base using `git apply --reverse` on that hunk, after confirmation, with a checkpoint taken first when [adr-turn-checkpoints](adr-turn-checkpoints.md) exists. It does not change the agent's conversation, so the timeline notes "hunk reverted by user" and the next prompt says so, as for checkpoint restore.
6. **Keep** marks a hunk reviewed (local state). It writes nothing.

## Options Considered

### Option A: rules-based label, summary via the same agent, per-hunk revert (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium. A classifier, sorting and grouping, a revert action. |
| Cost | None; summary uses existing prompt spend. |
| Scalability | Per diff. |
| Team familiarity | Same rule-table style as shell review. |

**Pros:** explainable and testable; cannot be talked out of a decision by the diff's contents; helps the phone user on small screens.
**Cons:** path rules miss risky code in an innocuous file; labels may create false confidence.

### Option B: a model-scored risk number

**Pros:** catches semantic risk. **Cons:** non-deterministic, costly, prompt-injectable by the diff itself; the project's shell review already limits the model to cases rules leave undecided.

### Option C: accept and reject hunks before they are written

**Pros:** closest to Zed's inline accept/reject. **Cons:** the host writes the file the agent asked for; applying a partial write would leave the agent believing the whole file exists. Reverting after the fact keeps the agent's view honest.

## Consequences

- The label is advice only and is shown with the words "path-based".
- Tests: classifier table on Windows and POSIX paths; a rename into a `high` path; hunk revert on a file that changed since; revert refused when the working tree has conflicting edits.
- Hunk revert is a host write: same consent, audit line ([adr-audit-and-trace](adr-audit-and-trace.md)) and containment as other writes.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: classifier and table, exposed on the git diff route.
3. [ ] Desktop and Android: label chips, sort, "N high-risk reviewed" gate.
4. [ ] Per-hunk revert after turn checkpoints exist.
