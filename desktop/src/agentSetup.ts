// Models and modes an ACP agent offers, in the shape the new-session composer uses. The host
// reports the raw ACP `session/new` fields (`modes`, `configOptions`) per session and, after a
// probe, per agent (`GET /v1/agents` -> `setup`).
export type AgentSetup = { modes?: { id: string; name: string }[]; mode?: string; modelConfig?: string; models?: { value: string; name: string }[]; model?: string }
type Raw = { modes?: { currentModeId?: string; availableModes?: { id: string; name: string }[] } | null; configOptions?: { id: string; category?: string; currentValue?: string; options?: { value: string; name: string }[] }[] | null }

export function setupFromAcp(raw: unknown, chosen?: { mode?: string; model?: string }): AgentSetup {
  const st = (raw ?? {}) as Raw
  const mo = st.configOptions?.find((o) => o.id === 'model' || o.category === 'model')
  return {
    modes: st.modes?.availableModes?.map((m) => ({ id: m.id, name: m.name })),
    mode: chosen?.mode ?? st.modes?.currentModeId ?? undefined,
    modelConfig: mo?.id,
    models: mo?.options?.map((o) => ({ value: o.value, name: o.name })),
    model: chosen?.model ?? mo?.currentValue
  }
}
export const hasChoices = (s?: AgentSetup): boolean => Boolean(s?.modes?.length || s?.models?.length)

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
