// New session: an inline composer centred in the main area (Penpot boards 96-99). The prompt
// box carries the agent and model/mode chips; a row of context chips under it picks host,
// folder, branch, access rights and the worktree switch, each opening a small anchored list.
// Sending docks the composer at the bottom (220 ms, none with reduced motion) while the
// session starts; the context folds into a header line.
import { Fragment, ReactNode, useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { AgentInfo, EarlierSessions, Workspace } from './types'
import type { Attachment } from './Session'
import type { McpServer } from './Settings'
import { I, tildify } from './ui'
import { parseBranches } from './gitparse'
import { type AgentSetup, choiceName, groupAgents, hasChoices, arranged, placePopover, setupFromAcp } from './agentSetup'
import { OptionsPanel } from './OptionPicker'
import { applyPreset, matchingPreset, NEVER_REMEMBERED, presetKey, type Preset } from './presets'

export type BranchPick = { mode: 'current' | 'existing' | 'new'; name: string; base?: string; worktree: boolean }
export type Access = { read: boolean; write: boolean; terminal: boolean; mcp: boolean; manual?: boolean; yolo?: boolean }
export type { AgentSetup } from './agentSetup'
export type StartRequest = { agent: string; workspace: string; branch: BranchPick; access: Access; text: string; atts: Attachment[]
  /** Session options picked here, in the order they were picked; applied in that order once the
   * agent is running (a provider before the model it brings). */
  choices: OptionPick[] }
export type OptionPick = { id: string; value: string }
export type HostChoice = { id: string; name: string; state: string; ok: boolean }

/** The folder choice for a session in no project: the host makes a new empty folder at start. */
export const NO_PROJECT = '\u0000no-project'
export const slug = (t: string): string => t.toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 32).replace(/^-|-$/g, '')
export const setupKey = (agent: string): string => `acportal.agentSetup.${agent}`
export function rememberedSetup(agent: string): AgentSetup {
  // Older versions saved a different shape (modes/models); it is ignored rather than guessed at.
  try { const v = JSON.parse(localStorage.getItem(setupKey(agent)) ?? '{}') as AgentSetup; return Array.isArray(v.selectors) ? v : {} } catch { return {} }
}

/** A chip that opens an anchored list. Escape or a click outside closes it and returns focus. */
function Chip({ id, open, setOpen, label, icon, active, children, wide, title, up: preferUp, busy }: { id: string; open: string | undefined; setOpen: (k: string | undefined) => void; label: ReactNode; icon?: ReactNode; active?: boolean; children: ReactNode; wide?: boolean; title?: string; up?: boolean; busy?: boolean }) {
  const btn = useRef<HTMLButtonElement>(null)
  const pop = useRef<HTMLDivElement>(null)
  const isOpen = open === id
  const [place, setPlace] = useState<{ up: boolean; dx: number; maxHeight: number }>()
  useLayoutEffect(() => {
    if (!isOpen) { setPlace(undefined); return }
    const b = btn.current?.getBoundingClientRect(); const p = pop.current
    if (!b || !p) return
    setPlace(placePopover(b, { width: p.offsetWidth, height: p.scrollHeight }, { width: window.innerWidth, height: window.innerHeight }, preferUp ? 'up' : 'down'))
  }, [isOpen])
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
      <button ref={btn} className={'ctx-chip' + (active || isOpen ? ' on' : '')} aria-haspopup="dialog" aria-expanded={isOpen} aria-busy={busy || undefined} title={title} onClick={() => setOpen(isOpen ? undefined : id)}>
        {icon}<span className="ell">{label}</span>{I.down}
      </button>
      {isOpen && <div ref={pop} className={'ctx-pop' + (wide ? ' wide' : '') + ((place ? place.up : preferUp) ? ' up' : ' down')} data-placement={place ? (place.up ? 'up' : 'down') : undefined}
        style={place ? { transform: place.dx ? `translateX(${place.dx}px)` : undefined, maxHeight: Math.min(420, place.maxHeight) } : undefined} role="dialog" aria-label={title} onKeyDown={keys}>{children}</div>}
    </span>
  )
}
function Item({ on, children, onPick, sub, preset, mode, profileOf }: { on?: boolean; children: ReactNode; onPick: () => void; sub?: ReactNode; preset?: string; mode?: string; profileOf?: string }) {
  return <button data-item data-preset={preset} data-mode={mode} data-profile-of={profileOf} className={'ctx-item' + (on ? ' on' : '')} onClick={onPick}><span className="grow ell">{children}</span>{sub && <span className="sub">{sub}</span>}{on && <span className="tick">{I.check}</span>}</button>
}
function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return <button data-item role="switch" aria-checked={on} aria-label={label} className={'switch' + (on ? ' on' : '')} onClick={() => onChange(!on)}><span /></button>
}

export function NewSessionHome({ hosts, hostId, onHost, agents, workspaces, mcp, canBranch, history, initial, initialFolder, onStart, onMulti, probe, newScratch, loadPresets, loadEarlier, openEarlier }: {
  hosts: HostChoice[]; hostId: string; onHost: (id: string) => void; agents: AgentInfo[]; workspaces: Workspace[]; mcp: McpServer[]; canBranch: boolean
  history: (ws: string) => string[]; initial?: { text: string; n: number }; initialFolder?: string
  onStart: (r: StartRequest) => Promise<string | undefined>; onMulti: (text: string) => void
  /** Ask the host which models and modes an agent offers (throwaway session); undefined if it cannot. */
  probe?: (agent: string, workspace: string) => Promise<unknown | undefined>
  /** A new empty folder for a session outside every project (hosts with `scratchSessions`). */
  newScratch?: () => Promise<string | undefined>
  /** Hosts that advertise `presets`. Undefined on an older host, which keeps the toggles only. */
  loadPresets?: (agent: string) => Promise<Preset[] | undefined>
  /** Hosts that advertise `agentSessionList`: what this agent remembers for a folder, read-only. */
  loadEarlier?: (agent: string, workspace: string) => Promise<EarlierSessions | undefined>
  /** Open one of those with the agent's `session/load`; returns an error text, or nothing. */
  openEarlier?: (agent: string, workspace: string, sessionId: string, access: Access) => Promise<string | undefined>
}) {
  const usable = agents.filter((a) => a.enabled !== false && a.status !== 'missing')
  const [agent, setAgent] = useState(localStorage.getItem('acportal.lastAgent') ?? usable[0]?.id ?? '')
  const [ws, setWs] = useState(initialFolder ?? workspaces[0]?.path ?? '')
  const [sub, setSub] = useState('')
  useEffect(() => { if (ws !== NO_PROJECT && !workspaces.some((w) => w.path === ws)) setWs(workspaces[0]?.path ?? (newScratch ? NO_PROJECT : '')) }, [workspaces, Boolean(newScratch)])
  useEffect(() => { if (initialFolder) setWs(initialFolder) }, [initialFolder])
  useEffect(() => { if (usable.length && !usable.some((x) => x.id === agent)) setAgent(usable[0].id) }, [agents])
  const [text, setText] = useState('')
  const [atts, setAtts] = useState<Attachment[]>([])
  const [open, setOpen] = useState<string>()
  const [err, setErr] = useState<string>()
  const [phase, setPhase] = useState<'idle' | 'starting'>('idle')
  const [access, setAccess] = useState<Access>({ read: true, write: true, terminal: true, mcp: true })
  const [presetList, setPresetList] = useState<Preset[]>([])
  const [presetId, setPresetId] = useState<string>()
  const [presetsLoading, setPresetsLoading] = useState(false)
  const presetTouch = useRef(false)
  // The last preset the person actually picked. Turning a right off and back on returns to it,
  // even when another preset advertises the same flags and is listed first.
  const heldPreset = useRef<string | undefined>(undefined)
  const accessRef = useRef(access)
  accessRef.current = access
  const [branch, setBranch] = useState<BranchPick>({ mode: 'current', name: '', worktree: false })
  const [branches, setBranches] = useState<string[]>()
  const [current, setCurrent] = useState<string>()
  const [query, setQuery] = useState('')
  // Options come from the agent: what the host reports for it (its sessions or an earlier probe),
  // else a probe now; only without either, what this computer saw in its last session. Nothing is
  // made up: when all fail the form says so, offers Retry, and the agent picks for itself.
  // Picks are only kept here, in order, and applied when the session starts: setting an option
  // before that would need the agent running, and some agents save every change as their default.
  // One option can change another's values (another provider, other models); the session's
  // replies then bring the agent's new lists into the header.
  const [probed, setProbed] = useState<Record<string, AgentSetup | 'loading' | 'failed'>>({})
  const remembered = rememberedSetup(agent)
  const hostReported = agents.find((a) => a.id === agent)?.setup
  const probeState = probed[hostId + '/' + agent]
  const setup: AgentSetup = hostReported && hasChoices(setupFromAcp(hostReported)) ? setupFromAcp(hostReported)
    : typeof probeState === 'object' ? probeState
    : hasChoices(remembered) ? remembered : {}
  const loadingSetup = probeState === 'loading'
  const retryProbe = () => setProbed((p) => { const { [hostId + '/' + agent]: _, ...rest } = p; return rest })
  const [picks, setPicks] = useState<OptionPick[]>([])
  const selectors = arranged(setup.selectors ?? []).filter((x) => x.choices.length)
  const valueOf = (id: string): string | undefined => [...picks].reverse().find((p) => p.id === id)?.value ?? selectors.find((x) => x.id === id)?.current
  // A pick other than the model can change what the other lists offer once the agent applies it.
  const mayChangeOthers = picks.some((p) => { const x = selectors.find((y) => y.id === p.id); return x !== undefined && x.category !== 'model' && x.id !== 'model' && selectors.length > 1 })
  // A new agent, host or folder starts from that agent's options; the last session's choices for
  // this agent are picked again where the agent still offers them.
  useEffect(() => {
    setPicks((setup.selectors ?? []).flatMap((x) => {
      const v = remembered.chosen?.[x.id]
      return v !== undefined && v !== x.current && x.choices.some((c) => c.value === v) ? [{ id: x.id, value: v }] : []
    }))
  }, [hostId, agent, ws, hasChoices(setup)])
  const pickOption = (x: { id: string }, v: string): void => setPicks((list) => [...list.filter((p) => p.id !== x.id), { id: x.id, value: v }])
  useEffect(() => {
    const key = hostId + '/' + agent
    if (!probe || !agent || !ws || ws === NO_PROJECT || hasChoices(setup) || probed[key]) return
    setProbed((p) => ({ ...p, [key]: 'loading' }))
    probe(agent, ws).then((raw) => setProbed((p) => ({ ...p, [key]: raw === undefined ? 'failed' : setupFromAcp(raw) })), () => setProbed((p) => ({ ...p, [key]: 'failed' })))
  }, [agent, hostId, ws, Boolean(probe), probeState === undefined])
  const ta = useRef<HTMLTextAreaElement>(null)
  const box = useRef<HTMLDivElement>(null)
  const target = sub.trim() ? `${ws.replace(/\/$/, '')}/${sub.trim().replace(/^\//, '')}` : ws
  const host = hosts.find((h) => h.id === hostId)
  const selectedAgent = usable.find((a) => a.id === agent)
  const agentName = selectedAgent?.profileLabel || selectedAgent?.name || agent
  useEffect(() => { if (initial) { setText(initial.text); ta.current?.focus() } }, [initial?.n])
  useEffect(() => { ta.current?.focus() }, [])
  useEffect(() => { const el = ta.current; if (el) { el.style.height = 'auto'; el.style.height = Math.min(220, el.scrollHeight) + 'px' } }, [text])
  useEffect(() => {
    let stop = false
    setBranches(undefined); setCurrent(undefined)
    if (!canBranch || !target || ws === NO_PROJECT) return
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
    // No project: the host makes the folder now, so nothing is left behind if starting is cancelled earlier.
    const workspace = ws === NO_PROJECT ? await newScratch?.() : target
    if (!workspace) { setPhase('idle'); setErr('The host could not make a folder for this session.'); return }
    const e = await onStart({ agent, workspace, branch: ws === NO_PROJECT ? { mode: 'current', name: '', worktree: false } : branch, access, text, atts, choices: picks })
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
  // The first two options (the model first) summarise the chip, as the phone's header does.
  const summary = selectors.slice(0, 2).map((x) => choiceName(x, valueOf(x.id)) ?? `${x.name}: agent's choice`).join(' · ')
  const recent = ws === NO_PROJECT ? [] : history(target).slice(0, 4)
  const starting = phase === 'starting'
  const enabledMcp = mcp.filter((m) => m.enabled)
  const rightsLabel = [access.read && 'Read', access.write && 'Write', access.terminal && 'Commands'].filter(Boolean).join(' · ') || 'No access'
  const chosenPreset = presetList.find((p) => p.id === presetId)
  const accessLabel = chosenPreset?.label ?? (presetList.length ? 'Custom' : presetsLoading ? 'Access' : rightsLabel)
  useEffect(() => {
    presetTouch.current = false
    heldPreset.current = undefined
    setPresetList([])
    setPresetId(undefined)
    if (!loadPresets || !agent) { setPresetsLoading(false); return }
    let stop = false
    setPresetsLoading(true)
    loadPresets(agent).then((list) => {
      if (stop) return
      const presets = list ?? []
      setPresetList(presets)
      setPresetsLoading(false)
      // A toggle that landed before this reply must not be overwritten. The list still appears.
      if (presetTouch.current) {
        const match = matchingPreset(presets, accessRef.current, heldPreset.current)
        setPresetId(match?.id)
        return
      }
      let remembered: string | null = null
      try { remembered = localStorage.getItem(presetKey(agent)) } catch { remembered = null }
      const hit = remembered && remembered !== NEVER_REMEMBERED ? presets.find((p) => p.id === remembered) : undefined
      // A remembered id that the host no longer offers is ignored. The flags are
      // whatever this preset advertises, including after switching agents.
      const chosen = hit ?? presets.find((preset) => preset.id === 'supervised')
      if (!chosen) return
      heldPreset.current = chosen.id
      setPresetId(chosen.id)
      setAccess((current) => applyPreset(current, chosen))
    }, () => { if (!stop) setPresetsLoading(false) })
    return () => { stop = true }
  }, [agent, loadPresets])
  const [earlier, setEarlier] = useState<{ state: 'loading' | 'done' | 'failed'; data?: EarlierSessions }>()
  useEffect(() => {
    if (open !== 'earlier' || !loadEarlier || !agent || ws === NO_PROJECT) return
    let stop = false
    setEarlier({ state: 'loading' })
    void loadEarlier(agent, target).then((data) => { if (!stop) setEarlier(data ? { state: 'done', data } : { state: 'failed' }) })
    return () => { stop = true }
  }, [open, agent, target, ws, loadEarlier])
  const pickPreset = (preset: Preset): void => {
    presetTouch.current = true
    heldPreset.current = preset.id
    setPresetId(preset.id)
    setAccess((current) => applyPreset(current, preset))
    try { localStorage.setItem(presetKey(agent), preset.id === NEVER_REMEMBERED ? 'supervised' : preset.id) } catch { /* ignore */ }
  }
  const toggleRight = (key: 'read' | 'write' | 'terminal'): void => {
    presetTouch.current = true
    const next = { ...access, [key]: !access[key] }
    const match = matchingPreset(presetList, next, presetId ?? heldPreset.current)
    if (match) heldPreset.current = match.id
    setAccess(next)
    setPresetId(match?.id)
    const remembered = !match ? 'custom' : match.id === NEVER_REMEMBERED ? 'supervised' : match.id
    try { localStorage.setItem(presetKey(agent), remembered) } catch { /* ignore */ }
  }
  const mode = selectors.find((selector) => selector.kind === 'mode' || selector.category === 'mode')

  const composer = (
    <div ref={box} className={'ns-composer' + (starting ? ' docked' : '')} onKeyDown={(e) => { if (e.key === 'Escape' && open) setOpen(undefined) }}>
      {atts.length > 0 && (
        <div className="ns-atts">{atts.map((a, i) => <span key={i} className="att-chip">{I.file}<span>{a.name}</span><button className="x" aria-label={`Remove ${a.name}`} onClick={() => setAtts(atts.filter((_, j) => j !== i))}>{I.close}</button></span>)}</div>
      )}
      <textarea ref={ta} rows={2} aria-label="Prompt for the new session" placeholder={`Ask ${agentName || 'an agent'} to build, fix or explain something…`} value={text} disabled={starting}
        onChange={(e) => setText(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send() } }} />
      <div className="ns-bar">
        <button className="icon-btn round" aria-label="Attach files" title="Attach files" onClick={() => void pick()} disabled={starting}>{I.plus}</button>
        <Chip id="agent" open={open} setOpen={setOpen} up icon={I.bot} label={agentName || 'No agent'} title="Agent">
          <div className="ctx-h">Agent</div>
          {groupAgents(usable).map((group) => <Fragment key={group.base.id}>
            <Item on={group.base.id === agent} sub={group.base.version} onPick={() => { setAgent(group.base.id); setOpen(undefined) }}>{group.base.profileLabel || group.base.name}</Item>
            {group.profiles.map((profile) => <Item key={profile.id} profileOf={group.base.id} on={profile.id === agent} sub="profile" onPick={() => { setAgent(profile.id); setOpen(undefined) }}>{profile.profileLabel || profile.name}</Item>)}
          </Fragment>)}
          {usable.length === 0 && <div className="hint ctx-pad">No agents on this host. Add one in Settings → Agents.</div>}
          <div className="ctx-sep" />
          <Item onPick={() => { setOpen(undefined); onMulti(text) }} sub="2-3 agents">{I.flow} Run on several agents…</Item>
        </Chip>
        <Chip id="model" open={open} setOpen={setOpen} up busy={loadingSetup} label={loadingSetup ? 'Reading models…' : probeState === 'failed' && !hasChoices(setup) ? 'Models unknown' : !hasChoices(setup) ? `${agentName}'s own choice` : summary} title="Model and options" wide>
          {loadingSetup && <div className="hint ctx-pad" role="status">Asking {agentName} which models and modes it offers…</div>}
          {!loadingSetup && !hasChoices(setup) && <div className="hint ctx-pad">{probeState === 'failed' ? `Could not read ${agentName}'s models and modes. The session can still start; ${agentName} then picks its own.` : `${agentName} reports no model or mode choices; it picks its own.`}</div>}
          {!loadingSetup && probeState === 'failed' && !hasChoices(setup) && <Item on={false} onPick={retryProbe}>Retry</Item>}
          {mayChangeOthers && <div className="hint ctx-pad">These choices are applied when the session starts. Lists that depend on them, such as the models of another provider, then appear in the session header.</div>}
          {selectors.length > 0 && <OptionsPanel selectors={selectors} valueOf={(x) => valueOf(x.id)} onPick={pickOption} />}
        </Chip>
        <span className="grow" />
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
      <Chip id="folder" open={open} setOpen={setOpen} icon={I.folder} label={ws === NO_PROJECT ? 'No project' : tildify(target) || 'Folder'} title="Folder" wide>
        <div className="ctx-h">Folder</div>
        {workspaces.map((w) => <Item key={w.path} on={w.path === ws} onPick={() => setWs(w.path)}><span className="mono">{tildify(w.path)}</span></Item>)}
        {workspaces.length === 0 && <div className="hint ctx-pad">No project folders are allowed on this host yet (Settings → Workspaces).</div>}
        {newScratch && <Item on={ws === NO_PROJECT} onPick={() => setWs(NO_PROJECT)} sub="new empty folder">No project</Item>}
        {ws !== NO_PROJECT && <>
          <div className="ctx-sep" />
          <label className="ctx-h" htmlFor="ns-sub">Sub-folder (optional)</label>
          <input id="ns-sub" className="input mono ctx-input" placeholder="e.g. calc" value={sub} onChange={(e) => setSub(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') setOpen(undefined) }} />
        </>}
        {ws === NO_PROJECT && <div className="hint ctx-pad">The host makes a new empty folder for this session, outside every project. Use it for questions and one-off tasks.</div>}
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
        {presetList.length > 0 && <>
          <div className="ctx-h">Preset</div>
          {presetList.map((preset) => <Item key={preset.id} preset={preset.id} sub={preset.effects[0]} on={preset.id === presetId} onPick={() => pickPreset(preset)}>{preset.label}</Item>)}
          {chosenPreset?.effects.map((line) => <div key={line} className="hint ctx-pad">{line}</div>)}
          {mode && <>
            <div className="ctx-h">{mode.name}</div>
            <div className="hint ctx-pad">The agent's own mode. It is separate from the preset.</div>
            {mode.choices.map((choice) => <Item key={choice.value} mode={choice.value} on={valueOf(mode.id) === choice.value} onPick={() => pickOption(mode, choice.value)}>{choice.name}</Item>)}
          </>}
          <div className="ctx-h">Custom</div>
        </>}
        {presetsLoading && <div className="hint ctx-pad" role="status">Reading presets…</div>}
        {presetList.length === 0 && !presetsLoading && <div className="ctx-h">Rights for this session</div>}
        {([['read', 'Read files'], ['write', 'Write files'], ['terminal', 'Run commands']] as const).map(([k, l]) => <Item key={k} on={access[k]} onPick={() => toggleRight(k)}>{l}</Item>)}
        <Item on={access.mcp && enabledMcp.length > 0} sub={enabledMcp.length ? String(enabledMcp.length) : 'none'} onPick={() => setAccess({ ...access, mcp: !access.mcp })}>MCP servers</Item>
        {access.yolo && !chosenPreset && <div className="hint ctx-pad warn-text">The host approves this session's requests itself.</div>}
        {access.manual && !chosenPreset && <div className="hint ctx-pad">The host's auto-approve and shell rules are off for this session.</div>}
        {!chosenPreset && <div className="hint ctx-pad">Writes and commands still ask for your approval. Delegation is set on the host.</div>}
      </Chip>
      {loadEarlier && ws !== NO_PROJECT && (
        <Chip id="earlier" open={open} setOpen={setOpen} icon={I.clock} label="Earlier" title="Earlier sessions of this agent in this folder" wide>
          <div className="ctx-h">Earlier sessions</div>
          {earlier?.state === 'loading' && <div className="hint ctx-pad" role="status">Asking {agentName}…</div>}
          {earlier?.state === 'failed' && <div className="hint ctx-pad">Could not read {agentName}'s sessions.</div>}
          {earlier?.state === 'done' && !earlier.data?.listSupported && <div className="hint ctx-pad">{agentName} does not list its earlier sessions.</div>}
          {earlier?.state === 'done' && earlier.data?.listSupported && earlier.data.sessions.length === 0 && <div className="hint ctx-pad">Nothing remembered for this folder.</div>}
          {earlier?.state === 'done' && earlier.data?.listSupported && !earlier.data.loadSupported && earlier.data.sessions.length > 0 && <div className="hint ctx-pad">{agentName} cannot reopen them, so this list is for reference.</div>}
          {earlier?.state === 'done' && earlier.data?.sessions.map((s) => (
            <Item key={s.sessionId} data-earlier={s.sessionId} sub={s.updatedAt ? new Date(s.updatedAt).toLocaleString() : undefined}
              onPick={() => { if (!earlier.data?.loadSupported || !openEarlier) return; setOpen(undefined); void openEarlier(agent, target, s.sessionId, access).then((e) => { if (e) setErr(e) }) }}>
              {s.title || s.sessionId}
            </Item>
          ))}
        </Chip>
      )}
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
    </div>
  )
}
