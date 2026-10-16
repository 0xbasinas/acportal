import { useEffect, useRef, useState } from 'react'
import { ElicitSchema, Item, PermissionItem, Timeline } from './acp'
import { DiffView, Empty, I } from './ui'
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
        <span className={'ans-ic ' + (rejected ? 'red' : 'green')}>{rejected ? I.close : I.check}</span>
        <span className="mono tool-title">{p.lines[0] ?? p.title}</span>
        <span className="sub">{p.source === 'agent' ? 'agent' : 'host'} · {p.answered === 'cancelled' ? 'Cancelled' : nice(chosen?.name ?? p.answered ?? '')}</span>
      </div>
    )
  }
  return (
    <div className={'card approval' + (isShell ? ' danger' : '')}>
      <div className="card-h">
        <span className="approval-ic">{isShell ? I.term : I.shield}</span>
        <div className="grow">
          <h3>{isShell ? `Run ${n > 1 ? `${n} shell commands` : 'a shell command'}` : p.title}</h3>
          <div className="sub">{p.source === 'agent' ? 'Asked by the agent' : `Host consent · ${p.source}`}{p.cwd ? <> · in <span className="mono">{p.cwd}</span></> : ''}</div>
        </div>
        <span className={'tag ' + (isShell ? 'red' : 'grey')}>{isShell ? 'Shell' : p.toolKind ?? 'Permission'}</span>
      </div>
      {isShell && p.title && p.title !== p.lines[0] && <div className="sub2" style={{ marginTop: 8 }}>{p.title}</div>}
      {p.lines.length > 0 && (
        <pre className="cmd-block mono">{p.lines.map((l, i) => <div key={i}><span className="prompt">$</span>{l}</div>)}</pre>
      )}
      {p.scope && p.scope.outside.length > 0 && (
        <div className="scope-warn" role="alert">
          {I.alert}
          <div>
            <b>Outside the workspace</b>
            <div className="sub2">
              {p.scope.outside.map((x) => <span key={x} className="mono chip">{x}</span>)}
            </div>
            <div className="sub2">{p.scope.agentTool ? "The agent's own tool runs unconfined; the host can only warn." : p.scope.sandboxed ? 'Runs in the host sandbox: only the workspace is writable and your home folder is hidden.' : 'No OS sandbox on this host: the command can reach these paths.'}</div>
          </div>
        </div>
      )}
      {p.scope && p.scope.outside.length === 0 && p.scope.sandboxed && <div className="sub2 scope-ok">{I.shield} Sandboxed to the workspace</div>}
      {p.diffs.map((d) => (
        <div style={{ marginTop: 10 }} key={d.path}><DiffView d={d} maxLines={14} /></div>
      ))}
      {ar && (
        <div className="review-note">
          {I.spark}<span>{ar.decision === 'ask' ? 'Not auto-approved' : `Auto-review: ${ar.decision}`}
          {ar.layer ? ` — ${ar.layer === 'model' ? 'smart review' : 'rules'}` : ''}
          {ar.reason ? `: ${ar.reason}` : ''}</span>
        </div>
      )}
      <div className="approval-foot">
        <span className="sub grow">{isShell ? 'Runs these exact lines once on the host.' : 'The agent waits for your answer.'}</span>
        {reject ? <button className="btn ghost" onClick={() => onAnswer(reject.optionId)}>{nice(reject.name || 'Reject')}</button> : <button className="btn ghost" onClick={() => onAnswer(null)}>Cancel</button>}
        {always && !isShell && <button className="btn" onClick={() => onAnswer(always.optionId)}>{nice(always.name)}</button>}
        {allow && <button className="btn primary" autoFocus onClick={() => onAnswer(allow.optionId)}>{nice(allow.name || 'Allow once')}</button>}
      </div>
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
        <div className="agent-msg">
          <div className="agent-name">{I.bot}{agent}</div>
          <div className="agent-text"><Markdown text={item.text} /></div>
        </div>
      )
    case 'thought':
      return <div className={'thought' + (open ? ' open' : '')} onClick={() => setOpen(!open)} title={open ? 'Collapse' : 'Show all'}>{item.text.length > 280 && !open ? item.text.slice(0, 280) + '…' : item.text}</div>
    case 'plan':
      return (
        <div className="card plan">
          <div className="group-h" style={{ marginTop: 0 }}><span>Plan</span><span className="count">{item.entries.filter((e) => e.status === 'completed').length}/{item.entries.length}</span></div>
          {item.entries.map((e, i) => (
            <div key={i} className={'plan-row ' + (e.status ?? '')}>
              <span className="plan-ic">{e.status === 'completed' ? I.check : e.status === 'in_progress' ? <span className="spin-dot" /> : <span className="ring" />}</span>
              <span>{e.content}</span>
            </div>
          ))}
        </div>
      )
    case 'tool':
      return (
        <div>
          <div className={'tool' + (item.terminal ? ' clickable' : '')} onClick={() => setOpen(!open)}>
            <span className="tool-ic">{item.toolKind === 'execute' || item.terminal ? I.term : item.diffs.length || item.toolKind === 'edit' ? I.file : I.tool}</span>
            <span className="tool-title">{item.title}</span>
            {item.autoReview && <span className="tag grey">auto-review {item.autoReview.decision}</span>}
            <span className={'status ' + (item.status ?? 'pending')}>
              <span className="st-dot" />{item.status === 'completed' ? 'Done' : item.status === 'failed' ? 'Failed' : item.status === 'in_progress' ? 'Running' : 'Pending'}
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
        {t.items.length === 0 && <div style={{ minHeight: 320, display: 'flex' }}><Empty icon={I.chat} title={readOnly ? 'Nothing here yet' : 'Start the conversation'}>{readOnly ? 'This task has not produced output.' : `Ask ${agent} to change, explain or test something in this folder.`}</Empty></div>}
        {t.items.map((i) => <Row key={String(i.id)} item={i} agent={agent} onAnswer={onAnswer} onReview={onReview} onElicit={onElicit} />)}
        {t.busy && <div className="working"><span className="pulse" />{agent} is working…</div>}
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
  if (item.answered) return <div className="answered"><span className={'ans-ic ' + (item.answered === 'accept' ? 'green' : 'red')}>{item.answered === 'accept' ? I.check : I.close}</span><span className="tool-title">{item.message}</span><span className="sub">form · {item.answered === 'accept' ? 'Sent' : item.answered === 'decline' ? 'Declined' : 'Cancelled'}</span></div>
  return (
    <div className="card approval">
      <div className="card-h"><span className="approval-ic">{I.pencil}</span><div className="grow"><h3>{item.message}</h3><div className="sub">The agent asks for input · form</div></div></div>
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
                {options.map((o) => <button key={o.v} className={'seg-btn' + (vals[k] === o.v ? ' on' : '')} onClick={() => setVals((v) => ({ ...v, [k]: o.v }))}>{o.l}</button>)}
              </div>
            ) : (
              <input className="input" type={f.type === 'number' || f.type === 'integer' ? 'number' : f.format === 'email' ? 'email' : 'text'} min={f.minimum} max={f.maximum} value={String(vals[k] ?? '')}
                onChange={(e) => setVals((v) => ({ ...v, [k]: f.type === 'number' || f.type === 'integer' ? (e.target.value === '' ? undefined : Number(e.target.value)) : e.target.value }))} />
            )}
          </div>
        )
      })}
      <div className="approval-foot">
        <span className="sub grow">{missing.length ? `${missing.length} required field${missing.length > 1 ? 's' : ''} left` : 'Your answer goes to the agent through the host.'}</span>
        <button className="btn ghost" onClick={() => onElicit(item.id, { action: 'cancel' })}>Cancel</button>
        <button className="btn" onClick={() => onElicit(item.id, { action: 'decline' })}>Decline</button>
        <button className="btn primary" disabled={missing.length > 0 || item.mode === 'url'} onClick={() => onElicit(item.id, { action: 'accept', content: vals })}>Send</button>
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
            <span key={i} className="att-chip">{a.data ? <img src={`data:${a.mimeType};base64,${a.data}`} alt="" /> : I.file}<span>{a.name}</span><span className="sub">{a.size < 1024 ? `${a.size} B` : `${Math.ceil(a.size / 1024)} KB`}</span>
              <button className="x" aria-label={`Remove ${a.name}`} onClick={() => setAtts(atts.filter((_, j) => j !== i))}>{I.close}</button></span>
          ))}
        </div>
      )}
      <textarea
        ref={ref}
        rows={1}
        placeholder={disabled ?? `Message ${agent}…`}
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
        <button className="icon-btn" title={canImage || canResource ? 'Attach images or text files' : `${agent} does not accept attachments`} disabled={!canImage && !canResource} onClick={() => void pick()}>{I.attach}</button>
        <button className={'icon-btn' + (showHist ? ' on' : '')} title="Prompt history (↑ in an empty box)" onClick={() => setShowHist(!showHist)}>{I.clock}</button>
        <span style={{ flex: 1 }} />
        <span className="hint-keys"><span className="kbd">Enter</span> send · <span className="kbd">Shift Enter</span> new line</span>
        {busy ? (
          <button className="btn stop" onClick={onCancel} title="Cancel this turn (Esc)">{I.stop}Stop</button>
        ) : (
          <button className="send" disabled={(!text.trim() && !atts.length) || Boolean(disabled)} onClick={send} aria-label="Send">{I.send}</button>
        )}
      </div>
    </div>
  )
}
