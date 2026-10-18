// A local copy of each conversation, so a session that ended when the host restarted can still be
// read. The host keeps only metadata after a restart; this is the app's own saved copy, kept in this
// app's storage on this computer. It is bounded: a few hundred KB per session and the 30 most recent.
import { emptyTimeline, Timeline } from './acp'

const PREFIX = 'acportal.transcript.'
const INDEX = 'acportal.transcript.index'
export const MAX_BYTES = 400_000
export const MAX_SESSIONS = 30

const read = (k: string): string | null => { try { return localStorage.getItem(k) } catch { return null } }

export function keep(hostId: string, sessionId: string, t: Timeline): void {
  if (!t.items.length) return
  const key = `${hostId}/${sessionId}`
  try {
    // Only the readable part: no live state (busy turn, open request ids) comes back later.
    const text = JSON.stringify({ items: t.items, title: t.title, usage: t.usage, savedAt: Date.now() })
    if (text.length > MAX_BYTES) return
    localStorage.setItem(PREFIX + key, text)
    const order = [key, ...(JSON.parse(read(INDEX) ?? '[]') as string[]).filter((k) => k !== key)]
    for (const old of order.slice(MAX_SESSIONS)) localStorage.removeItem(PREFIX + old)
    localStorage.setItem(INDEX, JSON.stringify(order.slice(0, MAX_SESSIONS)))
  } catch { /* storage full or unavailable: the copy is a convenience */ }
}

/** The saved conversation, or undefined if there is none. Nothing in it is still running. */
export function recall(hostId: string, sessionId: string): Timeline | undefined {
  const raw = read(`${PREFIX}${hostId}/${sessionId}`)
  if (!raw) return undefined
  try {
    const v = JSON.parse(raw) as { items?: Timeline['items']; title?: string; usage?: Timeline['usage'] }
    if (!Array.isArray(v.items)) return undefined
    // A request that was open when the process ended can never be answered now.
    const items = v.items.map((i) => ((i.kind === 'permission' || i.kind === 'elicitation') && !i.answered ? { ...i, answered: 'ended' } : i))
    return { ...emptyTimeline(), items, title: v.title, usage: v.usage }
  } catch { return undefined }
}

export function forget(hostId: string, sessionId: string): void {
  try {
    localStorage.removeItem(`${PREFIX}${hostId}/${sessionId}`)
    localStorage.setItem(INDEX, JSON.stringify((JSON.parse(read(INDEX) ?? '[]') as string[]).filter((k) => k !== `${hostId}/${sessionId}`)))
  } catch { /* ignore */ }
}
