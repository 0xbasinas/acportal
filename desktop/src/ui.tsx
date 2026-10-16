import { ReactNode, createContext, useContext } from 'react'
import {
  ArrowLeft, ArrowUp, Check, ChevronDown, ChevronRight, CircleAlert, Command, CornerDownRight, FileCode2, FilePlus2, FolderGit2, FolderOpen, GitBranch, GitCommitHorizontal,
  GitCompareArrows, GitMerge, GripVertical, GitPullRequest, History, Inbox, Paperclip, Pencil, Plus, Redo2, RotateCw, Search, Server, Settings as Gear, ShieldAlert, Sparkles, Square,
  Terminal, Trash2, Undo2, Upload, Workflow, Wrench, X, Archive, GraduationCap, MessageSquare, Bot, Network, ChevronUp, Filter
} from 'lucide-react'
import { DiffBlock, diffLines, lineStats } from './acp'

const ic = { size: 15, strokeWidth: 1.75, 'aria-hidden': true } as const
/** One icon set (Lucide), one size and stroke, everywhere. */
export const I = {
  up: <ChevronUp {...ic} size={13} />, filter: <Filter {...ic} size={13} />,
  back: <ArrowLeft {...ic} />, refresh: <RotateCw {...ic} />, plus: <Plus {...ic} />, gear: <Gear {...ic} />, chev: <ChevronRight {...ic} />, down: <ChevronDown {...ic} size={13} />,
  branch: <GitBranch {...ic} />, tool: <Wrench {...ic} size={13} />, sub: <CornerDownRight {...ic} size={13} />, tree: <Network {...ic} size={13} />, send: <ArrowUp {...ic} size={16} strokeWidth={2.25} />,
  clock: <History {...ic} />, close: <X {...ic} />, check: <Check {...ic} size={13} />, alert: <CircleAlert {...ic} />, shield: <ShieldAlert {...ic} size={16} />, grip: <GripVertical {...ic} size={14} />, attach: <Paperclip {...ic} />,
  term: <Terminal {...ic} size={13} />, file: <FileCode2 {...ic} size={13} />, newfile: <FilePlus2 {...ic} size={13} />, folder: <FolderOpen {...ic} />, repo: <FolderGit2 {...ic} />, commit: <GitCommitHorizontal {...ic} />,
  compare: <GitCompareArrows {...ic} />, merge: <GitMerge {...ic} />, pr: <GitPullRequest {...ic} />, undo: <Undo2 {...ic} />, redo: <Redo2 {...ic} />, upload: <Upload {...ic} />, pencil: <Pencil {...ic} size={13} />,
  trash: <Trash2 {...ic} size={13} />, search: <Search {...ic} />, server: <Server {...ic} />, spark: <Sparkles {...ic} />, stop: <Square {...ic} size={12} fill="currentColor" />, inbox: <Inbox {...ic} size={28} strokeWidth={1.25} />,
  cmd: <Command {...ic} />, flow: <Workflow {...ic} />, stash: <Archive {...ic} />, learn: <GraduationCap {...ic} />, chat: <MessageSquare {...ic} size={28} strokeWidth={1.25} />, bot: <Bot {...ic} size={13} />
}

/** Empty state: icon, title, one line of help, actions. */
export function Empty({ icon, title, children, actions }: { icon?: ReactNode; title: string; children?: ReactNode; actions?: ReactNode }): ReactNode {
  return (
    <div className="empty">
      <div className="empty-in">
        {icon && <div className="empty-ic">{icon}</div>}
        <h2>{title}</h2>
        {children && <div className="empty-sub">{children}</div>}
        {actions && <div className="empty-act">{actions}</div>}
      </div>
    </div>
  )
}

/** The session folder, so diffs inside it show a relative path (the full path stays in the tooltip). */
export const PathRoot = createContext<string | undefined>(undefined)
const relPath = (p: string, root?: string): string => { const r = root?.replace(/[\\/]$/, ''); return r && (p.startsWith(r + '/') || p.startsWith(r + '\\')) ? p.slice(r.length + 1) : p }
export function DiffView({ d, maxLines, onMore }: { d: DiffBlock; maxLines?: number; onMore?: () => void }): ReactNode {
  const root = useContext(PathRoot)
  const lines = diffLines(d)
  const shown = maxLines ? lines.slice(0, maxLines) : lines
  const s = lineStats(d)
  return (
    <div className="diff">
      <div className="diff-head">
        <span className="diff-path">{d.oldText ? I.file : I.newfile}<span className="mono" title={d.path}>{relPath(d.path, root)}</span></span>
        <span className="diff-stat mono">
          <span className="green">+{s.added}</span><span className="red">−{s.removed}</span>
          <span className="bar">{Array.from({ length: 5 }, (_, i) => { const t = s.added + s.removed || 1; const g = Math.round((s.added / t) * 5); return <i key={i} className={i < g ? 'g' : 'r'} /> })}</span>
        </span>
      </div>
      <div className="diff-body mono">
        <table>
          <tbody>
            {shown.map((l, k) =>
              l.kind === 'skip' ? (
                <tr key={k} className="skip"><td colSpan={4}>⋯</td></tr>
              ) : (
                <tr key={k} className={l.kind}>
                  <td className="n">{l.a ?? ''}</td>
                  <td className="n">{l.b ?? ''}</td>
                  <td className="sign">{l.kind === 'add' ? '+' : l.kind === 'del' ? '−' : ''}</td>
                  <td className="t">{l.text}</td>
                </tr>
              )
            )}
          </tbody>
        </table>
      </div>
      {maxLines && lines.length > maxLines && (
        <div className="diff-more">
          <button className="link" onClick={onMore}>Show {lines.length - maxLines} more lines</button>
        </div>
      )}
    </div>
  )
}

export function stateDot(state?: string): string {
  switch (state) {
    case 'working': case 'submitted': case 'running': case 'busy': return 'white'
    case 'input_required': case 'auth_required': case 'waiting': return 'amber'
    case 'completed': case 'ready': case 'merged': return 'green'
    case 'failed': case 'rejected': case 'interrupted': return 'red'
    default: return ''
  }
}
export const stateWord = (s?: string): string =>
  ({ input_required: 'Needs a decision', auth_required: 'Needs sign-in', working: 'Running', submitted: 'Starting', completed: 'Completed', failed: 'Failed', canceled: 'Cancelled', rejected: 'Rejected', interrupted: 'Interrupted', ready: 'Ready' } as Record<string, string>)[s ?? ''] ?? (s ? s[0].toUpperCase() + s.slice(1) : '')

export function ago(ms?: number): string {
  if (!ms) return ''
  const t = ms < 1e12 ? ms * 1000 : ms
  const s = Math.max(0, (Date.now() - t) / 1000)
  if (s < 60) return 'now'
  if (s < 3600) return `${Math.floor(s / 60)}m`
  if (s < 86400) return `${Math.floor(s / 3600)}h`
  return `${Math.floor(s / 86400)}d`
}
export const baseName = (p: string): string => p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? p
export function tildify(p: string): string {
  return p.replace(/^\/home\/[^/]+/, '~').replace(/^\/Users\/[^/]+/, '~')
}
