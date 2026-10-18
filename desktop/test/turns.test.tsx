import { describe, expect, it } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { Conversation, fold } from '../src/Session'
import { emptyTimeline, Item } from '../src/acp'
import { briefDuration, duration, turnEnded, turnStarted, workedMs } from '../src/turns'

const tool = (id: string): Item => ({ kind: 'tool', id, title: 'read ' + id, status: 'completed', diffs: [], host: false })
const turn = (n: number): Item[] => [
  { kind: 'user', id: 'u' + n, text: 'question ' + n },
  { kind: 'thought', id: 'th' + n, text: 'thinking ' + n },
  tool('t' + n),
  { kind: 'agent', id: 'a' + n, text: 'final answer ' + n }
]
const noop = () => {}
const show = (items: Item[], busy = false, key?: string) => render(<Conversation t={{ ...emptyTimeline(), items, busy }} agent="Goose" onAnswer={noop} onReview={noop} onElicit={noop} turnKey={key} />)

describe('finished turns fold their steps', () => {
  it('hide thoughts and tool calls once the final reply is there, and open on click', () => {
    show([...turn(1)])
    expect(screen.getByText('final answer 1')).toBeTruthy()
    expect(screen.queryByText('thinking 1')).toBeNull()
    expect(screen.queryByText('read t1')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /Worked · 1 step/ }))
    expect(screen.getByText('read t1')).toBeTruthy()
  })
  it('keep the running turn open', () => {
    show([...turn(1).slice(0, 3)], true)
    expect(screen.getByText('read t1')).toBeTruthy()
  })
  it('never fold a request that still waits for an answer', () => {
    const items = turn(1)
    items.splice(2, 0, { kind: 'permission', id: 7, title: 'Write', source: 'agent', lines: [], options: [], diffs: [] })
    expect(fold(items, false).some((b) => b.kind === 'steps')).toBe(false)
  })
  it('show the time a turn took when this app timed it', () => {
    turnStarted('h/s', 'u1', 1000); turnEnded('h/s', 'u1', 73_000)
    show([...turn(1)], false, 'h/s')
    expect(screen.getByRole('button', { name: /Worked for 1m 12s · 1 step/ })).toBeTruthy()
    expect(workedMs('h/s')).toBe(72_000)
    turnStarted('h/s', 'u1', 5); turnEnded('h/s', 'u1', 999_999)
    expect(workedMs('h/s')).toBe(72_000)
  })
})

describe('durations', () => {
  it('read short', () => {
    expect(duration(45_000)).toBe('45s')
    expect(duration(185_000)).toBe('3m 05s')
    expect(duration(4_320_000)).toBe('1h 12m')
    expect(briefDuration(185_000)).toBe('3m')
  })
})

describe('opening a conversation', () => {
  const user: Item = { kind: 'user', id: 'u9', text: 'saved question' }
  it('shows the saved copy, not an empty page, until the host has sent it', () => {
    render(<Conversation t={{ ...emptyTimeline(), loading: true, preview: [user] }} agent="Goose" onAnswer={noop} onReview={noop} onElicit={noop} />)
    expect(screen.getByText('saved question')).toBeTruthy()
    expect(screen.queryByText('Start the conversation')).toBeNull()
  })
  it('says it is loading when there is no copy, and starts fresh once loaded', () => {
    const { unmount } = render(<Conversation t={{ ...emptyTimeline(), loading: true }} agent="Goose" onAnswer={noop} onReview={noop} onElicit={noop} />)
    expect(screen.getByText('Loading the conversation…')).toBeTruthy()
    unmount()
    render(<Conversation t={emptyTimeline()} agent="Goose" onAnswer={noop} onReview={noop} onElicit={noop} />)
    expect(screen.getByText('Start the conversation')).toBeTruthy()
  })
})
