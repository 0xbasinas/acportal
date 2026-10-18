import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { StatusBar } from '../src/StatusBar'

afterEach(() => cleanup())

const bar = (extra: Partial<Parameters<typeof StatusBar>[0]> = {}) => {
  const terminal = { available: true, open: false, onToggle: vi.fn() }
  const browser = { open: false, onToggle: vi.fn() }
  render(<StatusBar entry={{ state: 'connected', latency: 14, agents: [], sessions: [], workspaces: [], failures: 0 }} branch="main" agent="Goose" terminal={terminal} browser={browser} {...extra} />)
  return { terminal, browser }
}

describe('the status bar', () => {
  it('says how the host is doing, and the branch and agent of the open session', () => {
    bar()
    expect(screen.getByRole('status').textContent).toContain('14 ms')
    expect(screen.getByText('main')).toBeTruthy()
    expect(screen.getByText('Goose')).toBeTruthy()
  })
  it('toggles the terminal and the browser, and shows which is open', () => {
    const { terminal, browser } = bar({ browser: { open: true, onToggle: vi.fn() } })
    const t = screen.getByRole('button', { name: /Terminal/ })
    expect(t.getAttribute('aria-pressed')).toBe('false')
    fireEvent.click(t)
    expect(terminal.onToggle).toHaveBeenCalled()
    expect(screen.getByRole('button', { name: /Browser/ }).getAttribute('aria-pressed')).toBe('true')
    expect(browser.onToggle).not.toHaveBeenCalled()
  })
  it('cannot open a terminal the host has off, but can still close one that is open', () => {
    bar({ terminal: { available: false, open: false, onToggle: vi.fn() } })
    expect((screen.getByRole('button', { name: /Terminal/ }) as HTMLButtonElement).disabled).toBe(true)
    cleanup()
    bar({ terminal: { available: false, open: true, onToggle: vi.fn() } })
    expect((screen.getByRole('button', { name: /Terminal/ }) as HTMLButtonElement).disabled).toBe(false)
  })
  it('names an offline host', () => {
    bar({ entry: { state: 'offline', agents: [], sessions: [], workspaces: [], failures: 3 } })
    expect(screen.getByRole('status').textContent).not.toContain('ms')
  })
})
