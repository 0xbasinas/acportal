import { describe, expect, it, vi } from 'vitest'
import type { BrowserWindow } from 'electron'
import { usable } from '../electron/window'

// A stand-in that behaves like Electron's: sending to a destroyed webContents throws.
function fakeWindow() {
  const state = { window: false, contents: false }
  const send = vi.fn(() => { if (state.contents) throw new TypeError('Object has been destroyed') })
  const win = { isDestroyed: () => state.window, webContents: { isDestroyed: () => state.contents, send } } as unknown as BrowserWindow
  return { win, state, send }
}

describe('the window while the app is quitting', () => {
  it('is returned while it is alive', () => {
    const { win } = fakeWindow()
    expect(usable(win)).toBe(win)
  })
  it('is not returned once the window or its page is destroyed, so a late socket report sends nothing', () => {
    const { win, state, send } = fakeWindow()
    state.contents = true
    expect(usable(win)).toBeNull()
    expect(() => usable(win)?.webContents.send('watch:state', {})).not.toThrow()
    expect(send).not.toHaveBeenCalled()
    state.contents = false
    state.window = true
    expect(usable(win)).toBeNull()
    expect(usable(null)).toBeNull()
  })
})
