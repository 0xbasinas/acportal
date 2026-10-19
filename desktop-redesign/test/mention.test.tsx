import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { Composer, dropMention, mentionAt, type Attachment, type FileHit } from '../src/Session'

describe('mentionAt', () => {
  it('reads the token at the cursor and ignores an email address', () => {
    expect(mentionAt('see @src/lib', 'see @src/lib'.length)).toEqual({ start: 4, query: 'src/lib' })
    expect(mentionAt('@', 1)).toEqual({ start: 0, query: '' })
    expect(mentionAt('see @"my file', 'see @"my file'.length)).toEqual({ start: 4, query: 'my file' })
    expect(mentionAt('user@host', 'user@host'.length)).toBeUndefined()
    expect(mentionAt('see @src and more', 8)).toEqual({ start: 4, query: 'src' })
    expect(mentionAt('see @src and more', 14)).toBeUndefined()
  })

  it('removes the token and leaves the surrounding words', () => {
    expect(dropMention('@HANDOFF.md', 0, '@HANDOFF.md'.length)).toEqual({ text: '', caret: 0 })
    expect(dropMention('see @lib', 4, 'see @lib'.length)).toEqual({ text: 'see ', caret: 4 })
    expect(dropMention('see @lib now', 4, 'see @lib'.length)).toEqual({ text: 'see now', caret: 4 })
  })
})

describe('tagging a file from the message box', () => {
  const hit: FileHit = { name: 'lib.rs', path: 'C:/ws/src/lib.rs', relative: 'src/lib.rs' }
  const file: Attachment = { name: 'lib.rs', mimeType: 'text/plain', size: 5, text: 'hello', path: hit.path }

  async function typeAt(value: string): Promise<HTMLTextAreaElement> {
    const box = screen.getByRole('textbox') as HTMLTextAreaElement
    const set = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value')!.set!
    set.call(box, value)
    const tracked = box as HTMLTextAreaElement & { _valueTracker?: { setValue: (v: string) => void } }
    tracked._valueTracker?.setValue('')
    box.selectionStart = value.length
    box.selectionEnd = value.length
    fireEvent.input(box)
    return box
  }

  it('removes the @ token, attaches the file, and does not send on Enter', async () => {
    const findFiles = vi.fn(async () => [hit])
    const onAttachFile = vi.fn(async () => ({ attachment: file }))
    const onSend = vi.fn()
    render(<Composer agent="Goose" busy={false} onSend={onSend} onCancel={() => {}} history={[]} canImage canResource findFiles={findFiles} onAttachFile={onAttachFile} />)
    const box = await typeAt('see @lib')
    expect(await screen.findByRole('option', { name: /lib\.rs/ })).toBeTruthy()
    expect(findFiles).toHaveBeenCalledWith('lib')
    fireEvent.keyDown(box, { key: 'Enter' })
    expect(onSend).not.toHaveBeenCalled()
    await waitFor(() => expect(onAttachFile).toHaveBeenCalledWith(hit.path))
    expect(box.value).toBe('see ')
    expect(await screen.findByRole('button', { name: 'Remove lib.rs' })).toBeTruthy()
    expect(screen.queryByRole('listbox')).toBeNull()
  })

  it('leaves an email address alone', async () => {
    const findFiles = vi.fn(async () => [hit])
    render(<Composer agent="Goose" busy={false} onSend={() => {}} onCancel={() => {}} history={[]} canImage canResource findFiles={findFiles} />)
    await typeAt('user@host')
    await new Promise((r) => setTimeout(r, 200))
    expect(findFiles).not.toHaveBeenCalled()
    expect(screen.queryByRole('listbox')).toBeNull()
  })
})
