# UI review of comparable apps, and a Penpot plan

**Date:** 18 October 2026
**Status:** Review notes and a work plan. No Penpot board has been changed yet: the Penpot plugin was not connected to the session that wrote this (the MCP call failed with "No Penpot instance connected"), so the existing boards were not inspected and nothing below claims what they contain.

Companion to [the ADR index](adr-ade-index.md). The visual rules in [ui-reference.md](ui-reference.md) still win: near-black surfaces, neutral cards, white primary actions, muted secondary text, restrained green, 24 dp gutters, no solid red banners. Nothing here proposes copying another product's look.

## What was looked at

Marketing and README screenshots, downloaded to a scratch folder and viewed (not committed: they are other projects' images). Eleven images in total.

| Project | Images viewed | Not obtained |
|---|---|---|
| Codeg (desktop, dark) | workspace with agent picker | light theme viewed only by file size, not looked at |
| Mux (desktop, dark) | code review panel, agent status sidebar, plan with Mermaid diagram, context compaction dialog | cost tab and git status (downloaded, not examined) |
| Reemoat (desktop and phone, light) | session list with approval sheet, phone approval | none |
| Orca (desktop and phone, dark) | hero, mobile companion | none |
| Mobvibe (web, light/yellow accent) | new conversation dialog | file explorer (downloaded, not examined) |
| ACP UI (desktop, light) | chat with ACP traffic panel | none |
| Harnss, AgentDeck, Kepler, Bellows, Threadlines | none | Harnss images returned 403; the others have no static images in their READMEs |

These are curated promotional shots of single states, not flows, so nothing here says how an interaction behaves.

## Observations worth taking

**1. Approval sheet (Reemoat, phone and desktop).** The sheet states the question in plain words ("Allow Claude to run Prisma migrate dev to create the ProcessedWebhook table?"), shows a waiting time ("waiting <1m"), the literal command in a monospaced inset, and three actions in a fixed order: **Deny**, **Always allow**, **Allow once** (the primary, filled). On desktop the buttons carry keyboard hints (1, 2, 3). The composer under it reads "Answer the request above first" and is disabled, which makes the blocking state obvious. A collapse chevron and a close button sit in the header. This maps directly to [approval rules](adr-approval-rules.md) and [policy engine](adr-policy-engine.md): our sheet should add the waiting time, a matched-rule line, and an "Allow and remember..." that opens a narrow rule editor instead of a bare Always allow.

**2. Plan as a checklist (Reemoat).** The plan appears as a soft card above the reply with completed items struck through and a check mark, the current item with a disclosure arrow. That is the right weight for a phone and fits [plan review](adr-plan-review.md).

**3. Attention in the session list (Reemoat, Orca, Mux).** Reemoat's list header says "2 waiting" and a filled dot marks rows that need the person against hollow dots for the rest; rows show title, then `agent · project · branch` as one muted line, with a relative time and a three-dot menu. A left rail groups projects with count badges and an "All" entry; a bell with a dot sits beside search. Mux's sidebar shows, under each workspace name, one line of what it is doing now with a state icon (spinner, check, hollow circle), which answers "what is it up to" without opening it. Orca groups workspaces under "Pinned" and "In progress (17)" and shows the linked issue and PR state on each row. This is the model for [attention board](adr-attention-board.md) and [activity panel](adr-activity-panel.md).

**4. Review panel (Mux).** A right dock with tabs (Stats, Review 0/3, Instructions, Goal), a Files tree with per-folder `+40 -2` totals and a Structured/Flat toggle, filters (Uncommitted, Read), a sort control, and one card per file with a header (`+13 -2 just now`) and the diff below, the focused file outlined. Search sits at the top with regex and case toggles. It is dense but orderly, and it is the layout [risk-ranked review](adr-risk-ranked-review.md) and [review comments](adr-review-comments.md) need: a place for a risk chip next to each file's counts, a read/unread state, and a hunk action.

**5. Stats and context (Mux).** A cost view with a segmented bar and a table by component (cache read, cache create, input, output, thinking) and by model; a context usage bar showing `150 / 1.0M`; and a compaction dialog with a slider for the auto-compact threshold plus an "idle compaction" switch. Useful shapes for [usage dashboard](adr-usage-dashboard.md) and [context handoff](adr-context-handoff.md); the slider and idle compaction are Mux features and are not proposed here, because compaction is the agent's act in ACP.

**6. A home screen on the phone (Orca).** "Welcome back" with three summary tiles, a Desktops list with connection state and counts ("Connected, 40 worktrees, 5 active"), a Resume card for the last session, a Tasks entry, quick actions (Pair desktop, New worktree), and account usage bars per provider with 5 h and 7 d windows. The desktop shows the same usage as two thin bars in the status bar. Our Connections page is the nearest equivalent; a "needs you" and "resume" block above the host list would suit it. Account usage bars only make sense when an agent reports usage windows (ACP does not today), so that part stays out.

**7. New-session dialog (Mobvibe, Codeg).** Mobvibe's dialog: title, target host, backend dropdown, working directory with Browse, and a "Create in new worktree" checkbox. Codeg's composer puts the agent, permission mode, model and effort in one row of compact selectors under the input, and its send button has a dropdown with "Fork and Send". Our New session already has most of this; the gaps are a worktree choice, a profile picker ([account profiles](adr-account-profiles.md)) and a recipe entry ([workflow recipes](adr-workflow-recipes.md)).

**8. Protocol traffic panel (ACP UI).** A docked panel at the bottom: pause, clear, search, a type filter, then rows with a timestamp, a direction arrow, the method name and the request id on the right, expandable to the JSON. Good model for [protocol inspector](adr-protocol-inspector.md). The product around it is a light, utilitarian layout with a solid red Disconnect button, which we should not copy.

**9. Agent picker as a popover (Codeg).** Typing `@` opens a popover with tabs (Agents 12, Files 4, Sessions 50, Commits) and rows of name plus one-line description, which is the same pattern [composer helpers](adr-composer-mentions.md) needs.

**What not to take:** Mobvibe's yellow accent and Reemoat's light theme are their own identities; ours is neutral with white actions. ACP UI's red button and Orca's terminal-heavy density do not match the phone guide. None of the apps shows an accessibility treatment, so their contrast and target sizes should not be assumed good.

## Penpot plan

Once the plugin is connected, in this order. Each item is "export and inspect first, then change":

1. **Audit existing boards** (Page 1 of file `7eed092c-bad6-802e-8008-bea06e03019c`: boards 17 to 35, 96 to 105, 106 to 114 per this repository's notes). For each: export to PNG, check 24 dp gutters, 48 dp minimum targets, text contrast against the guide, consistent corner radii and spacing tokens, layer names, and whether both themes exist. List defects before changing anything.
2. **Approval sheet v2** (phone and desktop, both themes): waiting time, literal command inset, matched-rule line, Deny / Allow and remember... / Allow once, disabled composer with "Answer the request above first", and the rule editor (scope picker, expiry, narrowed pattern).
3. **Needs-you list and attention board**: the filled/hollow dot, "N waiting" header, the one-line activity under each row, a project rail with count badges (desktop), and the phone home block.
4. **End-of-turn strip**: the verified-facts line from [claim verification](adr-claim-verification.md) in neutral with one amber mismatch state.
5. **Plan card**: checklist with struck-through completed items, version control, and the Approve / Edit / Comment actions.
6. **Review panel**: file list with risk chips and read state, hunk card with Revert and Comment, "N high-risk reviewed" gate.
7. **Activity panel and protocol inspector** (desktop docks), reusing the terminal dock frame from boards 106 to 114.
8. **Context chip tray and composer popover** for `@` and `/`.
9. **New session additions**: worktree, profile, recipe.
10. **Light and dark variants and large-text states** for each new board, plus an empty, loading and failed state where the screen can have one.

Rules for the work: reuse existing components and tokens; no invented provider names or numbers in sample data; keep a short legend on each board naming the ADR it illustrates; do not delete or restyle an existing board without listing the change first.

## Blocked on

Connecting the Penpot MCP plugin to the session (open the file, run the plugin, confirm the connection). If image export is still unreliable (an earlier note says it was), the audit can use `execute_code` geometry checks but cannot do visual comparison.
