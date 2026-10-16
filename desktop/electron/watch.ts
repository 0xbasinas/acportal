// Background watch of every session on every connected host. Each session gets a read-only
// socket (it never takes the controller lease, so the window's own socket and other devices
// keep working). The same reducer as the window turns envelopes into a timeline, from which
// we keep only what the sidebar and notifications need: pending approvals and busy state.
import { Notification, type BrowserWindow } from 'electron'
import { SessionSocket } from './host'
import { emptyTimeline, reduce, type Timeline } from '../src/acp'

export type WatchTarget = { hostRef: string; hostName: string; sessionId: string; title: string }
export type Pending = { id: string | number; title: string; allowOnce?: string; lines: string[]; outside: string[] }
export type WatchState = { pending: Pending[]; busy: boolean; conn: string; usage?: Timeline['usage']; reply?: string; turns: number }
type Entry = { t: WatchTarget; sock: SessionSocket; tl: Timeline; ready: boolean; notified: Set<string>; busy: boolean; conn: string; reply?: string; turns: number }

export class Watcher {
  private entries = new Map<string, Entry>()
  private focused?: string
  private timer?: NodeJS.Timeout
  constructor(
    private win: () => BrowserWindow | null,
    private creds: (hostRef: string) => { address: string; token: string },
    private notify: boolean = true
  ) {}

  setNotify(on: boolean): void { this.notify = on }
  /** The session the window shows; no notification for it while the window has focus. */
  focus(key?: string): void { this.focused = key }

  sync(targets: WatchTarget[]): void {
    const want = new Map(targets.map((t) => [`${t.hostRef}/${t.sessionId}`, t]))
    for (const [k, e] of this.entries) if (!want.has(k)) { e.sock.close(); this.entries.delete(k) }
    for (const [k, t] of want) {
      const cur = this.entries.get(k)
      if (cur) { cur.t = t; continue }
      let c: { address: string; token: string }
      try { c = this.creds(t.hostRef) } catch { continue }
      const e: Entry = { t, tl: emptyTimeline(), ready: false, notified: new Set(), busy: false, conn: 'connecting', turns: 0, sock: undefined as unknown as SessionSocket }
      e.sock = new SessionSocket(c.address, c.token, t.sessionId, (env) => this.onEnvelope(k, env as Record<string, unknown>), (s) => { e.conn = (s as { kind: string }).kind; this.emit() }, true)
      this.entries.set(k, e)
      e.sock.connect()
    }
    this.emit()
  }

  private onEnvelope(key: string, env: Record<string, unknown>): void {
    const e = this.entries.get(key)
    if (!e) return
    if (env.type === 'replay_start') e.ready = false
    if (env.type === 'replay_complete') e.ready = true
    const wasBusy = e.tl.busy
    e.tl = reduce(e.tl, env)
    // Only the pending list and busy flag are kept; drop the rest so long sessions stay small.
    // Keep the trailing agent message too, so streamed chunks still merge into one reply.
    const items = e.tl.items
    const last = items[items.length - 1]
    if (last?.kind === 'agent') e.reply = last.text.slice(-8000)
    if (items.some((i) => i.kind === 'user')) e.turns += items.filter((i) => i.kind === 'user').length
    e.tl = { ...e.tl, items: items.filter((i, n) => (i.kind === 'permission' && i.answered === undefined) || (n === items.length - 1 && i.kind === 'agent')) }
    const pending = this.pendingOf(e)
    if (e.ready) {
      for (const p of pending) {
        const id = `p${String(p.id)}`
        if (e.notified.has(id)) continue
        e.notified.add(id)
        this.toast(key, e, `${e.t.hostName}: approval needed`, `${e.t.title}\n${p.title}${p.lines[0] ? `\n$ ${p.lines[0]}` : ''}${p.outside.length ? `\n⚠ Outside the workspace: ${p.outside.join(', ')}` : ''}`, p)
      }
      if (wasBusy && !e.tl.busy) this.toast(key, e, `${e.t.hostName}: turn finished`, e.t.title)
    }
    if (e.tl.busy !== e.busy || pending.length || env.type === 'replay_complete' || last?.kind === 'agent' || env.type === 'event') { e.busy = e.tl.busy; this.emitSoon() }
  }

  private pendingOf(e: Entry): Pending[] {
    return e.tl.items.flatMap((i) => i.kind === 'permission' && i.answered === undefined
      ? [{ id: i.id, title: i.title, lines: i.lines, outside: i.scope?.outside ?? [], allowOnce: i.options.find((o) => o.kind === 'allow_once')?.optionId }] : [])
  }

  private toast(key: string, e: Entry, title: string, body: string, p?: Pending): void {
    const w = this.win()
    if (!this.notify || !Notification.isSupported()) return
    if (w && w.isFocused() && this.focused === key) return
    const n = new Notification({ title, body, silent: false, actions: p?.allowOnce && !p.outside.length ? [{ type: 'button', text: 'Approve once' }] : [] })
    const open = (): void => { const win = this.win(); if (!win) return; if (win.isMinimized()) win.restore(); win.show(); win.focus(); win.webContents.send('watch:open', e.t.hostRef, e.t.sessionId) }
    n.on('click', open)
    // Buttons exist on macOS and Windows; elsewhere the click opens the session to decide there.
    // Out-of-workspace requests must be read in full in the app, never approved from a toast.
    n.on('action', () => { if (p?.allowOnce && !p.outside.length) void this.approve(key, p.id, p.allowOnce) })
    n.show()
  }

  /** Answer one request. The window answers when it holds the session; otherwise a short
   *  controlling connection sends the answer and closes. */
  async approve(key: string, id: string | number, optionId: string): Promise<void> {
    const e = this.entries.get(key)
    if (!e) return
    if (this.focused === key) { this.win()?.webContents.send('watch:answer', key, id, optionId); return }
    const c = this.creds(e.t.hostRef)
    await new Promise<void>((resolve) => {
      let done = false
      const s = new SessionSocket(c.address, c.token, e.t.sessionId, () => {}, (st) => {
        const k = (st as { kind: string }).kind
        if (k === 'connected' && !done) {
          done = true
          try { s.send({ jsonrpc: '2.0', id, result: { outcome: { outcome: 'selected', optionId } } }) } catch { /* reported by the watcher state */ }
          setTimeout(() => { s.close(); resolve() }, 600)
        } else if (k === 'failed') { s.close(); resolve() }
      })
      s.cursor = e.tl.lastSeq
      s.connect()
      setTimeout(() => { if (!done) { s.close(); resolve() } }, 15_000)
    })
  }

  /** Send one message to a session the window is not attached to over a short controlling
   *  connection (starting a prompt in each run of a multi-agent run). */
  async sendOnce(hostRef: string, sessionId: string, message: unknown): Promise<boolean> {
    const c = this.creds(hostRef)
    return await new Promise<boolean>((resolve) => {
      let done = false
      const s = new SessionSocket(c.address, c.token, sessionId, () => {}, (st) => {
        const k = (st as { kind: string }).kind
        if (k === 'connected' && !done) {
          done = true
          try { s.send(message) } catch { s.close(); resolve(false); return }
          setTimeout(() => { s.close(); resolve(true) }, 800)
        } else if (k === 'failed') { s.close(); if (!done) { done = true; resolve(false) } }
      })
      s.cursor = 0
      s.connect()
      setTimeout(() => { if (!done) { done = true; s.close(); resolve(false) } }, 15_000)
    })
  }

  private emitSoon(): void {
    if (this.timer) return
    this.timer = setTimeout(() => { this.timer = undefined; this.emit() }, 150)
  }
  snapshot(): Record<string, WatchState> {
    const out: Record<string, WatchState> = {}
    for (const [k, e] of this.entries) out[k] = { pending: this.pendingOf(e), busy: e.busy, conn: e.conn, usage: e.tl.usage, reply: e.reply, turns: e.turns }
    return out
  }
  emit(): void { this.win()?.webContents.send('watch:state', this.snapshot()) }
  closeAll(): void { for (const e of this.entries.values()) e.sock.close(); this.entries.clear() }
}
