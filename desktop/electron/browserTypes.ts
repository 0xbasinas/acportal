// What the page is told about the in-app browser. Types only: the page and the main process share it.
export type BrowserState = {
  open: boolean
  url: string
  title: string
  loading: boolean
  canBack: boolean
  canForward: boolean
  /** Zoom in percent. */
  zoom: number
  /** The page is on this computer. */
  local: boolean
  /** Why the page could not be shown, in plain words. */
  error?: string
  /** Something the person should know: a link went to their own browser, or was refused. */
  note?: string
  /** How long the last page took to load, in milliseconds. */
  loadedMs?: number
  /** Sign-ins are kept between launches. */
  remembers: boolean
  /** Open tabs, in order. */
  tabs: BrowserTab[]
  /** The person is choosing an element on the page. */
  picking?: boolean
  /** Find in page: how many matches, and which one is current. Absent when not searching. */
  find?: { matches: number; active: number }
}

/** An element the person picked on the page (untrusted text from the page). */
export type PickedElement = {
  tag: string
  selector: string
  attributes: Record<string, string>
  text: string
  html: string
  htmlTruncated: boolean
  rect: { x: number; y: number; width: number; height: number }
  viewport: { width: number; height: number }
  parents: string[]
  styles: Record<string, string>
  url: string
  title: string
}

export type BrowserTab = { id: number; title: string; url: string; active: boolean; loading: boolean }
