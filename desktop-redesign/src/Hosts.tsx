import { moveTo } from './order'
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { Host } from './types'
import { aggregate, STATE_DOT, STATE_LABEL, type Fleet, type FleetEntry } from './fleet'
import { I } from './ui'

const addr = (h: Host): string => h.address.replace(/^https?:\/\//, '')
export const hostFacts = (e?: FleetEntry): string[] => {
  if (!e || e.state !== 'connected') return []
  const out = [`${e.latency ?? '–'} ms`]
  if (e.status?.version) out.push(`acpd ${e.status.version}`)
  out.push(`${e.agents.length} agent${e.agents.length === 1 ? '' : 's'}`)
  return out
}
export function FeatureTags({ e }: { e?: FleetEntry }) {
  const f = e?.state === 'connected' ? e.status?.features ?? [] : []
  const tags = [['git', 'git'], ['sessionWorktree', 'worktrees'], ['orchestrator', 'tasks'], ['formElicitation', 'forms']].filter(([k]) => f.includes(k))
  if (!tags.length) return null
  return <span className="feat-tags">{tags.map(([k, l]) => <span key={k} className="feat-tag" title={`Host feature: ${k}`}>{l}</span>)}</span>
}

/** The top-left corner: the current host, its state, and a popover with every host. */
export function HostSwitcher({ waiting, hosts, current, fleet, onSwitch, onReconnect, onRepair, onSettings, onAdd }: {
  waiting: (hostRef: string) => number
  hosts: Host[]; current?: Host; fleet: Fleet
  onSwitch: (h: Host) => void; onReconnect: (id?: string) => void; onRepair: (h: Host) => void; onSettings: () => void; onAdd: () => void
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const pop = useRef<HTMLDivElement>(null)
  const [box, setBox] = useState<{ top: number; left: number; width: number; maxHeight: number }>()
  useLayoutEffect(() => {
    if (!open) { setBox(undefined); return }
    const el = ref.current
    if (!el) return
    const place = (): void => {
      const r = el.getBoundingClientRect()
      const margin = 8
      const room = Math.max(160, window.innerWidth - r.left - margin)
      const width = Math.min(room, Math.max(r.width, Math.min(360, room)))
      const top = Math.min(r.bottom + 6, window.innerHeight - margin - 160)
      setBox({ top, left: r.left, width, maxHeight: Math.max(160, window.innerHeight - top - margin) })
    }
    place()
    const watch = new ResizeObserver(place)
    watch.observe(el)
    window.addEventListener('resize', place)
    return () => { watch.disconnect(); window.removeEventListener('resize', place) }
  }, [open])
  useEffect(() => {
    if (!open) return
    const h = (e: MouseEvent): void => {
      const t = e.target as Node
      if (ref.current?.contains(t) || pop.current?.contains(t)) return
      setOpen(false)
    }
    const k = (e: KeyboardEvent): void => { if (e.key === 'Escape') setOpen(false) }
    document.addEventListener('mousedown', h); document.addEventListener('keydown', k)
    return () => { document.removeEventListener('mousedown', h); document.removeEventListener('keydown', k) }
  }, [open])
  const e = current ? fleet[current.id] : undefined
  const st = e?.state ?? 'connecting'
  const agg = aggregate(hosts, fleet)
  const facts = hostFacts(e)
  return (
    <div className="host-switcher" ref={ref}>
      <button className={'host-switch' + (open ? ' open' : '')} onClick={() => setOpen(!open)} aria-haspopup="menu" aria-expanded={open}
        title={`${current?.name ?? 'No host'}: ${STATE_LABEL[st]}${hosts.length > 1 ? ` · ${agg.label}` : ''}`}>
        <span className={'host-avatar s-' + st}>{(current?.name ?? '?').slice(0, 1).toUpperCase()}<span className={'dot ' + STATE_DOT[st]} /></span>
        <span className="grow host-lines">
          <span className="host-name ell">{current?.name ?? 'No host'}</span>
          <span className={'host-state s-' + st}>{STATE_LABEL[st]}{facts.length ? <span className="host-facts"> · {facts[0]}</span> : null}</span>
        </span>
        {hosts.length > 1 && <span className={'host-count s-' + agg.state} title={agg.label}>{hosts.filter((h) => fleet[h.id]?.state === 'connected').length}/{hosts.length}</span>}
        {hosts.reduce((n, h) => n + waiting(h.id), 0) > 0 && <span className="badge" title="Waiting for your approval on all hosts">{hosts.reduce((n, h) => n + waiting(h.id), 0)}</span>}
        <span className="chev-ic">{I.down}</span>
      </button>
      {open && box && createPortal(
        <div ref={pop} className="popover host-pop" role="menu" style={{ position: 'fixed', top: box.top, left: box.left, width: box.width, maxHeight: box.maxHeight, right: 'auto' }}>
          <div className="host-pop-head"><span>Hosts</span><span className={'agg s-' + agg.state}><span className={'dot ' + STATE_DOT[agg.state]} />{agg.label}</span></div>
          <div className="pop-list">
            {hosts.map((h) => {
              const f = fleet[h.id]
              const s = f?.state ?? 'connecting'
              const cur = h.id === current?.id
              return (
                <div key={h.id} className={'host-item' + (cur ? ' on' : '')}>
                  <button className="host-item-main" onClick={() => { onSwitch(h); setOpen(false) }} title={cur ? 'Current host' : `Switch to ${h.name}`}>
                    <span className={'dot ' + STATE_DOT[s]} />
                    <span className="grow">
                      <span className="host-item-name"><span className="ell">{h.name}</span>{cur && <span className="cur-tag">current</span>}{waiting(h.id) > 0 && <span className="badge sm">{waiting(h.id)} waiting</span>}</span>
                      <span className="sub mono ell">{addr(h)}</span>
                      <span className={'host-item-state s-' + s}>{STATE_LABEL[s]}{hostFacts(f).map((x) => <span key={x}> · {x}</span>)}{s !== 'connected' && f?.error ? <span className="err-txt"> · {f.error}</span> : null}</span>
                      <FeatureTags e={f} />
                    </span>
                  </button>
                  <div className="host-item-acts">
                    {s === 'repair' ? <button className="btn sm primary" onClick={() => { onRepair(h); setOpen(false) }}>Re-pair</button>
                      : <button className="icon-btn" title={`Reconnect ${h.name}`} onClick={() => onReconnect(h.id)}>{I.refresh}</button>}
                  </div>
                </div>
              )
            })}
          </div>
          <div className="host-pop-foot">
            <button className="btn sm" onClick={() => { onAdd(); setOpen(false) }}>{I.plus}Add host</button>
            {hosts.length > 1 && <button className="btn sm" onClick={() => onReconnect()}>{I.refresh}Reconnect all</button>}
            <span className="grow" />
            <button className="btn sm ghost" onClick={() => { onSettings(); setOpen(false) }}>{I.gear}Manage hosts</button>
          </div>
        </div>,
        document.body)}
    </div>
  )
}

/** Settings → Hosts: every paired host with its state, rename, order and removal. */
export function HostsSettings({ hosts, current, fleet, storage, onSwitch, onAdd, onForget, onRepair, onReconnect, onChanged }: {
  hosts: Host[]; current?: Host; fleet: Fleet; storage: Record<string, string>
  onSwitch: (h: Host) => void; onAdd: () => void; onForget: (h: Host, revoke: boolean) => void; onRepair: (h: Host) => void; onReconnect: (id?: string) => void; onChanged: () => void
}) {
  const [editing, setEditing] = useState<string>()
  const [name, setName] = useState('')
  const [err, setErr] = useState<string>()
  const move = async (i: number, d: number): Promise<void> => {
    const ids = hosts.map((h) => h.id)
    const j = i + d
    if (j < 0 || j >= ids.length) return
    ;[ids[i], ids[j]] = [ids[j], ids[i]]
    await window.api.hosts.reorder(ids); onChanged()
  }
  const [drag, setDrag] = useState<string>()
  const [over, setOver] = useState<string>()
  const dropOn = async (target: string): Promise<void> => {
    const from = drag
    setDrag(undefined); setOver(undefined)
    if (!from || from === target) return
    await window.api.hosts.reorder(moveTo(hosts.map((h) => h.id), from, target)); onChanged()
  }
  const rename = async (h: Host): Promise<void> => {
    const r = await window.api.hosts.rename(h.id, name)
    if (!r.ok) { setErr(r.error); return }
    setEditing(undefined); setErr(undefined); onChanged()
  }
  const agg = aggregate(hosts, fleet)
  return (
    <>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <h2 style={{ flex: 1 }}>Hosts</h2>
        {hosts.length > 1 && <button className="btn" onClick={() => onReconnect()}>{I.refresh}Reconnect all</button>}
        <button className="btn primary" onClick={onAdd}>{I.plus}Add host</button>
      </div>
      <div className="sub" style={{ marginBottom: 6 }}>Each host is paired with its own one-use code and keeps its own token. The app connects to all of them at once; drag rows to set the order used in the sidebar and switcher.</div>
      <div className={'agg-line s-' + agg.state}><span className={'dot ' + STATE_DOT[agg.state]} />{agg.label}</div>
      {err && <div className="error">{err}</div>}
      {hosts.map((h, i) => {
        const f = fleet[h.id]
        const s = f?.state ?? 'connecting'
        const cur = h.id === current?.id
        const store = storage[h.id]
        return (
          <div className={'set-row host-row' + (drag === h.id ? ' dragging' : '') + (over === h.id && drag !== h.id ? ' drop-target' : '')} key={h.id}
            draggable={hosts.length > 1 && editing !== h.id}
            onDragStart={(e) => { setDrag(h.id); e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', h.id) }}
            onDragOver={(e) => { if (drag) { e.preventDefault(); setOver(h.id) } }}
            onDragLeave={() => setOver((o) => (o === h.id ? undefined : o))}
            onDrop={(e) => { e.preventDefault(); void dropOn(h.id) }}
            onDragEnd={() => { setDrag(undefined); setOver(undefined) }}>
            {hosts.length > 1 && <span className="drag-grip" title="Drag to reorder">{I.grip}</span>}
            <span className={'dot ' + STATE_DOT[s]} />
            <div style={{ flex: 1, minWidth: 0 }}>
              {editing === h.id
                ? <div className="row-edit"><input className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') void rename(h); if (e.key === 'Escape') setEditing(undefined) }} /><button className="btn sm primary" onClick={() => void rename(h)}>Save</button><button className="btn sm ghost" onClick={() => setEditing(undefined)}>Cancel</button></div>
                : <div className="host-row-name">{h.name}{cur && <span className="cur-tag">current</span>}</div>}
              <div className="sub mono ell">{addr(h)}</div>
              <div className={'host-item-state s-' + s}>{STATE_LABEL[s]}{hostFacts(f).map((x) => <span key={x}> · {x}</span>)}{store ? <span> · token in {store === 'enc' ? 'keychain' : store === 'file' ? 'encrypted file' : 'memory'}</span> : null}{s !== 'connected' && f?.error ? <span className="err-txt"> · {f.error}</span> : null}</div>
              <FeatureTags e={f} />
            </div>
            <div className="host-row-acts">
              {!cur && <button className="btn sm" onClick={() => onSwitch(h)}>Switch</button>}
              {s === 'repair' ? <button className="btn sm primary" onClick={() => onRepair(h)}>Re-pair</button> : <button className="icon-btn" title="Reconnect" onClick={() => onReconnect(h.id)}>{I.refresh}</button>}
              <button className="icon-btn" title="Rename" onClick={() => { setEditing(h.id); setName(h.name) }}>{I.pencil}</button>
              <button className="icon-btn" title="Move up" disabled={i === 0} onClick={() => void move(i, -1)}>{I.up}</button>
              <button className="icon-btn" title="Move down" disabled={i === hosts.length - 1} onClick={() => void move(i, 1)}>{I.down}</button>
              <button className="icon-btn danger" title="Unpair: revoke this device on the host and forget it here" onClick={() => onForget(h, true)}>{I.trash}</button>
            </div>
          </div>
        )
      })}
    </>
  )
}

