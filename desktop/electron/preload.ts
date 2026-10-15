import { contextBridge, ipcRenderer, IpcRendererEvent } from 'electron'

type Result<T> = { ok: true; value: T } | { ok: false; error: string; code?: string; status?: number }

const api = {
  getVersion: (): Promise<string> => ipcRenderer.invoke('app:get-version'),
  platform: process.platform,
  hosts: {
    list: (): Promise<Result<unknown>> => ipcRenderer.invoke('hosts:list'),
    parseLink: (s: string): Promise<Result<{ address?: string; code?: string }>> => ipcRenderer.invoke('hosts:parse-link', s),
    pair: (address: string, code: string, deviceName?: string): Promise<Result<unknown>> => ipcRenderer.invoke('hosts:pair', address, code, deviceName),
    forget: (id: string, revoke: boolean): Promise<Result<void>> => ipcRenderer.invoke('hosts:forget', id, revoke),
    api: (id: string, method: string, path: string, body?: unknown, query?: Record<string, string>): Promise<Result<unknown>> =>
      ipcRenderer.invoke('hosts:api', id, method, path, body, query)
  },
  session: {
    attach: (hostId: string, sessionId: string, after: number, readOnly = false): Promise<Result<string>> => ipcRenderer.invoke('session:attach', hostId, sessionId, after, readOnly),
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
  prefs: {
    get: (): Promise<Result<Record<string, unknown>>> => ipcRenderer.invoke('prefs:get'),
    set: (p: Record<string, unknown>): Promise<Result<void>> => ipcRenderer.invoke('prefs:set', p)
  },
  secureStorage: (): Promise<string> => ipcRenderer.invoke('app:secure-storage'),
  capture: (name: string): Promise<Result<string>> => ipcRenderer.invoke('debug:capture', name)
}

export type Api = typeof api
contextBridge.exposeInMainWorld('api', api)
