import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { Markdown } from '../src/Markdown'

describe('agent reply Markdown', () => {
  it('renders GitHub-style Markdown: headings, nested lists, tasks, tables, quotes, strikethrough', () => {
    const { container } = render(<Markdown text={[
      '## Plan', '', '- one', '  - nested', '- [x] done', '- [ ] todo', '', '| a | b |', '|---|---|', '| 1 | 2 |', '', '> quoted', '', '~~old~~ and **bold**'
    ].join('\n')} />)
    expect(container.querySelector('h2')?.textContent).toBe('Plan')
    expect(container.querySelector('ul ul li')?.textContent).toBe('nested')
    expect(container.querySelectorAll('input[type="checkbox"]')).toHaveLength(2)
    expect(container.querySelector('.md-table td')?.textContent).toBe('1')
    expect(container.querySelector('blockquote')?.textContent?.trim()).toBe('quoted')
    expect(container.querySelector('del')?.textContent).toBe('old')
  })
  it('highlights code blocks with a language label and a Copy button', () => {
    const { container } = render(<Markdown text={'```rust\nfn main() { let x = 1; }\n```\n\n```\nplain\n```'} />)
    const bars = container.querySelectorAll('.md-code-bar')
    expect(bars[0].textContent).toMatch(/rust.*Copy/)
    expect(bars[1].textContent).toMatch(/text/)
    expect(container.querySelector('.md-code .hljs-keyword')?.textContent).toBe('fn')
  })
  it('never interprets HTML, never loads images, and opens links only after confirming', () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null)
    const ask = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true)
    const { container } = render(<Markdown text={'<img src="https://x.test/a.png"> <b>raw</b>\n\n![chart](https://x.test/c.png)\n\n[docs](https://example.com) [bad](javascript:alert(1))'} />)
    expect(container.querySelector('img')).toBeNull()
    expect(container.querySelector('b')).toBeNull()
    expect(screen.getByText(/chart/).tagName).toBe('A')
    fireEvent.click(screen.getByText('docs'))
    expect(open).not.toHaveBeenCalled()
    fireEvent.click(screen.getByText('docs'))
    expect(open).toHaveBeenCalledWith('https://example.com')
    fireEvent.click(screen.getByText('bad'))
    expect(ask).toHaveBeenCalledTimes(2)
    open.mockRestore(); ask.mockRestore()
  })
})
