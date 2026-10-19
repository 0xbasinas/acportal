# Real app screenshots

Screenshots of the running **desktop app** (Electron, `desktop/`), taken on 18 October 2026 on Windows. These are the real app, not the Penpot designs in `../png/`: compare them to find where the app and the design differ.

The app was started with `scripts/dev-desktop.ps1 -SkipOmniRoute` (host on `127.0.0.1:8765`, no agents except the ones already added from the app: Goose and grok) and driven over Chromium's remote debugging port. Window size 1282 × 802 CSS pixels, captured at 1602 × 1003.

## The real run

Screens `07`–`13` and `34` come from one real Goose session (`ds/deepseek-flash`, **Supervised** mode) in a scratch project, `.local/real-app-demo/calc` (gitignored, not in this repository): a two-function Python module where `add` returns `a - b` and one unit test fails. The prompt was:

> The test in test_calc.py fails. Find the bug in calc.py, fix it, then run the tests to check.

Goose read both files, asked to write `calc.py` (screen `08`, allowed once), asked to run `python -m unittest -v test_calc.py` (screen `10`, allowed once), and the tests passed (screen `11`). It took 1 minute 17 seconds and cost about US$0.0012. Nothing else was approved or run in the agent.

## Screens

| File | What it shows |
| --- | --- |
| `01-home-new-session` | New session composer: agent, model, host, project, branch, rights, worktree |
| `02-command-palette` | Ctrl K palette |
| `03-sidebar-ended-sessions` | Sidebar with the project's ended sessions expanded |
| `04-new-session-project-picker`, `05-folder-browser`, `06-new-session-subfolder` | Choosing the project folder and a sub-folder |
| `07-composer-with-prompt` | Prompt typed, before sending |
| `08-approval-card-write` | Host-write approval with the diff |
| `09-run-finished` | Mid-run state after the write was allowed |
| `10-approval-card-shell` | Shell-line approval |
| `11-run-complete-conversation` | Finished turn: summary, fix, test output |
| `12-conversation-top-tool-steps` | Top of the same conversation: prompt, tool steps, diff |
| `13-review-changes` | Full review of the session's changes |
| `14`, `16`–`19` | Git panel: Changes, Branches, History, Worktrees, Stash |
| `15-browser-panel` | Browser panel beside the conversation |
| `20-terminal-dock` | Terminal dock (a bash shell on the host in the session folder) |
| `21`–`30` | Settings: General, Agents, MCP servers, Hosts, Workspaces, Approvals & review, Terminal, Browser, Appearance, Keyboard |
| `31`–`36` | Light theme: Appearance, General, Agents, conversation, palette, home |
| `37-run-on-several-agents` | Start dialog for the same prompt on several agents |
| `38-host-switcher` | Host switcher |

## Limits

- Windows, dark and light, one window size. Nothing from macOS, Linux or a small window.
- One host and one real session. States that need a second host, a host that is offline, a re-pair, a merge conflict or a failed run were not produced.
- The three older sessions in the sidebar were left unopened because they were not part of this run.
- Settings forms that can show credentials (agent Edit, Add host) were not opened.

The same screens as HTML, built with the app's own stylesheet: [`../real-app-html/`](../real-app-html/README.md).
