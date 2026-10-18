import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'

const written: Uint8Array[] = []
vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    rows = 24; cols = 80; options = {}
    loadAddon(): void {}
    open(): void {}
    write(b: Uint8Array): void { written.push(b) }
    onData(): { dispose: () => void } { return { dispose: () => {} } }
    onResize(): { dispose: () => void } { return { dispose: () => {} } }
    dispose(): void {}
  }
}))
vi.mock('@xterm/addon-fit', () => ({ FitAddon: class { fit(): void {} } }))
vi.mock('@xterm/addon-web-links', () => ({ WebLinksAddon: class {} }))
vi.mock('@xterm/xterm/css/xterm.css', () => ({}))

import { TerminalDock, devServerUrl, type TermInfo } from '../src/Terminal'

const info: TermInfo = { id: 't1', folder: '/work/app', shell: 'bash', rows: 24, cols: 80 }

function fakeApi() {
  let data: (k: string, b: Uint8Array) => void = () => {}
  let event: (k: string, e: Record<string, unknown>) => void = () => {}
  const term = {
    attach: vi.fn(async (_h: string, id: string) => ({ ok: true as const, value: `h/${id}` })),
    input: vi.fn(async () => ({ ok: true as const, value: undefined })),
    resize: vi.fn(async () => ({ ok: true as const, value: undefined })),
    take: vi.fn(async () => ({ ok: true as const, value: undefined })),
    detach: vi.fn(async () => ({ ok: true as const, value: undefined })),
    onData: vi.fn((cb: typeof data) => { data = cb; return () => {} }),
    onEvent: vi.fn((cb: typeof event) => { event = cb; return () => {} }),
    onState: vi.fn(() => () => {})
  }
  ;(window as unknown as { api: unknown }).api = { term }
  return { term, data: (k: string, s: string) => act(() => data(k, new TextEncoder().encode(s))), event: (k: string, e: Record<string, unknown>) => act(() => event(k, e)) }
}

const dock = (extra: Partial<Parameters<typeof TerminalDock>[0]> = {}, list: TermInfo[] = [info]) => {
  const api = vi.fn(async (method: string, path: string) => {
    if (method === 'GET') return { ok: true as const, value: { terminals: list } }
    if (method === 'POST') return { ok: true as const, value: { ...info, id: 't2' } }
    return { ok: true as const, value: undefined }
  })
  const props = { hostId: 'h', hostName: 'Studio', folder: '/work/app', enabled: true, height: 300, onHeight: vi.fn(), onClose: vi.fn(), onOpenUrl: vi.fn(), api: api as never, ...extra }
  render(<TerminalDock {...props} />)
  return { props, api }
}

beforeEach(() => { document.body.innerHTML = ''; written.length = 0 })
afterEach(() => cleanup())

describe('finding the dev server in the output', () => {
  it('picks the local address out of colourful output', () => {
    expect(devServerUrl('  \x1b[32m➜\x1b[39m  \x1b[1mLocal\x1b[22m:   \x1b[36mhttp://localhost:\x1b[1m5173\x1b[22m/\x1b[39m')).toBe('http://localhost:5173/')
    expect(devServerUrl('listening on http://0.0.0.0:3000')).toBe('http://localhost:3000')
    expect(devServerUrl('see http://127.0.0.1:8080/app.')).toBe('http://127.0.0.1:8080/app')
  })
  it('ignores other sites and ports-less addresses', () => {
    expect(devServerUrl('docs at https://example.com:8443/x')).toBeUndefined()
    expect(devServerUrl('http://localhost/')).toBeUndefined()
  })
})

describe('the terminal dock', () => {
  it('says how to turn terminals on when the host has them off', () => {
    fakeApi()
    const { api } = dock({ enabled: false })
    expect(screen.getByText('Terminals are off on this host')).toBeTruthy()
    expect(screen.getByText(/enabled = true/)).toBeTruthy()
    expect(api).not.toHaveBeenCalled()
  })

  it('attaches to the terminals that already exist on the host', async () => {
    const f = fakeApi()
    dock()
    await waitFor(() => expect(f.term.attach).toHaveBeenCalledWith('h', 't1', 0))
    expect(screen.getByRole('tab', { name: 'bash 1' })).toBeTruthy()
  })

  it('starts a first shell in the session folder when there is none', async () => {
    const f = fakeApi()
    const { api } = dock({}, [])
    await waitFor(() => expect(api).toHaveBeenCalledWith('POST', '/v1/terminals', { folder: '/work/app', rows: 24, cols: 80 }))
    await waitFor(() => expect(f.term.attach).toHaveBeenCalledWith('h', 't2', 0))
  })

  it('offers to open a dev server in the browser, once', async () => {
    const f = fakeApi()
    const { props } = dock()
    await waitFor(() => expect(f.term.onData).toHaveBeenCalled())
    f.data('h/t1', 'ready on http://localhost:5199/\r\n')
    fireEvent.click(await screen.findByRole('button', { name: /Open in the browser/ }))
    expect(props.onOpenUrl).toHaveBeenCalledWith('http://localhost:5199/')
  })

  it('lets this computer take the keyboard back, and reports a finished shell', async () => {
    const f = fakeApi()
    dock()
    await waitFor(() => expect(f.term.onEvent).toHaveBeenCalled())
    f.event('h/t1', { type: 'hello', you: false })
    fireEvent.click(await screen.findByRole('button', { name: 'Take control' }))
    expect(f.term.take).toHaveBeenCalledWith('h/t1')
    f.event('h/t1', { type: 'exit', code: 3 })
    expect(await screen.findByText(/The shell ended \(exit code 3\)/)).toBeTruthy()
  })

  it('closes a terminal on the host when its tab is closed', async () => {
    const f = fakeApi()
    const { api } = dock()
    await screen.findByRole('tab', { name: 'bash 1' })
    fireEvent.click(screen.getByRole('button', { name: 'Close bash 1' }))
    await waitFor(() => expect(api).toHaveBeenCalledWith('DELETE', '/v1/terminals/t1'))
    await waitFor(() => expect(f.term.detach).toHaveBeenCalledWith('h/t1'))
  })
})
