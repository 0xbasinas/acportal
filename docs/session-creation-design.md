# Session creation from All sessions

9 October 2026: design created through the user's Penpot MCP connection in `New File 1`, `Page 1`. File ID: `7eed092c-bad6-802e-8008-bea06e03019c`. Original boards 01–16 were preserved. New editable boards 17–23 sit to their right.

| Board | Purpose |
| --- | --- |
| 17 · All sessions / create entry | Existing Sessions design copied with a 48 dp plus target |
| 18 · New session / choose connection | Connection choice with online/offline status and pairing entry |
| 19 · New session / workspace and agent | Chosen connection, workspace, agent, recents and explicit Start session |
| 20 · New session / no paired connections | Pair-first explanation and Back |
| 21 · New session / connection unavailable | Explicit Retry or choose another connection |
| 22 · New session / choose workspace | Host-approved workspace selection |
| 23 · New session / choose agent | Discovered agent selection |

The main prototype starts from board 17's plus button. Workstation opens the creation form; Laptop opens the unavailable state. Back returns through the flow. Workspace and agent rows open their selection boards. Start links to the existing conversation board as a visual prototype only. Pairing entries link to the existing Connections board. The empty state is a separate scenario board. Retry stays on the failure board to represent a persistent failure.

Names and paths in these boards are illustrative design data. Runtime choices must come from saved host records and fresh authenticated host discovery, with host/workspace scoping intact. Selecting a connection or a recent choice must not start a process. Only Start session calls creation; failure keeps the choices and exposes an explicit retry. Never retry creation automatically. Back should preserve the Sessions filters and scroll. Pairing must use the existing pairing routes and credential handling.

Design follows the inspected original Sessions and workspace/agent boards: 390×844 portrait, 24 px gutters, Inter Tight, near-black `#0A0A0B`, neutral `#161618` surfaces, `#EDEDED` primary text/actions, `#A1A1AA` secondary text and `#72C7A2` online indicators. Existing Sessions and new connection/form PNG exports were visually inspected. Text containment and prototype targets were inspected through Penpot. This does not establish Android keyboard, large-font, TalkBack or live-host acceptance.

Implemented 9 October 2026 on `codex/session-model-picker`: `SessionConnectionRoute` is the new Sessions plus destination. It selects a saved host and opens `NewSessionRoute(fromSessions=true)` with fresh discovery, selected connection/workspace/agent cards and the existing validated creation controls. Discovery failure keeps Reload and choose-another-connection actions. Detail reads are cancellable and generation guarded. Existing connection entry points retain their inline choices. Successful creation removes the intermediate creation routes; Back returns to the originating Sessions page. See [TODO_DONE.md](../TODO_DONE.md) for the 52 JVM, build/lint and 19 scoped device checks and their limits. Real-phone successful launch, native TalkBack and broader live-host/lifecycle acceptance remain open.
