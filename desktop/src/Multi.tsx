// "Run on several agents": the same prompt to 2-3 agents, each on its own branch in its own
// worktree; a side-by-side compare of diffs, checks, review and cost; merge one, discard the rest.
// The pre-merge checklist (configured test/lint lines run on the host, optional AI review) gates
// every merge: green merges, anything else needs an explicit override.
import React, { useEffect, useMemo, useState } from 'react'
import type { AgentInfo, SessionInfo, Workspace } from './types'
import { DiffView, I } from './ui'
import { type Checks, type ChecksConfig, type Review, type Run, type RunEntry, diffTotals, gate, money, reviewPrompt, runBranch, splitDiff, verdictOf } from './runs'

export type WatchSnap = { busy: boolean; conn: string; pending: unknown[]; usage?: { used?: number; size?: number; cost?: { amount: number; currency: string } }; reply?: string; turns: number }
const tild = (p: string): string => p.replace(/^\/(home|Users)\/[^/]+/, '~')
const api = <T,>(h: string, m: string, p: string, b?: unknown) => window.api.hosts.api(h, m, p, b) as Promise<{ ok: true; value: T } | { ok: false; error: string; code?: string }>

async function sendPrompt(hostId: string, s: { id: string; acpSessionId: string }, text: string): Promise<boolean> {
  const r = await window.api.watch.send(hostId, s.id, { jsonrpc: '2.0', id: 900_000 + Math.floor(Math.random() * 99_999), method: 'session/prompt', params: { sessionId: s.acpSessionId, prompt: [{ type: 'text', text }] } })
  return r.ok && r.value
}

export function MultiRunDialog({ hostId, agents, workspaces, onClose, onStarted }: { hostId: string; agents: AgentInfo[]; workspaces: Workspace[]; onClose: () => void; onStarted: (r: Run) => void }) {
  const usable = agents.filter((a) => a.enabled !== false && a.status !== 'missing')
  const [prompt, setPrompt] = useState('')
  const [root, setRoot] = useState(workspaces[0]?.path ?? '')
  const [sub, setSub] = useState('')
  const [base, setBase] = useState('')
  const [branches, setBranches] = useState<string[]>()
  const [picks, setPicks] = useState<string[]>([usable[0]?.id ?? '', usable[1]?.id ?? usable[0]?.id ?? '', ''])
  const [busy, setBusy] = useState<string>()
  const [err, setErr] = useState<string>()
  const target = sub.trim() ? `${root.replace(/\/$/, '')}/${sub.trim().replace(/^\/+/, '')}` : root
  useEffect(() => {
    setBranches(undefined)
    void (async () => {
      const r = await window.api.git.run(hostId, target, 'branch-list').catch(() => null)
      if (!r || !r.ok) { setBranches([]); return }
      const rows = r.value.out.split('\n').filter(Boolean).map((l) => l.split('\u001f'))
      const local = rows.filter((x) => !x[0].startsWith('origin/') && !x[0].includes('/HEAD'))
      setBranches(local.map((x) => x[0]))
      setBase(local.find((x) => x[2] === '*')?.[0] ?? local[0]?.[0] ?? '')
    })()
  }, [hostId, target])
  const chosen = picks.filter(Boolean)
  const go = async (): Promise<void> => {
    setErr(undefined)
    if (!prompt.trim()) { setErr('Write the prompt first.'); return }
    if (chosen.length < 2) { setErr('Pick at least two agents (the same agent twice is fine).'); return }
    const stamp = Date.now().toString(36).slice(-4)
    const entries: RunEntry[] = []
    for (const [i, a] of chosen.entries()) {
      const branch = runBranch(prompt, a, i + 1, stamp)
      setBusy(`Starting ${a} on ${branch}…`)
      const s = await api<SessionInfo>(hostId, 'POST', '/v1/sessions', { agentId: a, workspace: target, loadSessionId: null, workspaceAccess: { readFiles: true, writeFiles: true, terminal: true }, mcpServers: [], branch: { name: branch, create: true, worktree: true, ...(base ? { base } : {}) } })
      if (!s.ok) { entries.push({ agentId: a, sessionId: '', acpSessionId: '', branch, path: '', error: s.code === 'branch_failed' ? 'git could not create the worktree' : s.error }); continue }
      entries.push({ agentId: a, sessionId: s.value.id, acpSessionId: s.value.acpSessionId, branch, path: s.value.workspace })
    }
    setBusy('Sending the prompt to every run…')
    for (const e of entries) if (e.sessionId && !(await sendPrompt(hostId, { id: e.sessionId, acpSessionId: e.acpSessionId }, prompt))) e.error = 'Could not send the prompt; open the session and send it there'
    setBusy(undefined)
    onStarted({ id: `run-${stamp}`, hostId, root: target, base, prompt, createdAt: Date.now(), entries, state: 'running' })
  }
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="modal" style={{ width: 640 }} onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-h"><h3>Run on several agents</h3><button className="icon-btn" onClick={onClose} aria-label="Close">{I.close}</button></div>
        <div className="sub2">Every agent gets the same prompt on its own new branch in its own worktree, so nothing touches your checkout until you merge the one you pick.</div>
        <div className="field"><label>Prompt</label><textarea className="input" rows={4} value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="Add a --verbose flag to the CLI and a test for it" /></div>
        <div className="grid2">
          <div className="field"><label>Folder</label><select className="input mono" value={root} onChange={(e) => setRoot(e.target.value)}>{workspaces.map((w) => <option key={w.path} value={w.path}>{tild(w.path)}</option>)}</select></div>
          <div className="field"><label>Sub-folder <span className="sub">optional</span></label><input className="input mono" value={sub} onChange={(e) => setSub(e.target.value)} placeholder="e.g. notes" /></div>
        </div>
        <div className="field"><label>Start from branch</label>
          {branches === undefined ? <div className="hint">Reading branches…</div> : branches.length === 0 ? <div className="error">Not a git repository: each run needs its own worktree.</div>
            : <select className="input mono" value={base} onChange={(e) => setBase(e.target.value)}>{branches.map((b) => <option key={b}>{b}</option>)}</select>}
        </div>
        <div className="field"><label>Agents</label>
          <div className="grid3">
            {picks.map((p, i) => (
              <select key={i} className="input" value={p} onChange={(e) => setPicks(picks.map((x, j) => (j === i ? e.target.value : x)))}>
                {i === 2 && <option value="">— none —</option>}
                {usable.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
              </select>
            ))}
          </div>
          <div className="hint">Runs get branches like <span className="mono">{runBranch(prompt || 'task', chosen[0] ?? 'agent', 1, 'xxxx')}</span>. Approvals from every run show up in the sidebar and as notifications.</div>
        </div>
        {err && <div className="error">{err}</div>}
        <div style={{ display: 'flex', gap: 8, marginTop: 14, alignItems: 'center' }}>
          <span className="sub grow">{busy}</span>
          <button className="btn ghost" onClick={onClose}>Cancel</button>
          <button className="btn primary" disabled={Boolean(busy) || !branches?.length} onClick={() => void go()}>{I.flow}Start {chosen.length} runs</button>
        </div>
      </div>
    </div>
  )
}

type EntryState = { diff?: string; diffErr?: string; checks?: Checks; checking?: boolean; checkErr?: string; review?: Review; reviewIds?: string[] }

export function useChecksConfig(root: string): [ChecksConfig, (c: ChecksConfig) => void] {
  const [cfg, setCfg] = useState<ChecksConfig>({ commands: [], aiReview: false })
  useEffect(() => { void window.api.prefs.get().then((r) => { const all = (r.ok ? (r.value.checks as Record<string, ChecksConfig>) : undefined) ?? {}; setCfg(all[root] ?? { commands: [], aiReview: false }) }) }, [root])
  const save = (c: ChecksConfig): void => {
    setCfg(c)
    void window.api.prefs.get().then((r) => { const p = r.ok ? r.value : {}; const all = { ...((p.checks as Record<string, ChecksConfig>) ?? {}), [root]: c }; return window.api.prefs.set({ ...p, checks: all }) })
  }
  return [cfg, save]
}

/** Run the configured lines on the host in `path`. */
export async function runChecks(hostId: string, path: string, cfg: ChecksConfig): Promise<Checks> {
  const r = await api<Omit<Checks, 'at'>>(hostId, 'POST', '/v1/checks', { path, commands: cfg.commands })
  if (!r.ok) throw new Error(r.code === 'not_found' ? 'This host does not run checks (update acpd)' : r.error)
  return { ...r.value, at: Date.now() }
}

/** Start a read-only review session on `path` and send it the diff. */
export async function startReview(hostId: string, path: string, agent: string, diff: string, prompt: string): Promise<Review> {
  const s = await api<SessionInfo>(hostId, 'POST', '/v1/sessions', { agentId: agent, workspace: path, loadSessionId: null, workspaceAccess: { readFiles: true, writeFiles: false, terminal: false }, mcpServers: [] })
  if (!s.ok) return { state: 'unclear', text: s.error }
  const ok = await sendPrompt(hostId, s.value, reviewPrompt(diff, prompt))
  return ok ? { state: 'running', sessionId: s.value.id } : { state: 'unclear', sessionId: s.value.id, text: 'Could not send the review prompt' }
}

export function reviewFromWatch(r: Review | undefined, w: WatchSnap | undefined): Review | undefined {
  if (!r || r.state !== 'running' || !w) return r
  const v = verdictOf(w.reply)
  if (v) return { ...r, state: v, text: w.reply }
  if (!w.busy && w.turns > 0 && w.reply) return { ...r, state: 'unclear', text: w.reply }
  return r
}

export function ChecksEditor({ cfg, onSave, agents: all }: { cfg: ChecksConfig; onSave: (c: ChecksConfig) => void; agents: AgentInfo[] }) {
  const agents = all.filter((a) => a.enabled !== false && a.status !== 'missing')
  const [text, setText] = useState(cfg.commands.join('\n'))
  useEffect(() => setText(cfg.commands.join('\n')), [cfg.commands.join('\n')])
  return (
    <div className="checks-cfg">
      <label className="sub">Test and lint lines for this workspace, one per line (run on the host in the run's worktree, sandboxed where the host supports it)</label>
      <textarea className="input mono" rows={3} value={text} placeholder={'python3 -m unittest -v\nruff check .'} onChange={(e) => setText(e.target.value)} onBlur={() => onSave({ ...cfg, commands: text.split('\n').map((l) => l.trim()).filter(Boolean).slice(0, 8) })} />
      <div className="inline" style={{ marginTop: 6, gap: 10 }}>
        <label className="inline"><input type="checkbox" checked={cfg.aiReview} onChange={(e) => onSave({ ...cfg, aiReview: e.target.checked, reviewAgent: cfg.reviewAgent ?? agents[0]?.id })} /> AI diff review by</label>
        <select className="input" style={{ width: 180 }} disabled={!cfg.aiReview} value={cfg.reviewAgent ?? agents[0]?.id} onChange={(e) => onSave({ ...cfg, reviewAgent: e.target.value })}>{agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}</select>
        <span className="sub">read-only session, answers PASS or FAIL</span>
      </div>
    </div>
  )
}

export function GateBadge({ g }: { g: ReturnType<typeof gate> }) {
  const cls = g.state === 'green' ? 'green' : g.state === 'red' ? 'red' : 'grey'
  return <span className={`gate ${cls}`} title={g.why}>{g.state === 'green' ? I.check : g.state === 'red' ? I.close : I.clock}{g.state === 'green' ? 'Ready to merge' : g.state === 'red' ? 'Checks failed' : g.state === 'pending' ? 'Checks pending' : 'No checks'}</span>
}

export function ChecksResults({ c, review }: { c?: Checks; review?: Review }) {
  return (
    <div className="check-list">
      {c?.results.map((r) => (
        <details key={r.command} className={'check-row ' + (r.code === 0 ? 'ok' : 'bad')}>
          <summary><span className="ck">{r.code === 0 ? I.check : I.close}</span><span className="mono ell">{r.command}</span><span className="sub">{r.timedOut ? 'timed out' : `exit ${r.code ?? '?'}`} · {(r.ms / 1000).toFixed(1)} s</span></summary>
          <pre className="mono check-out">{r.output.slice(-6000) || '(no output)'}</pre>
        </details>
      ))}
      {c && <div className="sub">{c.sandboxed ? `Ran in the host sandbox (only the worktree writable)` : 'Host has no OS sandbox: ran unconfined'}</div>}
      {review && (
        <details className={'check-row ' + (review.state === 'pass' ? 'ok' : review.state === 'running' ? '' : 'bad')}>
          <summary><span className="ck">{review.state === 'pass' ? I.check : review.state === 'running' ? I.clock : I.close}</span><span>AI review</span><span className="sub">{review.state === 'running' ? 'reviewing…' : review.state === 'unclear' ? 'no verdict' : review.state.toUpperCase()}</span></summary>
          {review.text && <pre className="check-out">{review.text.slice(-4000)}</pre>}
        </details>
      )}
    </div>
  )
}

export function CompareView({ run, watch, agents, onBack, onOpen, onUpdate, flash }: {
  run: Run; watch: Record<string, WatchSnap>; agents: AgentInfo[]; onBack: () => void; onOpen: (sessionId: string) => void; onUpdate: (r: Run | null) => void; flash: (m: string) => void
}) {
  const [st, setSt] = useState<Record<string, EntryState>>({})
  const [cfg, setCfg] = useChecksConfig(run.root)
  const [winner, setWinner] = useState<string>()
  const [confirm, setConfirm] = useState<'override' | 'discard' | undefined>()
  const [merging, setMerging] = useState<string>()
  const live = run.entries.filter((e) => e.sessionId)
  const w = (e: RunEntry): WatchSnap | undefined => watch[`${run.hostId}/${e.sessionId}`]
  const patch = (b: string, p: Partial<EntryState>): void => setSt((s) => ({ ...s, [b]: { ...s[b], ...p } }))
  const loadDiff = async (e: RunEntry): Promise<string | undefined> => {
    // Stage the run's edits (new files too) so the diff against the base shows everything.
    const add = await window.api.git.run(run.hostId, e.path, 'add-all')
    if (!add.ok) { patch(e.branch, { diffErr: add.error }); return undefined }
    const r = await window.api.git.run(run.hostId, e.path, 'diff', { base: run.base, staged: '1' })
    if (!r.ok) { patch(e.branch, { diffErr: r.error }); return undefined }
    patch(e.branch, { diff: r.value.out, diffErr: undefined })
    return r.value.out
  }
  const busyKey = live.map((e) => `${e.branch}:${w(e)?.busy ? 1 : 0}`).join(',')
  useEffect(() => { if (run.state === 'running') for (const e of live) if (!w(e)?.busy) void loadDiff(e) }, [busyKey, run.state])
  // Fold review verdicts in from the watched review sessions.
  useEffect(() => {
    for (const e of live) {
      const r = st[e.branch]?.review
      if (r?.state === 'running' && r.sessionId) { const n = reviewFromWatch(r, watch[`${run.hostId}/${r.sessionId}`]); if (n !== r) patch(e.branch, { review: n }) }
    }
  }, [watch])
  const check = async (e: RunEntry): Promise<void> => {
    const diff = st[e.branch]?.diff ?? (await loadDiff(e)) ?? ''
    if (cfg.commands.length) {
      patch(e.branch, { checking: true, checkErr: undefined })
      try { patch(e.branch, { checks: await runChecks(run.hostId, e.path, cfg), checking: false }) } catch (x) { patch(e.branch, { checking: false, checkErr: (x as Error).message }) }
    }
    if (cfg.aiReview && cfg.reviewAgent) {
      const review = await startReview(run.hostId, e.path, cfg.reviewAgent, diff, run.prompt)
      setSt((x) => ({ ...x, [e.branch]: { ...x[e.branch], review, reviewIds: [...(x[e.branch]?.reviewIds ?? []), ...(review.sessionId ? [review.sessionId] : [])] } }))
    }
  }
  const g = (e: RunEntry) => gate(cfg, st[e.branch]?.checks, st[e.branch]?.review)
  const cleanup = async (keep?: RunEntry): Promise<string[]> => {
    const notes: string[] = []
    for (const e of run.entries) {
      if (e.sessionId) await window.api.hosts.api(run.hostId, 'DELETE', `/v1/sessions/${e.sessionId}`)
      // Every review session started for this run, not only the latest.
      for (const id of st[e.branch]?.reviewIds ?? []) await window.api.hosts.api(run.hostId, 'DELETE', `/v1/sessions/${id}`)
      if (!e.path) continue
      const rm = await window.api.git.run(run.hostId, run.root, 'worktree-remove', { path: e.path, force: '1' })
      if (!rm.ok) notes.push(`${e.branch}: ${rm.error}`)
      const del = await window.api.git.run(run.hostId, run.root, 'branch-delete', { branch: e.branch, force: e === keep ? '0' : '1' })
      if (!del.ok) notes.push(`${e.branch}: ${del.error}`)
    }
    return notes
  }
  const merge = async (e: RunEntry): Promise<void> => {
    setConfirm(undefined); setMerging(e.branch)
    try {
      const add = await window.api.git.run(run.hostId, e.path, 'add-all')
      if (!add.ok) throw new Error(add.error)
      const c = await window.api.git.run(run.hostId, e.path, 'commit', { message: `${run.prompt.split('\n')[0].slice(0, 60)} (${e.agentId}, picked from ${run.entries.length} runs)` })
      if (!c.ok && !/nothing to commit|no changes added/i.test(c.error)) throw new Error(c.error)
      const m = await window.api.git.run(run.hostId, run.root, 'merge', { branch: e.branch })
      if (!m.ok) throw new Error(`Merge stopped: ${m.error}. Nothing was discarded; resolve or abort in Git.`)
      const notes = await cleanup(e)
      onUpdate({ ...run, state: 'merged', winner: e.branch })
      flash(`Merged ${e.branch} into ${tild(run.root)}${notes.length ? ` · cleanup: ${notes.join('; ')}` : ' · other runs discarded'}`)
    } catch (x) { flash((x as Error).message) } finally { setMerging(undefined) }
  }
  const discardAll = async (): Promise<void> => { setConfirm(undefined); const notes = await cleanup(); onUpdate({ ...run, state: 'discarded' }); flash(notes.length ? `Discarded with notes: ${notes.join('; ')}` : 'All runs discarded') }
  const pick = run.entries.find((e) => e.branch === winner)
  const totals = useMemo(() => Object.fromEntries(run.entries.map((e) => [e.branch, diffTotals(st[e.branch]?.diff ?? '')])), [st])
  const name = (id: string): string => agents.find((a) => a.id === id)?.name ?? id
  const done = run.state !== 'running'
  return (
    <div className="main runs-compare">
      <div className="topbar">
        <button className="icon-btn" onClick={onBack} aria-label="Back" title="Back (Esc)">{I.back}</button>
        <div className="topbar-title"><h1>Compare runs</h1><div className="sub ell">{run.entries.length} agents · from <span className="mono">{run.base}</span> · {tild(run.root)}{done ? ` · ${run.state}${run.winner ? ` (${run.winner})` : ''}` : ''}</div></div>
        {!done && <button className="btn ghost danger" onClick={() => setConfirm('discard')}>{I.trash}Discard all</button>}
        {!done && <button className="btn primary" disabled={!pick || Boolean(merging)} onClick={() => pick && (g(pick).state === 'green' || g(pick).state === 'none' ? void merge(pick) : setConfirm('override'))}>{I.merge}{merging ? 'Merging…' : pick ? `Merge ${name(pick.agentId)}` : 'Pick a winner'}</button>}
      </div>
      <div className="cmp-prompt"><span className="sub">Prompt</span><div>{run.prompt}</div></div>
      {!done && <ChecksEditor cfg={cfg} onSave={setCfg} agents={agents} />}
      <div className="cmp-grid" style={{ gridTemplateColumns: `repeat(${run.entries.length}, minmax(300px, 1fr))` }}>
        {run.entries.map((e) => {
          const s = st[e.branch] ?? {}
          const ws = w(e)
          const t = totals[e.branch]
          const gg = g(e)
          return (
            <div key={e.branch} className={'cmp-col' + (winner === e.branch ? ' win' : '')}>
              <div className="cmp-h">
                <label className="inline grow"><input type="radio" name="winner" disabled={done || !e.sessionId} checked={winner === e.branch} onChange={() => setWinner(e.branch)} /><b>{name(e.agentId)}</b></label>
                <span className={'tag ' + (e.error ? 'red' : ws?.busy ? 'grey' : 'green')}>{e.error ? 'error' : ws?.pending.length ? `${ws.pending.length} waiting` : ws?.busy ? 'working' : done ? run.state : 'idle'}</span>
              </div>
              <div className="mono sub ell" title={e.path}>{e.branch}</div>
              {e.error && <div className="error">{e.error}</div>}
              <div className="cmp-stats">
                <div><b>{t.files}</b><span>files</span></div>
                <div><b className="add">+{t.add}</b><span>added</span></div>
                <div><b className="del">−{t.del}</b><span>removed</span></div>
                <div><b>{money(ws?.usage?.cost)}</b><span>{ws?.usage?.used ? `${Math.round(ws.usage.used / 1000)}k tokens` : 'cost'}</span></div>
              </div>
              <div className="inline" style={{ gap: 6, margin: '8px 0' }}>
                <GateBadge g={gg} />
                <span className="grow" />
                {e.sessionId && <button className="btn sm" onClick={() => onOpen(e.sessionId)}>Open</button>}
                {!done && e.sessionId && <button className="btn sm" disabled={s.checking || Boolean(ws?.busy) || (!cfg.commands.length && !cfg.aiReview)} title={ws?.busy ? 'Wait until the agent is idle' : undefined} onClick={() => void check(e)}>{s.checking ? 'Running…' : 'Run checks'}</button>}
              </div>
              {s.checkErr && <div className="error">{s.checkErr}</div>}
              <ChecksResults c={s.checks} review={s.review} />
              {ws?.reply && <div className="cmp-reply"><span className="sub">Agent's last message</span><div>{ws.reply.slice(-500)}</div></div>}
              {s.diffErr && <div className="error">{s.diffErr}</div>}
              <div className="cmp-diffs">{splitDiff(s.diff ?? '').map((d) => <DiffView key={d.path} d={d} maxLines={40} />)}{s.diff === '' && <div className="sub">No changes yet</div>}</div>
            </div>
          )
        })}
      </div>
      {confirm && (
        <div className="overlay" onMouseDown={() => setConfirm(undefined)}>
          <div className="modal" onMouseDown={(x) => x.stopPropagation()}>
            {confirm === 'override' && pick ? (
              <>
                <h3 style={{ margin: 0, fontSize: 17 }}>Merge without green checks?</h3>
                <p className="sub2"><b>{name(pick.agentId)}</b>: {g(pick).why}. Merging commits the run's changes on <span className="mono">{pick.branch}</span>, merges it into <span className="mono">{tild(run.root)}</span> and discards the other runs.</p>
                <div style={{ display: 'flex', gap: 8, marginTop: 16 }}><button className="btn-wide danger" onClick={() => void merge(pick)}>Merge anyway</button><button className="btn-wide ghost" onClick={() => setConfirm(undefined)}>Cancel</button></div>
              </>
            ) : (
              <>
                <h3 style={{ margin: 0, fontSize: 17 }}>Discard every run?</h3>
                <p className="sub2">Closes the {live.length} sessions, removes their worktrees and deletes their branches. Your checkout is not touched.</p>
                <div style={{ display: 'flex', gap: 8, marginTop: 16 }}><button className="btn-wide danger" onClick={() => void discardAll()}>Discard all</button><button className="btn-wide ghost" onClick={() => setConfirm(undefined)}>Cancel</button></div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

/** Pre-merge checklist inside a merge confirmation (task worktrees). */
export function MergeGate({ hostId, root, path, prompt, agents, onMerge, onCancel, children }: { hostId: string; root: string; path: string; prompt: string; agents: AgentInfo[]; onMerge: () => void; onCancel: () => void; children?: React.ReactNode }) {
  const [cfg, setCfg] = useChecksConfig(root)
  const [checks, setChecks] = useState<Checks>()
  const [review, setReview] = useState<Review>()
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string>()
  const [override, setOverride] = useState(false)
  const [snap, setSnap] = useState<Record<string, WatchSnap>>({})
  useEffect(() => window.api.watch.onState((s) => setSnap(s as Record<string, WatchSnap>)), [])
  useEffect(() => { if (review?.sessionId) { const n = reviewFromWatch(review, snap[`${hostId}/${review.sessionId}`]); if (n !== review) setReview(n) } }, [snap])
  const g = gate(cfg, checks, review)
  const run = async (): Promise<void> => {
    setBusy(true); setErr(undefined)
    try {
      if (cfg.commands.length) setChecks(await runChecks(hostId, path, cfg))
      if (cfg.aiReview && cfg.reviewAgent) {
        const d = await window.api.git.run(hostId, path, 'diff', { base: 'HEAD' })
        setReview(await startReview(hostId, path, cfg.reviewAgent, d.ok ? d.value.out : '', prompt))
      }
    } catch (x) { setErr((x as Error).message) } finally { setBusy(false) }
  }
  return (
    <>
      {children}
      <div className="gate-box">
        <div className="inline" style={{ gap: 8 }}><b className="grow">Pre-merge checklist</b><GateBadge g={g} /><button className="btn sm" disabled={busy || (!cfg.commands.length && !cfg.aiReview)} onClick={() => void run()}>{busy ? 'Running…' : 'Run checks'}</button></div>
        <ChecksEditor cfg={cfg} onSave={setCfg} agents={agents} />
        {err && <div className="error">{err}</div>}
        <ChecksResults c={checks} review={review} />
      </div>
      {override && <div className="error">{g.why}. Merge anyway?</div>}
      <div style={{ display: 'flex', gap: 8, marginTop: 16 }}>
        {g.state === 'green' || g.state === 'none' || override
          ? <button className={'btn-wide' + (override ? ' danger' : '')} onClick={onMerge}>{override ? 'Merge anyway' : 'Merge'}</button>
          : <button className="btn-wide" onClick={() => setOverride(true)}>Merge…</button>}
        <button className="btn-wide ghost" onClick={onCancel}>Cancel</button>
      </div>
    </>
  )
}
