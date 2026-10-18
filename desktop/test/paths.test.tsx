import { describe, expect, it } from 'vitest'
import { render } from '@testing-library/react'
import { rel } from '../src/Tasks'
import { DiffView, PathRoot } from '../src/ui'

// A Windows host reports a changed file as \\?\C:\x and the session folder in the same or the plain
// form, with backslashes. The panel used to show the whole path because only `/` was understood.
describe('changed file paths', () => {
  const folder = String.raw`C:\Users\me\acportal`
  const verbatim = String.raw`\\?\C:\Users\me\acportal`

  it('are shown relative to the session folder, in any combination of forms', () => {
    expect(rel(verbatim + String.raw`\README.md`, folder)).toBe('README.md')
    expect(rel(String.raw`C:\Users\me\acportal\desktop\src\App.tsx`, verbatim)).toBe(String.raw`desktop\src\App.tsx`)
    expect(rel(verbatim + String.raw`\docs\a.md`, verbatim)).toBe(String.raw`docs\a.md`)
    expect(rel('c:/users/ME/acportal/x.txt', folder)).toBe('x.txt')
  })

  it('keep working for a Linux host', () => {
    expect(rel('/home/me/app/src/a.rs', '/home/me/app')).toBe('src/a.rs')
    expect(rel('/home/me/app/a.rs', '/home/me/app/')).toBe('a.rs')
  })

  it('lose the verbatim prefix when they are outside the folder', () => {
    expect(rel(String.raw`\\?\C:\Other\a.md`, folder)).not.toContain('?')
    expect(rel(String.raw`\\?\C:\Other\a.md`, folder)).toContain('Other')
    expect(rel(String.raw`\\?\C:\Other\a.md`)).not.toContain('?')
  })

  it('are also relative in the diff card header', () => {
    const { container } = render(
      <PathRoot.Provider value={folder}>
        <DiffView d={{ path: verbatim + String.raw`\README.md`, oldText: 'a', newText: 'b' }} />
      </PathRoot.Provider>
    )
    expect(container.querySelector('.diff-path .mono')?.textContent).toBe('README.md')
    expect(container.querySelector('.diff-path .mono')?.getAttribute('title')).toBe(verbatim + String.raw`\README.md`)
  })
})
