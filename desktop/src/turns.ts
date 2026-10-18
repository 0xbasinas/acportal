// How long each turn of a conversation took, measured by this app while it watched the turn run.
// The host does not time anything, and a conversation replayed later has no times, so these are kept
// here, keyed by the turn's prompt event, which has the same id every time it is replayed.
export type TurnTime = { at: number; took?: number }
const PREFIX = 'acportal.turns.'
const cache = new Map<string, Record<string, TurnTime>>()

function load(key: string): Record<string, TurnTime> {
  let m = cache.get(key)
  if (!m) {
    try { m = JSON.parse(localStorage.getItem(PREFIX + key) ?? '{}') as Record<string, TurnTime> } catch { m = {} }
    cache.set(key, m)
  }
  return m
}
function save(key: string): void { try { localStorage.setItem(PREFIX + key, JSON.stringify(load(key))) } catch { /* a convenience only */ } }

/** `key` is `host/session`. Safe to call twice for the same turn. */
export function turnStarted(key: string, turnId: string, now = Date.now()): void {
  const m = load(key)
  if (m[turnId]) return
  m[turnId] = { at: now }
  save(key)
}
export function turnEnded(key: string, turnId: string, now = Date.now()): void {
  const t = load(key)[turnId]
  if (!t || t.took !== undefined) return
  t.took = Math.max(0, now - t.at)
  save(key)
}
export const turnTimes = (key: string): Record<string, TurnTime> => load(key)
export const workedMs = (key: string): number => Object.values(load(key)).reduce((n, t) => n + (t.took ?? 0), 0)

/** 45s, 3m 05s, 1h 12m. */
export function duration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000))
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, '0')}s`
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, '0')}m`
}
/** The short form for a narrow list: 45s, 3m, 1h 12m. */
export function briefDuration(ms: number): string {
  const s = Math.round(ms / 1000)
  if (s < 60) return `${s}s`
  const m = Math.round(s / 60)
  return m < 60 ? `${m}m` : `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, '0')}m`
}
