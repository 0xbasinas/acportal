import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { TerminalSettings } from '../src/TerminalSettings'

afterEach(() => cleanup())

const off = { terminal: { enabled: false, setHere: false }, preview: { enabled: false, ports: [5173], setHere: false } }

function host(initial = off) {
  let now = initial
  const api = vi.fn(async (method: string, _path: string, body?: Record<string, unknown>) => {
    if (method === 'PUT' && body) {
      now = {
        terminal: { enabled: 'terminal' in body ? body.terminal === true : now.terminal.enabled, setHere: 'terminal' in body ? body.terminal !== null : now.terminal.setHere },
        preview: { enabled: 'previewEnabled' in body ? body.previewEnabled === true : now.preview.enabled, ports: (body.previewPorts as number[] | undefined) ?? now.preview.ports, setHere: now.preview.setHere }
      }
    }
    return { ok: true as const, value: now }
  })
  const onChanged = vi.fn()
  render(<TerminalSettings api={api as never} hostName="Studio" online onChanged={onChanged} />)
  return { api, onChanged }
}

describe('Settings → Terminal', () => {
  it('asks before turning terminals on, and says what it allows', async () => {
    const { api, onChanged } = host()
    const toggle = await screen.findByRole('switch', { name: 'Allow terminals on this host' })
    expect(toggle.getAttribute('aria-checked')).toBe('false')
    fireEvent.click(toggle)
    expect(screen.getByRole('alertdialog').textContent).toMatch(/not sandboxed/)
    expect(api).not.toHaveBeenCalledWith('PUT', expect.anything(), expect.anything())
    fireEvent.click(screen.getByRole('button', { name: 'Turn on terminals' }))
    await waitFor(() => expect(api).toHaveBeenCalledWith('PUT', '/v1/settings/features', { terminal: true }))
    await waitFor(() => expect(screen.getByRole('switch', { name: 'Allow terminals on this host' }).getAttribute('aria-checked')).toBe('true'))
    expect(onChanged).toHaveBeenCalled()
  })

  it('turning off needs no question, and cancelling the question changes nothing', async () => {
    const { api } = host({ ...off, terminal: { enabled: true, setHere: true } })
    fireEvent.click(await screen.findByRole('switch', { name: 'Allow terminals on this host' }))
    await waitFor(() => expect(api).toHaveBeenCalledWith('PUT', '/v1/settings/features', { terminal: false }))
    expect(screen.queryByRole('alertdialog')).toBeNull()
    cleanup()
    const second = host()
    fireEvent.click(await screen.findByRole('switch', { name: 'Allow terminals on this host' }))
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(second.api).not.toHaveBeenCalledWith('PUT', expect.anything(), expect.anything())
  })

  it('manages the preview ports', async () => {
    const { api } = host()
    fireEvent.click(await screen.findByRole('switch', { name: 'Allow preview' }))
    await waitFor(() => expect(api).toHaveBeenCalledWith('PUT', '/v1/settings/features', { previewEnabled: true }))
    fireEvent.change(screen.getByLabelText('Add a port'), { target: { value: '3000' } })
    fireEvent.click(screen.getByRole('button', { name: /Add$/ }))
    await waitFor(() => expect(api).toHaveBeenCalledWith('PUT', '/v1/settings/features', { previewPorts: [5173, 3000] }))
    fireEvent.click(await screen.findByRole('button', { name: 'Remove port 5173' }))
    await waitFor(() => expect(api).toHaveBeenCalledWith('PUT', '/v1/settings/features', { previewPorts: [3000] }))
  })

  it('refuses a bad port without asking the host', async () => {
    const { api } = host()
    fireEvent.change(await screen.findByLabelText('Add a port'), { target: { value: '99999' } })
    fireEvent.click(screen.getByRole('button', { name: /Add$/ }))
    expect(screen.getByRole('alert').textContent).toMatch(/between 1 and 65535/)
    expect(api).not.toHaveBeenCalledWith('PUT', expect.anything(), expect.anything())
  })

  it('says so when the host is too old to change this', async () => {
    const api = vi.fn(async () => ({ ok: false as const, error: 'Host answered 404', status: 404 }))
    render(<TerminalSettings api={api as never} online onChanged={() => {}} />)
    expect((await screen.findByRole('alert')).textContent).toMatch(/older than the app/)
  })
})
