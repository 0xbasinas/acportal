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

## Models and modes before the first session

ACP agents report their modes and options (`modes`, `configOptions`) only in the reply to `session/new`. The composer takes them from the first of these sources that has any:

1. What this computer saw in the agent's last session. This includes the model and mode you chose then, stored per agent in local storage.
2. `setup` on the agent in `GET /v1/agents`. The host fills it from the newest retained session of that agent, else from an earlier probe.
3. A probe, used when the host lists the `agentProbe` feature. `POST /v1/agents/{id}/probe` with `{"workspace": "<approved folder>"}` starts the agent and runs ACP `initialize` and `session/new`. The throwaway session has no file, terminal or MCP access and gets no prompt. The host keeps only `modes`/`models`/`configOptions` (cached in memory for `GET /v1/agents`), removes the session and stops the agent. This uses the protocol only and has no agent-specific code.

While the probe runs, the chip reads *Reading models…* (`aria-busy`). If the probe fails or the host is older, the chip says so and the session starts with the agent's defaults. The chosen mode and model are applied right after start (`session/set_mode`, `session/set_config_option`), before the first prompt.

## Dropdown placement

Each dropdown is measured when it opens. It goes on its preferred side (below; above for chips inside the prompt box). It flips when that side is too short and the other side has more room. It is shifted left to stay 8 px inside the window, and its height is capped to the room available (`placePopover` in `src/agentSetup.ts`).

## Tests

`desktop/test/NewSession.test.tsx` (Vitest, Testing Library, jsdom; `npm test` runs it after the node tests, and desktop CI runs `npm test`) covers:

- chips, keyboard navigation, Escape with focus return and outside click;
- agent, access and host choices, and recent prompts;
- the branch picker: current, existing, filter, new branch with base, worktree, not a repo, no git;
- sending: success, Shift Enter, and a failed start that keeps the text;
- the 220 ms animation and its absence under reduced motion;
- the probe: loading, choices, host-reported setup, failure fallback;
- dropdown flip and clamp.

The host side is covered by `acpd/tests/agent_probe.rs`.
