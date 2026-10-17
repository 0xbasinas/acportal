// Pure parsers for git's porcelain output (unit-tested).
export type FileStatus = { path: string; x: string; y: string; staged: boolean; unstaged: boolean; untracked: boolean; conflict: boolean; orig?: string }
export type Status = { branch: string; upstream?: string; ahead: number; behind: number; files: FileStatus[] }
const CONFLICT = new Set(['DD', 'AU', 'UD', 'UA', 'DU', 'AA', 'UU'])

export function parseStatus(out: string): Status {
  const st: Status = { branch: '', ahead: 0, behind: 0, files: [] }
  for (const line of out.split('\n')) {
    if (!line) continue
    if (line.startsWith('## ')) {
      const m = /^## (?:No commits yet on )?(\S+?)(?:\.\.\.(\S+))?(?: \[(.*)\])?$/.exec(line)
      st.branch = m?.[1] ?? line.slice(3)
      st.upstream = m?.[2]
      const a = /ahead (\d+)/.exec(m?.[3] ?? ''), b = /behind (\d+)/.exec(m?.[3] ?? '')
      st.ahead = a ? +a[1] : 0
      st.behind = b ? +b[1] : 0
      continue
    }
    const x = line[0], y = line[1]
    let path = line.slice(3)
    let orig: string | undefined
    if (path.includes(' -> ')) [orig, path] = path.split(' -> ')
    const xy = x + y
    st.files.push({ path, orig, x, y, untracked: xy === '??', conflict: CONFLICT.has(xy), staged: x !== ' ' && x !== '?' && !CONFLICT.has(xy), unstaged: (y !== ' ' && y !== '?' && !CONFLICT.has(xy)) || xy === '??' })
  }
  return st
}

export type Hunk = { header: string; lines: string[]; patch: string }
export type FileDiff = { path: string; header: string; hunks: Hunk[]; binary: boolean }
export function parseDiff(out: string): FileDiff[] {
  const files: FileDiff[] = []
  const chunks = out.split(/^(?=diff --git )/m).filter((c) => c.startsWith('diff --git '))
  for (const c of chunks) {
    const lines = c.replace(/\n$/, '').split('\n')
    const first = lines.findIndex((l) => l.startsWith('@@'))
    const header = (first < 0 ? lines : lines.slice(0, first)).join('\n') + '\n'
    const pm = /^\+\+\+ b\/(.*)$/m.exec(header) ?? /^diff --git a\/.* b\/(.*)$/m.exec(header)
    const f: FileDiff = { path: pm?.[1] ?? '', header, hunks: [], binary: /^Binary files /m.test(c) }
    if (first >= 0) {
      let cur: string[] | null = null
      for (const l of lines.slice(first)) {
        if (l.startsWith('@@')) { cur = [l]; f.hunks.push({ header: l, lines: cur, patch: '' }) }
        else cur?.push(l)
      }
      for (const h of f.hunks) h.patch = header + h.lines.join('\n') + '\n'
    }
    files.push(f)
  }
  return files
}

export type GraphRow = { graph: string; hash?: string; refs?: string; subject?: string; author?: string; when?: string }
export function parseGraph(out: string): GraphRow[] {
  return out.split('\n').filter(Boolean).map((l) => {
    const i = l.indexOf('\u001f')
    if (i < 0) return { graph: l }
    const firstField = l.slice(0, i)
    const m = /^(.*?)([0-9a-f]{7,40})$/.exec(firstField)
    const [refs, subject, author, when] = l.slice(i + 1).split('\u001f')
    return { graph: m ? m[1] : firstField, hash: m?.[2], refs: refs.trim().replace(/^\(|\)$/g, ''), subject, author, when }
  })
}

export type Worktree = { path: string; head?: string; branch?: string; bare?: boolean; detached?: boolean; locked?: boolean; prunable?: boolean }
export function parseWorktrees(out: string): Worktree[] {
  return out.split('\n\n').filter((b) => b.trim()).map((b) => {
    const w: Worktree = { path: '' }
    for (const l of b.split('\n')) {
      const [k, ...v] = l.split(' ')
      const val = v.join(' ')
      if (k === 'worktree') w.path = val
      else if (k === 'HEAD') w.head = val.slice(0, 8)
      else if (k === 'branch') w.branch = val.replace('refs/heads/', '')
      else if (k === 'bare') w.bare = true
      else if (k === 'detached') w.detached = true
      else if (k === 'locked') w.locked = true
      else if (k === 'prunable') w.prunable = true
    }
    return w
  })
}

export type Branch = { name: string; hash: string; current: boolean; remote: boolean; track: string; when: string }
/** `git branch --all --format=%(refname:short)<US>%(objectname:short)<US>%(HEAD)<US>%(upstream:track)<US>%(committerdate:relative)` */
export function parseBranches(out: string): Branch[] {
  return out.split('\n').filter(Boolean).map((l) => {
    const [name, hash, head, track, when] = l.split('\u001f')
    return { name, hash, current: head === '*', remote: /^(remotes\/|origin\/)/.test(name) || name.includes('/HEAD'), track: (track ?? '').replace(/^\[|\]$/g, ''), when: when ?? '' }
  }).filter((b) => !b.name.endsWith('/HEAD') && b.name !== 'origin' && !b.name.startsWith('(HEAD detached'))
}
export type Compare = { ahead: { hash: string; subject: string; when: string }[]; behind: { hash: string; subject: string; when: string }[] }
/** `git log --left-right --pretty=%m<US>%h<US>%s<US>%cr base...HEAD`: '<' only on base, '>' only on HEAD. */
export function parseCompare(out: string): Compare {
  const c: Compare = { ahead: [], behind: [] }
  for (const l of out.split('\n').filter(Boolean)) {
    const [m, hash, subject, when] = l.split('\u001f')
    ;(m === '>' ? c.ahead : c.behind).push({ hash, subject, when })
  }
  return c
}
/** The https page of a GitHub origin, from an https or ssh remote URL; undefined otherwise. */
export function githubRepo(remote: string): string | undefined {
  const m = /^(?:https:\/\/(?:[^@/]+@)?github\.com\/|git@github\.com:|ssh:\/\/git@github\.com\/)([\w.-]+)\/([\w.-]+?)(?:\.git)?\/?$/.exec(remote.trim())
  return m ? `https://github.com/${m[1]}/${m[2]}` : undefined
}

/** One commit of `log-dag` (`%H %P %d %s %an %cr`, unit-separated). */
export type DagCommit = { hash: string; parents: string[]; refs: string; subject: string; author: string; when: string }
export function parseDag(out: string): DagCommit[] {
  return out.split('\n').filter((l) => l.includes('\u001f')).map((l) => {
    const [hash, parents, refs, subject, author, when] = l.split('\u001f')
    return { hash, parents: parents.split(' ').filter(Boolean), refs: refs.trim().replace(/^\(|\)$/g, ''), subject, author, when }
  })
}

/** A drawn row of the commit graph. Lanes are columns; `color` indexes a palette. Lines that
 * pass the row (`through`), come down into the commit from a lane above (`into`), or leave it
 * for a lane below (`out`, its parents). */
export type DagRow = {
  commit: DagCommit; col: number; color: number; merge: boolean
  through: { lane: number; color: number }[]
  into: { from: number; color: number }[]
  out: { to: number; color: number }[]
}

/** Lay commits (newest first, as `git log --date-order` lists them) out in lanes: a commit takes
 * the lane that waits for it (or a free one, a branch tip); its first parent continues that lane;
 * other parents (a merge) join a lane already waiting for them or start one. Lanes that waited
 * for the same commit end there. Freed lanes are reused, so the graph stays narrow. */
export function layoutDag(commits: DagCommit[]): { rows: DagRow[]; lanes: number } {
  const lanes: (string | null)[] = []
  const colors: number[] = []
  let next = 0
  let widest = 0
  const free = (avoid: number): number => { const i = lanes.findIndex((l, k) => l === null && k !== avoid); return i < 0 ? lanes.length : i }
  const rows = commits.map((commit) => {
    const before = [...lanes]
    let col = lanes.indexOf(commit.hash)
    if (col < 0) { col = free(-1); lanes[col] = commit.hash; colors[col] = next++ }
    const into = before.flatMap((h, i) => (h === commit.hash ? [{ from: i, color: colors[i] }] : []))
    for (const { from } of into) if (from !== col) lanes[from] = null
    const color = colors[col]
    const [first, ...rest] = commit.parents
    lanes[col] = first ?? null
    const out: DagRow['out'] = first ? [{ to: col, color }] : []
    for (const parent of rest) {
      let j = lanes.indexOf(parent)
      if (j < 0) { j = free(col); lanes[j] = parent; colors[j] = next++ }
      out.push({ to: j, color: colors[j] })
    }
    const through = before.flatMap((h, i) => (h !== null && h !== commit.hash && i !== col && lanes[i] === h ? [{ lane: i, color: colors[i] }] : []))
    while (lanes.length && lanes[lanes.length - 1] === null) lanes.pop()
    widest = Math.max(widest, before.length, lanes.length, col + 1)
    return { commit, col, color, merge: commit.parents.length > 1, through, into, out }
  })
  return { rows, lanes: widest }
}
