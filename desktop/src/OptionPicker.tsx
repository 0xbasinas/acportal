// Session options (model, provider, mode, effort, ...) as the agent reports them, shown like the
// phone's Session options screen (SessionDetailScreens.kt): one row per option with its current
// value; a row opens only that option's list (grouped, with a search box for long lists), and
// picking a value returns to the rows. Opening or searching sends nothing; only a pick calls `onPick`.
import { useEffect, useRef, useState } from 'react'
import { I } from './ui'
import { type Choice, type Selector, SEARCH_ABOVE, choiceName, filterChoices, sections } from './agentSetup'

/** Rows drawn at most per list. */
const ROW_LIMIT = 200

/** The values of one selector. `onBack` adds a header that returns to the option rows. */
export function ChoiceList({ selector, value, onPick, heading, onBack }: { selector: Selector; value?: string; onPick: (value: string) => void; heading?: boolean; onBack?: () => void }) {
  const [query, setQuery] = useState('')
  const searchable = selector.choices.length > SEARCH_ABOVE
  const shown = filterChoices(selector.choices, query)
  // Very long lists (a router can offer thousands of models) draw the first rows, plus the current
  // value, and ask for a search to narrow them, like the phone's lazy list draws visible rows only.
  const drawn = shown.length > ROW_LIMIT ? shown.slice(0, ROW_LIMIT).concat(shown.slice(ROW_LIMIT).filter((c) => c.value === value)) : shown
  const rows: (Choice | string)[] = []
  let group: string | undefined
  for (const c of drawn) {
    if (c.group && c.group !== group) rows.push(c.group)
    group = c.group
    rows.push(c)
  }
  return (
    <div className="opt-list" role="group" aria-label={selector.name}>
      {onBack
        ? <button data-item className="ctx-item opt-back" aria-label={`Back to options (${selector.name})`} onClick={onBack}><span className="opt-back-icon">{I.back}</span><span className="grow ell">{selector.name}</span></button>
        : heading && <div className="ctx-h">{selector.name}</div>}
      {selector.description && <div className="hint ctx-pad">{selector.description}</div>}
      {searchable && <>
        <input className="input ctx-input" aria-label={`Search ${selector.name}`} placeholder={`Search ${selector.choices.length} options`} value={query} onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter' && shown.length) { e.preventDefault(); onPick(shown[0].value) } }} />
        {query && <div className="hint ctx-pad" aria-live="polite">{`${shown.length} of ${selector.choices.length}`}</div>}
      </>}
      <div className={searchable ? 'opt-scroll' : undefined}>
        {rows.map((r, i) => typeof r === 'string'
          ? <div key={'g' + i} className="ctx-h opt-group">{r}</div>
          : <button key={'v' + i} data-item className={'ctx-item' + (r.value === value ? ' on' : '')} role="menuitemradio" aria-checked={r.value === value} title={r.description ?? r.value} onClick={() => onPick(r.value)}>
              <span className="grow ell">{r.name}{r.description && <span className="sub opt-desc">{r.description}</span>}</span>
              {r.value === value && <span className="tick">{I.check}</span>}
            </button>)}
        {searchable && query && !shown.length && <div className="hint ctx-pad">No matching options. Try another search.</div>}
        {shown.length > ROW_LIMIT && <div className="hint ctx-pad">Showing {ROW_LIMIT} of {shown.length}. Search to find the rest.</div>}
      </div>
    </div>
  )
}

/** One row per option with its current value, under Model / Mode / More options headings when
 * there is more than one section; a row opens that option's list. With a single option its list
 * is shown straight away. */
export function OptionsPanel({ selectors, valueOf, onPick }: { selectors: Selector[]; valueOf: (s: Selector) => string | undefined; onPick: (s: Selector, value: string) => void }) {
  const [which, setWhich] = useState<string>()
  const box = useRef<HTMLDivElement>(null)
  const open = selectors.length === 1 ? selectors[0] : selectors.find((s) => s.kind + ':' + s.id === which)
  useEffect(() => {
    // Focus the search box or the current value when a list opens, the row when it closes.
    const el = box.current
    if (!el) return
    const target = open ? el.querySelector<HTMLElement>('input, [data-item].on, [data-item]:not(.opt-back)') : el.querySelector<HTMLElement>(`[data-row="${which ?? ''}"]`) ?? el.querySelector<HTMLElement>('[data-item]')
    target?.focus()
  }, [which])
  if (open) {
    const back = selectors.length > 1 ? () => setWhich(undefined) : undefined
    return (
      <div ref={box} onKeyDown={(e) => { if (e.key === 'Escape' && back) { e.preventDefault(); e.stopPropagation(); back() } }}>
        <ChoiceList key={open.kind + ':' + open.id} selector={open} value={valueOf(open)} heading onBack={back}
          onPick={(v) => { if (v !== valueOf(open)) onPick(open, v); back?.() }} />
      </div>
    )
  }
  return (
    <div ref={box} className="opt-list" role="group" aria-label="Session options">
      {sections(selectors).map((section, i, all) => (
        <div key={section.title} role="group" aria-label={section.title} className={i ? 'opt-section' : undefined}>
          {all.length > 1 && <div className="ctx-h">{section.title}</div>}
          {section.selectors.map((s) => {
            const key = s.kind + ':' + s.id
            const value = choiceName(s, valueOf(s)) ?? "agent's choice"
            return (
              <button key={key} data-item data-row={key} className="ctx-item opt-row" aria-label={`${s.name}: ${value}`} onClick={() => setWhich(key)}>
                <span className="opt-name">{s.name}</span><span className="grow ell opt-value">{value}</span><span className="opt-chev">{I.chev}</span>
              </button>
            )
          })}
        </div>
      ))}
    </div>
  )
}

/** The session header's options button: shows the model (or first option) and opens the panel. */
export function OptionsButton({ selectors, valueOf, onPick, disabled }: { selectors: Selector[]; valueOf: (s: Selector) => string | undefined; onPick: (s: Selector, value: string) => void; disabled?: boolean }) {
  const [open, setOpen] = useState(false)
  const btn = useRef<HTMLButtonElement>(null)
  const pop = useRef<HTMLDivElement>(null)
  useEffect(() => { if (disabled) setOpen(false) }, [disabled])
  useEffect(() => {
    if (!open) return
    const down = (e: MouseEvent): void => { if (!pop.current?.contains(e.target as Node) && !btn.current?.contains(e.target as Node)) setOpen(false) }
    document.addEventListener('mousedown', down)
    return () => document.removeEventListener('mousedown', down)
  }, [open])
  const keys = (e: React.KeyboardEvent): void => {
    if (e.key === 'Escape') { e.preventDefault(); setOpen(false); btn.current?.focus(); return }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
    const items = [...(pop.current?.querySelectorAll<HTMLElement>('input, [data-item]') ?? [])]
    const i = items.indexOf(document.activeElement as HTMLElement)
    const next = items[(i + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length]
    if (next) { e.preventDefault(); next.focus() }
  }
  if (!selectors.length) return null
  const first = selectors[0]
  const label = choiceName(first, valueOf(first)) ?? `${first.name}: agent's choice`
  return (
    <span className="ctx-wrap">
      <button ref={btn} className="select-btn" title="Session options" aria-label={`Session options: ${label}`} aria-haspopup="dialog" aria-expanded={open}
        disabled={disabled} onClick={() => setOpen(!open)}>
        <span className="ell">{label}</span>{I.down}
      </button>
      {open && <div ref={pop} className="ctx-pop down right wide" role="dialog" aria-label="Session options" onKeyDown={keys}>
        <OptionsPanel selectors={selectors} valueOf={valueOf} onPick={onPick} />
      </div>}
    </span>
  )
}
