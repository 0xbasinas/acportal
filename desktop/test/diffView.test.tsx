import { describe, expect, it } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { DiffView } from '../src/ui'

const block = { path: 'README.md', oldText: 'one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten', newText: 'one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nTEN' }

describe('a diff card folds', () => {
  it('shows its lines, and folds to just the path and the counts when the header is pressed', () => {
    render(<DiffView d={block} maxLines={3} onMore={() => {}} />)
    const head = screen.getByRole('button', { name: /Collapse README.md/ })
    expect(head.getAttribute('aria-expanded')).toBe('true')
    expect(document.querySelector('.diff-body')).not.toBeNull()
    expect(screen.queryByText(/more lines/)).not.toBeNull()

    fireEvent.click(head)
    const folded = screen.getByRole('button', { name: /Expand README.md/ })
    expect(folded.getAttribute('aria-expanded')).toBe('false')
    expect(document.querySelector('.diff-body')).toBeNull()
    expect(screen.queryByText(/more lines/)).toBeNull()
    expect(document.querySelector('.diff')?.className).toContain('collapsed')
    // the path and the +/- counts stay visible
    expect(screen.getByText('README.md')).toBeTruthy()
    expect(document.querySelector('.diff-stat')?.textContent).toContain('+1')

    fireEvent.click(folded)
    expect(document.querySelector('.diff-body')).not.toBeNull()
  })

  it('folds from the keyboard and can start folded', () => {
    render(<DiffView d={block} />)
    const head = screen.getByRole('button', { name: /Collapse README.md/ })
    fireEvent.keyDown(head, { key: 'Enter' })
    expect(document.querySelector('.diff-body')).toBeNull()
    fireEvent.keyDown(screen.getByRole('button', { name: /Expand README.md/ }), { key: ' ' })
    expect(document.querySelector('.diff-body')).not.toBeNull()
  })

  it('starts folded when asked', () => {
    render(<DiffView d={block} defaultOpen={false} />)
    expect(document.querySelector('.diff-body')).toBeNull()
    expect(screen.getByRole('button', { name: /Expand README.md/ })).toBeTruthy()
  })
})
