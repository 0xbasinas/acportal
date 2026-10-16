// Component tests for the new-session composer (jsdom, no layout: geometry is stubbed).
import { describe, expect, it, vi, beforeEach } from 'vitest'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { NewSessionHome, type StartRequest } from '../src/NewSession'
import { placePopover, setupFromAcp } from '../src/agentSetup'
import type { AgentInfo } from '../src/types'

const agents: AgentInfo[] = [{ id: 'goose', name: 'Goose', status: 'available' }, { id: 'mock', name: 'Mock', status: 'available' }]
const workspaces = [{ path: '/w/project', name: 'project' }]
const hosts = [{ id: 'h1', name: 'Studio PC', state: 'Connected', ok: true }, { id: 'h2', name: 'Build box', state: 'Connected', ok: true }]
const US = '\u001f'
const branchOut = [['main', '*'], ['feature/a', ' '], ['feature/b', ' ']].map(([n, h]) => [n, 'abc', h, '', 'now'].join(US)).join('\n')
let git: { ok: true; value: { out: string } } | { ok: false; error: string }
let reduced = false
const animate = vi.fn()

beforeEach(() => {
  git = { ok: true, value: { out: branchOut } }
  reduced = false
  animate.mockReset()
  ;(window as unknown as { api: unknown }).api = {
    git: { run: vi.fn(async () => git) },
    files: { pick: vi.fn(async () => ({ ok: true, value: [] })) }
  }
  window.matchMedia = vi.fn((q: string) => ({ matches: q.includes('reduce') && reduced, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia
  Element.prototype.animate = animate as unknown as typeof Element.prototype.animate
})

function setup(over: Partial<Parameters<typeof NewSessionHome>[0]> = {}) {
  const onStart = vi.fn(async (_: StartRequest): Promise<string | undefined> => undefined)
  const onHost = vi.fn(); const onMulti = vi.fn()
  const props = { hosts, hostId: 'h1', onHost, agents, workspaces, mcp: [], canBranch: true, history: () => ['fix the tests', 'explain calc.py'], onStart, onMulti, ...over }
  const view = render(<NewSessionHome {...props} />)
  return { ...view, onStart: props.onStart as typeof onStart, onHost, onMulti }
}
const chip = (name: RegExp): HTMLElement => {
  const b = screen.getAllByRole('button').find((x) => x.getAttribute('aria-haspopup') === 'dialog' && name.test(x.textContent ?? ''))
  if (!b) throw new Error(`no chip ${name}`)
  return b
}
const prompt = () => screen.getByLabelText('Prompt for the new session') as HTMLTextAreaElement

describe('chips and dropdowns', () => {
  it('opens a dropdown, moves with the arrow keys, and Escape closes it and returns focus', async () => {
    setup()
    const agent = chip(/Goose/)
    fireEvent.click(agent)
    expect(agent.getAttribute('aria-expanded')).toBe('true')
    const dialog = screen.getByRole('dialog', { name: 'Agent' })
    const items = dialog.querySelectorAll<HTMLElement>('[data-item]')
    expect(document.activeElement).toBe(items[0])
    fireEvent.keyDown(dialog, { key: 'ArrowDown' })
    expect(document.activeElement).toBe(items[1])
    fireEvent.keyDown(dialog, { key: 'ArrowUp' })
    fireEvent.keyDown(dialog, { key: 'ArrowUp' })
    expect(document.activeElement).toBe(items[items.length - 1])
    fireEvent.keyDown(dialog, { key: 'Escape' })
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(agent.getAttribute('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(agent)
  })
  it('closes on an outside click', () => {
    setup()
    fireEvent.click(chip(/Read · Write/))
    expect(screen.getByRole('dialog', { name: 'Access rights' })).toBeTruthy()
    fireEvent.mouseDown(document.body)
    expect(screen.queryByRole('dialog')).toBeNull()
  })
  it('picks an agent, toggles access, and offers several agents with the prompt', () => {
    const { onMulti } = setup()
    fireEvent.click(chip(/Goose/))
    fireEvent.click(screen.getByText('Mock'))
    expect(chip(/Mock/)).toBeTruthy()
    fireEvent.click(chip(/Read · Write/))
    fireEvent.click(screen.getByText('Run commands'))
    expect(chip(/Read · Write$/)).toBeTruthy()
    fireEvent.change(prompt(), { target: { value: 'compare these' } })
    fireEvent.click(chip(/Mock/))
    fireEvent.click(screen.getByText(/Run on several agents/))
    expect(onMulti).toHaveBeenCalledWith('compare these')
  })
  it('host chip shows the active host and switches to another', () => {
    const { onHost } = setup()
    fireEvent.click(chip(/Studio PC/))
    fireEvent.click(screen.getByText('Build box'))
    expect(onHost).toHaveBeenCalledWith('h2')
  })
  it('recent prompts fill the box', () => {
    setup()
    fireEvent.click(screen.getByText('explain calc.py'))
    expect(prompt().value).toBe('explain calc.py')
  })
})

describe('branch picker', () => {
  it('shows current and existing branches, filters, and creates a new branch with a base', async () => {
    const { onStart } = setup()
    await waitFor(() => expect(chip(/main/)).toBeTruthy())
    fireEvent.click(chip(/main/))
    const dialog = screen.getByRole('dialog', { name: 'Branch' })
    expect(dialog.textContent).toContain('Current')
    expect(dialog.textContent).toContain('feature/a')
    const find = screen.getByLabelText('Find or create a branch')
    fireEvent.change(find, { target: { value: 'feature/b' } })
    expect(dialog.querySelector('.ctx-scroll')!.textContent).toBe('feature/b')
    fireEvent.change(find, { target: { value: 'try this' } })
    fireEvent.keyDown(find, { key: 'Enter' })
    expect(chip(/try-this/)).toBeTruthy()
    fireEvent.change(prompt(), { target: { value: 'go' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0].branch).toEqual({ mode: 'new', name: 'try-this', base: 'main', worktree: false })
  })
  it('picks an existing branch and turns on its own worktree', async () => {
    const { onStart } = setup()
    await waitFor(() => chip(/main/))
    fireEvent.click(chip(/main/))
    fireEvent.click(screen.getByRole('dialog', { name: 'Branch' }).querySelector('.ctx-scroll [data-item]')!)
    fireEvent.click(screen.getAllByRole('switch', { name: 'Own worktree' })[0])
    expect(chip(/feature\/a · worktree/)).toBeTruthy()
    fireEvent.change(prompt(), { target: { value: 'go' } })
    fireEvent.click(screen.getByLabelText('Start session and send'))
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0].branch).toMatchObject({ mode: 'existing', name: 'feature/a', worktree: true })
  })
  it('says when the folder is not a repository, and when the host has no git', async () => {
    git = { ok: false, error: 'not a git repository' }
    const v = setup()
    await waitFor(() => chip(/no git/))
    fireEvent.click(chip(/no git/))
    expect(screen.getByRole('dialog', { name: 'Branch' }).textContent).toMatch(/Not a git repository/)
    v.unmount()
    setup({ canBranch: false })
    fireEvent.click(chip(/no git/))
    expect(screen.getByRole('dialog', { name: 'Branch' }).textContent).toMatch(/does not run git/)
  })
})

describe('send', () => {
  it('starts with the choices, docks the composer and animates it', async () => {
    const { onStart } = setup()
    fireEvent.change(prompt(), { target: { value: 'Add tests' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0]).toMatchObject({ agent: 'goose', workspace: '/w/project', text: 'Add tests', access: { read: true, write: true, terminal: true, mcp: true } })
    expect(screen.getByText(/Starting Goose on Studio PC/)).toBeTruthy()
    expect(document.querySelector('.ns-composer.docked')).toBeTruthy()
  })
  it('Shift Enter does not send', () => {
    const { onStart } = setup()
    fireEvent.change(prompt(), { target: { value: 'x' } })
    fireEvent.keyDown(prompt(), { key: 'Enter', shiftKey: true })
    expect(onStart).not.toHaveBeenCalled()
  })
  it('a failed start returns to the composer, keeps the text and shows the error', async () => {
    const onStart = vi.fn(async () => 'Agent failed to start')
    setup({ onStart })
    fireEvent.change(prompt(), { target: { value: 'keep me' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'Agent failed to start')
    expect(prompt().value).toBe('keep me')
    expect(document.querySelector('.ns-composer.docked')).toBeNull()
  })
  it('animates the docking unless reduced motion is asked for', async () => {
    // jsdom has no layout: give the composer a different position before and after.
    let top = 300
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(() => ({ top: (top -= 100), bottom: top + 80, left: 0, right: 600, width: 600, height: 80, x: 0, y: top, toJSON() {} }) as DOMRect)
    let onStart = vi.fn(() => new Promise<string | undefined>(() => {}))
    let v = setup({ onStart })
    fireEvent.change(prompt(), { target: { value: 'a' } })
    await act(async () => { fireEvent.keyDown(prompt(), { key: 'Enter' }) })
    expect(animate).toHaveBeenCalledTimes(1)
    expect(animate.mock.calls[0][1]).toMatchObject({ duration: 220 })
    v.unmount(); animate.mockReset(); reduced = true; top = 300
    onStart = vi.fn(() => new Promise<string | undefined>(() => {}))
    v = setup({ onStart })
    fireEvent.change(prompt(), { target: { value: 'a' } })
    await act(async () => { fireEvent.keyDown(prompt(), { key: 'Enter' }) })
    expect(onStart).toHaveBeenCalled()
    expect(animate).not.toHaveBeenCalled()
  })
})

describe('models and modes before the first session', () => {
  const raw = { modes: { currentModeId: 'ask', availableModes: [{ id: 'ask', name: 'Ask' }, { id: 'code', name: 'Code' }] }, configOptions: [{ id: 'model', category: 'model', currentValue: 'small', options: [{ value: 'small', name: 'Small' }, { value: 'large', name: 'Large' }] }] }
  it('probes the host, shows a loading state, then offers the reported choices', async () => {
    let resolve!: (v: unknown) => void
    const probe = vi.fn(() => new Promise<unknown>((r) => { resolve = r }))
    const { onStart } = setup({ probe })
    expect(chip(/Reading models/).getAttribute('aria-busy')).toBe('true')
    expect(probe).toHaveBeenCalledWith('goose', '/w/project')
    await act(async () => resolve(raw))
    fireEvent.click(chip(/Small · Ask/))
    fireEvent.click(screen.getByText('Large'))
    fireEvent.click(screen.getByText('Code'))
    fireEvent.change(prompt(), { target: { value: 'go' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0]).toMatchObject({ model: 'large', mode: 'code' })
  })
  it('uses what the host already reports for the agent without probing', () => {
    const probe = vi.fn()
    setup({ probe, agents: [{ ...agents[0], setup: raw }] })
    expect(probe).not.toHaveBeenCalled()
    expect(chip(/Small · Ask/)).toBeTruthy()
  })
  it('falls back to defaults when the probe fails', async () => {
    setup({ probe: vi.fn(async () => undefined) })
    await waitFor(() => chip(/Default model · default mode/))
    fireEvent.click(chip(/Default model/))
    expect(screen.getByRole('dialog', { name: 'Model and mode' }).textContent).toMatch(/Could not read Goose/)
  })
  it('setupFromAcp keeps a chosen value over the reported one', () => {
    expect(setupFromAcp(raw, { model: 'large' })).toMatchObject({ mode: 'ask', model: 'large', modelConfig: 'model', models: [{ value: 'small' }, { value: 'large' }] })
    expect(setupFromAcp(null)).toMatchObject({ modes: undefined, models: undefined })
  })
})

describe('dropdown placement', () => {
  const view = { width: 1000, height: 640 }
  it('stays below when there is room and flips up near the bottom', () => {
    expect(placePopover({ top: 100, bottom: 130, left: 10 }, { width: 280, height: 300 }, view)).toMatchObject({ up: false, dx: 0 })
    expect(placePopover({ top: 500, bottom: 530, left: 10 }, { width: 280, height: 300 }, view)).toMatchObject({ up: true })
  })
  it('keeps an upward preference unless there is no room above', () => {
    expect(placePopover({ top: 400, bottom: 430, left: 10 }, { width: 280, height: 200 }, view, 'up').up).toBe(true)
    expect(placePopover({ top: 60, bottom: 90, left: 10 }, { width: 280, height: 200 }, view, 'up').up).toBe(false)
  })
  it('clamps horizontally inside the window', () => {
    expect(placePopover({ top: 100, bottom: 130, left: 900 }, { width: 280, height: 100 }, view).dx).toBe(1000 - 8 - 1180)
    expect(placePopover({ top: 100, bottom: 130, left: 4 }, { width: 280, height: 100 }, view).dx).toBe(4)
  })
  it('the chip applies the flip and shift to its dropdown', () => {
    Object.defineProperty(window, 'innerHeight', { value: 640, configurable: true })
    Object.defineProperty(window, 'innerWidth', { value: 1000, configurable: true })
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ top: 560, bottom: 590, left: 900, right: 990, width: 90, height: 30, x: 900, y: 560, toJSON() {} } as DOMRect)
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(280)
    vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(300)
    setup()
    fireEvent.click(chip(/Read · Write/))
    const pop = screen.getByRole('dialog', { name: 'Access rights' })
    expect(pop.dataset.placement).toBe('up')
    expect(pop.className).toContain(' up')
    expect(pop.style.transform).toBe('translateX(-188px)')
  })
})
