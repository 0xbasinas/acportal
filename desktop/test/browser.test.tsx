import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { BrowserPanel } from '../src/Browser'
import { Composer, type Attachment } from '../src/Session'
import type { BrowserState } from '../electron/browserTypes'

const page: BrowserState = { tabs: [], open: true, url: 'http://localhost:5199/', title: 'Calc demo', loading: false, canBack: true, canForward: false, zoom: 100, local: true, remembers: false, loadedMs: 300 }

function fakeBrowser(initial: BrowserState = page, selection = 'Sum: 42') {
  let push: (s: BrowserState) => void = () => {}
  const api = {
    open: vi.fn(async () => ({ ok: true as const, value: initial })),
    go: vi.fn(async () => ({ ok: true as const, value: initial })),
    newTab: vi.fn(async () => ({ ok: true as const, value: initial })),
    selectTab: vi.fn(async () => ({ ok: true as const, value: initial })),
    closeTab: vi.fn(async () => ({ ok: true as const, value: initial })),
    find: vi.fn(async () => ({ ok: true as const, value: undefined })),
    findStop: vi.fn(async () => ({ ok: true as const, value: undefined })),
    close: vi.fn(async () => ({ ok: true as const, value: undefined })),
    state: vi.fn(async () => ({ ok: true as const, value: initial })),
    bounds: vi.fn(async () => ({ ok: true as const, value: undefined })),
    back: vi.fn(async () => ({ ok: true as const, value: undefined })),
    forward: vi.fn(async () => ({ ok: true as const, value: undefined })),
    reload: vi.fn(async () => ({ ok: true as const, value: undefined })),
    zoom: vi.fn(async () => ({ ok: true as const, value: undefined })),
    devtools: vi.fn(async () => ({ ok: true as const, value: undefined })),
    external: vi.fn(async () => ({ ok: true as const, value: undefined })),
    freeze: vi.fn(async () => ({ ok: true as const, value: 'data:image/png;base64,AAAA' })),
    thaw: vi.fn(async () => ({ ok: true as const, value: undefined })),
    capture: vi.fn(async () => ({ ok: true as const, value: { data: 'QUJD', mimeType: 'image/jpeg', size: 3, width: 10, height: 10, title: 'Calc demo', url: 'http://localhost:5199/' } })),
    selection: vi.fn(async () => ({ ok: true as const, value: selection })),
    onState: vi.fn((cb: (s: BrowserState) => void) => { push = cb; return () => {} }),
    onShortcut: vi.fn(() => () => {})
  }
  ;(window as unknown as { api: unknown }).api = { browser: api }
  return { api, push: (s: BrowserState) => act(() => push(s)) }
}

const panel = (extra: Partial<Parameters<typeof BrowserPanel>[0]> = {}) => {
  const props = { where: 'side' as const, size: 620, onSize: vi.fn(), onClose: vi.fn(), onWhere: vi.fn(), onShare: vi.fn(), canShare: true, ...extra }
  render(<BrowserPanel {...props} />)
  return props
}

beforeEach(() => { document.body.innerHTML = '' })
afterEach(() => cleanup())

describe('the browser panel', () => {
  it('offers a page to open when there is none, and the quick addresses go to the browser', async () => {
    const b = fakeBrowser({ ...page, url: '' })
    panel()
    await screen.findByText('Preview a page')
    fireEvent.click(screen.getByRole('button', { name: 'localhost:3000' }))
    expect(b.api.go).toHaveBeenCalledWith('localhost:3000')
    // With no page there is nothing for the native view to cover.
    await waitFor(() => expect(b.api.bounds).toHaveBeenCalledWith(null))
  })

  it('shows the address and goes where it is told, and says so when a page does not load', async () => {
    const b = fakeBrowser()
    panel()
    const input = (await screen.findByLabelText('Address')) as HTMLInputElement
    await waitFor(() => expect(input.value).toBe('http://localhost:5199/'))
    fireEvent.change(input, { target: { value: '127.0.0.1:3000' } })
    fireEvent.submit(input.closest('form')!)
    expect(b.api.go).toHaveBeenCalledWith('127.0.0.1:3000')
    b.push({ ...page, url: 'http://localhost:9/', error: 'Nothing is listening at this address. Is the dev server running?' })
    await screen.findByText('This page did not load')
    expect(screen.getByText(/Is the dev server running/)).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: /Try again/ }))
    expect(b.api.reload).toHaveBeenCalled()
  })

  it('tells the person when a link went to their own browser or was refused', async () => {
    const b = fakeBrowser()
    panel()
    await screen.findByLabelText('Address')
    b.push({ ...page, note: 'Opened in your own browser: example.org' })
    expect(screen.getByRole('status').textContent).toBe('Opened in your own browser: example.org')
  })

  it('moves between the side and the dock, and closes, on request', async () => {
    fakeBrowser()
    const props = panel()
    await screen.findByLabelText('Address')
    fireEvent.click(screen.getByRole('button', { name: 'Dock at the bottom' }))
    expect(props.onWhere).toHaveBeenCalledWith('dock')
    fireEvent.click(screen.getByRole('button', { name: 'Close the browser' }))
    expect(props.onClose).toHaveBeenCalled()
  })

  it('hides the page behind a still picture while a menu covers it, then shows it again', async () => {
    const b = fakeBrowser()
    panel()
    await screen.findByLabelText('Address')
    fireEvent.click(screen.getByRole('button', { name: /Send to agent/ }))
    await screen.findByRole('menu')
    await waitFor(() => expect(b.api.freeze).toHaveBeenCalled())
    await waitFor(() => expect(b.api.bounds).toHaveBeenLastCalledWith(null))
    fireEvent.click(screen.getByRole('button', { name: 'Close the menu' }))
    await waitFor(() => expect(b.api.thaw).toHaveBeenCalled())
  })

  it('sends a screenshot, the selected text or the address to the agent, and nothing else', async () => {
    const b = fakeBrowser()
    const props = panel()
    await screen.findByLabelText('Address')
    fireEvent.click(screen.getByRole('button', { name: /Send to agent/ }))
    fireEvent.click(await screen.findByRole('menuitem', { name: /Screenshot of the page/ }))
    await waitFor(() => expect(props.onShare).toHaveBeenCalledTimes(1))
    const shot = (props.onShare.mock.calls[0][0].attachments as Attachment[])[0]
    expect(shot).toMatchObject({ mimeType: 'image/jpeg', data: 'QUJD', uri: 'http://localhost:5199/' })
    expect(b.api.capture).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: /Send to agent/ }))
    fireEvent.click(await screen.findByRole('menuitem', { name: /Selected text/ }))
    await waitFor(() => expect(props.onShare).toHaveBeenCalledTimes(2))
    expect((props.onShare.mock.calls[1][0].attachments as Attachment[])[0]).toMatchObject({ mimeType: 'text/plain', text: 'Sum: 42', uri: 'http://localhost:5199/' })

    fireEvent.click(screen.getByRole('button', { name: /Send to agent/ }))
    fireEvent.click(await screen.findByRole('menuitem', { name: /Page address/ }))
    await waitFor(() => expect(props.onShare).toHaveBeenCalledTimes(3))
    expect(props.onShare.mock.calls[2][0]).toEqual({ append: 'http://localhost:5199/' })
  })

  it('cannot share selected text when nothing is selected, and cannot share at all without a session', async () => {
    fakeBrowser(page, '')
    panel()
    await screen.findByLabelText('Address')
    fireEvent.click(screen.getByRole('button', { name: /Send to agent/ }))
    const text = await screen.findByRole('menuitem', { name: /Selected text/ })
    expect((text as HTMLButtonElement).disabled).toBe(true)
    cleanup()
    fakeBrowser()
    panel({ canShare: false })
    await screen.findByLabelText('Address')
    expect((screen.getByRole('button', { name: /Send to agent/ }) as HTMLButtonElement).disabled).toBe(true)
  })
})

describe('tabs and find in page', () => {
  const tabs = [{ id: 1, title: 'Calc demo', url: 'http://localhost:5199/', active: true, loading: false }, { id: 2, title: 'Docs', url: 'http://localhost:3000/', active: false, loading: false }]
  it('lists the tabs, switches, opens and closes them', async () => {
    const b = fakeBrowser({ ...page, tabs })
    panel()
    fireEvent.click(await screen.findByRole('tab', { name: 'Docs' }))
    expect(b.api.selectTab).toHaveBeenCalledWith(2)
    fireEvent.click(screen.getByRole('button', { name: 'New tab' }))
    expect(b.api.newTab).toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Close Docs' }))
    expect(b.api.closeTab).toHaveBeenCalledWith(2)
  })
  it('finds text, moves between matches and stops on Escape', async () => {
    const b = fakeBrowser({ ...page, tabs })
    panel()
    fireEvent.click(await screen.findByRole('button', { name: 'Find in page' }))
    const box = await screen.findByLabelText('Find in page', { selector: 'input' })
    fireEvent.change(box, { target: { value: 'Sum' } })
    expect(b.api.find).toHaveBeenLastCalledWith('Sum', true, false)
    b.push({ ...page, tabs, find: { matches: 3, active: 1 } })
    expect(screen.getByText('1 of 3')).toBeTruthy()
    fireEvent.keyDown(box, { key: 'Enter' })
    expect(b.api.find).toHaveBeenLastCalledWith('Sum', true, true)
    fireEvent.keyDown(box, { key: 'Enter', shiftKey: true })
    expect(b.api.find).toHaveBeenLastCalledWith('Sum', false, true)
    fireEvent.keyDown(box, { key: 'Escape' })
    expect(b.api.findStop).toHaveBeenCalled()
  })
  it('says when nothing matches', async () => {
    const b = fakeBrowser({ ...page, tabs })
    panel()
    fireEvent.click(await screen.findByRole('button', { name: 'Find in page' }))
    fireEvent.change(await screen.findByLabelText('Find in page', { selector: 'input' }), { target: { value: 'zzz' } })
    b.push({ ...page, tabs, find: { matches: 0, active: 0 } })
    expect(screen.getByText('No matches')).toBeTruthy()
  })
})

describe('what the browser puts in the message box', () => {
  const composer = (incoming: Parameters<typeof Composer>[0]['incoming'], opts: { canImage?: boolean; canResource?: boolean } = {}) =>
    render(<Composer agent="Goose" busy={false} onSend={() => {}} onCancel={() => {}} history={[]} canImage={opts.canImage ?? true} canResource={opts.canResource ?? true} incoming={incoming} />)
  const image: Attachment = { name: 'localhost.jpg', mimeType: 'image/jpeg', size: 1000, data: 'QUJD', path: 'http://localhost:5199/', uri: 'http://localhost:5199/' }
  const text: Attachment = { name: 'selected text', mimeType: 'text/plain', size: 7, text: 'Sum: 42', path: '', uri: 'http://localhost:5199/' }

  it('adds the attachments and appends the address', () => {
    const { rerender } = composer(undefined)
    rerender(<Composer agent="Goose" busy={false} onSend={() => {}} onCancel={() => {}} history={[]} canImage canResource incoming={{ n: 1, attachments: [image, text] }} />)
    expect(screen.getByText('localhost.jpg')).toBeTruthy()
    expect(screen.getByText('selected text')).toBeTruthy()
    rerender(<Composer agent="Goose" busy={false} onSend={() => {}} onCancel={() => {}} history={[]} canImage canResource incoming={{ n: 2, append: 'http://localhost:5199/' }} />)
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('http://localhost:5199/')
  })

  it('says so when the agent does not take that kind of attachment, and adds nothing', () => {
    const { rerender } = composer(undefined, { canImage: false })
    rerender(<Composer agent="Goose" busy={false} onSend={() => {}} onCancel={() => {}} history={[]} canImage={false} canResource incoming={{ n: 1, attachments: [image] }} />)
    expect(screen.getByText(/Goose does not accept images in prompts/)).toBeTruthy()
    expect(screen.queryByText('localhost.jpg')).toBeNull()
  })

  it('keeps attachments within the host\'s size limit', () => {
    const big: Attachment = { ...image, size: 700 * 1024 }
    const { rerender } = composer(undefined)
    rerender(<Composer agent="Goose" busy={false} onSend={() => {}} onCancel={() => {}} history={[]} canImage canResource incoming={{ n: 1, attachments: [big] }} />)
    expect(screen.getByText(/must stay under 600 KB/)).toBeTruthy()
    expect(screen.queryByText('localhost.jpg')).toBeNull()
  })
})
