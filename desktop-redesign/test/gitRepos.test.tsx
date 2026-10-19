import { describe, expect, it } from 'vitest'
import { gitRepos } from '../src/gitRepos'

describe('repositories listed by the Git view', () => {
  const workspaces = [{ path: String.raw`C:\Users\me\acportal` }, { path: String.raw`C:\Work\lib` }]
  const tasks = [
    { worktree: { path: String.raw`C:\state\worktrees\a`, branch: 'task/a', outcome: 'open' } },
    { worktree: { path: String.raw`C:\state\worktrees\b`, branch: 'task/b', outcome: 'merged' } },
    {}
  ]

  it('puts the folder it was opened for first, then the session, the projects and open task worktrees', () => {
    const repos = gitRepos({ asked: String.raw`C:\Work\lib`, sessionFolder: String.raw`C:\Users\me\acportal\desktop`, workspaces, tasks })
    expect(repos.map((r) => r.label)).toEqual(['lib', 'desktop · session', 'acportal', 'task/a · task'])
    expect(repos[0].path).toBe(String.raw`C:\Work\lib`)
  })

  it('lists a folder once, however it is written, so a session in a project does not duplicate it', () => {
    const repos = gitRepos({ asked: String.raw`C:\Users\me\acportal`, sessionFolder: String.raw`\\?\C:\Users\me\acportal`, workspaces, tasks: [] })
    expect(repos.map((r) => r.label)).toEqual(['acportal', 'lib'])
    // the folder asked for is kept as the app wrote it, not as the session reported it
    expect(repos[0].path).toBe(String.raw`C:\Users\me\acportal`)
  })

  it('without a request it starts with the session, as before', () => {
    const repos = gitRepos({ sessionFolder: String.raw`\\?\C:\Work\lib`, workspaces, tasks: [] })
    expect(repos.map((r) => r.label)).toEqual(['lib · session', 'acportal'])
  })
})
