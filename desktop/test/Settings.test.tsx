import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { joinArgs, splitArgs, Settings } from '../src/Settings'
import { groupAgents } from '../src/agentSetup'
import type { AgentInfo } from '../src/types'

describe('agent arguments in the edit form', () => {
  it('round-trips arguments, including ones with spaces', () => {
    const args = ['acp', '--config', 'C:/Program Files/x/cfg.json', '']
    expect(joinArgs(args)).toBe('acp --config "C:/Program Files/x/cfg.json" ""')
    expect(splitArgs(joinArgs(args))).toEqual(args)
    expect(splitArgs('  --acp   x ')).toEqual(['--acp', 'x'])
  })
})

describe('agent profiles', () => {
  const agents: AgentInfo[] = [
    { id: 'goose', name: 'Goose', status: 'available' },
    { id: 'goose-work', name: 'Goose work', status: 'available', profileOf: 'goose', profileLabel: 'Work' },
    { id: 'mock', name: 'Mock', status: 'available' },
  ]
  it('keeps a profile under its base agent', () => {
    expect(groupAgents(agents).map((group) => [group.base.id, group.profiles.map((profile) => profile.id)])).toEqual([['goose', ['goose-work']], ['mock', []]])
    expect(groupAgents([{ id: 'a' }, { id: 'b', profileOf: 'a' }, { id: 'c', profileOf: 'b' }]).map((group) => group.base.id)).toEqual(['a', 'c'])
  })
  it('shows the profile under its base agent in Settings', () => {
    ;(window as unknown as { api: { getVersion: () => Promise<string>; secureStorage: () => Promise<string> } }).api = {
      getVersion: async () => 'test',
      secureStorage: async () => 'keychain:test',
    }
    render(<Settings section="Agents" setSection={() => {}} onBack={() => {}} hosts={[]} online agents={agents} workspaces={[]} theme="dark" setTheme={() => {}} onPairNew={() => {}} onForget={() => {}} onSwitch={() => {}} onAddWorkspace={async () => undefined} onRemoveWorkspace={() => {}} api={async () => ({ ok: false, error: 'no' })} onAgentsChanged={() => {}} mcp={[]} setMcp={() => {}} />)
    const rows = [...document.querySelectorAll<HTMLElement>('.set-row')]
    const ids = rows.map((row) => row.textContent ?? '')
    const goose = ids.findIndex((text) => text.includes('goose') && !text.includes('goose-work'))
    const work = ids.findIndex((text) => text.includes('goose-work'))
    const mock = ids.findIndex((text) => text.includes('mock'))
    expect(goose).toBeGreaterThanOrEqual(0)
    expect(work).toBeGreaterThan(goose)
    expect(mock).toBeGreaterThan(work)
    expect(rows[work].getAttribute('data-profile-of')).toBe('goose')
    expect(rows[work].textContent).toContain('profile of goose')
    expect(rows[work].textContent).toContain('Work')
    expect(screen.getByRole('heading', { name: 'Agents' })).toBeTruthy()
  })
})
