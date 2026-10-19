# ADR: a chat bridge (Telegram first) for notifications and light control

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

The official ACP clients list has more than a dozen messaging bridges: OpenACP (Telegram, Discord and Slack), acp-connector (Telegram and Discord), ACP Router (a Telegram client with diffs and approvals), Telegram-ACP (multi-thread chat), Sniptail (Discord and Slack), cc-connect (several platforms), Lark, WeChat, QQ and Matrix bridges, a Pomerium AgentOps Slack integration, and AgentConnect (agents in Slack, Telegram and Discord). Cursor and Devin start agents from Slack. Bellows lets prompts be answered from a phone but requires the machine to save a rule. Codeg has Telegram, Lark and WeChat channels. See [research notes](adr-ade-index.md#research-notes).

That is a strong demand signal for "answer my agent from the chat app I already have open". It is also a risky one: a chat bot is a new authenticated entry point, and the chat platform sees whatever is posted. The earlier watch-list entry promised a security review first. [adr-host-hooks](adr-host-hooks.md) already provides the safest piece: outbound notifications with no content.

## Decision

Build in stages, stopping after any stage that is enough.

1. **Stage 1: notify only (covered by host hooks).** A hook posts "Agent X needs approval in 'Fix login' on host Y" with a deep link to a Telegram chat (through the Bot API) or any webhook. No content, no commands accepted.
2. **Stage 2: a Telegram control bot, outbound long polling only.** The host polls the Bot API (`getUpdates`), so no inbound port. The bot answers only to an allowlist of Telegram user ids set in host config, in a private chat. Commands: `/sessions` (titles and states), `/status <n>`, `/stop <n>` (cancel the turn), and replying to an approval message with **Approve** or **Deny** inline buttons *only for requests the host classifies as low risk* ([adr-risk-ranked-review](adr-risk-ranked-review.md) classes, shell-review `Allow`-class lines). Anything else shows "Open in the app" with a deep link.
3. **The bot is a device.** It has a device record with a low rights level (`approver`, see [adr-device-roles](adr-device-roles.md)), so route middleware, audit and revocation apply as for any device. Its Telegram user id is the credential binding; a message from any other id is dropped silently.
4. **Content stays out of chat by default.** Messages carry titles and short reasons. "Show details" is a per-chat opt-in that adds the command line or file path for approvals, with a warning that Telegram's servers will see it. Diffs and file contents are never sent.
5. **No prompts from chat in stage 2.** Starting or steering work from a chat message is a separate decision after the approval flow has been used.
6. Other platforms (Slack, Discord, Matrix) need either inbound webhooks or websocket sessions and are not part of this ADR.

## Options Considered

### Option A: notify-only first, then a Telegram bot with low-risk approvals (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low for stage 1, medium for stage 2. |
| Cost | None. |
| Scalability | A handful of chats. |
| Team familiarity | Uses hooks, rights and audit proposed elsewhere. |

**Pros:** gives push-like behaviour without a relay or a closed-app limitation; the bot cannot do more than its rights allow; no inbound port.
**Cons:** Telegram's servers see whatever is sent and are not end-to-end encrypted for bots; a stolen Telegram account could approve low-risk requests.

### Option B: a full Slack/Discord/Telegram multi-platform bridge (OpenACP style)

**Pros:** broad. **Cons:** several platform integrations, inbound surfaces, and prompts from chat; too much at once.

### Option C: no chat integration

**Pros:** smallest attack surface. **Cons:** ignores the most common request from comparable users.

## Consequences

- `docs/security.md` gains a section on the bot token (a host secret), the user-id allowlist, what Telegram sees, and the low-risk-only approval rule.
- Tests with a fake Bot API server: messages from a non-allowlisted id are ignored; approval buttons appear only for low-risk requests; a revoked bot device stops working; no content leaves without the details opt-in.
- Needs a real Telegram run before any claim of working.

## Action Items

1. [ ] User reviews this ADR; accept [adr-host-hooks](adr-host-hooks.md) and [adr-device-roles](adr-device-roles.md) first.
2. [ ] Stage 1 via hooks.
3. [ ] Decide whether stage 2 is wanted after using stage 1.
