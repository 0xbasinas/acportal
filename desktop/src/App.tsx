import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { AgentInfo, Host, HostStatus, SessionInfo, Task, Workspace } from './types'
import { changedFiles, emptyTimeline, reduce, Timeline } from './acp'
import { Pairing } from './Pairing'
import { Command, Palette } from './Palette'
import { Attachment, Composer, Conversation, Permission } from './Session'
import { GitView } from './Git'
import { DiffReview, RightPanel, mergeError } from './Tasks'
import { McpServer, Section, Settings, Theme, mcpWire } from './Settings'
import { Empty, I, baseName, stateDot, stateWord, tildify } from './ui'
import { parseBranches } from './gitparse'
import { STATE_DOT, STATE_LABEL, useFleet } from './fleet'
import { HostSwitcher, HostsSettings } from './Hosts'

type View = { kind: 'git' } | { kind: 'workspace' } | { kind: 'settings'; section: Section } | { kind: 'review'; focus?: string } | { kind: 'task'; taskId: string } | { kind: 'pair'; repair?: string }
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
  const [branchOf, setBranchOf] = useState<Record<string, string>>({})
  const [taskDetail, setTaskDetail] = useState<Task>()
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
  const { fleet, reconnect } = useFleet(hosts)
  const [hostFilter, setHostFilter] = useState<string>('all')
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
  const pendingSel = useRef<string | undefined>(undefined)
  // Open a session on any host: switch the active host first, then select it once its list loads.
  const openOn = (hid: string, sid?: string): void => {
    if (hid === hostId) { if (sid) setSelected(sid); setView({ kind: 'workspace' }); return }
    pendingSel.current = sid
    setHostId(hid); setView({ kind: 'workspace' })
  }
  const [storage, setStorage] = useState<Record<string, string>>({})
  useEffect(() => { void Promise.all((hosts ?? []).map(async (h) => [h.id, ((await window.api.storageOf(h.id)) as { ok: boolean; value?: string }).value ?? ''] as const)).then((e) => setStorage(Object.fromEntries(e))) }, [hosts])
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
    setSessions([]); setTasks([]); setAgents([]); setWorkspaces([]); setSelected(pendingSel.current); pendingSel.current = undefined; setTimelines({})
    void refresh()
    const t = setInterval(() => void refresh(), 4000)
    return () => clearInterval(t)
  }, [refresh])

  // current branch of every session folder (host git), for the sidebar and headers
  const wsKey = [...new Set(sessions.map((x) => x.workspace))].sort().join('|')
  useEffect(() => {
    if (!hostId || !online || !wsKey) return
    let stop = false
    const load = async (): Promise<void> => {
      const next: Record<string, string> = {}
      for (const ws of wsKey.split('|')) {
        const r = await window.api.git.run(hostId, ws, 'rev-parse')
        if (r.ok) next[ws] = r.value.out.split('\n')[2] ?? ''
      }
      if (!stop) setBranchOf(next)
    }
    void load()
    const t = setInterval(() => void load(), 15000)
    return () => { stop = true; clearInterval(t) }
  }, [hostId, online, wsKey, view.kind])

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
    const branch = opts.branch?.trim()
    let branchOpt: { name: string; base?: string; create: boolean; worktree: boolean } | undefined
    if (branch) {
      if (status?.features?.includes('sessionWorktree')) branchOpt = { name: branch, create: opts.newBranch, worktree: opts.worktree, ...(opts.newBranch && opts.base ? { base: opts.base } : {}) }
      else if (opts.worktree) {
        // Older host on this computer: make the worktree locally, then start in it.
        const top = await window.api.git.run(hostId!, workspace, 'rev-parse')
        if (!top.ok) return `Own worktree needs a git repository: ${top.error}`
        const root = top.value.out.split('\n')[0]
        const path = root.replace(/[\\/][^\\/]+$/, '') + '/' + baseName(root) + '-' + branch.replace(/[^\w-]/g, '-')
        const w = await window.api.git.run(hostId!, root, 'worktree-add', { branch, path, ...(opts.base ? { base: opts.base } : {}) })
        if (!w.ok) return `Could not create the worktree: ${w.error}`
        target = path + workspace.slice(root.length)
      } else {
        const w = await window.api.git.run(hostId!, workspace, opts.newBranch ? 'branch-create' : 'checkout', { branch, ...(opts.newBranch && opts.base ? { base: opts.base } : {}) })
        if (!w.ok) return `Could not switch branch: ${w.error}`
      }
    }
    const access = { readFiles: opts.read, writeFiles: opts.write, terminal: opts.terminal, ...(status?.features?.includes('formElicitation') ? { formElicitation: true } : {}) }
    const r = await api<SessionInfo>('POST', '/v1/sessions', { agentId, workspace: target, loadSessionId: null, workspaceAccess: access, mcpServers: mcp.filter((m) => m.enabled && opts.mcp).map(mcpWire), ...(branchOpt ? { branch: branchOpt } : {}) })
    if (!r.ok) return ({ unsupported_mcp_transport: 'This agent does not support HTTP/SSE MCP servers; turn those off in Settings → MCP servers', branch_failed: 'Git could not make or switch to that branch (uncommitted changes in the way, or the branch is checked out elsewhere).', worktree_path_unavailable: 'The worktree folder already exists or is outside the host folders.', invalid_branch_name: 'That is not a valid branch name.', not_a_repository: 'This folder is not a git repository.' } as Record<string, string>)[r.code ?? ''] ?? r.error
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

  useEffect(() => {
    if (!viewTask) { setTaskDetail(undefined); return }
    let stop = false
    const load = async (): Promise<void> => { const r = await api<Task>('GET', `/v1/tasks/${viewTask.id}`); if (!stop && r.ok) setTaskDetail(r.value) }
    void load()
    const t = setInterval(() => void load(), 4000)
    return () => { stop = true; clearInterval(t) }
  }, [viewTask?.id, viewTask?.state])

  const commands: Command[] = useMemo(() => {
    const c: Command[] = [
      { id: 'new', group: 'Actions', label: 'New session…', hint: 'Ctrl N', run: () => setNewSession(true) },
      { id: 'git', group: 'Actions', label: 'Open Git (changes, history, worktrees, stash)', hint: 'Ctrl G', run: () => setView({ kind: 'git' }) },
      { id: 'review', group: 'Actions', label: 'Review changes in this session', hint: 'Ctrl D', run: () => setView({ kind: 'review' }) },
      { id: 'stop', group: 'Actions', label: 'Stop the current turn', hint: 'Esc', run: () => void cancelTurn() },
      { id: 'settings', group: 'Actions', label: 'Open settings', hint: 'Ctrl ,', run: () => setView({ kind: 'settings', section: 'General' }) },
      { id: 'pair', group: 'Hosts', label: 'Add host…', run: () => setView({ kind: 'pair' }) },
      { id: 'reconnect', group: 'Hosts', label: 'Reconnect all hosts', run: () => { reconnect(); void refresh() } },
      { id: 'hosts', group: 'Hosts', label: 'Manage hosts', run: () => setView({ kind: 'settings', section: 'Hosts' }) },
      { id: 'theme', group: 'Appearance', label: `Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`, run: () => setTheme(theme === 'dark' ? 'light' : 'dark') },
      { id: 'theme-sys', group: 'Appearance', label: 'Use system theme', run: () => setTheme('system') }
    ]
    for (const s of sessions) c.push({ id: 's' + s.id, group: 'Sessions', label: title(s), run: () => { setSelected(s.id); setView({ kind: 'workspace' }) } })
    for (const t of tasks) c.push({ id: 't' + t.id, group: 'Tasks', label: `${t.worktree?.branch ?? 'task ' + t.id.slice(0, 6)} · ${t.agent} · ${stateWord(t.state)}`, run: () => setView({ kind: 'task', taskId: t.id }) })
    for (const h of hosts ?? []) if (h.id !== hostId) c.push({ id: 'h' + h.id, group: 'Hosts', label: `Switch host: ${h.name} · ${STATE_LABEL[fleet[h.id]?.state ?? 'connecting']}`, run: () => openOn(h.id) })
    for (const h of hosts ?? []) if (h.id !== hostId) for (const s of fleet[h.id]?.sessions ?? []) c.push({ id: 'o' + s.id, group: `Sessions on ${h.name}`, label: title(s), run: () => openOn(h.id, s.id) })
    return c
  }, [sessions, tasks, hosts, hostId, theme, timelines, fleet])
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
    return <Pairing replaceId={view.kind === 'pair' ? view.repair : undefined} key={view.kind === 'pair' ? view.repair ?? 'new' : 'first'} initialAddress={view.kind === 'pair' && view.repair ? hosts.find((h) => h.id === view.repair)?.address : undefined} repairName={view.kind === 'pair' && view.repair ? hosts.find((h) => h.id === view.repair)?.name : undefined}
      onPaired={(h) => { void loadHosts().then(() => { setHostId(h.id); reconnect(h.id) }); setView({ kind: 'workspace' }); flash(`Paired with ${h.name}`) }} onCancel={hosts.length ? () => setView({ kind: 'workspace' }) : undefined} />

  const overlays = (
    <>
      {palette && <Palette commands={commands} onClose={() => setPalette(false)} onNew={paletteNew} />}
      {newSession && hostId && <NewSession key={hostId} hostId={hostId} hostPick={hosts.length > 1 ? hosts.map((h) => ({ id: h.id, label: `${h.name} · ${STATE_LABEL[fleet[h.id]?.state ?? 'connecting']}`, ok: fleet[h.id]?.state === 'connected' })) : undefined} onHost={(id) => openOn(id)} agents={agents} workspaces={workspaces} mcp={mcp} hostBranches={Boolean(status?.features?.includes('git'))} local={Boolean(host && /^http:\/\/(127\.0\.0\.1|localhost|\[::1\])/.test(host.address))} firstPrompt={newSessionPrompt} onClose={() => { setNewSession(false); setNewSessionPrompt(undefined) }} onCreate={async (a, w, o) => { const e = await createSession(a, w, o); if (!e) setNewSessionPrompt(undefined); return e }} />}
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
          api={api} onAgentsChanged={() => void refresh()} mcp={mcp} setMcp={setMcp}
          hostsPanel={<HostsSettings hosts={hosts} current={host} fleet={fleet} storage={storage} onSwitch={(h) => openOn(h.id)} onAdd={() => setView({ kind: 'pair' })}
            onForget={async (h) => { if (!confirm(`Unpair ${h.name}? This device's token is revoked on the host and removed here.`)) return; await window.api.hosts.forget(h.id, true); await loadHosts() }}
            onRepair={(h) => setView({ kind: 'pair', repair: h.id })} onReconnect={(id) => { reconnect(id); if (!id || id === hostId) void refresh() }} onChanged={() => void loadHosts()} />} />
        {overlays}
      </>
    )

  const projects = workspaces
  const sidebar = (
    <aside className="side">
      <div className="host-head">
        <HostSwitcher hosts={hosts} current={host} fleet={fleet} onSwitch={(h) => openOn(h.id)} onReconnect={(id) => { reconnect(id); if (!id || id === hostId) void refresh() }}
          onRepair={(h) => setView({ kind: 'pair', repair: h.id })} onSettings={() => setView({ kind: 'settings', section: 'Hosts' })} onAdd={() => setView({ kind: 'pair' })} />
      </div>
      <div className="side-actions">
        <button className="btn full new-btn" onClick={() => setNewSession(true)} disabled={!hosts.some((h) => fleet[h.id]?.state === 'connected') && !online}>{I.plus}New session<span className="kbd">Ctrl N</span></button>
        <button className="side-search" onClick={() => setPalette(true)}>{I.search}<span className="grow">Search or run a command</span><span className="kbd">Ctrl K</span></button>
      </div>
      {hosts.length > 1 && (
        <div className="host-filter" role="tablist" aria-label="Filter by host">
          <button className={'hf' + (hostFilter === 'all' ? ' on' : '')} onClick={() => setHostFilter('all')}>All</button>
          {hosts.map((h) => <button key={h.id} className={'hf' + (hostFilter === h.id ? ' on' : '')} onClick={() => setHostFilter(h.id)} title={`${h.name}: ${STATE_LABEL[fleet[h.id]?.state ?? 'connecting']}`}><span className={'dot ' + STATE_DOT[fleet[h.id]?.state ?? 'connecting']} />{h.name}</button>)}
        </div>
      )}
      <div className="list">
        {hosts.filter((h) => hostFilter === 'all' || h.id === hostFilter).map((h) => {
          const live = h.id === hostId
          const f = fleet[h.id]
          const st = f?.state ?? 'connecting'
          const hs = live ? sessions : f?.sessions ?? []
          const ws = live ? projects : f?.workspaces ?? []
          const multi = hosts.length > 1
          const shut = multi && collapsed[h.id]
          return (
            <div key={h.id} className={'host-group' + (live ? ' live' : '')}>
              {multi && (
                <div className="group-head" onClick={() => setCollapsed({ ...collapsed, [h.id]: !shut })} title={`${h.name} · ${STATE_LABEL[st]}`}>
                  <span className={'chev-ic' + (shut ? ' shut' : '')}>{I.down}</span>
                  <span className={'dot ' + STATE_DOT[st]} />
                  <span className="grow ell">{h.name}</span>
                  {st !== 'connected' ? <span className={'group-state s-' + st}>{STATE_LABEL[st]}</span> : <span className="count">{hs.length || ''}</span>}
                </div>
              )}
              {!shut && (
                <>
                  {(st === 'offline' || st === 'repair') && !live && <div className="hint" style={{ padding: '2px 12px 6px' }}>{st === 'repair' ? <a onClick={() => setView({ kind: 'pair', repair: h.id })}>Pair again</a> : 'Sessions keep running on the host; they show again when it is back.'}</div>}
                  <div className="section-label">Projects</div>
                  {ws.length === 0 && <div className="hint" style={{ padding: '0 10px' }}>{st === 'connected' ? 'No folders are allowed on this host yet.' : '—'}</div>}
                  {ws.slice(0, 6).map((w) => (
                    <div className="row compact" key={w.path} onClick={() => { if (!live) openOn(h.id); setNewSession(true) }} title={`New session in ${w.path} on ${h.name}`}>
                      <span className="row-ic">{I.folder}</span><span className="grow ell">{w.name}</span>
                      <span className="count">{hs.filter((s) => s.workspace.startsWith(w.path)).length || ''}</span>
                    </div>
                  ))}
                  <div className="section-label">Sessions</div>
                  {hs.length === 0 && <div className="hint" style={{ padding: '0 10px' }}>No sessions on this host.</div>}
                  {[...hs].reverse().map((s) => {
                    const t = live ? timelines[s.id] : undefined
                    const waiting = live ? pendingIn(s.id) : 0
                    const sst = waiting ? 'waiting' : t?.busy ? 'running' : s.status
                    const br = live ? branchOf[s.workspace] : undefined
                    return (
                      <div key={s.id} className={'row session' + (live && s.id === selected ? ' active' : '')} onClick={() => openOn(h.id, s.id)}>
                        <span className={'dot ' + stateDot(sst) + (sst === 'running' ? ' live' : '')} />
                        <div className="grow">
                          <div className="title">{title(s)}</div>
                          <div className={'sub row-meta' + (waiting ? ' amber' : s.status === 'interrupted' ? ' red' : '')}>
                            <span>{waiting ? `${waiting} waiting` : stateWord(sst)}</span>
                            {br ? <span className="mono ell" title={`On ${br}`}>{I.branch}{br}</span> : <span className="mono ell" title={s.workspace}>{baseName(s.workspace)}</span>}
                          </div>
                        </div>
                        {waiting > 0 && <span className="badge">{waiting}</span>}
                      </div>
                    )
                  })}
                </>
              )}
            </div>
          )
        })}
      </div>
      <div className="side-foot">
        <button className="foot-btn" onClick={() => setView({ kind: 'git' })} title="Git (Ctrl G)">{I.branch}Git</button>
        <button className="foot-btn" onClick={() => setView({ kind: 'settings', section: 'General' })} title="Settings (Ctrl ,)">{I.gear}Settings</button>
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
    center = <DiffReview title={session ? `Review · ${title(session)}` : 'Review changes'} subtitle={session ? `${agentName(session.agentId)} · ${tildify(session.workspace)}` : ''} diffs={tl ? changedFiles(tl).map((f) => f.diff) : []} focus={view.focus} root={session?.workspace} onBack={() => setView({ kind: 'workspace' })} task={rootTask?.worktree ? rootTask : undefined} onMerge={setConfirmMerge} merging={merging} />
  } else if (view.kind === 'task' && viewTask) {
    const ttl = viewTask.session ? timelines[viewTask.session] : undefined
    const detail = taskDetail?.id === viewTask.id ? taskDetail : undefined
    const waiting = detail?.needsDecision?.length ?? viewTask.pendingDecisions ?? 0
    // The host's view of the worktree (shell-made edits included) wins over the agent's reports.
    const hostDiffs = detail?.worktreeChanges?.map((c) => ({ path: c.path, oldText: c.status === 'A' ? null : '', newText: c.newText ?? '', ...(c.diff !== undefined ? { unified: c.diff } : {}) }))
    const diffs = hostDiffs?.length ? hostDiffs : ttl ? changedFiles(ttl).map((f) => f.diff) : []
    center = (
      <div className="main">
        <DiffReview title={viewTask.worktree?.branch ?? `Task ${viewTask.id.slice(0, 8)}`} subtitle={`${viewTask.agent} · ${stateWord(viewTask.state)}${viewTask.summary ? ` (${viewTask.summary})` : ''} · ${viewTask.worktree ? 'own worktree of ' + tildify((viewTask.worktree as { repository?: string }).repository ?? viewTask.scope) : tildify(viewTask.scope)}${waiting ? ` · ${waiting} waiting for a decision` : ''}${viewTask.costUsd ? ` · US$${viewTask.costUsd.toFixed(4)}` : ''}`}
          root={viewTask.worktree?.path ?? viewTask.scope} diffs={diffs} onBack={() => setView({ kind: 'workspace' })} task={viewTask} onMerge={setConfirmMerge} merging={merging} />
        {ttl && ttl.items.some((i) => (i.kind === 'permission' || i.kind === 'elicitation') && i.answered === undefined) && (
          <div className="decisions">
            <div className="group-h" style={{ marginTop: 0 }}><span>Waiting for your decision</span><span className="sub">your answer wins over the supervisor's</span></div>
            {ttl.items.filter((i) => i.kind === 'permission' && i.answered === undefined).map((i) => i.kind === 'permission' && <Permission key={String(i.id)} p={i} onAnswer={(o) => void answer(i.id, o, taskAttached, viewTask.session!)} />)}
          </div>
        )}
        {!['completed', 'failed', 'canceled', 'rejected'].includes(viewTask.state) && (
          <div className="action-bar">
            {viewTask.session && <button className="btn" onClick={() => { setSelected(viewTask.session!); setView({ kind: 'workspace' }) }}>Open its session</button>}
            <button className="btn danger" onClick={() => void cancelTask(viewTask)}>Cancel task</button>
          </div>
        )}
      </div>
    )
  } else if (!session) {
    const repair = !online && hostError && /[Pp]air again/.test(hostError)
    center = (
      <div className="main">
        {hostError && <div className="banner red">{I.alert}<span>{hostError}</span></div>}
        <Empty icon={online ? I.chat : I.server} title={online ? 'Pick a session or start one' : repair ? 'Pair this host again' : 'Host offline'}
          actions={<>
            {repair ? <button className="btn primary" onClick={() => setView({ kind: 'pair', repair: hostId })}>Pair again</button> : <button className="btn primary" onClick={() => setNewSession(true)} disabled={!online}>{I.plus}New session</button>}
            <button className="btn" onClick={() => setPalette(true)}>{I.cmd}Commands <span className="kbd">Ctrl K</span></button>
          </>}>
          {online ? `${sessions.length} session${sessions.length === 1 ? '' : 's'} on ${host?.name ?? 'this host'} · ${agents.filter((a) => a.enabled !== false).length} agents ready` : repair ? 'Run acpd pair on the host and enter the new code.' : 'Waiting for the host to answer. Check that acpd is running and reachable.'}
        </Empty>
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
    const br = rootTask?.worktree?.branch ?? branchOf[session.workspace]
    center = (
      <div className="main">
        <div className="topbar">
          <div className="topbar-title">
            <h1>{title(session)}</h1>
            <div className="sub crumbs"><span>{agentName(session.agentId)}</span><span className={tl?.busy ? 'live-word' : ''}>{tl?.busy ? 'Running' : stateWord(session.status)}</span><span className="mono ell" title={session.workspace}>{tildify(session.workspace)}</span></div>
          </div>
          {br && <button className="branch-pill mono" title="Branches (Ctrl G)" onClick={() => setView({ kind: 'git' })}>{I.branch}{br}</button>}
          {setup.modes?.availableModes?.length ? (
            <select className="select-btn" title="Agent mode" value={modeValue} onChange={(e) => { const v = e.target.value; setTimelines((a) => ({ ...a, [session.id]: { ...a[session.id], currentMode: v } })); configure('session/set_mode', { modeId: v }) }}>
              {setup.modes.availableModes.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
            </select>
          ) : null}
          {modelOpt?.options?.length ? (
            <select className="select-btn" title="Model" value={modelValue} onChange={(e) => { const v = e.target.value; setTimelines((a) => ({ ...a, [session.id]: { ...a[session.id], config: { ...(a[session.id]?.config ?? {}), [modelOpt.id]: v } } })); configure('session/set_config_option', { configId: modelOpt.id, value: v }) }}>
              {modelOpt.options.map((o) => <option key={o.value} value={o.value}>{o.name}</option>)}
            </select>
          ) : modelValue ? <span className="select-btn">{modelValue}</span> : null}
          <span className="vsep" />
          <button className="icon-btn" title="Git: changes, branches, history (Ctrl G)" onClick={() => setView({ kind: 'git' })}>{I.branch}</button>
          <button className="icon-btn" title="Review changes (Ctrl D)" onClick={() => setView({ kind: 'review' })}>{I.compare}</button>
          <button className="icon-btn danger" title="End session on the host" onClick={() => { if (confirm('End this session? The agent process stops and the host record is removed.')) void endSession(session.id) }}>{I.close}</button>
        </div>
        {c && c.kind !== 'connected' && (
          <div className={'banner' + (c.kind === 'failed' ? ' red' : '')}>
            {I.alert}<span>{c.kind === 'reconnecting' ? `Reconnecting (attempt ${c.attempt})… the turn keeps running on the host.` : c.kind === 'failed' ? c.reason : c.kind === 'connecting' ? 'Connecting…' : 'Disconnected'}</span>
          </div>
        )}
        {interrupted && <div className="banner">{I.alert}<span>The host restarted and this session's process ended. Start a new session in the same folder to continue.</span></div>}
        <Conversation t={tl ?? emptyTimeline()} agent={agentName(session.agentId)} onAnswer={(id, o) => void answer(id, o)} onReview={() => setView({ kind: 'review' })} onElicit={(id, r) => void elicit(id, r)} />
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

type NewOpts = { read: boolean; write: boolean; terminal: boolean; worktree: boolean; branch?: string; newBranch: boolean; base?: string; mcp: boolean; firstPrompt?: string }
const defaultOpts: NewOpts = { read: true, write: true, terminal: true, worktree: false, newBranch: false, mcp: true }
const slug = (t: string): string => t.toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 32).replace(/^-|-$/g, '')
function NewSession({ hostId, hostPick, onHost, agents, workspaces, mcp, hostBranches, local, firstPrompt, onClose, onCreate }: { hostId: string; hostPick?: { id: string; label: string; ok: boolean }[]; onHost: (id: string) => void; agents: AgentInfo[]; workspaces: Workspace[]; mcp: McpServer[]; hostBranches: boolean; local: boolean; firstPrompt?: string; onClose: () => void; onCreate: (a: string, w: string, o: NewOpts) => Promise<string | undefined> }) {
  const usable = agents.filter((a) => a.enabled !== false)
  const [agent, setAgent] = useState(localStorage.getItem('acportal.lastAgent') ?? usable[0]?.id ?? '')
  const [ws, setWs] = useState(workspaces[0]?.path ?? '')
  // The lists arrive after a host switch; keep the picks valid for the host shown.
  useEffect(() => { if (!workspaces.some((w) => w.path === ws)) setWs(workspaces[0]?.path ?? '') }, [workspaces])
  useEffect(() => { if (!usable.some((x) => x.id === agent)) setAgent(usable[0]?.id ?? '') }, [agents])
  const [sub, setSub] = useState('')
  const [o, setO] = useState<NewOpts>({ ...defaultOpts, firstPrompt, newBranch: Boolean(firstPrompt), branch: firstPrompt ? 'acportal/' + slug(firstPrompt) : '' })
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string>()
  const [branches, setBranches] = useState<string[]>()
  const [current, setCurrent] = useState<string>()
  const target = sub.trim() ? `${ws.replace(/\/$/, '')}/${sub.trim().replace(/^\//, '')}` : ws
  const canBranch = hostBranches || local
  useEffect(() => {
    let stop = false
    setBranches(undefined)
    if (!canBranch || !target) return
    const t = setTimeout(async () => {
      const r = await window.api.git.run(hostId, target, 'branch-list')
      if (stop) return
      if (!r.ok) { setBranches([]); return }
      const list = parseBranches(r.value.out)
      setBranches(list.filter((b) => !b.remote).map((b) => b.name))
      const cur = list.find((b) => b.current)?.name
      setCurrent(cur)
      setO((x) => ({ ...x, base: x.base && list.some((b) => b.name === x.base) ? x.base : cur }))
    }, 250)
    return () => { stop = true; clearTimeout(t) }
  }, [target, canBranch])
  const go = async (): Promise<void> => {
    setBusy(true); setErr(undefined)
    localStorage.setItem('acportal.lastAgent', agent)
    const e = await onCreate(agent, target, o)
    setBusy(false)
    if (e) setErr(e)
  }
  const Check = ({ k, label, title, disabled }: { k: 'read' | 'write' | 'terminal' | 'mcp'; label: string; title?: string; disabled?: boolean }) => (
    <button className={'check' + (o[k] ? ' on' : '')} title={title} disabled={disabled} onClick={() => setO({ ...o, [k]: !o[k] })}><span className="box">{o[k] ? I.check : null}</span>{label}</button>
  )
  const enabledMcp = mcp.filter((m) => m.enabled)
  const isRepo = Boolean(branches && branches.length)
  const branchName = o.branch?.trim() ?? ''
  const exists = Boolean(branches?.includes(branchName))
  const cmd = !o.newBranch && !o.worktree ? '' : o.worktree
    ? `git worktree add ${o.newBranch ? `-b ${branchName || '<branch>'} ` : ''}../${baseName(target)}-${(branchName || '<branch>').replace(/[^\w-]/g, '-')}${o.newBranch ? ` ${o.base ?? ''}` : ` ${branchName}`}`
    : `git switch ${o.newBranch ? `-c ${branchName || '<branch>'} ${o.base ?? ''}` : branchName}`
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="modal" style={{ width: 620 }} onMouseDown={(e) => e.stopPropagation()} onKeyDown={(e) => { if (e.key === 'Escape') onClose(); if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT' && !busy && agent && ws && !(o.newBranch && (!branchName || exists))) void go() }}>
        <div className="modal-h"><h3>New session</h3><button className="icon-btn" onClick={onClose} aria-label="Close">{I.close}</button></div>
        <div className="sub2">{firstPrompt ? <>Task: <span style={{ color: 'var(--text)' }}>{firstPrompt}</span></> : 'The agent starts on the host in the folder you pick.'}</div>
        {hostPick && (
          <div className="field"><label>Host</label>
            <select className="input" value={hostId} onChange={(e) => onHost(e.target.value)}>{hostPick.map((h) => <option key={h.id} value={h.id} disabled={!h.ok && h.id !== hostId}>{h.label}</option>)}</select>
          </div>
        )}
        <div className="grid2">
          <div className="field"><label>Agent</label>
            <select className="input" value={agent} onChange={(e) => setAgent(e.target.value)}>{usable.map((a) => <option key={a.id} value={a.id}>{a.name}{a.version ? ` · ${a.version}` : ''}</option>)}</select>
          </div>
          <div className="field"><label>Folder</label>
            <select className="input mono" value={ws} onChange={(e) => setWs(e.target.value)}>{workspaces.map((w) => <option key={w.path} value={w.path}>{tildify(w.path)}</option>)}</select>
          </div>
        </div>
        <div className="field"><label>Sub-folder <span className="sub">optional</span></label><input className="input mono" placeholder="e.g. calc" value={sub} onChange={(e) => setSub(e.target.value)} /></div>
        <div className="field"><label>Branch</label>
          {!canBranch ? <div className="hint">This host does not run git for the app; the session starts on whatever branch the folder is on.</div>
            : branches === undefined ? <div className="hint">Reading branches…</div>
            : !isRepo ? <div className="hint">Not a git repository; the session starts in the folder as it is.</div>
            : (
              <>
                <div className="seg">
                  <button className={!o.newBranch && !o.worktree ? 'on' : ''} onClick={() => setO({ ...o, newBranch: false, worktree: false, branch: '' })}>Current <span className="mono">{current}</span></button>
                  <button className={o.newBranch ? 'on' : ''} onClick={() => setO({ ...o, newBranch: true, branch: o.branch || 'acportal/' + (firstPrompt ? slug(firstPrompt) : 'session-' + Date.now().toString(36)) })}>New branch</button>
                  <button className={!o.newBranch && (o.worktree || Boolean(o.branch)) ? 'on' : ''} onClick={() => setO({ ...o, newBranch: false, branch: branches?.find((b) => b !== current) ?? '' })}>Existing branch</button>
                </div>
                {o.newBranch && (
                  <div className="grid2" style={{ marginTop: 8 }}>
                    <input className="input mono" placeholder="acportal/my-change" value={o.branch} onChange={(e) => setO({ ...o, branch: e.target.value.replace(/\s+/g, '-') })} />
                    <div className="inline"><span className="sub">from</span><select className="input mono" value={o.base} onChange={(e) => setO({ ...o, base: e.target.value })}>{branches!.map((b) => <option key={b} value={b}>{b}</option>)}</select></div>
                  </div>
                )}
                {!o.newBranch && (o.worktree || Boolean(o.branch)) && (
                  <select className="input mono" style={{ marginTop: 8 }} value={o.branch} onChange={(e) => setO({ ...o, branch: e.target.value })}>{branches!.filter((b) => b !== current || o.worktree).map((b) => <option key={b} value={b}>{b}</option>)}</select>
                )}
                {(o.newBranch || Boolean(o.branch)) && (
                  <div className="where">
                    <button className={'where-card' + (!o.worktree ? ' on' : '')} onClick={() => setO({ ...o, worktree: false })}><b>Switch this folder</b><span className="sub">Edits land in {tildify(target)}</span></button>
                    <button className={'where-card' + (o.worktree ? ' on' : '')} onClick={() => setO({ ...o, worktree: true })}><b>Own worktree</b><span className="sub">A new folder beside the repo; nothing else changes</span></button>
                  </div>
                )}
                {o.newBranch && exists && <div className="error">A branch named {branchName} already exists.</div>}
                {cmd && <code className="learn-cmd" style={{ marginTop: 8, display: 'inline-block' }}>$ {cmd}</code>}
              </>
            )}
        </div>
        <div className="field"><label>Rights for this session</label>
          <div className="checks"><Check k="read" label="Read files" /><Check k="write" label="Write files" /><Check k="terminal" label="Run commands" />
            <Check k="mcp" label={`MCP servers (${enabledMcp.length})`} title={enabledMcp.map((m) => m.name).join(', ') || 'Add servers in Settings → MCP servers'} disabled={!enabledMcp.length} /></div>
          <div className="hint" title="Delegation is enabled per agent in the host config ([orchestrator] orchestrate)">Delegation to other agents is set on the host.</div>
        </div>
        {err && <div className="error">{err}</div>}
        <div className="modal-foot">
          <button className="btn ghost" onClick={onClose}>Cancel</button>
          <button className="btn primary" disabled={!agent || !ws || busy || (o.newBranch && (!branchName || exists))} onClick={() => void go()}>{busy ? 'Starting agent…' : 'Start session'}<span className="kbd on-primary">Enter</span></button>
        </div>
      </div>
    </div>
  )
}
