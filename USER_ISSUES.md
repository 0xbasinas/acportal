# User issues

Issues reported by the user from phone testing. Open items only; move an item to `TODO_DONE.md` once it is fixed and verified.

## 1. No built-in agents: the user registers their own agent CLIs

The user must register their preferred agent CLIs themselves. The project must not ship or assume any default agent (Goose, Gemini or others).

- Remove the built-in definitions in `acpd/registry/builtin.json` as defaults; an empty registry is a valid starting state.
- The example registry/config and docs (`examples/`, `docs/agents.md`, `docs/installation.md`) should show how to add an agent, not present one as the default.
- The Android app and host must handle "no agents registered" with clear copy and an explicit way to recover, not an error. No Android behaviour may depend on agent names.
- Needs a decision on how a user registers an agent (edit the registry file, or an `acpd agents add` command) and on migrating existing setups that rely on the built-in Goose entry.

## 2. Improve scrolling inside an active session

Scrolling in a live conversation needs to be smoother and more reliable.

- Already fixed in PR #18: follow-latest no longer snaps back to the top of a reply taller than the screen. That fix is not yet verified on a physical phone with a real streamed reply.
- Still to investigate: jank while a reply streams, scroll position jumping when content or the keyboard changes, and how follow-latest turns on and off while the user is reading or dragging.
- Verify on a real phone with long streamed replies; emulator results are not enough.

## 3. Smoother animations and transitions

Duration is now right (about 90 ms), but the transitions still need to look smoother on every screen.

- Remove any remaining glitch, flash or jump on push, Back and predictive Back, across all screens including dialogs, sheets and the keyboard.
- Keep the current duration; improve smoothness only (easing, no layout shifts during the transition, no dropped frames).
- Measure frame behaviour on a physical phone; the debug-emulator numbers are not representative.

## 4. Show folders nicely, especially Windows paths

Folder paths in the app, especially Windows ones, look poor (raw `C:/Users/...` strings).

- Show a clear folder name first, with the full path as secondary text, in the folder browser, the new-session form, Sessions rows and workspace access.
- Display Windows paths consistently (drive letter, separators, long paths truncated in the middle) without changing the exact path sent to the host.
- Check long and deeply nested paths at large font sizes and in both themes.

## 5. Add folders and projects without the CLI or config file

Authorized workspace roots are currently set only in the host config (`workspace_roots`). Users need an easier way to add folders or projects.

- Let the user add a project folder from the app (or a simple host command or UI), instead of editing `config.toml` by hand.
- Keep the security boundary: the host owns the authorization decision. A phone request to add a folder should need explicit approval on the host or an already-authorized operator action, never silent widening of roots. Needs a design decision.
- Show which folders are authorized, let the user remove them, and take effect without a full restart if possible (the host already supports a soft reload for some settings).
- Empty roots must still deny every workspace; show clear copy and a recovery action when none are set.

## 6. Make the session UI slightly smaller

In a conversation, the chat composer and the message bubbles (user and agent) are too large. Make them slightly smaller.

- Reduce the size of the composer (input field, buttons, padding) and of the user and agent bubbles (padding, text size, spacing) by a small amount, not a redesign.
- Keep touch targets accessible (48 dp minimum for tappable controls), text readable at large font sizes, and the composer reachable with the keyboard open.
- Check dark and light themes at normal and 2× font. Any visible change to the layout must first be reflected in the Penpot designs (see TODO.md "Ongoing contributor requirements").

## Status, 11 October 2026

- **1. No built-in agents:** done in code: the built-in list is empty and the phone registers agents on an **Add agent** screen that checks `<command> --version` first (docs/agents.md). Penpot boards 37-40. Not tried on a phone; existing setups that used the old Goose/Gemini built-ins must register them again.
- **2. Scrolling:** follow-latest now reaches the end of a tall reply (PR #18); a **Jump to latest** button appears when you have scrolled up (Penpot board 41). Streaming jank on a real phone is still unmeasured.
- **3. Transitions:** fade-through, animated bottom bar and overlaid progress bar (PR #23). Still needs a feel check on a phone.
- **4. Folder display:** done (PR #21); the Windows verbatim prefix is also removed by the host.
- **5. Adding folders from the app:** done (PR #22), Penpot board 36.
- **6. Smaller session UI:** done (PR #21), Penpot board 41 shows the compact composer and bubbles. The older conversation boards were not edited.
