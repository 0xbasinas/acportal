import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { AgentInfo, Host, HostStatus, SessionInfo, Task, Workspace } from './types'
import { changedFiles, emptyTimeline, reduce, Timeline } from './acp'
import { Pairing } from './Pairing'
import { Command, Palette } from './Palette'
import { Composer, Conversation } from './Session'
import { DiffReview, RightPanel, mergeError } from './Tasks'
import { Section, Settings, Theme } from './Settings'
import { I, baseName, stateDot, stateWord, tildify } from './ui'

type View = { kind: 'workspace' } | { kind: 'settings'; section: Section } | { kind: 'review'; focus?: string } | { kind: 'task'; taskId: string } | { kind: 'pair' }
type ConnState = { kind: string; attempt?: number; reason?: string }
const TITLES_KEY = 'acportal.titles'
const titles = (): Record<string, string> => JSON.parse(localStorage.getItem(TITLES_KEY) ?? '{}')

export default function App() {
  const [hosts, setHosts] = useState<Host[] | null>(null)
  const [hostId, setHostId] = useState<string>()
  const host = hosts?.find((h) => h.id === hostId)
  const [status, setStatus] = useState<HostStatus>()
  const [online, setOnline] = useState(false)
  const [hostError, setHostError] = useState<string>()
  const [agents, setAgents] = useState<AgentInfo[]>([])
  const [workspaces, setWorkspaces] = useState<Workspace[]>([])
  const [sessions, setSessions] = useState<SessionInfo[]>([])
  const [tasks, setTasks] = useState<Task[]>([])
  const [selected, setSelected] = useState<string>()
  const [timelines, setTimelines] = useState<Record<string, Timeline>>({})
  const [conn, setConn] = useState<Record<string, ConnState>>({})
  const [view, setView] = useState<View>({ kind: 'workspace' })
  const [palette, setPalette] = useState(false)
  const [newSession, setNewSession] = useState(false)
  const [theme, setThemeState] = useState<Theme>('dark')
  const [toast, setToast] = useState<string>()
  const [merging, setMerging] = useState(false)
  const [confirmMerge, setConfirmMerge] = useState<Task>()
  const attached = useRef<string | undefined>(undefined)
  const taskAttached = useRef<string | undefined>(undefined)
  const nextId = useRef(Date.now() % 1_000_000)

  const flash = (m: string): void => { setToast(m); setTimeout(() => setToast(undefined), 4000) }

  // theme
  useEffect(() => {
    void window.api.prefs.get().then((r) => { if (r.ok && r.value.theme) setThemeState(r.value.theme as Theme); if (r.ok && r.value.host) setHostId(r.value.host as string) })
  }, [])
  const [systemDark, setSystemDark] = useState(matchMedia('(prefers-color-scheme: dark)').matches)
  useEffect(() => {
    const m = matchMedia('(prefers-color-scheme: dark)')
    const l = (): void => setSystemDark(m.matches)
    m.addEventListener('change', l)
    return () => m.removeEventListener('change', l)
  }, [])
  useEffect(() => { document.documentElement.dataset.theme = theme === 'system' ? (systemDark ? 'dark' : 'light') : theme }, [theme, systemDark])
  const savePrefs = (p: Record<string, unknown>): void => void window.api.prefs.get().then((r) => window.api.prefs.set({ ...(r.ok ? r.value : {}), ...p }))
  const setTheme = (t: Theme): void => { setThemeState(t); savePrefs({ theme: t }) }

  // hosts
  const loadHosts = useCallback(async () => {
    const r = await window.api.hosts.list()
    const list = r.ok ? (r.value as Host[]) : []
    setHosts(list)
    setHostId((cur) => (cur && list.some((h) => h.id === cur) ? cur : list[0]?.id))
  }, [])
  useEffect(() => { void loadHosts() }, [loadHosts])
  useEffect(() => { if (hostId) savePrefs({ host: hostId }) }, [hostId])

  const api = useCallback(async <T,>(method: string, path: string, body?: unknown, query?: Record<string, string>): Promise<{ ok: true; value: T } | { ok: false; error: string; code?: string; status?: number }> => {
    if (!hostId) return { ok: false, error: 'No host' }
    return (await window.api.hosts.api(hostId, method, path, body, query)) as never
  }, [hostId])

  const refresh = useCallback(async () => {
    if (!hostId) return
    const s = await api<HostStatus>('GET', '/v1/status')
    if (!s.ok) { setOnline(false); setHostError(s.code === 'locked' ? 'This computer has no system keychain, so the token was kept only until the app quit. Pair again (Settings → Hosts).' : s.status === 401 ? 'This device is no longer paired with the host. Pair again in Settings → Hosts.' : s.error); return }
    setOnline(true); setHostError(undefined); setStatus(s.value)
    const [a, w, ss] = await Promise.all([api<AgentInfo[]>('GET', '/v1/agents'), api<Workspace[]>('GET', '/v1/workspaces'), api<SessionInfo[]>('GET', '/v1/sessions')])
    if (a.ok) setAgents(a.value)
    if (w.ok) setWorkspaces(w.value)
    if (ss.ok) setSessions(ss.value)
    if (s.value.features?.includes('orchestrator')) { const t = await api<Task[]>('GET', '/v1/tasks'); if (t.ok) setTasks(t.value) }
  }, [api, hostId])
  useEffect(() => {
    setSessions([]); setTasks([]); setSelected(undefined); setTimelines({})
    void refresh()
    const t = setInterval(() => void refresh(), 4000)
    return () => clearInterval(t)
  }, [refresh])

  // socket events
  useEffect(() => {
    const off1 = window.api.session.onEnvelope((key, env) => {
      const sid = key.split('/')[1]
      setTimelines((all) => ({ ...all, [sid]: reduce(all[sid] ?? emptyTimeline(), env as Record<string, unknown>) }))
    })
    const off2 = window.api.session.onState((key, s) => setConn((c) => ({ ...c, [key.split('/')[1]]: s as ConnState })))
    return () => { off1(); off2() }
  }, [])

  async function attach(sid: string, ref: typeof attached, readOnly = false): Promise<void> {
    if (!hostId || ref.current === `${hostId}/${sid}`) return
    if (ref.current) void window.api.session.detach(ref.current)
    setTimelines((all) => ({ ...all, [sid]: emptyTimeline() }))
    const r = await window.api.session.attach(hostId, sid, 0, readOnly)
    if (r.ok) ref.current = r.value
    else flash(r.error)
  }
  useEffect(() => { if (selected) void attach(selected, attached) }, [selected, hostId])

  const session = sessions.find((s) => s.id === selected)
  const tl = selected ? timelines[selected] : undefined
  const agentName = (id?: string): string => agents.find((a) => a.id === id)?.name ?? id ?? ''
  const rootTask = tasks.find((t) => t.session === selected && !t.parent) ?? tasks.find((t) => t.session === selected)
  const title = (s: SessionInfo): string => timelines[s.id]?.title ?? titles()[s.id] ?? `${agentName(s.agentId)} in ${baseName(s.workspace)}`
  const pendingIn = (sid: string): number => (timelines[sid]?.items ?? []).filter((i) => i.kind === 'permission' && i.answered === undefined).length

  async function sendPrompt(text: string): Promise<void> {
    if (!session || !attached.current) return
    const id = ++nextId.current
    if (!titles()[session.id]) localStorage.setItem(TITLES_KEY, JSON.stringify({ ...titles(), [session.id]: text.split('\n')[0].slice(0, 60) }))
    setTimelines((all) => ({ ...all, [session.id]: { ...(all[session.id] ?? emptyTimeline()), promptId: id, busy: true } }))
    const r = await window.api.session.send(attached.current, { jsonrpc: '2.0', id, method: 'session/prompt', params: { sessionId: session.acpSessionId, prompt: [{ type: 'text', text }] } })
    if (!r.ok) { flash(r.error); setTimelines((all) => ({ ...all, [session.id]: { ...all[session.id], busy: false } })) }
  }
  async function cancelTurn(): Promise<void> {
    if (!session || !attached.current) return
    await window.api.session.send(attached.current, { jsonrpc: '2.0', method: 'session/cancel', params: { sessionId: session.acpSessionId } })
  }
  async function answer(id: string | number, optionId: string | null): Promise<void> {
    if (!attached.current || !selected) return
    const outcome = optionId ? { outcome: 'selected', optionId } : { outcome: 'cancelled' }
    const r = await window.api.session.send(attached.current, { jsonrpc: '2.0', id, result: { outcome } })
    if (!r.ok) return flash(r.error)
    setTimelines((all) => {
      const t = all[selected]
      return { ...all, [selected]: { ...t, items: t.items.map((i) => (i.kind === 'permission' && i.id === id ? { ...i, answered: optionId ?? 'cancelled' } : i)) } }
    })
  }
  async function createSession(agentId: string, workspace: string): Promise<string | undefined> {
    const r = await api<SessionInfo>('POST', '/v1/sessions', { agentId, workspace, loadSessionId: null })
    if (!r.ok) return r.error
    setSessions((s) => [...s, r.value])
    setSelected(r.value.id)
    setView({ kind: 'workspace' })
    setNewSession(false)
  }
  async function endSession(id: string): Promise<void> {
    const r = await api('DELETE', `/v1/sessions/${id}`)
    if (!r.ok) return flash(r.error)
    if (attached.current?.endsWith(id)) { void window.api.session.detach(attached.current); attached.current = undefined }
    setSelected(undefined)
    void refresh()
  }
  async function merge(t: Task): Promise<void> {
    setConfirmMerge(undefined)
    setMerging(true)
    const r = await api<{ merged: string; branch: string; into: string }>('POST', `/v1/tasks/${t.id}/merge`)
    setMerging(false)
    if (r.ok) flash(`Merged ${r.value.branch} into ${tildify(r.value.into)} (${String(r.value.merged).slice(0, 8)})`)
    else flash(mergeError(r.code, r.error))
    void refresh()
  }
  async function cancelTask(t: Task): Promise<void> {
    const r = await api('POST', `/v1/tasks/${t.id}/cancel`)
    flash(r.ok ? 'Cancel requested' : r.error)
    void refresh()
  }

  // task view attaches read-only to the task's own session for its diffs
  const viewTask = view.kind === 'task' ? tasks.find((t) => t.id === view.taskId) : undefined
  useEffect(() => {
    if (viewTask?.session && viewTask.session !== selected) void attach(viewTask.session, taskAttached, true)
    if (!viewTask && taskAttached.current) { void window.api.session.detach(taskAttached.current); taskAttached.current = undefined }
  }, [viewTask?.session])

  const commands: Command[] = useMemo(() => {
    const c: Command[] = [
      { id: 'new', group: 'Actions', label: 'New session…', hint: 'Ctrl N', run: () => setNewSession(true) },
      { id: 'review', group: 'Actions', label: 'Review changes in this session', hint: 'Ctrl D', run: () => setView({ kind: 'review' }) },
      { id: 'stop', group: 'Actions', label: 'Stop the current turn', hint: 'Esc', run: () => void cancelTurn() },
      { id: 'settings', group: 'Actions', label: 'Open settings', hint: 'Ctrl ,', run: () => setView({ kind: 'settings', section: 'General' }) },
      { id: 'pair', group: 'Actions', label: 'Pair another host…', run: () => setView({ kind: 'pair' }) },
      { id: 'theme', group: 'Appearance', label: `Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`, run: () => setTheme(theme === 'dark' ? 'light' : 'dark') },
      { id: 'theme-sys', group: 'Appearance', label: 'Use system theme', run: () => setTheme('system') }
    ]
    for (const s of sessions) c.push({ id: 's' + s.id, group: 'Sessions', label: title(s), run: () => { setSelected(s.id); setView({ kind: 'workspace' }) } })
    for (const t of tasks) c.push({ id: 't' + t.id, group: 'Tasks', label: `${t.worktree?.branch ?? 'task ' + t.id.slice(0, 6)} · ${t.agent} · ${stateWord(t.state)}`, run: () => setView({ kind: 'task', taskId: t.id }) })
    for (const h of hosts ?? []) if (h.id !== hostId) c.push({ id: 'h' + h.id, group: 'Hosts', label: `Switch to ${h.name}`, run: () => setHostId(h.id) })
    return c
  }, [sessions, tasks, hosts, hostId, theme, timelines])

  useEffect(() => {
    const k = (e: KeyboardEvent): void => {
      const mod = e.metaKey || e.ctrlKey
      if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); setPalette((p) => !p) }
      else if (mod && e.key.toLowerCase() === 'n') { e.preventDefault(); setNewSession(true) }
      else if (mod && e.key === ',') { e.preventDefault(); setView({ kind: 'settings', section: 'General' }) }
      else if (mod && e.key.toLowerCase() === 'd') { e.preventDefault(); setView({ kind: 'review' }) }
      else if (e.key === 'Escape' && !palette && view.kind !== 'workspace') setView({ kind: 'workspace' })
      else if (e.key === 'Escape' && tl?.busy) void cancelTurn()
    }
    window.addEventListener('keydown', k)
    return () => window.removeEventListener('keydown', k)
  }, [palette, view, tl?.busy])

  // test hook: window.__acportal for screenshot automation
  ;(window as unknown as Record<string, unknown>).__acportal = { setView, setPalette, setNewSession, setSelected, setTheme, sessions, tasks, setConfirmMerge }

  if (hosts === null) return null
  if (hosts.length === 0 || view.kind === 'pair')
    return <Pairing onPaired={(h) => { void loadHosts().then(() => setHostId(h.id)); setView({ kind: 'workspace' }); flash(`Paired with ${h.name}`) }} onCancel={hosts.length ? () => setView({ kind: 'workspace' }) : undefined} />

  const overlays = (
    <>
      {palette && <Palette commands={commands} onClose={() => setPalette(false)} />}
      {newSession && <NewSession agents={agents} workspaces={workspaces} onClose={() => setNewSession(false)} onCreate={createSession} />}
      {confirmMerge && (
        <div className="overlay" onMouseDown={() => setConfirmMerge(undefined)}>
          <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
            <h3 style={{ margin: 0, fontSize: 17 }}>Merge into parent folder?</h3>
            <p className="sub2">Merges <span className="mono">{confirmMerge.worktree?.branch}</span> into the checkout it was made from as one <span className="mono">--no-ff</span> commit. A conflict or uncommitted changes in the parent stop the merge and change nothing.</p>
            <div style={{ display: 'flex', gap: 8, marginTop: 16 }}>
              <button className="btn-wide" onClick={() => void merge(confirmMerge)}>Merge</button>
              <button className="btn-wide ghost" onClick={() => setConfirmMerge(undefined)}>Cancel</button>
            </div>
          </div>
        </div>
      )}
      {toast && <div className="toast">{toast}</div>}
    </>
  )

  if (view.kind === 'settings')
    return (
      <>
        <Settings section={view.section} setSection={(s) => setView({ kind: 'settings', section: s })} onBack={() => setView({ kind: 'workspace' })}
          hosts={hosts} current={host} status={status} online={online} agents={agents} workspaces={workspaces} theme={theme} setTheme={setTheme}
          onPairNew={() => setView({ kind: 'pair' })} onSwitch={(h) => setHostId(h.id)}
          onForget={async (h, revoke) => { if (!confirm(`Unpair ${h.name}? This device's token is revoked on the host.`)) return; await window.api.hosts.forget(h.id, revoke); await loadHosts() }}
          onAddWorkspace={async (p) => { const r = await api('POST', '/v1/workspaces', { path: p }); if (!r.ok) return r.error; void refresh() }}
          onRemoveWorkspace={async (p) => { await api('DELETE', '/v1/workspaces', { path: p }); void refresh() }} />
        {overlays}
      </>
    )

  const projects = workspaces
  const sidebar = (
    <aside className="side">
      <div className="host-head">
        <div style={{ minWidth: 0 }}>
          <div className="host-name">{status?.name ?? host?.name}</div>
          <div className="sub mono" style={{ display: 'flex', alignItems: 'center', gap: 6 }}><span className={'dot ' + (online ? 'green' : 'red')} />{host?.address.replace(/^https?:\/\//, '')}</div>
        </div>
        <button className="icon-btn" onClick={() => void refresh()} title="Refresh">{I.refresh}</button>
      </div>
      <div style={{ padding: '4px 16px 6px' }}>
        <button className="pill" onClick={() => setNewSession(true)} disabled={!online}>{I.plus} New session</button>
      </div>
      <div className="section-label">Projects</div>
      <div style={{ padding: '0 8px' }}>
        {projects.length === 0 && <div className="sub" style={{ padding: '0 8px' }}>No folders are allowed on this host yet.</div>}
        {projects.slice(0, 6).map((w) => (
          <div className="row" key={w.path} style={{ padding: '5px 8px' }} onClick={() => setNewSession(true)} title={w.path}>
            <span className="sub">▸</span><span className="grow">{w.name}</span>
            <span className="sub mono">{sessions.filter((s) => s.workspace.startsWith(w.path)).length || ''}</span>
          </div>
        ))}
      </div>
      <div className="section-label">Active sessions</div>
      <div className="list">
        {sessions.length === 0 && <div className="sub" style={{ padding: '0 8px' }}>No sessions on this host.</div>}
        {[...sessions].reverse().map((s) => {
          const t = timelines[s.id]
          const waiting = pendingIn(s.id)
          const st = waiting ? 'waiting' : t?.busy ? 'running' : s.status
          return (
            <div key={s.id} className={'row' + (s.id === selected ? ' active' : '')} onClick={() => { setSelected(s.id); setView({ kind: 'workspace' }) }}>
              <div className="grow">
                <div className="title">{title(s)}</div>
                <div className={'sub' + (waiting ? ' amber' : s.status === 'interrupted' ? ' red' : '')}>{s.agentId} · {waiting ? `${waiting} request${waiting > 1 ? 's' : ''} waiting` : stateWord(st)}</div>
              </div>
              <span className={'dot ' + stateDot(st)} />
            </div>
          )
        })}
      </div>
      <div className="side-foot">
        <span className="sub2">{host?.name}</span>
        <div style={{ display: 'flex', gap: 2 }}>
          <button className="icon-btn" onClick={() => setPalette(true)} title="Command palette (Ctrl K)">⌘</button>
          <button className="icon-btn" onClick={() => setView({ kind: 'settings', section: 'General' })} title="Settings">{I.gear}</button>
        </div>
      </div>
    </aside>
  )

  let center
  if (view.kind === 'review') {
    center = <DiffReview title={session ? `Review: ${title(session)}` : 'Review changes'} subtitle={session ? `${session.agentId} · ${tildify(session.workspace)}` : ''} diffs={tl ? changedFiles(tl).map((f) => f.diff) : []} focus={view.focus} root={session?.workspace} onBack={() => setView({ kind: 'workspace' })} task={rootTask?.worktree ? rootTask : undefined} onMerge={setConfirmMerge} merging={merging} />
  } else if (view.kind === 'task' && viewTask) {
    const ttl = viewTask.session ? timelines[viewTask.session] : undefined
    const waiting = viewTask.needsDecision?.length ?? viewTask.pendingDecisions ?? 0
    center = (
      <div className="main">
        <DiffReview title={viewTask.worktree?.branch ?? `Task ${viewTask.id.slice(0, 8)}`} subtitle={`${viewTask.agent} · ${stateWord(viewTask.state)}${viewTask.summary ? ` (${viewTask.summary})` : ''} · ${viewTask.worktree ? 'own worktree of ' + tildify((viewTask.worktree as { repository?: string }).repository ?? viewTask.scope) : tildify(viewTask.scope)}${waiting ? ` · ${waiting} waiting for a decision` : ''}${viewTask.costUsd ? ` · US$${viewTask.costUsd.toFixed(4)}` : ''}`}
          root={viewTask.worktree?.path ?? viewTask.scope} diffs={ttl ? changedFiles(ttl).map((f) => f.diff) : []} onBack={() => setView({ kind: 'workspace' })} task={viewTask} onMerge={setConfirmMerge} merging={merging} />
        {!['completed', 'failed', 'canceled', 'rejected'].includes(viewTask.state) && (
          <div style={{ padding: '10px 20px', borderTop: '1px solid var(--line)', display: 'flex', gap: 8 }}>
            {viewTask.session && <button className="pill" onClick={() => { setSelected(viewTask.session!); setView({ kind: 'workspace' }) }}>Open its session</button>}
            <button className="pill" onClick={() => void cancelTask(viewTask)}>Cancel task</button>
          </div>
        )}
      </div>
    )
  } else if (!session) {
    center = (
      <div className="main">
        {hostError && <div className="banner red">{hostError}</div>}
        <div className="empty">
          <div>
            <h2>{online ? 'Pick a session or start one' : hostError && /[Pp]air again/.test(hostError) ? 'Pair this host again' : 'Host offline'}</h2>
            <div>{online ? `${sessions.length} session${sessions.length === 1 ? '' : 's'} on ${status?.name ?? 'this host'} · ${agents.filter((a) => a.enabled !== false).length} agents` : hostError && /[Pp]air again/.test(hostError) ? 'Run acpd pair on the host and enter the new code.' : 'Waiting for the host to answer. Check that acpd is running and reachable.'}</div>
            <div style={{ display: 'flex', gap: 8, justifyContent: 'center', marginTop: 18 }}>
              {!online && hostError && /[Pp]air again/.test(hostError) ? <button className="pill primary" onClick={() => setView({ kind: 'pair' })}>Pair again</button> : <button className="pill primary" onClick={() => setNewSession(true)} disabled={!online}>{I.plus} New session</button>}
              <button className="pill" onClick={() => setPalette(true)}>Commands <span className="kbd">Ctrl K</span></button>
            </div>
          </div>
        </div>
      </div>
    )
  } else {
    const c = conn[session.id]
    const setup = (session.setup ?? {}) as { modes?: { currentModeId?: string; availableModes?: { id: string; name: string }[] }; configOptions?: { id: string; name: string; category?: string; currentValue?: string; options?: { value: string; name: string }[] }[]; models?: { currentModelId?: string } }
    const modelOpt = setup.configOptions?.find((o) => o.id === 'model' || o.category === 'model')
    const modelValue = tl?.config?.[modelOpt?.id ?? ''] ?? modelOpt?.currentValue ?? setup.models?.currentModelId
    const modeValue = tl?.currentMode ?? setup.modes?.currentModeId
    const configure = (method: string, params: Record<string, unknown>): void => {
      if (attached.current) void window.api.session.send(attached.current, { jsonrpc: '2.0', id: ++nextId.current, method, params: { sessionId: session.acpSessionId, ...params } }).then((r) => { if (!r.ok) flash(r.error) })
    }
    const interrupted = session.status === 'interrupted'
    center = (
      <div className="main">
        <div className="topbar">
          <div style={{ flex: 1, minWidth: 0 }}>
            <h1>{title(session)}</h1>
            <div className="sub">{session.agentId} · {tl?.busy ? 'Running' : stateWord(session.status)} · {tildify(session.workspace)}</div>
          </div>
          <span className="chip">{agentName(session.agentId)}</span>
          {setup.modes?.availableModes?.length ? (
            <select className="mode-select" title="Agent mode" value={modeValue} onChange={(e) => { const v = e.target.value; setTimelines((a) => ({ ...a, [session.id]: { ...a[session.id], currentMode: v } })); configure('session/set_mode', { modeId: v }) }}>
              {setup.modes.availableModes.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
            </select>
          ) : null}
          {modelOpt?.options?.length ? (
            <select className="mode-select" title="Model" value={modelValue} onChange={(e) => { const v = e.target.value; setTimelines((a) => ({ ...a, [session.id]: { ...a[session.id], config: { ...(a[session.id]?.config ?? {}), [modelOpt.id]: v } } })); configure('session/set_config_option', { configId: modelOpt.id, value: v }) }}>
              {modelOpt.options.map((o) => <option key={o.value} value={o.value}>{o.name}</option>)}
            </select>
          ) : modelValue ? <span className="chip">{modelValue}</span> : null}
          {rootTask?.worktree && <span className="chip mono">{I.branch} {rootTask.worktree.branch}</span>}
          <button className="icon-btn" title="End session on the host" onClick={() => { if (confirm('End this session? The agent process stops and the host record is removed.')) void endSession(session.id) }}>{I.close}</button>
        </div>
        {c && c.kind !== 'connected' && (
          <div className={'banner' + (c.kind === 'failed' ? ' red' : '')}>
            {c.kind === 'reconnecting' ? `Reconnecting (attempt ${c.attempt})… the turn keeps running on the host.` : c.kind === 'failed' ? c.reason : c.kind === 'connecting' ? 'Connecting…' : 'Disconnected'}
          </div>
        )}
        {interrupted && <div className="banner">The host restarted and this session's process ended. Start a new session in the same folder to continue.</div>}
        <Conversation t={tl ?? emptyTimeline()} agent={session.agentId} onAnswer={(id, o) => void answer(id, o)} onReview={() => setView({ kind: 'review' })} />
        <Composer agent={agentName(session.agentId)} busy={Boolean(tl?.busy)} disabled={interrupted ? 'Session interrupted' : c?.kind !== 'connected' ? 'Waiting for the connection…' : undefined} onSend={(t) => void sendPrompt(t)} onCancel={() => void cancelTurn()} />
      </div>
    )
  }

  const showRight = view.kind === 'workspace' && Boolean(session)
  return (
    <div className={'app' + (showRight ? '' : ' no-right')}>
      {sidebar}
      {center}
      {showRight && (
        <RightPanel workspace={session?.workspace} tasks={tasks} rootTask={rootTask} timeline={tl} orchestrator={Boolean(status?.features?.includes('orchestrator'))}
          onOpenTask={(t) => setView({ kind: 'task', taskId: t.id })} onReview={(p) => setView({ kind: 'review', focus: p })} onMerge={setConfirmMerge} />
      )}
      {overlays}
    </div>
  )
}

function NewSession({ agents, workspaces, onClose, onCreate }: { agents: AgentInfo[]; workspaces: Workspace[]; onClose: () => void; onCreate: (a: string, w: string) => Promise<string | undefined> }) {
  const usable = agents.filter((a) => a.enabled !== false)
  const [agent, setAgent] = useState(usable[0]?.id ?? '')
  const [ws, setWs] = useState(workspaces[0]?.path ?? '')
  const [sub, setSub] = useState('')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string>()
  const go = async (): Promise<void> => {
    setBusy(true); setErr(undefined)
    const target = sub.trim() ? `${ws.replace(/\/$/, '')}/${sub.trim().replace(/^\//, '')}` : ws
    const e = await onCreate(agent, target)
    setBusy(false)
    if (e) setErr(e)
  }
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <h3 style={{ margin: 0, fontSize: 18 }}>New session</h3>
        <div className="sub">The agent starts on the host in the folder you pick.</div>
        <div className="field"><label>Agent</label>
          <select className="input" value={agent} onChange={(e) => setAgent(e.target.value)}>{usable.map((a) => <option key={a.id} value={a.id}>{a.name}{a.version ? ` · ${a.version}` : ''}</option>)}</select>
        </div>
        <div className="field"><label>Folder</label>
          <select className="input mono" value={ws} onChange={(e) => setWs(e.target.value)}>{workspaces.map((w) => <option key={w.path} value={w.path}>{tildify(w.path)}</option>)}</select>
        </div>
        <div className="field"><label>Sub-folder (optional)</label><input className="input mono" placeholder="e.g. calc" value={sub} onChange={(e) => setSub(e.target.value)} /></div>
        {err && <div className="error">{err}</div>}
        <div style={{ display: 'flex', gap: 8, marginTop: 20 }}>
          <button className="btn-wide" disabled={!agent || !ws || busy} onClick={() => void go()}>{busy ? 'Starting agent…' : 'Start session'}</button>
          <button className="btn-wide ghost" style={{ width: 120 }} onClick={onClose}>Cancel</button>
        </div>
      </div>
    </div>
  )
}
