import { useCallback, useEffect, useRef, useState } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import '@xterm/xterm/css/xterm.css'
import { I } from './ui'

export type TermInfo = { id: string; folder: string; shell: string; rows: number; cols: number; exitCode?: number | null; controller?: string | null }
type Api = <T>(method: string, path: string, body?: unknown) => Promise<{ ok: true; value: T } | { ok: false; error: string; code?: string }>

const SHELL_KEY = 'acportal.termShell'
const clampHeight = (n: number): number => Math.min(Math.max(n, 160), 720)

/** The first dev-server address in a piece of terminal output, as a page this computer can show. */
export function devServerUrl(text: string): string | undefined {
  // eslint-disable-next-line no-control-regex
  const plain = text.replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, '').replace(/\x1b\][^\x07]*\x07/g, '')
  const m = plain.match(/https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]):\d{2,5}(?:\/[^\s'")\]]*)?/i)
  if (!m) return undefined
  return m[0].replace('0.0.0.0', 'localhost').replace(/[.,;:]+$/, '')
}

const isLocal = (url: string): boolean => /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\])(:|\/|$)/i.test(url)

function palette(): Record<string, string> {
  const css = getComputedStyle(document.documentElement)
  const v = (name: string, fallback: string): string => css.getPropertyValue(name).trim() || fallback
  return { background: v('--bg', '#0a0a0a'), foreground: v('--text', '#ededed'), cursor: v('--text', '#ededed'), selectionBackground: 'rgba(120,160,255,.35)' }
}

/** One shell. Stays mounted (hidden) while another tab is shown so its history is kept. */
function TermView(props: { hostId: string; info: TermInfo; active: boolean; onOpenUrl: (url: string) => void; onEnded: (code: number) => void }) {
  const box = useRef<HTMLDivElement>(null)
  const fit = useRef<FitAddon | undefined>(undefined)
  const key = useRef<string | undefined>(undefined)
  const [url, setUrl] = useState<string>()
  const [lost, setLost] = useState(false)
  const [conn, setConn] = useState<string>('connecting')
  const [ended, setEnded] = useState<number | null>(props.info.exitCode ?? null)
  const { hostId, info, onEnded } = props

  useEffect(() => {
    const term = new XTerm({ fontFamily: 'ui-monospace, "Cascadia Mono", Menlo, Consolas, monospace', fontSize: 13, cursorBlink: true, scrollback: 5000, allowProposedApi: false, theme: palette() })
    const fitter = new FitAddon()
    fit.current = fitter
    term.loadAddon(fitter)
    term.loadAddon(new WebLinksAddon((_e, uri) => { if (isLocal(uri)) props.onOpenUrl(uri); else window.open(uri) }))
    term.open(box.current!)
    const safeFit = (): void => { try { if (box.current && box.current.clientWidth > 0 && box.current.clientHeight > 0) fitter.fit() } catch { /* hidden */ } }
    safeFit()
    let tail = ''
    const offs: (() => void)[] = []
    let dead = false
    void window.api.term.attach(hostId, info.id, 0).then((r) => {
      if (dead) { if (r.ok) void window.api.term.detach(r.value); return }
      if (!r.ok) { setConn('failed'); term.write(`\r\n\x1b[31m${r.error}\x1b[0m\r\n`); return }
      const k = r.value
      key.current = k
      offs.push(window.api.term.onData((from, bytes) => {
        if (from !== k) return
        term.write(bytes)
        tail = (tail + new TextDecoder().decode(bytes)).slice(-600)
        const found = devServerUrl(tail)
        if (found) setUrl((old) => old ?? found)
      }))
      offs.push(window.api.term.onEvent((from, ev) => {
        if (from !== k) return
        if (ev.type === 'hello') setLost(ev.you === false)
        else if (ev.type === 'controller') setLost(ev.you === false)
        else if (ev.type === 'error' && ev.code === 'not_controller') setLost(true)
        else if (ev.type === 'exit') { const code = Number(ev.code); setEnded(code); onEnded(code) }
      }))
      offs.push(window.api.term.onState((from, state) => { if (from === k) setConn(state) }))
      offs.push(term.onData((d) => { void window.api.term.input(k, d) }).dispose)
      offs.push(term.onResize(({ rows, cols }) => { void window.api.term.resize(k, rows, cols) }).dispose)
      void window.api.term.resize(k, term.rows, term.cols)
    })
    const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(safeFit) : undefined
    if (box.current) ro?.observe(box.current)
    const theme = new MutationObserver(() => { term.options.theme = palette() })
    theme.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
    return () => {
      dead = true
      ro?.disconnect()
      theme.disconnect()
      offs.forEach((f) => f())
      if (key.current) void window.api.term.detach(key.current)
      term.dispose()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hostId, info.id])

  useEffect(() => { if (props.active) requestAnimationFrame(() => { try { fit.current?.fit() } catch { /* not shown yet */ } }) }, [props.active])

  return (
    <div className="term-view" hidden={!props.active}>
      {url && props.active && (
        <div className="term-chip" role="status">
          <span className="mono ell">{url.replace(/^https?:\/\//, '')}</span>
          <button className="btn" onClick={() => props.onOpenUrl(url)}>{I.globe}Open in the browser</button>
          <button className="icon-btn" aria-label="Dismiss" onClick={() => setUrl(undefined)}>{I.close}</button>
        </div>
      )}
      {lost && !ended && (
        <div className="term-chip warn" role="status">
          <span>Another device has the keyboard.</span>
          <button className="btn" onClick={() => { if (key.current) { void window.api.term.take(key.current); setLost(false) } }}>Take control</button>
        </div>
      )}
      {conn === 'reconnecting' && <div className="term-chip warn" role="status">Reconnecting… the shell keeps running on the host.</div>}
      {conn === 'failed' && !ended && <div className="term-chip warn" role="status">Disconnected from this terminal.</div>}
      {ended !== null && <div className="term-chip" role="status">The shell ended (exit code {ended}). Close this tab or start a new terminal.</div>}
      <div className="term-box" ref={box} />
    </div>
  )
}

/**
 * The terminal dock: shells running on the host, in the folder of the open session. They are real
 * shells with the host user's rights (the host decides whether they exist at all); an agent cannot
 * see them.
 */
export function TerminalDock(props: {
  hostId: string
  hostName: string
  /** The folder a new shell starts in (the open session's folder). */
  folder?: string
  /** The host lists the `terminal` feature, i.e. its config turns terminals on. */
  enabled: boolean
  height: number
  onHeight: (n: number) => void
  onClose: () => void
  onOpenUrl: (url: string) => void
  /** Opens Settings → Terminal, where terminals are turned on. */
  onOpenSettings?: () => void
  api: Api
}) {
  const { api, hostId, folder } = props
  const [list, setList] = useState<TermInfo[]>([])
  const [active, setActive] = useState<string>()
  const [error, setError] = useState<string>()
  const [busy, setBusy] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [shells, setShells] = useState<{ id: string; name: string }[]>([])
  const [menu, setMenu] = useState(false)

  useEffect(() => {
    let live = true
    setLoaded(false)
    if (!props.enabled) { setList([]); setLoaded(true); return }
    void api<{ terminals: TermInfo[]; shells?: { id: string; name: string }[] }>('GET', '/v1/terminals').then((r) => {
      if (!live) return
      if (r.ok) { setShells(r.value.shells ?? []); setList(r.value.terminals); setActive((a) => a ?? r.value.terminals[0]?.id) } else setError(r.error)
      setLoaded(true)
    })
    return () => { live = false }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hostId, props.enabled])

  /** The shell last picked, if this host still offers it; otherwise the host chooses. */
  const remembered = useCallback((): string | undefined => {
    try { const id = localStorage.getItem(SHELL_KEY); return id && shells.some((s) => s.id === id) ? id : undefined } catch { return undefined }
  }, [shells])
  const create = useCallback(async (pick?: string) => {
    if (!folder || busy) return
    setBusy(true); setError(undefined); setMenu(false)
    if (pick) { try { localStorage.setItem(SHELL_KEY, pick) } catch { /* not remembered */ } }
    const shell = pick ?? remembered()
    const r = await api<TermInfo>('POST', '/v1/terminals', { folder, rows: 24, cols: 80, ...(shell ? { shell } : {}) })
    setBusy(false)
    if (r.ok) { setList((l) => [...l, r.value]); setActive(r.value.id) }
    else setError(r.code === 'too_many_terminals' ? 'The host allows only a few terminals at once. Close one first.' : r.code === 'path_not_allowed' ? 'The host does not allow terminals in this folder.' : r.error)
  }, [api, folder, busy, remembered])

  const closeTab = useCallback(async (id: string) => {
    await api('DELETE', `/v1/terminals/${id}`)
    setList((l) => { const next = l.filter((t) => t.id !== id); setActive((a) => (a === id ? next[next.length - 1]?.id : a)); return next })
  }, [api])

  // Open a first shell when the dock appears with none and there is a folder to open it in.
  const started = useRef(false)
  useEffect(() => {
    if (loaded && props.enabled && list.length === 0 && folder && !started.current) { started.current = true; void create() }
  }, [loaded, props.enabled, list.length, folder, create])

  const drag = (e: React.PointerEvent): void => {
    const startY = e.clientY, start = props.height
    const move = (ev: PointerEvent): void => props.onHeight(clampHeight(start + (startY - ev.clientY)))
    const up = (): void => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up)
  }
  const label = (t: TermInfo, i: number): string => `${t.shell} ${i + 1}`

  return (
    <section className="term-dock" style={{ height: props.height }} aria-label="Terminal">
      <div className="term-resize" role="separator" aria-orientation="horizontal" aria-label="Resize the terminal" tabIndex={0} onPointerDown={drag}
        onKeyDown={(e) => { if (e.key === 'ArrowUp') props.onHeight(clampHeight(props.height + 24)); if (e.key === 'ArrowDown') props.onHeight(clampHeight(props.height - 24)) }} />
      <div className="term-bar">
        <span className="term-title">{I.term}Terminal <span className="sub">on {props.hostName}</span></span>
        <div className="term-tabs" role="tablist">
          {list.map((t, i) => (
            <span key={t.id} className={'term-tab' + (t.id === active ? ' on' : '')}>
              <button role="tab" aria-selected={t.id === active} onClick={() => setActive(t.id)} title={t.folder}>{label(t, i)}</button>
              <button className="term-x" aria-label={`Close ${label(t, i)}`} onClick={() => void closeTab(t.id)}>{I.close}</button>
            </span>
          ))}
        </div>
        <span className="term-new">
          <button className="icon-btn" aria-label="New terminal" title={folder ? 'New terminal in this session’s folder' : 'Open a session to start a terminal in its folder'} disabled={!props.enabled || !folder || busy} onClick={() => void create()}>{I.plus}</button>
          {shells.length > 1 && (
            <button className="icon-btn term-pick" aria-label="Choose a shell" aria-haspopup="menu" aria-expanded={menu} title="Choose which shell to open" disabled={!props.enabled || !folder || busy} onClick={() => setMenu((m) => !m)}>{I.down}</button>
          )}
          {menu && (
            <>
              <button className="term-scrim" aria-label="Close the menu" onClick={() => setMenu(false)} />
              <div className="term-menu" role="menu">
                <div className="term-menu-h">Open a new terminal in</div>
                {shells.map((s) => (
                  <button key={s.id} role="menuitem" onClick={() => void create(s.id)}>{s.name}{remembered() === s.id && <span className="sub"> · last used</span>}</button>
                ))}
              </div>
            </>
          )}
        </span>
        <button className="icon-btn" aria-label="Close the terminal panel" onClick={props.onClose}>{I.close}</button>
      </div>
      {!props.enabled ? (
        <div className="term-off">
          <b>Terminals are off on this host</b>
          <p>A terminal is a real shell with your rights, so it stays off until you turn it on.</p>
          {props.onOpenSettings && <button className="btn primary" onClick={props.onOpenSettings}>Turn on in Settings</button>}
          <p className="sub" style={{ marginTop: 10 }}>Or add this to <span className="mono">config.toml</span> on {props.hostName} and restart <span className="mono">acpd</span>:</p>
          <pre className="mono">[terminal]{'\n'}enabled = true</pre>
        </div>
      ) : (
        <div className="term-stage">
          {error && <div className="term-chip warn" role="alert">{error}<button className="icon-btn" aria-label="Dismiss" onClick={() => setError(undefined)}>{I.close}</button></div>}
          {loaded && list.length === 0 && !folder && <div className="term-off"><p>Open a session to start a terminal in its folder.</p></div>}
          {list.map((t) => (
            <TermView key={t.id} hostId={hostId} info={t} active={t.id === active} onOpenUrl={props.onOpenUrl} onEnded={() => undefined} />
          ))}
        </div>
      )}
    </section>
  )
}
