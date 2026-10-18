// The repositories the Git view can show, in the order it lists them: the folder it was opened for
// (a project's Git button, or the session's branch button), then the open session's folder, then
// the allowed project folders, then task worktrees that are not merged yet.
import { inside, plain } from './projects'

export type GitRepo = { path: string; label: string }
type Task = { worktree?: { path?: string; branch: string; outcome?: string } }

const base = (p: string): string => plain(p).replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? p
/** The same folder, whichever way it is written (`\\?\C:\x` and `c:/X` are one folder). */
const same = (a: string, b: string): boolean => inside(a, b) && inside(b, a)

export function gitRepos(opts: { asked?: string; sessionFolder?: string; workspaces: { path: string }[]; tasks: Task[] }): GitRepo[] {
  const { asked, sessionFolder } = opts
  const out: GitRepo[] = []
  const add = (r: GitRepo): void => { if (!out.some((o) => same(o.path, r.path))) out.push(r) }
  if (asked) add({ path: asked, label: base(asked) })
  if (sessionFolder) add({ path: sessionFolder, label: base(sessionFolder) + ' · session' })
  for (const w of opts.workspaces) add({ path: w.path, label: base(w.path) })
  for (const t of opts.tasks) if (t.worktree?.path && t.worktree.outcome !== 'merged') add({ path: t.worktree.path, label: t.worktree.branch + ' · task' })
  return out
}
