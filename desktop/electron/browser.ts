// The in-app browser: one Chromium view in its own session, laid over the window where the page
// puts its panel. The view has no preload and no access to the app's IPC, asks for no permission
// without being refused, and may only show the pages `browserRules` allows. Everything else opens in
// the person's own browser, or is refused with a note.
import { BrowserWindow, IpcMain, Session, WebContents, WebContentsView, session, shell } from 'electron'
import { browserPrefs, classify, isLoopbackHost, isWebUrl, type BrowserPrefs } from './browserRules'
import { usable } from './window'
import type { BrowserState, BrowserTab, PickedElement } from './browserTypes'
import { PICK_CANCEL, PICK_ELEMENT, PICK_WORLD, pickedText } from './browserPicker'

export type { BrowserState }

const BLANK: BrowserState = { open: false, url: '', title: '', loading: false, canBack: false, canForward: false, zoom: 100, local: true, remembers: false, tabs: [] }
const MAX_TABS = 8
const ZOOM_STEPS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3]
/** The largest picture sent to an agent, in bytes (the host's frame limit is 600 KB for everything attached). */
const MAX_PICTURE = 420 * 1024

/**
 * Runs inside the page: selects the next or previous match of `q` (case-insensitive, wrapping) and
 * says how many there are and which one is selected. Source text, so keep it self-contained.
 */
export const FIND_IN_PAGE = `function (q, forward, again) {
  var text = document.body ? document.body.innerText : '';
  var lower = text.toLowerCase(), needle = q.toLowerCase(), count = 0, at = 0;
  while (needle && (at = lower.indexOf(needle, at)) !== -1) { count++; at += needle.length; }
  var state = window.__acpFind;
  if (!state || state.q !== q || !again) { state = window.__acpFind = { q: q, i: 0 }; var s = window.getSelection(); if (s) s.removeAllRanges(); }
  if (!count) { state.i = 0; return { matches: 0, active: 0 }; }
  var found = window.find(q, false, !forward, true);
  if (!found) return { matches: count, active: 0 };
  state.i = forward ? (state.i % count) + 1 : ((state.i - 2 + count) % count) + 1;
  return { matches: count, active: state.i };
}`

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
  // The active tab's view; `tabs` holds every open one, in order.
  let view: WebContentsView | null = null
  let tabs: { id: number; view: WebContentsView }[] = []
  let nextTab = 1
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
      tabs: tabs.map<BrowserTab>((t) => ({ id: t.id, title: t.view.webContents.isDestroyed() ? '' : t.view.webContents.getTitle(), url: t.view.webContents.isDestroyed() ? '' : t.view.webContents.getURL(), active: t.view === view, loading: !t.view.webContents.isDestroyed() && t.view.webContents.isLoading() })),
      ...patch
    }
    usable(win())?.webContents.send('browser:state', state)
  }

  const layout = (): void => {
    if (!view) return
    const show = want && bounds.width > 0 && bounds.height > 0
    for (const t of tabs) {
      t.view.setVisible(show && t.view === view)
      if (show) t.view.setBounds({ x: Math.round(bounds.x), y: Math.round(bounds.y), width: Math.round(bounds.width), height: Math.round(bounds.height) })
    }
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

  const dropView = (v: WebContentsView): void => {
    try { win()?.contentView.removeChildView(v) } catch { /* the window is already gone */ }
    if (!v.webContents.isDestroyed()) v.webContents.close()
  }
  const destroy = (): void => {
    const all = tabs
    tabs = []
    view = null
    state = { ...BLANK }
    for (const t of all) dropView(t.view)
    usable(win())?.webContents.send('browser:state', state)
  }

  const ensure = (): WebContentsView => {
    const p = prefs()
    if (view && remembers !== p.remember) destroy()
    return view ?? addTab()
  }
  /** A new tab, which becomes the active one. */
  const addTab = (): WebContentsView => {
    const p = prefs()
    if (tabs.length >= MAX_TABS) throw new Error('Close a tab before opening another')
    const w = win()
    if (!w) throw new Error('The window is not ready')
    remembers = p.remember
    const ses = session.fromPartition(p.remember ? 'persist:acportal-browser' : 'acportal-browser')
    harden(ses)
    const v = new WebContentsView({ webPreferences: { session: ses, sandbox: true, contextIsolation: true, nodeIntegration: false, webSecurity: true, allowRunningInsecureContent: false, spellcheck: false, webviewTag: false, navigateOnDragDrop: false } })
    view = v
    tabs.push({ id: nextTab++, view: v })
    const wc = v.webContents
    wc.setWindowOpenHandler(({ url }) => { route(url, wc, false); return { action: 'deny' } })
    wc.on('will-navigate', (e, url) => { if (!route(url, wc, true)) e.preventDefault() })
    wc.on('will-redirect', (e, url) => { if (!route(url, wc, true)) e.preventDefault() })
    wc.on('did-start-loading', () => { started = Date.now(); publish({ error: undefined, loadedMs: undefined }) })
    wc.on('did-stop-loading', () => publish({ loadedMs: started ? Date.now() - started : undefined }))
    wc.on('did-navigate', () => publish({ note: undefined, find: undefined }))
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
      else if (mod && key === 'f') { handled(); usable(win())?.webContents.send('browser:shortcut', 'find') }
      else if (mod && key === 't') { handled(); usable(win())?.webContents.send('browser:shortcut', 'newtab') }
      else if (mod && key === 'w') { handled(); closeTab(tabs.find((t) => t.view === v)?.id ?? -1) }
      else if (mod && key === 'l') { handled(); usable(win())?.webContents.send('browser:shortcut', 'address') }
      else if (mod && input.shift && key === 'b') { handled(); usable(win())?.webContents.send('browser:shortcut', 'toggle') }
    })
    w.contentView.addChildView(v)
    layout()
    publish()
    return v
  }

  const closeTab = (id: number): void => {
    const at = tabs.findIndex((t) => t.id === id)
    if (at < 0) return
    const [gone] = tabs.splice(at, 1)
    if (!tabs.length) { dropView(gone.view); destroy(); return }
    if (gone.view === view) view = tabs[Math.min(at, tabs.length - 1)].view
    dropView(gone.view)
    layout()
    publish({ find: undefined })
  }
  const selectTab = (id: number): void => {
    const t = tabs.find((x) => x.id === id)
    if (!t || t.view === view) return
    void view?.webContents.executeJavaScript('window.getSelection && window.getSelection().removeAllRanges()', true).catch(() => undefined)
    view = t.view
    layout()
    publish({ find: undefined })
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
  const picture = async (wc: WebContents, rect?: Electron.Rectangle): Promise<{ data: string; mimeType: string; size: number; width: number; height: number }> => {
    let image = await (rect ? wc.capturePage(rect) : wc.capturePage())
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
  ipc.handle('browser:new-tab', wrap((url?: string) => {
    if (!view) { ensure(); if (url) route(url, view!.webContents, false); publish(); return state }
    addTab()
    if (url) route(url, view!.webContents, false)
    publish()
    return state
  }))
  ipc.handle('browser:select-tab', wrap((id: number) => { selectTab(id); return state }))
  ipc.handle('browser:close-tab', wrap((id: number) => { closeTab(id); return state }))
  ipc.handle('browser:find', wrap((text: string, forward = true, again = false) => {
    const wc = view?.webContents
    if (!wc) return
    const q = String(text).slice(0, 200)
    if (!q) { void wc.executeJavaScript('window.getSelection && window.getSelection().removeAllRanges()', true).catch(() => undefined); publish({ find: undefined }); return }
    // Electron's own find reports nothing for a view that does not hold the keyboard focus, which is
    // the find box in the app's window here, so the page is searched by script instead.
    void wc.executeJavaScript(`(${FIND_IN_PAGE})(${JSON.stringify(q)}, ${forward}, ${again})`, true)
      .then((r: { matches: number; active: number }) => { if (view?.webContents === wc) publish({ find: r }) })
      .catch(() => undefined)
  }))
  ipc.handle('browser:find-stop', wrap(() => { void view?.webContents.executeJavaScript('window.getSelection && window.getSelection().removeAllRanges()', true).catch(() => undefined); publish({ find: undefined }) }))
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
  // The person clicks an element on the page; a description and a picture of it come back.
  let picking = false
  ipc.handle('browser:pick', wrap(async () => {
    const wc = view?.webContents
    if (!wc || !view) throw new Error('There is no page to pick from')
    if (picking) throw new Error('Already choosing an element')
    picking = true
    publish({ picking: true })
    try {
      const raw = (await wc.executeJavaScriptInIsolatedWorld(PICK_WORLD, [{ code: `(${PICK_ELEMENT})()` }])) as PickedElement | null
      if (!raw || typeof raw !== 'object' || typeof raw.selector !== 'string') return null
      // Everything from the page is data: only fields of the expected shape are kept, and bounded.
      const num = (n: unknown): number => (Number.isFinite(n) ? Math.round(Number(n)) : 0)
      const str = (s: unknown, max: number): string => (typeof s === 'string' ? s.slice(0, max) : '')
      const rec = (o: unknown, keep: number, max: number): Record<string, string> =>
        o && typeof o === 'object' ? Object.fromEntries(Object.entries(o as Record<string, unknown>).slice(0, keep).map(([k, v]) => [str(k, 40), str(v, max)])) : {}
      const element: PickedElement = {
        tag: str(raw.tag, 40), selector: str(raw.selector, 400), attributes: rec(raw.attributes, 16, 200), text: str(raw.text, 420),
        html: str(raw.html, 1600), htmlTruncated: raw.htmlTruncated === true,
        rect: { x: num(raw.rect?.x), y: num(raw.rect?.y), width: num(raw.rect?.width), height: num(raw.rect?.height) },
        viewport: { width: num(raw.viewport?.width), height: num(raw.viewport?.height) },
        parents: Array.isArray(raw.parents) ? raw.parents.slice(0, 3).map((p) => str(p, 80)) : [], styles: rec(raw.styles, 16, 80),
        url: str(raw.url, 2000), title: str(raw.title, 120)
      }
      let shot: Awaited<ReturnType<typeof picture>> | undefined
      try {
        const zoom = wc.getZoomFactor()
        const size = view.getBounds()
        const x = Math.max(0, Math.round(element.rect.x * zoom)), y = Math.max(0, Math.round(element.rect.y * zoom))
        const width = Math.min(size.width - x, Math.round(element.rect.width * zoom)), height = Math.min(size.height - y, Math.round(element.rect.height * zoom))
        if (width > 4 && height > 4) shot = await picture(wc, { x, y, width, height })
      } catch { /* the text alone is still useful */ }
      return { element, text: pickedText(element), shot }
    } catch {
      return null
    } finally {
      picking = false
      publish({ picking: false })
    }
  }))
  ipc.handle('browser:pick-cancel', wrap(async () => {
    const wc = view?.webContents
    if (wc && picking) await wc.executeJavaScriptInIsolatedWorld(PICK_WORLD, [{ code: PICK_CANCEL }]).catch(() => undefined)
  }))
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
