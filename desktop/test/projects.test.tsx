import { describe, expect, it } from 'vitest'
import { groupByProject, inside, isScratch, plain, subfolder } from '../src/projects'

describe('sessions grouped under projects', () => {
  const projects = [{ path: 'C:\\Work\\app', name: 'app' }, { path: 'C:\\Work\\app\\web', name: 'web' }, { path: 'C:\\Work\\lib', name: 'lib' }, { path: 'C:\\Work\\idle', name: 'idle' }]
  const s = (id: string, workspace: string) => ({ id, workspace })
  it('matches folders like Windows does: either slash, any case, whole names only', () => {
    expect(inside('c:/work/APP/src', 'C:\\Work\\app')).toBe(true)
    expect(inside('C:\\Work\\app', 'C:\\Work\\app\\')).toBe(true)
    expect(inside('C:\\Work\\apple', 'C:\\Work\\app')).toBe(false)
  })
  it('puts each session under its deepest project, recent projects first, then idle ones, then no project', () => {
    const groups = groupByProject([
      s('1', 'C:\\Work\\lib'), s('2', 'C:\\Work\\app\\src'), s('3', 'C:\\Work\\app\\web'), s('4', 'C:\\Users\\me\\.acpd\\scratch\\x'), s('5', 'C:\\Work\\app')
    ], projects)
    expect(groups.map((g) => [g.name, g.sessions.map((x) => x.id)])).toEqual([
      ['app', ['5', '2']], ['web', ['3']], ['lib', ['1']], ['idle', []], ['No project', ['4']]
    ])
    expect(groupByProject([], projects).map((g) => g.name)).toEqual(['app', 'web', 'lib', 'idle'])
  })
})

// A Windows host lists projects as C:\x but reports a session's folder as \\?\C:\x.
describe('canonical Windows folders', () => {
  const verbatim = String.raw`\\?\C:\Users\me\acportal`
  const projects = [{ path: String.raw`C:\Users\me\acportal`, name: 'acportal' }, { path: String.raw`\\srv\share\work`, name: 'work' }]
  const s = (id: string, workspace: string) => ({ id, workspace })
  it('treats the prefixed and plain forms as one folder, so the session lands in its project', () => {
    expect(inside(verbatim, String.raw`C:\Users\me\acportal`)).toBe(true)
    expect(inside(verbatim + String.raw`\desktop`, String.raw`C:\Users\me\acportal`)).toBe(true)
    expect(inside(verbatim + 'x', String.raw`C:\Users\me\acportal`)).toBe(false)
    expect(inside(String.raw`\\?\UNC\srv\share\work\app`, String.raw`\\srv\share\work`)).toBe(true)
    const groups = groupByProject([s('1', verbatim), s('2', String.raw`\\?\C:\Users\me\scratch\x`), s('3', String.raw`\\?\UNC\srv\share\work\app`)], projects)
    expect(groups.map((g) => [g.name, g.sessions.map((x) => x.id)])).toEqual([['work', ['3']], ['acportal', ['1']], ['No project', ['2']]])
  })
  it('names the sub-folder as the session reports it, with or without the prefix', () => {
    expect(subfolder(verbatim + String.raw`\desktop\src`, String.raw`C:\Users\me\acportal`)).toBe(String.raw`desktop\src`)
    expect(subfolder(String.raw`C:\Users\me\acportal\desktop`, 'C:\\Users\\me\\acportal\\')).toBe('desktop')
    expect(subfolder(verbatim, String.raw`C:\Users\me\acportal`)).toBeUndefined()
    expect(subfolder(String.raw`C:\elsewhere`, String.raw`C:\Users\me\acportal`)).toBeUndefined()
    expect(plain(String.raw`\\?\UNC\srv\share`)).toBe(String.raw`\\srv\share`)
    expect(plain('/home/me')).toBe('/home/me')
  })
})

// A host whose state sits inside a project (a development host keeps it in the repository) makes
// scratch folders inside that project. They are still "No project".
describe('scratch folders inside a project', () => {
  const projects = [{ path: String.raw`C:\Users\me\acportal`, name: 'acportal' }]
  const root = String.raw`C:\Users\me\acportal\.local\dev-host\state\scratch`
  const scratch = String.raw`\\?\C:\Users\me\acportal\.local\dev-host\state\scratch\1792266951-283465`
  const s = (id: string, workspace: string) => ({ id, workspace })
  it('uses the root the host reports', () => {
    expect(isScratch(scratch, root)).toBe(true)
    expect(isScratch(root, root)).toBe(false)
    expect(isScratch(String.raw`C:\Users\me\acportal\desktop`, root)).toBe(false)
    const groups = groupByProject([s('proj', String.raw`\\?\C:\Users\me\acportal\desktop`), s('pen', scratch)], projects, root)
    expect(groups.map((g) => [g.name, g.sessions.map((x) => x.id)])).toEqual([['acportal', ['proj']], ['No project', ['pen']]])
  })
  it('recognises the folder name when an older host does not report a root', () => {
    expect(isScratch(scratch)).toBe(true)
    expect(isScratch(String.raw`C:\Users\me\acportal\scratch\notes`)).toBe(false)
    const groups = groupByProject([s('pen', scratch)], projects)
    expect(groups.map((g) => [g.name, g.sessions.map((x) => x.id)])).toEqual([['acportal', []], ['No project', ['pen']]])
  })
})
