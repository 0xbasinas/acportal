# ADR: keep the host awake while agents work

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Nimbalyst's mobile docs list sleep prevention with three settings (Off, Always, When plugged in) so a desktop stays awake during sync; its Mac app must otherwise be awake for the phone to reach it. The ChatGPT mobile app can connect to a host running the desktop Codex app, but the host must stay awake and online, and Codex task links keep the selected computer while it reconnects or is offline. Electron's `powerSaveBlocker` has two types, `prevent-app-suspension` (screen may turn off) and `prevent-display-sleep`, and no built-in "only when plugged in" option. See [research notes](adr-ade-index.md#research-notes).

This project's whole point is a phone talking to a computer that runs the agents. When the laptop sleeps, a running turn is suspended, the WebSocket drops, and the phone shows a dead host. `acpd service install` can keep the daemon running across logins, but a sleeping machine runs nothing. A search of the host and desktop sources found no sleep inhibition. The desktop app could hold an Electron blocker, but the daemon, which is what matters, is a separate Rust process and can run with no desktop app open.

## Decision

The **host daemon** holds a sleep inhibitor while there is work, with an explicit setting.

1. `[power] keep_awake = "off" | "while_working" | "always"` with `"while_working"` as the proposed default. `while_working` means: at least one turn running, a pending approval or elicitation, a delegated task, a host terminal with activity in the last 10 minutes, or a running automation ([adr-scheduled-runs](adr-scheduled-runs.md)). The inhibitor is released shortly (60 seconds) after the last of those ends.
2. `[power] only_on_ac = true` (default) skips holding the inhibitor on battery; the host reports "on battery: not keeping awake" so a surprise is visible. Battery state is read through the platform (Windows power status, macOS `pmset -g batt`, Linux `/sys/class/power_supply`); if it cannot be read, behave as if on AC and say so in `doctor`.
3. **Mechanisms**, each with the platform's own API and no new dependency beyond what the crate set already allows: Windows `SetThreadExecutionState` (system required, display not required) held from a dedicated thread; macOS an IOKit power assertion for idle system sleep (or the `caffeinate` binary as a fallback); Linux `systemd-inhibit` or the `org.freedesktop.login1` `Inhibit` call. The display may always sleep. Lid-close behaviour is a hardware setting this feature cannot override and the docs say so.
4. **Visible**: the status bar and the host screen on the phone show "Keeping this computer awake" with the reason, and `acpd status` prints it. `doctor` reports the mechanism available on this platform.
5. **Wake-ups are out of scope**: waking a sleeping machine (Wake-on-LAN, a relay that wakes it) is not attempted.

## Options Considered

### Option A: daemon-held inhibitor tied to work, with an AC-only default (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A small per-platform module and a state tracker. |
| Cost | Power draw while working. |
| Scalability | n/a |
| Team familiarity | `process_tree.rs` already has per-platform OS calls. |

**Pros:** fixes the most common "my host disappeared" cause; no effect when idle; visible.
**Cons:** a laptop in a bag with an active turn can get warm; hence AC-only default; macOS and Linux paths are unverified like other platform code here.

### Option B: only the desktop app holds Electron's blocker

**Pros:** trivial. **Cons:** useless when the daemon runs without the app.

### Option C: leave it to the user's OS power settings

**Pros:** none. **Cons:** silent failure from the phone's point of view.

## Consequences

- `docs/security.md` is unchanged except for noting the daemon asks the OS to stay awake; it exposes nothing new.
- Tests: the work-tracking state machine with a fake clock; release after the grace period; battery case. The OS calls themselves can only be checked on each platform's CI (Windows and Linux runners exist; macOS is skipped in some workflows) and on real machines.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: state tracker and the Windows mechanism first (the platform used for most verification).
3. [ ] Linux and macOS mechanisms; try on real machines before claiming them.
4. [ ] Status bar and phone display.
