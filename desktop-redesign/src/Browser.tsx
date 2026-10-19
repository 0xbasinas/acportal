import { useCallback, useEffect, useRef, useState } from 'react'
import type { BrowserState } from '../electron/browserTypes'
import { I } from './ui'
import type { Attachment } from './Session'

const QUICK = ['localhost:5173', 'localhost:3000', 'localhost:8080']
const clampSize = (where: 'side' | 'dock', n: number): number => (where === 'side' ? Math.min(Math.max(n, 360), 1000) : Math.min(Math.max(n, 200), 640))

/** What the person chose to send to the agent from the page. */
export type BrowserShare = { attachments?: Attachment[]; append?: string }

/**
 * The in-app browser. The page itself is a native view drawn over the empty area below the toolbar
 * (the main process sizes it to this area); everything here is the frame around it. While a menu or
 * dialog needs the space the view is hidden and a still picture of the page is shown instead.
 */
/** The newest address already shown, so a panel that moves (side to dock) does not load it again. */
let handledRequest = 0

export function BrowserPanel(props: {
  where: 'side' | 'dock'
  /** An address to show: a link in a message, or the command palette. `n` makes each request new. */
  request?: { url: string; n: number }
  /** Something else (a dialog, the command palette) needs the screen. */
  covered?: boolean
  /** Width at the side, height when docked, in pixels. */
  size: number
  onSize: (size: number) => void
  onClose: () => void
  onWhere: (w: 'side' | 'dock') => void
  onShare: (s: BrowserShare) => void
  canShare: boolean
}) {
  const [s, setS] = useState<BrowserState>()
  const [input, setInput] = useState('')
  const [editing, setEditing] = useState(false)
  const [menu, setMenu] = useState(false)
  const [selection, setSelection] = useState('')
  const [shot, setShot] = useState<string | null>(null)
  const [busy, setBusy] = useState<string>()
  const [problem, setProblem] = useState<string>()
  const area = useRef<HTMLDivElement>(null)
  const [finding, setFinding] = useState(false)
  const [needle, setNeedle] = useState('')
  const findBox = useRef<HTMLInputElement>(null)
  const address = useRef<HTMLInputElement>(null)

  // The page itself lives in the main process and outlives this panel, so moving the panel from the
  // side to the dock keeps the page and its history. Closing it is the app's call (closeBrowser).
  useEffect(() => {
    const off = window.api.browser.onState((next) => setS(next))
    void window.api.browser.open().then((r) => { if (r.ok) setS(r.value); else setProblem(r.error) })
    const offKey = window.api.browser.onShortcut((name) => {
      if (name === 'address') { address.current?.focus(); address.current?.select() }
      else if (name === 'find') setFinding(true)
      else if (name === 'newtab') void window.api.browser.newTab().then((r) => { if (!r.ok) setProblem(r.error) })
      else props.onClose()
    })
    return () => { off(); offKey(); void window.api.browser.bounds(null) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  useEffect(() => {
    const r = props.request
    if (r && r.n > handledRequest) { handledRequest = r.n; setProblem(undefined); void window.api.browser.go(r.url) }
  }, [props.request?.n])
  useEffect(() => { if (!editing) setInput(s?.url && s.url !== 'about:blank' ? s.url : '') }, [s?.url, editing])

  const covered = Boolean(props.covered) || menu
  const hasPage = Boolean(s?.url) && s?.url !== 'about:blank' && !s?.error
  // Keep the native view exactly over the area; hide it while something covers it or there is no page.
  const place = useCallback(() => {
    const el = area.current
    if (!el || !hasPage || covered) { void window.api.browser.bounds(null); return }
    const r = el.getBoundingClientRect()
    void window.api.browser.bounds({ x: r.left, y: r.top, width: r.width, height: r.height })
  }, [hasPage, covered])
  useEffect(() => {
    place()
    const el = area.current
    const ro = el && typeof ResizeObserver !== 'undefined' ? new ResizeObserver(place) : undefined
    if (el) ro?.observe(el)
    window.addEventListener('resize', place)
    return () => { ro?.disconnect(); window.removeEventListener('resize', place) }
  }, [place, props.where])
  useEffect(() => {
    // Wait a frame after the layout changes (placement, panel resize) so the rectangle is final.
    const id = requestAnimationFrame(place)
    return () => cancelAnimationFrame(id)
  })

  useEffect(() => {
    let live = true
    if (covered && hasPage) void window.api.browser.freeze().then((r) => { if (live) setShot(r.ok ? r.value : null) })
    else { setShot(null); if (hasPage) void window.api.browser.thaw() }
    return () => { live = false }
  }, [covered, hasPage])

  const stopFind = (): void => { setFinding(false); setNeedle(''); void window.api.browser.findStop() }
  useEffect(() => { if (finding) { findBox.current?.focus(); findBox.current?.select() } }, [finding])
  // A page that was left (another tab, a new address) is no longer being searched.
  useEffect(() => { if (!s?.find && !needle) return; if (!s?.url) setFinding(false) }, [s?.url])
  const find = (forward: boolean, again: boolean): void => { void window.api.browser.find(needle, forward, again) }
  const go = (text: string): void => { setProblem(undefined); void window.api.browser.go(text).then((r) => { if (!r.ok) setProblem(r.error) }); address.current?.blur() }
  const openMenu = async (): Promise<void> => {
    if (menu) { setMenu(false); return }
    const r = await window.api.browser.selection()
    setSelection(r.ok ? r.value.trim() : '')
    setMenu(true)
  }
  const share = async (what: 'shot' | 'text' | 'url'): Promise<void> => {
    setBusy(what); setProblem(undefined)
    try {
      if (what === 'shot') {
        const r = await window.api.browser.capture()
        if (!r.ok) { setProblem(r.error); return }
        const host = (() => { try { return new URL(r.value.url).host } catch { return 'page' } })()
        props.onShare({ attachments: [{ name: `${host}.jpg`, mimeType: r.value.mimeType, size: r.value.size, data: r.value.data, path: r.value.url, uri: r.value.url }] })
      } else if (what === 'text') {
        const bytes = new TextEncoder().encode(selection).length
        props.onShare({ attachments: [{ name: 'selected text', mimeType: 'text/plain', size: bytes, text: selection, path: s?.url ?? '', uri: s?.url }] })
      } else if (s?.url) props.onShare({ append: s.url })
      setMenu(false)
    } finally { setBusy(undefined) }
  }

  /** The person chooses an element on the page; its description (and a picture of it) is attached to their next message. */
  const pickElement = async (): Promise<void> => {
    setMenu(false); setProblem(undefined)
    await new Promise((r) => setTimeout(r, 200)) // let the page be shown again before it is asked to take clicks
    const r = await window.api.browser.pick()
    if (!r.ok) { setProblem(r.error); return }
    if (!r.value) return
    const { text, shot, element } = r.value
    const attachments: Attachment[] = [{ name: `element ${element.selector.split(' > ').pop() ?? ''}`.slice(0, 60), mimeType: 'text/plain', size: new TextEncoder().encode(text).length, text, path: element.url, uri: element.url }]
    if (shot) attachments.push({ name: 'element.jpg', mimeType: shot.mimeType, size: shot.size, data: shot.data, path: element.url, uri: element.url })
    props.onShare({ attachments })
  }

  const seconds = s?.loadedMs !== undefined ? `${(s.loadedMs / 1000).toFixed(1)} s` : undefined
  const foot = s?.note ?? problem ?? (s?.loading ? 'Loading…' : seconds ? `Loaded in ${seconds}  ·  ${s?.remembers ? 'sign-ins are kept between launches' : 'isolated: no sign-ins kept'}` : s?.remembers ? 'Sign-ins are kept between launches' : 'Isolated: no sign-ins kept')
  const hot = Boolean(s?.note || problem)
  return (
    <section className={'browser ' + props.where} aria-label="Browser">
      <div className="browser-resize" role="separator" aria-orientation={props.where === 'side' ? 'vertical' : 'horizontal'} aria-label="Resize the browser" tabIndex={0}
        onKeyDown={(e) => { const step = e.shiftKey ? 60 : 20; if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') { e.preventDefault(); props.onSize(clampSize(props.where, props.size + step)) } else if (e.key === 'ArrowRight' || e.key === 'ArrowDown') { e.preventDefault(); props.onSize(clampSize(props.where, props.size - step)) } }}
        onPointerDown={(e) => {
          const start = props.where === 'side' ? e.clientX : e.clientY
          const from = props.size
          e.currentTarget.setPointerCapture(e.pointerId)
          const move = (m: PointerEvent): void => props.onSize(clampSize(props.where, from + (start - (props.where === 'side' ? m.clientX : m.clientY))))
          const up = (): void => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
          window.addEventListener('pointermove', move); window.addEventListener('pointerup', up)
        }} />
      {(s?.tabs?.length ?? 0) > 0 && (
        <div className="browser-tabs" role="tablist" aria-label="Browser tabs">
          {s!.tabs.map((t) => (
            <span key={t.id} className={'browser-tab' + (t.active ? ' on' : '')}>
              <button role="tab" aria-selected={t.active} title={t.url} onClick={() => void window.api.browser.selectTab(t.id)}>{t.loading ? 'Loading…' : (t.title || t.url.replace(/^https?:\/\//, '') || 'New tab').slice(0, 28)}</button>
              <button className="browser-tab-x" aria-label={`Close ${t.title || 'tab'}`} onClick={() => void window.api.browser.closeTab(t.id)}>{I.close}</button>
            </span>
          ))}
          <button className="icon-btn" aria-label="New tab" title="New tab (Ctrl T)" disabled={s!.tabs.length >= 8} onClick={() => void window.api.browser.newTab().then((r) => { if (!r.ok) setProblem(r.error) })}>{I.plus}</button>
        </div>
      )}
      <div className="browser-bar">
        <button className="icon-btn" title="Back (Alt ←)" aria-label="Back" disabled={!s?.canBack} onClick={() => void window.api.browser.back()}>{I.back}</button>
        <button className="icon-btn" title="Forward (Alt →)" aria-label="Forward" disabled={!s?.canForward} onClick={() => void window.api.browser.forward()}>{I.forward}</button>
        <button className="icon-btn" title={s?.loading ? 'Stop' : 'Reload (Ctrl R)'} aria-label={s?.loading ? 'Stop loading' : 'Reload'} disabled={!hasPage && !s?.error} onClick={() => void window.api.browser.reload()}>{s?.loading ? I.close : I.refresh}</button>
        <form className={'browser-address' + (editing ? ' editing' : '')} onSubmit={(e) => { e.preventDefault(); go(input) }}>
          <span className={'browser-scope' + (s && !s.local ? ' away' : '')} title={s && !s.local ? 'A site on your list of allowed sites' : 'On this computer'}>{s && !s.local ? I.globe : I.home}</span>
          <input ref={address} className="mono" value={input} spellCheck={false} aria-label="Address" placeholder="Address (Ctrl L)" onChange={(e) => setInput(e.target.value)} onFocus={(e) => { setEditing(true); e.currentTarget.select() }} onBlur={() => setEditing(false)} />
        </form>
        <button className="browser-zoom" title="Zoom (Ctrl + − 0). Click to reset" aria-label={`Zoom ${s?.zoom ?? 100} percent, reset`} disabled={!hasPage} onClick={() => void window.api.browser.zoom('reset')}>{s?.zoom ?? 100}%</button>
        <button className={'icon-btn' + (finding ? ' on' : '')} title="Find in page (Ctrl F)" aria-label="Find in page" aria-pressed={finding} disabled={!hasPage} onClick={() => (finding ? stopFind() : setFinding(true))}>{I.search}</button>
        <button className="icon-btn" title="Developer tools (F12)" aria-label="Developer tools" disabled={!hasPage} onClick={() => void window.api.browser.devtools()}>{I.code}</button>
        <button className="icon-btn" title="Open in my browser" aria-label="Open in my browser" disabled={!hasPage} onClick={() => void window.api.browser.external().then((r) => { if (!r.ok) setProblem(r.error) })}>{I.external}</button>
        <div className="browser-send">
          <button className="btn primary" disabled={!hasPage || !props.canShare} title={props.canShare ? 'Attach this page to your next message' : 'Open a session to send this to an agent'} aria-haspopup="menu" aria-expanded={menu} onClick={() => void openMenu()}>{I.spark}Send to agent{I.down}</button>
          {menu && (
            <div className="browser-menu" role="menu">
              <div className="browser-menu-h">Attach to your next message</div>
              <button role="menuitem" onClick={() => void share('shot')} disabled={busy === 'shot'}>{I.camera}<span>{busy === 'shot' ? 'Taking the picture…' : 'Screenshot of the page'}</span></button>
              <button role="menuitem" onClick={() => void share('text')} disabled={!selection}>{I.quote}<span>Selected text{selection ? <span className="sub"> · “{selection.length > 24 ? selection.slice(0, 24) + '…' : selection}”</span> : <span className="sub"> · nothing selected</span>}</span></button>
              <button role="menuitem" onClick={() => void pickElement()}>{I.code}<span>Pick an element on the page…</span></button>
              <button role="menuitem" onClick={() => void share('url')}>{I.link}<span>Page address</span></button>
              <div className="browser-menu-f">Nothing is sent until you send.</div>
            </div>
          )}
        </div>
        <button className="icon-btn" title={props.where === 'side' ? 'Dock at the bottom' : 'Show at the side'} aria-label={props.where === 'side' ? 'Dock at the bottom' : 'Show at the side'} onClick={() => props.onWhere(props.where === 'side' ? 'dock' : 'side')}>{props.where === 'side' ? I.dockBottom : I.dockSide}</button>
        <button className="icon-btn" title="Close the browser (Ctrl Shift B)" aria-label="Close the browser" onClick={props.onClose}>{I.close}</button>
      </div>
      {finding && (
        <form className="browser-find" role="search" onSubmit={(e) => e.preventDefault()}>
          <input ref={findBox} aria-label="Find in page" placeholder="Find in page" value={needle} spellCheck={false}
            onChange={(e) => { setNeedle(e.target.value); void window.api.browser.find(e.target.value, true, false) }}
            onKeyDown={(e) => { if (e.key === 'Escape') { e.preventDefault(); stopFind() } else if (e.key === 'Enter') { e.preventDefault(); find(!e.shiftKey, true) } }} />
          <span className="sub" role="status">{needle ? (s?.find ? (s.find.matches ? `${s.find.active} of ${s.find.matches}` : 'No matches') : '…') : ''}</span>
          <button type="button" className="icon-btn" aria-label="Previous match" disabled={!s?.find?.matches} onClick={() => find(false, true)}>{I.up}</button>
          <button type="button" className="icon-btn" aria-label="Next match" disabled={!s?.find?.matches} onClick={() => find(true, true)}>{I.down}</button>
          <button type="button" className="icon-btn" aria-label="Close find" onClick={stopFind}>{I.close}</button>
        </form>
      )}
      {s?.picking && (
        <div className="browser-picking" role="status">
          <span>Click an element on the page. It goes to your next message. <span className="kbd">Esc</span> cancels.</span>
          <button className="btn" onClick={() => void window.api.browser.pickCancel()}>Cancel</button>
        </div>
      )}
      {menu && <button className="browser-scrim" aria-label="Close the menu" onClick={() => setMenu(false)} />}
      <div className="browser-area" ref={area}>
        {shot && hasPage && <img className="browser-still" src={shot} alt="" />}
        {!hasPage && !s?.error && (
          <div className="browser-empty">
            <div className="empty-ic">{I.globe}</div>
            <h2>Preview a page</h2>
            <p>Type an address above. Pages on this computer open here; other sites open in your own browser.</p>
            <div className="browser-quick">{QUICK.map((q) => <button key={q} className="btn mono" onClick={() => go(q)}>{q}</button>)}</div>
          </div>
        )}
        {s?.error && (
          <div className="browser-empty">
            <div className="empty-ic">{I.alert}</div>
            <h2>This page did not load</h2>
            <p>{s.error}</p>
            <div className="mono sub">{s.url}</div>
            <button className="btn primary" onClick={() => void window.api.browser.reload()}>{I.refresh}Try again</button>
          </div>
        )}
      </div>
      <div className={'browser-foot' + (hot ? ' hot' : '')} role="status">{foot}</div>
    </section>
  )
}
