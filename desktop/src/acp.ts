// Turns acpd delivery envelopes into a conversation timeline (same ACP semantics as the
// Android reducers: dedupe by sequence, permission id and tool-call id).
export type DiffBlock = { path: string; oldText?: string | null; newText: string; unified?: string }
export type ToolItem = { kind: 'tool'; id: string; title: string; toolKind?: string; status?: string; diffs: DiffBlock[]; terminal?: string; host: boolean; autoReview?: { decision: string; layer?: string; reason?: string } }
export type PermissionItem = {
  kind: 'permission'
  id: string | number
  title: string
  toolKind?: string
  source: string
  lines: string[]
  cwd?: string
  options: { optionId: string; name: string; kind?: string }[]
  autoReview?: { decision: string; layer?: string; reason?: string; risk?: string }
  /** Host scope check: paths named outside the workspace, and whether it runs sandboxed. */
  scope?: { outside: string[]; sandboxed: boolean; agentTool: boolean; shell?: string; shellNote?: string }
  answered?: string
  diffs: DiffBlock[]
}
export type Item =
  | { kind: 'user'; id: string; text: string }
  | { kind: 'agent'; id: string; text: string }
  | { kind: 'thought'; id: string; text: string }
  | { kind: 'plan'; id: string; entries: { content: string; status: string }[] }
  | ToolItem
  | PermissionItem
  | { kind: 'notice'; id: string; text: string; tone?: 'warn' | 'error' }
  | { kind: 'elicitation'; id: string | number; message: string; schema?: ElicitSchema; mode?: string; url?: string; answered?: string }

export type ElicitField = { type?: string; title?: string; description?: string; enum?: string[]; enumNames?: string[]; oneOf?: { const: string; title?: string }[]; default?: unknown; minimum?: number; maximum?: number; format?: string }
export type ElicitSchema = { properties?: Record<string, ElicitField>; required?: string[] }
export type Usage = { used?: number; size?: number; cost?: { amount: number; currency: string } }
export type Timeline = { items: Item[]; seen: Set<number>; lastSeq: number; busy: boolean; promptId?: number; usage?: Usage; title?: string; gap?: boolean; currentMode?: string; config?: Record<string, string>
  /** The agent's latest full `configOptions` list (from `config_option_update`), replacing the one from session start. */
  configOptions?: unknown[]
  /** A model set with `session/set_model` (agents without `configOptions`). */
  currentModel?: string
  /** Option changes sent and not answered yet (request id -> option id), to undo a refused one. */
  configRequests?: Record<string, string> }

export const emptyTimeline = (): Timeline => ({ items: [], seen: new Set(), lastSeq: 0, busy: false })

type Json = Record<string, unknown>
const text = (c: unknown): string => {
  const o = c as Json | undefined
  if (!o) return ''
  if (o.type === 'text') return String(o.text ?? '')
  if (o.type === 'resource_link') return `[${o.name ?? o.uri}]`
  if (o.type === 'image') return '[image]'
  return ''
}

function toolContent(content: unknown): { diffs: DiffBlock[]; terminalText?: string } {
  const diffs: DiffBlock[] = []
  let terminalText: string | undefined
  for (const c of (content as Json[]) ?? []) {
    if (c.type === 'diff') diffs.push({ path: String(c.path), oldText: (c.oldText as string) ?? null, newText: String(c.newText ?? '') })
    else if (c.type === 'content') terminalText = (terminalText ?? '') + text(c.content)
  }
  return { diffs, terminalText }
}

function shellLines(tc: Json): { lines: string[]; cwd?: string } {
  const raw = (tc.rawInput ?? {}) as Json
  if (typeof raw.shellLine === 'string') return { lines: raw.shellLine.split('\n'), cwd: raw.cwd as string | undefined }
  if (typeof raw.command === 'string') {
    const args = Array.isArray(raw.args) ? (raw.args as string[]).join(' ') : ''
    return { lines: [(raw.command + ' ' + args).trim()], cwd: raw.cwd as string | undefined }
  }
  if (Array.isArray(raw.commands)) return { lines: raw.commands as string[] }
  return { lines: [] }
}

export function reduce(t: Timeline, env: Json): Timeline {
  const type = env.type as string
  if (type === 'replay_start') return { ...t, gap: Boolean(env.gap) || t.gap, busy: Boolean(env.processing) || t.busy }
  if (type === 'replay_complete') return t
  if (type === 'reconnect_required') return t
  if (type !== 'event' && type !== 'pending_permission') return t
  const seq = env.sequence as number | undefined
  if (typeof seq === 'number' && t.seen.has(seq)) return t
  // A new set, never the old one changed: React may run a state update twice (StrictMode does in
  // development), and a shared set would make the second run drop the event as already seen.
  const seen = typeof seq === 'number' ? new Set(t.seen).add(seq) : t.seen
  const next: Timeline = { ...t, seen, items: [...t.items], lastSeq: Math.max(t.lastSeq, seq ?? 0) }
  const m = env.message as Json
  const dir = env.direction as string | undefined
  const method = m.method as string | undefined
  const params = (m.params ?? {}) as Json
  if (dir === 'client' && method === 'session/prompt') {
    const blocks = (params.prompt as Json[]) ?? []
    const atts = blocks.filter((b) => b.type !== 'text').map((b) => (b.type === 'image' ? '🖼 image' : b.type === 'resource' ? '📄 ' + String(((b.resource as Json)?.uri as string) ?? '').split('/').pop() : '📎'))
    const txt = blocks.filter((b) => b.type === 'text').map(text).join('') + (atts.length ? '\n' + atts.join('  ') : '')
    next.items.push({ kind: 'user', id: `u${seq ?? Math.random()}`, text: txt })
    next.busy = true
    next.promptId = m.id as number
    return next
  }
  if (dir === 'client' && (method === 'session/set_config_option' || method === 'session/set_model' || method === 'session/set_mode') && m.id !== undefined) {
    const option = method === 'session/set_config_option' ? String(params.configId ?? '') : method === 'session/set_model' ? 'model' : 'mode'
    next.configRequests = { ...(next.configRequests ?? {}), [String(m.id)]: option }
    return next
  }
  if (dir === 'agent' && m.id !== undefined && !method && next.configRequests?.[String(m.id)] !== undefined) {
    // ACP: the reply to an option change carries the agent's complete option list, which may
    // differ in more than the changed option (another provider brings other models).
    const { [String(m.id)]: option, ...rest } = next.configRequests
    next.configRequests = rest
    const err = m.error as Json | undefined
    const list = (m.result as Json | undefined)?.configOptions
    if (Array.isArray(list)) {
      next.configOptions = list
      next.config = Object.fromEntries((list as Json[]).filter((o) => typeof o.id === 'string' && o.currentValue !== undefined).map((o) => [o.id as string, String(o.currentValue)]))
    } else if (err) {
      // Show the agent's value again instead of the one it refused.
      if (option === 'mode') next.currentMode = undefined
      else if (option === 'model' && next.config?.model === undefined) next.currentModel = undefined
      else if (next.config) { const { [option]: _, ...keep } = next.config; next.config = keep }
      next.items.push({ kind: 'notice', id: `n${seq ?? Math.random()}`, text: `The agent did not accept that ${option} (${err.code ?? 'error'}).`, tone: 'warn' })
    }
    return next
  }
  if (dir === 'client' && 'result' in m) {
    // the user's own permission answer, journaled
    const ei = next.items.findIndex((i) => i.kind === 'elicitation' && i.id === m.id)
    if (ei >= 0) {
      next.items[ei] = { ...(next.items[ei] as Extract<Item, { kind: 'elicitation' }>), answered: String(((m.result as Json)?.action as string) ?? 'answered') }
      return next
    }
    const idx = next.items.findIndex((i) => i.kind === 'permission' && i.id === m.id)
    if (idx >= 0) {
      const o = ((m.result as Json)?.outcome ?? {}) as Json
      next.items[idx] = { ...(next.items[idx] as PermissionItem), answered: (o.optionId as string) ?? 'cancelled' }
    }
    return next
  }
  if (method === 'session/update') {
    const u = params.update as Json
    const kind = u.sessionUpdate as string
    const last = next.items[next.items.length - 1]
    if (kind === 'agent_message_chunk' || kind === 'agent_thought_chunk') {
      const k = kind === 'agent_message_chunk' ? 'agent' : 'thought'
      const s = text(u.content)
      if (last && last.kind === k) next.items[next.items.length - 1] = { ...last, text: last.text + s }
      else next.items.push({ kind: k, id: `${k}${seq}`, text: s })
    } else if (kind === 'user_message_chunk') {
      const s = text(u.content)
      if (last && last.kind === 'user') next.items[next.items.length - 1] = { ...last, text: last.text + s }
      else next.items.push({ kind: 'user', id: `u${seq}`, text: s })
    } else if (kind === 'tool_call' || kind === 'tool_call_update') {
      const id = String(u.toolCallId)
      const meta = (dir !== 'agent' ? (u._meta as Json) : undefined) ?? (u._meta as Json) ?? {}
      const term = (meta.acpdTerminal as Json | undefined)?.output as string | undefined
      const { diffs, terminalText } = toolContent(u.content)
      const idx = next.items.findIndex((i) => i.kind === 'tool' && i.id === id)
      const prev = idx >= 0 ? (next.items[idx] as ToolItem) : undefined
      const ar = meta.acpdAutoReview as ToolItem['autoReview']
      const item: ToolItem = {
        kind: 'tool',
        id,
        title: (u.title as string) ?? prev?.title ?? 'Tool',
        toolKind: (u.kind as string) ?? prev?.toolKind,
        status: (u.status as string) ?? prev?.status,
        diffs: diffs.length ? diffs : prev?.diffs ?? [],
        terminal: term ?? terminalText ?? prev?.terminal,
        host: dir === 'host' || Boolean(prev?.host),
        autoReview: ar ?? prev?.autoReview
      }
      if (idx >= 0) next.items[idx] = item
      else next.items.push(item)
    } else if (kind === 'plan') {
      next.items = next.items.filter((i) => i.kind !== 'plan')
      next.items.push({ kind: 'plan', id: 'plan', entries: (u.entries as { content: string; status: string }[]) ?? [] })
    } else if (kind === 'usage_update') {
      next.usage = { used: u.used as number, size: u.size as number, cost: u.cost as Usage['cost'] }
    } else if (kind === 'session_info_update') {
      if (typeof u.title === 'string') next.title = u.title
    } else if (kind === 'current_mode_update') {
      next.currentMode = u.currentModeId as string
    } else if (kind === 'config_option_update' || kind === 'config_options_update') {
      const opts = (u.configOptions as { id: string; currentValue: string }[]) ?? []
      next.config = { ...(next.config ?? {}), ...Object.fromEntries(opts.map((o) => [o.id, o.currentValue])) }
      if (Array.isArray(u.configOptions)) next.configOptions = u.configOptions as unknown[]
    }
    return next
  }
  if (method === 'session/request_permission') {
    if (next.items.some((i) => i.kind === 'permission' && i.id === m.id)) return next
    const tc = (params.toolCall ?? {}) as Json
    const meta = (params._meta ?? {}) as Json
    const { lines, cwd } = shellLines(tc)
    next.items.push({
      kind: 'permission',
      id: m.id as string | number,
      title: (tc.title as string) ?? 'Permission requested',
      toolKind: tc.kind as string,
      source: (meta.acpdSource as string) ?? 'agent',
      lines,
      cwd,
      options: (params.options as PermissionItem['options']) ?? [],
      autoReview: meta.acpdAutoReview as PermissionItem['autoReview'],
      scope: scopeOf(meta.acpdScope, dir !== 'host'),
      diffs: toolContent(tc.content).diffs
    })
    return next
  }
  if (method === 'elicitation/create') {
    if (next.items.some((i) => i.kind === 'elicitation' && i.id === m.id)) return next
    next.items.push({ kind: 'elicitation', id: m.id as number, message: String(params.message ?? 'The agent asks for input'), schema: params.requestedSchema as ElicitSchema, mode: params.mode as string, url: params.url as string })
    return next
  }
  if (dir === 'agent' && m.id !== undefined && m.id === t.promptId && !method) {
    next.busy = false
    const err = m.error as Json | undefined
    const stop = (m.result as Json | undefined)?.stopReason as string | undefined
    if (err) next.items.push({ kind: 'notice', id: `n${seq}`, text: `Prompt failed (${err.code ?? 'error'})`, tone: 'error' })
    else if (stop && stop !== 'end_turn') next.items.push({ kind: 'notice', id: `n${seq}`, text: `Stopped: ${stop.replace('_', ' ')}`, tone: 'warn' })
    return next
  }
  return next
}

/** Every diff the session produced, newest version per path. */
export function changedFiles(t: Timeline): { path: string; added: number; removed: number; diff: DiffBlock }[] {
  const byPath = new Map<string, DiffBlock>()
  for (const i of t.items) {
    if (i.kind === 'tool' && i.status !== 'failed') for (const d of i.diffs) {
      const prev = byPath.get(d.path)
      byPath.set(d.path, prev ? { path: d.path, oldText: prev.oldText, newText: d.newText } : d)
    }
  }
  return [...byPath.values()].map((d) => {
    const s = lineStats(d)
    return { path: d.path, added: s.added, removed: s.removed, diff: d }
  })
}

export type DiffLine = { kind: 'ctx' | 'add' | 'del' | 'skip'; a?: number; b?: number; text: string }
/** Line diff (LCS, bounded) with 3 lines of context. */
export function diffLines(d: DiffBlock): DiffLine[] {
  if (d.unified !== undefined) return unifiedLines(d.unified)
  const a = (d.oldText ?? '').split('\n')
  const b = d.newText.split('\n')
  if (d.oldText == null || d.oldText === '') return b.map((text, i) => ({ kind: 'add', b: i + 1, text }))
  if (a.length * b.length > 4_000_000) return [...a.map((text, i) => ({ kind: 'del' as const, a: i + 1, text })), ...b.map((text, i) => ({ kind: 'add' as const, b: i + 1, text }))]
  const n = a.length, m = b.length
  const dp: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1])
  const out: DiffLine[] = []
  let i = 0, j = 0
  while (i < n || j < m) {
    if (i < n && j < m && a[i] === b[j]) out.push({ kind: 'ctx', a: ++i, b: ++j, text: a[i - 1] })
    else if (i < n && (j >= m || dp[i + 1][j] >= dp[i][j + 1])) out.push({ kind: 'del', a: ++i, text: a[i - 1] })
    else out.push({ kind: 'add', b: ++j, text: b[j - 1] })
  }
  const keep = out.map((l, k) => l.kind !== 'ctx' || out.slice(Math.max(0, k - 3), k + 4).some((x) => x.kind !== 'ctx'))
  const res: DiffLine[] = []
  out.forEach((l, k) => {
    if (keep[k]) res.push(l)
    else if (res[res.length - 1]?.kind !== 'skip') res.push({ kind: 'skip', text: '' })
  })
  return res
}
export function lineStats(d: DiffBlock): { added: number; removed: number } {
  let added = 0, removed = 0
  for (const l of diffLines(d)) {
    if (l.kind === 'add') added++
    if (l.kind === 'del') removed++
  }
  return { added, removed }
}

/** Lines of a unified diff (from git, e.g. the host's task worktree changes). */
export function unifiedLines(u: string): DiffLine[] {
  const out: DiffLine[] = []
  let a = 0, b = 0, started = false
  for (const l of u.split('\n')) {
    const h = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(l)
    if (h) { if (started) out.push({ kind: 'skip', text: '' } as DiffLine); a = +h[1]; b = +h[2]; started = true; continue }
    if (!started || l === '' || l.startsWith('\\')) continue
    if (l.startsWith('+')) out.push({ kind: 'add', b: b++, text: l.slice(1) })
    else if (l.startsWith('-')) out.push({ kind: 'del', a: a++, text: l.slice(1) })
    else out.push({ kind: 'ctx', a: a++, b: b++, text: l.slice(1) })
  }
  return out
}

function scopeOf(v: unknown, agent: boolean): PermissionItem['scope'] {
  if (!v || typeof v !== 'object') return undefined
  const o = v as Json
  const outside = Array.isArray(o.outsideWorkspace) ? (o.outsideWorkspace as unknown[]).filter((x): x is string => typeof x === 'string').slice(0, 16) : []
  // The agent's own tools never run in the host sandbox, whatever the metadata says.
  const text = (x: unknown, n: number): string | undefined => (typeof x === 'string' && x ? x.slice(0, n) : undefined)
  const shell = agent ? undefined : text(o.shell, 32)
  const shellNote = agent ? undefined : text(o.shellNote, 300)
  return { outside, sandboxed: !agent && o.sandboxed === true, agentTool: agent || o.agentTool === true, ...(shell ? { shell } : {}), ...(shellNote ? { shellNote } : {}) }
}
