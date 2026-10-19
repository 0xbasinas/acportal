import type { BrowserWindow } from 'electron'

/** The window, only while it can still be used. Closing the window closes the session sockets, and
 * each of them reports its new state; sending that to a destroyed window throws "Object has been
 * destroyed" in the main process, which Electron shows as an error box when the app is quit. */
export function usable(win: BrowserWindow | null): BrowserWindow | null {
  return win && !win.isDestroyed() && !win.webContents.isDestroyed() ? win : null
}
