import { ReactNode } from 'react'
import { DiffBlock, diffLines, lineStats } from './acp'

export const I = {
  back: '←', refresh: '↻', plus: '+', gear: '⚙', chev: '›', branch: '⎇', tool: '⚒', sub: '↳', tree: '⌥', send: '↑', clock: '◷', close: '×'
}

export function DiffView({ d, maxLines, onMore }: { d: DiffBlock; maxLines?: number; onMore?: () => void }): ReactNode {
  const lines = diffLines(d)
  const shown = maxLines ? lines.slice(0, maxLines) : lines
  const s = lineStats(d)
  return (
    <div className="diff">
      <div className="diff-head">
        <span className="mono">{d.path}</span>
        <span className="sub mono">
          <span className="green">+{s.added}</span> <span className="red">−{s.removed}</span>
        </span>
      </div>
      <div className="diff-body mono">
        <table>
          <tbody>
            {shown.map((l, k) =>
              l.kind === 'skip' ? (
                <tr key={k} className="skip"><td colSpan={3}>⋯</td></tr>
              ) : (
                <tr key={k} className={l.kind}>
                  <td className="n">{l.a ?? ''}</td>
                  <td className="n">{l.b ?? ''}</td>
                  <td className="t">{(l.kind === 'add' ? '+ ' : l.kind === 'del' ? '- ' : '  ') + l.text}</td>
                </tr>
              )
            )}
          </tbody>
        </table>
      </div>
      {maxLines && lines.length > maxLines && (
        <div style={{ padding: '8px 12px', borderTop: '1px solid var(--line)' }}>
          <button className="sub2" onClick={onMore}>Show {lines.length - maxLines} more lines</button>
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
