import { ReactNode, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { ElicitSchema, Item, PermissionItem, Timeline } from './acp'
import { DiffView, Empty, I, PathRoot } from './ui'
import { Markdown } from './Markdown'
import { duration, turnTimes } from './turns'

const nice = (n: string): string => (/^[a-z]+(_[a-z]+)+$/.test(n) ? n.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase()) : n)

export function Permission({ p, onAnswer, quiet, busy }: { p: PermissionItem; onAnswer: (optionId: string | null) => void; /** In the transcript, while the dock above the composer has the buttons. */ quiet?: boolean; /** A click is in flight. Further clicks wait. */ busy?: boolean }) {
  const isShell = p.source.startsWith('host-shell') || p.lines.length > 0
  const allow = p.options.find((o) => o.kind === 'allow_once') ?? p.options.find((o) => o.kind?.startsWith('allow'))
  const always = p.options.find((o) => o.kind === 'allow_always')
  const reject = p.options.find((o) => o.kind === 'reject_once') ?? p.options.find((o) => o.kind?.startsWith('reject'))
  const ar = p.autoReview
  const done = p.answered !== undefined
  const n = p.lines.length
  const answerLabel = p.answered === 'cancelled' ? 'Cancelled' : p.answered === 'ended' ? 'Ended' : p.answered === 'settled' ? 'Answered' : nice(p.options.find((o) => o.optionId === p.answered)?.name ?? p.answered ?? '')
  if (done) {
    const chosen = p.options.find((o) => o.optionId === p.answered)
    const rejected = p.answered === 'cancelled' || p.answered === 'ended' || chosen?.kind?.startsWith('reject')
    return (
      <div className="answered">
        <span className={'ans-ic ' + (rejected ? 'red' : 'green')}>{rejected ? I.close : I.check}</span>
        <span className={(p.lines.length ? 'mono ' : '') + 'tool-title'}>{p.lines[0] ?? p.title}</span>
        <span className="sub">{p.source === 'agent' ? 'agent' : 'host'} · {answerLabel}</span>
      </div>
    )
  }
  if (quiet) {
    return (
      <div className="answered">
        <span className="ans-ic">{isShell ? I.term : I.shield}</span>
        <span className={(p.lines.length ? 'mono ' : '') + 'tool-title'}>{p.lines[0] ?? p.title}</span>
        <span className="sub">Waiting above the message box</span>
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
        <span className="sub grow">{isShell ? (p.source === 'agent' ? 'The agent runs this with its own tool once you allow it; the host does not sandbox it.' : `Runs these exact lines once on the host${p.scope?.shell ? ` in ${shellName(p.scope.shell)}` : ''}${p.scope?.sandboxed ? ', in its sandbox' : ''}.${p.scope?.shellNote ? ' ' + p.scope.shellNote : ''}`) : 'The agent waits for your answer.'}</span>
        {reject ? <button className="btn ghost" disabled={busy} onClick={() => onAnswer(reject.optionId)}>{nice(reject.name || 'Reject')}</button> : <button className="btn ghost" disabled={busy} onClick={() => onAnswer(null)}>Cancel</button>}
        {always && !isShell && <button className="btn" disabled={busy} onClick={() => onAnswer(always.optionId)}>{nice(always.name)}</button>}
        {allow && <button className="btn primary" disabled={busy} onClick={() => onAnswer(allow.optionId)}>{busy ? 'Sending…' : nice(allow.name || 'Allow once')}</button>}
      </div>
    </div>
  )
}

function Row({ item, agent, onAnswer, onReview, onElicit, docked, answering }: { item: Item; agent: string; onAnswer: (id: string | number, o: string | null) => void; onReview: () => void; onElicit: (id: string | number, result: unknown) => void; docked?: boolean; answering?: (id: string | number) => boolean }) {
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
      return <Permission p={item} quiet={docked && item.answered === undefined} onAnswer={(o) => onAnswer(item.id, o)} />
    case 'notice':
      return <div className={'sub2 ' + (item.tone === 'error' ? 'red' : item.tone === 'warn' ? 'amber' : '')}>{item.text}</div>
    case 'elicitation':
      return <ElicitForm item={item} busy={answering?.(item.id)} onElicit={onElicit} />
  }
}

export function Conversation({ t, agent, onAnswer, onReview, readOnly, onElicit, root, turnKey, docked, answering }: { root?: string; turnKey?: string; t: Timeline; agent: string; onAnswer: (id: string | number, o: string | null) => void; onReview: () => void; readOnly?: boolean; onElicit: (id: string | number, result: unknown) => void; /** Unanswered permissions are answered in a dock, so the transcript only points at them. */ docked?: boolean; /** A click is in flight for this request id. */ answering?: (id: string | number) => boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const stick = useRef(true)
  // Turns whose steps the reader opened again; every finished turn starts folded.
  const [shown, setShown] = useState<Set<string>>(() => new Set())
  const times = turnKey ? turnTimes(turnKey) : {}
  const blocks = fold(t.items, t.busy)
  useEffect(() => {
    const el = ref.current
    if (el && stick.current) el.scrollTop = el.scrollHeight
  }, [t])
  return (
    <PathRoot.Provider value={root}>
    <div className="scroll" ref={ref} onScroll={(e) => { const el = e.currentTarget; stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80 }}>
      <div className="convo">
        {t.gap && <div className="sub amber">Older history was trimmed by the host; this view starts later.</div>}
        {t.items.length === 0 && t.loading && !t.preview?.length && <div className="sub" style={{ padding: 24, textAlign: 'center' }}>Loading the conversation…</div>}
        {t.items.length === 0 && t.loading && Boolean(t.preview?.length) && <div className="preview"><div className="sub" style={{ paddingBottom: 6 }}>Showing the saved copy while the host catches up…</div>{fold(t.preview!, false).map((b) => b.kind === 'row' ? <Row key={String(b.item.id)} item={b.item} agent={agent} onAnswer={noopAnswer} onReview={onReview} onElicit={noopAnswer} /> : null)}</div>}
        {t.items.length === 0 && !t.loading && <div style={{ minHeight: 320, display: 'flex' }}><Empty icon={I.chat} title={readOnly ? 'Nothing here yet' : 'Start the conversation'}>{readOnly ? 'This task has not produced output.' : `Ask ${agent} to change, explain or test something in this folder.`}</Empty></div>}
        {blocks.map((b) => b.kind === 'row'
          ? <Row key={String(b.item.id)} item={b.item} agent={agent} onAnswer={onAnswer} onReview={onReview} onElicit={onElicit} docked={docked} answering={answering} />
          : <Steps key={'steps' + b.id} block={b} took={times[b.id]?.took} open={shown.has(b.id)} onToggle={() => setShown((s) => { const n = new Set(s); if (!n.delete(b.id)) n.add(b.id); return n })}>
              {b.items.map((i) => <Row key={String(i.id)} item={i} agent={agent} onAnswer={onAnswer} onReview={onReview} onElicit={onElicit} docked={docked} answering={answering} />)}
            </Steps>)}
        {t.busy && <Working agent={agent} since={times[[...t.items].reverse().find((i) => i.kind === 'user')?.id ?? '']?.at} />}
      </div>
    </div>
    </PathRoot.Provider>
  )
}

type Block = { kind: 'row'; item: Item } | { kind: 'steps'; id: string; items: Item[]; tools: number }

/** After a turn has its final reply, the steps before that reply (thoughts, tool calls, earlier
 *  remarks) fold into one line. Anything still waiting on the user, and the running turn, stay open. */
export function fold(items: Item[], busy: boolean): Block[] {
  const out: Block[] = []
  let i = 0
  while (i < items.length) {
    if (items[i].kind !== 'user') { out.push({ kind: 'row', item: items[i] }); i++; continue }
    const user = items[i]
    let end = i + 1
    while (end < items.length && items[end].kind !== 'user') end++
    const turn = items.slice(i + 1, end)
    const running = busy && end === items.length
    const finalAt = turn.map((x) => x.kind).lastIndexOf('agent')
    const before = finalAt > 0 ? turn.slice(0, finalAt) : []
    const waiting = before.some((x) => (x.kind === 'permission' || x.kind === 'elicitation') && !x.answered)
    out.push({ kind: 'row', item: user })
    if (running || finalAt < 0 || waiting || before.length === 0) { for (const x of turn) out.push({ kind: 'row', item: x }) }
    else {
      out.push({ kind: 'steps', id: String(user.id), items: before, tools: before.filter((x) => x.kind === 'tool' || x.kind === 'permission').length })
      for (const x of turn.slice(finalAt)) out.push({ kind: 'row', item: x })
    }
    i = end
  }
  return out
}

function Steps({ block, took, open, onToggle, children }: { block: Extract<Block, { kind: 'steps' }>; took?: number; open: boolean; onToggle: () => void; children: ReactNode }) {
  const n = block.tools
  const what = n ? `${n} step${n === 1 ? '' : 's'}` : 'thinking'
  return (
    <div className="steps">
      <button className="steps-head" aria-expanded={open} onClick={onToggle}>
        <span className={'chev-ic' + (open ? '' : ' shut')}>{I.down}</span>
        <span>{took !== undefined ? `Worked for ${duration(took)}` : 'Worked'} · {what}</span>
      </button>
      {open && <div className="steps-body">{children}</div>}
    </div>
  )
}

function Working({ agent, since }: { agent: string; since?: number }) {
  const [, tick] = useState(0)
  useEffect(() => { if (since === undefined) return; const id = setInterval(() => tick((n) => n + 1), 1000); return () => clearInterval(id) }, [since])
  return <div className="working"><span className="pulse" />{agent} is working…{since !== undefined && <span className="sub"> {duration(Date.now() - since)}</span>}</div>
}

type Elicit = Extract<Item, { kind: 'elicitation' }>
export function Elicitation({ item, onElicit, busy }: { item: Elicit; onElicit: (id: string | number, r: unknown) => void; busy?: boolean }) {
  return <ElicitForm item={item} onElicit={onElicit} busy={busy} />
}
function ElicitForm({ item, onElicit, busy }: { item: Elicit; onElicit: (id: string | number, r: unknown) => void; busy?: boolean }) {
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
        <button className="btn ghost" disabled={busy} onClick={() => onElicit(item.id, { action: 'cancel' })}>Cancel</button>
        <button className="btn" disabled={busy} onClick={() => onElicit(item.id, { action: 'decline' })}>Decline</button>
        <button className="btn primary" disabled={busy || missing.length > 0 || item.mode === 'url'} onClick={() => onElicit(item.id, { action: 'accept', content: vals })}>{busy ? 'Sending…' : 'Send'}</button>
      </div>
    </div>
  )
}

/** `uri` names where a text attachment came from when that is not a file (a page in the browser).
 *  `data` is an image. `blob` is a PDF or other file. `text` is a text file. */
export type Attachment = { name: string; mimeType: string; size: number; data?: string; text?: string; blob?: string; path: string; uri?: string }

/** One attachment as an ACP content block. An image stays an image. A PDF is a resource with its bytes. */
export function promptBlock(a: Attachment): Record<string, unknown> {
  if (a.data) return { type: 'image', mimeType: a.mimeType, data: a.data }
  const resource: Record<string, unknown> = { uri: a.uri ?? 'file://' + a.path, mimeType: a.mimeType }
  if (a.blob) resource.blob = a.blob
  else resource.text = a.text ?? ''
  return { type: 'resource', resource }
}
/** Something put into the message box from outside it: attachments to add, or text to append. `n` makes each one new. */
export type Incoming = { n: number; attachments?: Attachment[]; append?: string }
/** A file the host found under the session folder. `relative` uses forward slashes. */
export type FileHit = { name: string; path: string; relative: string }
/** `note` is why a file was not attached. An attachment is ready to sit on the message. */
export type AttachResult = { attachment?: Attachment; note?: string }
const ATTACH_LIMIT = 600 * 1024

/** The @ file token touching the cursor. `name@host` is left alone. A quote after @ keeps spaces. */
export function mentionAt(text: string, cursor: number): { start: number; query: string } | undefined {
  const upto = text.slice(0, Math.max(0, Math.min(cursor, text.length)))
  const quoted = /(?:^|\s)@"([^"]*)$/.exec(upto)
  if (quoted) {
    const query = quoted[1]
    return { start: upto.length - query.length - 2, query }
  }
  const plain = /(?:^|\s)@([^\s@]*)$/.exec(upto)
  if (!plain) return
  const query = plain[1]
  return { start: upto.length - query.length - 1, query }
}

/** Drop the @ token the person just accepted. The file stays on the chip, not in the text. */
export function dropMention(text: string, start: number, cursor: number): { text: string; caret: number } {
  const before = text.slice(0, start)
  const after = text.slice(Math.max(start, cursor))
  const next = (before + after).replace(/[ \t]{2,}/g, ' ')
  if (!next.trim()) return { text: '', caret: 0 }
  return { text: next, caret: Math.min(before.length, next.length) }
}

export function Composer({ agent, busy, disabled, onSend, onCancel, history, canImage, canResource, draft, remote, onText, incoming, findFiles, onAttachFile, staged, onStagedClear }: {
  incoming?: Incoming
  agent: string; busy: boolean; disabled?: string; onSend: (text: string, atts: Attachment[]) => void; onCancel: () => void
  history: string[]; canImage: boolean; canResource: boolean; draft?: { text: string; n: number }
  /** The draft saved on the host (from this or another device) and a callback to save typing there. */
  remote?: { text: string; at: number }; onText?: (text: string) => void
  /** Files under the session folder, for the @ menu. Absent: typing @ is ordinary text. */
  findFiles?: (query: string) => Promise<FileHit[] | { error: string }>
  /** Read one workspace file and turn it into an attachment, or explain why not. */
  onAttachFile?: (path: string) => Promise<AttachResult>
  /** Files added from outside the box (a right-click) while this box may have been unmounted. */
  staged?: { stamp: number; file: Attachment }[]
  onStagedClear?: () => void
}) {
  const [text, setText] = useState(remote?.text ?? '')
  // Text known to match the host copy; a newer host draft replaces the box only while it is unedited.
  const synced = useRef(remote?.text ?? '')
  useEffect(() => {
    if (remote && text === synced.current && remote.text !== text) { setText(remote.text); synced.current = remote.text }
  }, [remote?.at])
  useEffect(() => {
    if (!onText || text === synced.current) return
    const t = setTimeout(() => { synced.current = text; onText(text) }, 800)
    return () => clearTimeout(t)
  }, [text])
  const [atts, setAtts] = useState<Attachment[]>([])
  const attsRef = useRef(atts)
  attsRef.current = atts
  const [err, setErr] = useState<string>()
  const [hIdx, setHIdx] = useState(-1)
  const [showHist, setShowHist] = useState(false)
  const [mention, setMention] = useState<{ start: number; query: string }>()
  const [hits, setHits] = useState<FileHit[] | { error: string } | undefined>(undefined)
  const [hi, setHi] = useState(0)
  const [menuBox, setMenuBox] = useState<{ left: number; width: number; bottom: number; maxHeight: number }>()
  const ref = useRef<HTMLTextAreaElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const findRef = useRef(findFiles)
  findRef.current = findFiles
  const caret = useRef<number | undefined>(undefined)
  const seenStaged = useRef(new Set<number>())
  const accept = (list: Attachment[]): void => {
    const prev = attsRef.current
    const fresh = list.filter((a) => !a.path || !prev.some((p) => p.path === a.path))
    const allowed = fresh.filter((a) => (a.data ? canImage : canResource))
    if (allowed.length < fresh.length) {
      setErr(`${agent} does not accept ${canImage ? 'file contents' : 'images'} in prompts; that was left out`)
      if (!allowed.length) return
    }
    if ([...prev, ...allowed].reduce((s, a) => s + a.size, 0) > ATTACH_LIMIT) {
      setErr('Attachments must stay under 600 KB in total (host frame limit)')
      return
    }
    if (!allowed.length) return
    setErr(undefined)
    const next = [...prev, ...allowed]
    attsRef.current = next
    setAtts(next)
  }
  useEffect(() => { if (draft) { setText(draft.text); ref.current?.focus() } }, [draft?.n])
  // Pages and selections sent from the browser panel.
  useEffect(() => {
    if (!incoming) return
    if (incoming.append) setText((t) => (t && !/\s$/.test(t) ? t + ' ' : t) + incoming.append)
    const list = incoming.attachments ?? []
    const ok = list.filter((a) => (a.data ? canImage : canResource))
    if (ok.length < list.length) { setErr(`${agent} does not accept ${canImage ? 'file contents' : 'images'} in prompts; that was left out`); return }
    if ([...atts, ...ok].reduce((s, a) => s + a.size, 0) > ATTACH_LIMIT) { setErr('Attachments must stay under 600 KB in total (host frame limit)'); return }
    if (ok.length) { setErr(undefined); setAtts((a) => [...a, ...ok]) }
    ref.current?.focus()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [incoming?.n])
  useEffect(() => {
    if (!staged?.length) return
    const fresh = staged.filter((s) => !seenStaged.current.has(s.stamp))
    if (!fresh.length) return
    for (const s of fresh) seenStaged.current.add(s.stamp)
    accept(fresh.map((s) => s.file))
    onStagedClear?.()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [staged])
  useEffect(() => {
    const el = ref.current
    if (el) { el.style.height = 'auto'; el.style.height = Math.min(200, el.scrollHeight) + 'px' }
  }, [text])
  const syncMention = (value: string, cursor: number): void => {
    if (!findFiles || disabled) { setMention(undefined); return }
    const next = mentionAt(value, cursor)
    setMention((cur) => (cur?.start === next?.start && cur?.query === next?.query ? cur : next))
  }
  useEffect(() => { setHi(0) }, [mention?.start, mention?.query])
  useEffect(() => {
    const run = findRef.current
    if (!mention || !run) { setHits(undefined); return }
    let live = true
    setHits(undefined)
    const q = mention.query
    const t = setTimeout(() => { void run(q).then((r) => { if (live) setHits(r) }) }, 80)
    return () => { live = false; clearTimeout(t) }
  }, [mention?.query])
  useLayoutEffect(() => {
    if (!mention) { setMenuBox(undefined); return }
    const el = ref.current
    if (!el) return
    const place = (): void => {
      const r = el.getBoundingClientRect()
      const margin = 8
      const width = Math.min(420, Math.max(220, window.innerWidth - margin * 2))
      let left = r.left
      if (left + width > window.innerWidth - margin) left = Math.max(margin, window.innerWidth - margin - width)
      setMenuBox({ left, width, bottom: window.innerHeight - r.top + 6, maxHeight: Math.min(320, Math.max(120, r.top - margin - 6)) })
    }
    place()
    window.addEventListener('resize', place)
    return () => window.removeEventListener('resize', place)
  }, [mention, text])
  useEffect(() => {
    const row = menuRef.current?.querySelector<HTMLElement>('[aria-selected="true"]')
    if (row && typeof row.scrollIntoView === 'function') row.scrollIntoView({ block: 'nearest' })
  }, [hi, hits])
  useEffect(() => {
    if (caret.current === undefined) return
    const el = ref.current
    const n = caret.current
    caret.current = undefined
    el?.focus()
    el?.setSelectionRange(n, n)
  }, [text])
  const hitsList = Array.isArray(hits) ? hits : []
  const send = (): void => {
    if ((!text.trim() && !atts.length) || busy || disabled) return
    onSend(text, atts)
    if (onText && synced.current) onText('')
    synced.current = ''
    setText(''); setAtts([]); setHIdx(-1); setMention(undefined)
  }
  const pickMention = (file: FileHit): void => {
    const el = ref.current
    const cursor = el?.selectionStart ?? text.length
    const token = mentionAt(text, cursor) ?? mention
    if (token) {
      const next = dropMention(text, token.start, cursor)
      caret.current = next.caret
      setText(next.text)
    }
    setMention(undefined)
    if (!onAttachFile) return
    void onAttachFile(file.path).then((r) => {
      if (r.attachment) accept([r.attachment])
      if (r.note) setErr(r.note)
    })
  }
  const pick = async (): Promise<void> => {
    setErr(undefined)
    const r = await window.api.files.pick()
    if (!r.ok) return setErr(r.error)
    const ok = r.value.filter((a) => (a.data ? canImage : canResource))
    if (ok.length < r.value.length) setErr(`${agent} does not accept ${canImage ? 'file contents' : 'images'} in prompts; those were left out`)
    const total = [...atts, ...ok].reduce((s, a) => s + a.size, 0)
    if (total > ATTACH_LIMIT) return setErr('Attachments must stay under 600 KB in total (host frame limit)')
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
      {mention && findFiles && menuBox && createPortal(
        <div ref={menuRef} className="mention-pop" role="listbox" aria-label="Files" style={{ left: menuBox.left, width: menuBox.width, bottom: menuBox.bottom, maxHeight: menuBox.maxHeight }} onMouseDown={(e) => e.preventDefault()}>
          <div className="mention-h">Tag a file</div>
          {hits === undefined && <div className="mention-empty">Looking…</div>}
          {hits && !Array.isArray(hits) && <div className="mention-empty">{hits.error}</div>}
          {Array.isArray(hits) && hits.length === 0 && <div className="mention-empty">No files match</div>}
          {hitsList.map((file, i) => (
            <button type="button" role="option" aria-selected={i === hi} key={file.path} className={'mention-row' + (i === hi ? ' on' : '')} onMouseEnter={() => setHi(i)} onClick={() => pickMention(file)}>
              <span className="mention-ic">{I.file}</span>
              <span className="grow">
                <span className="mention-name ell">{file.name}</span>
                {file.relative !== file.name && <span className="mention-path mono ell">{file.relative}</span>}
              </span>
            </button>
          ))}
        </div>,
        document.body)}
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
        title={disabled ? undefined : 'Type @ to tag a file'}
        disabled={Boolean(disabled)}
        value={text}
        onChange={(e) => { setText(e.target.value); setHIdx(-1); syncMention(e.target.value, e.target.selectionStart ?? e.target.value.length) }}
        onSelect={(e) => { const el = e.target as HTMLTextAreaElement; syncMention(el.value, el.selectionStart ?? el.value.length) }}
        onBlur={() => { window.setTimeout(() => setMention(undefined), 0) }}
        onKeyDown={(e) => {
          if (mention && findFiles) {
            if (e.key === 'ArrowDown' && hitsList.length) { e.preventDefault(); setHi((i) => Math.min(hitsList.length - 1, i + 1)); return }
            if (e.key === 'ArrowUp' && hitsList.length) { e.preventDefault(); setHi((i) => Math.max(0, i - 1)); return }
            if ((e.key === 'Enter' && !e.shiftKey) || e.key === 'Tab') {
              if (hitsList.length) { e.preventDefault(); e.stopPropagation(); pickMention(hitsList[Math.min(hi, hitsList.length - 1)]); return }
            }
            if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setMention(undefined); return }
          }
          if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send() }
          if (e.key === 'ArrowUp' && (text === '' || hIdx >= 0) && history.length) { e.preventDefault(); const n = Math.min(history.length - 1, hIdx + 1); setHIdx(n); setText(history[n]) }
          if (e.key === 'ArrowDown' && hIdx >= 0) { e.preventDefault(); const n = hIdx - 1; setHIdx(n); setText(n < 0 ? '' : history[n]) }
        }}
      />
      {err && <div className="error" style={{ marginTop: 0, marginBottom: 6 }}>{err}</div>}
      <div className="bar">
        <button className="icon-btn" title={canImage || canResource ? 'Attach a file' : `${agent} does not accept attachments`} disabled={!canImage && !canResource} onClick={() => void pick()}>{I.attach}</button>
        <button className={'icon-btn' + (showHist ? ' on' : '')} title="Prompt history (↑ in an empty box)" onClick={() => setShowHist(!showHist)}>{I.clock}</button>
        <span style={{ flex: 1 }} />
        {busy ? (
          <button className="btn stop" onClick={onCancel} title="Cancel this turn (Esc)">{I.stop}Stop</button>
        ) : (
          <button className="send" disabled={(!text.trim() && !atts.length) || Boolean(disabled)} onClick={send} aria-label="Send">{I.send}</button>
        )}
      </div>
    </div>
  )
}

/** Display name for the host shell reported in `_meta.acpdScope.shell`. */
export function shellName(s: string): string {
  return ({ sh: 'sh', cmd: 'cmd.exe', powershell: 'PowerShell', bash: 'Git Bash' } as Record<string, string>)[s] ?? s
}

const noopAnswer = (): void => {}
