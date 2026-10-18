import { describe, expect, it } from 'vitest'
import { browserPrefs, classify, DEFAULT_BROWSER_PREFS, hostAllowed, isLoopbackHost, normalizeHost } from '../electron/browserRules'

const prefs = { ...DEFAULT_BROWSER_PREFS, allowed: ['docs.example.com', '*.internal.test'] }

describe('which pages the in-app browser may show', () => {
  it('loads this computer, with or without a scheme', () => {
    expect(classify('localhost:5173', prefs)).toEqual({ kind: 'load', url: 'http://localhost:5173/' })
    expect(classify('http://127.0.0.1:8080/app?x=1', prefs)).toEqual({ kind: 'load', url: 'http://127.0.0.1:8080/app?x=1' })
    expect(classify('127.0.0.1:3000/a', prefs)).toEqual({ kind: 'load', url: 'http://127.0.0.1:3000/a' })
    expect(classify('[::1]:5173', prefs)).toEqual({ kind: 'load', url: 'http://[::1]:5173/' })
    expect(classify('app.localhost:3000', prefs)).toEqual({ kind: 'load', url: 'http://app.localhost:3000/' })
  })
  it('loads sites on the list and opens everything else in the person\'s own browser', () => {
    expect(classify('docs.example.com/guide', prefs)).toEqual({ kind: 'load', url: 'https://docs.example.com/guide' })
    expect(classify('https://a.internal.test/x', prefs)).toEqual({ kind: 'load', url: 'https://a.internal.test/x' })
    expect(classify('https://example.org/', prefs)).toEqual({ kind: 'external', url: 'https://example.org/' })
    expect(classify('https://internal.test/', prefs).kind).toBe('external')
  })
  it('refuses everything else when opening other sites is off', () => {
    const closed = { ...prefs, openOther: false }
    expect(classify('https://example.org/', closed)).toMatchObject({ kind: 'reject' })
    expect(classify('localhost:5173', closed).kind).toBe('load')
  })
  it('refuses pages that are not http or https, odd addresses and secrets in the address', () => {
    for (const bad of ['', '   ', 'javascript:alert(1)', 'file:///C:/Windows/win.ini', 'chrome://settings', 'data:text/html,<b>x</b>', 'ftp://example.com', 'hello', 'http://user:pw@localhost/', 'a'.repeat(3000)]) {
      expect(classify(bad, prefs).kind, bad.slice(0, 30)).toBe('reject')
    }
  })
  it('does not treat a look-alike host as local', () => {
    expect(isLoopbackHost('localhost.evil.com')).toBe(false)
    expect(isLoopbackHost('127.0.0.1.evil.com')).toBe(false)
    expect(hostAllowed('evil-docs.example.com', prefs)).toBe(false)
    expect(classify('http://localhost.evil.com/', prefs).kind).toBe('external')
  })
})

describe('the saved preferences', () => {
  it('fall back to the defaults when missing or malformed', () => {
    expect(browserPrefs(undefined)).toEqual(DEFAULT_BROWSER_PREFS)
    expect(browserPrefs({ allowed: 'x', remember: 'yes' })).toEqual(DEFAULT_BROWSER_PREFS)
  })
  it('keep a clean, de-duplicated list of hosts', () => {
    const p = browserPrefs({ allowed: ['HTTPS://Docs.Example.com:8443/path', 'docs.example.com', 'not a host', '*.corp.test', 5], remember: true, openOther: false, downloads: true })
    expect(p.allowed).toEqual(['docs.example.com', '*.corp.test'])
    expect(p).toMatchObject({ remember: true, openOther: false, downloads: true })
  })
  it('normalise one host', () => {
    expect(normalizeHost(' https://Example.COM:443/x ')).toBe('example.com')
    expect(normalizeHost('[::1]:5173')).toBe('[::1]')
    expect(normalizeHost('a b')).toBeUndefined()
    expect(normalizeHost('')).toBeUndefined()
  })
})
