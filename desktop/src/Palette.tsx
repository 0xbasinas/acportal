import type React from 'react'
import { useEffect, useMemo, useState } from 'react'
import { I } from './ui'

const ID_ICON: Record<string, React.ReactNode> = { new: I.plus, git: I.branch, review: I.compare, stop: I.stop, settings: I.gear, pair: I.server, theme: I.spark, 'theme-sys': I.spark, quick: I.plus }
const GROUP_ICON: Record<string, React.ReactNode> = { Actions: I.spark, Appearance: I.spark, Sessions: I.chat, 'New session': I.plus, Tasks: I.flow, Hosts: I.server }

export type Command = { id: string; group: string; label: string; hint?: string; run: () => void }

export function Palette({ commands, onClose, onNew }: { commands: Command[]; onClose: () => void; onNew?: (q: string) => void }) {
  const [q, setQ] = useState('')
  const [sel, setSel] = useState(0)
  const list = useMemo(() => {
    const t = q.toLowerCase().trim()
    const hits = commands.filter((c) => !t || (c.label + ' ' + c.group).toLowerCase().includes(t))
    return q.trim().length > 3 && onNew ? [{ id: 'quick', group: 'New session', label: `New session: ${q.trim()}`, hint: 'Enter', run: () => onNew(q.trim()) }, ...hits] : hits
  }, [q, commands])
  useEffect(() => setSel(0), [q])
  const run = (c?: Command): void => {
    if (!c) return
    onClose()
    c.run()
  }
  let lastGroup = ''
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="palette" onMouseDown={(e) => e.stopPropagation()}>
        <div className="palette-search">{I.search}<input
          autoFocus
          placeholder="Type a command or search sessions…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') onClose()
            if (e.key === 'ArrowDown') { e.preventDefault(); setSel((s) => Math.min(list.length - 1, s + 1)) }
            if (e.key === 'ArrowUp') { e.preventDefault(); setSel((s) => Math.max(0, s - 1)) }
            if (e.key === 'Enter') run(list[sel])
          }}
        /><span className="kbd">Esc</span></div>
        <div className="palette-list" style={{ maxHeight: 420, overflowY: 'auto', paddingBottom: 8 }}>
          {list.length === 0 && <div className="group">No matches</div>}
          {list.map((c, i) => {
            const head = c.group !== lastGroup
            lastGroup = c.group
            return (
              <div key={c.id}>
                {head && <div className="group">{c.group}</div>}
                <div className={'item' + (i === sel ? ' sel' : '')} onMouseEnter={() => setSel(i)} onClick={() => run(c)}>
                  <span className="pal-ic">{ID_ICON[c.id] ?? GROUP_ICON[c.group] ?? I.chev}</span><span className="grow">{c.label}</span>
                  {c.hint && <span className="kbd">{c.hint}</span>}
                </div>
              </div>
            )
          })}
        </div>
        <div className="palette-foot"><span><span className="kbd">↑</span><span className="kbd">↓</span> move</span><span><span className="kbd">Enter</span> run</span><span className="grow" /><span>{list.length} result{list.length === 1 ? '' : 's'}</span></div>
      </div>
    </div>
  )
}
