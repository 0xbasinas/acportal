// Every paired host is polled in the background so the sidebar, the host switcher and the
// new-session dialog can show all of them at once. The active host keeps its own faster loop
// in App (timelines, tasks); this one only needs status, agents, folders and sessions.
import { useCallback, useEffect, useRef, useState } from 'react'
import type { AgentInfo, Host, HostStatus, SessionInfo, Workspace } from './types'

export type HostState = 'connecting' | 'connected' | 'reconnecting' | 'offline' | 'repair'
export type FleetEntry = {
  state: HostState
  latency?: number
  status?: HostStatus
  agents: AgentInfo[]
  sessions: SessionInfo[]
  workspaces: Workspace[]
  error?: string
  failures: number
  checkedAt?: number
}
export type Fleet = Record<string, FleetEntry>

const EMPTY: FleetEntry = { state: 'connecting', agents: [], sessions: [], workspaces: [], failures: 0 }
// A host that answered before gets this many failed polls as "reconnecting" before "offline".
const GRACE = 3

type R<T> = { ok: true; value: T } | { ok: false; error: string; code?: string; status?: number }
const call = <T,>(id: string, path: string): Promise<R<T>> => window.api.hosts.api(id, 'GET', path) as Promise<R<T>>

export async function probe(h: Host, prev: FleetEntry = EMPTY): Promise<FleetEntry> {
  const t0 = performance.now()
  const s = await call<HostStatus>(h.id, '/v1/status')
  const latency = Math.round(performance.now() - t0)
  if (!s.ok) {
    if (s.status === 401 || s.code === 'locked') return { ...prev, state: 'repair', error: s.code === 'locked' ? 'Token was kept only in memory; pair again' : 'This device is no longer paired', failures: prev.failures + 1, checkedAt: Date.now() }
    const failures = prev.failures + 1
    const wasUp = prev.state === 'connected' || prev.state === 'reconnecting'
    return { ...prev, state: wasUp && failures <= GRACE ? 'reconnecting' : 'offline', error: s.error, failures, checkedAt: Date.now(), latency: undefined }
  }
  const [a, w, ss] = await Promise.all([call<AgentInfo[]>(h.id, '/v1/agents'), call<Workspace[]>(h.id, '/v1/workspaces'), call<SessionInfo[]>(h.id, '/v1/sessions')])
  return {
    state: 'connected', latency, status: s.value, failures: 0, checkedAt: Date.now(),
    agents: a.ok ? a.value : prev.agents, workspaces: w.ok ? w.value : prev.workspaces, sessions: ss.ok ? ss.value : prev.sessions
  }
}

export function useFleet(hosts: Host[] | null, every = 5000): { fleet: Fleet; reconnect: (id?: string) => void } {
  const [fleet, setFleet] = useState<Fleet>({})
  const ref = useRef<Fleet>({})
  const busy = useRef(new Set<string>())
  const poll = useCallback(async (h: Host) => {
    if (busy.current.has(h.id)) return
    busy.current.add(h.id)
    try {
      const next = await probe(h, ref.current[h.id])
      ref.current = { ...ref.current, [h.id]: next }
      setFleet(ref.current)
    } finally { busy.current.delete(h.id) }
  }, [])
  useEffect(() => {
    if (!hosts) return
    const ids = new Set(hosts.map((h) => h.id))
    ref.current = Object.fromEntries(Object.entries(ref.current).filter(([k]) => ids.has(k)))
    setFleet(ref.current)
    const all = (): void => { for (const h of hosts) void poll(h) }
    all()
    const t = setInterval(all, every)
    return () => clearInterval(t)
  }, [hosts, every, poll])
  const reconnect = useCallback((id?: string) => {
    for (const h of hosts ?? []) if (!id || h.id === id) {
      ref.current = { ...ref.current, [h.id]: { ...(ref.current[h.id] ?? EMPTY), state: ref.current[h.id]?.state === 'connected' ? 'connected' : 'connecting', failures: 0 } }
      setFleet(ref.current)
      void poll(h)
    }
  }, [hosts, poll])
  return { fleet, reconnect }
}

export const STATE_LABEL: Record<HostState, string> = { connecting: 'Connecting…', connected: 'Connected', reconnecting: 'Reconnecting…', offline: 'Offline', repair: 'Needs re-pair' }
export const STATE_DOT: Record<HostState, string> = { connecting: 'amber live', connected: 'green', reconnecting: 'amber live', offline: 'red', repair: 'red' }
const RANK: Record<HostState, number> = { connected: 0, connecting: 1, reconnecting: 2, repair: 3, offline: 4 }

/** One line for several hosts: the worst state decides the colour, the counts say the rest. */
export function aggregate(hosts: Host[], fleet: Fleet): { state: HostState; label: string } {
  const states = hosts.map((h) => fleet[h.id]?.state ?? 'connecting')
  const up = states.filter((s) => s === 'connected').length
  const worst = states.reduce<HostState>((w, s) => (RANK[s] > RANK[w] ? s : w), 'connected')
  if (hosts.length <= 1) return { state: worst, label: STATE_LABEL[worst] }
  const label = up === hosts.length ? `All ${hosts.length} hosts connected` : `${up} of ${hosts.length} hosts connected`
  return { state: worst, label }
}
