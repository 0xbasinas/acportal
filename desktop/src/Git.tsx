import { ReactNode, useCallback, useEffect, useState } from 'react'
import { FileDiff, GraphRow, Status, Worktree, parseDiff, parseGraph, parseStatus, parseWorktrees } from './gitparse'
import { I, baseName, tildify } from './ui'

type LogEntry = { at: number; cwd: string; args: string[]; why: string; ok: boolean; ms: number }
type Undo = { label: string; cmd: string; run: () => Promise<string | undefined> }
type Tab = 'Changes' | 'History' | 'Worktrees' | 'Stash' | 'Learn'
export const gitLog: LogEntry[] = []
const listeners = new Set<() => void>()
if (typeof window !== 'undefined' && window.api?.git) window.api.git.onLog((e) => { gitLog.unshift(e); gitLog.splice(200); listeners.forEach((l) => l()) })

/** A button that always explains the git command it runs, and shows it inline in Learn mode. */
function GitBtn({ label, cmd, why, onClick, primary, learn, disabled }: { label: ReactNode; cmd: string; why?: string; onClick: () => void; primary?: boolean; learn: boolean; disabled?: boolean }) {
  return (
    <span className="gitbtn" title={`${why ? why + '\n' : ''}$ ${cmd}`}>
      <button className={'pill' + (primary ? ' primary' : '')} disabled={disabled} onClick={onClick}>{label}</button>
      {learn && <code className="learn-cmd">$ {cmd}</code>}
    </span>
  )
}

const GRAPH_COLORS = ['#eeeeee', '#72d5b0', '#e8b04a', '#8fb7ff', '#f07474', '#c6aff5']
function GraphGlyphs({ g }: { g: string }) {
  return <span className="graph">{[...g].map((ch, i) => <span key={i} style={{ color: ch === '*' ? 'var(--text)' : GRAPH_COLORS[Math.floor(i / 2) % GRAPH_COLORS.length] }}>{ch === '*' ? '●' : ch}</span>)}</span>
}

export function GitView({ hostId, repos, onBack, suggest, askAgent, openSession }: {
  hostId: string; repos: { path: string; label: string }[]; onBack: () => void
  suggest: (diff: string) => Promise<string | undefined>; askAgent: (prompt: string) => void; openSession: (path: string) => void
}) {
  const [available, setAvailable] = useState<boolean>()
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
  const [stashes, setStashes] = useState<{ ref: string; msg: string; when: string }[]>([])
  const [conflictDiffs, setConflictDiffs] = useState<Record<string, string>>({})
  const [msg, setMsg] = useState('')
  const [err, setErr] = useState<string>()
  const [note, setNote] = useState<string>()
  const [undo, setUndo] = useState<Undo[]>([])
  const [busy, setBusy] = useState(false)
  const [, force] = useState(0)
  const [newWt, setNewWt] = useState('')

  useEffect(() => { const l = (): void => force((n) => n + 1); listeners.add(l); return () => void listeners.delete(l) }, [])
  useEffect(() => { void window.api.git.available(hostId).then((r) => setAvailable(r.ok && r.value)); void window.api.git.explain().then((r) => r.ok && setWhy(r.value)) }, [hostId])
  useEffect(() => { localStorage.setItem('acportal.learnGit', learn ? '1' : '0') }, [learn])
  useEffect(() => { if (!repos.some((r) => r.path === cwd) && repos[0]) setCwd(repos[0].path) }, [repos])

  const git = useCallback(async (op: string, params?: Record<string, string>, input?: string): Promise<string | undefined> => {
    const r = await window.api.git.run(hostId, cwd, op, params, input)
    if (!r.ok) { setErr(r.error); return undefined }
    setErr(undefined)
    return r.value.out
  }, [hostId, cwd])

  const refresh = useCallback(async () => {
    if (!available || !cwd) return
    const s = await git('status')
    if (s === undefined) return
    const st = parseStatus(s)
    setStatus(st)
    setMerging(((await git('merging')) ?? '').trim() === 'yes')
    setUnstaged(parseDiff((await git('diff')) ?? ''))
    setStaged(parseDiff((await git('diff', { staged: '1' })) ?? ''))
    setGraph(parseGraph((await git('log')) ?? ''))
    setWorktrees(parseWorktrees((await git('worktree-list')) ?? ''))
    setStashes(((await git('stash-list')) ?? '').split('\n').filter(Boolean).map((l) => { const [ref, m, when] = l.split('\u001f'); return { ref, msg: m, when } }))
    const cd: Record<string, string> = {}
    for (const f of st.files.filter((x) => x.conflict)) cd[f.path] = (await git('diff', { file: f.path })) ?? ''
    setConflictDiffs(cd)
  }, [available, cwd, git])
  useEffect(() => { void refresh() }, [refresh])

  async function act(label: string, op: string, params: Record<string, string> = {}, input?: string, inverse?: Omit<Undo, 'label'>): Promise<boolean> {
    setBusy(true)
    const out = await git(op, params, input)
    setBusy(false)
    if (out === undefined) return false
    if (inverse) setUndo((u) => [{ label, ...inverse }, ...u].slice(0, 20))
    setNote(label)
    await refresh()
    return true
  }
  const head = async (): Promise<string> => ((await git('rev-parse')) ?? '').split('\n')[1] ?? ''

  if (available === false)
    return (
      <div className="main"><div className="topbar"><button className="icon-btn" onClick={onBack}>{I.back}</button><h1>Git</h1></div>
        <div className="empty"><div style={{ maxWidth: 560 }}><h2>Git needs a host on this computer</h2>
          <div>acpd does not expose git yet, and the app will not run git on another machine behind the host's back. For remote hosts, ask the agent (“show git status”) or use the task review and merge.</div>
          <div className="sub" style={{ marginTop: 12 }}>Proposed host routes: <span className="mono">GET /v1/git/status · /diff · /log · /worktrees · /stashes</span> and <span className="mono">POST /v1/git/stage · /commit</span>, scoped to workspace roots.</div></div></div></div>
    )
  if (!cwd) return <div className="main"><div className="empty"><div><h2>No folder</h2><div>Open a session first.</div></div></div></div>

  const files = status?.files ?? []
  const conflicts = files.filter((f) => f.conflict)
  const canCommit = staged.length > 0 || (merging && conflicts.length === 0)
  const untracked = files.filter((f) => f.untracked)
  const undoTop = undo[0]
  const W = (op: string): string => why[op] ?? ''

  const HunkView = ({ f, stagedSide }: { f: FileDiff; stagedSide: boolean }) => (
    <div className="diff" style={{ marginBottom: 10 }}>
      <div className="diff-head">
        <span className="mono">{f.path}</span>
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
            {h.lines.slice(1).map((l, k) => <tr key={k} className={l[0] === '+' ? 'add' : l[0] === '-' ? 'del' : ''}><td className="t">{l}</td></tr>)}
          </tbody></table></div>
        </div>
      ))}
    </div>
  )

  return (
    <div className="main">
      <div className="topbar">
        <button className="icon-btn" onClick={onBack} aria-label="Back">{I.back}</button>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h1>Git · {baseName(cwd)}</h1>
          <div className="sub">{status ? <>On <span className="mono">{status.branch}</span>{status.upstream ? ` · tracking ${status.upstream}` : ''}{status.ahead ? ` · ${status.ahead} ahead` : ''}{status.behind ? ` · ${status.behind} behind` : ''}</> : 'Reading…'} · {tildify(cwd)}</div>
        </div>
        <select className="mode-select mono" value={cwd} onChange={(e) => setCwd(e.target.value)} title="Repository or worktree">{repos.map((r) => <option key={r.path} value={r.path}>{r.label}</option>)}</select>
        <label className="chip" style={{ cursor: 'pointer' }} title="Show the git command under every button and log each one with an explanation">
          <span className={'toggle' + (learn ? ' on' : '')} style={{ transform: 'scale(.8)' }} onClick={() => setLearn(!learn)} /> Learn git
        </label>
        <button className="icon-btn" onClick={() => void refresh()} title="Refresh">{I.refresh}</button>
      </div>
      <div className="tabs">
        {(['Changes', 'History', 'Worktrees', 'Stash', 'Learn'] as Tab[]).map((t) => (
          <button key={t} className={tab === t ? 'on' : ''} onClick={() => setTab(t)}>{t}{t === 'Changes' && files.length ? ` ${files.length}` : ''}{t === 'Stash' && stashes.length ? ` ${stashes.length}` : ''}{t === 'Learn' && gitLog.length ? ` ${gitLog.length}` : ''}</button>
        ))}
        <span style={{ flex: 1 }} />
        {undoTop && (
          <span className="gitbtn" title={`Undo safely: ${undoTop.cmd}`}>
            <button className="pill" disabled={busy} onClick={async () => { setBusy(true); const r = await undoTop.run(); setBusy(false); if (r !== undefined) { setUndo((u) => u.slice(1)); setNote(`Undid: ${undoTop.label}`); void refresh() } }}>↶ Undo: {undoTop.label}</button>
            {learn && <code className="learn-cmd">$ {undoTop.cmd}</code>}
          </span>
        )}
      </div>
      {err && <div className="banner red">{err}</div>}
      {note && !err && <div className="banner">{note}</div>}
      <div className="scroll">
        {tab === 'Changes' && (
          <div className="git-grid">
            <div>
              {conflicts.length > 0 && (
                <div className="card" style={{ marginBottom: 16, borderColor: 'var(--red)' }}>
                  <h3>Merge conflicts · {conflicts.length} file{conflicts.length > 1 ? 's' : ''}</h3>
                  <div className="sub2">Both sides changed the same lines. Pick a side per file, or ask the agent to combine them, then commit.</div>
                  {conflicts.map((f) => (
                    <div key={f.path} style={{ marginTop: 12 }}>
                      <div className="mono" style={{ marginBottom: 6 }}>{f.path}</div>
                      <pre className="term mono conflict">{(conflictDiffs[f.path] ?? '').split('\n').filter((l) => !l.startsWith('diff --cc') && !l.startsWith('index ')).map((l, i) => <span key={i} className={/^[ +-]{1,2}(<<<<<<<|=======|>>>>>>>)/.test(l) ? 'marker' : l.startsWith('++') || l.startsWith(' +') || l.startsWith('+ ') ? 'green' : l.startsWith('--') || l.startsWith(' -') || l.startsWith('- ') ? 'red' : ''}>{l + '\n'}</span>)}</pre>
                      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 8 }}>
                        <GitBtn learn={learn} label="Keep mine" cmd={`git checkout --ours -- ${f.path} && git add -- ${f.path}`} why={W('checkout-ours')} onClick={async () => { if (await act(`Kept our side of ${f.path}`, 'checkout-ours', { file: f.path })) await act(`Resolved ${f.path}`, 'add', { file: f.path }) }} />
                        <GitBtn learn={learn} label="Take theirs" cmd={`git checkout --theirs -- ${f.path} && git add -- ${f.path}`} why={W('checkout-theirs')} onClick={async () => { if (await act(`Took their side of ${f.path}`, 'checkout-theirs', { file: f.path })) await act(`Resolved ${f.path}`, 'add', { file: f.path }) }} />
                        <button className="pill" onClick={() => askAgent(`Resolve the git merge conflict in ${f.path} in ${cwd}: combine both sides so both intentions survive, remove the conflict markers, run the tests, then git add the file. Do not commit.`)}>Ask the agent to resolve</button>
                      </div>
                    </div>
                  ))}
                  <div style={{ marginTop: 14 }}><GitBtn learn={learn} label="Abort the merge" cmd="git merge --abort" why={W('merge-abort')} onClick={() => void act('Aborted the merge', 'merge-abort')} /></div>
                </div>
              )}
              <div className="section-label" style={{ padding: '0 0 8px' }}><span>Unstaged · {unstaged.length + untracked.length}</span></div>
              {unstaged.length + untracked.length === 0 && <div className="sub" style={{ marginBottom: 14 }}>Nothing unstaged. The working folder matches the index.</div>}
              {untracked.map((f) => (
                <div className="diff" key={f.path} style={{ marginBottom: 10 }}><div className="diff-head"><span className="mono">{f.path} <span className="tag green">new</span></span>
                  <GitBtn learn={learn} label="Stage file" cmd={`git add -- ${f.path}`} why={W('add')} onClick={() => void act(`Staged ${f.path}`, 'add', { file: f.path }, undefined, { cmd: `git restore --staged -- ${f.path}`, run: () => git('restore-staged', { file: f.path }) })} /></div></div>
              ))}
              {unstaged.map((f) => <HunkView key={f.path} f={f} stagedSide={false} />)}
              <div className="section-label" style={{ padding: '14px 0 8px' }}><span>Staged · {staged.length}</span></div>
              {staged.length === 0 && <div className="sub">Nothing staged yet. Stage a file or a single hunk to build the next commit.</div>}
              {staged.map((f) => <HunkView key={f.path} f={f} stagedSide />)}
            </div>
            <div className="card commit-box">
              <h3>Commit</h3>
              <div className="sub2">{staged.length ? `${staged.length} file${staged.length > 1 ? 's' : ''} staged on ${status?.branch}` : canCommit ? 'Conflicts resolved: commit to finish the merge' : 'Stage changes first'}</div>
              <textarea className="input" style={{ height: 110, padding: 12, marginTop: 12, resize: 'vertical', width: '100%' }} placeholder="Summary of the change (first line ≤ 72 chars)" value={msg} onChange={(e) => setMsg(e.target.value)} />
              {msg.split('\n')[0].length > 72 && <div className="sub amber">The first line is {msg.split('\n')[0].length} characters; 50–72 reads best in git log.</div>}
              <div style={{ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
                <button className="pill" disabled={!staged.length || busy} title="Ask the current session's agent for a message from the staged diff (one small model request)" onClick={async () => {
                  setBusy(true); setNote('Asking the agent for a commit message…')
                  const d = (await git('diff', { staged: '1' })) ?? ''
                  const s = await suggest(d.slice(0, 12000))
                  setBusy(false)
                  if (s) { setMsg(s); setNote('Suggested by the agent; edit before committing') } else setNote('No suggestion (open a session to use this)')
                }}>✦ Suggest message</button>
                <GitBtn learn={learn} primary label="Commit" disabled={!canCommit || !msg.trim() || busy} cmd={`git commit -m "${msg.split('\n')[0].slice(0, 40)}"`} why={W('commit')}
                  onClick={async () => {
                    const before = await head()
                    const ok = await act(`Committed “${msg.split('\n')[0].slice(0, 40)}”`, 'commit', { message: msg })
                    if (!ok) return
                    const after = await head()
                    setMsg('')
                    setUndo((u) => [{ label: 'the last commit', cmd: `git reset --soft ${before.slice(0, 8)}`, run: async () => ((await head()) === after ? git('reset-soft', { rev: before }) : (setErr('HEAD moved since that commit; undo would lose work, so it was skipped'), undefined)) }, ...u])
                  }} />
              </div>
              <div className="sub" style={{ marginTop: 14 }}>Every action can be undone from the bar above. Undo of a commit uses <span className="mono">git reset --soft</span>, which keeps your changes staged.</div>
            </div>
          </div>
        )}
        {tab === 'History' && (
          <div className="mono graph-list">
            {graph.map((r, i) => (
              <div key={i} className="graph-row" title={r.hash ? `git show ${r.hash}` : ''}>
                <GraphGlyphs g={r.graph} />
                {r.hash && <>
                  <span className="sub">{r.hash}</span>
                  {r.refs && r.refs.split(', ').map((ref) => <span key={ref} className={'ref' + (ref.startsWith('HEAD') ? ' head' : ref.includes('/') && !ref.startsWith('acpd/') && !ref.startsWith('acportal/') ? ' remote' : '')}>{ref.replace('HEAD -> ', '⌂ ')}</span>)}
                  <span className="subj">{r.subject}</span>
                  <span className="sub">{r.author} · {r.when}</span>
                </>}
              </div>
            ))}
            {learn && <div className="learn-cmd" style={{ marginTop: 10 }}>$ git log --graph --all --date-order --oneline — every ● is a commit; lines show which branch it belongs to; merges join two lines.</div>}
          </div>
        )}
        {tab === 'Worktrees' && (
          <div style={{ maxWidth: 900 }}>
            <div className="sub2" style={{ marginBottom: 12 }}>A worktree is another folder checked out from the same repository, on its own branch. Agents use them so parallel tasks never edit the same files.</div>
            {worktrees.map((w, i) => (
              <div className="set-row" key={w.path}>
                <span className={'dot ' + (w.path === cwd ? 'green' : '')} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="mono">{w.branch ?? (w.detached ? `detached at ${w.head}` : w.head)}{i === 0 ? ' · main checkout' : ''}{w.locked ? ' · locked' : ''}{w.prunable ? ' · missing folder' : ''}</div>
                  <div className="sub mono">{tildify(w.path)}</div>
                </div>
                <button className="pill" onClick={() => openSession(w.path)}>Session here</button>
                {i > 0 && !w.path.includes('/worktrees/') && <GitBtn learn={learn} label="Remove" cmd={`git worktree remove ${tildify(w.path)}`} why={W('worktree-remove')} onClick={() => { if (confirm(`Remove the folder ${w.path}? Its branch ${w.branch ?? ''} stays.`)) void act(`Removed worktree ${baseName(w.path)}`, 'worktree-remove', { path: w.path }) }} />}
              </div>
            ))}
            <div style={{ display: 'flex', gap: 8, marginTop: 16, alignItems: 'center' }}>
              <input className="input mono" style={{ flex: 1 }} placeholder="new-branch-name" value={newWt} onChange={(e) => setNewWt(e.target.value.replace(/[^\w./-]/g, '-'))} />
              <GitBtn learn={learn} label="New worktree" disabled={!newWt} cmd={`git worktree add -b ${newWt || '<branch>'} ../${baseName(cwd)}-${newWt.replace(/\//g, '-') || '<branch>'}`} why={W('worktree-add')}
                onClick={() => { const p = cwd.replace(/[\\/][^\\/]+$/, '') + '/' + baseName(cwd) + '-' + newWt.replace(/\//g, '-'); void act(`Created worktree ${newWt}`, 'worktree-add', { branch: newWt, path: p }).then((ok) => ok && setNewWt('')) }} />
            </div>
            <div className="section-label" style={{ padding: '22px 0 8px' }}>Branches made by tasks</div>
            <div className="sub">acpd names delegated task branches <span className="mono">acpd/task-…</span>; merge them from the task view so the host can check the parent folder first.</div>
          </div>
        )}
        {tab === 'Stash' && (
          <div style={{ maxWidth: 900 }}>
            <div className="sub2" style={{ marginBottom: 12 }}>The stash is a shelf for unfinished work: it cleans the folder without committing, and you can bring the changes back later.</div>
            <GitBtn learn={learn} label="Stash all changes" disabled={!files.length} cmd="git stash push --include-untracked" why={W('stash-push')} onClick={() => void act('Stashed changes', 'stash-push', { message: `acportal ${new Date().toLocaleString()}` }, undefined, { cmd: 'git stash pop', run: () => git('stash-pop') })} />
            <div style={{ marginTop: 14 }}>
              {stashes.length === 0 && <div className="sub">The stash is empty.</div>}
              {stashes.map((s) => (
                <div className="set-row" key={s.ref}>
                  <span className="mono sub">{s.ref}</span>
                  <div style={{ flex: 1 }}><div>{s.msg}</div><div className="sub">{s.when}</div></div>
                  <GitBtn learn={learn} label="Apply" cmd={`git stash apply ${s.ref}`} why={W('stash-apply')} onClick={() => void act(`Applied ${s.ref}`, 'stash-apply', { ref: s.ref })} />
                  <GitBtn learn={learn} label="Pop" cmd={`git stash pop ${s.ref}`} why={W('stash-pop')} onClick={() => void act(`Popped ${s.ref}`, 'stash-pop', { ref: s.ref })} />
                  <GitBtn learn={learn} label="Drop" cmd={`git stash drop ${s.ref}`} why={W('stash-drop')} onClick={() => { if (confirm(`Delete ${s.ref}? This cannot be undone from the app.`)) void act(`Dropped ${s.ref}`, 'stash-drop', { ref: s.ref }) }} />
                </div>
              ))}
            </div>
          </div>
        )}
        {tab === 'Learn' && (
          <div style={{ maxWidth: 900 }}>
            <div className="sub2" style={{ marginBottom: 12 }}>Every git command the app ran for you, newest first, with what it does. Copy any line into a terminal to do the same by hand.</div>
            {gitLog.length === 0 && <div className="sub">Nothing yet. Stage, commit or stash something.</div>}
            {gitLog.map((e, i) => (
              <div key={i} className="learn-row">
                <code className={e.ok ? '' : 'red'}>$ {e.args.join(' ')}</code>
                <div className="sub2">{e.why}</div>
                <div className="sub">{new Date(e.at).toLocaleTimeString()} · {baseName(e.cwd)} · {e.ms} ms{e.ok ? '' : ' · failed'}</div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
