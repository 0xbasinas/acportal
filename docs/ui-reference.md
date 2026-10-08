# UI reference

The user's 16 PNG mockups in `C:/Users/<you>/Downloads/Page 1` are the visual reference for the Android app. They supersede the earlier teal Material styling. All 16 were inspected directly.

Use a near-black background, neutral surfaces, white primary actions, muted secondary text and green online indicators. Keep 24 dp page gutters, compact headings, simple rows and a rounded text-only bottom navigation pill. User messages have a restrained inset surface; agent messages use the page background. The composer has one rounded surface. Permissions open in a bottom sheet and keep the supplied ACP option names and IDs. Dismissal leaves the decision pending.

The current pass implements the foundation, Connections, inline workspace/agent selection, searchable grouped Sessions, row-based appearance/history Settings, the composer, compact tool rows and the permission sheet. Detail-page coverage is tracked below:

| Mockups | Implementation status |
| --- | --- |
| 07 Agent details | Implemented negotiated session details and registry discovery from Settings or a connection, with availability-aware session creation |
| 08 and 13 Session options | Implemented full model/mode/configuration rows, grouped choices and protocol acknowledgments |
| 09 MCP servers | Implemented per-connection encrypted definitions, stdio/HTTP/SSE editor, enable/remove, and capability-checked new/load setup |
| 10 and 14 Workspace access | Implemented saved per-connection/workspace read/write/terminal policy, enforced callbacks, and current-versus-next-session display |
| 11 and 15 Connection details | Implemented session page with real transport state, endpoint, agent, reconnect and disconnect actions |
| 12 Session menu | Implemented rounded menu, detail-page links, native Markdown export and confirmed local-copy removal with replay cursor retained |
| 16 Connection logs | Implemented latest 200 lifecycle events, filters, pause/live updates and copy; raw failures and wire payloads excluded |

These pages must use real host and protocol data. Do not add sample provider names, invented capabilities or inert controls to resemble the images. Connection and agent transport remain WebSocket and stdio. Keep accessibility targets and the wide-window navigation rail while applying the same visual language.

The MCP editor retains its normal portrait Save footer; below 420 dp or above 1.3× text scale, Save becomes part of the scrolling form. IME insets reduce the available form area so the action stays above the keyboard. Transport controls wrap as complete buttons instead of squeezing labels; Save has a minimum 52 dp height. Appearance/history dialog bodies scroll independently of Done. Actual landscape/system-font 2.0 settings checks and a full-window native-keyboard MCP check pass. Screenshots are `.local/screenshots/mcp-editor-keyboard.png`, `settings-history-dark.png` and `settings-history-light.png`. The mockup folder was absent at its supplied path during this increment; these refinements follow the previously recorded reference rather than a new image comparison.

Pairing keeps the entered address, name and code fixed while a request runs; fields and submission disable together. Failed requests leave the inputs for an explicit retry. Normal and actual system 2× keyboard component checks pass. MCP save failures retain the editor's fields; a full Settings route check verifies explicit successful encrypted retry without automatically launching a host session.

New session shows up to five recent workspace/agent combinations for the chosen host. Selecting a row restores both choices. Disabled or missing agents and paths outside current allowed roots are excluded; folder existence is checked by the host at launch. Agent-specific navigation limits recent rows to that agent. Connections show the discovered available-agent count only while online. Session rows prefer reported titles and update timestamps; local checkpoint times are separately labeled Saved. Session action targets are at least 48 dp.

The new-session chooser retains a fixed action footer in regular portrait windows. Below 420 dp available height or above 1.3× text scale, access, MCP and Start become separate items in the scrolling list. This avoids a fixed footer consuming the list or wrapping a settings link down a narrow column. A 320×280 dp fixture with 2× text and actual emulator landscape both verify agent selection and all actions; the latter is captured in `.local/screenshots/chooser-landscape.png`. This is page-specific evidence, not a full tablet/large-font acceptance claim.

Session rows now observe independent live activities across navigation: Working, Waiting for approval, Syncing, Ready and explicit connection states. Green indicators require a connected session whose replay has completed. An older host-reported running value is labeled Last reported: Working when the phone has no live session observer.

Registry Agent details shows the executable path, version and configuration reason only when supplied by the host. Paths and reasons are selectable text. A configuration problem prevents session creation even when the executable is installed; agent/workspace selection and recent choices apply the same rule.

Agents list/detail pages can show a dated saved catalog while fresh discovery is pending or unavailable. The notice identifies saved data, list rows say Last reported, and New session stays disabled until fresh data succeeds. The Refresh action remains visible.

Shared page errors use the same neutral inset surface as conversation recovery, with muted text and explicit Retry/dismiss controls. Avoid a solid red banner across the page. The saved-agent failure/recovery screenshot is `.local/screenshots/saved-agent-details.png`.

Folder dialogs in New session and Workspace access show the requested path and keep loading/failure/Retry inside the dialog. Loading or failed requests hide previous subfolder rows and disable Use this folder. Close returns without selecting a workspace. Empty allowed roots explain host configuration; a successfully browsed empty folder remains explicitly selectable.

Browsing has separate request state, so its loading and errors do not also appear in the underlying page. Close or a new folder cancels the previous request; a generation guard prevents late results from replacing the current folder. Normal and actual system 2× font screenshots are `.local/screenshots/workspace-browser-error.png` and `workspace-browser-error-large-text.png`; Retry and Close remain reachable in the large-text dialog.

Prompt history uses a neutral bottom sheet reached through the composer's + menu. Rows show a compact three-line preview of retained submitted text. Choosing a row fills the draft; existing draft replacement requires confirmation and Send remains explicit. Empty history explains its conversation retention scope.

The original request also requires rich conversation controls beyond these mockups. A full Changes page follows the same visual system, with file selection, modification navigation, expandable unchanged sections, syntax colors and wide-window split view. The composer supports native file/image/audio selection and context references. Received content uses compact title/type rows and explicit image-preview, audio-playback, resource-save and link actions in conversations and expanded tools. Recovery messages use neutral inset surfaces with explicit dismissal, pairing, session-list or reconnect actions. Broader codec/device/accessibility verification remains separate work.
