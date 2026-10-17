import type { Api } from '../electron/preload'
declare global {
  interface Window {
    api: Api
  }
}
export type Host = { id: string; hostId: string; name: string; address: string; deviceId: string; expiresAt: number; addedAt: number }
/** `GET /v1/shared` (feature `sharedDrafts`): what the phone and the desktop continue from. */
export type SharedData = { revision: number; drafts: Record<string, { text: string; updatedAt: number; device: string }>; titles: Record<string, string>; history: Record<string, string[]> }
export type HostStatus = { hostId: string; name: string; version: string; activeSessions?: number; features?: string[]; gh?: 'found' | 'missing'; /** Where the host makes folders for sessions outside every project. */ scratchRoot?: string }
export type AgentInfo = { id: string; name: string; enabled?: boolean; installed?: boolean; status?: string; executable?: string; version?: string; reason?: string; ownTools?: boolean; removable?: boolean; setup?: unknown
  /** Set when this entry is an account of another agent on the same host. */
  profileOf?: string; profileLabel?: string
  /** `off`: host-approved commands run unconfined; `auto`: in the host's OS sandbox where it has one. */
  sandbox?: 'auto' | 'off' }
export type Workspace = { path: string; name: string; removable?: boolean }
export type SessionInfo = {
  id: string
  agentId: string
  acpSessionId: string
  workspace: string
  status?: string
  setup?: Record<string, unknown>
  initialization?: Record<string, unknown>
  title?: string
  createdAt?: number
  updatedAt?: number
}
export type Worktree = { branch: string; repository?: string; base?: string; path?: string; baseDirty?: boolean; outcome?: string; commit?: string; isolationNote?: string }
export type Decision = { requestId: string; digest?: string; source?: string; kind?: string; risk?: string; facts?: unknown; decidable?: boolean; reason?: string; note?: string }
export type Task = {
  id: string
  parent?: string | null
  root: string
  depth: number
  agent: string
  scope: string
  state: string
  session?: string | null
  createdAt: number
  updatedAt: number
  summary?: string | null
  stopReason?: string | null
  costUsd?: number
  worktree?: Worktree
  pendingDecisions?: number
  needsDecision?: Decision[]
  /** Host-computed changes of the task worktree, shell-made edits included (task detail only). */
  worktreeChanges?: { path: string; status: string; diff?: string; newText?: string | null }[]
  delegate?: string
}
export type Result<T> = { ok: true; value: T } | { ok: false; error: string; code?: string; status?: number }

/** Sessions an agent remembers for one folder (`GET /v1/agents/{id}/sessions`). */
export type EarlierSessions = { listSupported: boolean; loadSupported: boolean; sessions: { sessionId: string; title?: string | null; updatedAt?: string | null }[] }
