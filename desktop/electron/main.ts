import { app, BrowserWindow, ipcMain, shell, safeStorage, nativeTheme, Menu } from 'electron'
import { join } from 'node:path'
import { readFileSync, writeFileSync, mkdirSync, renameSync } from 'node:fs'
import { hostname } from 'node:os'
import { randomUUID } from 'node:crypto'
import { request, validateAddress, SessionSocket, HostError } from './host'
import { sealWithFileKey, openWithFileKey } from './secrets'
import { registerGit } from './git'
import { dialog } from 'electron'
import { readFile, stat } from 'node:fs/promises'
import { basename, extname } from 'node:path'

type StoredHost = { id: string; hostId: string; name: string; address: string; deviceId: string; secret: string; expiresAt: number; addedAt: number }
type PublicHost = Omit<StoredHost, 'secret'>

let win: BrowserWindow | null = null
const storeFile = (): string => join(app.getPath('userData'), 'hosts.json')
const prefsFile = (): string => join(app.getPath('userData'), 'prefs.json')

/** OS keychain via safeStorage when a real backend exists; otherwise AES-256-GCM with a
 *  random key in an owner-only file (weaker: anyone who can read this user's files can decrypt). */
function keychainUsable(): boolean {
  if (!safeStorage.isEncryptionAvailable()) return false
  if (process.platform !== 'linux') return true
  const backend = safeStorage.getSelectedStorageBackend?.()
  return backend !== 'basic_text' && backend !== 'unknown'
}
function seal(token: string): string {
  if (keychainUsable()) return 'enc:' + safeStorage.encryptString(token).toString('base64')
  return 'file:' + sealWithFileKey(token, app.getPath('userData'))
}
const memoryTokens = new Map<string, string>()
function unseal(h: StoredHost): string {
  if (h.secret.startsWith('enc:')) return safeStorage.decryptString(Buffer.from(h.secret.slice(4), 'base64'))
  if (h.secret.startsWith('file:')) return openWithFileKey(h.secret.slice(5), app.getPath('userData'))
  const t = memoryTokens.get(h.id)
  if (!t) throw new HostError(401, 'locked', 'Pair this host again (no secure storage on this system)')
  return t
}
export function storageKind(): 'keychain' | 'file' {
  return keychainUsable() ? 'keychain' : 'file'
}
function loadHosts(): StoredHost[] {
  try {
    return JSON.parse(readFileSync(storeFile(), 'utf8')) as StoredHost[]
  } catch {
    return []
  }
}
function saveHosts(hosts: StoredHost[]): void {
  mkdirSync(app.getPath('userData'), { recursive: true })
  const tmp = storeFile() + '.tmp'
  writeFileSync(tmp, JSON.stringify(hosts, null, 1), { mode: 0o600 })
  renameSync(tmp, storeFile())
}
const publicHost = ({ secret: _s, ...rest }: StoredHost): PublicHost => rest
function host(id: string): StoredHost {
  const h = loadHosts().find((x) => x.id === id)
  if (!h) throw new Error('Unknown host')
  return h
}

/** Parses "acpd://pair?address=…&code=…" (QR/link form) or plain address + code. */
function parsePairing(input: string): { address?: string; code?: string } {
  const s = input.trim()
  if (s.startsWith('acpd://') || s.startsWith('acportal://')) {
    try {
      const u = new URL(s)
      return { address: u.searchParams.get('address') ?? u.searchParams.get('host') ?? undefined, code: u.searchParams.get('code') ?? undefined }
    } catch {
      return {}
    }
  }
  return {}
}

function wrap<A extends unknown[], R>(fn: (...a: A) => Promise<R> | R) {
  return async (_e: unknown, ...a: A): Promise<{ ok: true; value: R } | { ok: false; error: string; code?: string; status?: number }> => {
    try {
      return { ok: true, value: await fn(...a) }
    } catch (e) {
      const err = e as HostError
      return { ok: false, error: err.message, code: err.code, status: err.status }
    }
  }
}

const sockets = new Map<string, SessionSocket>()

function registerIpc(): void {
  ipcMain.handle('app:get-version', () => app.getVersion())
  ipcMain.handle('hosts:list', wrap(() => loadHosts().map(publicHost)))
  ipcMain.handle('hosts:parse-link', wrap((s: string) => parsePairing(s)))
  ipcMain.handle(
    'hosts:pair',
    wrap(async (address: string, code: string, deviceName?: string) => {
      const base = validateAddress(address)
      const addr = base.origin
      const result = (await request(addr, '/v1/pair', { method: 'POST', body: { code: code.trim(), deviceName: deviceName?.trim() || `acportal desktop (${hostname()})` } })) as {
        hostId: string
        deviceId: string
        token: string
        expiresAt: number
      }
      const status = (await request(addr, '/v1/status', { token: result.token })) as { name?: string }
      const stored: StoredHost = { id: randomUUID(), hostId: result.hostId, name: status.name || base.hostname, address: addr, deviceId: result.deviceId, secret: seal(result.token), expiresAt: result.expiresAt, addedAt: Date.now() }
      if (!stored.secret.startsWith('enc:')) memoryTokens.set(stored.id, result.token)
      const hosts = loadHosts().filter((h) => !(h.hostId === stored.hostId && h.address === stored.address))
      saveHosts([...hosts, stored])
      return publicHost(stored)
    })
  )
  ipcMain.handle(
    'hosts:forget',
    wrap(async (id: string, revoke: boolean) => {
      const h = host(id)
      if (revoke) await request(h.address, '/v1/device', { method: 'DELETE', token: unseal(h) }).catch(() => undefined)
      saveHosts(loadHosts().filter((x) => x.id !== id))
      memoryTokens.delete(id)
    })
  )
  ipcMain.handle(
    'hosts:api',
    wrap(async (id: string, method: string, path: string, body?: unknown, query?: Record<string, string>) => {
      if (!path.startsWith('/v1/') || path.includes('..')) throw new Error('Invalid host route')
      const h = host(id)
      return request(h.address, path, { method, body, query, token: unseal(h) })
    })
  )
  ipcMain.handle(
    'session:attach',
    wrap((hostRef: string, sessionId: string, after: number, readOnly: boolean) => {
      const key = `${hostRef}/${sessionId}`
      sockets.get(key)?.close()
      const h = host(hostRef)
      const sock = new SessionSocket(
        h.address,
        unseal(h),
        sessionId,
        (env) => win?.webContents.send('session:envelope', key, env),
        (state) => win?.webContents.send('session:state', key, state),
        readOnly
      )
      sock.cursor = after
      sockets.set(key, sock)
      sock.connect()
      return key
    })
  )
  ipcMain.handle('session:send', wrap((key: string, msg: unknown) => {
    const s = sockets.get(key)
    if (!s) throw new Error('Not attached')
    s.send(msg)
  }))
  ipcMain.handle('session:detach', wrap((key: string) => {
    sockets.get(key)?.close()
    sockets.delete(key)
  }))
  ipcMain.handle('prefs:get', wrap(() => {
    try {
      return JSON.parse(readFileSync(prefsFile(), 'utf8')) as Record<string, unknown>
    } catch {
      return {}
    }
  }))
  ipcMain.handle('prefs:set', wrap((p: Record<string, unknown>) => {
    mkdirSync(app.getPath('userData'), { recursive: true })
    writeFileSync(prefsFile(), JSON.stringify(p))
    if (typeof p.theme === 'string') nativeTheme.themeSource = p.theme as 'dark' | 'light' | 'system'
  }))
  ipcMain.handle('app:secure-storage', () => (storageKind() === 'keychain' ? `keychain:${process.platform === 'linux' ? safeStorage.getSelectedStorageBackend?.() : process.platform}` : 'file'))
  ipcMain.handle('hosts:storage-of', wrap((id: string) => host(id).secret.split(':')[0]))
  ipcMain.handle('files:pick', wrap(async () => {
    if (!win) return []
    const r = await dialog.showOpenDialog(win, { properties: ['openFile', 'multiSelections'] })
    const out: { name: string; mimeType: string; size: number; data?: string; text?: string; path: string }[] = []
    for (const p of r.filePaths) {
      const st = await stat(p)
      if (st.size > 512 * 1024) throw new Error(`${basename(p)} is larger than 512 KB (the host frame limit is 1 MiB)`)
      const buf = await readFile(p)
      const ext = extname(p).toLowerCase()
      const img = ({ '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp' } as Record<string, string>)[ext]
      if (img) out.push({ name: basename(p), mimeType: img, size: st.size, data: buf.toString('base64'), path: p })
      else if (!buf.subarray(0, 4096).includes(0)) out.push({ name: basename(p), mimeType: 'text/plain', size: st.size, text: buf.toString('utf8'), path: p })
      else throw new Error(`${basename(p)} is binary; only images and text files can be attached`)
    }
    return out
  }))
  registerGit(ipcMain, wrap as never, () => win, (h: string) => host(h).address, async (h: string) => {
    const x = host(h)
    const token = unseal(x)
    const ws = ((await request(x.address, '/v1/workspaces', { token })) as { path: string }[]).map((w) => w.path)
    const tasks = ((await request(x.address, '/v1/tasks', { token }).catch(() => [])) as { worktree?: { path?: string } }[]).map((t) => t.worktree?.path).filter((p): p is string => Boolean(p))
    return [...ws, ...tasks]
  })
  // Screenshot hook for local verification only; inert unless ACPORTAL_SHOT_DIR is set.
  ipcMain.handle('debug:capture', wrap(async (name: string) => {
    const dir = process.env.ACPORTAL_SHOT_DIR
    if (!dir || !win || !/^[\w.-]+$/.test(name)) throw new Error('capture disabled')
    const img = await win.webContents.capturePage()
    mkdirSync(dir, { recursive: true })
    writeFileSync(join(dir, name + '.png'), img.toPNG())
    return join(dir, name + '.png')
  }))
}

function createWindow(): void {
  win = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 960,
    minHeight: 600,
    show: false,
    backgroundColor: '#0A0A0A',
    title: 'acportal',
    webPreferences: { preload: join(__dirname, '../preload/preload.js'), contextIsolation: true, nodeIntegration: false, sandbox: true, webSecurity: true }
  })
  win.once('ready-to-show', () => win?.show())
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('https:')) void shell.openExternal(url)
    return { action: 'deny' }
  })
  win.webContents.on('will-navigate', (e) => e.preventDefault())
  const devUrl = process.env['ELECTRON_RENDERER_URL']
  if (!app.isPackaged && devUrl) void win.loadURL(devUrl)
  else void win.loadFile(join(__dirname, '../renderer/index.html'))
  win.on('closed', () => {
    for (const s of sockets.values()) s.close()
    sockets.clear()
    win = null
  })
}

app.whenReady().then(() => {
  try {
    const p = JSON.parse(readFileSync(prefsFile(), 'utf8')) as { theme?: 'dark' | 'light' | 'system' }
    if (p.theme) nativeTheme.themeSource = p.theme
  } catch {
    nativeTheme.themeSource = 'dark'
  }
  if (process.platform !== 'darwin') Menu.setApplicationMenu(null)
  registerIpc()
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})
