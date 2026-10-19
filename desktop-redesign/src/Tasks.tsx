import { useState } from 'react'
import type { Task } from './types'
import { changedFiles, DiffBlock, Timeline } from './acp'
import { DiffView, Empty, I, ago, stateWord, tildify } from './ui'
import { plain, subfolder } from './projects'

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
          <div key={task.id} className={'tree-row' + (selected === task.id ? ' active' : '')} style={{ paddingLeft: 6 + depth * 16 }} onClick={() => onOpen(task)}>
            <span className="tree-ic">{depth === 0 ? I.tree : I.sub}</span>
            <div className="grow">
              <div className="title">{task.worktree?.branch ?? (depth === 0 ? 'Root session' : `Task ${task.id.slice(0, 6)}`)}</div>
              <div className="sub">
                {task.agent} · <span className={waiting ? 'amber' : task.state === 'failed' ? 'red' : ''}>{word}</span>
                {task.summary ? ` (${task.summary})` : ''} · {ago(task.updatedAt)}
              </div>
            </div>
            {waiting > 0 && <span className="badge">{waiting}</span>}
            <span className="chev">{I.chev}</span>
          </div>
        )
      })}
    </>
  )
}

/** A changed file's path as shown in the panel: relative to the session's folder, whichever way
 * either is written (a Windows host reports `\\?\C:\x`, with backslashes), else shortened with `~`. */
export const rel = (p: string, ws?: string): string => (ws ? subfolder(p, ws) : undefined) ?? tildify(plain(p))

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
      {/* Delegation is a host setting; without it the task tree has nothing to show. */}
      {orchestrator && <section className="panel">
        <div className="panel-h">{I.flow}<span>Task tree</span>{rows.length > 0 && <span className="count">{rows.length}</span>}</div>
        {orchestrator && rows.length === 0 && <div className="hint">No delegated tasks in this session yet.</div>}
        <TaskRows rows={rows} onOpen={onOpenTask} />
      </section>}
      <section className="panel">
        <div className="panel-h">{I.file}<span>Changed files</span>
          {files.length > 0 && <span className="diff-stat mono"><span className="green">+{add}</span><span className="red">−{del}</span></span>}
        </div>
        {files.length === 0 && <div className="hint">No edits reported by the agent yet.</div>}
        {files.map((f) => (
          <div className="file-row" key={f.path} onClick={() => onReview(f.path)} title={f.path}>
            <span className={'st mono ' + (f.diff.oldText ? 'm' : 'a')}>{f.diff.oldText ? 'M' : 'A'}</span>
            <span className="mono grow ell">{rel(f.path, workspace)}</span>
            <span className="mono sub"><span className="green">+{f.added}</span> <span className="red">−{f.removed}</span></span>
          </div>
        ))}
        {files.length > 0 && withWorktree.length === 0 && (
          <button className="btn full" style={{ marginTop: 10 }} onClick={() => onReview()}>{I.compare}Review all changes</button>
        )}
      </section>
      {withWorktree.map((t) => (
        <section className="panel" key={t.id}>
          <div className="panel-h">{I.branch}<span>Worktree</span><span className={'tag ' + (t.worktree!.outcome === 'merged' ? 'green' : 'grey')}>{t.worktree!.outcome ?? stateWord(t.state)}</span></div>
          <div className="branch-pill mono">{I.branch}{t.worktree!.branch}</div>
          <div className="hint" style={{ marginTop: 6 }}>Own worktree{t.worktree!.base ? <> · from <span className="mono">{t.worktree!.base.slice(0, 8)}</span></> : ''}. {t.worktree!.outcome === 'merged' ? 'Merged into the parent folder.' : 'Uncommitted work is saved to the branch when it ends.'}</div>
          {t.worktree!.outcome !== 'merged' && (
            <div className="stack">
              <button className="btn primary full" disabled={!['completed', 'failed', 'canceled'].includes(t.state)} onClick={() => onMerge(t)}>{I.merge}Merge into parent</button>
              <button className="btn full" onClick={() => onOpenTask(t)}>{I.compare}Review change</button>
            </div>
          )}
        </section>
      ))}
      {usage && (
        <section className="panel">
          <div className="panel-h">{I.server}<span>Usage</span></div>
          {usage.used !== undefined && usage.size ? <div className="meter"><i style={{ width: `${Math.min(100, (usage.used / usage.size) * 100)}%` }} /></div> : null}
          <div className="hint">
            {usage.used !== undefined && `${Math.round(usage.used / 1000)}k of ${Math.round((usage.size ?? 0) / 1000)}k context`}
            {usage.cost && ` · ${usage.cost.currency === 'USD' ? 'US$' : usage.cost.currency + ' '}${usage.cost.amount.toFixed(4)}`}
          </div>
        </section>
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
        <button className="icon-btn" onClick={onBack} aria-label="Back" title="Back (Esc)">{I.back}</button>
        <div className="topbar-title">
          <h1>{title}</h1>
          <div className="sub">{subtitle}</div>
        </div>
        {task?.worktree && <span className="branch-pill mono">{I.branch}{task.worktree.branch}</span>}
        {task?.worktree && task.worktree.outcome !== 'merged' && onMerge && (
          <button className="btn primary" disabled={merging || !['completed', 'failed', 'canceled'].includes(task.state)} onClick={() => onMerge(task)}>
            {merging ? 'Merging…' : 'Merge into parent'}
          </button>
        )}
      </div>
      <div className="review-grid">
        <div className="review-files">
          <div className={'row' + (!sel ? ' active' : '')} onClick={() => setSel(undefined)}><span className="grow">All files</span><span className="count">{diffs.length}</span></div>
          {files.map(({ d, s }) => (
            <div key={d.path} className={'row' + (sel === d.path ? ' active' : '')} onClick={() => setSel(d.path)} title={d.path}>
              <span className="row-ic">{d.oldText || d.unified ? I.file : I.newfile}</span>
              <div className="grow"><div className="mono ell">{d.path.split('/').pop()}</div>{d.path.includes('/') && <div className="sub mono ell">{d.path.split('/').slice(0, -1).join('/')}</div>}</div>
              <span className="mono sub"><span className="green">+{s?.added ?? 0}</span> <span className="red">−{s?.removed ?? 0}</span></span>
            </div>
          ))}
        </div>
        <div className="scroll">
          {diffs.length === 0 ? (
            <Empty icon={I.inbox} title="No file changes">Nothing changed in this {task ? 'task' : 'session'} yet{task ? ', including edits made by shell commands' : ''}.</Empty>
          ) : (
            <div className="review-list">
              {shown.map((d) => <DiffView key={d.path} d={d} />)}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
