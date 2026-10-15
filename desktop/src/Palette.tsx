import { useEffect, useMemo, useState } from 'react'

export type Command = { id: string; group: string; label: string; hint?: string; run: () => void }

export function Palette({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const [q, setQ] = useState('')
  const [sel, setSel] = useState(0)
  const list = useMemo(() => {
    const t = q.toLowerCase().trim()
    return commands.filter((c) => !t || (c.label + ' ' + c.group).toLowerCase().includes(t))
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
        <input
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
        />
        <div style={{ maxHeight: 420, overflowY: 'auto', paddingBottom: 8 }}>
          {list.length === 0 && <div className="group">No matches</div>}
          {list.map((c, i) => {
            const head = c.group !== lastGroup
            lastGroup = c.group
            return (
              <div key={c.id}>
                {head && <div className="group">{c.group}</div>}
                <div className={'item' + (i === sel ? ' sel' : '')} onMouseEnter={() => setSel(i)} onClick={() => run(c)}>
                  <span>{c.label}</span>
                  {c.hint && <span className="kbd">{c.hint}</span>}
                </div>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
