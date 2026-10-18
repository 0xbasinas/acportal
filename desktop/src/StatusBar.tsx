import { I } from './ui'
import { STATE_DOT, STATE_LABEL, type FleetEntry } from './fleet'

const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)
const mod = mac ? '⌘' : 'Ctrl'

/**
 * The strip under the conversation: how the host is doing on the left, and the two panels that can be
 * opened (terminal, browser) on the right, each with its shortcut. A panel that is open is shown in
 * full colour and bold; a toggle is a real button, so it works with the keyboard and a screen reader.
 */
export function StatusBar(props: {
  entry?: FleetEntry
  branch?: string
  agent?: string
  terminal: { available: boolean; open: boolean; onToggle: () => void }
  browser: { open: boolean; onToggle: () => void }
}) {
  const { entry } = props
  const state = entry?.state ?? 'connecting'
  return (
    <footer className="statusbar" aria-label="Status">
      <span className="statusbar-item" role="status">
        <span className={'dot ' + STATE_DOT[state]} aria-hidden="true" />
        {STATE_LABEL[state]}{entry?.latency !== undefined && state === 'connected' ? ` · ${entry.latency} ms` : ''}
      </span>
      {props.branch && <span className="statusbar-item mono" title="Branch of this session's folder">{I.branch}{props.branch}</span>}
      {props.agent && <span className="statusbar-item">{props.agent}</span>}
      <span className="grow" />
      <button className={'statusbar-toggle' + (props.terminal.open ? ' on' : '')} aria-pressed={props.terminal.open} disabled={!props.terminal.available && !props.terminal.open}
        title={props.terminal.available ? 'Show or hide the terminal' : 'Terminals are off on this host'} onClick={props.terminal.onToggle}>
        {I.term}Terminal <span className="kbd">{mod} `</span>
      </button>
      <button className={'statusbar-toggle' + (props.browser.open ? ' on' : '')} aria-pressed={props.browser.open} title="Show or hide the browser" onClick={props.browser.onToggle}>
        {I.globe}Browser <span className="kbd">{mod} Shift B</span>
      </button>
    </footer>
  )
}
