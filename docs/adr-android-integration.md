# ADR: Android system integration (share, voice, camera, widgets, quick access)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Mobile agent clients compete on how little friction it takes to hand work to an agent. Nimbalyst's phone app supports camera and photo attachments, voice (find or switch a session, answer a pending question, send a task), a Lock Screen and Dynamic Island view ranked by how long a session has waited, and a queue that sends after reconnect. ChatGPT for iOS added Home Screen widgets that show recent tasks from the selected computers (2 October 2026) and task links that open directly in the app. Forge Remote and CC Pocket push a notification when an agent needs permission and put approval on the session list. Wispr Flow shows a floating dictation button on Android. See [research notes](adr-ade-index.md#research-notes).

This Android app already has pairing by QR, native Markdown, attachments (`AttachmentControls.kt`), a terminal, preview, git and task screens, and a session list with live status. A keyword search of the Android sources found no share-target handling, no voice input and no widgets.

## Decision

Four small additions, each independent and each respecting the existing consent rules. None sends anything without the person seeing it.

1. **Share target**: register `ACTION_SEND` for text, URLs and images. Sharing opens "Send to agent": choose host and session (or "new session"), see the content and edit it, then send. Shared text and URLs become the draft; images go through the existing attachment limits and the "agent does not accept images" check. Nothing is sent from the share intent itself.
2. **Voice dictation**: a microphone button in the composer using Android's system `SpeechRecognizer` (on-device where the device supports it) or the keyboard's own dictation. The recognised text lands in the draft for review. No audio is stored or sent to this project's host. A "voice answer" shortcut on an approval or form question fills the form field, never taps Approve.
3. **Camera**: "Take photo" in the attachment menu, with the same limits as picked files. The app asks for the camera permission only when the button is first used.
4. **Glanceable surfaces**:
   - A notification per session that needs a person (approval, question), with Open as the only action; Approve and Deny stay inside the app where the full request is shown. (This is the phone side of [adr-attention-board](adr-attention-board.md).)
   - A Glance widget listing sessions that need attention with their waiting time; tapping opens the session.
   - A Quick Settings tile that opens the New session screen.
   - Link handling for `acportal://session/<host>/<id>`, shared with the desktop deep links in [adr-issue-tracker](adr-issue-tracker.md).

## Options Considered

### Option A: share, dictation, camera, notifications, widget (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium; each is separate. |
| Cost | Minimal. |
| Scalability | n/a |
| Team familiarity | Standard Android APIs; the app already has attachment and notification-free foreground work. |

**Pros:** lowers the cost of starting and answering work from a phone; no new host surface.
**Cons:** widget and notification content shows session titles on a lock screen, which some people won't want.

### Option B: approve and deny from the notification

**Pros:** fastest path. **Cons:** an approval decided without seeing the diff or command is the failure the review UI exists to prevent; deliberately left out.

### Option C: a wearable or Wear OS companion

**Pros:** glanceable. **Cons:** another app; not now.

## Consequences

- A setting hides titles on the lock screen (`VISIBILITY_PRIVATE` by default for notifications and the widget).
- Android 13+ notification permission is requested only when the person turns notifications on; the app says that notifications work only while it holds a connection ([adr-attention-board](adr-attention-board.md) option C).
- Tests: share intents with oversized text, many images and a non-text MIME type; the dictation result never auto-sends; the widget and notification show no content when "hide titles" is on.
- Verification on a real phone is required for notifications and widgets; the emulator has been the only device so far.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Share target and composer dictation first (smallest, most useful).
3. [ ] Notifications with the attention board.
4. [ ] Widget and tile after a real-device run.
