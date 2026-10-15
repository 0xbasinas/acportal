// Local git for hosts on this computer (loopback address). acpd exposes no git routes, so for a
// remote host the app never runs git (it would bypass the host); the Git view says so instead.
// Every command is run without a shell, inside a folder the host itself allows (workspace roots
// or a task worktree it reported), and is reported to the renderer for the "Learn git" log.
import { execFile } from 'node:child_process'
import { existsSync, realpathSync } from 'node:fs'
import { isAbsolute, join, sep } from 'node:path'
import type { BrowserWindow, IpcMain } from 'electron'

type Wrap = <A extends unknown[], R>(fn: (...a: A) => Promise<R> | R) => (e: unknown, ...a: A) => Promise<unknown>
export type GitLogEntry = { at: number; cwd: string; args: string[]; why: string; ok: boolean; ms: number }

const LOOPBACK = /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?$/
const READ_ONLY = new Set(['status', 'diff', 'log', 'worktree-list', 'stash-list', 'rev-parse', 'show', 'branch-list', 'conflicts', 'reflog', 'merging'])

function run(cwd: string, args: string[], input?: string): Promise<{ out: string; err: string; code: number }> {
  return new Promise((resolve) => {
    const child = execFile('git', ['-c', 'core.quotepath=off', '-c', 'color.ui=never', ...args], { cwd, maxBuffer: 16 * 1024 * 1024, timeout: 60_000, env: { ...process.env, GIT_TERMINAL_PROMPT: '0', GIT_OPTIONAL_LOCKS: '0', LC_ALL: 'C' } }, (error, stdout, stderr) => {
      resolve({ out: stdout.toString(), err: stderr.toString(), code: error ? (typeof error.code === 'number' ? error.code : 1) : 0 })
    })
    if (input !== undefined) child.stdin?.end(input)
  })
}

/** Plain-language explanations shown in tooltips and the Learn git log. */
const WHY: Record<string, string> = {
  status: 'Show which files changed, which are staged, and the current branch',
  diff: 'Show the line changes not yet staged (add --cached for staged ones)',
  log: 'Show the commit history as a graph of branches',
  'worktree-list': 'List every working folder attached to this repository',
  'stash-list': 'List work you set aside with git stash',
  add: 'Stage a file: put its current contents into the next commit',
  'restore-staged': 'Unstage a file: keep your edits but leave them out of the next commit',
  'apply-cached': 'Stage one hunk: apply just that piece of the diff to the index',
  'apply-cached-reverse': 'Unstage one hunk: take just that piece back out of the index',
  commit: 'Record the staged changes as a new commit on the current branch',
  'reset-soft': 'Undo the last commit but keep its changes staged (nothing is lost)',
  'stash-push': 'Set your uncommitted changes aside and return to a clean folder',
  'stash-pop': 'Bring the most recent stashed changes back and remove them from the stash',
  'stash-apply': 'Bring stashed changes back but keep them in the stash',
  'stash-drop': 'Delete a stash entry',
  'worktree-add': 'Create a second working folder on a new branch, sharing this repository',
  'worktree-remove': 'Delete a working folder created with git worktree add (the branch stays)',
  'checkout-ours': 'Resolve a conflict by keeping your side of the file',
  'checkout-theirs': 'Resolve a conflict by taking the incoming side of the file',
  'merge-abort': 'Give up on the merge in progress and go back to before it started',
  conflicts: 'List files with merge conflicts',
  'branch-list': 'List branches with their latest commit',
  merging: 'Check whether a merge is in progress',
  reflog: 'Show where HEAD has been: the safety net for undo'
}

const OPS: Record<string, (p: Record<string, string>) => string[]> = {
  status: () => ['status', '--porcelain=v1', '-b', '-uall'],
  diff: (p) => ['diff', ...(p.staged ? ['--cached'] : []), '-U3', '--no-ext-diff', '--', ...(p.file ? [p.file] : [])],
  log: () => ['log', '--graph', '--all', '--date-order', '-n', '120', '--pretty=format:%h\u001f%d\u001f%s\u001f%an\u001f%cr'],
  'worktree-list': () => ['worktree', 'list', '--porcelain'],
  'stash-list': () => ['stash', 'list', '--pretty=format:%gd\u001f%s\u001f%cr'],
  'branch-list': () => ['branch', '--all', '--format=%(refname:short)\u001f%(objectname:short)\u001f%(HEAD)\u001f%(upstream:track)'],
  merging: () => ['rev-parse', '--git-path', 'MERGE_HEAD'],
  conflicts: () => ['diff', '--name-only', '--diff-filter=U'],
  'rev-parse': () => ['rev-parse', '--show-toplevel', 'HEAD', '--abbrev-ref', 'HEAD'],
  show: (p) => ['show', '--stat', '--format=%H%n%an%n%cr%n%B', p.rev],
  reflog: () => ['reflog', '-n', '15', '--pretty=format:%h\u001f%gs'],
  add: (p) => ['add', '--', p.file],
  'restore-staged': (p) => ['restore', '--staged', '--', p.file],
  'apply-cached': () => ['apply', '--cached', '--recount', '-'],
  'apply-cached-reverse': () => ['apply', '--cached', '--recount', '-R', '-'],
  commit: (p) => ['commit', '-m', p.message],
  'reset-soft': (p) => ['reset', '--soft', p.rev ?? 'HEAD~1'],
  'stash-push': (p) => ['stash', 'push', '--include-untracked', ...(p.message ? ['-m', p.message] : [])],
  'stash-pop': (p) => ['stash', 'pop', ...(p.ref ? [p.ref] : [])],
  'stash-apply': (p) => ['stash', 'apply', p.ref],
  'stash-drop': (p) => ['stash', 'drop', p.ref],
  'worktree-add': (p) => ['worktree', 'add', '-b', p.branch, p.path],
  'worktree-remove': (p) => ['worktree', 'remove', p.path],
  'checkout-ours': (p) => ['checkout', '--ours', '--', p.file],
  'checkout-theirs': (p) => ['checkout', '--theirs', '--', p.file],
  'merge-abort': () => ['merge', '--abort']
}

function inside(path: string, roots: string[]): boolean {
  let real: string
  try {
    real = realpathSync(path)
  } catch {
    return false
  }
  return roots.some((r) => {
    try {
      const rr = realpathSync(r)
      return real === rr || real.startsWith(rr.endsWith(sep) ? rr : rr + sep)
    } catch {
      return false
    }
  })
}

export function registerGit(ipc: IpcMain, wrap: Wrap, win: () => BrowserWindow | null, address: (hostRef: string) => string, roots?: (hostRef: string) => Promise<string[]>): void {
  const rootsOf = roots
  ipc.handle('git:available', wrap((hostRef: string) => LOOPBACK.test(address(hostRef))))
  ipc.handle('git:explain', wrap(() => WHY))
  ipc.handle(
    'git:run',
    wrap(async (hostRef: string, cwd: string, op: string, params: Record<string, string> = {}, input?: string) => {
      if (!LOOPBACK.test(address(hostRef))) throw new Error('Git runs only for a host on this computer; acpd has no git routes for remote hosts yet')
      const build = OPS[op]
      if (!build) throw new Error('Unknown git action')
      const allowed = rootsOf ? await rootsOf(hostRef) : []
      if (!inside(cwd, allowed)) throw new Error('That folder is not one the host allows')
      if (op === 'worktree-add' && !inside(params.path.replace(/[\\/][^\\/]+$/, ''), allowed)) throw new Error('New worktrees must be inside a host workspace root')
      const args = build(params)
      const t = Date.now()
      const r = await run(cwd, args, input)
      const entry: GitLogEntry = { at: t, cwd, args: ['git', ...args.map((a) => (op === 'commit' && a === params.message ? JSON.stringify(a.length > 60 ? a.slice(0, 57) + '…' : a) : a))], why: WHY[op] ?? '', ok: r.code === 0, ms: Date.now() - t }
      if (!READ_ONLY.has(op) || r.code !== 0) win()?.webContents.send('git:log', entry)
      if (r.code !== 0) throw new Error((r.err || r.out).trim().split('\n').slice(-3).join(' ') || `git exited ${r.code}`)
      if (op === 'merging') { const f = r.out.trim(); return { out: existsSync(isAbsolute(f) ? f : join(cwd, f)) ? 'yes' : '', args: entry.args } }
      return { out: r.out, args: entry.args }
    })
  )
}
