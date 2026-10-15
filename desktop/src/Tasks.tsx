import { useState } from 'react'
import type { Task } from './types'
import { changedFiles, DiffBlock, Timeline } from './acp'
import { DiffView, I, ago, stateDot, stateWord, tildify } from './ui'

/** Trees: roots newest first, children oldest first; orphaned tasks become roots, cycles shown flat. */
export function flattenTree(tasks: Task[], rootFilter?: string): { task: Task; depth: number }[] {
  const ids = new Set(tasks.map((t) => t.id))
  const kids = new Map<string, Task[]>()
  const roots: Task[] = []
  for (const t of tasks) {
    if (t.parent && ids.has(t.parent) && t.parent !== t.id) kids.set(t.parent, [...(kids.get(t.parent) ?? []), t])
    else roots.push(t)
  }
  const out: { task: Task; depth: number }[] = []
  const seen = new Set<string>()
  const walk = (t: Task, d: number): void => {
    if (seen.has(t.id)) return
    seen.add(t.id)
    out.push({ task: t, depth: d })
    for (const c of (kids.get(t.id) ?? []).sort((a, b) => a.createdAt - b.createdAt)) walk(c, d + 1)
  }
  for (const r of roots.sort((a, b) => b.createdAt - a.createdAt)) if (!rootFilter || r.id === rootFilter || r.root === rootFilter) walk(r, 0)
  for (const t of tasks) if (!seen.has(t.id) && !rootFilter) out.push({ task: t, depth: 0 })
  return out
}

export function TaskRows({ rows, selected, onOpen }: { rows: { task: Task; depth: number }[]; selected?: string; onOpen: (t: Task) => void }) {
  return (
    <>
      {rows.map(({ task, depth }) => {
        const waiting = task.pendingDecisions ?? task.needsDecision?.length ?? 0
        const word = waiting ? 'Needs a decision' : stateWord(task.state)
        return (
          <div key={task.id} className={'tree-row' + (selected === task.id ? ' active' : '')} style={{ paddingLeft: 4 + depth * 16, background: selected === task.id ? 'var(--surface-2)' : undefined }} onClick={() => onOpen(task)}>
            <span className="sub">{depth === 0 ? I.tree : I.sub}</span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="title" style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{task.worktree?.branch ?? (depth === 0 ? 'Root session' : `Task ${task.id.slice(0, 6)}`)}</div>
              <div className="sub">
                {task.agent} · <span className={waiting ? 'amber' : task.state === 'failed' ? 'red' : ''}>{word}</span>
                {task.summary ? ` (${task.summary})` : ''} · {ago(task.updatedAt)}
              </div>
            </div>
            {waiting > 0 && <span className="badge">{waiting}</span>}
            <span className="sub">{I.chev}</span>
          </div>
        )
      })}
    </>
  )
}

export const rel = (p: string, ws?: string): string => (ws && p.startsWith(ws.replace(/\/$/, '') + '/') ? p.slice(ws.replace(/\/$/, '').length + 1) : tildify(p))

export function mergeError(code?: string, fallback?: string): string {
  return (
    ({
      task_not_finished: 'The task is still running. Merge when it has finished.',
      no_worktree: 'This task ran in the shared folder; there is no branch to merge.',
      nothing_to_merge: 'The task branch has no changes to merge.',
      parent_dirty: 'The parent folder has uncommitted changes or untracked files. Commit or stash them first; nothing was changed.',
      merge_conflict: 'The merge conflicts and was aborted; nothing was changed.',
      worktree_dirty: 'The task folder has uncommitted work kept by the host settings.',
      already_merged: 'This task was already merged.',
      worktree_busy: 'The task folder is in use. Try again in a moment.'
    } as Record<string, string>)[code ?? ''] ?? fallback ?? 'Merge failed'
  )
}

export function RightPanel({ tasks, rootTask, timeline, onOpenTask, onReview, onMerge, orchestrator, workspace }: {
  tasks: Task[]; rootTask?: Task; timeline?: Timeline; orchestrator: boolean; workspace?: string
  onOpenTask: (t: Task) => void; onReview: (path?: string) => void; onMerge: (t: Task) => void
}) {
  const rows = rootTask ? flattenTree(tasks, rootTask.root ?? rootTask.id) : []
  const files = timeline ? changedFiles(timeline) : []
  const add = files.reduce((s, f) => s + f.added, 0)
  const del = files.reduce((s, f) => s + f.removed, 0)
  const withWorktree = rows.map((r) => r.task).filter((t) => t.worktree)
  const usage = timeline?.usage
  return (
    <aside className="right">
      <div className="section-label" style={{ paddingTop: 0 }}>Task tree</div>
      {!orchestrator && <div className="sub">Delegation is off on this host ([orchestrator] enabled = false).</div>}
      {orchestrator && rows.length === 0 && <div className="sub">No delegated tasks in this session yet.</div>}
      <TaskRows rows={rows} onOpen={onOpenTask} />
      <div className="section-label">
        <span>Changed files</span>
        {files.length > 0 && <span className="mono"><span className="green">+{add}</span> <span className="red">−{del}</span></span>}
      </div>
      {files.length === 0 && <div className="sub">No edits reported by the agent yet.</div>}
      {files.map((f) => (
        <div className="file-row" key={f.path} onClick={() => onReview(f.path)}>
          <span className="st mono">{f.diff.oldText ? 'M' : 'A'}</span>
          <span className="mono" style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{rel(f.path, workspace)}</span>
        </div>
      ))}
      {withWorktree.map((t) => (
        <div key={t.id}>
          <div className="section-label">Worktree</div>
          <div className="mono">{t.worktree!.branch}</div>
          <div className="sub">Own worktree{t.worktree!.base ? ` · from ${t.worktree!.base.slice(0, 10)}` : ''}</div>
          <div style={{ marginTop: 10 }}>{t.worktree!.outcome ? `Outcome: ${t.worktree!.outcome}` : stateWord(t.state)}</div>
          <div className="sub">{t.worktree!.outcome === 'merged' ? 'Merged into the parent folder' : 'Uncommitted work is saved to the branch when it ends'}</div>
          {t.worktree!.outcome !== 'merged' && (
            <div style={{ marginTop: 14, display: 'flex', flexDirection: 'column', gap: 6 }}>
              <button className="btn-wide" disabled={!['completed', 'failed', 'canceled'].includes(t.state)} onClick={() => onMerge(t)}>Merge into parent</button>
              <button className="btn-wide ghost" onClick={() => onOpenTask(t)}>Review change</button>
            </div>
          )}
        </div>
      ))}
      {files.length > 0 && withWorktree.length === 0 && (
        <button className="btn-wide ghost" style={{ marginTop: 12 }} onClick={() => onReview()}>Review all changes</button>
      )}
      {usage && (
        <>
          <div className="section-label">Usage</div>
          <div className="sub2">
            {usage.used !== undefined && `${Math.round(usage.used / 1000)}k of ${Math.round((usage.size ?? 0) / 1000)}k context`}
            {usage.cost && ` · ${usage.cost.currency === 'USD' ? 'US$' : usage.cost.currency + ' '}${usage.cost.amount.toFixed(4)}`}
          </div>
        </>
      )}
    </aside>
  )
}

export function DiffReview({ title, subtitle, diffs: raw, focus, onBack, task, onMerge, merging, root }: {
  title: string; subtitle: string; diffs: DiffBlock[]; root?: string; focus?: string; onBack: () => void; task?: Task; onMerge?: (t: Task) => void; merging?: boolean
}) {
  const diffs = raw.map((d) => ({ ...d, path: rel(d.path, root) }))
  const [sel, setSel] = useState<string | undefined>(focus ? rel(focus, root) : undefined)
  const shown = sel ? diffs.filter((d) => d.path === sel) : diffs
  const files = diffs.map((d) => ({ d, s: changedFiles({ items: [{ kind: 'tool', id: d.path, title: '', diffs: [d], host: false }], seen: new Set(), lastSeq: 0, busy: false })[0] }))
  return (
    <div className="main">
      <div className="topbar">
        <button className="icon-btn" onClick={onBack} aria-label="Back">{I.back}</button>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h1>{title}</h1>
          <div className="sub">{subtitle}</div>
        </div>
        {task?.worktree && <span className="chip mono">{I.branch} {task.worktree.branch}</span>}
        {task?.worktree && task.worktree.outcome !== 'merged' && onMerge && (
          <button className="pill primary" disabled={merging || !['completed', 'failed', 'canceled'].includes(task.state)} onClick={() => onMerge(task)}>
            {merging ? 'Merging…' : 'Merge into parent'}
          </button>
        )}
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: '240px 1fr', flex: 1, minHeight: 0 }}>
        <div style={{ borderRight: '1px solid var(--line)', overflowY: 'auto', padding: 12 }}>
          <div className={'row' + (!sel ? ' active' : '')} onClick={() => setSel(undefined)}><span className="grow">All files</span><span className="sub">{diffs.length}</span></div>
          {files.map(({ d, s }) => (
            <div key={d.path} className={'row' + (sel === d.path ? ' active' : '')} onClick={() => setSel(d.path)}>
              <span className="mono grow" style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{d.path.split('/').pop()}</span>
              <span className="mono sub"><span className="green">+{s.added}</span> <span className="red">−{s.removed}</span></span>
            </div>
          ))}
        </div>
        <div className="scroll">
          {diffs.length === 0 ? (
            <div className="empty"><div><h2>No file changes</h2><div>The agent has not reported any edits in this {task ? 'task' : 'session'}.</div></div></div>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 16, maxWidth: 980, margin: '0 auto' }}>
              {shown.map((d) => <DiffView key={d.path} d={d} />)}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
