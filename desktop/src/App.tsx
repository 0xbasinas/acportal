import { CompareView, MergeGate, MultiRunDialog, type WatchSnap } from './Multi'
import type { Run } from './runs'
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react'
import type { AgentInfo, EarlierSessions, Host, HostStatus, SessionInfo, SharedData, Task, Workspace } from './types'
import { changedFiles, emptyTimeline, reduce, Timeline } from './acp'
import { forget, keep, recall } from './transcripts'
import { briefDuration, duration, noteTurn, turnTimes, workedMs, type TurnEnvelope } from './turns'

const saveTimers = new Map<string, ReturnType<typeof setTimeout>>()
import { Pairing } from './Pairing'
import { Command, Palette } from './Palette'
import { Attachment, Composer, Conversation, Permission, type Incoming } from './Session'
import { BrowserPanel } from './Browser'
import { TerminalDock } from './Terminal'
import { StatusBar } from './StatusBar'
import { GitView } from './Git'
import { gitRepos } from './gitRepos'
import { DiffReview, RightPanel, mergeError } from './Tasks'
import { McpServer, Section, Settings, Theme, mcpWire } from './Settings'
import { Empty, I, baseName, stateDot, stateWord, tildify } from './ui'
import { parseBranches } from './gitparse'
import { STATE_DOT, STATE_LABEL, useFleet } from './fleet'
import { HostSwitcher, HostsSettings } from './Hosts'
import { NewSessionHome, setupKey, type AgentSetup, type OptionPick, type StartRequest } from './NewSession'
import type { Preset } from './presets'
import { type Selector, hasChoices, liveSelectors, liveValue, arranged, setRequest, setupFromAcp } from './agentSetup'
import { OptionsButton } from './OptionPicker'
import { groupByProject, inside, isScratch, subfolder } from './projects'

type View = { kind: 'git'; path?: string } | { kind: 'workspace' } | { kind: 'settings'; section: Section } | { kind: 'review'; focus?: string } | { kind: 'task'; taskId: string } | { kind: 'pair'; repair?: string } | { kind: 'compare'; runId: string }
type ConnState = { kind: string; attempt?: number; reason?: string }
const plural = (n: number, word: string): string => `${n} ${word}${n === 1 ? '' : 's'}`
/** The last two folders of a path, so the header shows where a session runs without the full prefix (full path in the tooltip). */
const shortPath = (p: string): string => { const t = tildify(p); const parts = t.split(/[\\/]/).filter(Boolean); return parts.length > 3 ? '…/' + parts.slice(-2).join('/') : t }
/** A timeline showing a value the user just picked, before the agent confirms it. */
function pickedInto(t: Timeline | undefined, sel: Selector, v: string): Timeline {
  const base = t ?? emptyTimeline()
  if (sel.kind === 'mode') return { ...base, currentMode: v }
  if (sel.kind === 'model') return { ...base, currentModel: v }
  return { ...base, config: { ...(base.config ?? {}), [sel.id]: v } }
}
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
  // The in-app browser: open or not, where it sits, how big, and what the person sent from it.
  const [browser, setBrowser] = useState<{ open: boolean; request?: { url: string; n: number } }>({ open: false })
  const [browserWhere, setBrowserWhere] = useState<'side' | 'dock'>(() => (localStorage.getItem('acportal.browserWhere') === 'dock' ? 'dock' : 'side'))
  const [browserSize, setBrowserSize] = useState<{ side: number; dock: number }>(() => { try { return { side: 620, dock: 380, ...JSON.parse(localStorage.getItem('acportal.browserSize') ?? '{}') } } catch { return { side: 620, dock: 380 } } })
  const [incoming, setIncoming] = useState<Incoming>()
  // The terminal dock: shells on the host, under the conversation.
  const [termOpen, setTermOpen] = useState(false)
  const [termHeight, setTermHeight] = useState<number>(() => Number(localStorage.getItem('acportal.termHeight')) || 300)
  const toggleTerminal = useCallback(() => setTermOpen((o) => !o), [])
  const openBrowser = useCallback((url?: string) => setBrowser({ open: true, request: url ? { url, n: Date.now() } : undefined }), [])
  const closeBrowser = useCallback(() => { setBrowser({ open: false }); void window.api.browser.close() }, [])
  const toggleBrowser = useCallback(() => setBrowser((b) => { if (b.open) { void window.api.browser.close(); return { open: false } } return { open: true } }), [])
  useEffect(() => {
    const open = (e: Event): void => openBrowser((e as CustomEvent<string>).detail)
    window.addEventListener('acportal:open-url', open)
    return () => window.removeEventListener('acportal:open-url', open)
  }, [openBrowser])
  const [theme, setThemeState] = useState<Theme>('dark')
  const [toast, setToast] = useState<string>()
  const [merging, setMerging] = useState(false)
  const [confirmMerge, setConfirmMerge] = useState<Task>()
  const [mcp, setMcpState] = useState<McpServer[]>([])
  const [draft, setDraft] = useState<{ text: string; n: number }>()
  const [shared, setShared] = useState<SharedData>()
  const [compose, setCompose] = useState<{ text: string; n: number }>()
  const [composeFolder, setComposeFolder] = useState<string>()
  const [multiPrompt, setMultiPrompt] = useState<string>()
  // The session being started from the composer: shown in the sidebar at once, then its first prompt.
  const [starting, setStarting] = useState<{ hostId: string; title: string; agent: string; workspace: string }>()
  const [firstSend, setFirstSend] = useState<{ sid: string; text: string; atts: Attachment[]; choices: OptionPick[] }>()
  const [branchOf, setBranchOf] = useState<Record<string, string>>({})
  const [taskDetail, setTaskDetail] = useState<Task>()
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
  // Projects the user closed in the sidebar (host|folder -> closed); kept across restarts.
  const [shutProjects, setShutProjects] = useState<Record<string, boolean>>(() => { try { return JSON.parse(localStorage.getItem('acportal.shutProjects') ?? '{}') } catch { return {} } })
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
  const presetsOn = Boolean(status?.features?.includes('presets'))
  const earlierOn = Boolean(status?.features?.includes('agentSessionList'))
  const loadEarlier = useCallback(async (agent: string, workspace: string): Promise<EarlierSessions | undefined> => {
    const r = await api<EarlierSessions>('GET', `/v1/agents/${encodeURIComponent(agent)}/sessions`, undefined, { workspace })
    return r.ok ? r.value : undefined
  }, [api])
  const loadPresets = useCallback(async (id: string): Promise<Preset[] | undefined> => {
    const r = await api<{ presets: Preset[] }>('GET', '/v1/presets', undefined, { agent: id })
    return r.ok ? r.value.presets : undefined
  }, [api])

  const refresh = useCallback(async () => {
    if (!hostId) return
    const s = await api<HostStatus>('GET', '/v1/status')
    if (!s.ok) { setOnline(false); setHostError(s.code === 'locked' ? 'This computer has no system keychain, so the token was kept only until the app quit. Pair again (Settings → Hosts).' : s.status === 401 ? 'This device is no longer paired with the host. Pair again in Settings → Hosts.' : s.error); return }
    setOnline(true); setHostError(undefined); setStatus(s.value)
    const [a, w, ss] = await Promise.all([api<AgentInfo[]>('GET', '/v1/agents'), api<Workspace[]>('GET', '/v1/workspaces'), api<SessionInfo[]>('GET', '/v1/sessions')])
    if (a.ok) setAgents(a.value)
    if (w.ok) setWorkspaces(w.value)
    if (ss.ok) setSessions(ss.value)
    if (s.value.features?.includes('sharedDrafts')) { const sh = await api<SharedData>('GET', '/v1/shared'); if (sh.ok) setShared((old) => (old?.revision === sh.value.revision ? old : sh.value)) } else setShared(undefined)
    if (s.value.features?.includes('orchestrator')) { const t = await api<Task[]>('GET', '/v1/tasks'); if (t.ok) setTasks(t.value) }
  }, [api, hostId])
  useEffect(() => {
    setSessions([]); setTasks([]); setAgents([]); setWorkspaces([]); setShared(undefined); setSelected(pendingSel.current); pendingSel.current = undefined; setTimelines({})
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
        noteTurn(key, env as TurnEnvelope, before, next)
        if (next.title && next.title !== before?.title) localStorage.setItem(TITLES_KEY, JSON.stringify({ ...titles(), [sid]: next.title }))
        // Save a copy after a short quiet period, so an ended session can still be read.
        const copy = key
        clearTimeout(saveTimers.get(copy))
        saveTimers.set(copy, setTimeout(() => { saveTimers.delete(copy); keep(copy.split('/')[0], sid, next) }, 1500))
        return { ...all, [sid]: next }
      })
    })
    const off2 = window.api.session.onState((key, s) => setConn((c) => ({ ...c, [key.split('/')[1]]: s as ConnState })))
    return () => { off1(); off2() }
  }, [])

  async function attach(sid: string, ref: typeof attached, readOnly = false, takeover = false): Promise<void> {
    if (!hostId || (ref.current === `${hostId}/${sid}` && !takeover)) return
    if (ref.current) void window.api.session.detach(ref.current)
    // A session whose process ended has nothing to connect to: show the copy this app saved.
    if (ref === attached && sessions.find((x) => x.id === sid)?.status === 'interrupted') {
      ref.current = undefined
      setTimelines((all) => ({ ...all, [sid]: recall(hostId, sid) ?? emptyTimeline() }))
      return
    }
    // Show what this computer saved while the host sends the conversation again.
    setTimelines((all) => ({ ...all, [sid]: { ...emptyTimeline(), loading: true, preview: recall(hostId, sid)?.items } }))
    const r = await window.api.session.attach(hostId, sid, 0, readOnly, takeover)
    if (r.ok) { ref.current = r.value; if (ref === attached) void window.api.watch.focus(r.value) }
    else { flash(r.error); setTimelines((all) => ({ ...all, [sid]: { ...(all[sid] ?? emptyTimeline()), loading: false, preview: undefined } })) }
  }
  const selectedEnded = sessions.find((x) => x.id === selected)?.status === 'interrupted'
  useEffect(() => { if (selected) void attach(selected, attached) }, [selected, hostId, selectedEnded])

  const session = sessions.find((s) => s.id === selected)
  const tl = selected ? timelines[selected] : undefined
  const agentName = (id?: string): string => agents.find((a) => a.id === id)?.name ?? id ?? ''
  const rootTask = tasks.find((t) => t.session === selected && !t.parent) ?? tasks.find((t) => t.session === selected)
  const title = (s: SessionInfo): string => timelines[s.id]?.title ?? shared?.titles[s.id] ?? titles()[s.id] ?? `${agentName(s.agentId)} in ${baseName(s.workspace)}`
  // The window title (shown in the app's title bar and the taskbar): the open session, else the page.
  const windowTitle = session ? title(session) : view.kind === 'settings' ? 'Settings' : view.kind === 'git' ? 'Git' : ''
  useEffect(() => { document.title = windowTitle ? `${windowTitle} · acportal` : 'acportal' }, [windowTitle])
  const pendingIn = (sid: string): number => (timelines[sid]?.items ?? []).filter((i) => i.kind === 'permission' && i.answered === undefined).length
  const latest = useRef({ openOn, answer })
  latest.current = { openOn, answer }
  useEffect(() => {
    const off1 = window.api.watch.onOpen((h, sid) => latest.current.openOn(h, sid))
    const off2 = window.api.watch.onAnswer((key, id, optionId) => { if (key === attached.current) void latest.current.answer(id, optionId) })
    return () => { off1(); off2() }
  }, [])

  const histKey = (ws?: string): string => `acportal.history.${hostId}.${ws ?? ''}`
  const history = (ws?: string): string[] => {
    const local: string[] = JSON.parse(localStorage.getItem(histKey(ws)) ?? '[]')
    return [...new Set([...(shared?.history[ws ?? ''] ?? []), ...local])].slice(0, 50)
  }
  async function sendPrompt(text: string, atts: Attachment[] = [], silent = false): Promise<number | undefined> {
    if (!session || !attached.current) return
    const id = ++nextId.current
    if (text.trim() && !silent) {
      localStorage.setItem(histKey(session.workspace), JSON.stringify([text, ...history(session.workspace).filter((h) => h !== text)].slice(0, 50)))
      if (shared && text.length <= 16 * 1024) void api('POST', '/v1/shared/history', { workspace: session.workspace, text })
    }
    const blocks: unknown[] = [{ type: 'text', text }]
    for (const a of atts) blocks.push(a.data ? { type: 'image', mimeType: a.mimeType, data: a.data } : { type: 'resource', resource: { uri: a.uri ?? 'file://' + a.path, mimeType: a.mimeType, text: a.text } })
    if (!titles()[session.id] && !shared?.titles[session.id] && !silent) {
      const first = text.split('\n')[0].slice(0, 60)
      localStorage.setItem(TITLES_KEY, JSON.stringify({ ...titles(), [session.id]: first }))
      if (shared && first.trim()) void api('PUT', `/v1/shared/titles/${session.id}`, { title: first })
    }
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
  // A commit message for what is staged in `path`, written by an agent in a throwaway session on
  // the host (POST /v1/commit-message): no open session needed and nothing lands in a conversation.
  async function suggestCommit(path: string, agentId: string): Promise<{ message?: string; error?: string }> {
    const r = await api<{ message: string }>('POST', '/v1/commit-message', { path, agentId })
    if (r.ok) return { message: r.value.message }
    const words: Record<string, string> = {
      nothing_staged: 'Nothing is staged.', agent_unavailable: 'That agent could not start in this folder.', agent_timeout: 'The agent did not answer in time.',
      agent_failed: 'The agent reported an error.', empty_reply: 'The agent replied with nothing.', path_not_allowed: 'This folder is not allowed on the host.', not_a_repository: 'This folder is not a git repository.'
    }
    return { error: words[r.code ?? ''] ?? r.error }
  }
  async function suggestPr(path: string, agentId: string, base: string): Promise<{ title?: string; body?: string; error?: string }> {
    const r = await api<{ title: string; body: string }>('POST', '/v1/commit-message', { path, agentId, kind: 'pullRequest', base })
    if (r.ok) return { title: r.value.title, body: r.value.body }
    const words: Record<string, string> = {
      nothing_to_describe: 'Nothing to describe since that base.', base_not_found: 'That base branch was not found.',
      agent_unavailable: 'That agent could not start in this folder.', agent_timeout: 'The agent did not answer in time.',
      agent_failed: 'The agent reported an error.', empty_reply: 'The agent replied with nothing.',
      path_not_allowed: 'This folder is not allowed on the host.', invalid_branch_name: 'That branch name is not valid.',
      invalid_parameter: 'The request was not valid.'
    }
    return { error: words[r.code ?? ''] ?? r.error }
  }
  async function createPr(input: { path: string; base: string; head: string; title: string; body: string; draft: boolean }): Promise<{ url?: string; existing?: boolean; error?: string }> {
    const r = await api<{ url: string }>('POST', '/v1/pull-requests', input)
    if (r.ok) return { url: r.value.url }
    // The host puts the existing pull request's address in the message of an already_exists reply.
    const existing = r.code === 'already_exists' ? /https:\/\/\S+/.exec(r.error)?.[0] : undefined
    if (existing) return { url: existing, existing: true }
    const words: Record<string, string> = {
      gh_unavailable: 'GitHub CLI (gh) was not found on the host.', gh_not_signed_in: 'gh is not signed in on the host.',
      no_remote: 'This repository has no GitHub remote gh can use.', already_exists: 'A pull request for this branch already exists.',
      gh_failed: 'gh could not open the pull request.', gh_timeout: 'gh did not finish in time.',
      invalid_branch_name: 'That branch name is not valid.', body_too_large: 'The description is too long.',
      path_not_allowed: 'This folder is not allowed on the host.', invalid_parameter: 'The title is empty or too long.'
    }
    return { error: words[r.code ?? ''] ?? r.error }
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
    const access = { readFiles: opts.read, writeFiles: opts.write, terminal: opts.terminal, ...(opts.manual ? { manualApproval: true } : {}), ...(opts.yolo ? { yolo: true } : {}), ...(status?.features?.includes('formElicitation') ? { formElicitation: true } : {}), ...(status?.features?.includes('booleanOptions') ? { booleanOptions: true } : {}) }
    const r = await api<SessionInfo>('POST', '/v1/sessions', { agentId, workspace: target, loadSessionId: opts.load ?? null, workspaceAccess: access, mcpServers: mcp.filter((m) => m.enabled && opts.mcp).map(mcpWire), ...(branchOpt ? { branch: branchOpt } : {}) })
    if (!r.ok) return ({ unsupported_mcp_transport: 'This agent does not support HTTP/SSE MCP servers; turn those off in Settings → MCP servers', branch_failed: 'Git could not make or switch to that branch (uncommitted changes in the way, or the branch is checked out elsewhere).', worktree_path_unavailable: 'The worktree folder already exists or is outside the host folders.', invalid_branch_name: 'That is not a valid branch name.', not_a_repository: 'This folder is not a git repository.' } as Record<string, string>)[r.code ?? ''] ?? r.error
    if (opts.firstPrompt) setTimeout(() => setDraft({ text: opts.firstPrompt!, n: Date.now() }), 300)
    setSessions((s) => [...s, r.value])
    setSelected(r.value.id)
    setView({ kind: 'workspace' })
    lastCreated.current = r.value.id
  }
  const lastCreated = useRef<string | undefined>(undefined)
  // An ended session is continued by asking the agent to load it into a new session. The new one
  // takes over the old one's title and the old record goes, so the list never shows both.
  const [resuming, setResuming] = useState<string>()
  // Removes ended sessions from the host's list (their records only; nothing in the folders changes).
  async function clearEnded(list: SessionInfo[]): Promise<void> {
    if (!list.length || !confirm(`Remove ${list.length} ended session${list.length === 1 ? '' : 's'}? Their saved conversation copies on this computer go too. Files in your folders are not touched.`)) return
    let failed = 0
    for (const s of list) { const d = await api('DELETE', `/v1/sessions/${s.id}`); if (d.ok) forget(hostId!, s.id); else failed++ }
    if (selected && list.some((s) => s.id === selected)) setSelected(undefined)
    if (failed) flash(`${failed} could not be removed.`)
    void refresh()
  }
  async function resume(s: SessionInfo): Promise<void> {
    if (resuming) return
    setResuming(s.id)
    lastCreated.current = undefined
    const err = await createSession(s.agentId, s.workspace, { ...defaultOpts, load: s.acpSessionId })
    setResuming(undefined)
    const fresh = lastCreated.current
    if (err || !fresh) return flash(`${agentName(s.agentId)} could not load that conversation${err ? ': ' + err : ''}. The saved copy stays here and can still be read.`)
    const old = titles()[s.id]
    if (old) localStorage.setItem(TITLES_KEY, JSON.stringify({ ...titles(), [fresh]: old }))
    await api('DELETE', `/v1/sessions/${s.id}`)
    forget(hostId!, s.id)
    void refresh()
  }
  const openComposer = (text?: string, folder?: string): void => {
    setSelected(undefined); setView({ kind: 'workspace' }); setComposeFolder(folder)
    if (text !== undefined) setCompose({ text, n: Date.now() })
  }
  async function startFromComposer(r: StartRequest): Promise<string | undefined> {
    const title = r.text.split('\n')[0].slice(0, 60)
    setStarting({ hostId: hostId!, title, agent: r.agent, workspace: r.workspace })
    lastCreated.current = undefined
    const b = r.branch
    const e = await createSession(r.agent, r.workspace, { read: r.access.read, manual: r.access.manual, yolo: r.access.yolo, write: r.access.write, terminal: r.access.terminal, mcp: r.access.mcp, worktree: b.worktree, newBranch: b.mode === 'new', ...(b.mode === 'current' ? {} : { branch: b.name }), ...(b.mode === 'new' && b.base ? { base: b.base } : {}) })
    setStarting(undefined)
    if (e) return e
    const sid = lastCreated.current
    if (sid) {
      localStorage.setItem(TITLES_KEY, JSON.stringify({ ...titles(), [sid]: title }))
      if (shared && title.trim()) void api('PUT', `/v1/shared/titles/${sid}`, { title })
      setFirstSend({ sid, text: r.text, atts: r.atts, choices: r.choices })
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
    const selectors = liveSelectors(session.setup, tl)
    const next: AgentSetup = setupFromAcp(null)
    next.selectors = selectors
    if (!hasChoices(next)) return
    for (const s of selectors) { const v = liveValue(s, tl); if (v !== undefined) next.chosen![s.id] = v }
    localStorage.setItem(setupKey(session.agentId), JSON.stringify(next))
  }, [session?.id, session?.setup, tl?.currentMode, tl?.currentModel, tl?.config, tl?.configOptions])
  // First prompt of a session started from the composer: once connected, apply the remembered
  // model and mode, then send it.
  useEffect(() => {
    if (!firstSend || !session || session.id !== firstSend.sid || conn[session.id]?.kind !== 'connected' || !attached.current?.endsWith(session.id)) return
    const f = firstSend
    setFirstSend(undefined)
    const rpc = (method: string, params: Record<string, unknown>): Promise<unknown> => window.api.session.send(attached.current!, { jsonrpc: '2.0', id: ++nextId.current, method, params: { sessionId: session.acpSessionId, ...params } })
    void (async () => {
      // In the order they were picked, so a provider is set before the model it brings. Options
      // the agent does not report are skipped; a refused value shows a notice from the reply.
      const start = liveSelectors(session.setup)
      for (const p of f.choices) {
        const sel = start.find((x) => x.id === p.id)
        if (!sel || (sel.kind !== 'config' && !sel.choices.some((c) => c.value === p.value))) continue
        const req = setRequest(sel, p.value)
        await rpc(req.method, req.params)
        setTimelines((a) => ({ ...a, [session.id]: pickedInto(a[session.id], sel, p.value) }))
      }
      await sendPrompt(f.text, f.atts)
    })()
  }, [firstSend, session?.id, conn[selected ?? '']?.kind])

  const commands: Command[] = useMemo(() => {
    const c: Command[] = [
      { id: 'new', group: 'Actions', label: 'New session…', hint: 'Ctrl N', run: () => openComposer() },
      { id: 'multi', group: 'Actions', label: 'Run on several agents…', run: () => setMultiOpen(true) },
      { id: 'git', group: 'Actions', label: 'Open Git (changes, history, worktrees, stash)', hint: 'Ctrl G', run: () => setView({ kind: 'git' }) },
      { id: 'terminal', group: 'Actions', label: 'Show or hide the terminal', hint: 'Ctrl `', run: () => toggleTerminal() },
      { id: 'browser', group: 'Actions', label: 'Open the browser (preview a page)', hint: 'Ctrl Shift B', run: () => openBrowser() },
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
  // Ask the host for an agent's models and modes before its first session (throwaway session on
  // the host, no prompt). Undefined when the host cannot tell; the composer then uses defaults.
  const newScratch = useCallback(async (): Promise<string | undefined> => {
    const r = await api<{ path: string }>('POST', '/v1/scratch')
    return r.ok ? r.value.path : undefined
  }, [api])
  const probeAgent = useCallback(async (agent: string, workspace: string): Promise<unknown | undefined> => {
    const r = await api<{ setup?: unknown }>('POST', `/v1/agents/${encodeURIComponent(agent)}/probe`, { workspace })
    return r.ok ? (r.value.setup ?? {}) : undefined
  }, [api])

  useEffect(() => {
    const k = (e: KeyboardEvent): void => {
      const mod = e.metaKey || e.ctrlKey
      if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); setPalette((p) => !p) }
      else if (mod && e.key.toLowerCase() === 'n') { e.preventDefault(); openComposer() }
      else if (mod && e.key === ',') { e.preventDefault(); setView({ kind: 'settings', section: 'General' }) }
      else if (mod && e.key.toLowerCase() === 'd') { e.preventDefault(); setView({ kind: 'review' }) }
      else if (mod && e.key.toLowerCase() === 'g') { e.preventDefault(); setView({ kind: 'git' }) }
      else if (mod && e.shiftKey && e.key.toLowerCase() === 'b') { e.preventDefault(); toggleBrowser() }
      else if (mod && e.key === '`') { e.preventDefault(); toggleTerminal() }
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
        <button className="side-search" onClick={() => setPalette(true)}>{I.search}<span className="grow" title="Search sessions or run a command">Search or command</span><span className="kbd">Ctrl K</span></button>
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
                  {ws.length === 0 && hs.length === 0 && <div className="hint" style={{ padding: '0 10px' }}>{st === 'connected' ? 'No folders are allowed on this host yet.' : '—'}</div>}
                  {/* Sessions under the project they run in, like folders: the most recent project on top. */}
                  {groupByProject(hs, ws, fleet[h.id]?.status?.scratchRoot).map((g) => {
                    const key = h.id + '|' + g.key
                    const scratchRoot = fleet[h.id]?.status?.scratchRoot
                    const startingScratch = starting ? isScratch(starting.workspace, scratchRoot) : false
                    const startingHere = starting && starting.hostId === h.id && (g.path ? !startingScratch && inside(starting.workspace, g.path) : startingScratch || !ws.some((w) => inside(starting.workspace, w.path)))
                    const closed = shutProjects[key] ?? (!g.sessions.length && !startingHere)
                    const toggle = (): void => setShutProjects((c) => { const n = { ...c, [key]: !closed }; try { localStorage.setItem('acportal.shutProjects', JSON.stringify(n)) } catch { /* ignore */ } return n })
                    const isEnded = (s: { status?: string }): boolean => s.status === 'interrupted'
                    const endedCount = g.sessions.filter(isEnded).length
                    const endedKey = key + '|ended'
                    const endedShut = shutProjects[endedKey] ?? true
                    const toggleEnded = (): void => setShutProjects((c) => { const n = { ...c, [endedKey]: !endedShut }; try { localStorage.setItem('acportal.shutProjects', JSON.stringify(n)) } catch { /* ignore */ } return n })
                    return (
                      <div key={g.key} className="project-group">
                        <div className="row compact project-row" role="button" aria-expanded={!closed} onClick={toggle} title={g.path ? `${g.path} on ${h.name}` : 'Sessions outside every project folder'}>
                          <span className={'chev-ic' + (closed ? ' shut' : '')}>{I.down}</span>
                          <span className="row-ic">{g.path ? I.folder : I.noProject}</span><span className="grow ell">{g.name}</span>
                          <span className="count">{g.sessions.length || ''}</span>
                          {g.path && <button className="icon-btn project-git" title={`Git for ${g.name}`} aria-label={`Git for ${g.name}`} onClick={(e) => { e.stopPropagation(); if (!live) openOn(h.id); setView({ kind: 'git', path: g.path }) }}>{I.branch}</button>}
          {g.path && <button className="icon-btn project-new" title={`New session in ${g.name}`} aria-label={`New session in ${g.name}`} onClick={(e) => { e.stopPropagation(); if (!live) openOn(h.id); openComposer(undefined, g.path) }}>{I.plus}</button>}
                        </div>
                        {!closed && startingHere && (
                          <div className="row session nested active" aria-busy="true" title={`Starting ${agentName(starting!.agent)} in ${starting!.workspace}`}>
                            <span className="dot white live" />
                            <span className="grow title">{starting!.title}</span>
                            <span className="row-note">Starting…</span>
                          </div>
                        )}
                        {!closed && g.sessions.filter((s) => !isEnded(s) || !endedShut || (live && s.id === selected)).map((s) => {
                          const t = live ? timelines[s.id] : undefined
                          const waiting = live && s.id === selected ? pendingIn(s.id) : waitingOn(h.id, s.id)
                          const sst = waiting ? 'waiting' : (t?.busy || (!(live && s.id === selected) && watch[`${h.id}/${s.id}`]?.busy)) ? 'running' : s.status
                          const br = live ? branchOf[s.workspace] : undefined
                          const sub = g.path ? subfolder(s.workspace, g.path) : undefined
                          // One line per session: the dot shows the state, the tooltip the rest;
                          // only states that need attention get a word on the row.
                          const where = br ? `on ${br}` : sub ?? (!g.path ? baseName(s.workspace) : undefined)
                          const worked = workedMs(`${h.id}/${s.id}`)
                          const note = !waiting && sst !== 'ready' && sst !== 'running' ? stateWord(sst) : undefined
                          return (
                            <div key={s.id} className={'row session nested' + (live && s.id === selected ? ' active' : '') + (s.status === 'interrupted' ? ' ended' : '')} onClick={() => openOn(h.id, s.id)}
                              title={[title(s), waiting ? `${waiting} waiting` : stateWord(sst), where].filter(Boolean).join(' · ')}>
                              <span className={'dot ' + stateDot(sst) + (sst === 'running' ? ' live' : '')} />
                              <span className="grow title">{title(s)}</span>
                              {worked > 0 && <span className="row-note" title={`Time spent working in this session: ${duration(worked)}`}>{briefDuration(worked)}</span>}
                              {s.status === 'interrupted'
                                ? <button className="row-resume" title="The host restarted and this session's process ended. Resume asks the agent to load the conversation again in a new session." aria-label={`Resume ${title(s)}`}
                                    disabled={Boolean(resuming)} onClick={(e) => { e.stopPropagation(); if (!live) openOn(h.id); void resume(s) }}>{resuming === s.id ? 'Resuming…' : 'Resume'}</button>
                                : note && <span className="row-note">{note}</span>}
                              {waiting > 0 && <span className="badge">{waiting}</span>}
                            </div>
                          )
                        })}
                        {!closed && endedCount > 0 && (
                          <div className="row compact ended-row" role="button" aria-expanded={!endedShut} onClick={toggleEnded}
                            title="Sessions whose process ended when the host restarted. Their conversation is not kept; Resume asks the agent to load it again.">
                            <span className={'chev-ic' + (endedShut ? ' shut' : '')}>{I.down}</span>
                            <span className="grow ell">{endedShut ? `${endedCount} ended` : 'Ended'}</span>
                            <button className="row-resume" title="Remove every ended session in this project from the list" onClick={(e) => { e.stopPropagation(); void clearEnded(g.sessions.filter(isEnded)) }}>Clear</button>
                          </div>
                        )}
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
        <button className="foot-btn" onClick={() => setView({ kind: 'settings', section: 'General' })} title="Settings (Ctrl ,)">{I.gear}Settings</button>
      </div>
    </aside>
  )

  let center
  if (view.kind === 'git' && hostId) {
    const repos = gitRepos({ asked: view.path, sessionFolder: session?.workspace, workspaces, tasks })
    center = <GitView key={view.path ?? 'default'} hostId={hostId} repos={repos} onBack={() => setView({ kind: 'workspace' })}
      suggest={status?.features?.includes('commitMessage') ? suggestCommit : undefined}
      pullRequests={status?.features?.includes('pullRequests')} ghCli={status?.gh}
      suggestPr={status?.features?.includes('commitMessage') ? suggestPr : undefined}
      createPr={status?.features?.includes('pullRequests') ? createPr : undefined}
      agents={agents.filter((a) => a.enabled !== false && a.status !== 'missing' && a.status !== 'misconfigured')} defaultAgent={session?.agentId}
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
        history={history} initial={compose} initialFolder={composeFolder} onStart={startFromComposer} onMulti={(t) => { setMultiPrompt(t); setMultiOpen(true) }}
        probe={status?.features?.includes('agentProbe') ? probeAgent : undefined}
        newScratch={status?.features?.includes('scratchSessions') ? newScratch : undefined}
        loadPresets={presetsOn ? loadPresets : undefined}
        loadEarlier={earlierOn ? loadEarlier : undefined}
        openEarlier={(agent, workspace, sessionId, access) => createSession(agent, workspace, { ...defaultOpts, load: sessionId, read: access.read, write: access.write, terminal: access.terminal, mcp: access.mcp, manual: access.manual, yolo: access.yolo })} />
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
          {online ? `${sessions.length} session${sessions.length === 1 ? '' : 's'} on ${host?.name ?? 'this host'} · ${plural(agents.filter((a) => a.enabled !== false).length, 'agent')} ready` : repair ? 'Run acpd pair on the host and enter the new code.' : 'Waiting for the host to answer. Check that acpd is running and reachable.'}
        </Empty>
      </div>
    )
  } else {
    const c = conn[session.id]
    const selectors = arranged(liveSelectors(session.setup, tl))
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
            <div className="sub crumbs"><span>{agentName(session.agentId)}</span><span className={tl?.busy ? 'live-word' : ''}>{tl?.busy ? 'Running' : stateWord(session.status)}</span><span className="mono ell" title={session.workspace}>{shortPath(session.workspace)}</span></div>
          </div>
          {br && <button className="branch-pill mono" title="Branches (Ctrl G)" onClick={() => setView({ kind: 'git', path: session.workspace })}>{I.branch}{br}</button>}
          <OptionsButton selectors={selectors.filter((s) => s.choices.length)} valueOf={(s) => liveValue(s, tl)} disabled={interrupted}
            onPick={(sel, v) => { setTimelines((a) => ({ ...a, [session.id]: pickedInto(a[session.id], sel, v) })); const req = setRequest(sel, v); configure(req.method, req.params) }} />
          <span className="vsep" />
          <button className={'icon-btn' + (termOpen ? ' on' : '')} title="Terminal (Ctrl `)" aria-label="Terminal" aria-pressed={termOpen} onClick={toggleTerminal}>{I.term}</button>
          <button className={'icon-btn' + (browser.open ? ' on' : '')} title="Browser (Ctrl Shift B)" aria-label="Browser" aria-pressed={browser.open} onClick={toggleBrowser}>{I.globe}</button>
          <button className="icon-btn" title="Review changes (Ctrl D)" onClick={() => setView({ kind: 'review' })}>{I.compare}</button>
          <button className="icon-btn danger" title="End session on the host" onClick={() => { if (confirm('End this session? The agent process stops and the host record is removed.')) void endSession(session.id) }}>{I.close}</button>
        </div>
        {c?.kind === 'observing' && (
          <div className="banner">
            {I.alert}<span>Another device (your phone or another computer) is controlling this session. You are watching it live.</span>
            <button className="btn" onClick={() => void attach(session.id, attached, false, true)}>Take control</button>
          </div>
        )}
        {c && c.kind !== 'connected' && c.kind !== 'observing' && (
          <div className={'banner' + (c.kind === 'failed' ? ' red' : '')}>
            {I.alert}<span>{c.kind === 'reconnecting' ? `Reconnecting (attempt ${c.attempt})… the turn keeps running on the host.` : c.kind === 'failed' ? c.reason : c.kind === 'connecting' ? 'Connecting…' : 'Disconnected'}</span>
          </div>
        )}
        {interrupted && (
          <div className="banner ended-banner">
            {I.alert}
            <span className="grow"><b>This session ended</b> when the host restarted.{' '}
              <span className="sub">{tl?.items.length ? 'You are reading the copy saved on this computer.' : 'No copy was saved on this computer.'}</span></span>
            <button className="btn primary" disabled={Boolean(resuming)} onClick={() => void resume(session)} title={`Asks ${agentName(session.agentId)} to load this conversation into a new session. Only agents that can load earlier sessions manage it; if it cannot, this copy stays.`}>{resuming === session.id ? 'Resuming…' : 'Resume'}</button>
          </div>
        )}
        <Conversation root={session.workspace} turnKey={`${hostId}/${session.id}`} t={tl ?? emptyTimeline()} agent={agentName(session.agentId)} onAnswer={(id, o) => void answer(id, o)} onReview={() => setView({ kind: 'review' })} onElicit={(id, r) => void elicit(id, r)} />
        <Composer agent={agentName(session.agentId)} busy={Boolean(tl?.busy)} disabled={interrupted ? 'This session ended. Press Resume to continue it.' : c?.kind === 'observing' ? 'Take control to send from this computer' : c?.kind !== 'connected' ? 'Waiting for the connection…' : undefined} onSend={(t, a) => void sendPrompt(t, a)} onCancel={() => void cancelTurn()}
          history={history(session.workspace)} canImage={Boolean(pc?.image)} canResource={Boolean(pc?.embeddedContext)} draft={draft} incoming={incoming} key={session.id}
          remote={shared ? { text: shared.drafts[session.id]?.text ?? '', at: shared.drafts[session.id]?.updatedAt ?? 0 } : undefined}
          onText={shared ? (t) => { if (new TextEncoder().encode(t).length <= 64 * 1024) void api('PUT', `/v1/shared/drafts/${session.id}`, { text: t }) } : undefined} />
      </div>
    )
  }

  const showRight = view.kind === 'workspace' && Boolean(session)
  const sideBrowser = browser.open && browserWhere === 'side'
  const resizeBrowser = (size: number): void => setBrowserSize((s) => { const n = { ...s, [browserWhere]: size }; localStorage.setItem('acportal.browserSize', JSON.stringify(n)); return n })
  const browserPanel = browser.open && (
    <BrowserPanel where={browserWhere} request={browser.request} size={browserSize[browserWhere]} onSize={resizeBrowser} covered={palette || multiOpen || Boolean(confirmMerge)}
      onClose={closeBrowser} onWhere={(w) => { setBrowserWhere(w); localStorage.setItem('acportal.browserWhere', w) }}
      canShare={Boolean(session) && view.kind === 'workspace'} onShare={(s) => { setIncoming({ n: Date.now(), ...s }); flash(s.attachments?.length ? 'Added to your message' : 'Address added to your message') }} />
  )
  return (
    <div className={'app' + (sideBrowser ? ' browser-side' : showRight ? '' : ' no-right')} style={sideBrowser ? ({ '--browser-w': browserSize.side + 'px' } as CSSProperties) : undefined}>
      {sidebar}
      <div className="main-stack" style={browser.open && browserWhere === 'dock' ? ({ '--browser-h': browserSize.dock + 'px' } as CSSProperties) : undefined}>
        {center}
        {browser.open && browserWhere === 'dock' && browserPanel}
        {termOpen && hostId && host && (
          <TerminalDock hostId={hostId} hostName={host.name} folder={session?.workspace} enabled={Boolean(status?.features?.includes('terminal'))}
            height={termHeight} onHeight={(n) => { setTermHeight(n); localStorage.setItem('acportal.termHeight', String(n)) }} onClose={() => setTermOpen(false)}
            onOpenUrl={(u) => openBrowser(u)} onOpenSettings={() => setView({ kind: 'settings', section: 'Terminal' })} api={api} />
        )}
        {hostId && (
          <StatusBar entry={fleet[hostId]} branch={session ? (rootTask?.worktree?.branch ?? branchOf[session.workspace]) : undefined} agent={session ? agentName(session.agentId) : undefined}
            terminal={{ available: Boolean(status?.features?.includes('terminal')), open: termOpen, onToggle: toggleTerminal }} browser={{ open: browser.open, onToggle: toggleBrowser }} />
        )}
      </div>
      {sideBrowser && browserPanel}
      {!sideBrowser && showRight && (
        <RightPanel workspace={session?.workspace} tasks={tasks} rootTask={rootTask} timeline={tl} orchestrator={Boolean(status?.features?.includes('orchestrator'))}
          onOpenTask={(t) => setView({ kind: 'task', taskId: t.id })} onReview={(p) => setView({ kind: 'review', focus: p })} onMerge={setConfirmMerge} />
      )}
      {overlays}
    </div>
  )
}

type NewOpts = { load?: string; manual?: boolean; yolo?: boolean; read: boolean; write: boolean; terminal: boolean; worktree: boolean; branch?: string; newBranch: boolean; base?: string; mcp: boolean; firstPrompt?: string }
const defaultOpts: NewOpts = { read: true, write: true, terminal: true, worktree: false, newBranch: false, mcp: true }
