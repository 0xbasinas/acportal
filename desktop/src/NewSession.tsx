// New session: an inline composer centred in the main area (Penpot boards 96-99). The prompt
// box carries the agent and model/mode chips; a row of context chips under it picks host,
// folder, branch, access rights and the worktree switch, each opening a small anchored list.
// Sending docks the composer at the bottom (220 ms, none with reduced motion) while the
// session starts; the context folds into a header line.
import { ReactNode, useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { AgentInfo, Workspace } from './types'
import type { Attachment } from './Session'
import type { McpServer } from './Settings'
import { I, tildify } from './ui'
import { parseBranches } from './gitparse'

export type BranchPick = { mode: 'current' | 'existing' | 'new'; name: string; base?: string; worktree: boolean }
export type Access = { read: boolean; write: boolean; terminal: boolean; mcp: boolean }
export type AgentSetup = { modes?: { id: string; name: string }[]; mode?: string; modelConfig?: string; models?: { value: string; name: string }[]; model?: string }
export type StartRequest = { agent: string; workspace: string; branch: BranchPick; access: Access; text: string; atts: Attachment[]; mode?: string; model?: string }
export type HostChoice = { id: string; name: string; state: string; ok: boolean }

export const slug = (t: string): string => t.toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 32).replace(/^-|-$/g, '')
export const setupKey = (agent: string): string => `acportal.agentSetup.${agent}`
export function rememberedSetup(agent: string): AgentSetup {
  try { return JSON.parse(localStorage.getItem(setupKey(agent)) ?? '{}') as AgentSetup } catch { return {} }
}

/** A chip that opens an anchored list. Escape or a click outside closes it and returns focus. */
function Chip({ id, open, setOpen, label, icon, active, children, wide, title }: { id: string; open: string | undefined; setOpen: (k: string | undefined) => void; label: ReactNode; icon?: ReactNode; active?: boolean; children: ReactNode; wide?: boolean; title?: string }) {
  const btn = useRef<HTMLButtonElement>(null)
  const pop = useRef<HTMLDivElement>(null)
  const isOpen = open === id
  useEffect(() => {
    if (!isOpen) return
    const first = pop.current?.querySelector<HTMLElement>('input, [data-item]')
    first?.focus()
    const down = (e: MouseEvent): void => { if (!pop.current?.contains(e.target as Node) && !btn.current?.contains(e.target as Node)) setOpen(undefined) }
    document.addEventListener('mousedown', down)
    return () => document.removeEventListener('mousedown', down)
  }, [isOpen])
  const keys = (e: React.KeyboardEvent): void => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setOpen(undefined); btn.current?.focus(); return }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
    const items = [...(pop.current?.querySelectorAll<HTMLElement>('input, [data-item]') ?? [])]
    const i = items.indexOf(document.activeElement as HTMLElement)
    const next = items[(i + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length]
    if (next) { e.preventDefault(); next.focus() }
  }
  return (
    <span className="ctx-wrap">
      <button ref={btn} className={'ctx-chip' + (active || isOpen ? ' on' : '')} aria-haspopup="dialog" aria-expanded={isOpen} title={title} onClick={() => setOpen(isOpen ? undefined : id)}>
        {icon}<span className="ell">{label}</span>{I.down}
      </button>
      {isOpen && <div ref={pop} className={'ctx-pop' + (wide ? ' wide' : '')} role="dialog" aria-label={title} onKeyDown={keys}>{children}</div>}
    </span>
  )
}
function Item({ on, children, onPick, sub }: { on?: boolean; children: ReactNode; onPick: () => void; sub?: ReactNode }) {
  return <button data-item className={'ctx-item' + (on ? ' on' : '')} onClick={onPick}><span className="grow ell">{children}</span>{sub && <span className="sub">{sub}</span>}{on && <span className="tick">{I.check}</span>}</button>
}
function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return <button data-item role="switch" aria-checked={on} aria-label={label} className={'switch' + (on ? ' on' : '')} onClick={() => onChange(!on)}><span /></button>
}

export function NewSessionHome({ hosts, hostId, onHost, agents, workspaces, mcp, canBranch, history, initial, initialFolder, onStart, onMulti }: {
  hosts: HostChoice[]; hostId: string; onHost: (id: string) => void; agents: AgentInfo[]; workspaces: Workspace[]; mcp: McpServer[]; canBranch: boolean
  history: (ws: string) => string[]; initial?: { text: string; n: number }; initialFolder?: string
  onStart: (r: StartRequest) => Promise<string | undefined>; onMulti: (text: string) => void
}) {
  const usable = agents.filter((a) => a.enabled !== false && a.status !== 'missing')
  const [agent, setAgent] = useState(localStorage.getItem('acportal.lastAgent') ?? usable[0]?.id ?? '')
  const [ws, setWs] = useState(initialFolder ?? workspaces[0]?.path ?? '')
  const [sub, setSub] = useState('')
  useEffect(() => { if (!workspaces.some((w) => w.path === ws)) setWs(workspaces[0]?.path ?? '') }, [workspaces])
  useEffect(() => { if (initialFolder) setWs(initialFolder) }, [initialFolder])
  useEffect(() => { if (usable.length && !usable.some((x) => x.id === agent)) setAgent(usable[0].id) }, [agents])
  const [text, setText] = useState('')
  const [atts, setAtts] = useState<Attachment[]>([])
  const [open, setOpen] = useState<string>()
  const [err, setErr] = useState<string>()
  const [phase, setPhase] = useState<'idle' | 'starting'>('idle')
  const [access, setAccess] = useState<Access>({ read: true, write: true, terminal: true, mcp: true })
  const [branch, setBranch] = useState<BranchPick>({ mode: 'current', name: '', worktree: false })
  const [branches, setBranches] = useState<string[]>()
  const [current, setCurrent] = useState<string>()
  const [query, setQuery] = useState('')
  const setup = rememberedSetup(agent)
  const [mode, setMode] = useState<string | undefined>(setup.mode)
  const [model, setModel] = useState<string | undefined>(setup.model)
  useEffect(() => { const s = rememberedSetup(agent); setMode(s.mode); setModel(s.model) }, [agent])
  const ta = useRef<HTMLTextAreaElement>(null)
  const box = useRef<HTMLDivElement>(null)
  const target = sub.trim() ? `${ws.replace(/\/$/, '')}/${sub.trim().replace(/^\//, '')}` : ws
  const host = hosts.find((h) => h.id === hostId)
  const agentName = usable.find((a) => a.id === agent)?.name ?? agent
  useEffect(() => { if (initial) { setText(initial.text); ta.current?.focus() } }, [initial?.n])
  useEffect(() => { ta.current?.focus() }, [])
  useEffect(() => { const el = ta.current; if (el) { el.style.height = 'auto'; el.style.height = Math.min(220, el.scrollHeight) + 'px' } }, [text])
  useEffect(() => {
    let stop = false
    setBranches(undefined); setCurrent(undefined)
    if (!canBranch || !target) return
    const t = setTimeout(async () => {
      const r = await window.api.git.run(hostId, target, 'branch-list')
      if (stop) return
      if (!r.ok) { setBranches([]); return }
      const list = parseBranches(r.value.out)
      setBranches(list.filter((b) => !b.remote).map((b) => b.name))
      setCurrent(list.find((b) => b.current)?.name)
    }, 200)
    return () => { stop = true; clearTimeout(t) }
  }, [target, canBranch, hostId])
  useEffect(() => { setBranch({ mode: 'current', name: '', worktree: false }) }, [target])
  const isRepo = Boolean(branches && branches.length)
  const autoName = (): string => 'acportal/' + (slug(text.split('\n')[0]) || 'session-' + Date.now().toString(36))
  const newName = (query.trim().replace(/\s+/g, '-') || branch.name || autoName())
  const exists = (n: string): boolean => Boolean(branches?.includes(n))
  const branchLabel = branch.mode === 'current' ? (current ?? (branches === undefined && canBranch ? '…' : 'no git')) : branch.name
  const pick = async (): Promise<void> => {
    setErr(undefined)
    const r = await window.api.files.pick()
    if (!r.ok) return setErr(r.error)
    const total = [...atts, ...r.value].reduce((s, a) => s + a.size, 0)
    if (total > 600 * 1024) return setErr('Attachments must stay under 600 KB in total (host frame limit)')
    setAtts([...atts, ...r.value])
  }
  // Docking: measure before and after the layout change, then animate the difference (FLIP).
  const from = useRef<DOMRect | undefined>(undefined)
  const send = async (): Promise<void> => {
    if (!text.trim() || !agent || !ws || phase === 'starting') return
    if (branch.mode === 'new' && exists(branch.name)) return setErr(`A branch named ${branch.name} already exists.`)
    from.current = box.current?.getBoundingClientRect()
    setPhase('starting'); setErr(undefined); setOpen(undefined)
    localStorage.setItem('acportal.lastAgent', agent)
    const e = await onStart({ agent, workspace: target, branch, access, text, atts, mode, model })
    if (e) { setPhase('idle'); setErr(e) }
  }
  useLayoutEffect(() => {
    const el = box.current
    if (phase !== 'starting' || !el || !from.current) return
    const to = el.getBoundingClientRect()
    const dy = from.current.top - to.top
    from.current = undefined
    if (!dy || matchMedia('(prefers-reduced-motion: reduce)').matches) return
    el.animate([{ transform: `translateY(${dy}px)` }, { transform: 'translateY(0)' }], { duration: 220, easing: 'cubic-bezier(.2,.8,.2,1)' })
  }, [phase])
  const modeName = setup.modes?.find((m) => m.id === mode)?.name
  const modelName = setup.models?.find((m) => m.value === model)?.name ?? model
  const recent = history(target).slice(0, 4)
  const starting = phase === 'starting'
  const enabledMcp = mcp.filter((m) => m.enabled)
  const accessLabel = [access.read && 'Read', access.write && 'Write', access.terminal && 'Commands'].filter(Boolean).join(' · ') || 'No access'

  const composer = (
    <div ref={box} className={'ns-composer' + (starting ? ' docked' : '')} onKeyDown={(e) => { if (e.key === 'Escape' && open) setOpen(undefined) }}>
      {atts.length > 0 && (
        <div className="ns-atts">{atts.map((a, i) => <span key={i} className="att-chip">{I.file}<span>{a.name}</span><button className="x" aria-label={`Remove ${a.name}`} onClick={() => setAtts(atts.filter((_, j) => j !== i))}>{I.close}</button></span>)}</div>
      )}
      <textarea ref={ta} rows={2} aria-label="Prompt for the new session" placeholder={`Ask ${agentName || 'an agent'} to build, fix or explain something…`} value={text} disabled={starting}
        onChange={(e) => setText(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send() } }} />
      <div className="ns-bar">
        <button className="icon-btn round" aria-label="Attach files" title="Attach files" onClick={() => void pick()} disabled={starting}>{I.plus}</button>
        <Chip id="agent" open={open} setOpen={setOpen} icon={I.bot} label={agentName || 'No agent'} title="Agent">
          <div className="ctx-h">Agent</div>
          {usable.map((a) => <Item key={a.id} on={a.id === agent} sub={a.version} onPick={() => { setAgent(a.id); setOpen(undefined) }}>{a.name}</Item>)}
          {usable.length === 0 && <div className="hint ctx-pad">No agents on this host. Add one in Settings → Agents.</div>}
          <div className="ctx-sep" />
          <Item onPick={() => { setOpen(undefined); onMulti(text) }} sub="2-3 agents">{I.flow} Run on several agents…</Item>
        </Chip>
        <Chip id="model" open={open} setOpen={setOpen} label={`${modelName ?? 'Default model'} · ${modeName ?? 'default mode'}`} title="Model and mode">
          {!setup.models?.length && !setup.modes?.length && <div className="hint ctx-pad">{agentName} reports its models and modes once a session starts. The choice you make then is remembered here.</div>}
          {setup.models?.length ? <><div className="ctx-h">Model</div>{setup.models.map((m) => <Item key={m.value} on={m.value === model} onPick={() => setModel(m.value)}>{m.name}</Item>)}</> : null}
          {setup.modes?.length ? <><div className="ctx-h">Mode</div>{setup.modes.map((m) => <Item key={m.id} on={m.id === mode} onPick={() => setMode(m.id)}>{m.name}</Item>)}</> : null}
        </Chip>
        <span className="grow" />
        <span className="hint-keys">Enter to send · Shift Enter new line</span>
        <button className="send" aria-label="Start session and send" disabled={!text.trim() || !agent || !ws || starting} onClick={() => void send()}>{I.send}</button>
      </div>
    </div>
  )

  const context = (
    <div className="ctx-row" role="toolbar" aria-label="Where the session runs">
      <Chip id="host" open={open} setOpen={setOpen} icon={<span className={'dot ' + (host?.ok ? "green" : "")} />} label={host?.name ?? 'Host'} title="Host">
        <div className="ctx-h">Host</div>
        {hosts.map((h) => <Item key={h.id} on={h.id === hostId} sub={h.state} onPick={() => { setOpen(undefined); if (h.id !== hostId && h.ok) onHost(h.id) }}>{h.name}</Item>)}
        <div className="hint ctx-pad">Choosing another host switches the whole window to it.</div>
      </Chip>
      <Chip id="folder" open={open} setOpen={setOpen} icon={I.folder} label={tildify(target) || 'Folder'} title="Folder" wide>
        <div className="ctx-h">Folder</div>
        {workspaces.map((w) => <Item key={w.path} on={w.path === ws} onPick={() => setWs(w.path)}><span className="mono">{tildify(w.path)}</span></Item>)}
        {workspaces.length === 0 && <div className="hint ctx-pad">No folders are allowed on this host yet (Settings → Workspaces).</div>}
        <div className="ctx-sep" />
        <label className="ctx-h" htmlFor="ns-sub">Sub-folder (optional)</label>
        <input id="ns-sub" className="input mono ctx-input" placeholder="e.g. calc" value={sub} onChange={(e) => setSub(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') setOpen(undefined) }} />
      </Chip>
      <Chip id="branch" open={open} setOpen={setOpen} icon={I.branch} label={branchLabel + (branch.worktree ? ' · worktree' : '')} title="Branch" active={branch.mode !== 'current'} wide>
        {!canBranch ? <div className="hint ctx-pad">This host does not run git for the app; the session starts on whatever branch the folder is on.</div>
          : branches === undefined ? <div className="hint ctx-pad">Reading branches…</div>
          : !isRepo ? <div className="hint ctx-pad">Not a git repository; the session starts in the folder as it is.</div>
          : (
            <>
              <input className="input ctx-input" aria-label="Find or create a branch" placeholder="Find or create a branch" value={query} onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { const n = query.trim().replace(/\s+/g, '-'); if (!n) return; setBranch(exists(n) ? { ...branch, mode: n === current ? 'current' : 'existing', name: n === current ? '' : n } : { ...branch, mode: 'new', name: n, base: branch.base ?? current }); setQuery(''); setOpen(undefined) } }} />
              <div className="ctx-h">Current</div>
              <Item on={branch.mode === 'current'} onPick={() => { setBranch({ mode: 'current', name: '', worktree: false }); setOpen(undefined) }}><span className="mono">{current}</span></Item>
              {(branches ?? []).filter((b) => b !== current && b.includes(query.trim())).length > 0 && <div className="ctx-h">Existing</div>}
              <div className="ctx-scroll">
                {(branches ?? []).filter((b) => b !== current && b.includes(query.trim())).map((b) => <Item key={b} on={branch.mode === 'existing' && branch.name === b} onPick={() => { setBranch({ ...branch, mode: 'existing', name: b }); setOpen(undefined) }}><span className="mono">{b}</span></Item>)}
              </div>
              <div className="ctx-sep" />
              {!exists(newName) && (
                <Item on={branch.mode === 'new'} onPick={() => { setBranch({ ...branch, mode: 'new', name: newName, base: branch.base ?? current }); setQuery(''); setOpen(undefined) }}>
                  {I.plus} New branch <span className="mono">{newName}</span>
                </Item>
              )}
              <div className="ctx-line"><span className="sub">from</span>
                <select className="input mono ctx-select" aria-label="Base branch for a new branch" value={branch.base ?? current} onChange={(e) => setBranch({ ...branch, base: e.target.value })}>{(branches ?? []).map((b) => <option key={b} value={b}>{b}</option>)}</select>
              </div>
              <div className="ctx-sep" />
              <div className="ctx-line"><span className="grow"><b>Own worktree</b><br /><span className="sub">A new folder beside the repo; your checkout stays untouched</span></span>
                <Switch label="Own worktree" on={branch.worktree} onChange={(v) => setBranch(v && branch.mode === 'current' ? { mode: 'new', name: newName, base: current, worktree: true } : { ...branch, worktree: v })} /></div>
            </>
          )}
      </Chip>
      <Chip id="access" open={open} setOpen={setOpen} icon={I.shield} label={accessLabel} title="Access rights">
        <div className="ctx-h">Rights for this session</div>
        {([['read', 'Read files'], ['write', 'Write files'], ['terminal', 'Run commands']] as const).map(([k, l]) => <Item key={k} on={access[k]} onPick={() => setAccess({ ...access, [k]: !access[k] })}>{l}</Item>)}
        <Item on={access.mcp && enabledMcp.length > 0} sub={enabledMcp.length ? String(enabledMcp.length) : 'none'} onPick={() => setAccess({ ...access, mcp: !access.mcp })}>MCP servers</Item>
        <div className="hint ctx-pad">Writes and commands still ask for your approval. Delegation is set on the host.</div>
      </Chip>
      <span className="ctx-switch" title={isRepo ? 'Start in its own git worktree' : 'Needs a git repository'}>
        <span>Worktree</span>
        <Switch label="Own worktree" on={branch.worktree} onChange={(v) => { if (!isRepo) return; setBranch(v && branch.mode === 'current' ? { mode: 'new', name: newName, base: current, worktree: true } : { ...branch, worktree: v }) }} />
      </span>
    </div>
  )

  if (starting)
    return (
      <div className="main ns-main docked">
        <div className="topbar"><div className="topbar-title"><h1 className="ell">{text.split('\n')[0].slice(0, 60)}</h1>
          <div className="ns-fold"><span>{agentName} · Starting…</span><span>{host?.name}</span><span className="mono">{tildify(target)}</span>{branch.mode !== 'current' || current ? <span className="mono">{I.branch}{branch.mode === 'current' ? current : branch.name}{branch.worktree ? ' · worktree' : ''}</span> : null}<span>{accessLabel}</span></div></div></div>
        <div className="ns-thread"><div className="bubble-user">{text}</div><div className="sub ns-starting"><span className="dot white live" />Starting {agentName} on {host?.name}…</div></div>
        {composer}
      </div>
    )
  return (
    <div className="main ns-main">
      <div className="ns-center">
        <h1 className="ns-title">What should we work on?</h1>
        <div className="sub ns-lead">New session on {host?.name ?? 'this host'} in {tildify(target) || 'a folder'}. Change anything with the chips below.</div>
        {composer}
        {context}
        {err && <div className="error ns-err" role="alert">{err}</div>}
        {recent.length > 0 && (
          <div className="ns-recent">
            <div className="section-label">Recent prompts in {tildify(target)}</div>
            <div className="ns-grid">{recent.map((h, i) => <button key={i} className="ns-card" onClick={() => { setText(h); ta.current?.focus() }}><span className="ell2">{h.split('\n')[0]}</span></button>)}</div>
          </div>
        )}
      </div>
      <div className="ns-keys sub">Ctrl N new session · Ctrl K commands · Shift Enter new line</div>
    </div>
  )
}
