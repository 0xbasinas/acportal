// Pure parsers for git's porcelain output (unit-tested).
export type FileStatus = { path: string; x: string; y: string; staged: boolean; unstaged: boolean; untracked: boolean; conflict: boolean; orig?: string }
export type Status = { branch: string; upstream?: string; ahead: number; behind: number; files: FileStatus[] }
const CONFLICT = new Set(['DD', 'AU', 'UD', 'UA', 'DU', 'AA', 'UU'])

export function parseStatus(out: string): Status {
  const st: Status = { branch: '', ahead: 0, behind: 0, files: [] }
  for (const line of out.split('\n')) {
    if (!line) continue
    if (line.startsWith('## ')) {
      const m = /^## (?:No commits yet on )?([^.\s]+)(?:\.\.\.(\S+))?(?: \[(.*)\])?/.exec(line)
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
