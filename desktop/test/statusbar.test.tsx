import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { StatusBar } from '../src/StatusBar'

afterEach(() => cleanup())

const bar = (extra: Partial<Parameters<typeof StatusBar>[0]> = {}) => {
  const terminal = { available: true, open: false, onToggle: vi.fn() }
  const browser = { open: false, onToggle: vi.fn() }
  render(<StatusBar terminal={terminal} browser={browser} {...extra} />)
  return { terminal, browser }
}

describe('the panel bar', () => {
  it('shows only the two panel toggles, no connection indicators', () => {
    bar()
    expect(screen.getAllByRole('button')).toHaveLength(2)
    expect(screen.queryByRole('status')).toBeNull()
    expect(screen.queryByText(/Connected|Offline| ms/)).toBeNull()
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
})
