import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { GitView } from '../src/Git'

type Call = { op: string; params?: Record<string, string> }

const diff = (path: string): string => `diff --git a/${path} b/${path}\nindex 111..222 100644\n--- a/${path}\n+++ b/${path}\n@@ -1 +1 @@\n-old\n+new\n`

/** A host with two modified files and one new file, that records every git operation it is asked for. */
function fakeHost(failOn?: string) {
  const calls: Call[] = []
  const run = vi.fn(async (_host: string, _cwd: string, op: string, params?: Record<string, string>) => {
    calls.push({ op, params })
    if ((op === 'add' || op === 'restore-staged') && params?.file === failOn) return { ok: false as const, error: 'index.lock exists' }
    const out =
      op === 'status' ? '## main...origin/main\n M a.txt\n M c.txt\n?? b.txt\n'
      : op === 'diff' && !params?.staged ? diff('a.txt') + diff('c.txt')
      : ''
    return { ok: true as const, value: { out, args: ['git', op] } }
  })
  ;(window as unknown as { api: unknown }).api = {
    git: { available: async () => ({ ok: true, value: 'host' }), explain: async () => ({ ok: true, value: {} }), run }
  }
  return { calls, files: (op: string) => calls.filter((c) => c.op === op).map((c) => c.params?.file) }
}

async function open() {
  render(<GitView hostId="h" repos={[{ path: '/w', label: 'w' }]} onBack={() => {}} agents={[]} askAgent={() => {}} openSession={() => {}} />)
  await screen.findByText('b.txt')
}

describe('staging several files at once', () => {
  it('stages every ticked file, one git add each, and nothing else', async () => {
    const host = fakeHost()
    await open()
    expect((screen.getByRole('button', { name: 'Stage selected' }) as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(screen.getByLabelText('Select a.txt'))
    fireEvent.click(screen.getByLabelText('Select b.txt'))
    fireEvent.click(screen.getByRole('button', { name: 'Stage 2 selected' }))
    await waitFor(() => expect(host.files('add')).toEqual(['b.txt', 'a.txt']))
    expect(host.files('add')).not.toContain('c.txt')
  })

  it('selects all, stages all, and takes them all back with one Undo', async () => {
    const host = fakeHost()
    await open()
    fireEvent.click(screen.getAllByRole('checkbox', { name: /Select all/ })[0])
    expect(screen.getByText('3 selected')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Stage 3 selected' }))
    await waitFor(() => expect(host.files('add')).toEqual(['b.txt', 'a.txt', 'c.txt']))
    await screen.findByText('Staged 3 files')

    fireEvent.keyDown(document.body, { key: 'z', ctrlKey: true })
    await waitFor(() => expect(host.files('restore-staged')).toEqual(['b.txt', 'a.txt', 'c.txt']))
  })

  it('"Stage all" needs no ticks', async () => {
    const host = fakeHost()
    await open()
    fireEvent.click(screen.getByRole('button', { name: 'Stage all' }))
    await waitFor(() => expect(host.files('add')).toEqual(['b.txt', 'a.txt', 'c.txt']))
  })

  it('stops at the first failure and keeps what was done undoable', async () => {
    const host = fakeHost('a.txt')
    await open()
    fireEvent.click(screen.getByRole('button', { name: 'Stage all' }))
    await screen.findByText(/index.lock exists/)
    expect(screen.getByText(/1 of 3 files staged before it stopped/)).toBeTruthy()
    expect(host.files('add')).toEqual(['b.txt', 'a.txt'])
    expect(host.files('add')).not.toContain('c.txt')
    // the one file that was staged can still be taken back
    fireEvent.keyDown(document.body, { key: 'z', ctrlKey: true })
    await waitFor(() => expect(host.files('restore-staged')).toEqual(['b.txt']))
  })
})
