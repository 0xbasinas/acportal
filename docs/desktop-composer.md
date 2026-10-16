# Desktop app: new-session composer

Opening a new session (Ctrl N, the sidebar *New session* button, a project row, the command palette) shows an inline composer in the main area instead of a dialog. Design: Penpot boards 96–99.

## Layout

- Heading *What should we work on?* and a line naming host and folder.
- Prompt box: **+** attaches files (600 KB each), the agent chip lists agents and *Run on several agents…* (opens the multi-run dialog with the prompt filled in), the model/mode chip, and Send. Enter sends, Shift Enter adds a line.
- Context row: **host** (defaults to the active host; picking another switches the whole app to that host and says so), **folder** (approved workspaces plus an optional sub-folder), **branch** (search or type a new name, Current, Existing, *New branch …* with a *from* base, *Own worktree*), **access** (read, write, commands, MCP servers) and a **Worktree** switch. The branch chip uses the Lucide GitBranch icon and reads *no git* outside a repository.
- Recent prompts for the folder as a 2×2 grid; clicking one fills the box.

Every chip is a button with `aria-haspopup`/`aria-expanded`. Its dropdown is anchored below it (above it for chips inside the prompt box), moves with ArrowUp/ArrowDown, closes on Escape (focus returns to the chip) or on an outside click.

## After Send

The composer docks to the bottom with a 220 ms ease-out FLIP animation (skipped under `prefers-reduced-motion`). The chips fold into one header line, and a *Starting…* row appears under the host's Sessions in the sidebar. Once the host returns the session, it is selected. The chosen mode and model are applied if they differ from the agent's defaults, and the first prompt is then sent. If starting fails, the composer returns with the text kept and the error shown.

## Limits

The agent reports models and modes only after it starts, so the chip offers the values it reported last time (stored per agent in local storage). The first time, it shows *Default model · default mode*.
