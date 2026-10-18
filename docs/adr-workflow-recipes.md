# ADR: saved workflow recipes (parameterised, runnable from phone and desktop)

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Goose "recipes" are workflows saved as portable YAML: a prompt, extensions and model settings, with parameters and sub-recipes, shareable across a team and runnable in CI. Codex automations bundle a prompt with optional skills and triggers. Mux has customisable prompts per mode. Claude Code has slash-command and skill files. See [research notes](adr-ade-index.md#research-notes).

This project has prompt history (`PromptHistorySheet.kt`) and the checks config, but no way to say "run *this* kind of job again": same agent, workspace pattern, permission profile, checks and prompt with a blank to fill (an issue number, a package name). People retype or copy from history, and [adr-scheduled-runs](adr-scheduled-runs.md) and [adr-issue-tracker](adr-issue-tracker.md) both need a stored recipe to be useful.

Goose's recipe format belongs to Goose. Per the project's protocol-uniform rule, a recipe here must work with any ACP agent and not depend on an agent's own feature.

## Decision

Add host-stored **recipes**, a plain data object that creates an ordinary task.

1. A recipe is `{id, title, agent, profile, workspace (a root or alias), isolation, prompt (template), params, checks, budget}`. The prompt is Markdown with `{{name}}` placeholders. `params` declare name, label, type (`text`, `number`, `choice`, `path`), default and whether it is required. No expressions, loops or code: a placeholder is replaced by a string.
2. Storage: `[recipes.<id>]` in the host's user data (the same store as shared drafts), not in the repository. Export and import as a single TOML or JSON file; imported recipes arrive **disabled** and show their full prompt and every field for review.
3. Run: choose a recipe, fill the form, see the final prompt, confirm. The host creates a task with the recipe's agent, profile, isolation and budget, then sends the prompt once with a request id. Run history records the recipe id and parameter values (not secrets; a `secret` param type is rejected).
4. Safe defaults: a recipe may *narrow* but never widen the host's limits. It cannot enable auto-approve where the agent has no rule, cannot name a workspace outside the authorised roots, and cannot set a budget above the host maximum.
5. Chaining: a recipe may list follow-up recipe ids that are *offered* when the task finishes ("Open pull request", "Run review"). Nothing runs without a tap, unless the follow-up is on a scheduled automation that already carries its own limits.
6. Recipes are the unit [adr-scheduled-runs](adr-scheduled-runs.md) and [adr-issue-tracker](adr-issue-tracker.md) reference. Those ADRs store a recipe id plus parameters instead of a bare prompt.

## Options Considered

### Option A: host recipes as plain templates (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Low to medium. A store, a form renderer, a task creator. |
| Cost | None. |
| Scalability | Dozens of recipes. |
| Team familiarity | Same user-data store and task creation as delegation. |

**Pros:** removes retyping; makes automation safe to review; works for every agent.
**Cons:** a template cannot branch; shared recipes can carry harmful prompts, hence the disabled-on-import rule.

### Option B: adopt Goose's recipe format directly

**Pros:** an existing ecosystem. **Cons:** couples the host to one agent's schema and extension model.

### Option C: store recipes in the repository as files

**Pros:** version-controlled, shared by clone. **Cons:** a cloned repository would supply prompts and profiles; the same trust problem as [adr-workspace-scripts](adr-workspace-scripts.md) option B. Allowed only as a "suggested recipe" that needs an import tap.

## Consequences

- A recipe with `profile = Autopilot` shows the profile on the confirm screen in words ([adr-session-modes](adr-session-modes.md)).
- Tests: placeholder substitution with braces and backticks in values; a required parameter left empty blocks the run; an imported recipe is inert until enabled; a recipe cannot exceed host limits.
- Recipes hold prompt text and are treated as prompt content: never logged.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Host: recipe store, validation, run endpoint, tests.
3. [ ] Desktop and Android: recipe list, parameter form, "Save as recipe" from a past prompt.
4. [ ] Decide the import/export file format (TOML to match host config, or JSON to match the API).
