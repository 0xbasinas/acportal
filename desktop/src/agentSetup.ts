// The session options an ACP agent offers (model, mode, reasoning effort, ...), read the same way
// the phone does (SessionDetailScreens.kt): when the agent sends `configOptions`, every `select`
// option is offered by its own name, with its groups; only otherwise the older `models` and `modes`
// fields. Nothing is invented: an agent that reports nothing gets no choices. The host reports the
// raw ACP `session/new` fields per session and, after a probe, per agent (`GET /v1/agents` -> `setup`).

export type Choice = { value: string; name: string; description?: string; group?: string }
/** One thing the user can choose. `kind` says how it is set: `config` with
 * `session/set_config_option`, `model` with `session/set_model`, `mode` with `session/set_mode`. */
export type Selector = { id: string; name: string; description?: string; category?: string; kind: 'config' | 'model' | 'mode'; current?: string; choices: Choice[] }
/** What the new-session composer offers: the selectors and the values picked so far (selector id -> value). */
export type AgentSetup = { selectors?: Selector[]; chosen?: Record<string, string> }

type RawValue = { value?: unknown; name?: unknown; description?: unknown; options?: unknown }
type RawOption = { id?: unknown; name?: unknown; description?: unknown; category?: unknown; type?: unknown; currentValue?: unknown; options?: unknown }
type Raw = {
  configOptions?: RawOption[] | null
  models?: { currentModelId?: unknown; availableModels?: { modelId?: unknown; name?: unknown; description?: unknown }[] } | null
  modes?: { currentModeId?: unknown; availableModes?: { id?: unknown; name?: unknown; description?: unknown }[] } | null
}
const str = (v: unknown): string | undefined => (typeof v === 'string' ? v : undefined)

/** Flat or grouped (`{ group, name, options: [...] }`) option values, each with its group name. */
function choicesOf(options: unknown): Choice[] {
  if (!Array.isArray(options)) return []
  const out: Choice[] = []
  for (const entry of options as RawValue[]) {
    if (entry && Array.isArray(entry.options)) {
      const group = str(entry.name)
      for (const v of entry.options as RawValue[]) { const value = str(v?.value); if (value !== undefined) out.push({ value, name: str(v.name) || value, description: str(v.description), group }) }
    } else {
      const value = str(entry?.value)
      if (value !== undefined) out.push({ value, name: str(entry.name) || value, description: str(entry.description) })
    }
  }
  return out
}

export function selectorsFromAcp(raw: unknown): Selector[] {
  const st = (raw ?? {}) as Raw
  const config = (Array.isArray(st.configOptions) ? st.configOptions : []).filter((o) => o && (o.type === undefined || o.type === 'select') && str(o.id))
  if (config.length) {
    return config.map((o) => ({ id: str(o.id)!, name: str(o.name) || str(o.id)!, description: str(o.description), category: str(o.category), kind: 'config' as const, current: str(o.currentValue), choices: choicesOf(o.options) }))
  }
  const out: Selector[] = []
  const models = st.models?.availableModels
  if (Array.isArray(models) && models.length) {
    out.push({ id: 'model', name: 'Model', category: 'model', kind: 'model', current: str(st.models?.currentModelId), choices: models.flatMap((m) => { const value = str(m?.modelId); return value === undefined ? [] : [{ value, name: str(m.name) || value, description: str(m.description) }] }) })
  }
  const modes = st.modes?.availableModes
  if (Array.isArray(modes) && modes.length) {
    out.push({ id: 'mode', name: 'Mode', category: 'mode', kind: 'mode', current: str(st.modes?.currentModeId), choices: modes.flatMap((m) => { const value = str(m?.id); return value === undefined ? [] : [{ value, name: str(m.name) || value, description: str(m.description) }] }) })
  }
  return out
}

/** The ACP request that sets `value` on a selector. */
export function setRequest(s: Selector, value: string): { method: string; params: Record<string, string> } {
  if (s.kind === 'model') return { method: 'session/set_model', params: { modelId: value } }
  if (s.kind === 'mode') return { method: 'session/set_mode', params: { modeId: value } }
  return { method: 'session/set_config_option', params: { configId: s.id, value } }
}

/** A value's display name, or the raw value when the agent did not list it. */
export const choiceName = (s: Selector, value: string | undefined): string | undefined =>
  value === undefined ? undefined : s.choices.find((c) => c.value === value)?.name ?? value

/** Lists longer than this get a search box (the phone switches to its searchable dialog above 40). */
export const SEARCH_ABOVE = 40
export function filterChoices(choices: Choice[], query: string): Choice[] {
  const q = query.trim().toLowerCase()
  if (!q) return choices
  return choices.filter((c) => c.name.toLowerCase().includes(q) || c.value.toLowerCase().includes(q) || (c.group ?? '').toLowerCase().includes(q))
}

/** A selector's value in a live session: what the agent last reported, else what it started with. */
export function liveValue(s: Selector, live?: { config?: Record<string, string>; currentMode?: string; currentModel?: string }): string | undefined {
  if (s.kind === 'mode') return live?.currentMode ?? s.current
  if (s.kind === 'model') return live?.currentModel ?? s.current
  return live?.config?.[s.id] ?? s.current
}
/** Selectors for a live session, using the agent's latest option list when it sent one. */
export const liveSelectors = (setup: unknown, live?: { configOptions?: unknown[] }): Selector[] =>
  selectorsFromAcp(live?.configOptions ? { ...((setup ?? {}) as object), configOptions: live.configOptions } : setup)

export function setupFromAcp(raw: unknown, chosen?: Record<string, string>): AgentSetup {
  return { selectors: selectorsFromAcp(raw), chosen: chosen ?? {} }
}
export const hasChoices = (s?: AgentSetup): boolean => Boolean(s?.selectors?.some((x) => x.choices.length))
/** The model selector first (by category or id), then the rest in the agent's order. */
export const modelFirst = (list: Selector[]): Selector[] => [...list].sort((a, b) => Number(b.category === 'model' || b.id === 'model') - Number(a.category === 'model' || a.id === 'model'))

/** Where an anchored dropdown goes: below (or above) its chip, flipped when the preferred side
 * has no room and the other has more, shifted left so it stays inside the window. */
export function placePopover(anchor: { top: number; bottom: number; left: number }, size: { width: number; height: number }, view: { width: number; height: number }, prefer: 'down' | 'up' = 'down', margin = 8): { up: boolean; dx: number; maxHeight: number } {
  const gap = 6
  const below = view.height - anchor.bottom - gap - margin
  const above = anchor.top - gap - margin
  let up = prefer === 'up'
  if (up && size.height > above && below > above) up = false
  else if (!up && size.height > below && above > below) up = true
  const right = anchor.left + size.width
  let dx = 0
  if (right > view.width - margin) dx = view.width - margin - right
  if (anchor.left + dx < margin) dx = margin - anchor.left
  return { up, dx, maxHeight: Math.max(120, Math.floor(up ? above : below)) }
}
