// Fallback token storage when no OS keychain exists (e.g. Linux without a Secret Service).
// AES-256-GCM; the 32-byte key lives in userData/token.key with 0600 permissions.
// This protects against casual disclosure (backups of hosts.json, screenshots of files) but not
// against someone who can read this user's files. Settings > Hosts labels it as weaker storage.
import { randomBytes, createCipheriv, createDecipheriv } from 'node:crypto'
import { readFileSync, writeFileSync, mkdirSync, chmodSync, existsSync } from 'node:fs'
import { join } from 'node:path'

function key(dir: string): Buffer {
  const f = join(dir, 'token.key')
  if (existsSync(f)) {
    const k = readFileSync(f)
    if (k.length === 32) {
      if (process.platform !== 'win32') chmodSync(f, 0o600)
      return k
    }
  }
  mkdirSync(dir, { recursive: true })
  const k = randomBytes(32)
  writeFileSync(f, k, { mode: 0o600, flag: 'w' })
  return k
}
export function sealWithFileKey(token: string, dir: string): string {
  const iv = randomBytes(12)
  const c = createCipheriv('aes-256-gcm', key(dir), iv)
  const body = Buffer.concat([c.update(token, 'utf8'), c.final()])
  return Buffer.concat([iv, c.getAuthTag(), body]).toString('base64')
}
export function openWithFileKey(sealed: string, dir: string): string {
  const raw = Buffer.from(sealed, 'base64')
  const d = createDecipheriv('aes-256-gcm', key(dir), raw.subarray(0, 12))
  d.setAuthTag(raw.subarray(12, 28))
  return Buffer.concat([d.update(raw.subarray(28)), d.final()]).toString('utf8')
}
