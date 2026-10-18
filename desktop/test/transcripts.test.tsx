import { describe, expect, it } from 'vitest'
import { emptyTimeline } from '../src/acp'
import { keep, recall, MAX_SESSIONS } from '../src/transcripts'

const tl = (text: string) => ({ ...emptyTimeline(), busy: true, title: 'T', items: [{ kind: 'user' as const, id: 'u', text }, { kind: 'permission' as const, id: 1, title: 'Write', source: 'agent', lines: [], options: [], diffs: [] }] })

describe('saved conversations', () => {
  it('come back readable, with nothing still running or waiting', () => {
    keep('h', 's', tl('hello'))
    const back = recall('h', 's')!
    expect(back.items[0]).toMatchObject({ text: 'hello' })
    expect(back.title).toBe('T')
    expect(back.busy).toBe(false)
    expect(back.items[1]).toMatchObject({ answered: 'ended' })
  })
  it('are absent for sessions never saved, and empty ones are not saved', () => {
    expect(recall('h', 'nope')).toBeUndefined()
    keep('h', 'e', emptyTimeline())
    expect(recall('h', 'e')).toBeUndefined()
  })
  it('keep only the most recent sessions', () => {
    for (let i = 0; i < MAX_SESSIONS + 3; i++) keep('h', 'x' + i, tl('m' + i))
    expect(recall('h', 'x0')).toBeUndefined()
    expect(recall('h', 'x' + (MAX_SESSIONS + 2))).toBeDefined()
  })
})
