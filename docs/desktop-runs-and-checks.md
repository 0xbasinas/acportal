# Desktop app: run on several agents, and the pre-merge checklist

## Run on several agents

**Run on several agents** (sidebar button, or the command palette) sends one prompt to two or three agents at once. You can pick the same agent twice. Each run gets:

- a new branch named `run/<prompt-slug>-<id>-<n>-<agent>`, made from the base branch you choose, and
- its own git worktree beside the repository (for example `project2/notes-run-…`). Your own checkout is not touched until you merge.

The host creates the worktrees (`POST /v1/sessions` with `branch.worktree`), so the folder must be a git repository inside one of the host's workspace roots.

While the runs work, every approval they ask for shows up in the sidebar, the host switcher and desktop notifications, as for any other session. The **Runs** group at the top of the sidebar lists active runs, and the palette lists all of them.

### Compare

The compare view shows one column per run:

- **Diff:** what the run changed relative to the base branch, new files included. The app stages the run's files in its worktree (`git add -A`) and diffs the index against the base.
- **Totals and cost:** files, added and removed lines, and the agent's cost and tokens when the agent reports them (`usage_update`).
- **Status:** working, idle or waiting, the agent's last message, and the checklist results.

### Merge or discard

Pick a winner and press **Merge**. The app then:

1. commits the run's changes on its branch,
2. merges that branch into the folder the runs started from as one `--no-ff` merge commit,
3. closes every run's session, removes every run's worktree and deletes the run branches. The losing branches are force-deleted; the merged one is deleted with `-d`.

If the merge stops on a conflict or on uncommitted changes in your checkout, nothing is discarded; resolve or abort it in Git. **Discard all** closes the sessions and removes all the worktrees and branches without merging.

## Pre-merge checklist

Each workspace can have test and lint lines, configured in the compare view or in a task's merge dialog. They are stored on this computer, keyed by the workspace folder. You can also turn on an **AI diff review** by an agent you choose.

- **Checks** run on the host through `POST /v1/checks`, one line after another, with `/bin/sh -c` in the run's worktree. Where the host has the OS sandbox (bubblewrap on Linux), only that worktree is writable and the home folder is hidden. Each result shows its exit code, time and an output tail, and says whether it ran sandboxed. Untracked files the checks create (bytecode, caches, reports) are removed afterwards, so they never end up in the merge commit. Run checks only once the agent is idle.
- **The AI review** starts a separate session on the worktree with reading allowed and writing and commands off. It sends the diff and asks the agent to end its reply with `VERDICT: PASS` or `VERDICT: FAIL`. Agents with their own tools may still ask to run commands; reject those, since reading the files is enough.

The gate is:

- **green** when everything configured ran and passed,
- **red** when a check failed, the review said FAIL, or the review gave no verdict,
- **pending** until the checks finish.

Green merges right away. Anything else needs an explicit **Merge anyway** confirmation. The same checklist appears when merging a delegated task's worktree.
