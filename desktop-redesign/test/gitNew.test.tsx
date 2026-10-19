import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { GitView } from '../src/Git'

const modified = (path: string): string => `diff --git a/${path} b/${path}\nindex 111..222 100644\n--- a/${path}\n+++ b/${path}\n@@ -1 +1 @@\n-old line\n+changed line\n`
const added = (path: string): string => `diff --git a/${path} b/${path}\nnew file mode 100644\nindex 0000000..333\n--- /dev/null\n+++ b/${path}\n@@ -0,0 +1,2 @@\n+brand new line one\n+brand new line two\n`

/** Two modified files and two new files; records which operations were asked for. */
function fakeHost(opts: { newFails?: boolean } = {}) {
  const calls: { op: string; file?: string }[] = []
  const run = vi.fn(async (_host: string, _cwd: string, op: string, params?: Record<string, string>) => {
    calls.push({ op, file: params?.file })
    if (op === 'diff-new' && opts.newFails) return { ok: false as const, error: 'Host answered 404 (unknown_git_operation)' }
    const out =
      op === 'status' ? '## main...origin/main\n M a.txt\n M c.txt\n?? b.txt\n?? d.txt\n'
      : op === 'diff' && !params?.staged ? modified('a.txt') + modified('c.txt')
      : op === 'diff-new' ? added(params!.file)
      : ''
    return { ok: true as const, value: { out, args: ['git', op] } }
  })
  ;(window as unknown as { api: unknown }).api = {
    git: { available: async () => ({ ok: true, value: 'host' }), explain: async () => ({ ok: true, value: {} }), run }
  }
  return { newReads: () => calls.filter((c) => c.op === 'diff-new').map((c) => c.file) }
}

async function open() {
  render(<GitView hostId="h" repos={[{ path: '/w', label: 'w' }]} onBack={() => {}} agents={[]} askAgent={() => {}} openSession={() => {}} />)
  await screen.findByText('b.txt')
}

describe('new files in the Git view', () => {
  it('show only their name until opened, then their content, read once per open', async () => {
    const host = fakeHost()
    await open()
    expect(screen.queryByText('brand new line one')).toBeNull()
    expect(host.newReads()).toEqual([])

    fireEvent.click(screen.getByRole('button', { name: 'Show the new file b.txt' }))
    await screen.findByText('brand new line one')
    expect(screen.getByText('brand new line two')).toBeTruthy()
    expect(host.newReads()).toEqual(['b.txt'])

    fireEvent.click(screen.getByRole('button', { name: 'Hide the new file b.txt' }))
    expect(screen.queryByText('brand new line one')).toBeNull()
  })

  it('say so when the host cannot show a new file', async () => {
    fakeHost({ newFails: true })
    await open()
    fireEvent.click(screen.getByRole('button', { name: 'Show the new file b.txt' }))
    await screen.findByText(/This host is older than the app and cannot show a new file yet/)
    expect(screen.getByText(/rebuild it \(cargo build -p acpd/)).toBeTruthy()
    expect(screen.queryByText(/unknown_git_operation/)).toBeNull()
  })
})

describe('Expand all and Collapse all', () => {
  it('open every file, new ones too, and fold every file back', async () => {
    const host = fakeHost()
    await open()
    // one tracked file folded by hand, to show that Expand all undoes it
    fireEvent.click(screen.getByRole('button', { name: 'Collapse a.txt' }))
    expect(screen.queryAllByText('changed line').length).toBe(1)

    fireEvent.click(screen.getByRole('button', { name: 'Expand all' }))
    await waitFor(() => expect(screen.getAllByText('brand new line one').length).toBe(2))
    expect(screen.getAllByText('changed line').length).toBe(2)
    expect(host.newReads().sort()).toEqual(['b.txt', 'd.txt'])

    fireEvent.click(screen.getByRole('button', { name: 'Collapse all' }))
    expect(screen.queryByText('changed line')).toBeNull()
    expect(screen.queryByText('brand new line one')).toBeNull()
    // the names stay, so each file can be opened again on its own
    expect(screen.getByText('a.txt')).toBeTruthy()
    expect(screen.getByText('d.txt')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Expand c.txt' }))
    expect(screen.getAllByText('changed line').length).toBe(1)
  })
})
