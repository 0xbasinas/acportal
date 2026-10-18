import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { I } from './ui'

type Api = <T>(method: string, path: string, body?: unknown) => Promise<{ ok: true; value: T } | { ok: false; error: string; code?: string; status?: number }>
type Features = { terminal: { enabled: boolean; setHere: boolean }; preview: { enabled: boolean; ports: number[]; setHere: boolean } }

/**
 * Settings → Terminal: turns terminals and the phone's dev-server preview on or off on the host that
 * is open. A terminal is a shell with the host user's rights, so turning it on asks first; the host
 * applies the change at once and keeps it with the folders and agents added from an app.
 */
export function TerminalSettings(props: { api: Api; hostName?: string; online: boolean; onChanged: () => void }) {
  const [f, setF] = useState<Features>()
  const [problem, setProblem] = useState<string>()
  const [busy, setBusy] = useState(false)
  const [port, setPort] = useState('')
  const [confirming, setConfirming] = useState(false)
  const { api } = props

  const load = useCallback(async () => {
    setProblem(undefined)
    const r = await api<Features>('GET', '/v1/settings/features')
    if (r.ok) setF(r.value)
    else setProblem(r.status === 404 ? 'This host is older than the app and cannot change this from here. Update and restart acpd, or edit its config.toml.' : r.error)
  }, [api])
  useEffect(() => { setF(undefined); if (props.online) void load() }, [props.online, load, props.hostName])

  const change = async (body: Record<string, unknown>): Promise<void> => {
    setBusy(true); setProblem(undefined)
    const r = await api<Features>('PUT', '/v1/settings/features', body)
    setBusy(false)
    if (r.ok) { setF(r.value); props.onChanged() } else setProblem(r.error)
  }
  const addPort = (): void => {
    const n = Number(port.trim())
    if (!Number.isInteger(n) || n < 1 || n > 65535) { setProblem('Enter a port between 1 and 65535, such as 5173.'); return }
    setPort('')
    if (f && !f.preview.ports.includes(n)) void change({ previewPorts: [...f.preview.ports, n] })
  }
  const row = (label: string, hint: ReactNode, on: boolean, set: (v: boolean) => void, disabled = false): ReactNode => (
    <div className="set-row">
      <div style={{ flex: 1 }}><div>{label}</div><div className="sub">{hint}</div></div>
      <span role="switch" aria-checked={on} aria-label={label} aria-disabled={disabled} tabIndex={0} className={'toggle' + (on ? ' on' : '')} style={{ cursor: disabled ? 'default' : 'pointer', opacity: disabled ? 0.5 : 1 }}
        onClick={() => { if (!disabled) set(!on) }} onKeyDown={(e) => { if (!disabled && (e.key === ' ' || e.key === 'Enter')) { e.preventDefault(); set(!on) } }} />
    </div>
  )

  return (
    <>
      <h2>Terminal</h2>
      <div className="sub">Real shells on {props.hostName ?? 'the host'}, opened from the desktop app or a phone. They are off until you turn them on.</div>
      {!props.online && <div className="error" role="status">The host is offline. Connect to it to change this.</div>}
      {problem && <div className="error" role="alert">{problem}</div>}
      {f && (
        <>
          {row('Allow terminals on this host', f.terminal.enabled ? 'On: a paired device can open a shell in an allowed folder.' : 'Off: no one can open a shell.', f.terminal.enabled, (v) => { if (v) setConfirming(true); else void change({ terminal: false }) }, busy)}
          {confirming && (
            <div className="term-confirm" role="alertdialog" aria-label="Turn on terminals">
              <b>Turn on terminals?</b>
              <p>A terminal is a real shell that runs with {props.hostName ? `${props.hostName}'s` : 'the host'} user's rights and is not sandboxed. Anyone with a paired device on this host will be able to run commands in the folders it allows, and agents cannot see these terminals. Turn it off again here at any time.</p>
              <div className="term-confirm-actions">
                <button className="btn primary" disabled={busy} onClick={() => { setConfirming(false); void change({ terminal: true }) }}>Turn on terminals</button>
                <button className="btn" onClick={() => setConfirming(false)}>Cancel</button>
              </div>
            </div>
          )}
          {f.terminal.setHere && <div className="sub" style={{ marginTop: 6 }}>Set from an app. <button className="linkish" disabled={busy} onClick={() => void change({ terminal: null })}>Use the host's config.toml instead</button></div>}

          <h3 className="set-h">Preview on the phone</h3>
          <div className="sub" style={{ marginBottom: 8 }}>Lets a paired phone show a dev server running on this computer, through the host. Only the ports below, only pages (no form posts), and whatever those servers show is visible to every paired phone.</div>
          {row('Allow preview', f.preview.enabled ? 'On for the ports below.' : 'Off.', f.preview.enabled, (v) => void change({ previewEnabled: v }), busy)}
          <div className="chips" aria-label="Preview ports">
            {f.preview.ports.length === 0 && <span className="sub">No ports.</span>}
            {f.preview.ports.map((p) => (
              <span key={p} className="chip mono">{p}<button aria-label={`Remove port ${p}`} disabled={busy} onClick={() => void change({ previewPorts: f.preview.ports.filter((x) => x !== p) })}>{I.close}</button></span>
            ))}
          </div>
          <form className="set-add" onSubmit={(e) => { e.preventDefault(); addPort() }}>
            <input className="input mono" value={port} inputMode="numeric" placeholder="5173" aria-label="Add a port" onChange={(e) => setPort(e.target.value)} />
            <button className="btn" type="submit" disabled={!port.trim() || busy}>{I.plus}Add</button>
          </form>
          {f.preview.setHere && <div className="sub" style={{ marginTop: 6 }}>Set from an app. <button className="linkish" disabled={busy} onClick={() => void change({ previewEnabled: null, previewPorts: null })}>Use the host's config.toml instead</button></div>}
        </>
      )}
    </>
  )
}
