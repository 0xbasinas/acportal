import { useEffect, useRef, useState } from 'react'
import { ElicitSchema, Item, PermissionItem, Timeline } from './acp'
import { DiffView, I } from './ui'
import { Markdown } from './Markdown'

const nice = (n: string): string => (/^[a-z]+(_[a-z]+)+$/.test(n) ? n.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase()) : n)

export function Permission({ p, onAnswer }: { p: PermissionItem; onAnswer: (optionId: string | null) => void }) {
  const isShell = p.source.startsWith('host-shell') || p.lines.length > 0
  const allow = p.options.find((o) => o.kind === 'allow_once') ?? p.options.find((o) => o.kind?.startsWith('allow'))
  const always = p.options.find((o) => o.kind === 'allow_always')
  const reject = p.options.find((o) => o.kind === 'reject_once') ?? p.options.find((o) => o.kind?.startsWith('reject'))
  const ar = p.autoReview
  const done = p.answered !== undefined
  const n = p.lines.length
  if (done) {
    const chosen = p.options.find((o) => o.optionId === p.answered)
    const rejected = p.answered === 'cancelled' || chosen?.kind?.startsWith('reject')
    return (
      <div className="answered">
        <span className={rejected ? 'red' : 'green'}>{rejected ? '✕' : '✓'}</span>
        <span className="mono tool-title" style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.lines[0] ?? p.title}</span>
        <span className="sub">{p.source === 'agent' ? 'agent' : 'host'} · {p.answered === 'cancelled' ? 'Cancelled' : nice(chosen?.name ?? p.answered ?? '')}</span>
      </div>
    )
  }
  return (
    <div className="card" style={{ opacity: done ? 0.6 : 1 }}>
      <h3>{isShell ? `Run ${n > 1 ? `${n} shell commands` : 'a shell command'}` : p.title}</h3>
      {isShell && p.title && <div className="sub2">{p.title}</div>}
      <div style={{ display: 'flex', gap: 10, alignItems: 'center', marginTop: 10 }}>
        <span className={'tag ' + (isShell ? 'red' : 'grey')}>{isShell ? 'Shell command' : p.toolKind ?? 'Permission'}</span>
        <span className="sub">{p.source === 'agent' ? 'Asked by the agent' : `Host consent · ${p.source}`}{p.cwd ? ` · in ${p.cwd}` : ''}</span>
      </div>
      {p.lines.map((l, i) => (
        <div className="codeline" key={i}>
          <code>{l}</code>
          {n > 1 && <span className="sub">Line {i + 1}</span>}
        </div>
      ))}
      {p.diffs.map((d) => (
        <div style={{ marginTop: 10 }} key={d.path}><DiffView d={d} maxLines={14} /></div>
      ))}
      {ar && (
        <div className="sub2" style={{ marginTop: 10 }}>
          {ar.decision === 'ask' ? 'Not auto-approved' : `Auto-review: ${ar.decision}`}
          {ar.layer ? ` — ${ar.layer === 'model' ? 'smart review' : 'rules'}` : ''}
          {ar.reason ? `: ${ar.reason}` : ''}
        </div>
      )}
      {done ? (
        <div className="sub2" style={{ marginTop: 12 }}>{p.answered === 'cancelled' ? 'Cancelled' : `Answered: ${p.options.find((o) => o.optionId === p.answered)?.name ?? p.answered}`}</div>
      ) : (
        <>
          <div style={{ color: 'var(--text)', marginTop: 10, fontSize: 12 }}>{isShell ? 'Approve only if you want these exact lines to run once on your host.' : 'The agent waits for your answer.'}</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6, marginTop: 14 }}>
            {allow && <button className="btn-wide" onClick={() => onAnswer(allow.optionId)}>{nice(allow.name || 'Allow once')}</button>}
            {always && !isShell && <button className="btn-wide ghost" onClick={() => onAnswer(always.optionId)}>{nice(always.name)}</button>}
            {reject ? <button className="btn-wide ghost" onClick={() => onAnswer(reject.optionId)}>{nice(reject.name || 'Reject')}</button> : <button className="btn-wide ghost" onClick={() => onAnswer(null)}>Cancel</button>}
          </div>
        </>
      )}
    </div>
  )
}

function Row({ item, agent, onAnswer, onReview, onElicit }: { item: Item; agent: string; onAnswer: (id: string | number, o: string | null) => void; onReview: () => void; onElicit: (id: string | number, result: unknown) => void }) {
  const [open, setOpen] = useState(false)
  switch (item.kind) {
    case 'user':
      return <div className="bubble-user">{item.text}</div>
    case 'agent':
      return (
        <div>
          <div className="agent-name">{agent}</div>
          <div className="agent-text"><Markdown text={item.text} /></div>
        </div>
      )
    case 'thought':
      return <div className="thought">{item.text.length > 600 && !open ? item.text.slice(0, 600) + '…' : item.text}</div>
    case 'plan':
      return (
        <div className="card" style={{ padding: '12px 16px' }}>
          <div className="sub2" style={{ marginBottom: 6 }}>Plan</div>
          {item.entries.map((e, i) => (
            <div key={i} style={{ display: 'flex', gap: 8 }}>
              <span className={e.status === 'completed' ? 'green' : 'sub'}>{e.status === 'completed' ? '✓' : e.status === 'in_progress' ? '•' : '○'}</span>
              <span>{e.content}</span>
            </div>
          ))}
        </div>
      )
    case 'tool':
      return (
        <div>
          <div className="tool" onClick={() => setOpen(!open)} style={{ cursor: item.terminal ? 'pointer' : undefined }}>
            <span className="sub">{I.tool}</span>
            <span className="mono tool-title">{item.title}</span>
            <span className={'status' + (item.status === 'failed' ? ' red' : '')}>
              {item.autoReview ? `auto-review ${item.autoReview.decision} · ` : ''}
              {item.status === 'completed' ? 'Completed' : item.status === 'failed' ? 'Failed' : item.status === 'in_progress' ? 'Running…' : 'Pending'}
            </span>
          </div>
          {item.diffs.map((d) => (
            <div style={{ marginTop: 10 }} key={d.path}><DiffView d={d} maxLines={8} onMore={onReview} /></div>
          ))}
          {item.terminal && open && <pre className="term mono">{item.terminal}</pre>}
        </div>
      )
    case 'permission':
      return <Permission p={item} onAnswer={(o) => onAnswer(item.id, o)} />
    case 'notice':
      return <div className={'sub2 ' + (item.tone === 'error' ? 'red' : item.tone === 'warn' ? 'amber' : '')}>{item.text}</div>
    case 'elicitation':
      return <ElicitForm item={item} onElicit={onElicit} />
  }
}

export function Conversation({ t, agent, onAnswer, onReview, readOnly, onElicit }: { t: Timeline; agent: string; onAnswer: (id: string | number, o: string | null) => void; onReview: () => void; readOnly?: boolean; onElicit: (id: string | number, result: unknown) => void }) {
  const ref = useRef<HTMLDivElement>(null)
  const stick = useRef(true)
  useEffect(() => {
    const el = ref.current
    if (el && stick.current) el.scrollTop = el.scrollHeight
  }, [t])
  return (
    <div className="scroll" ref={ref} onScroll={(e) => { const el = e.currentTarget; stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80 }}>
      <div className="convo">
        {t.gap && <div className="sub amber">Older history was trimmed by the host; this view starts later.</div>}
        {t.items.length === 0 && <div className="empty" style={{ minHeight: 300 }}><div><h2>{readOnly ? 'Nothing here yet' : 'Start the conversation'}</h2><div>{readOnly ? 'This task has not produced output.' : `Ask ${agent} to change, explain or test something in this folder.`}</div></div></div>}
        {t.items.map((i) => <Row key={String(i.id)} item={i} agent={agent} onAnswer={onAnswer} onReview={onReview} onElicit={onElicit} />)}
        {t.busy && <div className="sub">{agent} is working…</div>}
      </div>
    </div>
  )
}

type Elicit = Extract<Item, { kind: 'elicitation' }>
function ElicitForm({ item, onElicit }: { item: Elicit; onElicit: (id: string | number, r: unknown) => void }) {
  const props = (item.schema as ElicitSchema | undefined)?.properties ?? {}
  const required = new Set(item.schema?.required ?? [])
  const [vals, setVals] = useState<Record<string, unknown>>(() => Object.fromEntries(Object.entries(props).filter(([, f]) => f.default !== undefined).map(([k, f]) => [k, f.default])))
  const missing = [...required].filter((k) => vals[k] === undefined || vals[k] === '')
  if (item.answered) return <div className="answered"><span className={item.answered === 'accept' ? 'green' : 'red'}>{item.answered === 'accept' ? '✓' : '✕'}</span><span className="tool-title">{item.message}</span><span className="sub">form · {item.answered === 'accept' ? 'Sent' : item.answered === 'decline' ? 'Declined' : 'Cancelled'}</span></div>
  return (
    <div className="card">
      <h3>The agent asks for input</h3>
      <div className="sub2">{item.message}</div>
      {item.mode === 'url' && <div className="sub" style={{ marginTop: 8 }}>This request wants a web page opened ({item.url}). The app never opens agent links on its own.</div>}
      {Object.entries(props).map(([k, f]) => {
        const label = (f.title ?? k) + (required.has(k) ? ' *' : '')
        const options = f.oneOf?.map((o) => ({ v: o.const, l: o.title ?? o.const })) ?? f.enum?.map((v, i) => ({ v, l: f.enumNames?.[i] ?? v }))
        return (
          <div className="field" key={k}>
            <label>{label}{f.description ? <span className="sub"> · {f.description}</span> : null}</label>
            {f.type === 'boolean' ? (
              <span className={'toggle' + (vals[k] ? ' on' : '')} style={{ cursor: 'pointer' }} onClick={() => setVals((v) => ({ ...v, [k]: !v[k] }))} />
            ) : options ? (
              <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                {options.map((o) => <button key={o.v} className={'pill' + (vals[k] === o.v ? ' primary' : '')} onClick={() => setVals((v) => ({ ...v, [k]: o.v }))}>{o.l}</button>)}
              </div>
            ) : (
              <input className="input" type={f.type === 'number' || f.type === 'integer' ? 'number' : f.format === 'email' ? 'email' : 'text'} min={f.minimum} max={f.maximum} value={String(vals[k] ?? '')}
                onChange={(e) => setVals((v) => ({ ...v, [k]: f.type === 'number' || f.type === 'integer' ? (e.target.value === '' ? undefined : Number(e.target.value)) : e.target.value }))} />
            )}
          </div>
        )
      })}
      <div style={{ display: 'flex', gap: 8, marginTop: 16 }}>
        <button className="btn-wide" disabled={missing.length > 0 || item.mode === 'url'} onClick={() => onElicit(item.id, { action: 'accept', content: vals })}>Send</button>
        <button className="btn-wide ghost" style={{ width: 120 }} onClick={() => onElicit(item.id, { action: 'decline' })}>Decline</button>
        <button className="btn-wide ghost" style={{ width: 120 }} onClick={() => onElicit(item.id, { action: 'cancel' })}>Cancel</button>
      </div>
    </div>
  )
}

export type Attachment = { name: string; mimeType: string; size: number; data?: string; text?: string; path: string }
export function Composer({ agent, busy, disabled, onSend, onCancel, history, canImage, canResource, draft }: {
  agent: string; busy: boolean; disabled?: string; onSend: (text: string, atts: Attachment[]) => void; onCancel: () => void
  history: string[]; canImage: boolean; canResource: boolean; draft?: { text: string; n: number }
}) {
  const [text, setText] = useState('')
  const [atts, setAtts] = useState<Attachment[]>([])
  const [err, setErr] = useState<string>()
  const [hIdx, setHIdx] = useState(-1)
  const [showHist, setShowHist] = useState(false)
  const ref = useRef<HTMLTextAreaElement>(null)
  useEffect(() => { if (draft) { setText(draft.text); ref.current?.focus() } }, [draft?.n])
  useEffect(() => {
    const el = ref.current
    if (el) { el.style.height = 'auto'; el.style.height = Math.min(200, el.scrollHeight) + 'px' }
  }, [text])
  const send = (): void => {
    if ((!text.trim() && !atts.length) || busy || disabled) return
    onSend(text, atts)
    setText(''); setAtts([]); setHIdx(-1)
  }
  const pick = async (): Promise<void> => {
    setErr(undefined)
    const r = await window.api.files.pick()
    if (!r.ok) return setErr(r.error)
    const ok = r.value.filter((a) => (a.data ? canImage : canResource))
    if (ok.length < r.value.length) setErr(`${agent} does not accept ${canImage ? 'file contents' : 'images'} in prompts; those were left out`)
    const total = [...atts, ...ok].reduce((s, a) => s + a.size, 0)
    if (total > 600 * 1024) return setErr('Attachments must stay under 600 KB in total (host frame limit)')
    setAtts([...atts, ...ok])
  }
  return (
    <div className="composer" style={{ position: 'relative' }}>
      {showHist && (
        <div className="hist-pop">
          <div className="sub" style={{ padding: '6px 12px' }}>Recent prompts in this folder</div>
          {history.length === 0 && <div className="sub" style={{ padding: '6px 12px' }}>Nothing yet</div>}
          {history.slice(0, 12).map((h, i) => <div key={i} className="item" onClick={() => { setText(h); setShowHist(false); ref.current?.focus() }}>{h.split('\n')[0].slice(0, 90)}</div>)}
        </div>
      )}
      {atts.length > 0 && (
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 8 }}>
          {atts.map((a, i) => (
            <span key={i} className="chip">{a.data ? '🖼' : '📄'} {a.name} <span className="sub">{Math.ceil(a.size / 1024)} KB</span>
              <button className="sub" onClick={() => setAtts(atts.filter((_, j) => j !== i))}>{I.close}</button></span>
          ))}
        </div>
      )}
      <textarea
        ref={ref}
        rows={1}
        placeholder={disabled ?? 'Message, @files  ·  ↑ for history'}
        disabled={Boolean(disabled)}
        value={text}
        onChange={(e) => { setText(e.target.value); setHIdx(-1) }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send() }
          if (e.key === 'ArrowUp' && (text === '' || hIdx >= 0) && history.length) { e.preventDefault(); const n = Math.min(history.length - 1, hIdx + 1); setHIdx(n); setText(history[n]) }
          if (e.key === 'ArrowDown' && hIdx >= 0) { e.preventDefault(); const n = hIdx - 1; setHIdx(n); setText(n < 0 ? '' : history[n]) }
        }}
      />
      {err && <div className="error" style={{ marginTop: 0, marginBottom: 6 }}>{err}</div>}
      <div className="bar">
        <button className="icon-btn" title={canImage || canResource ? 'Attach images or text files' : `${agent} does not accept attachments`} disabled={!canImage && !canResource} onClick={() => void pick()}>{I.plus}</button>
        <button className="icon-btn" title="Prompt history" onClick={() => setShowHist(!showHist)}>{I.clock}</button>
        <span className="sub2">{agent}</span>
        <span style={{ flex: 1 }} />
        {busy ? (
          <button className="pill" onClick={onCancel} title="Cancel this turn (Esc)">Stop</button>
        ) : (
          <button className="send" disabled={(!text.trim() && !atts.length) || Boolean(disabled)} onClick={send} aria-label="Send">{I.send}</button>
        )}
      </div>
    </div>
  )
}
