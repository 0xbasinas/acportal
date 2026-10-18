import { contextBridge, ipcRenderer, IpcRendererEvent } from 'electron'

import type { BrowserState } from './browserTypes'

type Result<T> = { ok: true; value: T } | { ok: false; error: string; code?: string; status?: number }

const api = {
  getVersion: (): Promise<string> => ipcRenderer.invoke('app:get-version'),
  platform: process.platform,
  hosts: {
    list: (): Promise<Result<unknown>> => ipcRenderer.invoke('hosts:list'),
    parseLink: (s: string): Promise<Result<{ address?: string; code?: string }>> => ipcRenderer.invoke('hosts:parse-link', s),
    pair: (address: string, code: string, deviceName?: string, replaceId?: string): Promise<Result<unknown>> => ipcRenderer.invoke('hosts:pair', address, code, deviceName, replaceId),
    forget: (id: string, revoke: boolean): Promise<Result<void>> => ipcRenderer.invoke('hosts:forget', id, revoke),
    rename: (id: string, name: string): Promise<Result<void>> => ipcRenderer.invoke('hosts:rename', id, name),
    reorder: (ids: string[]): Promise<Result<void>> => ipcRenderer.invoke('hosts:reorder', ids),
    api: (id: string, method: string, path: string, body?: unknown, query?: Record<string, string>): Promise<Result<unknown>> =>
      ipcRenderer.invoke('hosts:api', id, method, path, body, query)
  },
  session: {
    attach: (hostId: string, sessionId: string, after: number, readOnly = false, takeover = false): Promise<Result<string>> => ipcRenderer.invoke('session:attach', hostId, sessionId, after, readOnly, takeover),
    send: (key: string, msg: unknown): Promise<Result<void>> => ipcRenderer.invoke('session:send', key, msg),
    detach: (key: string): Promise<Result<void>> => ipcRenderer.invoke('session:detach', key),
    onEnvelope: (cb: (key: string, env: unknown) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, k: string, env: unknown): void => cb(k, env)
      ipcRenderer.on('session:envelope', l)
      return () => ipcRenderer.removeListener('session:envelope', l)
    },
    onState: (cb: (key: string, state: unknown) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, k: string, s: unknown): void => cb(k, s)
      ipcRenderer.on('session:state', l)
      return () => ipcRenderer.removeListener('session:state', l)
    }
  },
  watch: {
    sync: (targets: { hostRef: string; hostName: string; sessionId: string; title: string }[]): Promise<Result<void>> => ipcRenderer.invoke('watch:sync', targets),
    snapshot: (): Promise<Result<Record<string, { pending: { id: string | number; title: string; allowOnce?: string; lines: string[]; outside: string[] }[]; busy: boolean; conn: string; usage?: { used?: number; size?: number; cost?: { amount: number; currency: string } }; reply?: string; turns: number }>>> => ipcRenderer.invoke('watch:snapshot'),
    send: (hostRef: string, sessionId: string, message: unknown): Promise<Result<boolean>> => ipcRenderer.invoke('watch:send', hostRef, sessionId, message),
    focus: (key?: string): Promise<Result<void>> => ipcRenderer.invoke('watch:focus', key),
    notify: (on: boolean): Promise<Result<void>> => ipcRenderer.invoke('watch:notify', on),
    approve: (key: string, id: string | number, optionId: string): Promise<Result<void>> => ipcRenderer.invoke('watch:approve', key, id, optionId),
    onState: (cb: (s: Record<string, { pending: { id: string | number; title: string; allowOnce?: string; lines: string[]; outside: string[] }[]; busy: boolean; conn: string; usage?: { used?: number; size?: number; cost?: { amount: number; currency: string } }; reply?: string; turns: number }>) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, s: Parameters<typeof cb>[0]): void => cb(s)
      ipcRenderer.on('watch:state', l)
      return () => ipcRenderer.removeListener('watch:state', l)
    },
    onOpen: (cb: (hostRef: string, sessionId?: string) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, h: string, s?: string): void => cb(h, s)
      ipcRenderer.on('watch:open', l)
      return () => ipcRenderer.removeListener('watch:open', l)
    },
    onAnswer: (cb: (key: string, id: string | number, optionId: string) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, k: string, id: string | number, o: string): void => cb(k, id, o)
      ipcRenderer.on('watch:answer', l)
      return () => ipcRenderer.removeListener('watch:answer', l)
    }
  },
  notify: (title: string, body: string, hostRef: string, sessionId?: string): Promise<Result<void>> => ipcRenderer.invoke('notify:show', title, body, hostRef, sessionId),
  prefs: {
    get: (): Promise<Result<Record<string, unknown>>> => ipcRenderer.invoke('prefs:get'),
    set: (p: Record<string, unknown>): Promise<Result<void>> => ipcRenderer.invoke('prefs:set', p)
  },
  secureStorage: (): Promise<string> => ipcRenderer.invoke('app:secure-storage'),
  files: { pick: (): Promise<Result<{ name: string; mimeType: string; size: number; data?: string; text?: string; path: string }[]>> => ipcRenderer.invoke('files:pick') },
  storageOf: (id: string): Promise<Result<string>> => ipcRenderer.invoke('hosts:storage-of', id),
  git: {
    available: (hostId: string): Promise<Result<'host' | 'local' | false>> => ipcRenderer.invoke('git:available', hostId),
    explain: (): Promise<Result<Record<string, string>>> => ipcRenderer.invoke('git:explain'),
    run: (hostId: string, cwd: string, op: string, params?: Record<string, string>, input?: string): Promise<Result<{ out: string; args: string[] }>> => ipcRenderer.invoke('git:run', hostId, cwd, op, params ?? {}, input),
    onLog: (cb: (e: { at: number; cwd: string; args: string[]; why: string; ok: boolean; ms: number }) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, entry: { at: number; cwd: string; args: string[]; why: string; ok: boolean; ms: number }): void => cb(entry)
      ipcRenderer.on('git:log', l)
      return () => ipcRenderer.removeListener('git:log', l)
    }
  },
  term: {
    attach: (hostId: string, terminalId: string, after: number): Promise<Result<string>> => ipcRenderer.invoke('term:attach', hostId, terminalId, after),
    input: (key: string, data: string): Promise<Result<void>> => ipcRenderer.invoke('term:input', key, data),
    resize: (key: string, rows: number, cols: number): Promise<Result<void>> => ipcRenderer.invoke('term:resize', key, rows, cols),
    take: (key: string): Promise<Result<void>> => ipcRenderer.invoke('term:take', key),
    detach: (key: string): Promise<Result<void>> => ipcRenderer.invoke('term:detach', key),
    onData: (cb: (key: string, bytes: Uint8Array) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, k: string, b: Uint8Array): void => cb(k, b)
      ipcRenderer.on('term:data', l)
      return () => ipcRenderer.removeListener('term:data', l)
    },
    onEvent: (cb: (key: string, event: Record<string, unknown>) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, k: string, ev: Record<string, unknown>): void => cb(k, ev)
      ipcRenderer.on('term:event', l)
      return () => ipcRenderer.removeListener('term:event', l)
    },
    onState: (cb: (key: string, state: string, reason?: string) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, k: string, s: string, r?: string): void => cb(k, s, r)
      ipcRenderer.on('term:state', l)
      return () => ipcRenderer.removeListener('term:state', l)
    }
  },
  browser: {
    open: (url?: string): Promise<Result<BrowserState>> => ipcRenderer.invoke('browser:open', url),
    go: (input: string): Promise<Result<BrowserState>> => ipcRenderer.invoke('browser:go', input),
    newTab: (url?: string): Promise<Result<BrowserState>> => ipcRenderer.invoke('browser:new-tab', url),
    selectTab: (id: number): Promise<Result<BrowserState>> => ipcRenderer.invoke('browser:select-tab', id),
    closeTab: (id: number): Promise<Result<BrowserState>> => ipcRenderer.invoke('browser:close-tab', id),
    find: (text: string, forward = true, again = false): Promise<Result<void>> => ipcRenderer.invoke('browser:find', text, forward, again),
    findStop: (): Promise<Result<void>> => ipcRenderer.invoke('browser:find-stop'),
    close: (): Promise<Result<void>> => ipcRenderer.invoke('browser:close'),
    state: (): Promise<Result<BrowserState>> => ipcRenderer.invoke('browser:state'),
    bounds: (b: { x: number; y: number; width: number; height: number } | null): Promise<Result<void>> => ipcRenderer.invoke('browser:bounds', b),
    back: (): Promise<Result<void>> => ipcRenderer.invoke('browser:back'),
    forward: (): Promise<Result<void>> => ipcRenderer.invoke('browser:forward'),
    reload: (hard?: boolean): Promise<Result<void>> => ipcRenderer.invoke('browser:reload', hard),
    zoom: (direction: 'in' | 'out' | 'reset'): Promise<Result<void>> => ipcRenderer.invoke('browser:zoom', direction),
    devtools: (): Promise<Result<void>> => ipcRenderer.invoke('browser:devtools'),
    external: (): Promise<Result<void>> => ipcRenderer.invoke('browser:external'),
    freeze: (): Promise<Result<string | null>> => ipcRenderer.invoke('browser:freeze'),
    thaw: (): Promise<Result<void>> => ipcRenderer.invoke('browser:thaw'),
    capture: (): Promise<Result<{ data: string; mimeType: string; size: number; width: number; height: number; title: string; url: string }>> => ipcRenderer.invoke('browser:capture'),
    selection: (): Promise<Result<string>> => ipcRenderer.invoke('browser:selection'),
    pick: (): Promise<Result<{ text: string; shot?: { data: string; mimeType: string; size: number; width: number; height: number }; element: { selector: string; url: string; title: string } } | null>> => ipcRenderer.invoke('browser:pick'),
    pickCancel: (): Promise<Result<void>> => ipcRenderer.invoke('browser:pick-cancel'),
    onState: (cb: (s: BrowserState) => void): (() => void) => {
      const l = (_e: IpcRendererEvent, s: BrowserState): void => cb(s)
      ipcRenderer.on('browser:state', l)
      return () => ipcRenderer.removeListener('browser:state', l)
    },
    onShortcut: (cb: (name: 'address' | 'toggle' | 'find' | 'newtab') => void): (() => void) => {
      const l = (_e: IpcRendererEvent, n: 'address' | 'toggle' | 'find' | 'newtab'): void => cb(n)
      ipcRenderer.on('browser:shortcut', l)
      return () => ipcRenderer.removeListener('browser:shortcut', l)
    }
  },
  capture: (name: string): Promise<Result<string>> => ipcRenderer.invoke('debug:capture', name)
}

export type Api = typeof api
contextBridge.exposeInMainWorld('api', api)
