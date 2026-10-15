import { useEffect, useState } from 'react'
import type { AgentInfo, Host, HostStatus, Workspace } from './types'
import { I, tildify } from './ui'

export type Theme = 'dark' | 'light' | 'system'
const SECTIONS = ['General', 'Agents', 'MCP servers', 'Hosts', 'Workspaces', 'Approvals & review', 'Appearance', 'Keyboard'] as const
export type Section = (typeof SECTIONS)[number]

export function Settings(props: {
  section: Section; setSection: (s: Section) => void; onBack: () => void
  hosts: Host[]; current?: Host; status?: HostStatus; online: boolean; agents: AgentInfo[]; workspaces: Workspace[]
  theme: Theme; setTheme: (t: Theme) => void; onPairNew: () => void; onForget: (h: Host, revoke: boolean) => void; onSwitch: (h: Host) => void
  onAddWorkspace: (p: string) => Promise<string | undefined>; onRemoveWorkspace: (p: string) => void
  api: <T>(method: string, path: string, body?: unknown) => Promise<{ ok: true; value: T } | { ok: false; error: string; code?: string }>
  onAgentsChanged: () => void; mcp: McpServer[]; setMcp: (m: McpServer[]) => void
}) {
  const { section } = props
  const [version, setVersion] = useState('')
  const [storage, setStorage] = useState('')
  const [adding, setAdding] = useState(false)
  const [hostStore, setHostStore] = useState<Record<string, string>>({})
  useEffect(() => { void Promise.all(props.hosts.map(async (h) => [h.id, ((await window.api.storageOf(h.id)) as { ok: boolean; value?: string }).value ?? ''] as const)).then((e) => setHostStore(Object.fromEntries(e))) }, [props.hosts])
  useEffect(() => {
    void window.api.getVersion().then(setVersion)
    void window.api.secureStorage().then(setStorage)
  }, [])
  return (
    <div className="settings">
      <nav className="settings-nav">
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '0 8px 18px' }}>
          <button className="icon-btn" onClick={props.onBack} aria-label="Back">{I.back}</button>
          <span style={{ fontSize: 18, fontWeight: 600 }}>Settings</span>
        </div>
        {SECTIONS.map((s) => (
          <div key={s} className={'row' + (s === section ? ' active' : '')} onClick={() => props.setSection(s)}>
            <span>{s}</span><span className="sub">{I.chev}</span>
          </div>
        ))}
      </nav>
      <div className="settings-body">
        {section === 'General' && (
          <>
            <h2>General</h2>
            <div className="sub" style={{ marginBottom: 18 }}>acportal desktop {version}</div>
            <div className="set-row"><div style={{ flex: 1 }}><div>Credential storage</div><div className={'sub' + (storage === 'file' ? ' amber' : '')}>{storage === 'file' ? 'Weaker storage: no system keychain, so tokens are encrypted with a key kept in an owner-only file in the app folder' : `System keychain (${storage.replace('keychain:', '')})`}</div></div></div>
            <div className="set-row"><div style={{ flex: 1 }}><div>Connection</div><div className="sub">HTTPS with system trust for remote hosts; localhost may use HTTP. Certificate checks cannot be turned off.</div></div></div>
            <div className="set-row"><div style={{ flex: 1 }}><div>Prompts after a disconnect</div><div className="sub">Never resent automatically; a lost connection does not cancel a turn or approve anything.</div></div></div>
          </>
        )}
        {section === 'Agents' && (
          <>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <h2>Agents</h2>
              <button className="pill" onClick={() => setAdding(!adding)}>{I.plus} Add agent</button>
            </div>
            <div className="sub" style={{ marginBottom: 14 }}>Agents registered on {props.current?.name ?? 'the host'}. Any agent that speaks ACP over stdio.</div>
            {adding && <AddAgent api={props.api} onDone={() => { setAdding(false); props.onAgentsChanged() }} />}
            {props.agents.length === 0 && <div className="sub2">{props.online ? 'The host lists no agents.' : 'Host offline.'}</div>}
            {props.agents.map((a) => (
              <div className="set-row" key={a.id}>
                <span className="sub">{I.gear}</span>
                <div style={{ flex: 1 }}>
                  <div>{a.name}</div>
                  <div className={'sub' + (a.status === 'available' || a.installed ? '' : ' amber')}>
                    <span className="mono">{a.id}</span>{a.version ? ` · ${a.version}` : ''}{a.ownTools ? ' · own tools' : ''} · {a.enabled === false ? 'Disabled in the registry' : a.status ?? (a.installed ? 'Available' : 'Missing')}{a.reason ? ` · ${a.reason}` : ''}
                  </div>
                </div>
                {a.removable && <button className="pill" onClick={async () => { if (!confirm(`Remove ${a.name} from the host?`)) return; const r = await props.api('DELETE', `/v1/agents/${encodeURIComponent(a.id)}`); if (!r.ok) alert(r.error); props.onAgentsChanged() }}>Remove</button>}
                <span className={'toggle' + (a.enabled !== false ? ' on' : '')} title="Enabled in the host registry (edit on the host)" />
              </div>
            ))}
          </>
        )}
        {section === 'Hosts' && (
          <>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <h2>Hosts</h2>
              <button className="pill" onClick={props.onPairNew}>{I.plus} Pair a host</button>
            </div>
            <div className="sub" style={{ marginBottom: 14 }}>Each host is paired separately with a one-use code.</div>
            {Object.values(hostStore).includes('file') && <div className="banner" style={{ margin: '0 0 12px', color: 'var(--amber)' }}>Weaker storage on this computer: no system keychain was found, so host tokens are encrypted with a key kept in an owner-only file next to them. Anyone who can read your user files could use them. Install a keyring (GNOME Keyring or KWallet) and pair again for keychain storage.</div>}
            {props.hosts.map((h) => {
              const cur = h.id === props.current?.id
              return (
                <div className="set-row" key={h.id}>
                  <span className={'dot ' + (cur && props.online ? 'green' : '')} />
                  <div style={{ flex: 1, cursor: 'pointer' }} onClick={() => props.onSwitch(h)}>
                    <div>{h.name}{cur ? ' · current' : ''}</div>
                    <div className="sub mono">{h.address}{cur && props.status ? ` · acpd ${props.status.version} · ${props.online ? 'Connected' : 'Offline'}` : ''}{hostStore[h.id] ? ` · token in ${hostStore[h.id] === 'enc' ? 'keychain' : hostStore[h.id] === 'file' ? 'encrypted file (weaker)' : 'memory'}` : ''}</div>
                  </div>
                  <button className="pill" onClick={() => props.onForget(h, true)} title="Revoke this device on the host and forget it here">Unpair</button>
                </div>
              )
            })}
          </>
        )}
        {section === 'Workspaces' && <Workspaces {...props} />}
        {section === 'MCP servers' && <McpEditor servers={props.mcp} setServers={props.setMcp} />}
        {section === 'Approvals & review' && (
          <>
            <h2>Approvals &amp; review</h2>
            <div className="sub" style={{ marginBottom: 14 }}>How decisions reach you. Rules and smart review are configured on the host.</div>
            <div className="set-row"><div style={{ flex: 1 }}><div>Shell commands</div><div className="sub">Shown line by line with the host's auto-review verdict. "Allow once" runs the exact lines once.</div></div></div>
            <div className="set-row"><div style={{ flex: 1 }}><div>Delegated tasks</div><div className="sub">Requests a supervising agent may not decide come here with a badge on the task tree.</div></div></div>
            <div className="set-row"><div style={{ flex: 1 }}><div>Host smart review</div><div className="sub mono">[shell_review.&lt;agent&gt;] in the host config</div></div></div>
          </>
        )}
        {section === 'Appearance' && (
          <>
            <h2>Appearance</h2>
            <div className="sub">Matches the Android app: near-black, near-white, no accent colour.</div>
            <div className="theme-cards">
              {(['dark', 'light', 'system'] as Theme[]).map((t) => (
                <div key={t} className={'theme-card' + (props.theme === t ? ' on' : '')} onClick={() => props.setTheme(t)}>
                  <div className="pv" style={{ background: t === 'dark' ? '#0a0a0a' : t === 'light' ? '#fafafa' : 'linear-gradient(135deg,#0a0a0a 50%,#fafafa 50%)', border: '1px solid var(--line)' }}>
                    <div style={{ margin: 12, width: 60, height: 10, borderRadius: 5, background: t === 'light' ? '#111' : '#eee' }} />
                    <div style={{ margin: '0 12px', width: 90, height: 8, borderRadius: 4, background: '#72d5b0', opacity: 0.8 }} />
                  </div>
                  <div style={{ marginTop: 8, textTransform: 'capitalize' }}>{t}</div>
                </div>
              ))}
            </div>
          </>
        )}
        {section === 'Keyboard' && (
          <>
            <h2>Keyboard</h2>
            {[
              ['Command palette', 'Ctrl/⌘ K'], ['New session', 'Ctrl/⌘ N'], ['Settings', 'Ctrl/⌘ ,'], ['Review changes', 'Ctrl/⌘ D'], ['Stop the current turn', 'Esc'], ['Send', 'Enter'], ['New line', 'Shift Enter']
            ].map(([a, k]) => (
              <div className="set-row" key={a}><span style={{ flex: 1 }}>{a}</span><span className="kbd">{k}</span></div>
            ))}
          </>
        )}
      </div>
    </div>
  )
}

function Workspaces(props: { workspaces: Workspace[]; onAddWorkspace: (p: string) => Promise<string | undefined>; onRemoveWorkspace: (p: string) => void }) {
  const [path, setPath] = useState('')
  const [err, setErr] = useState<string>()
  return (
    <>
      <h2>Workspaces</h2>
      <div className="sub" style={{ marginBottom: 14 }}>Folders agents may work in. Folders added here must be inside the host's workspace roots.</div>
      {props.workspaces.map((w) => (
        <div className="set-row" key={w.path}>
          <div style={{ flex: 1 }}><div>{w.name}</div><div className="sub mono">{tildify(w.path)}</div></div>
          {w.removable && <button className="pill" onClick={() => props.onRemoveWorkspace(w.path)}>Remove</button>}
        </div>
      ))}
      <div style={{ display: 'flex', gap: 8, marginTop: 14 }}>
        <input className="input mono" style={{ flex: 1 }} placeholder="/absolute/path/on/host" value={path} onChange={(e) => setPath(e.target.value)} />
        <button className="pill" disabled={!path} onClick={async () => { const e = await props.onAddWorkspace(path); setErr(e); if (!e) setPath('') }}>Add folder</button>
      </div>
      {err && <div className="error">{err}</div>}
    </>
  )
}

export type McpServer = { name: string; type: 'stdio' | 'http' | 'sse'; command?: string; args?: string[]; env?: { name: string; value: string }[]; url?: string; headers?: { name: string; value: string }[]; enabled: boolean }
/** ACP wire form for POST /v1/sessions mcpServers (stable ACP session-setup shape). */
export function mcpWire(m: McpServer): unknown {
  if (m.type === 'stdio') return { name: m.name, command: m.command, args: m.args ?? [], env: m.env ?? [] }
  return { type: m.type, name: m.name, url: m.url, headers: m.headers ?? [] }
}
const pairs = (s: string): { name: string; value: string }[] => s.split('\n').map((l) => l.trim()).filter(Boolean).map((l) => { const i = l.indexOf('='); return { name: l.slice(0, i).trim(), value: l.slice(i + 1) } })

function McpEditor({ servers, setServers }: { servers: McpServer[]; setServers: (m: McpServer[]) => void }) {
  const blank: McpServer = { name: '', type: 'stdio', command: '', args: [], env: [], enabled: true }
  const [edit, setEdit] = useState<{ i: number; m: McpServer; env: string } | null>(null)
  const [err, setErr] = useState<string>()
  const save = (): void => {
    const prev = (edit!.m.type === 'stdio' ? edit!.m.env : edit!.m.headers) ?? []
    const m = { ...edit!.m, [edit!.m.type === 'stdio' ? 'env' : 'headers']: pairs(edit!.env).map((p) => (p.value === '••••' ? { name: p.name, value: prev.find((q) => q.name === p.name)?.value ?? '' } : p)) }
    if (!/^[\w.-]{1,64}$/.test(m.name)) return setErr('Name: letters, digits, dot, dash, underscore')
    if (servers.some((s, j) => s.name === m.name && j !== edit!.i)) return setErr('Names must be unique')
    if (m.type === 'stdio' && !(m.command ?? '').startsWith('/')) return setErr('The host needs an absolute executable path (e.g. /usr/bin/npx)')
    if (m.type !== 'stdio' && !/^https?:\/\//.test(m.url ?? '')) return setErr('Enter an http(s) URL')
    const next = [...servers]
    if (edit!.i < 0) next.push(m); else next[edit!.i] = m
    if (next.filter((x) => x.enabled).length > 16) return setErr('The host accepts at most 16 servers per session')
    setServers(next); setEdit(null); setErr(undefined)
  }
  return (
    <>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h2>MCP servers</h2>
        <button className="pill" onClick={() => setEdit({ i: -1, m: blank, env: '' })}>{I.plus} Add server</button>
      </div>
      <div className="sub" style={{ marginBottom: 14 }}>Saved on this computer and sent with each new session. The agent runs them on the host; HTTP and SSE need agent support.</div>
      {servers.length === 0 && !edit && <div className="sub2">No servers yet.</div>}
      {servers.map((m, i) => (
        <div className="set-row" key={m.name}>
          <span className="sub">⛭</span>
          <div style={{ flex: 1, cursor: 'pointer' }} onClick={() => setEdit({ i, m, env: (m.type === 'stdio' ? m.env : m.headers)?.map((p) => `${p.name}=${p.value ? '••••' : ''}`).join('\n') ?? '' })}>
            <div>{m.name}</div>
            <div className="sub mono">{m.type} · {m.type === 'stdio' ? [m.command, ...(m.args ?? [])].join(' ') : m.url}{(m.env?.length || m.headers?.length) ? ` · ${(m.env ?? m.headers)!.length} secret${(m.env ?? m.headers)!.length > 1 ? 's' : ''}` : ''}</div>
          </div>
          <button className="pill" onClick={() => setServers(servers.filter((_, j) => j !== i))}>Delete</button>
          <span className={'toggle' + (m.enabled ? ' on' : '')} style={{ cursor: 'pointer' }} onClick={() => setServers(servers.map((x, j) => (j === i ? { ...x, enabled: !x.enabled } : x)))} title="Send with new sessions" />
        </div>
      ))}
      {edit && (
        <div className="card" style={{ marginTop: 14 }}>
          <h3>{edit.i < 0 ? 'Add MCP server' : `Edit ${edit.m.name}`}</h3>
          <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>{(['stdio', 'http', 'sse'] as const).map((t) => <button key={t} className={'pill' + (edit.m.type === t ? ' primary' : '')} onClick={() => setEdit({ ...edit, m: { ...edit.m, type: t } })}>{t}</button>)}</div>
          <div className="field"><label>Name</label><input className="input mono" value={edit.m.name} onChange={(e) => setEdit({ ...edit, m: { ...edit.m, name: e.target.value } })} /></div>
          {edit.m.type === 'stdio' ? (
            <>
              <div className="field"><label>Command (absolute path on the host)</label><input className="input mono" placeholder="/usr/bin/npx" value={edit.m.command} onChange={(e) => setEdit({ ...edit, m: { ...edit.m, command: e.target.value } })} /></div>
              <div className="field"><label>Arguments (one per line)</label><textarea className="input mono" style={{ height: 70, padding: 10 }} value={(edit.m.args ?? []).join('\n')} onChange={(e) => setEdit({ ...edit, m: { ...edit.m, args: e.target.value.split('\n') } })} /></div>
            </>
          ) : (
            <div className="field"><label>URL</label><input className="input mono" placeholder="https://mcp.example.com/mcp" value={edit.m.url} onChange={(e) => setEdit({ ...edit, m: { ...edit.m, url: e.target.value } })} /></div>
          )}
          <div className="field"><label>{edit.m.type === 'stdio' ? 'Environment' : 'Headers'} (NAME=value per line; kept on this computer, never shown again)</label><textarea className="input mono" style={{ height: 70, padding: 10 }} value={edit.env} onChange={(e) => setEdit({ ...edit, env: e.target.value })} /></div>
          {err && <div className="error">{err}</div>}
          <div style={{ display: 'flex', gap: 8, marginTop: 14 }}><button className="btn-wide" onClick={save}>Save</button><button className="btn-wide ghost" style={{ width: 120 }} onClick={() => setEdit(null)}>Cancel</button></div>
        </div>
      )}
    </>
  )
}

function AddAgent({ api, onDone }: { api: <T>(m: string, p: string, b?: unknown) => Promise<{ ok: true; value: T } | { ok: false; error: string }>; onDone: () => void }) {
  const [f, setF] = useState({ id: '', name: '', command: '', args: '', env: '', ownTools: false })
  const [check, setCheck] = useState<string>()
  const [err, setErr] = useState<string>()
  const [found, setFound] = useState<{ command: string; found: boolean; executable?: string }[]>()
  useEffect(() => { void api<{ command: string; found: boolean; executable?: string }[]>('POST', '/v1/agents/locate', { commands: ['claude-code-acp', 'codex-acp', 'gemini', 'opencode', 'goose', 'qwen', 'kiro-cli'] }).then((r) => r.ok && setFound(r.value.filter((x) => x.found))) }, [])
  const envObj = (): Record<string, string> => Object.fromEntries(pairs(f.env).map((p) => [p.name, p.value]))
  return (
    <div className="card" style={{ marginBottom: 16 }}>
      <h3>Add an agent to the host</h3>
      <div className="sub2">Registered in the host's user settings. Provider keys belong in the host's environment, not here.</div>
      {found && found.length > 0 && <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginTop: 10 }}><span className="sub">Found on the host:</span>{found.map((x) => <button key={x.command} className="pill" onClick={() => setF({ ...f, id: x.command.replace(/[^\w-]/g, ''), name: x.command, command: x.executable ?? x.command, args: x.command === 'gemini' ? '--experimental-acp' : x.command === 'goose' || x.command === 'opencode' ? 'acp' : '' })}>{x.command}</button>)}</div>}
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
        <div className="field"><label>Id</label><input className="input mono" value={f.id} onChange={(e) => setF({ ...f, id: e.target.value })} /></div>
        <div className="field"><label>Name</label><input className="input" value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} /></div>
      </div>
      <div className="field"><label>Command</label><input className="input mono" placeholder="/usr/local/bin/claude-code-acp" value={f.command} onChange={(e) => setF({ ...f, command: e.target.value })} /></div>
      <div className="field"><label>Arguments (space separated)</label><input className="input mono" value={f.args} onChange={(e) => setF({ ...f, args: e.target.value })} /></div>
      <div className="field"><label>Environment (NAME=value per line, non-secret settings)</label><textarea className="input mono" style={{ height: 60, padding: 10 }} value={f.env} onChange={(e) => setF({ ...f, env: e.target.value })} /></div>
      <div className="set-row"><span className={'toggle' + (f.ownTools ? ' on' : '')} style={{ cursor: 'pointer' }} onClick={() => setF({ ...f, ownTools: !f.ownTools })} /><span>The agent edits files with its own tools (host consents cannot see those edits)</span></div>
      {check && <div className="sub2">{check}</div>}
      {err && <div className="error">{err}</div>}
      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button className="pill" disabled={!f.command} onClick={async () => { const r = await api<{ found: boolean; executable: boolean; versionOk: boolean; version?: string }>('POST', '/v1/agents/check', { command: f.command, env: envObj() }); setCheck(r.ok ? (r.value.found ? `Found${r.value.version ? ' · ' + r.value.version : ''}${r.value.versionOk ? '' : ' · --version failed'}` : 'Not found on the host') : r.error) }}>Check on host</button>
        <button className="pill primary" disabled={!f.id || !f.name || !f.command} onClick={async () => { const r = await api('POST', '/v1/agents', { id: f.id, name: f.name, command: f.command, args: f.args.split(' ').filter(Boolean), env: envObj(), ...(f.ownTools ? { ownTools: true } : {}) }); if (r.ok) onDone(); else setErr(r.error) }}>Add agent</button>
      </div>
    </div>
  )
}
