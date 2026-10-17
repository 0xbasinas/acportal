import { describe, expect, it } from 'vitest'
import { groupByProject, inside } from '../src/projects'

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
