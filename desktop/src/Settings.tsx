import { useEffect, useState } from 'react'
import type { AgentInfo, Host, HostStatus, Workspace } from './types'
import { I, tildify } from './ui'

export type Theme = 'dark' | 'light' | 'system'
const SECTIONS = ['General', 'Agents', 'Hosts', 'Workspaces', 'Approvals & review', 'Appearance', 'Keyboard'] as const
export type Section = (typeof SECTIONS)[number]

export function Settings(props: {
  section: Section; setSection: (s: Section) => void; onBack: () => void
  hosts: Host[]; current?: Host; status?: HostStatus; online: boolean; agents: AgentInfo[]; workspaces: Workspace[]
  theme: Theme; setTheme: (t: Theme) => void; onPairNew: () => void; onForget: (h: Host, revoke: boolean) => void; onSwitch: (h: Host) => void
  onAddWorkspace: (p: string) => Promise<string | undefined>; onRemoveWorkspace: (p: string) => void
}) {
  const { section } = props
  const [version, setVersion] = useState('')
  const [storage, setStorage] = useState('')
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
            <div className="set-row"><div style={{ flex: 1 }}><div>Credential storage</div><div className="sub">{storage === 'unavailable' ? 'No system keychain: tokens stay in memory until you quit' : storage === 'basic_text' ? 'No desktop keyring found; tokens are only obfuscated on disk' : `System keychain (${storage})`}</div></div></div>
            <div className="set-row"><div style={{ flex: 1 }}><div>Connection</div><div className="sub">HTTPS with system trust for remote hosts; localhost may use HTTP. Certificate checks cannot be turned off.</div></div></div>
            <div className="set-row"><div style={{ flex: 1 }}><div>Prompts after a disconnect</div><div className="sub">Never resent automatically; a lost connection does not cancel a turn or approve anything.</div></div></div>
          </>
        )}
        {section === 'Agents' && (
          <>
            <h2>Agents</h2>
            <div className="sub" style={{ marginBottom: 14 }}>Agents registered on {props.current?.name ?? 'the host'}. Any agent that speaks ACP over stdio.</div>
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
            {props.hosts.map((h) => {
              const cur = h.id === props.current?.id
              return (
                <div className="set-row" key={h.id}>
                  <span className={'dot ' + (cur && props.online ? 'green' : '')} />
                  <div style={{ flex: 1, cursor: 'pointer' }} onClick={() => props.onSwitch(h)}>
                    <div>{h.name}{cur ? ' · current' : ''}</div>
                    <div className="sub mono">{h.address}{cur && props.status ? ` · acpd ${props.status.version} · ${props.online ? 'Connected' : 'Offline'}` : ''}</div>
                  </div>
                  <button className="pill" onClick={() => props.onForget(h, true)} title="Revoke this device on the host and forget it here">Unpair</button>
                </div>
              )
            })}
          </>
        )}
        {section === 'Workspaces' && <Workspaces {...props} />}
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
