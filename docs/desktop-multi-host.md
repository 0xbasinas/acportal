# Desktop app: several hosts

The desktop app can be paired with any number of acpd hosts (your workstation, a build box, a VM) and works with all of them at once.

## Adding a host

1. On the host, run `acpd pair` and keep the one-use code or link it prints.
2. In the app, open the host switcher (top left) and choose **Add host**, or go to **Settings › Hosts › Add host**, and paste the code or link.
3. Give the host a name. Each host keeps its own device token, stored in the OS keychain when one is available (Settings › Hosts shows where).

To pair again after a token expired or was revoked, use **Re-pair** on that host. **Unpair** revokes this device on the host and forgets it locally.

## Everything live at once

- The app polls every paired host every 5 seconds (status, agents, workspaces, sessions and, on orchestrator hosts, tasks). The dot next to each host shows *Connected*, *Reconnecting*, *Offline* or *Needs re-pair*; the switcher button shows the overall state.
- For every running session on a connected host, the app keeps a read-only event stream open. Read-only streams never take control of a session, so a phone or another computer can keep driving it.
- **Approval badges.** A red number appears on the session row, on its host's heading in the sidebar, on the host in the switcher popover ("N waiting"), and on the switcher button (total). You do not need to switch hosts to notice an agent waiting.
- **Desktop notifications** fire for new approval requests and for finished turns and tasks in sessions you are not looking at. Clicking one opens the session on the right host. On macOS and Windows an approval notification has an **Approve once** button; it is never offered when the request names paths outside the workspace, and on Linux the click opens the session so you decide there. Notifications are skipped while the app window is focused on that session.
- Approving from a notification for a session you are not attached to briefly takes control of that session to send the answer, then releases it. If that fails (for example because another device controls the session), open the session and answer there.

## Organising hosts

- **Sidebar.** Sessions are grouped by host; the filter chips at the top show all hosts or one.
- **Order.** Drag rows in **Settings › Hosts** (or use the up/down buttons) to set the order used in the sidebar, switcher and palette.
- **Rename** a host with the pencil button; the name is local to this computer.
- **Command palette** (Ctrl/Cmd K): *Switch host* jumps between hosts; **New session** asks which host to start on when more than one is connected.

## Shell commands and the workspace

Each host enforces its own rules (see [security](security.md#workspace-scope-of-shell-commands)). In the app:

- An approval card that names paths outside the session's workspace shows a red **Outside the workspace** box listing them. Read these carefully; reject anything you did not expect.
- A card for a host command says whether it will run in the host's sandbox ("Sandboxed to the workspace") or unconfined. Commands from an agent's own built-in tool always run unconfined; the host can only warn.
- The sandbox is per agent (`"sandbox": "auto" | "off"` in the host's agent registry, see [agents](agents.md)) and is available on Linux hosts with bubblewrap.

See also [run on several agents and the pre-merge checklist](desktop-runs-and-checks.md).
