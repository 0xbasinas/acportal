import { useState } from 'react'
import type { Host } from './types'
import { I } from './ui'
import { Logo } from './TitleBar'

export function Pairing({ onPaired, onCancel, initialAddress, repairName, replaceId }: { replaceId?: string; onPaired: (h: Host) => void; onCancel?: () => void; initialAddress?: string; repairName?: string }) {
  const [address, setAddress] = useState(initialAddress ?? '')
  const [code, setCode] = useState('')
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()

  async function pasteLink(v: string): Promise<void> {
    const r = await window.api.hosts.parseLink(v)
    if (r.ok && r.value.address) {
      setAddress(r.value.address)
      if (r.value.code) setCode(r.value.code)
    } else setAddress(v)
  }
  async function pair(): Promise<void> {
    setBusy(true)
    setError(undefined)
    const r = await window.api.hosts.pair(address, code, name || undefined, replaceId)
    setBusy(false)
    if (r.ok) onPaired(r.value as Host)
    else setError(r.code === 'invalid_pairing_code' || r.status === 401 ? 'That code was not accepted. Codes are single-use and expire; run acpd pair again.' : r.error)
  }
  return (
    <div className="pair-wrap">
      <div className="pair-left">
        <div className="pair-logo"><Logo size={44} /></div>
        <h1 className="pair-title">{repairName ? `Pair ${repairName} again` : 'Pair with a host'}</h1>
        <p className="sub2" style={{ marginTop: 8 }}>
          acportal desktop drives the agents on a machine running <span className="mono">acpd</span>, the same way the Android app does.
        </p>
        <div className="field">
          <label>Host address</label>
          <input className="input mono" placeholder="https://studio.tailnet.ts.net:8765" value={address} onChange={(e) => void pasteLink(e.target.value)} autoFocus />
        </div>
        <div className="field">
          <label>Pairing code</label>
          <input className="input mono" placeholder="from acpd pair" value={code} onChange={(e) => setCode(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && address && code && void pair()} />
        </div>
        <div className="field">
          <label>This device's name (shown on the host)</label>
          <input className="input" placeholder="acportal desktop" value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        {error && <div className="error">{error}</div>}
        <div style={{ display: 'flex', gap: 10, marginTop: 22 }}>
          <button className="btn-wide" disabled={!address || !code || busy} onClick={() => void pair()}>
            {busy ? 'Pairing…' : 'Pair'}
          </button>
          {onCancel && <button className="btn-wide ghost" style={{ width: 140 }} onClick={onCancel}>Cancel</button>}
        </div>
        <p className="sub pair-note">{I.shield}<span>Remote hosts need HTTPS with a certificate your system trusts. Plain HTTP is only allowed for this computer (localhost).</span></p>
      </div>
      <div className="pair-right">
        <div className="pair-card"><div className="pair-card-head">{I.server}<span>On the host</span></div>
        {[
          ['Start the host', 'acpd --config config.toml start'],
          ['Create a one-use code', 'acpd --config config.toml pair'],
          ['Enter the address and code here', 'The token is kept in this computer’s secure storage, never shown.']
        ].map(([t, d], i) => (
          <div className="step" key={i}>
            <div className="n">{i + 1}</div>
            <div>
              <div>{t}</div>
              {i < 2 ? <div className="cmd-block mono">$ {d}</div> : <div className="sub2">{d}</div>}
            </div>
          </div>
        ))}
        </div>
        <div className="sub" style={{ marginTop: 4 }}>You can also paste an <span className="mono">acpd://pair?address=…&amp;code=…</span> link into the address field.</div>
      </div>
    </div>
  )
}
