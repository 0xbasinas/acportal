// Component tests for the new-session composer (jsdom, no layout: geometry is stubbed).
import { describe, expect, it, vi, beforeEach } from 'vitest'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { NewSessionHome, type StartRequest } from '../src/NewSession'
import { filterChoices, liveSelectors, liveValue, arranged, placePopover, sections, selectorsFromAcp, setRequest, setupFromAcp } from '../src/agentSetup'
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

describe('agent profiles', () => {
  it('groups a profile under its base and leaves a missing base at the top', () => {
    localStorage.setItem('acportal.lastAgent', 'goose')
    setup({ agents: [
      { id: 'goose', name: 'Goose', status: 'available' },
      { id: 'goose-work', name: 'Goose work', status: 'available', profileOf: 'goose', profileLabel: 'Work' },
      { id: 'mock', name: 'Mock', status: 'available' },
      { id: 'orphan', name: 'Orphan', status: 'available', profileOf: 'gone', profileLabel: 'Lost' },
    ] })
    fireEvent.click(chip(/Goose/))
    const dialog = screen.getByRole('dialog', { name: 'Agent' })
    const rows = [...dialog.querySelectorAll<HTMLElement>('[data-item]')]
    expect(rows.map((row) => row.textContent?.replace('profile', '').trim())).toEqual(['Goose', 'Work', 'Mock', 'Lost', 'Run on several agents…2-3 agents'])
    expect(rows[1].getAttribute('data-profile-of')).toBe('goose')
    expect(rows[3].getAttribute('data-profile-of')).toBeNull()
    fireEvent.click(rows[1])
    expect(chip(/Work/)).toBeTruthy()
    localStorage.removeItem('acportal.lastAgent')
  })
})

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

describe('models and options before the first session', () => {
  // Agents with configOptions report the mode there too; the older `modes` field is then ignored.
  const raw = {
    modes: { currentModeId: 'legacy', availableModes: [{ id: 'legacy', name: 'Legacy mode' }] },
    configOptions: [
      { id: 'model', name: 'Model', category: 'model', type: 'select', currentValue: 'small', options: [{ value: 'small', name: 'Small' }, { value: 'large', name: 'Large' }] },
      { id: 'mode', name: 'Mode', category: 'mode', type: 'select', currentValue: 'ask', options: [{ value: 'ask', name: 'Ask' }, { value: 'code', name: 'Code' }] }
    ]
  }
  it('probes the host, shows a loading state, then offers the reported choices', async () => {
    let resolve!: (v: unknown) => void
    const probe = vi.fn(() => new Promise<unknown>((r) => { resolve = r }))
    const { onStart } = setup({ probe })
    expect(chip(/Reading models/).getAttribute('aria-busy')).toBe('true')
    expect(probe).toHaveBeenCalledWith('goose', '/w/project')
    await act(async () => resolve(raw))
    fireEvent.click(chip(/Small · Ask/))
    expect(screen.queryByText('Legacy mode')).toBeNull()
    // Like the phone: one row per option with its value; a row opens only that list.
    expect(screen.queryByText('Large')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Model: Small' }))
    fireEvent.click(screen.getByText('Large'))
    fireEvent.click(screen.getByRole('button', { name: 'Mode: Ask' }))
    fireEvent.click(screen.getByText('Code'))
    expect(screen.getByRole('button', { name: 'Model: Large' })).toBeTruthy()
    fireEvent.change(prompt(), { target: { value: 'go' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0].choices).toEqual([{ id: 'model', value: 'large' }, { id: 'mode', value: 'code' }])
  })
  it('uses what the host already reports for the agent without probing', () => {
    const probe = vi.fn()
    setup({ probe, agents: [{ ...agents[0], setup: raw }] })
    expect(probe).not.toHaveBeenCalled()
    expect(chip(/Small · Ask/)).toBeTruthy()
  })
  it('says the models are unknown when the probe fails, invents none, and retries on request', async () => {
    const probe = vi.fn().mockResolvedValueOnce(undefined).mockResolvedValueOnce(raw)
    setup({ probe })
    await waitFor(() => chip(/Models unknown/))
    fireEvent.click(chip(/Models unknown/))
    const dialog = screen.getByRole('dialog', { name: 'Model and options' })
    expect(dialog.textContent).toMatch(/Could not read Goose/)
    expect(dialog.textContent).not.toMatch(/Default model/)
    fireEvent.click(screen.getByText('Retry'))
    await waitFor(() => chip(/Small · Ask/))
    expect(probe).toHaveBeenCalledTimes(2)
  })
  it('Escape in an option list goes back to the rows; Back does too', () => {
    const many = { configOptions: [
      { id: 'provider', name: 'Provider', type: 'select', currentValue: 'p0', options: Array.from({ length: 85 }, (_, i) => ({ value: 'p' + i, name: 'Provider ' + i })) },
      { id: 'model', name: 'Model', category: 'model', type: 'select', currentValue: 'a', options: [{ value: 'a', name: 'Model A' }] }
    ] }
    setup({ probe: vi.fn(), agents: [{ ...agents[0], setup: many }] })
    fireEvent.click(chip(/Model A · Provider 0/))
    expect(screen.queryByText('Provider 5')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Provider: Provider 0' }))
    const search = screen.getByRole('textbox', { name: 'Search Provider' })
    fireEvent.keyDown(search, { key: 'Escape' })
    expect(screen.getByRole('button', { name: 'Model: Model A' })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Provider: Provider 0' }))
    fireEvent.click(screen.getByRole('button', { name: 'Back to options (Provider)' }))
    expect(screen.queryByRole('textbox', { name: 'Search Provider' })).toBeNull()
  })
  it('a long grouped model list gets a search box; Enter picks the first match', async () => {
    const options = ['openai', 'anthropic', 'deepseek'].map((provider) => ({ group: provider, name: provider, options: Array.from({ length: 20 }, (_, i) => ({ value: provider + '/m' + i, name: provider + ' model ' + i })) }))
    const big = { configOptions: [{ id: 'model', name: 'Model', category: 'model', type: 'select', currentValue: 'openai/m0', options }] }
    const { onStart } = setup({ probe: vi.fn(), agents: [{ ...agents[0], setup: big }] })
    fireEvent.click(chip(/openai model 0/))
    expect(screen.getByText('anthropic')).toBeTruthy()
    const search = screen.getByRole('textbox', { name: 'Search Model' })
    fireEvent.change(search, { target: { value: 'deepseek model 7' } })
    expect(screen.getByText('1 of 60')).toBeTruthy()
    expect(screen.queryByText('openai model 1')).toBeNull()
    fireEvent.keyDown(search, { key: 'Enter' })
    fireEvent.change(prompt(), { target: { value: 'go' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0].choices).toEqual([{ id: 'model', value: 'deepseek/m7' }])
  })
})

describe('options that change other options', () => {
  // Any agent: a provider can bring other models. Before the session nothing is set on the agent;
  // picks are kept in order and applied at start, and the session's replies bring the new lists.
  const options = { configOptions: [
    { id: 'provider', name: 'Provider', type: 'select', currentValue: 'alpha', options: [{ value: 'alpha', name: 'Alpha' }, { value: 'beta', name: 'Beta' }] },
    { id: 'model', name: 'Model', category: 'model', type: 'select', currentValue: 'small', options: [{ value: 'small', name: 'Small' }, { value: 'large', name: 'Large' }] }
  ] }
  it('keeps picks in order without asking the agent, explains when lists may change, and starts with them', async () => {
    const probe = vi.fn()
    const { onStart } = setup({ probe, agents: [{ ...agents[0], setup: options }] })
    fireEvent.click(chip(/Small · Alpha/))
    fireEvent.click(screen.getByRole('button', { name: 'Model: Small' }))
    fireEvent.click(screen.getByText('Large'))
    expect(screen.queryByText(/applied when the session starts/)).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Provider: Alpha' }))
    fireEvent.click(screen.getByText('Beta'))
    expect(screen.getByText(/applied when the session starts/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Provider: Beta' })).toBeTruthy()
    expect(probe).not.toHaveBeenCalled()
    fireEvent.change(prompt(), { target: { value: 'go' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0].choices).toEqual([{ id: 'model', value: 'large' }, { id: 'provider', value: 'beta' }])
  })
})

describe('permission presets', () => {
  const presets = [
    { id: 'supervised', label: 'Supervised', effects: ['Each write asks you before it runs.', 'Each command asks you before it runs.'], workspaceAccess: { readFiles: true, writeFiles: true, terminal: true } },
    { id: 'auto-review', label: 'Auto-review', effects: ['Some command lines may be allowed or refused by the host\'s shell rules. Undecided lines still ask.'], workspaceAccess: { readFiles: true, writeFiles: true, terminal: true } },
  ]
  const modeAgent: AgentInfo = {
    ...agents[0],
    setup: { modes: { currentModeId: 'ask', availableModes: [{ id: 'ask', name: 'Ask' }, { id: 'full-access', name: 'Full access' }] } },
  }
  const loadPresets = vi.fn(async () => presets)

  it('shows presets first, keeps the agent mode separate, and a missing remembered id grants nothing', async () => {
    localStorage.setItem('acportal.preset.goose', 'full-access')
    const { onStart } = setup({ loadPresets, agents: [modeAgent] })
    await waitFor(() => expect(chip(/Supervised/)).toBeTruthy())
    fireEvent.click(chip(/Supervised/))
    const dialog = screen.getByRole('dialog', { name: 'Access rights' })
    const presetButtons = [...dialog.querySelectorAll<HTMLElement>('[data-preset]')]
    expect(presetButtons.map((button) => button.getAttribute('data-preset'))).toEqual(['supervised', 'auto-review'])
    expect(dialog.querySelector('[data-preset="full-access"]')).toBeNull()
    expect(dialog.textContent).toContain('Each write asks you before it runs.')
    fireEvent.click(dialog.querySelector<HTMLElement>('[data-preset="auto-review"]')!)
    expect(dialog.textContent).toContain('Undecided lines still ask')
    fireEvent.click(dialog.querySelector<HTMLElement>('[data-mode="full-access"]')!)
    expect(chip(/Auto-review/)).toBeTruthy()
    fireEvent.change(prompt(), { target: { value: 'go' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0].access).toMatchObject({ read: true, write: true, terminal: true, mcp: true })
    expect(onStart.mock.calls[0][0].choices).toEqual([{ id: 'mode', value: 'full-access' }])
    localStorage.removeItem('acportal.preset.goose')
  })

  it('remembers a preset per agent and custom toggles leave that preset', async () => {
    localStorage.setItem('acportal.preset.goose', 'auto-review')
    setup({ loadPresets, agents: [modeAgent] })
    await waitFor(() => expect(chip(/Auto-review/)).toBeTruthy())
    fireEvent.click(chip(/Auto-review/))
    fireEvent.click(screen.getByText('Run commands'))
    expect(chip(/Custom/)).toBeTruthy()
    expect(localStorage.getItem('acportal.preset.goose')).toBe('custom')
    localStorage.removeItem('acportal.preset.goose')
  })

  it('Ask for everything starts the session with manual approval and plain Supervised does not', async () => {
    const withAskAll = [
      presets[0],
      { id: 'ask-all', label: 'Ask for everything', effects: ['The host\'s auto-approve and shell rules do not act in this session. Every request waits for you.'], workspaceAccess: { readFiles: true, writeFiles: true, terminal: true, manualApproval: true } },
    ]
    localStorage.removeItem('acportal.preset.goose')
    const { onStart } = setup({ loadPresets: vi.fn(async () => withAskAll), agents: [modeAgent] })
    await waitFor(() => expect(chip(/Supervised/)).toBeTruthy())
    fireEvent.change(prompt(), { target: { value: 'one' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalledTimes(1))
    expect(onStart.mock.calls[0][0].access.manual).toBeFalsy()
  })

  it('picking Ask for everything sets the manual flag, and a custom toggle keeps it visible', async () => {
    const withAskAll = [
      presets[0],
      { id: 'ask-all', label: 'Ask for everything', effects: ['Every request waits for you.'], workspaceAccess: { readFiles: true, writeFiles: true, terminal: true, manualApproval: true } },
    ]
    localStorage.removeItem('acportal.preset.goose')
    const { onStart } = setup({ loadPresets: vi.fn(async () => withAskAll), agents: [modeAgent] })
    await waitFor(() => expect(chip(/Supervised/)).toBeTruthy())
    fireEvent.click(chip(/Supervised/))
    fireEvent.click(screen.getByRole('dialog', { name: 'Access rights' }).querySelector<HTMLElement>('[data-preset="ask-all"]')!)
    expect(chip(/Ask for everything/)).toBeTruthy()
    fireEvent.click(screen.getByText('Run commands'))
    expect(chip(/Custom/)).toBeTruthy()
    expect(screen.getByText(/auto-approve and shell rules are off for this session/)).toBeTruthy()
    fireEvent.click(screen.getByText('Run commands'))
    expect(chip(/Ask for everything/)).toBeTruthy()
    fireEvent.change(prompt(), { target: { value: 'two' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0].access).toMatchObject({ read: true, write: true, terminal: true, manual: true })
    localStorage.removeItem('acportal.preset.goose')
  })

  it('applies the next agent preset when the agent changes', async () => {
    localStorage.setItem('acportal.lastAgent', 'goose')
    localStorage.removeItem('acportal.preset.goose')
    localStorage.removeItem('acportal.preset.mock')
    const { onStart } = setup({ loadPresets, agents: [modeAgent, agents[1]] })
    await waitFor(() => expect(chip(/Supervised/)).toBeTruthy())
    fireEvent.click(chip(/Supervised/))
    fireEvent.click(screen.getByText('Write files'))
    expect(chip(/Custom/)).toBeTruthy()
    fireEvent.click(chip(/Goose/))
    fireEvent.click(screen.getByText('Mock'))
    await waitFor(() => expect(chip(/Supervised/)).toBeTruthy())
    fireEvent.change(prompt(), { target: { value: 'go' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(onStart.mock.calls[0][0].agent).toBe('mock')
    expect(onStart.mock.calls[0][0].access).toMatchObject({ read: true, write: true, terminal: true })
    localStorage.removeItem('acportal.lastAgent')
    localStorage.removeItem('acportal.preset.goose')
    localStorage.removeItem('acportal.preset.mock')
  })
})

describe('model section', () => {
  // In the agent's order: an uncategorised provider, the mode, the model, its effort and context.
  const agentOptions = { configOptions: [
    { id: 'provider', name: 'Provider', type: 'select', currentValue: 'p1', options: [{ value: 'p1', name: 'P1' }] },
    { id: 'mode', name: 'Mode', category: 'mode', type: 'select', currentValue: 'ask', options: [{ value: 'ask', name: 'Ask' }] },
    { id: 'effort', name: 'Thinking effort', category: 'thought_level', type: 'select', currentValue: 'off', options: [{ value: 'off', name: 'Off' }] },
    { id: 'm', name: 'Model', category: 'model', type: 'select', currentValue: 'a', options: [{ value: 'a', name: 'Model A' }] },
    { id: 'ctx', name: 'Context size', category: 'model_config', type: 'select', currentValue: '200k', options: [{ value: '200k', name: '200K' }] }
  ] }
  it('puts the model with what tunes it first, then the mode, then the rest in agent order', () => {
    const list = sections(selectorsFromAcp(agentOptions))
    expect(list.map((x) => [x.title, x.selectors.map((s) => s.id)])).toEqual([['Model', ['m', 'effort', 'ctx']], ['Mode', ['mode']], ['More options', ['provider']]])
    expect(arranged(selectorsFromAcp(agentOptions)).map((s) => s.id)).toEqual(['m', 'effort', 'ctx', 'mode', 'provider'])
    // One section has no heading; legacy fields still sort into Model and Mode.
    expect(sections(selectorsFromAcp({ models: { currentModelId: 'x', availableModels: [{ modelId: 'x' }] }, modes: { currentModeId: 'a', availableModes: [{ id: 'a', name: 'A' }] } })).map((x) => x.title)).toEqual(['Model', 'Mode'])
  })
  it('shows the sections as headings in the options dropdown', () => {
    setup({ probe: vi.fn(), agents: [{ ...agents[0], setup: agentOptions }] })
    fireEvent.click(chip(/Model A · Off/))
    const dialog = screen.getByRole('dialog', { name: 'Model and options' })
    expect(dialog.textContent).toMatch(/Model.*Model A.*Thinking effort.*Context size.*Mode.*Ask.*More options.*Provider/)
    expect(screen.getByRole('group', { name: 'More options' }).textContent).toMatch(/Provider/)
  })
})

describe('no project', () => {
  it('starts in a new empty folder the host makes at start, on the folder as it is', async () => {
    const newScratch = vi.fn(async () => 'C:/state/scratch/1792240000-ab12cd')
    const { onStart } = setup({ newScratch })
    fireEvent.click(chip(/project/))
    fireEvent.click(screen.getByText('No project'))
    expect(chip(/No project/)).toBeTruthy()
    expect(newScratch).not.toHaveBeenCalled()
    fireEvent.change(prompt(), { target: { value: 'what is a monad?' } })
    fireEvent.keyDown(prompt(), { key: 'Enter' })
    await waitFor(() => expect(onStart).toHaveBeenCalled())
    expect(newScratch).toHaveBeenCalledTimes(1)
    expect(onStart.mock.calls[0][0]).toMatchObject({ workspace: 'C:/state/scratch/1792240000-ab12cd', branch: { mode: 'current', worktree: false } })
  })
  it('is not offered by hosts without scratch folders, and a failed folder keeps the prompt', async () => {
    setup()
    fireEvent.click(chip(/project/))
    expect(screen.queryByText('No project')).toBeNull()
  })
})

describe('reading session options like the phone', () => {
  it('every select configOption by its own name, with groups, and how each is set', () => {
    const list = selectorsFromAcp({ configOptions: [
      { id: 'effort', name: 'Reasoning effort', type: 'select', currentValue: 'high', options: [{ value: 'low', name: 'Low' }, { value: 'high', name: 'High' }] },
      { id: 'm', name: 'Model', category: 'model', type: 'select', currentValue: 'a', options: [{ group: 'g', name: 'Provider G', options: [{ value: 'a', name: 'A', description: 'fast' }] }] },
      { id: 'flag', name: 'Some toggle', type: 'boolean', currentValue: true }
    ] })
    expect(list.map((x) => x.id)).toEqual(['effort', 'm'])
    expect(list[1].choices).toEqual([{ value: 'a', name: 'A', description: 'fast', group: 'Provider G' }])
    expect(arranged(list)[0].id).toBe('m')
    expect(setRequest(list[0], 'low')).toEqual({ method: 'session/set_config_option', params: { configId: 'effort', value: 'low' } })
  })
  it('falls back to the older models and modes fields only without configOptions', () => {
    const list = selectorsFromAcp({ models: { currentModelId: 'x', availableModels: [{ modelId: 'x', name: 'X' }, { modelId: 'y' }] }, modes: { currentModeId: 'ask', availableModes: [{ id: 'ask', name: 'Ask' }] } })
    expect(list.map((x) => [x.kind, x.current])).toEqual([['model', 'x'], ['mode', 'ask']])
    expect(list[0].choices[1]).toEqual({ value: 'y', name: 'y', description: undefined })
    expect(setRequest(list[0], 'y')).toEqual({ method: 'session/set_model', params: { modelId: 'y' } })
    expect(setRequest(list[1], 'ask')).toEqual({ method: 'session/set_mode', params: { modeId: 'ask' } })
    expect(selectorsFromAcp(null)).toEqual([])
  })
  it('search matches name, value and group; live values win over the starting ones', () => {
    const choices = [{ value: 'p/a', name: 'Alpha', group: 'Prov' }, { value: 'q/b', name: 'Beta', group: 'Other' }]
    expect(filterChoices(choices, 'prov').map((c) => c.value)).toEqual(['p/a'])
    expect(filterChoices(choices, 'q/').map((c) => c.value)).toEqual(['q/b'])
    const [model] = selectorsFromAcp({ configOptions: [{ id: 'model', type: 'select', currentValue: 'p/a', options: choices }] })
    expect(liveValue(model, { config: { model: 'q/b' } })).toBe('q/b')
    expect(liveSelectors({ configOptions: [] }, { configOptions: [{ id: 'model', type: 'select', options: choices }] })[0].choices).toHaveLength(2)
    expect(setupFromAcp(null)).toEqual({ selectors: [], chosen: {} })
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
