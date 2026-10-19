import { useEffect, useRef, useState, type CSSProperties, type MouseEvent as ReactMouseEvent } from 'react'
import { createPortal } from 'react-dom'
import hljs from 'highlight.js'
import { PANE, PaneResize, readPane, writePane } from './PaneResize'
import type { Result, Task } from './types'
import { changedFiles, DiffBlock, Timeline } from './acp'
import { Markdown } from './Markdown'
import { DiffView, Empty, I, ago, stateWord, tildify } from './ui'
import { inside, plain, subfolder } from './projects'

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

type HostCall = <T>(method: string, path: string, body?: unknown, query?: Record<string, string>) => Promise<Result<T>>
type FolderEntry = { name: string; path: string; directory: boolean }

const leaf = (p: string): string => {
  const s = plain(p).replace(/[\\/]+$/, '')
  const cut = Math.max(s.lastIndexOf('/'), s.lastIndexOf('\\'))
  return cut >= 0 ? s.slice(cut + 1) : s
}

/** A changed-file path as the host can open it. Absolute paths stay; a relative one is joined to the session folder. */
function filePath(path: string, workspace?: string): string | undefined {
  if (!path) return
  if (/^(?:[a-zA-Z]:[\\/]|\\\\|\/)/.test(path)) return path
  if (!workspace) return
  return workspace.replace(/[\\/]+$/, '') + '/' + path.replace(/^[/\\]+/, '')
}

function FileMenu({ x, y, onOpen, onAttach, onClose }: { x: number; y: number; onOpen: () => void; onAttach: () => void; onClose: () => void }) {
  const width = 196
  const height = 84
  const margin = 8
  const left = Math.max(margin, Math.min(x, window.innerWidth - width - margin))
  const top = Math.max(margin, Math.min(y, window.innerHeight - height - margin))
  const ref = useRef<HTMLDivElement>(null)
  const close = useRef(onClose)
  close.current = onClose
  useEffect(() => {
    const down = (e: MouseEvent): void => { if (!ref.current?.contains(e.target as Node)) close.current() }
    const key = (e: KeyboardEvent): void => { if (e.key === 'Escape') { e.stopPropagation(); close.current() } }
    document.addEventListener('mousedown', down)
    document.addEventListener('keydown', key)
    return () => { document.removeEventListener('mousedown', down); document.removeEventListener('keydown', key) }
  }, [])
  return createPortal(
    <div ref={ref} className="file-menu" role="menu" style={{ left, top }}>
      <button type="button" role="menuitem" onClick={onOpen}>{I.file}<span>Open</span></button>
      <button type="button" role="menuitem" onClick={onAttach}>{I.attach}<span>Attach to message</span></button>
    </div>,
    document.body)
}

/** The session folder, one level at a time. The host lists names only, and only inside a folder it already allows. */
function SessionFolder({ workspace, api, openPath, onOpenFile, onFileMenu }: { workspace?: string; api: HostCall; openPath?: string; onOpenFile: (path: string) => void; onFileMenu: (x: number, y: number, path: string) => void }) {
  const [stack, setStack] = useState<string[]>([])
  const [tick, setTick] = useState(0)
  const [view, setView] = useState<{ entries: FolderEntry[]; truncated: boolean } | { error: string } | undefined>(undefined)
  const nested = Boolean(workspace && stack.length > 0 && inside(stack[stack.length - 1], workspace))
  const at = nested ? stack[stack.length - 1] : workspace
  useEffect(() => { setStack([]) }, [workspace])
  useEffect(() => {
    if (!at) { setView({ error: 'This session has no folder.' }); return }
    let live = true
    setView(undefined)
    void api<{ entries?: FolderEntry[]; truncated?: boolean }>('GET', '/v1/workspaces/entries', undefined, { path: at }).then((r) => {
      if (!live) return
      if (r.ok) setView({ entries: r.value.entries ?? [], truncated: Boolean(r.value.truncated) })
      else if (r.status === 404) setView({ error: 'This host cannot list folders yet. Restart acpd to pick up the update.' })
      else if (r.code === 'invalid_request') setView({ error: 'The host could not open this folder.' })
      else setView({ error: 'The folder could not be read.' })
    })
    return () => { live = false }
  }, [api, at, tick])
  const where = at ? (workspace ? subfolder(at, workspace) : undefined) ?? leaf(at) : ''
  const entries = view && 'entries' in view ? view.entries : undefined
  const same = (p: string): boolean => Boolean(openPath && inside(p, openPath) && inside(openPath, p))
  return (
    <section className="panel" aria-label="Session folder">
      <div className="panel-h">{I.folder}<span>Folder</span>
        <button className="icon-btn" aria-label="Refresh the folder" title="Refresh" onClick={() => setTick((n) => n + 1)}>{I.refresh}</button>
      </div>
      {where && <div className="folder-where mono ell" title={at}>{where}</div>}
      {nested && (
        <button className="folder-row" onClick={() => setStack((s) => s.slice(0, -1))}>{I.back}<span>Up</span></button>
      )}
      {entries === undefined && !view && <div className="hint">Reading this folder…</div>}
      {view && 'error' in view && <div className="hint">{view.error}</div>}
      {entries?.length === 0 && <div className="hint">This folder is empty.</div>}
      {entries?.map((entry) => entry.directory ? (
        <button className="folder-row" key={entry.path} title={entry.path} onClick={() => setStack((s) => [...s, entry.path])}>{I.folder}<span className="grow ell">{entry.name}</span></button>
      ) : (
        <button className={'folder-row' + (same(entry.path) ? ' on' : '')} key={entry.path} title={entry.path} onClick={() => onOpenFile(entry.path)} onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); onFileMenu(e.clientX, e.clientY, entry.path) }}>{I.file}<span className="grow ell">{entry.name}</span></button>
      ))}
      {view && 'truncated' in view && view.truncated && <div className="hint">Only the first 1,024 names are shown.</div>}
    </section>
  )
}

const LANG: Record<string, string> = {
  ts: 'typescript', tsx: 'typescript', mts: 'typescript', js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript',
  rs: 'rust', py: 'python', go: 'go', css: 'css', scss: 'scss', html: 'xml', htm: 'xml', xml: 'xml', svg: 'xml',
  json: 'json', yml: 'yaml', yaml: 'yaml', toml: 'ini', sh: 'bash', bash: 'bash', zsh: 'bash', ps1: 'powershell',
  kt: 'kotlin', kts: 'kotlin', java: 'java', c: 'c', h: 'c', cpp: 'cpp', cc: 'cpp', hpp: 'cpp', cs: 'csharp',
  rb: 'ruby', php: 'php', sql: 'sql', md: 'markdown', lua: 'lua', swift: 'swift'
}
const extOf = (name: string): string => { const i = name.lastIndexOf('.'); return i > 0 ? name.slice(i + 1).toLowerCase() : '' }
const sizeOf = (n: number): string => n < 1024 ? `${n} B` : n < 1048576 ? `${Math.max(1, Math.round(n / 1024))} KB` : `${(n / 1048576).toFixed(1)} MB`
const escapeHtml = (s: string): string => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
const SAFE_IMAGE = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp'])

type OpenedFile = { name: string; bytes: number; kind: 'text' | 'image' | 'binary'; truncated?: boolean; content?: string; mediaType?: string; data?: string }

function CodeFile({ text, lang }: { text: string; lang?: string }) {
  const known = lang && hljs.getLanguage(lang) ? lang : undefined
  const html = known ? hljs.highlight(text, { language: known }).value : escapeHtml(text)
  const body = text.endsWith('\n') ? text.slice(0, -1) : text
  const lines = body.length === 0 ? [] : body.split('\n')
  return (
    <div className="code-file">
      <div className="code-gutter" aria-hidden="true">{lines.map((_, i) => <span key={i}>{i + 1}</span>)}</div>
      <pre className="code-pre"><code className={known ? `hljs language-${known}` : 'hljs'} dangerouslySetInnerHTML={{ __html: html }} /></pre>
    </div>
  )
}

/** The open file, in the main pane. Markdown is rendered, code is highlighted, images are shown. */
export function FileView({ path, root, api, onBack }: { path: string; root?: string; api: HostCall; onBack: () => void }) {
  const [file, setFile] = useState<OpenedFile | { error: string } | undefined>(undefined)
  useEffect(() => {
    let live = true
    setFile(undefined)
    void api<OpenedFile>('GET', '/v1/workspaces/file', undefined, { path }).then((r) => {
      if (!live) return
      if (r.ok) setFile(r.value)
      else if (r.status === 404) setFile({ error: 'This host cannot open files yet. Restart acpd to pick up the update.' })
      else if (r.code === 'invalid_request') setFile({ error: 'The host could not open this file.' })
      else setFile({ error: 'The file could not be read.' })
    })
    return () => { live = false }
  }, [api, path])
  const name = leaf(path)
  const where = root ? subfolder(path, root) ?? name : name
  const text = file && 'kind' in file && file.kind === 'text' ? file.content ?? '' : undefined
  const markdown = text !== undefined && /^(md|mdx|markdown)$/.test(extOf(name))
  const image = file && 'kind' in file && file.kind === 'image' && file.data && file.mediaType && SAFE_IMAGE.has(file.mediaType) ? file : undefined
  return (
    <div className="main">
      <div className="topbar">
        <button className="icon-btn" onClick={onBack} aria-label="Back" title="Back (Esc)">{I.back}</button>
        <div className="topbar-title">
          <h1 title={path}>{name}</h1>
          <div className="sub mono ell" title={path}>{where}{file && 'bytes' in file ? ` · ${sizeOf(file.bytes)}` : ''}</div>
        </div>
      </div>
      <div className="scroll file-view">
        {file === undefined && <div className="hint">Reading {name}…</div>}
        {file && 'error' in file && <div className="hint">{file.error}</div>}
        {file && 'truncated' in file && file.truncated && <div className="hint file-note">{file.kind === 'text' ? 'Showing the first 256 KB.' : 'This file is too large to show.'}</div>}
        {text !== undefined && text.length === 0 && <div className="hint">This file is empty.</div>}
        {markdown && text && <Markdown text={text} />}
        {text !== undefined && !markdown && text.length > 0 && <CodeFile text={text} lang={LANG[extOf(name)]} />}
        {image && <img className="file-image" alt={name} src={`data:${image.mediaType};base64,${image.data}`} />}
        {file && 'kind' in file && file.kind === 'binary' && <div className="hint">This file is not text ({sizeOf(file.bytes)}). Nothing to show.</div>}
      </div>
    </div>
  )
}

export function RightPanel({ tasks, rootTask, timeline, onOpenTask, onReview, onMerge, onOpenFile, onAttachFile, openPath, orchestrator, workspace, api }: {
  tasks: Task[]; rootTask?: Task; timeline?: Timeline; orchestrator: boolean; workspace?: string; api: HostCall; openPath?: string
  onOpenTask: (t: Task) => void; onReview: (path?: string) => void; onMerge: (t: Task) => void; onOpenFile: (path: string) => void; onAttachFile: (path: string) => void
}) {
  const rows = rootTask ? flattenTree(tasks, rootTask.root ?? rootTask.id) : []
  const files = timeline ? changedFiles(timeline) : []
  const add = files.reduce((s, f) => s + f.added, 0)
  const del = files.reduce((s, f) => s + f.removed, 0)
  const withWorktree = rows.map((r) => r.task).filter((t) => t.worktree)
  const usage = timeline?.usage
  const [menu, setMenu] = useState<{ x: number; y: number; path: string }>()
  const openMenu = (x: number, y: number, path: string): void => setMenu({ x, y, path })
  return (
    <aside className="right">
      {/* Delegation is a host setting; without it the task tree has nothing to show. */}
      {orchestrator && <section className="panel">
        <div className="panel-h">{I.flow}<span>Task tree</span>{rows.length > 0 && <span className="count">{rows.length}</span>}</div>
        {orchestrator && rows.length === 0 && <div className="hint">No delegated tasks in this session yet.</div>}
        <TaskRows rows={rows} onOpen={onOpenTask} />
      </section>}
      <SessionFolder workspace={workspace} api={api} openPath={openPath} onOpenFile={onOpenFile} onFileMenu={openMenu} />
      <section className="panel">
        <div className="panel-h">{I.file}<span>Changed files</span>
          {files.length > 0 && <span className="diff-stat mono"><span className="green">+{add}</span><span className="red">−{del}</span></span>}
        </div>
        {files.length === 0 && <div className="hint">No edits reported by the agent yet.</div>}
        {files.map((f) => (
          <div className="file-row" key={f.path} onClick={() => onReview(f.path)} title={f.path} onContextMenu={(e: ReactMouseEvent) => { const path = filePath(f.path, workspace); if (!path) return; e.preventDefault(); e.stopPropagation(); openMenu(e.clientX, e.clientY, path) }}>
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
      {menu && <FileMenu x={menu.x} y={menu.y} onClose={() => setMenu(undefined)} onOpen={() => { onOpenFile(menu.path); setMenu(undefined) }} onAttach={() => { onAttachFile(menu.path); setMenu(undefined) }} />}
    </aside>
  )
}

export function DiffReview({ title, subtitle, diffs: raw, focus, onBack, task, onMerge, merging, root }: {
  title: string; subtitle: string; diffs: DiffBlock[]; root?: string; focus?: string; onBack: () => void; task?: Task; onMerge?: (t: Task) => void; merging?: boolean
}) {
  const diffs = raw.map((d) => ({ ...d, path: rel(d.path, root) }))
  const [sel, setSel] = useState<string | undefined>(focus ? rel(focus, root) : undefined)
  const [filesW, setFilesW] = useState(() => readPane('review'))
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
      <div className="review-grid" style={{ '--review-w': filesW + 'px' } as CSSProperties}>
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
        <PaneResize className="at-review" axis="x" grow="start" label="Resize the file list" value={filesW} min={PANE.review.min} max={PANE.review.max}
          onChange={setFilesW} onCommit={(n) => writePane('review', n)} onReset={() => { setFilesW(PANE.review.def); writePane('review', PANE.review.def) }} />
      </div>
    </div>
  )
}
