import { describe, expect, it } from 'vitest'
import { classifyPicked } from '../electron/attach'
import { promptBlock, type Attachment } from '../src/Session'

describe('files picked for a message', () => {
  it('keeps a picture, a pdf, and a text file as different attachments', () => {
    const png = Uint8Array.from([137, 80, 78, 71, 13, 10, 26, 10])
    const picture = classifyPicked('dot.png', png)
    expect(picture.mimeType).toBe('image/png')
    expect(picture.data?.startsWith('iVBOR')).toBe(true)
    expect(picture.blob).toBeUndefined()

    const pdf = classifyPicked('notes.pdf', new TextEncoder().encode('%PDF-1.4\n'))
    expect(pdf.mimeType).toBe('application/pdf')
    expect(pdf.blob?.startsWith('JVBER')).toBe(true)
    expect(pdf.data).toBeUndefined()
    expect(pdf.text).toBeUndefined()

    const text = classifyPicked('note.txt', new TextEncoder().encode('hello\n'))
    expect(text).toEqual({ mimeType: 'text/plain', text: 'hello\n' })

    const binary = classifyPicked('blob.bin', Uint8Array.from([0, 1, 2]))
    expect(binary.mimeType).toBe('application/octet-stream')
    expect(binary.blob).toBe('AAEC')
  })

  it('sends an image as an image and a pdf as a file resource', () => {
    const image: Attachment = { name: 'dot.png', mimeType: 'image/png', size: 8, data: 'iVBOR', path: 'C:/dot.png' }
    expect(promptBlock(image)).toEqual({ type: 'image', mimeType: 'image/png', data: 'iVBOR' })
    const pdf: Attachment = { name: 'notes.pdf', mimeType: 'application/pdf', size: 9, blob: 'JVBER', path: 'C:/notes.pdf' }
    expect(promptBlock(pdf)).toEqual({ type: 'resource', resource: { uri: 'file://C:/notes.pdf', mimeType: 'application/pdf', blob: 'JVBER' } })
    const text: Attachment = { name: 'note.txt', mimeType: 'text/plain', size: 6, text: 'hello\n', path: 'C:/note.txt' }
    expect(promptBlock(text)).toEqual({ type: 'resource', resource: { uri: 'file://C:/note.txt', mimeType: 'text/plain', text: 'hello\n' } })
  })
})
