// What the in-app browser may show. It is for pages on this computer or the host's dev servers
// (localhost by default) and for sites the person added to a list; everything else opens in their own
// browser, or is refused. Pure functions: the window code and the settings page both use them.

export type BrowserPrefs = {
  /** Host names that may load in the in-app browser, besides localhost. `*.example.com` allows its subdomains. */
  allowed: string[]
  /** Keep cookies and sign-ins between launches. Off: every launch starts clean. */
  remember: boolean
  /** A link to a site that is not on the list opens in the person's own browser. Off: it is refused. */
  openOther: boolean
  /** Let pages download files. */
  downloads: boolean
}

export const DEFAULT_BROWSER_PREFS: BrowserPrefs = { allowed: ['localhost', '127.0.0.1'], remember: false, openOther: true, downloads: false }

/** The saved preferences, with anything missing or malformed replaced by its default. */
export function browserPrefs(saved: unknown): BrowserPrefs {
  const s = (saved && typeof saved === 'object' ? saved : {}) as Partial<BrowserPrefs>
  const allowed = Array.isArray(s.allowed) ? s.allowed.filter((x): x is string => typeof x === 'string').map(normalizeHost).filter((x): x is string => Boolean(x)) : DEFAULT_BROWSER_PREFS.allowed
  return {
    allowed: [...new Set(allowed)].slice(0, 50),
    remember: typeof s.remember === 'boolean' ? s.remember : DEFAULT_BROWSER_PREFS.remember,
    openOther: typeof s.openOther === 'boolean' ? s.openOther : DEFAULT_BROWSER_PREFS.openOther,
    downloads: typeof s.downloads === 'boolean' ? s.downloads : DEFAULT_BROWSER_PREFS.downloads
  }
}

/** A host name for the allow-list: lower case, no scheme, port or path. `*.example.com` is kept. Undefined when it is not a plausible host. */
export function normalizeHost(input: string): string | undefined {
  let h = input.trim().toLowerCase()
  h = h.replace(/^[a-z][a-z0-9+.-]*:\/\//, '').replace(/[/?#].*$/, '')
  if (h.startsWith('[')) h = h.replace(/\]:\d+$/, ']')
  else h = h.replace(/:\d+$/, '')
  return /^(\*\.)?[a-z0-9]([a-z0-9.-]*[a-z0-9])?$/.test(h) || /^\[[0-9a-f:]+\]$/.test(h) ? h : undefined
}

/** Loopback: this computer only. */
export function isLoopbackHost(host: string): boolean {
  const h = host.toLowerCase()
  return h === 'localhost' || h.endsWith('.localhost') || h === '[::1]' || h === '::1' || /^127(\.\d{1,3}){3}$/.test(h)
}

export function hostAllowed(host: string, prefs: BrowserPrefs): boolean {
  const h = host.toLowerCase()
  if (isLoopbackHost(h)) return true
  return prefs.allowed.some((a) => (a.startsWith('*.') ? h.endsWith(a.slice(1)) && h.length > a.length - 1 : a === h))
}

export type Target =
  | { kind: 'load'; url: string }
  | { kind: 'external'; url: string }
  | { kind: 'reject'; reason: string }

/** Only http and https ever leave through the system browser. */
export function isWebUrl(url: string): boolean {
  try { const u = new URL(url); return u.protocol === 'http:' || u.protocol === 'https:' } catch { return false }
}

/**
 * What to do with an address the person typed, or a link or redirect a page followed. An address with
 * no scheme gets http for this computer and https for anything else; there is no web search.
 */
export function classify(input: string, prefs: BrowserPrefs): Target {
  const text = input.trim()
  if (!text) return { kind: 'reject', reason: 'Type an address.' }
  if (text.length > 2048) return { kind: 'reject', reason: 'That address is too long.' }
  let candidate = text
  if (!/^[a-z][a-z0-9+.-]*:/i.test(text) || /^[a-z0-9.-]+:\d+(\/|$)/i.test(text) || /^\[[0-9a-f:]+\](:\d+)?(\/|$)/i.test(text)) {
    const head = text.replace(/[/?#].*$/, '')
    const host = normalizeHost(head)
    if (!host || (!host.includes('.') && !isLoopbackHost(host) && !host.startsWith('['))) return { kind: 'reject', reason: 'That is not an address this browser can open.' }
    candidate = (isLoopbackHost(host) ? 'http://' : 'https://') + text
  }
  let url: URL
  try { url = new URL(candidate) } catch { return { kind: 'reject', reason: 'That is not a valid address.' } }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return { kind: 'reject', reason: 'Only http and https pages can be shown.' }
  if (url.username || url.password) return { kind: 'reject', reason: 'Addresses with a user name or password are not opened.' }
  if (hostAllowed(url.hostname, prefs)) return { kind: 'load', url: url.toString() }
  return prefs.openOther
    ? { kind: 'external', url: url.toString() }
    : { kind: 'reject', reason: `${url.hostname} is not on your list of allowed sites.` }
}
