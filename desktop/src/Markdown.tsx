// Agent replies as GitHub-flavoured Markdown (headings, nested lists, task lists, tables,
// strikethrough, block quotes, highlighted code), rendered safely like the phone does:
// raw HTML is shown as text and never interpreted, remote images are never fetched (they show
// as a link), and a link opens in the browser only after the user confirms it.
import { type ReactNode, isValidElement, useState } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { isLoopbackHost } from '../electron/browserRules'

function openLink(href: string | undefined): void {
  if (!href || !/^https?:/i.test(href)) return
  // A page on this computer opens in the app's own browser; anything else asks before it leaves.
  try { if (isLoopbackHost(new URL(href).hostname)) { window.dispatchEvent(new CustomEvent('acportal:open-url', { detail: href })); return } } catch { /* not a URL */ }
  if (confirm(`Open ${href} in your browser?`)) window.open(href)
}

/** The plain text of rendered children (for copying a highlighted code block). */
function textOf(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(textOf).join('')
  if (isValidElement(node)) return textOf((node.props as { children?: ReactNode }).children)
  return ''
}

function CodeBlock({ children }: { children?: ReactNode }) {
  const [copied, setCopied] = useState(false)
  const code = Array.isArray(children) ? children[0] : children
  const className = isValidElement(code) ? String((code.props as { className?: string }).className ?? '') : ''
  const lang = /language-([\w+-]+)/.exec(className)?.[1]
  const text = textOf(children).replace(/\n$/, '')
  return (
    <div className="md-code-wrap">
      <div className="md-code-bar">
        <span className="mono">{lang ?? 'text'}</span>
        <button className="md-copy" onClick={() => { void navigator.clipboard.writeText(text).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1500) }) }}>{copied ? 'Copied' : 'Copy'}</button>
      </div>
      <pre className="md-code mono">{children}</pre>
    </div>
  )
}

const components: Components = {
  a: ({ href, children }) => <a href={href} title={href} onClick={(e) => { e.preventDefault(); openLink(href) }}>{children}</a>,
  // Never fetch: an image becomes a link the user can choose to open.
  img: ({ src, alt }) => <a className="md-img" href={typeof src === 'string' ? src : undefined} title={typeof src === 'string' ? src : undefined} onClick={(e) => { e.preventDefault(); openLink(typeof src === 'string' ? src : undefined) }}>🖼 {alt || 'image'}</a>,
  pre: ({ children }) => <CodeBlock>{children}</CodeBlock>,
  code: ({ className, children }) => (className ? <code className={className}>{children}</code> : <code className="ic">{children}</code>),
  table: ({ children }) => <div className="md-table"><table>{children}</table></div>,
  input: ({ checked }) => <input type="checkbox" checked={Boolean(checked)} readOnly disabled />
}

export function Markdown({ text }: { text: string }) {
  return (
    <div className="md">
      {/* No rehype-raw: raw HTML is never parsed into elements. The default URL transform drops
          javascript: and other unsafe link targets. */}
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[[rehypeHighlight, { detect: false }]]} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  )
}
