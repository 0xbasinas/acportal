// Sessions grouped under the project (allowed workspace folder) they run in, for the sidebar.
// A session belongs to the deepest project folder that contains its workspace; sessions in no
// project (a host scratch folder, or a folder that is no longer allowed) form their own group.

/** A Windows host lists project folders as `C:\x` but reports a session's folder in its canonical
 * form, `\\?\C:\x` (`\\?\UNC\srv\share` for a share). Both name the same folder, so the prefix is
 * dropped before paths are compared. Without that, no session on Windows matched its project and
 * every one landed under "No project". */
export const plain = (p: string): string => p.replace(/^\\\\\?\\UNC\\/, '\\\\').replace(/^\\\\\?\\/, '')
const norm = (p: string): string => plain(p).replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase()
/** Whether `path` is `folder` or inside it (case-insensitive, either slash, as on Windows). */
export function inside(path: string, folder: string): boolean {
  const a = norm(path)
  const b = norm(folder)
  return a === b || a.startsWith(b + '/')
}
/** What follows `folder` in `path`, as `path` writes it, or nothing when `path` is the folder
 * itself or is not inside it. */
export function subfolder(path: string, folder: string): string | undefined {
  if (!inside(path, folder) || norm(path) === norm(folder)) return undefined
  return plain(path).replace(/[\\/]+$/, '').slice(plain(folder).replace(/[\\/]+$/, '').length + 1)
}

/** Whether a session folder is one the host made for a session outside every project
 * (`<state>/scratch/<seconds>-<six hex>`). The host reports that root as `scratchRoot`; an older
 * host does not, so the shape of the name is used then. Such a folder can sit inside a project
 * (a development host keeps its state in the repository), and it must never count as that project. */
export function isScratch(workspace: string, scratchRoot?: string): boolean {
  if (scratchRoot) return inside(workspace, scratchRoot) && norm(workspace) !== norm(scratchRoot)
  return /(^|\/)scratch\/\d+-[0-9a-f]{6}(\/|$)/i.test(norm(workspace))
}

export type ProjectGroup<S> = { key: string; name: string; path?: string; sessions: S[] }

/** Groups `sessions` (oldest first, as the host lists them) under `projects`. Projects with
 * sessions come first, the most recently used on top, each with its newest session first; then
 * projects without sessions in their given order, then sessions in no project. */
export function groupByProject<S extends { workspace: string }>(sessions: S[], projects: { path: string; name: string }[], scratchRoot?: string): ProjectGroup<S>[] {
  const owner = (s: S): { path: string; name: string } | undefined => isScratch(s.workspace, scratchRoot) ? undefined :
    projects.filter((p) => inside(s.workspace, p.path)).sort((a, b) => norm(b.path).length - norm(a.path).length)[0]
  const groups = new Map<string, ProjectGroup<S> & { last: number }>()
  projects.forEach((p) => groups.set(p.path, { key: p.path, name: p.name, path: p.path, sessions: [], last: -1 }))
  const none: ProjectGroup<S> & { last: number } = { key: '\u0000none', name: 'No project', sessions: [], last: -1 }
  sessions.forEach((s, i) => {
    const p = owner(s)
    const g = p ? groups.get(p.path)! : none
    g.sessions.unshift(s)
    g.last = i
  })
  const used = [...groups.values()].filter((g) => g.sessions.length).sort((a, b) => b.last - a.last)
  const empty = [...groups.values()].filter((g) => !g.sessions.length)
  return [...used, ...empty, ...(none.sessions.length ? [none] : [])].map(({ last: _, ...g }) => g)
}
