// Sessions grouped under the project (allowed workspace folder) they run in, for the sidebar.
// A session belongs to the deepest project folder that contains its workspace; sessions in no
// project (a host scratch folder, or a folder that is no longer allowed) form their own group.

const norm = (p: string): string => p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase()
/** Whether `path` is `folder` or inside it (case-insensitive, either slash, as on Windows). */
export function inside(path: string, folder: string): boolean {
  const a = norm(path)
  const b = norm(folder)
  return a === b || a.startsWith(b + '/')
}

export type ProjectGroup<S> = { key: string; name: string; path?: string; sessions: S[] }

/** Groups `sessions` (oldest first, as the host lists them) under `projects`. Projects with
 * sessions come first, the most recently used on top, each with its newest session first; then
 * projects without sessions in their given order, then sessions in no project. */
export function groupByProject<S extends { workspace: string }>(sessions: S[], projects: { path: string; name: string }[]): ProjectGroup<S>[] {
  const owner = (s: S): { path: string; name: string } | undefined =>
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
