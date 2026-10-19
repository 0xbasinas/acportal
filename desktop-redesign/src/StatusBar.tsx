import { I } from './ui'

/**
 * A thin strip under the conversation with the two panels that can be opened, terminal and browser.
 * A panel that is open is shown in full colour and bold; a toggle is a real button, so it works
 * with the keyboard and a screen reader. How the host is doing is shown in the sidebar, not here.
 */
export function StatusBar(props: {
  terminal: { available: boolean; open: boolean; onToggle: () => void }
  browser: { open: boolean; onToggle: () => void }
}) {
  return (
    <footer className="statusbar" aria-label="Panels">
      <span className="grow" />
      <button className={'statusbar-toggle' + (props.terminal.open ? ' on' : '')} aria-pressed={props.terminal.open} disabled={!props.terminal.available && !props.terminal.open}
        title={props.terminal.available ? 'Show or hide the terminal' : 'Terminals are off on this host'} onClick={props.terminal.onToggle}>
        {I.term}Terminal
      </button>
      <button className={'statusbar-toggle' + (props.browser.open ? ' on' : '')} aria-pressed={props.browser.open} title="Show or hide the browser" onClick={props.browser.onToggle}>
        {I.globe}Browser
      </button>
    </footer>
  )
}
