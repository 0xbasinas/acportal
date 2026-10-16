import { CompareView, MergeGate, MultiRunDialog, type WatchSnap } from './Multi'
import type { Run } from './runs'
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
import { NewSessionHome, setupKey, type AgentSetup, type StartRequest } from './NewSession'

type View = { kind: 'git' } | { kind: 'workspace' } | { kind: 'settings'; section: Section } | { kind: 'review'; focus?: string } | { kind: 'task'; taskId: string } | { kind: 'pair'; repair?: string } | { kind: 'compare'; runId: string }
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
  const [theme, setThemeState] = useState<Theme>('dark')
  const [toast, setToast] = useState<string>()
  const [merging, setMerging] = useState(false)
  const [confirmMerge, setConfirmMerge] = useState<Task>()
  const [mcp, setMcpState] = useState<McpServer[]>([])
  const [draft, setDraft] = useState<{ text: string; n: number }>()
  const [compose, setCompose] = useState<{ text: string; n: number }>()
  const [composeFolder, setComposeFolder] = useState<string>()
  const [multiPrompt, setMultiPrompt] = useState<string>()
  // The session being started from the composer: shown in the sidebar at once, then its first prompt.
  const [starting, setStarting] = useState<{ hostId: string; title: string; agent: string; workspace: string }>()
  const [firstSend, setFirstSend] = useState<{ sid: string; text: string; atts: Attachment[]; mode?: string; model?: string }>()
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
  // Every live session on every connected host is watched (read-only) for approvals and turn ends.
  type WatchState = WatchSnap & { pending: { id: string | number; title: string; allowOnce?: string; lines: string[] }[] }
  const [watch, setWatch] = useState<Record<string, WatchState>>({})
  const [runs, setRunsState] = useState<Run[]>([])
  const [multiOpen, setMultiOpen] = useState(false)
  useEffect(() => { void window.api.prefs.get().then((r) => { if (r.ok && Array.isArray(r.value.runs)) setRunsState(r.value.runs as Run[]) }) }, [])
  const setRuns = (next: Run[]): void => { setRunsState(next); savePrefs({ runs: next.slice(-20) }) }
  useEffect(() => { void window.api.watch.snapshot().then((r) => { if (r.ok) setWatch(r.value) }); return window.api.watch.onState(setWatch) }, [])
  useEffect(() => {
    const targets = (hosts ?? []).flatMap((h) => fleet[h.id]?.state === 'connected'
      ? (fleet[h.id]?.sessions ?? []).filter((s) => s.status !== 'interrupted' && s.status !== 'failed' && s.status !== 'closed')
        .map((s) => ({ hostRef: h.id, hostName: h.name, sessionId: s.id, title: titles()[s.id] ?? `${s.agentId} in ${baseName(s.workspace)}` }))
      : [])
    void window.api.watch.sync(targets)
  }, [fleet, hosts])
  const waitingOn = (hid: string, sid: string): number => watch[`${hid}/${sid}`]?.pending.length ?? 0
  const waitingHost = (hid: string): number => Object.entries(watch).filter(([k]) => k.startsWith(hid + '/')).reduce((n, [, w]) => n + w.pending.length, 0)
  // Finished delegated tasks on any host become a desktop notification.
  const taskSeen = useRef<Record<string, string>>({})
  useEffect(() => {
    for (const h of hosts ?? []) for (const t of fleet[h.id]?.tasks ?? []) {
      const k = `${h.id}/${t.id}`
      const was = taskSeen.current[k]
      taskSeen.current[k] = t.state
      if (was && was !== t.state && /^(completed|failed|cancelled)$/i.test(t.state)) void window.api.notify(`${h.name}: task ${t.state.toLowerCase()}`, `${t.worktree?.branch ?? t.id.slice(0, 8)} · ${t.agent}${t.summary ? `\n${t.summary.slice(0, 140)}` : ''}`, h.id, t.session ?? undefined)
    }
  }, [fleet, hosts])
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
    if (r.ok) { ref.current = r.value; if (ref === attached) void window.api.watch.focus(r.value) }
    else flash(r.error)
  }
  useEffect(() => { if (selected) void attach(selected, attached) }, [selected, hostId])

  const session = sessions.find((s) => s.id === selected)
  const tl = selected ? timelines[selected] : undefined
  const agentName = (id?: string): string => agents.find((a) => a.id === id)?.name ?? id ?? ''
  const rootTask = tasks.find((t) => t.session === selected && !t.parent) ?? tasks.find((t) => t.session === selected)
  const title = (s: SessionInfo): string => timelines[s.id]?.title ?? titles()[s.id] ?? `${agentName(s.agentId)} in ${baseName(s.workspace)}`
  const pendingIn = (sid: string): number => (timelines[sid]?.items ?? []).filter((i) => i.kind === 'permission' && i.answered === undefined).length
  const latest = useRef({ openOn, answer })
  latest.current = { openOn, answer }
  useEffect(() => {
    const off1 = window.api.watch.onOpen((h, sid) => latest.current.openOn(h, sid))
    const off2 = window.api.watch.onAnswer((key, id, optionId) => { if (key === attached.current) void latest.current.answer(id, optionId) })
    return () => { off1(); off2() }
  }, [])

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
    lastCreated.current = r.value.id
  }
  const lastCreated = useRef<string | undefined>(undefined)
  const openComposer = (text?: string, folder?: string): void => {
    setSelected(undefined); setView({ kind: 'workspace' }); setComposeFolder(folder)
    if (text !== undefined) setCompose({ text, n: Date.now() })
  }
  async function startFromComposer(r: StartRequest): Promise<string | undefined> {
    const title = r.text.split('\n')[0].slice(0, 60)
    setStarting({ hostId: hostId!, title, agent: r.agent, workspace: r.workspace })
    lastCreated.current = undefined
    const b = r.branch
    const e = await createSession(r.agent, r.workspace, { read: r.access.read, write: r.access.write, terminal: r.access.terminal, mcp: r.access.mcp, worktree: b.worktree, newBranch: b.mode === 'new', ...(b.mode === 'current' ? {} : { branch: b.name }), ...(b.mode === 'new' && b.base ? { base: b.base } : {}) })
    setStarting(undefined)
    if (e) return e
    const sid = lastCreated.current
    if (sid) {
      localStorage.setItem(TITLES_KEY, JSON.stringify({ ...titles(), [sid]: title }))
      setFirstSend({ sid, text: r.text, atts: r.atts, mode: r.mode, model: r.model })
    }
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

  // Remember each agent's models and modes, so the new-session composer can offer them.
  useEffect(() => {
    if (!session) return
    const st = (session.setup ?? {}) as { modes?: { currentModeId?: string; availableModes?: { id: string; name: string }[] }; configOptions?: { id: string; category?: string; currentValue?: string; options?: { value: string; name: string }[] }[] }
    const mo = st.configOptions?.find((o) => o.id === 'model' || o.category === 'model')
    if (!st.modes?.availableModes?.length && !mo?.options?.length) return
    const prev = JSON.parse(localStorage.getItem(setupKey(session.agentId)) ?? '{}') as AgentSetup
    const next: AgentSetup = { modes: st.modes?.availableModes?.map((m) => ({ id: m.id, name: m.name })), mode: tl?.currentMode ?? prev.mode ?? st.modes?.currentModeId, modelConfig: mo?.id, models: mo?.options?.map((o) => ({ value: o.value, name: o.name })), model: tl?.config?.[mo?.id ?? ''] ?? prev.model ?? mo?.currentValue }
    localStorage.setItem(setupKey(session.agentId), JSON.stringify(next))
  }, [session?.id, session?.setup, tl?.currentMode, tl?.config])
  // First prompt of a session started from the composer: once connected, apply the remembered
  // model and mode, then send it.
  useEffect(() => {
    if (!firstSend || !session || session.id !== firstSend.sid || conn[session.id]?.kind !== 'connected' || !attached.current?.endsWith(session.id)) return
    const f = firstSend
    setFirstSend(undefined)
    const st = (session.setup ?? {}) as { modes?: { currentModeId?: string; availableModes?: { id: string }[] }; configOptions?: { id: string; category?: string; currentValue?: string; options?: { value: string }[] }[] }
    const mo = st.configOptions?.find((o) => o.id === 'model' || o.category === 'model')
    const rpc = (method: string, params: Record<string, unknown>): Promise<unknown> => window.api.session.send(attached.current!, { jsonrpc: '2.0', id: ++nextId.current, method, params: { sessionId: session.acpSessionId, ...params } })
    void (async () => {
      if (f.mode && f.mode !== st.modes?.currentModeId && st.modes?.availableModes?.some((m) => m.id === f.mode)) { await rpc('session/set_mode', { modeId: f.mode }); setTimelines((a) => ({ ...a, [session.id]: { ...a[session.id], currentMode: f.mode } })) }
      if (mo && f.model && f.model !== mo.currentValue && mo.options?.some((o) => o.value === f.model)) { await rpc('session/set_config_option', { configId: mo.id, value: f.model }); setTimelines((a) => ({ ...a, [session.id]: { ...a[session.id], config: { ...(a[session.id]?.config ?? {}), [mo.id]: f.model! } } })) }
      await sendPrompt(f.text, f.atts)
    })()
  }, [firstSend, session?.id, conn[selected ?? '']?.kind])

  const commands: Command[] = useMemo(() => {
    const c: Command[] = [
      { id: 'new', group: 'Actions', label: 'New session…', hint: 'Ctrl N', run: () => openComposer() },
      { id: 'multi', group: 'Actions', label: 'Run on several agents…', run: () => setMultiOpen(true) },
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
    for (const r of runs) c.push({ id: 'r' + r.id, group: 'Runs', label: `${r.prompt.split('\n')[0].slice(0, 60)} · ${r.entries.length} agents · ${r.state}`, run: () => { if (r.hostId !== hostId) openOn(r.hostId); setView({ kind: 'compare', runId: r.id }) } })
    for (const t of tasks) c.push({ id: 't' + t.id, group: 'Tasks', label: `${t.worktree?.branch ?? 'task ' + t.id.slice(0, 6)} · ${t.agent} · ${stateWord(t.state)}`, run: () => setView({ kind: 'task', taskId: t.id }) })
    for (const h of hosts ?? []) if (h.id !== hostId) c.push({ id: 'h' + h.id, group: 'Hosts', label: `Switch host: ${h.name} · ${STATE_LABEL[fleet[h.id]?.state ?? 'connecting']}`, run: () => openOn(h.id) })
    for (const h of hosts ?? []) if (h.id !== hostId) for (const s of fleet[h.id]?.sessions ?? []) c.push({ id: 'o' + s.id, group: `Sessions on ${h.name}`, label: title(s), run: () => openOn(h.id, s.id) })
    return c
  }, [sessions, tasks, hosts, hostId, theme, timelines, fleet, runs])
  const paletteNew = (q: string): void => openComposer(q)

  useEffect(() => {
    const k = (e: KeyboardEvent): void => {
      const mod = e.metaKey || e.ctrlKey
      if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); setPalette((p) => !p) }
      else if (mod && e.key.toLowerCase() === 'n') { e.preventDefault(); openComposer() }
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
  ;(window as unknown as Record<string, unknown>).__acportal = { setView, setPalette, openComposer, setSelected, setTheme, sessions, tasks, setConfirmMerge }

  if (hosts === null) return null
  if (hosts.length === 0 || view.kind === 'pair')
    return <Pairing replaceId={view.kind === 'pair' ? view.repair : undefined} key={view.kind === 'pair' ? view.repair ?? 'new' : 'first'} initialAddress={view.kind === 'pair' && view.repair ? hosts.find((h) => h.id === view.repair)?.address : undefined} repairName={view.kind === 'pair' && view.repair ? hosts.find((h) => h.id === view.repair)?.name : undefined}
      onPaired={(h) => { void loadHosts().then(() => { setHostId(h.id); reconnect(h.id) }); setView({ kind: 'workspace' }); flash(`Paired with ${h.name}`) }} onCancel={hosts.length ? () => setView({ kind: 'workspace' }) : undefined} />

  const overlays = (
    <>
      {multiOpen && hostId && <MultiRunDialog hostId={hostId} initialPrompt={multiPrompt} agents={agents} workspaces={workspaces} onClose={() => { setMultiOpen(false); setMultiPrompt(undefined) }} onStarted={(r) => { setMultiOpen(false); setMultiPrompt(undefined); setRuns([...runs, r]); setView({ kind: 'compare', runId: r.id }); void refresh(); flash(`Started ${r.entries.filter((e) => e.sessionId).length} runs`) }} />}
      {palette && <Palette commands={commands} onClose={() => setPalette(false)} onNew={paletteNew} />}
      {confirmMerge && (
        <div className="overlay" onMouseDown={() => setConfirmMerge(undefined)}>
          <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
            <h3 style={{ margin: 0, fontSize: 17 }}>Merge into parent folder?</h3>
            <MergeGate hostId={hostId!} root={confirmMerge.worktree?.repository ?? confirmMerge.scope} path={confirmMerge.worktree?.path ?? confirmMerge.scope} prompt={confirmMerge.summary ?? confirmMerge.scope} agents={agents} onMerge={() => void merge(confirmMerge)} onCancel={() => setConfirmMerge(undefined)}>
              <p className="sub2">Merges <span className="mono">{confirmMerge.worktree?.branch}</span> into the checkout it was made from as one <span className="mono">--no-ff</span> commit. A conflict or uncommitted changes in the parent stop the merge and change nothing.</p>
            </MergeGate>
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
        <HostSwitcher waiting={waitingHost} hosts={hosts} current={host} fleet={fleet} onSwitch={(h) => openOn(h.id)} onReconnect={(id) => { reconnect(id); if (!id || id === hostId) void refresh() }}
          onRepair={(h) => setView({ kind: 'pair', repair: h.id })} onSettings={() => setView({ kind: 'settings', section: 'Hosts' })} onAdd={() => setView({ kind: 'pair' })} />
      </div>
      <div className="side-actions">
        <button className={'btn full new-btn' + (!session && view.kind === 'workspace' && online ? ' on' : '')} onClick={() => openComposer()} disabled={!hosts.some((h) => fleet[h.id]?.state === 'connected') && !online}>{I.plus}New session<span className="kbd">Ctrl N</span></button>
        <button className="btn full multi-btn" onClick={() => setMultiOpen(true)} disabled={!online} title="Same prompt to 2-3 agents, each in its own worktree; compare and merge one">{I.flow}Run on several agents</button>
        <button className="side-search" onClick={() => setPalette(true)}>{I.search}<span className="grow">Search or run a command</span><span className="kbd">Ctrl K</span></button>
      </div>
      {hosts.length > 1 && (
        <div className="host-filter" role="tablist" aria-label="Filter by host">
          <button className={'hf' + (hostFilter === 'all' ? ' on' : '')} onClick={() => setHostFilter('all')}>All</button>
          {hosts.map((h) => <button key={h.id} className={'hf' + (hostFilter === h.id ? ' on' : '')} onClick={() => setHostFilter(h.id)} title={`${h.name}: ${STATE_LABEL[fleet[h.id]?.state ?? 'connecting']}`}><span className={'dot ' + STATE_DOT[fleet[h.id]?.state ?? 'connecting']} />{h.name}</button>)}
        </div>
      )}
      <div className="list">
        {runs.some((r) => r.state === 'running') && (
          <div className="runs-group">
            <div className="group-h"><span>Runs</span></div>
            {runs.filter((r) => r.state === 'running').map((r) => {
              const waiting = r.entries.reduce((n, e) => n + (watch[`${r.hostId}/${e.sessionId}`]?.pending.length ?? 0), 0)
              const working = r.entries.filter((e) => watch[`${r.hostId}/${e.sessionId}`]?.busy).length
              return (
                <div key={r.id} className={'row' + (view.kind === 'compare' && view.runId === r.id ? ' active' : '')} onClick={() => { if (r.hostId !== hostId) openOn(r.hostId); setView({ kind: 'compare', runId: r.id }) }} title={r.prompt}>
                  <span className="row-ic">{I.flow}</span>
                  <div className="grow"><div className="ell">{r.prompt.split('\n')[0]}</div><div className="sub ell">{r.entries.length} agents · {working ? `${working} working` : 'all idle'}</div></div>
                  {waiting > 0 && <span className="badge">{waiting}</span>}
                </div>
              )
            })}
          </div>
        )}
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
                  {st !== 'connected' ? <span className={'group-state s-' + st}>{STATE_LABEL[st]}</span> : waitingHost(h.id) > 0 ? <span className="badge" title={`${waitingHost(h.id)} waiting for approval`}>{waitingHost(h.id)}</span> : <span className="count">{hs.length || ''}</span>}
                </div>
              )}
              {!shut && (
                <>
                  {(st === 'offline' || st === 'repair') && !live && <div className="hint" style={{ padding: '2px 12px 6px' }}>{st === 'repair' ? <a onClick={() => setView({ kind: 'pair', repair: h.id })}>Pair again</a> : 'Sessions keep running on the host; they show again when it is back.'}</div>}
                  <div className="section-label">Projects</div>
                  {ws.length === 0 && <div className="hint" style={{ padding: '0 10px' }}>{st === 'connected' ? 'No folders are allowed on this host yet.' : '—'}</div>}
                  {ws.slice(0, 6).map((w) => (
                    <div className="row compact" key={w.path} onClick={() => { if (!live) openOn(h.id); openComposer(undefined, w.path) }} title={`New session in ${w.path} on ${h.name}`}>
                      <span className="row-ic">{I.folder}</span><span className="grow ell">{w.name}</span>
                      <span className="count">{hs.filter((s) => s.workspace.startsWith(w.path)).length || ''}</span>
                    </div>
                  ))}
                  <div className="section-label">Sessions</div>
                  {starting && starting.hostId === h.id && (
                    <div className="row session active" aria-busy="true" title={`Starting ${agentName(starting.agent)} in ${starting.workspace}`}>
                      <span className="dot white live" />
                      <div className="grow"><div className="title">{starting.title}</div><div className="sub row-meta"><span>Starting…</span><span className="mono ell">{baseName(starting.workspace)}</span></div></div>
                    </div>
                  )}
                  {hs.length === 0 && !(starting && starting.hostId === h.id) && <div className="hint" style={{ padding: '0 10px' }}>No sessions on this host.</div>}
                  {[...hs].reverse().map((s) => {
                    const t = live ? timelines[s.id] : undefined
                    const waiting = live && s.id === selected ? pendingIn(s.id) : waitingOn(h.id, s.id)
                    const sst = waiting ? 'waiting' : (t?.busy || (!(live && s.id === selected) && watch[`${h.id}/${s.id}`]?.busy)) ? 'running' : s.status
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
      openSession={(path) => { openComposer(); setTimeout(() => flash(`Pick the folder containing ${tildify(path)}; worktrees inside host roots are listed`), 100) }} />
  } else if (view.kind === 'compare' && runs.some((r) => r.id === view.runId)) {
    const run = runs.find((r) => r.id === view.runId)!
    center = <CompareView key={run.id} run={run} watch={watch} agents={agents} flash={flash} onBack={() => setView({ kind: 'workspace' })}
      onOpen={(sid) => { setSelected(sid); setView({ kind: 'workspace' }) }}
      onUpdate={(n) => { setRuns(n ? runs.map((r) => (r.id === n.id ? n : r)) : runs.filter((r) => r.id !== run.id)); void refresh() }} />
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
  } else if (!session && online && hostId) {
    center = (
      <NewSessionHome hosts={hosts.map((h) => ({ id: h.id, name: h.name, state: STATE_LABEL[fleet[h.id]?.state ?? 'connecting'], ok: fleet[h.id]?.state === 'connected' || h.id === hostId }))} hostId={hostId}
        onHost={(id) => { openOn(id); flash(`Switched to ${hosts.find((h) => h.id === id)?.name}`) }} agents={agents} workspaces={workspaces} mcp={mcp}
        canBranch={Boolean(status?.features?.includes('git')) || Boolean(host && /^http:\/\/(127\.0\.0\.1|localhost|\[::1\])/.test(host.address))}
        history={history} initial={compose} initialFolder={composeFolder} onStart={startFromComposer} onMulti={(t) => { setMultiPrompt(t); setMultiOpen(true) }} />
    )
  } else if (!session) {
    const repair = !online && hostError && /[Pp]air again/.test(hostError)
    center = (
      <div className="main">
        {hostError && <div className="banner red">{I.alert}<span>{hostError}</span></div>}
        <Empty icon={online ? I.chat : I.server} title={online ? 'Pick a session or start one' : repair ? 'Pair this host again' : 'Host offline'}
          actions={<>
            {repair ? <button className="btn primary" onClick={() => setView({ kind: 'pair', repair: hostId })}>Pair again</button> : <button className="btn primary" onClick={() => openComposer()} disabled={!online}>{I.plus}New session</button>}
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
