import { ReactNode, useCallback, useEffect, useRef, useState } from 'react'
import { Branch, Compare, FileDiff, GraphRow, Status, Worktree, githubRepo, parseBranches, parseCompare, parseDiff, parseGraph, parseStatus, parseWorktrees } from './gitparse'
import { Empty, I, baseName, tildify } from './ui'

type LogEntry = { at: number; cwd: string; args: string[]; why: string; ok: boolean; ms: number }
/** One reversible step: how to undo it and how to do it again. */
type Step = { label: string; undoCmd: string; undo: () => Promise<unknown>; redoCmd: string; redo: () => Promise<unknown> }
type Tab = 'Changes' | 'Branches' | 'History' | 'Worktrees' | 'Stash' | 'Learn'
export const gitLog: LogEntry[] = []
const listeners = new Set<() => void>()
if (typeof window !== 'undefined' && window.api?.git) window.api.git.onLog((e) => { gitLog.unshift(e); gitLog.splice(200); listeners.forEach((l) => l()) })

/** A button that always explains the git command it runs, and shows it inline in Learn mode. */
function GitBtn({ label, cmd, why, onClick, primary, danger, learn, disabled, icon }: { label: ReactNode; cmd: string; why?: string; onClick: () => void; primary?: boolean; danger?: boolean; learn: boolean; disabled?: boolean; icon?: ReactNode }) {
  return (
    <span className="gitbtn" title={`${why ? why + '\n' : ''}$ ${cmd}`}>
      <button className={'btn' + (primary ? ' primary' : danger ? ' danger' : '')} disabled={disabled} onClick={onClick}>{icon}{label}</button>
      {learn && <code className="learn-cmd">$ {cmd}</code>}
    </span>
  )
}

const GRAPH_COLORS = ['var(--text)', 'var(--green)', 'var(--amber)', '#8fb7ff', 'var(--red)', '#c6aff5']
function GraphGlyphs({ g }: { g: string }) {
  return <span className="graph">{[...g].map((ch, i) => <span key={i} style={{ color: ch === '*' ? 'var(--text)' : GRAPH_COLORS[Math.floor(i / 2) % GRAPH_COLORS.length] }}>{ch === '*' ? '●' : ch}</span>)}</span>
}

// GitHub compare URLs keep the slashes of a branch name.
const refPath = (r: string) => r.split("/").map(encodeURIComponent).join("/")

export function GitView({ hostId, repos, onBack, suggest, agents, defaultAgent, askAgent, openSession, confirmDanger }: {
  hostId: string; repos: { path: string; label: string }[]; onBack: () => void
  /** A commit message for what is staged in `path`, from `agentId` (absent on older hosts). */
  suggest?: (path: string, agentId: string) => Promise<{ message?: string; error?: string }>
  agents: { id: string; name: string }[]; defaultAgent?: string
  askAgent: (prompt: string) => void; openSession: (path: string) => void
  confirmDanger?: (text: string) => boolean
}) {
  const sure = confirmDanger ?? ((t: string) => confirm(t))
  const [available, setAvailable] = useState<'host' | 'local' | false>()
  const [cwd, setCwd] = useState(repos[0]?.path ?? '')
  const [merging, setMerging] = useState(false)
  const [tab, setTab] = useState<Tab>('Changes')
  const [learn, setLearn] = useState(() => localStorage.getItem('acportal.learnGit') === '1')
  const [why, setWhy] = useState<Record<string, string>>({})
  const [status, setStatus] = useState<Status>()
  const [unstaged, setUnstaged] = useState<FileDiff[]>([])
  const [staged, setStaged] = useState<FileDiff[]>([])
  const [graph, setGraph] = useState<GraphRow[]>([])
  const [worktrees, setWorktrees] = useState<Worktree[]>([])
  const [branches, setBranches] = useState<Branch[]>([])
  const [remote, setRemote] = useState<string>()
  const [stashes, setStashes] = useState<{ ref: string; msg: string; when: string }[]>([])
  const [conflictDiffs, setConflictDiffs] = useState<Record<string, string>>({})
  const [msg, setMsg] = useState('')
  // Which agent writes suggested commit messages; remembered across visits.
  const [msgAgent, setMsgAgent] = useState(() => localStorage.getItem('acportal.commitAgent') ?? defaultAgent ?? agents[0]?.id ?? '')
  useEffect(() => { if (agents.length && !agents.some((a) => a.id === msgAgent)) setMsgAgent(defaultAgent && agents.some((a) => a.id === defaultAgent) ? defaultAgent : agents[0].id) }, [agents])
  const [err, setErr] = useState<string>()
  const [note, setNote] = useState<string>()
  const [undo, setUndo] = useState<Step[]>([])
  const [redo, setRedo] = useState<Step[]>([])
  const [busy, setBusy] = useState(false)
  const [, force] = useState(0)
  const [newWt, setNewWt] = useState('')
  const [menu, setMenu] = useState(false)

  useEffect(() => { const l = (): void => force((n) => n + 1); listeners.add(l); return () => void listeners.delete(l) }, [])
  useEffect(() => { void window.api.git.available(hostId).then((r) => setAvailable(r.ok ? r.value : false)); void window.api.git.explain().then((r) => r.ok && setWhy(r.value)) }, [hostId])
  useEffect(() => { localStorage.setItem('acportal.learnGit', learn ? '1' : '0') }, [learn])
  useEffect(() => { if (!repos.some((r) => r.path === cwd) && repos[0]) setCwd(repos[0].path) }, [repos])
  useEffect(() => { setUndo([]); setRedo([]) }, [cwd])

  const run = useCallback(async (op: string, params?: Record<string, string>, input?: string): Promise<{ out: string; args: string[] } | undefined> => {
    const r = await window.api.git.run(hostId, cwd, op, params, input)
    if (!r.ok) { setErr(r.error); return undefined }
    setErr(undefined)
    return r.value
  }, [hostId, cwd])
  const git = useCallback(async (op: string, params?: Record<string, string>, input?: string): Promise<string | undefined> => (await run(op, params, input))?.out, [run])

  const refresh = useCallback(async () => {
    if (!available || !cwd) return
    const s = await git('status')
    if (s === undefined) return
    const st = parseStatus(s)
    setStatus(st)
    const [m, d, ds, l, w, b, sl, ru] = await Promise.all([git('merging'), git('diff'), git('diff', { staged: '1' }), git('log'), git('worktree-list'), git('branch-list'), git('stash-list'), window.api.git.run(hostId, cwd, 'remote-url')])
    setMerging((m ?? '').trim() === 'yes')
    setUnstaged(parseDiff(d ?? '')); setStaged(parseDiff(ds ?? '')); setGraph(parseGraph(l ?? '')); setWorktrees(parseWorktrees(w ?? '')); setBranches(parseBranches(b ?? ''))
    setStashes((sl ?? '').split('\n').filter(Boolean).map((x) => { const [ref, mm, when] = x.split('\u001f'); return { ref, msg: mm, when } }))
    setRemote(ru.ok ? ru.value.out.trim() : undefined)
    const cd: Record<string, string> = {}
    for (const f of st.files.filter((x) => x.conflict)) cd[f.path] = (await git('diff', { file: f.path })) ?? ''
    setConflictDiffs(cd)
  }, [available, cwd, git, hostId])
  useEffect(() => { void refresh() }, [refresh])

  /** Runs one operation. With `inverse`, the step lands on the undo stack and can be redone. */
  async function act(label: string, op: string, params: Record<string, string> = {}, input?: string, inverse?: { cmd: string; run: () => Promise<unknown> }): Promise<boolean> {
    setBusy(true)
    const r = await run(op, params, input)
    setBusy(false)
    if (r === undefined) return false
    if (inverse) {
      setUndo((u) => [{ label, undoCmd: inverse.cmd, undo: inverse.run, redoCmd: r.args.join(' '), redo: () => git(op, params, input) }, ...u].slice(0, 30))
      setRedo([])
    }
    setNote(label)
    await refresh()
    return true
  }
  async function step(dir: 'undo' | 'redo'): Promise<void> {
    const from = dir === 'undo' ? undo : redo
    const s = from[0]
    if (!s) return
    setBusy(true)
    const r = await (dir === 'undo' ? s.undo() : s.redo())
    setBusy(false)
    if (r === undefined) return
    if (dir === 'undo') { setUndo((u) => u.slice(1)); setRedo((x) => [s, ...x]) } else { setRedo((x) => x.slice(1)); setUndo((u) => [s, ...u]) }
    setNote(`${dir === 'undo' ? 'Undid' : 'Redid'}: ${s.label}`)
    void refresh()
  }
  const head = async (): Promise<string> => ((await git('rev-parse')) ?? '').split('\n')[1] ?? ''

  // Ctrl Z / Ctrl Shift Z outside text fields
  const stepRef = useRef(step); stepRef.current = step
  useEffect(() => {
    const k = (e: KeyboardEvent): void => {
      const t = e.target as HTMLElement
      if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 'z' || /INPUT|TEXTAREA|SELECT/.test(t.tagName)) return
      e.preventDefault(); void stepRef.current(e.shiftKey ? 'redo' : 'undo')
    }
    window.addEventListener('keydown', k)
    return () => window.removeEventListener('keydown', k)
  }, [])

  const header = (title: ReactNode, sub: ReactNode, right?: ReactNode) => (
    <div className="topbar">
      <button className="icon-btn" onClick={onBack} aria-label="Back" title="Back (Esc)">{I.back}</button>
      <div className="topbar-title"><h1>{title}</h1><div className="sub">{sub}</div></div>
      {right}
    </div>
  )

  if (available === false)
    return (
      <div className="main">{header('Git', 'Not available for this host')}
        <Empty icon={I.repo} title="This host does not run git for the app">
          Update acpd on the host: hosts with the <span className="mono">git</span> feature answer <span className="mono">POST /v1/git/&lt;op&gt;</span>, scoped to their workspace folders. The app never runs git on another machine behind the host's back.
        </Empty>
      </div>
    )
  if (!cwd) return <div className="main">{header('Git', '')}<Empty icon={I.repo} title="No folder yet">Open a session or add a workspace folder on the host.</Empty></div>

  const files = status?.files ?? []
  const conflicts = files.filter((f) => f.conflict)
  const canCommit = staged.length > 0 || (merging && conflicts.length === 0)
  const untracked = files.filter((f) => f.untracked)
  const W = (op: string): string => why[op] ?? ''
  const local = branches.filter((b) => !b.remote)
  const gh = remote ? githubRepo(remote) : undefined
  const current = status?.branch ?? ''

  const HunkView = ({ f, stagedSide }: { f: FileDiff; stagedSide: boolean }) => (
    <div className="diff">
      <div className="diff-head">
        <span className="diff-path">{I.file}<span className="mono">{f.path}</span></span>
        <GitBtn learn={learn} label={stagedSide ? 'Unstage file' : 'Stage file'} cmd={stagedSide ? `git restore --staged -- ${f.path}` : `git add -- ${f.path}`} why={W(stagedSide ? 'restore-staged' : 'add')}
          onClick={() => void act(`${stagedSide ? 'Unstaged' : 'Staged'} ${f.path}`, stagedSide ? 'restore-staged' : 'add', { file: f.path }, undefined, { cmd: stagedSide ? `git add -- ${f.path}` : `git restore --staged -- ${f.path}`, run: () => git(stagedSide ? 'add' : 'restore-staged', { file: f.path }) })} />
      </div>
      {f.binary && <div className="sub" style={{ padding: 10 }}>Binary file</div>}
      {f.hunks.map((h, i) => (
        <div key={i}>
          <div className="hunk-head mono">
            <span>{h.header}</span>
            <GitBtn learn={learn} label={stagedSide ? 'Unstage hunk' : 'Stage hunk'} cmd={stagedSide ? 'git apply --cached -R <hunk>' : 'git apply --cached <hunk>'} why={W(stagedSide ? 'apply-cached-reverse' : 'apply-cached')}
              onClick={() => void act(`${stagedSide ? 'Unstaged' : 'Staged'} a hunk of ${f.path}`, stagedSide ? 'apply-cached-reverse' : 'apply-cached', {}, h.patch, { cmd: stagedSide ? 'git apply --cached <hunk>' : 'git apply --cached -R <hunk>', run: () => git(stagedSide ? 'apply-cached' : 'apply-cached-reverse', {}, h.patch) })} />
          </div>
          <div className="diff-body mono"><table><tbody>
            {h.lines.slice(1).map((l, k) => <tr key={k} className={l[0] === '+' ? 'add' : l[0] === '-' ? 'del' : ''}><td className="sign">{l[0] === '+' || l[0] === '-' ? l[0] : ''}</td><td className="t">{l.slice(1)}</td></tr>)}
          </tbody></table></div>
        </div>
      ))}
    </div>
  )

  const branchBtn = (
    <div className="pop-anchor">
      <button className="select-btn" onClick={() => setMenu(!menu)} title="Switch, create or manage branches">{I.branch}<span className="mono">{current || '…'}</span>{I.down}</button>
      {menu && <BranchMenu branches={local} current={current} learn={learn} onClose={() => setMenu(false)}
        onSwitch={(b) => { setMenu(false); void act(`Switched to ${b}`, 'checkout', { branch: b }, undefined, { cmd: `git switch ${current}`, run: () => git('checkout', { branch: current }) }) }}
        onCreate={(name, base) => { setMenu(false); void act(`Created ${name}${base ? ' from ' + base : ''}`, 'branch-create', { branch: name, ...(base ? { base } : {}) }, undefined, { cmd: `git switch ${current} && git branch -d ${name}`, run: async () => (await git('checkout', { branch: current })) !== undefined ? git('branch-delete', { branch: name }) : undefined }) }}
        onAll={() => { setMenu(false); setTab('Branches') }} />}
    </div>
  )

  return (
    <div className="main">
      {header(<>Git <span className="h-dim">·</span> {baseName(cwd)}</>,
        <>{status ? <>{status.upstream ? `tracking ${status.upstream}` : 'not published'}{status.ahead ? ` · ${status.ahead} ahead` : ''}{status.behind ? ` · ${status.behind} behind` : ''}</> : 'Reading…'} · <span className="mono">{tildify(cwd)}</span>{available === 'host' ? ' · run by the host' : ' · run on this computer'}</>,
        <>
          {branchBtn}
          <select className="select-btn mono" value={cwd} onChange={(e) => setCwd(e.target.value)} title="Repository or worktree">{repos.map((r) => <option key={r.path} value={r.path}>{r.label}</option>)}</select>
          <button className={'toggle-chip' + (learn ? ' on' : '')} onClick={() => setLearn(!learn)} title="Show the git command under every button and log each one with an explanation">{I.learn} Learn git</button>
          <button className="icon-btn" onClick={() => void refresh()} title="Refresh">{I.refresh}</button>
        </>)}
      <div className="tabs">
        {(['Changes', 'Branches', 'History', 'Worktrees', 'Stash', 'Learn'] as Tab[]).map((t) => {
          const n = t === 'Changes' ? files.length : t === 'Branches' ? local.length : t === 'Stash' ? stashes.length : t === 'Learn' ? gitLog.length : t === 'Worktrees' ? worktrees.length : 0
          return <button key={t} className={tab === t ? 'on' : ''} onClick={() => setTab(t)}>{t}{n ? <span className="count">{n}</span> : null}</button>
        })}
        <span style={{ flex: 1 }} />
        <span className="gitbtn" title={undo[0] ? `Undo: ${undo[0].label}\n$ ${undo[0].undoCmd}` : 'Nothing to undo'}>
          <button className="btn ghost" disabled={busy || !undo[0]} onClick={() => void step('undo')}>{I.undo}{undo[0] ? `Undo ${undo[0].label.length > 28 ? undo[0].label.slice(0, 27) + '…' : undo[0].label}` : 'Undo'}</button>
          {learn && undo[0] && <code className="learn-cmd">$ {undo[0].undoCmd}</code>}
        </span>
        <span className="gitbtn" title={redo[0] ? `Redo: ${redo[0].label}\n$ ${redo[0].redoCmd}` : 'Nothing to redo'}>
          <button className="btn ghost" disabled={busy || !redo[0]} onClick={() => void step('redo')}>{I.redo}Redo</button>
          {learn && redo[0] && <code className="learn-cmd">$ {redo[0].redoCmd}</code>}
        </span>
      </div>
      {err && <div className="banner red">{I.alert}<span>{err}</span></div>}
      {note && !err && <div className="banner">{I.check}<span>{note}</span></div>}
      <div className="scroll">
        {tab === 'Changes' && (
          <div className="git-grid">
            <div>
              {conflicts.length > 0 && (
                <div className="card warn" style={{ marginBottom: 16 }}>
                  <div className="card-h">{I.merge}<h3>Merge conflicts · {conflicts.length} file{conflicts.length > 1 ? 's' : ''}</h3></div>
                  <div className="sub2">Both sides changed the same lines. Pick a side per file, or ask the agent to combine them, then commit.</div>
                  {conflicts.map((f) => (
                    <div key={f.path} style={{ marginTop: 12 }}>
                      <div className="mono" style={{ marginBottom: 6 }}>{f.path}</div>
                      <pre className="term mono conflict">{(conflictDiffs[f.path] ?? '').split('\n').filter((l) => !l.startsWith('diff --cc') && !l.startsWith('index ')).map((l, i) => <span key={i} className={/^[ +-]{1,2}(<<<<<<<|=======|>>>>>>>)/.test(l) ? 'marker' : l.startsWith('++') || l.startsWith(' +') || l.startsWith('+ ') ? 'green' : l.startsWith('--') || l.startsWith(' -') || l.startsWith('- ') ? 'red' : ''}>{l + '\n'}</span>)}</pre>
                      <div className="btn-row">
                        <GitBtn learn={learn} label="Keep mine" cmd={`git checkout --ours -- ${f.path} && git add -- ${f.path}`} why={W('checkout-ours')} onClick={async () => { if (await act(`Kept our side of ${f.path}`, 'checkout-ours', { file: f.path })) await act(`Resolved ${f.path}`, 'add', { file: f.path }) }} />
                        <GitBtn learn={learn} label="Take theirs" cmd={`git checkout --theirs -- ${f.path} && git add -- ${f.path}`} why={W('checkout-theirs')} onClick={async () => { if (await act(`Took their side of ${f.path}`, 'checkout-theirs', { file: f.path })) await act(`Resolved ${f.path}`, 'add', { file: f.path }) }} />
                        <button className="btn" onClick={() => askAgent(`Resolve the git merge conflict in ${f.path} in ${cwd}: combine both sides so both intentions survive, remove the conflict markers, run the tests, then git add the file. Do not commit.`)}>{I.spark}Ask the agent to resolve</button>
                      </div>
                    </div>
                  ))}
                  <div style={{ marginTop: 14 }}><GitBtn learn={learn} danger label="Abort the merge" cmd="git merge --abort" why={W('merge-abort')} onClick={() => void act('Aborted the merge', 'merge-abort')} /></div>
                </div>
              )}
              <div className="group-h"><span>Unstaged</span><span className="count">{unstaged.length + untracked.length}</span></div>
              {unstaged.length + untracked.length === 0 && <div className="hint">Nothing unstaged. The working folder matches the index.</div>}
              {untracked.map((f) => (
                <div className="diff" key={f.path}><div className="diff-head"><span className="diff-path">{I.newfile}<span className="mono">{f.path}</span><span className="tag green">new</span></span>
                  <GitBtn learn={learn} label="Stage file" cmd={`git add -- ${f.path}`} why={W('add')} onClick={() => void act(`Staged ${f.path}`, 'add', { file: f.path }, undefined, { cmd: `git restore --staged -- ${f.path}`, run: () => git('restore-staged', { file: f.path }) })} /></div></div>
              ))}
              {unstaged.map((f) => <HunkView key={f.path} f={f} stagedSide={false} />)}
              <div className="group-h" style={{ marginTop: 18 }}><span>Staged</span><span className="count">{staged.length}</span></div>
              {staged.length === 0 && <div className="hint">Nothing staged yet. Stage a file or a single hunk to build the next commit.</div>}
              {staged.map((f) => <HunkView key={f.path} f={f} stagedSide />)}
            </div>
            <div className="card commit-box">
              <div className="card-h">{I.commit}<h3>Commit</h3></div>
              <div className="sub2">{staged.length ? <>{staged.length} file{staged.length > 1 ? 's' : ''} staged on <span className="mono">{current}</span></> : canCommit ? 'Conflicts resolved: commit to finish the merge' : 'Stage changes first'}</div>
              <textarea className="input area" placeholder="Summary of the change (first line ≤ 72 chars)" value={msg} onChange={(e) => setMsg(e.target.value)} />
              <div className={'counter' + (msg.split('\n')[0].length > 72 ? ' amber' : '')}>{msg.split('\n')[0].length}/72</div>
              <div className="btn-row">
                <button className="btn" disabled={!staged.length || busy || !suggest || !msgAgent}
                  title={suggest ? 'Ask the agent for a message from the staged changes, in a throwaway session (one small model request)' : 'Needs a newer acpd on the host'}
                  onClick={async () => {
                    if (!suggest) return
                    const name = agents.find((a) => a.id === msgAgent)?.name ?? msgAgent
                    setBusy(true); setErr(undefined); setNote(`Asking ${name} for a commit message…`)
                    const r = await suggest(cwd, msgAgent)
                    setBusy(false)
                    if (r.message) { setMsg(r.message); setNote(`Suggested by ${name}; edit before committing`) } else setNote(r.error ?? 'No suggestion')
                  }}>{I.spark}Suggest</button>
                {agents.length > 1 && (
                  <select className="select-btn" aria-label="Agent that writes the message" title="Agent that writes the message" value={msgAgent} disabled={busy}
                    onChange={(e) => { setMsgAgent(e.target.value); try { localStorage.setItem('acportal.commitAgent', e.target.value) } catch { /* ignore */ } }}>
                    {agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
                  </select>
                )}
                <span style={{ flex: 1 }} />
                <GitBtn learn={learn} primary label={`Commit to ${current || 'branch'}`} disabled={!canCommit || !msg.trim() || busy} cmd={`git commit -m "${msg.split('\n')[0].slice(0, 40)}"`} why={W('commit')}
                  onClick={async () => {
                    const before = await head()
                    const label = `commit “${msg.split('\n')[0].slice(0, 32)}”`
                    const ok = await act(`Committed ${label.slice(7)}`, 'commit', { message: msg })
                    if (!ok) return
                    const after = await head()
                    setMsg('')
                    const guard = async (want: string, rev: string): Promise<unknown> => ((await head()) === want ? git('reset-soft', { rev }) : (setErr('HEAD moved since then; this step would lose work, so it was skipped'), undefined))
                    setUndo((u) => [{ label, undoCmd: `git reset --soft ${before.slice(0, 8)}`, undo: () => guard(after, before), redoCmd: `git reset --soft ${after.slice(0, 8)}`, redo: () => guard(before, after) }, ...u])
                    setRedo([])
                  }} />
              </div>
              <div className="hint" style={{ marginTop: 12 }}>Undo of a commit uses <span className="mono">git reset --soft</span>, which keeps your changes staged; Redo moves the branch back to it. <span className="kbd">Ctrl Z</span> <span className="kbd">Ctrl Shift Z</span></div>
            </div>
          </div>
        )}
        {tab === 'Branches' && (
          <Branches branches={branches} current={current} status={status} learn={learn} W={W} gh={gh} git={git} sure={sure}
            act={act} onCompareError={setErr} />
        )}
        {tab === 'History' && (
          <div className="mono graph-list">
            {graph.map((r, i) => (
              <div key={i} className="graph-row" title={r.hash ? `git show ${r.hash}` : ''}>
                <GraphGlyphs g={r.graph} />
                {r.hash && <>
                  <span className="sub">{r.hash}</span>
                  {r.refs && r.refs.split(', ').map((ref) => <span key={ref} className={'ref' + (ref.startsWith('HEAD') ? ' head' : ref.includes('/') && !ref.startsWith('acpd/') && !ref.startsWith('acportal/') ? ' remote' : '')}>{ref.replace('HEAD -> ', '')}</span>)}
                  <span className="subj">{r.subject}</span>
                  <span className="sub">{r.author} · {r.when}</span>
                </>}
              </div>
            ))}
            {learn && <div className="learn-cmd" style={{ marginTop: 10 }}>$ git log --graph --all --date-order --oneline — every ● is a commit; lines show which branch it belongs to; merges join two lines.</div>}
          </div>
        )}
        {tab === 'Worktrees' && (
          <div className="narrow">
            <div className="hint" style={{ marginBottom: 12 }}>A worktree is another folder checked out from the same repository, on its own branch. Agents use them so parallel tasks never edit the same files.</div>
            <div className="list-card">
              {worktrees.map((w, i) => (
                <div className="list-row" key={w.path}>
                  <span className={'dot ' + (w.path === cwd ? 'green' : '')} />
                  <div className="grow">
                    <div><span className="branch-pill">{I.branch}{w.branch ?? (w.detached ? `detached at ${w.head}` : w.head)}</span>{i === 0 ? <span className="sub"> · main checkout</span> : ''}{w.locked ? ' · locked' : ''}{w.prunable ? ' · missing folder' : ''}</div>
                    <div className="sub mono">{tildify(w.path)}</div>
                  </div>
                  <button className="btn" onClick={() => openSession(w.path)}>Session here</button>
                  {i > 0 && !w.path.includes('/worktrees/') && <GitBtn learn={learn} danger label="Remove" cmd={`git worktree remove ${tildify(w.path)}`} why={W('worktree-remove')} onClick={() => { if (sure(`Remove the folder ${w.path}? Its branch ${w.branch ?? ''} stays.`)) void act(`Removed worktree ${baseName(w.path)}`, 'worktree-remove', { path: w.path }) }} />}
                </div>
              ))}
            </div>
            <div className="btn-row" style={{ marginTop: 14 }}>
              <input className="input mono" style={{ flex: 1 }} placeholder="new-branch-name" value={newWt} onChange={(e) => setNewWt(e.target.value.replace(/[^\w./-]/g, '-'))} />
              <GitBtn learn={learn} icon={I.plus} label="New worktree" disabled={!newWt} cmd={`git worktree add -b ${newWt || '<branch>'} ../${baseName(cwd)}-${newWt.replace(/\//g, '-') || '<branch>'}`} why={W('worktree-add')}
                onClick={() => { const p = cwd.replace(/[\\/][^\\/]+$/, '') + '/' + baseName(cwd) + '-' + newWt.replace(/\//g, '-'); void act(`Created worktree ${newWt}`, 'worktree-add', { branch: newWt, path: p }).then((ok) => ok && setNewWt('')) }} />
            </div>
            <div className="hint" style={{ marginTop: 18 }}>acpd names delegated task branches <span className="mono">acpd/task-…</span>; merge them from the task view so the host can check the parent folder first.</div>
          </div>
        )}
        {tab === 'Stash' && (
          <div className="narrow">
            <div className="hint" style={{ marginBottom: 12 }}>The stash is a shelf for unfinished work: it cleans the folder without committing, and you can bring the changes back later.</div>
            <GitBtn learn={learn} icon={I.stash} label="Stash all changes" disabled={!files.length} cmd="git stash push --include-untracked" why={W('stash-push')} onClick={() => void act('Stashed changes', 'stash-push', { message: `acportal ${new Date().toLocaleString()}` }, undefined, { cmd: 'git stash pop', run: () => git('stash-pop') })} />
            <div className="list-card" style={{ marginTop: 14 }}>
              {stashes.length === 0 && <div className="list-row sub">The stash is empty.</div>}
              {stashes.map((s) => (
                <div className="list-row" key={s.ref}>
                  <span className="mono sub">{s.ref}</span>
                  <div className="grow"><div>{s.msg}</div><div className="sub">{s.when}</div></div>
                  <GitBtn learn={learn} label="Apply" cmd={`git stash apply ${s.ref}`} why={W('stash-apply')} onClick={() => void act(`Applied ${s.ref}`, 'stash-apply', { ref: s.ref })} />
                  <GitBtn learn={learn} label="Pop" cmd={`git stash pop ${s.ref}`} why={W('stash-pop')} onClick={() => void act(`Popped ${s.ref}`, 'stash-pop', { ref: s.ref })} />
                  <GitBtn learn={learn} danger label="Drop" cmd={`git stash drop ${s.ref}`} why={W('stash-drop')} onClick={() => { if (sure(`Delete ${s.ref}? This cannot be undone from the app.`)) void act(`Dropped ${s.ref}`, 'stash-drop', { ref: s.ref }) }} />
                </div>
              ))}
            </div>
          </div>
        )}
        {tab === 'Learn' && (
          <div className="narrow">
            <div className="hint" style={{ marginBottom: 12 }}>Every git command the app ran for you, newest first, with what it does. Copy any line into a terminal to do the same by hand.</div>
            {gitLog.length === 0 && <div className="hint">Nothing yet. Stage, commit or stash something.</div>}
            <div className="list-card">
              {gitLog.map((e, i) => (
                <div key={i} className="learn-row">
                  <code className={e.ok ? '' : 'red'}>$ {e.args.join(' ')}</code>
                  <div className="sub2">{e.why}</div>
                  <div className="sub">{new Date(e.at).toLocaleTimeString()} · {baseName(e.cwd)} · {e.ms} ms{e.ok ? '' : ' · failed'}</div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

/** Quick branch switcher (GitHub Desktop's "Current branch" menu). */
function BranchMenu({ branches, current, learn, onSwitch, onCreate, onAll, onClose }: { branches: Branch[]; current: string; learn: boolean; onSwitch: (b: string) => void; onCreate: (name: string, base?: string) => void; onAll: () => void; onClose: () => void }) {
  const [q, setQ] = useState('')
  const [base, setBase] = useState(current)
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => { const h = (e: MouseEvent): void => { if (ref.current && !ref.current.contains(e.target as Node)) onClose() }; setTimeout(() => document.addEventListener('mousedown', h)); return () => document.removeEventListener('mousedown', h) }, [])
  const name = q.trim().replace(/\s+/g, '-')
  const shown = branches.filter((b) => b.name.toLowerCase().includes(q.toLowerCase()))
  const exists = branches.some((b) => b.name === name)
  return (
    <div className="popover branch-menu" ref={ref} onKeyDown={(e) => e.key === 'Escape' && onClose()}>
      <div className="pop-search">{I.search}<input autoFocus placeholder="Filter or name a new branch" value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter' && name && !exists) onCreate(name, base !== current ? base : undefined) }} /></div>
      <div className="pop-list">
        {shown.map((b) => (
          <button key={b.name} className={'pop-item' + (b.current ? ' on' : '')} onClick={() => !b.current && onSwitch(b.name)} title={`$ git switch ${b.name}`}>
            <span className="pop-check">{b.current ? I.check : null}</span><span className="mono grow">{b.name}</span><span className="sub">{b.track || b.when}</span>
          </button>
        ))}
        {shown.length === 0 && <div className="pop-empty sub">No branch matches.</div>}
      </div>
      {name && !exists && (
        <div className="pop-foot">
          <div className="btn-row" style={{ marginTop: 0 }}>
            <span className="sub">from</span>
            <select className="select-btn mono" value={base} onChange={(e) => setBase(e.target.value)}>{branches.map((b) => <option key={b.name} value={b.name}>{b.name}</option>)}</select>
            <span style={{ flex: 1 }} />
            <button className="btn primary" onClick={() => onCreate(name, base !== current ? base : undefined)}>{I.plus}Create <span className="mono">{name}</span></button>
          </div>
          {learn && <code className="learn-cmd" style={{ marginTop: 6 }}>$ git switch -c {name}{base !== current ? ' ' + base : ''}</code>}
        </div>
      )}
      <button className="pop-all" onClick={onAll}>Manage branches…</button>
    </div>
  )
}

function Branches({ branches, current, status, learn, W, gh, git, act, sure, onCompareError }: {
  branches: Branch[]; current: string; status?: Status; learn: boolean; W: (op: string) => string; gh?: string; sure: (t: string) => boolean
  git: (op: string, p?: Record<string, string>) => Promise<string | undefined>
  act: (label: string, op: string, params?: Record<string, string>, input?: string, inverse?: { cmd: string; run: () => Promise<unknown> }) => Promise<boolean>
  onCompareError: (e?: string) => void
}) {
  const local = branches.filter((b) => !b.remote)
  const remotes = branches.filter((b) => b.remote)
  const defaultBase = local.find((b) => b.name === 'main')?.name ?? local.find((b) => b.name === 'master')?.name ?? local[0]?.name ?? ''
  const [base, setBase] = useState(defaultBase)
  const [cmp, setCmp] = useState<{ branch: string; base: string; c: Compare }>()
  const [renaming, setRenaming] = useState<string>()
  const [newName, setNewName] = useState('')
  const [create, setCreate] = useState('')
  useEffect(() => { if (!base && defaultBase) setBase(defaultBase) }, [defaultBase])
  const compare = async (): Promise<void> => {
    const out = await git('compare', { base })
    if (out !== undefined) setCmp({ branch: current, base, c: parseCompare(out) }); else onCompareError(undefined)
  }
  const published = Boolean(status?.upstream)
  const prUrl = gh && current && base && current !== base ? `${gh}/compare/${refPath(base.replace(/^origin\//, ''))}...${refPath(current)}?expand=1` : undefined
  return (
    <div className="narrow">
      <div className="card" style={{ marginBottom: 16 }}>
        <div className="card-h">{I.branch}<h3>On <span className="mono">{current}</span></h3><span style={{ flex: 1 }} />
          {status?.upstream ? <span className="tag grey">tracking {status.upstream}{status.ahead ? ` · ${status.ahead} to push` : ''}{status.behind ? ` · ${status.behind} behind` : ''}</span> : <span className="tag grey">only on this computer</span>}
        </div>
        <div className="btn-row">
          <span className="sub">Compare with</span>
          <select className="select-btn mono" value={base} onChange={(e) => setBase(e.target.value)}>{branches.filter((b) => b.name !== current).map((b) => <option key={b.name} value={b.name}>{b.name}</option>)}</select>
          <GitBtn learn={learn} icon={I.compare} label="Compare" disabled={!base || base === current} cmd={`git log --left-right ${base}...HEAD`} why={W('compare')} onClick={() => void compare()} />
          <span style={{ flex: 1 }} />
          <GitBtn learn={learn} icon={I.upload} primary={!published || Boolean(status?.ahead)} label={published ? 'Push' : 'Publish branch'} disabled={!current} cmd={`git push --set-upstream origin ${current}`} why={W('push')}
            onClick={() => void act(published ? `Pushed ${current}` : `Published ${current}`, 'push', { branch: current })} />
          {prUrl && <button className="btn" title={prUrl} onClick={() => window.open(prUrl)}>{I.pr}Open pull request</button>}
        </div>
        {!gh && <div className="hint" style={{ marginTop: 8 }}>{status ? 'Pull request links appear when origin is on GitHub.' : ''} Push uses the host's own git credentials; the app stores none.</div>}
        {cmp && cmp.branch === current && (
          <div className="compare">
            <div><div className="group-h"><span>Only on {cmp.branch}</span><span className="count">{cmp.c.ahead.length}</span></div>
              {cmp.c.ahead.map((c) => <div key={c.hash} className="cmp-row"><span className="mono sub">{c.hash}</span><span className="grow">{c.subject}</span><span className="sub">{c.when}</span></div>)}
              {!cmp.c.ahead.length && <div className="hint">Nothing new on this branch.</div>}</div>
            <div><div className="group-h"><span>Only on {cmp.base}</span><span className="count">{cmp.c.behind.length}</span></div>
              {cmp.c.behind.map((c) => <div key={c.hash} className="cmp-row"><span className="mono sub">{c.hash}</span><span className="grow">{c.subject}</span><span className="sub">{c.when}</span></div>)}
              {!cmp.c.behind.length && <div className="hint">Up to date with {cmp.base}.</div>}</div>
          </div>
        )}
      </div>
      <div className="group-h"><span>Local branches</span><span className="count">{local.length}</span></div>
      <div className="list-card">
        {local.map((b) => (
          <div className="list-row" key={b.name}>
            <span className={'dot ' + (b.current ? 'green' : '')} />
            {renaming === b.name ? (
              <>
                <input className="input mono grow" autoFocus value={newName} onChange={(e) => setNewName(e.target.value.replace(/\s+/g, '-'))} />
                <GitBtn learn={learn} primary label="Rename" disabled={!newName || newName === b.name} cmd={`git branch -m ${b.name} ${newName}`} why={W('branch-rename')}
                  onClick={() => void act(`Renamed ${b.name} to ${newName}`, 'branch-rename', { from: b.name, to: newName }, undefined, { cmd: `git branch -m ${newName} ${b.name}`, run: () => git('branch-rename', { from: newName, to: b.name }) }).then((ok) => ok && setRenaming(undefined))} />
                <button className="btn ghost" onClick={() => setRenaming(undefined)}>Cancel</button>
              </>
            ) : (
              <>
                <div className="grow"><div className="mono">{b.name}{b.current && <span className="tag green" style={{ marginLeft: 8 }}>current</span>}</div><div className="sub">{b.hash} · {b.when}{b.track ? ` · ${b.track}` : ''}</div></div>
                {!b.current && <GitBtn learn={learn} label="Switch" cmd={`git switch ${b.name}`} why={W('checkout')} onClick={() => void act(`Switched to ${b.name}`, 'checkout', { branch: b.name }, undefined, { cmd: `git switch ${current}`, run: () => git('checkout', { branch: current }) })} />}
                <button className="icon-btn" title={`Rename ${b.name}\n$ git branch -m ${b.name} <new>`} onClick={() => { setRenaming(b.name); setNewName(b.name) }}>{I.pencil}</button>
                {!b.current && <button className="icon-btn danger" title={`Delete ${b.name}\n$ git branch -d ${b.name}`} onClick={() => {
                  if (!sure(`Delete the branch ${b.name}? Git refuses if it has commits that are not merged anywhere.`)) return
                  void act(`Deleted ${b.name}`, 'branch-delete', { branch: b.name }, undefined, { cmd: `git branch ${b.name} ${b.hash}`, run: () => git('branch-create', { branch: b.name, base: b.hash }).then((r) => r !== undefined ? git('checkout', { branch: current }) : r) })
                }}>{I.trash}</button>}
              </>
            )}
          </div>
        ))}
      </div>
      <div className="btn-row" style={{ marginTop: 12 }}>
        <input className="input mono" style={{ flex: 1 }} placeholder="new-branch-name" value={create} onChange={(e) => setCreate(e.target.value.replace(/\s+/g, '-'))} />
        <span className="sub">from</span>
        <select className="select-btn mono" value={base} onChange={(e) => setBase(e.target.value)}>{branches.map((b) => <option key={b.name} value={b.name}>{b.name}</option>)}</select>
        <GitBtn learn={learn} icon={I.plus} label="Create and switch" disabled={!create} cmd={`git switch -c ${create || '<name>'} ${base}`} why={W('branch-create')}
          onClick={() => void act(`Created ${create} from ${base}`, 'branch-create', { branch: create, base }, undefined, { cmd: `git switch ${current} && git branch -d ${create}`, run: async () => (await git('checkout', { branch: current })) !== undefined ? git('branch-delete', { branch: create, force: '1' }) : undefined }).then((ok) => ok && setCreate(''))} />
      </div>
      {remotes.length > 0 && <>
        <div className="group-h" style={{ marginTop: 20 }}><span>Remote branches</span><span className="count">{remotes.length}</span></div>
        <div className="list-card">{remotes.map((b) => <div className="list-row" key={b.name}><span className="dot" /><span className="mono grow">{b.name}</span><span className="sub">{b.when}</span></div>)}</div>
      </>}
    </div>
  )
}
