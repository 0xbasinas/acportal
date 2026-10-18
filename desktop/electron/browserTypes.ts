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
}
