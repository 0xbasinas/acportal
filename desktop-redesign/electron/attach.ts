import { extname } from 'node:path'

/** Raw bytes the composer will accept. Base64 of this still fits the host's 1 MiB frame. */
export const PICK_LIMIT = 512 * 1024

const IMAGES: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
}

const FILES: Record<string, string> = {
  '.pdf': 'application/pdf',
  '.zip': 'application/zip',
  '.gz': 'application/gzip',
  '.tgz': 'application/gzip',
  '.doc': 'application/msword',
  '.docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  '.xls': 'application/vnd.ms-excel',
  '.xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  '.ppt': 'application/vnd.ms-powerpoint',
  '.pptx': 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  '.heic': 'image/heic',
  '.heif': 'image/heic',
  '.bmp': 'image/bmp',
  '.tif': 'image/tiff',
  '.tiff': 'image/tiff',
  '.mp3': 'audio/mpeg',
  '.wav': 'audio/wav',
  '.mp4': 'video/mp4',
}

export type PickedBody = { mimeType: string; data?: string; text?: string; blob?: string }

function base64(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString('base64')
}

/** An image becomes `data`. A PDF or other binary becomes `blob`. Text stays text. */
export function classifyPicked(name: string, bytes: Uint8Array): PickedBody {
  const ext = extname(name).toLowerCase()
  const image = IMAGES[ext]
  if (image) return { mimeType: image, data: base64(bytes) }
  const head = bytes.subarray(0, Math.min(bytes.length, 4096))
  const binary = head.includes(0)
  const file = FILES[ext]
  if (file || binary) return { mimeType: file ?? 'application/octet-stream', blob: base64(bytes) }
  return { mimeType: 'text/plain', text: Buffer.from(bytes).toString('utf8') }
}
