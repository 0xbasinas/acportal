import { useEffect, useState, type ReactNode } from 'react'
import { browserPrefs, DEFAULT_BROWSER_PREFS, normalizeHost, type BrowserPrefs } from '../electron/browserRules'
import { I } from './ui'

/** Settings → Browser: which sites the in-app browser may show, and what it keeps. Saved with the other preferences. */
export function BrowserSettings() {
  const [p, setP] = useState<BrowserPrefs>(DEFAULT_BROWSER_PREFS)
  const [site, setSite] = useState('')
  const [problem, setProblem] = useState<string>()
  useEffect(() => { void window.api.prefs.get().then((r) => { if (r.ok) setP(browserPrefs(r.value.browser)) }) }, [])
  const save = (next: BrowserPrefs): void => {
    setP(next)
    void window.api.prefs.get().then((r) => window.api.prefs.set({ ...(r.ok ? r.value : {}), browser: next }))
  }
  const add = (): void => {
    const host = normalizeHost(site)
    if (!host) { setProblem('Enter a site name such as docs.example.com, or *.example.com for all its subdomains.'); return }
    setProblem(undefined)
    if (!p.allowed.includes(host)) save({ ...p, allowed: [...p.allowed, host].slice(0, 50) })
    setSite('')
  }
  const row = (label: string, hint: string, on: boolean, change: (v: boolean) => void): ReactNode => (
    <div className="set-row">
      <div style={{ flex: 1 }}><div>{label}</div><div className="sub">{hint}</div></div>
      <span role="switch" aria-checked={on} aria-label={label} tabIndex={0} className={'toggle' + (on ? ' on' : '')} style={{ cursor: 'pointer' }} onClick={() => change(!on)} onKeyDown={(e) => { if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); change(!on) } }} />
    </div>
  )
  return (
    <>
      <h2>Browser</h2>
      <div className="sub">A page on this computer or a dev server on the host opens in the app. The page runs apart from the app and cannot reach it.</div>
      <h3 className="set-h">Allowed sites</h3>
      <div className="sub" style={{ marginBottom: 8 }}>localhost and 127.0.0.1 are always allowed. Links to other sites {p.openOther ? 'open in your own browser' : 'are refused'}.</div>
      <div className="chips" aria-label="Allowed sites">
        {p.allowed.length === 0 && <span className="sub">None added.</span>}
        {p.allowed.map((a) => (
          <span key={a} className="chip mono">{a}<button aria-label={`Remove ${a}`} onClick={() => save({ ...p, allowed: p.allowed.filter((x) => x !== a) })}>{I.close}</button></span>
        ))}
      </div>
      <form className="set-add" onSubmit={(e) => { e.preventDefault(); add() }}>
        <input className="input mono" value={site} placeholder="docs.example.com" aria-label="Add a site" onChange={(e) => setSite(e.target.value)} />
        <button className="btn" type="submit" disabled={!site.trim()}>{I.plus}Add</button>
      </form>
      {problem && <div className="error">{problem}</div>}
      {row('Open other links in my browser', 'Links to sites that are not on the list. Off: they are refused.', p.openOther, (v) => save({ ...p, openOther: v }))}
      {row('Remember sign-ins', 'Keeps cookies between launches. Off: every launch starts clean. Applies the next time the browser opens.', p.remember, (v) => save({ ...p, remember: v }))}
      {row('Downloads', 'Let pages download files. Off: downloads are blocked.', p.downloads, (v) => save({ ...p, downloads: v }))}
    </>
  )
}
