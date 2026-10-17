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

While the probe runs, the chip reads *Reading models…* (`aria-busy`). If the probe fails or the host is older, the chip says *Models unknown* with Retry; nothing is invented, and a session started anyway uses whatever the agent picks.

Options are read the way the phone does (`SessionDetailScreens.kt`, code in `desktop/src/agentSetup.ts`): when the agent sends `configOptions`, every `select` option is offered under its own name (model, mode, reasoning effort, ...), grouped values under their group names; the older `models`/`modes` fields are used only when there are no `configOptions`. Like the phone's Session options screen, the dropdown shows one row per option with its current value, in sections as the ACP spec suggests: **Model** (the `model` option, then `model_config` and `thought_level` options), **Mode**, then **More options** (everything else, such as an uncategorised provider), each in the agent's order (headings only when there is more than one section; the phone uses the same grouping, `SessionOptionSections.kt`); a row opens only that option's list, and picking a value or Escape/Back returns to the rows. An agent with a single option shows its list straight away. Lists longer than 40 get a search box (name, id or group; Enter picks the first match), like the phone's searchable dialog. The chip shows the first two values. The session header has one options button (showing the model) that opens the same panel (`OptionPicker.tsx`), and a later `config_option_update` replaces the list there.

One option can change another's values (another provider brings other models). ACP has no dependency field; instead every `session/set_config_option` reply, and every `config_option_update`, carries the agent's complete option list. The desktop relies only on that, so it works for any agent:

- **Before the session**, nothing is set on the agent: setting an option needs the agent running, and some agents (Goose, for one) save every change as their default. Picks are kept in the order they are made, and the form starts from what the host reports for the agent, not from this computer's memory; the last session's choices are picked again where the agent still offers them. When a pick other than the model may change other lists, the form says that those lists appear in the session header after start.
- **At start**, the picks are applied in that order (a provider before the model it brings), before the first prompt.
- **In a session**, the reply to each change replaces the whole list in the header; a refused change shows the agent's value again with a notice. Picked values are applied right after start, only when that session's agent offers them and they differ from its start values (`session/set_config_option`, or `session/set_model` / `session/set_mode` for the older fields), before the first prompt.

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
