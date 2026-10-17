import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { PullRequestForm } from '../src/Git'
import { prBase, prBlockedReason, prCommand } from '../src/pr'

describe('pull request command shown in the Git view', () => {
  it('keeps a tricky title as one quoted argument and puts the body nowhere', () => {
    const cmd = prCommand({ title: 'Fix $(whoami); "x"', base: 'main', head: 'feature$(whoami)', draft: true })
    expect(cmd).toBe('gh pr create --title "Fix $(whoami); \\"x\\"" --body-file - --base "main" --head "feature$(whoami)" --draft')
    expect(cmd).not.toContain('BODY')
    expect(prCommand({ title: 'Fix', base: 'main', head: 'feature', draft: false })).not.toContain('--draft')
  })
  it('says why the button is off', () => {
    expect(prBlockedReason(false, 'found')).toMatch(/newer acpd/)
    expect(prBlockedReason(true, 'missing')).toMatch(/not found/)
    expect(prBlockedReason(true, undefined)).toMatch(/not found/)
    expect(prBlockedReason(true, 'found')).toBeUndefined()
  })
  it('strips the origin prefix from the base', () => {
    expect(prBase('origin/main')).toBe('main')
    expect(prBase('feature/x')).toBe('feature/x')
  })
  it('renders the command and keeps the description out of it', () => {
    const onCreate = vi.fn()
    render(<PullRequestForm learn title={'Fix $(whoami); "x"'} body="BODY-MARKER-9f3" draft base="main" head="feature$(whoami)" busy={false} canSuggest
      onTitle={() => {}} onBody={() => {}} onDraft={() => {}} onSuggest={() => {}} onCreate={onCreate} />)
    const shown = document.querySelector('.learn-cmd')?.textContent ?? ''
    expect(shown).toContain('gh pr create --title')
    expect(shown).toContain('feature$(whoami)')
    expect(shown).not.toContain('BODY-MARKER-9f3')
    fireEvent.click(screen.getByRole('button', { name: 'Open pull request' }))
    expect(onCreate).toHaveBeenCalledOnce()
  })
})
