import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { AgentInfo, Host, HostStatus, SessionInfo, Task, Workspace } from './types'
import { changedFiles, emptyTimeline, reduce, Timeline } from './acp'
import { Pairing } from './Pairing'
import { Command, Palette } from './Palette'
import { Attachment, Composer, Conversation, Permission } from './Session'
import { GitView } from './Git'
import { DiffReview, RightPanel, mergeError } from './Tasks'
import { McpServer, Section, Settings, Theme, mcpWire } from './Settings'
import { I, baseName, stateDot, stateWord, tildify } from './ui'

type View = { kind: 'git' } | { kind: 'workspace' } | { kind: 'settings'; section: Section } | { kind: 'review'; focus?: string } | { kind: 'task'; taskId: string } | { kind: 'pair' }
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
  const [mcp, setMcpState] = useState<McpServer[]>([])
  const [draft, setDraft] = useState<{ text: string; n: number }>()
  const [newSessionPrompt, setNewSessionPrompt] = useState<string>()
  const pendingSuggest = useRef<{ sid: string; id: number; resolve: (s: string | undefined) => void } | undefined>(undefined)
  const attached = useRef<string | undefined>(undefined)
  const taskAttached = useRef<string | undefined>(undefined)
  const nextId = useRef(Date.now() % 1_000_000)

  const flash = (m: string): void => { setToast(m); setTimeout(() => setToast(undefined), 4000) }

  // theme
  useEffect(() => {
    void window.api.prefs.get().then((r) => { if (r.ok && r.value.theme) setThemeState(r.value.theme as Theme); if (r.ok && r.value.host) setHostId(r.value.host as string); if (r.ok && Array.isArray(r.value.mcp)) setMcpState(r.value.mcp as McpServer[]) })
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
  const setMcp = (m: McpServer[]): void => { setMcpState(m); savePrefs({ mcp: m }) }

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
      setTimelines((all) => {
        const before = all[sid]
        const next = reduce(before ?? emptyTimeline(), env as Record<string, unknown>)
        if (next.title && next.title !== before?.title) localStorage.setItem(TITLES_KEY, JSON.stringify({ ...titles(), [sid]: next.title }))
        const ps = pendingSuggest.current
        if (ps && ps.sid === sid && before?.busy && !next.busy) {
          pendingSuggest.current = undefined
          const last = [...next.items].reverse().find((i) => i.kind === 'agent')
          ps.resolve(last && last.kind === 'agent' ? last.text.trim().replace(/^```\w*\n?|```$/g, '').trim() : undefined)
        }
        return { ...all, [sid]: next }
      })
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

  const histKey = (ws?: string): string => `acportal.history.${hostId}.${ws ?? ''}`
  const history = (ws?: string): string[] => JSON.parse(localStorage.getItem(histKey(ws)) ?? '[]')
  async function sendPrompt(text: string, atts: Attachment[] = [], silent = false): Promise<number | undefined> {
    if (!session || !attached.current) return
    const id = ++nextId.current
    if (text.trim() && !silent) localStorage.setItem(histKey(session.workspace), JSON.stringify([text, ...history(session.workspace).filter((h) => h !== text)].slice(0, 50)))
    const blocks: unknown[] = [{ type: 'text', text }]
    for (const a of atts) blocks.push(a.data ? { type: 'image', mimeType: a.mimeType, data: a.data } : { type: 'resource', resource: { uri: 'file://' + a.path, mimeType: a.mimeType, text: a.text } })
    if (!titles()[session.id]) localStorage.setItem(TITLES_KEY, JSON.stringify({ ...titles(), [session.id]: text.split('\n')[0].slice(0, 60) }))
    setTimelines((all) => ({ ...all, [session.id]: { ...(all[session.id] ?? emptyTimeline()), promptId: id, busy: true } }))
    const r = await window.api.session.send(attached.current, { jsonrpc: '2.0', id, method: 'session/prompt', params: { sessionId: session.acpSessionId, prompt: blocks } })
    if (!r.ok) { flash(r.error); setTimelines((all) => ({ ...all, [session.id]: { ...all[session.id], busy: false } })); return }
    return id
  }
  async function elicit(id: string | number, result: unknown, ref = attached, sid = selected): Promise<void> {
    if (!ref.current || !sid) return
    const r = await window.api.session.send(ref.current, { jsonrpc: '2.0', id, result })
    if (!r.ok) return flash(r.error)
    setTimelines((all) => ({ ...all, [sid]: { ...all[sid], items: all[sid].items.map((i) => (i.kind === 'elicitation' && i.id === id ? { ...i, answered: (result as { action: string }).action } : i)) } }))
  }
  async function suggestCommit(diff: string): Promise<string | undefined> {
    if (!session || !attached.current || tl?.busy) { flash('Open an idle session to ask its agent'); return undefined }
    return new Promise((resolve) => {
      pendingSuggest.current = { sid: session.id, id: 0, resolve }
      void sendPrompt(`Write a git commit message for this staged diff. Conventional style, imperative summary line of at most 72 characters, then an optional short body. Reply with only the message, no tools, no code fences.\n\n${diff}`, [], true)
      setTimeout(() => { if (pendingSuggest.current?.resolve === resolve) { pendingSuggest.current = undefined; resolve(undefined) } }, 120_000)
    })
  }
  async function cancelTurn(): Promise<void> {
    if (!session || !attached.current) return
    await window.api.session.send(attached.current, { jsonrpc: '2.0', method: 'session/cancel', params: { sessionId: session.acpSessionId } })
  }
  async function answer(id: string | number, optionId: string | null, ref = attached, sid = selected): Promise<void> {
    if (!ref.current || !sid) return
    const outcome = optionId ? { outcome: 'selected', optionId } : { outcome: 'cancelled' }
    const r = await window.api.session.send(ref.current, { jsonrpc: '2.0', id, result: { outcome } })
    if (!r.ok) return flash(r.error)
    setTimelines((all) => {
      const t = all[sid]
      return { ...all, [sid]: { ...t, items: t.items.map((i) => (i.kind === 'permission' && i.id === id ? { ...i, answered: optionId ?? 'cancelled' } : i)) } }
    })
    if (ref === taskAttached) setTimeout(() => void refresh(), 800)
  }
  async function createSession(agentId: string, workspace: string, opts: NewOpts = defaultOpts): Promise<string | undefined> {
    let target = workspace
    if (opts.worktree) {
      const branch = 'acportal/' + (opts.branch || `session-${Date.now().toString(36)}`)
      const top = await window.api.git.run(hostId!, workspace, 'rev-parse')
      if (!top.ok) return `Own worktree needs a git repository: ${top.error}`
      const root = top.value.out.split('\n')[0]
      const path = root.replace(/[\\/][^\\/]+$/, '') + '/' + baseName(root) + '-' + branch.split('/').pop()
      const w = await window.api.git.run(hostId!, root, 'worktree-add', { branch, path })
      if (!w.ok) return `Could not create the worktree: ${w.error}`
      target = path + workspace.slice(root.length)
    }
    const access = { readFiles: opts.read, writeFiles: opts.write, terminal: opts.terminal, ...(status?.features?.includes('formElicitation') ? { formElicitation: true } : {}) }
    const r = await api<SessionInfo>('POST', '/v1/sessions', { agentId, workspace: target, loadSessionId: null, workspaceAccess: access, mcpServers: mcp.filter((m) => m.enabled && opts.mcp).map(mcpWire) })
    if (!r.ok) return r.code === 'unsupported_mcp_transport' ? 'This agent does not support HTTP/SSE MCP servers; turn those off in Settings → MCP servers' : r.error
    if (opts.firstPrompt) setTimeout(() => setDraft({ text: opts.firstPrompt!, n: Date.now() }), 300)
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
    if (viewTask?.session && viewTask.session !== selected) void attach(viewTask.session, taskAttached, ['completed', 'failed', 'canceled', 'rejected'].includes(viewTask.state))
    if (!viewTask && taskAttached.current) { void window.api.session.detach(taskAttached.current); taskAttached.current = undefined }
  }, [viewTask?.session])

  const commands: Command[] = useMemo(() => {
    const c: Command[] = [
      { id: 'new', group: 'Actions', label: 'New session…', hint: 'Ctrl N', run: () => setNewSession(true) },
      { id: 'git', group: 'Actions', label: 'Open Git (changes, history, worktrees, stash)', hint: 'Ctrl G', run: () => setView({ kind: 'git' }) },
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
  const paletteNew = (q: string): void => { setNewSessionPrompt(q); setNewSession(true) }

  useEffect(() => {
    const k = (e: KeyboardEvent): void => {
      const mod = e.metaKey || e.ctrlKey
      if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); setPalette((p) => !p) }
      else if (mod && e.key.toLowerCase() === 'n') { e.preventDefault(); setNewSession(true) }
      else if (mod && e.key === ',') { e.preventDefault(); setView({ kind: 'settings', section: 'General' }) }
      else if (mod && e.key.toLowerCase() === 'd') { e.preventDefault(); setView({ kind: 'review' }) }
      else if (mod && e.key.toLowerCase() === 'g') { e.preventDefault(); setView({ kind: 'git' }) }
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
      {palette && <Palette commands={commands} onClose={() => setPalette(false)} onNew={paletteNew} />}
      {newSession && <NewSession agents={agents} workspaces={workspaces} mcp={mcp} local={Boolean(host && /^http:\/\/(127\.0\.0\.1|localhost|\[::1\])/.test(host.address))} firstPrompt={newSessionPrompt} onClose={() => { setNewSession(false); setNewSessionPrompt(undefined) }} onCreate={async (a, w, o) => { const e = await createSession(a, w, o); if (!e) setNewSessionPrompt(undefined); return e }} />}
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
          onRemoveWorkspace={async (p) => { await api('DELETE', '/v1/workspaces', { path: p }); void refresh() }}
          api={api} onAgentsChanged={() => void refresh()} mcp={mcp} setMcp={setMcp} />
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
          <button className="icon-btn" onClick={() => setView({ kind: 'git' })} title="Git (Ctrl G)">{I.branch}</button>
          <button className="icon-btn" onClick={() => setPalette(true)} title="Command palette (Ctrl K)">⌘</button>
          <button className="icon-btn" onClick={() => setView({ kind: 'settings', section: 'General' })} title="Settings">{I.gear}</button>
        </div>
      </div>
    </aside>
  )

  let center
  if (view.kind === 'git' && hostId) {
    const repos = [
      ...(session ? [{ path: session.workspace, label: baseName(session.workspace) + ' · session' }] : []),
      ...workspaces.filter((w) => w.path !== session?.workspace).map((w) => ({ path: w.path, label: baseName(w.path) })),
      ...tasks.filter((t) => t.worktree?.path && t.worktree.outcome !== 'merged').map((t) => ({ path: t.worktree!.path!, label: t.worktree!.branch + ' · task' }))
    ]
    center = <GitView hostId={hostId} repos={repos} onBack={() => setView({ kind: 'workspace' })} suggest={suggestCommit}
      askAgent={(p) => { setView({ kind: 'workspace' }); setDraft({ text: p, n: Date.now() }) }}
      openSession={(path) => { setNewSessionPrompt(undefined); setNewSession(true); setTimeout(() => flash(`Pick the folder containing ${tildify(path)}; worktrees inside host roots are listed`), 100) }} />
  } else if (view.kind === 'review') {
    center = <DiffReview title={session ? `Review: ${title(session)}` : 'Review changes'} subtitle={session ? `${session.agentId} · ${tildify(session.workspace)}` : ''} diffs={tl ? changedFiles(tl).map((f) => f.diff) : []} focus={view.focus} root={session?.workspace} onBack={() => setView({ kind: 'workspace' })} task={rootTask?.worktree ? rootTask : undefined} onMerge={setConfirmMerge} merging={merging} />
  } else if (view.kind === 'task' && viewTask) {
    const ttl = viewTask.session ? timelines[viewTask.session] : undefined
    const waiting = viewTask.needsDecision?.length ?? viewTask.pendingDecisions ?? 0
    center = (
      <div className="main">
        <DiffReview title={viewTask.worktree?.branch ?? `Task ${viewTask.id.slice(0, 8)}`} subtitle={`${viewTask.agent} · ${stateWord(viewTask.state)}${viewTask.summary ? ` (${viewTask.summary})` : ''} · ${viewTask.worktree ? 'own worktree of ' + tildify((viewTask.worktree as { repository?: string }).repository ?? viewTask.scope) : tildify(viewTask.scope)}${waiting ? ` · ${waiting} waiting for a decision` : ''}${viewTask.costUsd ? ` · US$${viewTask.costUsd.toFixed(4)}` : ''}`}
          root={viewTask.worktree?.path ?? viewTask.scope} diffs={ttl ? changedFiles(ttl).map((f) => f.diff) : []} onBack={() => setView({ kind: 'workspace' })} task={viewTask} onMerge={setConfirmMerge} merging={merging} />
        {ttl && ttl.items.some((i) => (i.kind === 'permission' || i.kind === 'elicitation') && i.answered === undefined) && (
          <div className="decisions">
            <div className="section-label" style={{ padding: '0 0 8px' }}>Waiting for your decision · your answer wins over the supervisor's</div>
            {ttl.items.filter((i) => i.kind === 'permission' && i.answered === undefined).map((i) => i.kind === 'permission' && <Permission key={String(i.id)} p={i} onAnswer={(o) => void answer(i.id, o, taskAttached, viewTask.session!)} />)}
          </div>
        )}
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
    const pc = ((session.initialization?.agentCapabilities as Record<string, unknown> | undefined)?.promptCapabilities ?? {}) as { image?: boolean; embeddedContext?: boolean }
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
          <button className="icon-btn" title="Git: changes, history, worktrees (Ctrl G)" onClick={() => setView({ kind: 'git' })}>{I.branch}</button>
          <button className="icon-btn" title="End session on the host" onClick={() => { if (confirm('End this session? The agent process stops and the host record is removed.')) void endSession(session.id) }}>{I.close}</button>
        </div>
        {c && c.kind !== 'connected' && (
          <div className={'banner' + (c.kind === 'failed' ? ' red' : '')}>
            {c.kind === 'reconnecting' ? `Reconnecting (attempt ${c.attempt})… the turn keeps running on the host.` : c.kind === 'failed' ? c.reason : c.kind === 'connecting' ? 'Connecting…' : 'Disconnected'}
          </div>
        )}
        {interrupted && <div className="banner">The host restarted and this session's process ended. Start a new session in the same folder to continue.</div>}
        <Conversation t={tl ?? emptyTimeline()} agent={session.agentId} onAnswer={(id, o) => void answer(id, o)} onReview={() => setView({ kind: 'review' })} onElicit={(id, r) => void elicit(id, r)} />
        <Composer agent={agentName(session.agentId)} busy={Boolean(tl?.busy)} disabled={interrupted ? 'Session interrupted' : c?.kind !== 'connected' ? 'Waiting for the connection…' : undefined} onSend={(t, a) => void sendPrompt(t, a)} onCancel={() => void cancelTurn()}
          history={history(session.workspace)} canImage={Boolean(pc?.image)} canResource={Boolean(pc?.embeddedContext)} draft={draft} />
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

type NewOpts = { read: boolean; write: boolean; terminal: boolean; worktree: boolean; branch?: string; mcp: boolean; firstPrompt?: string }
const defaultOpts: NewOpts = { read: true, write: true, terminal: true, worktree: false, mcp: true }
function NewSession({ agents, workspaces, mcp, local, firstPrompt, onClose, onCreate }: { agents: AgentInfo[]; workspaces: Workspace[]; mcp: McpServer[]; local: boolean; firstPrompt?: string; onClose: () => void; onCreate: (a: string, w: string, o: NewOpts) => Promise<string | undefined> }) {
  const usable = agents.filter((a) => a.enabled !== false)
  const [agent, setAgent] = useState(localStorage.getItem('acportal.lastAgent') ?? usable[0]?.id ?? '')
  const [ws, setWs] = useState(workspaces[0]?.path ?? '')
  const [sub, setSub] = useState('')
  const [o, setO] = useState<NewOpts>({ ...defaultOpts, firstPrompt, branch: firstPrompt ? firstPrompt.toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 32).replace(/-$/, '') : '' })
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string>()
  const go = async (): Promise<void> => {
    setBusy(true); setErr(undefined)
    localStorage.setItem('acportal.lastAgent', agent)
    const target = sub.trim() ? `${ws.replace(/\/$/, '')}/${sub.trim().replace(/^\//, '')}` : ws
    const e = await onCreate(agent, target, o)
    setBusy(false)
    if (e) setErr(e)
  }
  const Check = ({ k, label, title, disabled }: { k: 'read' | 'write' | 'terminal' | 'worktree' | 'mcp'; label: string; title?: string; disabled?: boolean }) => (
    <button className={'check' + (o[k] ? ' on' : '')} title={title} disabled={disabled} onClick={() => setO({ ...o, [k]: !o[k] })}>{o[k] ? '✓' : '○'} {label}</button>
  )
  const enabledMcp = mcp.filter((m) => m.enabled)
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="modal" style={{ width: 600 }} onMouseDown={(e) => e.stopPropagation()}>
        <h3 style={{ margin: 0, fontSize: 18 }}>New session</h3>
        <div className="sub">{firstPrompt ? <>Task: <span style={{ color: 'var(--text)' }}>{firstPrompt}</span></> : 'The agent starts on the host in the folder you pick.'}</div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
          <div className="field"><label>Agent</label>
            <select className="input" value={agent} onChange={(e) => setAgent(e.target.value)}>{usable.map((a) => <option key={a.id} value={a.id}>{a.name}{a.version ? ` · ${a.version}` : ''}</option>)}</select>
          </div>
          <div className="field"><label>Folder</label>
            <select className="input mono" value={ws} onChange={(e) => setWs(e.target.value)}>{workspaces.map((w) => <option key={w.path} value={w.path}>{tildify(w.path)}</option>)}</select>
          </div>
        </div>
        <div className="field"><label>Sub-folder (optional)</label><input className="input mono" placeholder="e.g. calc" value={sub} onChange={(e) => setSub(e.target.value)} /></div>
        <div className="field"><label>Rights for this session</label>
          <div className="checks"><Check k="read" label="Read files" /><Check k="write" label="Write files" /><Check k="terminal" label="Run commands" /></div>
        </div>
        <div className="field"><label>Isolation</label>
          <div className="checks">
            <button className={'check' + (!o.worktree ? ' on' : '')} onClick={() => setO({ ...o, worktree: false })}>{!o.worktree ? '✓' : '○'} Shared folder</button>
            <Check k="worktree" label="Own worktree" disabled={!local} title={local ? 'git worktree add -b acportal/<name> next to the repository' : 'Needs a host on this computer: acpd has no worktree option for phone sessions yet'} />
            {o.worktree && <input className="input mono" style={{ height: 32, flex: 1 }} placeholder="branch name" value={o.branch} onChange={(e) => setO({ ...o, branch: e.target.value.replace(/[^\w./-]/g, '-') })} />}
          </div>
          {o.worktree && <div className="sub" style={{ marginTop: 4 }}>$ git worktree add -b acportal/{o.branch || '<name>'} ../{'<repo>'}-{o.branch || '<name>'}</div>}
        </div>
        <div className="field"><label>Tools</label>
          <div className="checks"><Check k="mcp" label={`MCP servers (${enabledMcp.length})`} title={enabledMcp.map((m) => m.name).join(', ') || 'Add servers in Settings → MCP servers'} disabled={!enabledMcp.length} />
            <span className="sub" title="Delegation is enabled per agent in the host config ([orchestrator] orchestrate)">Delegation: set on the host</span></div>
        </div>
        {err && <div className="error">{err}</div>}
        <div style={{ display: 'flex', gap: 8, marginTop: 20 }}>
          <button className="btn-wide" disabled={!agent || !ws || busy || (!o.read && !o.write && !o.terminal && false)} onClick={() => void go()}>{busy ? 'Starting agent…' : 'Start session'}</button>
          <button className="btn-wide ghost" style={{ width: 120 }} onClick={onClose}>Cancel</button>
        </div>
      </div>
    </div>
  )
}
