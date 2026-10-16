// Host connection layer: the same pairing, TLS and WebSocket rules as the Android app
// (android/universal-acp/.../data/HostApi.kt and transport/WebSocketTransport.kt).
// - HTTPS for remote hosts; plain HTTP only for loopback (debug parity with Android).
// - Certificate validation is never disabled. Trust = Node's bundled roots plus the
//   operating system's store, mirroring Android's system-only trust policy.
// - Bearer token in the Authorization header, never in a URL. WebSocket subprotocol acpd.v1.
// - Reconnect with capped exponential backoff and jitter, resuming from the last sequence.
import http from 'node:http'
import https from 'node:https'
import tls from 'node:tls'
import WebSocket from 'ws'

const LOOPBACK = new Set(['localhost', '127.0.0.1', '[::1]', '::1'])

export function validateAddress(address: string): URL {
  let url: URL
  try {
    url = new URL(address.trim())
  } catch {
    throw new Error('Enter a host address such as https://studio.example:8765')
  }
  if (url.username || url.password || url.search || url.hash || (url.pathname !== '/' && url.pathname !== '')) {
    throw new Error('Enter a host address without a path or credentials')
  }
  if (url.protocol === 'https:') return url
  if (url.protocol === 'http:' && LOOPBACK.has(url.hostname)) return url
  throw new Error('Use HTTPS for remote hosts')
}

function trustedRoots(): string[] | undefined {
  const get = (tls as unknown as { getCACertificates?: (kind: string) => string[] }).getCACertificates
  if (!get) return undefined
  try {
    return [...new Set([...get('default'), ...get('system')])]
  } catch {
    return undefined
  }
}

const httpsAgent = new https.Agent({ keepAlive: true, ca: trustedRoots(), rejectUnauthorized: true })
const MAX_BODY = 8 * 1024 * 1024

export class HostError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message)
  }
}

export function request(address: string, path: string, opts: { token?: string; method?: string; body?: unknown; query?: Record<string, string>; timeoutMs?: number } = {}): Promise<unknown> {
  const base = validateAddress(address)
  const target = new URL(path, base)
  for (const [k, v] of Object.entries(opts.query ?? {})) target.searchParams.set(k, v)
  const payload = opts.body === undefined ? undefined : Buffer.from(JSON.stringify(opts.body))
  const headers: Record<string, string> = { Accept: 'application/json' }
  if (opts.token) headers.Authorization = `Bearer ${opts.token}`
  if (payload) {
    headers['Content-Type'] = 'application/json'
    headers['Content-Length'] = String(payload.length)
  }
  const lib = target.protocol === 'https:' ? https : http
  return new Promise((resolve, reject) => {
    const req = lib.request(target, { method: opts.method ?? 'GET', headers, agent: target.protocol === 'https:' ? httpsAgent : undefined, timeout: opts.timeoutMs ?? 30_000 }, (res) => {
      const chunks: Buffer[] = []
      let size = 0
      res.on('data', (c: Buffer) => {
        size += c.length
        if (size > MAX_BODY) req.destroy(new Error('Host response too large'))
        else chunks.push(c)
      })
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8')
        let json: unknown = null
        try {
          json = text ? JSON.parse(text) : null
        } catch {
          /* non-JSON */
        }
        const status = res.statusCode ?? 0
        if (status >= 300 && status < 400) return reject(new HostError(status, 'redirect', 'The host redirected; enter its final address'))
        if (status >= 400) {
          // acpd errors: {"error": {"code", "message"?, "files"?}}; tolerate flat shapes too.
          const j = (json ?? {}) as { code?: string; message?: string; error?: string | { code?: string; message?: string; files?: string[] } }
          const inner = typeof j.error === 'object' && j.error ? j.error : undefined
          const code = inner?.code ?? j.code ?? (typeof j.error === 'string' ? j.error : undefined) ?? `http_${status}`
          const files = inner?.files?.length ? ` (${inner.files.join(', ')})` : ''
          const message = inner?.message ?? j.message ?? (status === 401 ? 'This device is no longer paired' : `Host answered ${status} (${code})`)
          return reject(new HostError(status, code, message + files))
        }
        resolve(json)
      })
    })
    req.on('timeout', () => req.destroy(new Error('Host did not answer in time')))
    req.on('error', (e) => reject(tlsMessage(e)))
    if (payload) req.write(payload)
    req.end()
  })
}

function tlsMessage(e: Error & { code?: string }): Error {
  if (e.code && /CERT|SELF_SIGNED|UNABLE_TO|HOSTNAME|ERR_TLS/.test(e.code)) {
    return new HostError(0, 'tls', 'The host certificate is not trusted for this address. Check the hostname, chain and dates; validation stays on.')
  }
  if (e.code === 'ECONNREFUSED') return new HostError(0, 'unreachable', 'Nothing is listening at that address')
  return e
}

export type ConnState = { kind: 'connecting' } | { kind: 'connected' } | { kind: 'reconnecting'; attempt: number } | { kind: 'disconnected' } | { kind: 'failed'; reason: string }

/** One controlling WebSocket attachment to a host session, with replay from a cursor. */
export class SessionSocket {
  private ws?: WebSocket
  private stopped = false
  private attempt = 0
  private timer?: NodeJS.Timeout
  cursor = 0

  constructor(
    private address: string,
    private token: string,
    private sessionId: string,
    private onMessage: (envelope: unknown) => void,
    private onState: (s: ConnState) => void,
    private readOnly = false
  ) {}

  connect(): void {
    this.stopped = false
    this.open()
  }

  private open(): void {
    const base = validateAddress(this.address)
    const url = new URL(`/v1/sessions/${encodeURIComponent(this.sessionId)}/connect`, base)
    url.searchParams.set('after', String(this.cursor))
    if (this.readOnly) url.searchParams.set('readOnly', 'true')
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
    this.onState(this.attempt === 0 ? { kind: 'connecting' } : { kind: 'reconnecting', attempt: this.attempt })
    const ws = new WebSocket(url, 'acpd.v1', {
      headers: { Authorization: `Bearer ${this.token}` },
      maxPayload: 1024 * 1024 + 4096,
      handshakeTimeout: 20_000,
      ca: url.protocol === 'wss:' ? trustedRoots() : undefined,
      followRedirects: false
    })
    this.ws = ws
    let opened = false
    let terminal: string | undefined
    ws.on('open', () => {
      opened = true
      this.attempt = 0
      this.onState({ kind: 'connected' })
    })
    ws.on('message', (data, isBinary) => {
      if (isBinary) return ws.close(1003, 'Use JSON text frames')
      try {
        const env = JSON.parse(data.toString()) as { type?: string; sequence?: number }
        if (env.type === 'event' && typeof env.sequence === 'number') this.cursor = Math.max(this.cursor, env.sequence)
        this.onMessage(env)
      } catch {
        /* ignore malformed frame */
      }
    })
    ws.on('unexpected-response', (_req, res) => {
      const code = res.statusCode ?? 0
      if (code === 401 || code === 403) terminal = 'This device is no longer paired with the host'
      else if (code === 404 || code === 410) terminal = 'The session no longer exists on the host'
      else if (code === 409) terminal = 'Another device is controlling this session'
      ws.terminate()
    })
    ws.on('error', () => {})
    ws.on('close', () => {
      if (this.ws !== ws) return
      this.ws = undefined
      if (this.stopped) return this.onState({ kind: 'disconnected' })
      if (terminal) return this.onState({ kind: 'failed', reason: terminal })
      if (opened) this.attempt = 0
      this.attempt++
      this.onState({ kind: 'reconnecting', attempt: this.attempt })
      const max = Math.min(60_000, 1000 * 2 ** Math.min(this.attempt - 1, 6))
      this.timer = setTimeout(() => this.open(), max / 2 + Math.random() * (max / 2))
    })
  }

  send(message: unknown): void {
    if (this.readOnly) throw new Error('This connection is read-only')
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) throw new Error('Wait for the connection to recover before sending')
    this.ws.send(JSON.stringify(message))
  }

  close(): void {
    this.stopped = true
    if (this.timer) clearTimeout(this.timer)
    this.ws?.close(1000, 'Client detached')
    this.onState({ kind: 'disconnected' })
  }
}
