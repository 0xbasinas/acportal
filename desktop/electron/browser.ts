// The in-app browser: one Chromium view in its own session, laid over the window where the page
// puts its panel. The view has no preload and no access to the app's IPC, asks for no permission
// without being refused, and may only show the pages `browserRules` allows. Everything else opens in
// the person's own browser, or is refused with a note.
import { BrowserWindow, IpcMain, Session, WebContents, WebContentsView, session, shell } from 'electron'
import { browserPrefs, classify, isLoopbackHost, isWebUrl, type BrowserPrefs } from './browserRules'
import { usable } from './window'
import type { BrowserState } from './browserTypes'

export type { BrowserState }

const BLANK: BrowserState = { open: false, url: '', title: '', loading: false, canBack: false, canForward: false, zoom: 100, local: true, remembers: false }
const ZOOM_STEPS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3]
/** The largest picture sent to an agent, in bytes (the host's frame limit is 600 KB for everything attached). */
const MAX_PICTURE = 420 * 1024

/** The reason in plain words for a page that did not load. */
export function loadFailure(code: number, description: string): string {
  if (code === -102 || description.includes('CONNECTION_REFUSED')) return 'Nothing is listening at this address. Is the dev server running?'
  if (code === -105 || description.includes('NAME_NOT_RESOLVED')) return 'This address could not be found.'
  if (code === -106 || description.includes('INTERNET_DISCONNECTED')) return 'This computer is offline.'
  if (code === -109 || description.includes('ADDRESS_UNREACHABLE')) return 'This address cannot be reached.'
  if (code === -118 || description.includes('TIMED_OUT')) return 'The page took too long to answer.'
  if (code <= -200 && code > -300) return 'The page\'s security certificate was refused.'
  return description ? `The page could not be loaded (${description.replace(/^ERR_/, '').toLowerCase().replace(/_/g, ' ')}).` : 'The page could not be loaded.'
}

export function registerBrowser(deps: { ipc: IpcMain; wrap: <A extends unknown[], R>(fn: (...a: A) => R | Promise<R>) => (e: unknown, ...a: A) => Promise<unknown>; win: () => BrowserWindow | null; readPrefs: () => Record<string, unknown> }): { dispose: () => void } {
  const { ipc, wrap, win, readPrefs } = deps
  let view: WebContentsView | null = null
  let remembers = false
  let want = false // the page has a place to show it
  let bounds = { x: 0, y: 0, width: 0, height: 0 }
  let started = 0
  let state: BrowserState = { ...BLANK }
  const hardened = new WeakSet<Session>()
  const prefs = (): BrowserPrefs => browserPrefs(readPrefs().browser)

  const publish = (patch: Partial<BrowserState> = {}): void => {
    const wc = view && !view.webContents.isDestroyed() ? view.webContents : undefined
    let host = ''
    try { host = wc ? new URL(wc.getURL()).hostname : '' } catch { /* blank page */ }
    state = {
      ...state,
      open: Boolean(view),
      url: wc?.getURL() ?? '',
      title: wc?.getTitle() ?? '',
      loading: wc?.isLoading() ?? false,
      canBack: wc?.navigationHistory.canGoBack() ?? false,
      canForward: wc?.navigationHistory.canGoForward() ?? false,
      zoom: Math.round((wc?.getZoomFactor() ?? 1) * 100),
      local: !host || isLoopbackHost(host),
      remembers,
      ...patch
    }
    usable(win())?.webContents.send('browser:state', state)
  }

  const layout = (): void => {
    if (!view) return
    const show = want && bounds.width > 0 && bounds.height > 0
    view.setVisible(show)
    if (show) view.setBounds({ x: Math.round(bounds.x), y: Math.round(bounds.y), width: Math.round(bounds.width), height: Math.round(bounds.height) })
  }

  /** A permission is never granted; a download is refused unless the person allowed downloads. */
  const harden = (ses: Session): void => {
    if (hardened.has(ses)) return
    hardened.add(ses)
    ses.setPermissionRequestHandler((_wc, _permission, callback) => callback(false))
    ses.setPermissionCheckHandler(() => false)
    ses.setDevicePermissionHandler(() => false)
    ses.on('will-download', (event) => { if (!prefs().downloads) event.preventDefault() })
  }

  /** Follows a link, redirect or typed address: loads it here, hands it to the person's browser, or refuses it. Returns whether it may load here. */
  const route = (url: string, wc: WebContents, asNavigation: boolean): boolean => {
    const target = classify(url, prefs())
    if (target.kind === 'load') { if (!asNavigation) void wc.loadURL(target.url); return true }
    if (target.kind === 'external' && isWebUrl(target.url)) {
      void shell.openExternal(target.url)
      publish({ note: `Opened in your own browser: ${new URL(target.url).hostname}` })
    } else if (target.kind === 'reject') publish({ note: target.reason })
    return false
  }

  const destroy = (): void => {
    const v = view
    view = null
    state = { ...BLANK }
    if (v) {
      try { win()?.contentView.removeChildView(v) } catch { /* the window is already gone */ }
      if (!v.webContents.isDestroyed()) v.webContents.close()
    }
    usable(win())?.webContents.send('browser:state', state)
  }

  const ensure = (): WebContentsView => {
    const p = prefs()
    if (view && remembers !== p.remember) destroy()
    if (view) return view
    const w = win()
    if (!w) throw new Error('The window is not ready')
    remembers = p.remember
    const ses = session.fromPartition(p.remember ? 'persist:acportal-browser' : 'acportal-browser')
    harden(ses)
    const v = new WebContentsView({ webPreferences: { session: ses, sandbox: true, contextIsolation: true, nodeIntegration: false, webSecurity: true, allowRunningInsecureContent: false, spellcheck: false, webviewTag: false, navigateOnDragDrop: false } })
    view = v
    const wc = v.webContents
    wc.setWindowOpenHandler(({ url }) => { route(url, wc, false); return { action: 'deny' } })
    wc.on('will-navigate', (e, url) => { if (!route(url, wc, true)) e.preventDefault() })
    wc.on('will-redirect', (e, url) => { if (!route(url, wc, true)) e.preventDefault() })
    wc.on('did-start-loading', () => { started = Date.now(); publish({ error: undefined, loadedMs: undefined }) })
    wc.on('did-stop-loading', () => publish({ loadedMs: started ? Date.now() - started : undefined }))
    wc.on('did-navigate', () => publish({ note: undefined }))
    wc.on('did-navigate-in-page', () => publish())
    wc.on('page-title-updated', () => publish())
    wc.on('did-fail-load', (_e, code, description, _url, isMainFrame) => { if (isMainFrame && code !== -3) publish({ error: loadFailure(code, description) }) })
    wc.on('render-process-gone', () => publish({ error: 'The page stopped working. Reload it.' }))
    wc.on('before-input-event', (event, input) => {
      if (input.type !== 'keyDown') return
      const mod = input.control || input.meta
      const key = input.key.toLowerCase()
      const handled = (): void => event.preventDefault()
      if ((mod && key === 'r') || key === 'f5') { handled(); if (input.shift) wc.reloadIgnoringCache(); else wc.reload() }
      else if (input.alt && key === 'arrowleft') { handled(); if (wc.navigationHistory.canGoBack()) wc.navigationHistory.goBack() }
      else if (input.alt && key === 'arrowright') { handled(); if (wc.navigationHistory.canGoForward()) wc.navigationHistory.goForward() }
      else if (mod && (key === '=' || key === '+')) { handled(); zoomBy(1) }
      else if (mod && key === '-') { handled(); zoomBy(-1) }
      else if (mod && key === '0') { handled(); wc.setZoomFactor(1); publish() }
      else if (key === 'f12' || (mod && input.shift && key === 'i')) { handled(); toggleDevTools() }
      else if (mod && key === 'l') { handled(); usable(win())?.webContents.send('browser:shortcut', 'address') }
      else if (mod && input.shift && key === 'b') { handled(); usable(win())?.webContents.send('browser:shortcut', 'toggle') }
    })
    w.contentView.addChildView(v)
    layout()
    publish()
    return v
  }

  const zoomBy = (direction: 1 | -1): void => {
    const wc = view?.webContents
    if (!wc) return
    const now = wc.getZoomFactor()
    const next = direction > 0 ? ZOOM_STEPS.find((z) => z > now + 0.001) : [...ZOOM_STEPS].reverse().find((z) => z < now - 0.001)
    if (next) { wc.setZoomFactor(next); publish() }
  }
  const toggleDevTools = (): void => {
    const wc = view?.webContents
    if (!wc) return
    if (wc.isDevToolsOpened()) wc.closeDevTools()
    else wc.openDevTools({ mode: 'detach' })
  }

  /** Shrinks the page picture until it fits what an agent can be sent. */
  const picture = async (wc: WebContents): Promise<{ data: string; mimeType: string; size: number; width: number; height: number }> => {
    let image = await wc.capturePage()
    const natural = image.getSize()
    for (const [width, quality] of [[1280, 80], [1024, 70], [800, 60], [640, 50], [480, 40]] as const) {
      const scaled = natural.width > width ? image.resize({ width }) : image
      const bytes = scaled.toJPEG(quality)
      if (bytes.length <= MAX_PICTURE || width === 480) { const size = scaled.getSize(); return { data: bytes.toString('base64'), mimeType: 'image/jpeg', size: bytes.length, width: size.width, height: size.height } }
      image = scaled
    }
    throw new Error('The page picture is too large')
  }

  ipc.handle('browser:state', wrap(() => state))
  ipc.handle('browser:open', wrap((url?: string) => {
    ensure()
    if (url) { const t = classify(url, prefs()); if (t.kind === 'load') void view!.webContents.loadURL(t.url); else route(url, view!.webContents, false) }
    publish()
    return state
  }))
  ipc.handle('browser:go', wrap((input: string) => { const v = ensure(); route(input, v.webContents, false); publish(); return state }))
  ipc.handle('browser:close', wrap(() => { destroy() }))
  ipc.handle('browser:bounds', wrap((b: { x: number; y: number; width: number; height: number } | null) => {
    want = Boolean(b)
    if (b) bounds = { x: Number(b.x) || 0, y: Number(b.y) || 0, width: Math.max(0, Number(b.width) || 0), height: Math.max(0, Number(b.height) || 0) }
    layout()
  }))
  ipc.handle('browser:back', wrap(() => { const wc = view?.webContents; if (wc?.navigationHistory.canGoBack()) wc.navigationHistory.goBack() }))
  ipc.handle('browser:forward', wrap(() => { const wc = view?.webContents; if (wc?.navigationHistory.canGoForward()) wc.navigationHistory.goForward() }))
  ipc.handle('browser:reload', wrap((hard?: boolean) => { const wc = view?.webContents; if (!wc) return; if (wc.isLoading()) wc.stop(); else if (hard) wc.reloadIgnoringCache(); else wc.reload() }))
  ipc.handle('browser:zoom', wrap((direction: 'in' | 'out' | 'reset') => { const wc = view?.webContents; if (!wc) return; if (direction === 'reset') { wc.setZoomFactor(1); publish() } else zoomBy(direction === 'in' ? 1 : -1) }))
  ipc.handle('browser:devtools', wrap(() => { toggleDevTools() }))
  ipc.handle('browser:external', wrap(() => {
    const url = view?.webContents.getURL() ?? ''
    if (!isWebUrl(url)) throw new Error('There is no page to open')
    void shell.openExternal(url)
  }))
  // A still picture of the page, shown in its place while a menu or dialog covers it: the view is
  // drawn above the page's own controls, so it is hidden while something else needs the space.
  ipc.handle('browser:freeze', wrap(async () => {
    const wc = view?.webContents
    if (!wc || !view) return null
    let shot: string | null = null
    try { shot = (await wc.capturePage()).toDataURL() } catch { /* nothing to show */ }
    view.setVisible(false)
    return shot
  }))
  ipc.handle('browser:thaw', wrap(() => { layout() }))
  ipc.handle('browser:capture', wrap(async () => {
    const wc = view?.webContents
    if (!wc) throw new Error('There is no page to capture')
    return { ...(await picture(wc)), title: wc.getTitle(), url: wc.getURL() }
  }))
  ipc.handle('browser:selection', wrap(async () => {
    const wc = view?.webContents
    if (!wc) return ''
    const text = await wc.executeJavaScript('String((window.getSelection && window.getSelection().toString()) || "")', true)
    return typeof text === 'string' ? text.slice(0, 20_000) : ''
  }))

  return { dispose: destroy }
}
