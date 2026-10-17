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
  it('shows a link button for the pull request and opens nothing by itself', () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null)
    render(<PullRequestForm learn={false} title="Fix" body="" draft={false} base="main" head="feature" busy={false} canSuggest
      link={{ url: 'https://github.com/o/r/pull/3', existing: true }}
      onTitle={() => {}} onBody={() => {}} onDraft={() => {}} onSuggest={() => {}} onCreate={() => {}} />)
    expect(open).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'View the existing pull request' }))
    expect(open).toHaveBeenCalledWith('https://github.com/o/r/pull/3')
    open.mockRestore()
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

describe('pull request from a fork', () => {
  it('shows owner:branch in the gh line once a fork owner is typed', () => {
    const { rerender } = render(<PullRequestForm learn title="Fix" body="" draft={false} base="main" head="feature" busy={false} canSuggest
      owner="" onOwner={() => {}} onTitle={() => {}} onBody={() => {}} onDraft={() => {}} onSuggest={() => {}} onCreate={() => {}} />)
    expect(document.querySelector('.learn-cmd')?.textContent).toContain('--head "feature"')
    rerender(<PullRequestForm learn title="Fix" body="" draft={false} base="main" head="feature" busy={false} canSuggest
      owner="octo-cat" onOwner={() => {}} onTitle={() => {}} onBody={() => {}} onDraft={() => {}} onSuggest={() => {}} onCreate={() => {}} />)
    expect(document.querySelector('.learn-cmd')?.textContent).toContain('--head "octo-cat:feature"')
    expect(screen.getByLabelText('Fork owner')).toBeTruthy()
  })
})
