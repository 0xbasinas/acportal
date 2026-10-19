import { useState } from 'react'

/** Remembered widths for the panes that used to be fixed. */
export const PANE = {
  side: { def: 252, min: 200, max: 520 },
  right: { def: 288, min: 220, max: 560 },
  settings: { def: 232, min: 200, max: 380 },
  review: { def: 248, min: 180, max: 480 },
} as const

export type PaneName = keyof typeof PANE

const KEY = 'acportal.panes'

export function clampPane(n: number, min: number, max: number): number {
  const hi = Math.max(min, max)
  if (!Number.isFinite(n)) return min
  return Math.round(Math.min(hi, Math.max(min, n)))
}

export function readPane(name: PaneName): number {
  const b = PANE[name]
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Record<string, unknown>
    const n = Number(raw[name])
    return Number.isFinite(n) && n > 0 ? clampPane(n, b.min, b.max) : b.def
  } catch {
    return b.def
  }
}

export function writePane(name: PaneName, n: number): void {
  const b = PANE[name]
  try {
    const cur = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Record<string, number>
    cur[name] = clampPane(n, b.min, b.max)
    localStorage.setItem(KEY, JSON.stringify(cur))
  } catch { /* storage can be blocked; the width still applies for this launch */ }
}

/**
 * A wide hit target in the gutter between two panes. Dragging follows the pointer even
 * after it leaves the strip. Double-click restores the default width.
 */
export function PaneResize(props: {
  axis: 'x' | 'y'
  /** Which side grows when the pointer moves right or down. */
  grow: 'start' | 'end'
  label: string
  value: number
  min: number
  max: number
  onChange: (n: number) => void
  onCommit?: (n: number) => void
  onReset: () => void
  className?: string
}) {
  const [dragging, setDragging] = useState(false)
  const sign = props.grow === 'start' ? 1 : -1
  const set = (n: number, commit: boolean): void => {
    const next = clampPane(n, props.min, props.max)
    props.onChange(next)
    if (commit) props.onCommit?.(next)
  }
  return (
    <div
      className={'pane-resize ' + (props.axis === 'x' ? 'v ' : 'h ') + (dragging ? 'dragging ' : '') + (props.className ?? '')}
      role="separator"
      aria-orientation={props.axis === 'x' ? 'vertical' : 'horizontal'}
      aria-label={props.label}
      aria-valuenow={Math.round(props.value)}
      aria-valuemin={props.min}
      aria-valuemax={Math.max(props.min, props.max)}
      title="Drag to resize. Double-click to reset."
      tabIndex={0}
      onDoubleClick={(e) => { e.preventDefault(); props.onReset() }}
      onKeyDown={(e) => {
        const dir = props.axis === 'x'
          ? e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0
          : e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0
        if (!dir) return
        e.preventDefault()
        set(props.value + dir * sign * (e.shiftKey ? 72 : 24), true)
      }}
      onPointerDown={(e) => {
        if (e.button !== 0) return
        e.preventDefault()
        e.stopPropagation()
        const start = props.axis === 'x' ? e.clientX : e.clientY
        const origin = props.value
        const target = e.currentTarget
        target.setPointerCapture(e.pointerId)
        setDragging(true)
        const prevCursor = document.body.style.cursor
        const prevSelect = document.body.style.userSelect
        document.body.style.cursor = props.axis === 'x' ? 'col-resize' : 'row-resize'
        document.body.style.userSelect = 'none'
        let latest = origin
        const move = (m: PointerEvent): void => {
          const now = props.axis === 'x' ? m.clientX : m.clientY
          latest = clampPane(origin + sign * (now - start), props.min, props.max)
          props.onChange(latest)
        }
        const up = (): void => {
          setDragging(false)
          document.body.style.cursor = prevCursor
          document.body.style.userSelect = prevSelect
          window.removeEventListener('pointermove', move)
          window.removeEventListener('pointerup', up)
          window.removeEventListener('pointercancel', up)
          props.onCommit?.(latest)
        }
        window.addEventListener('pointermove', move)
        window.addEventListener('pointerup', up)
        window.addEventListener('pointercancel', up)
      }}
    />
  )
}
