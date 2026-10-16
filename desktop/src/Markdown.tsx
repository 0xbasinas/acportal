import { ReactNode } from 'react'

// Small, safe Markdown subset for agent replies (no HTML, no automatic link fetches).
function inline(s: string, key: string): ReactNode[] {
  const out: ReactNode[] = []
  const re = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*\s][^*]*\*|\[[^\]]+\]\([^)\s]+\))/g
  let last = 0
  let m: RegExpExecArray | null
  let k = 0
  while ((m = re.exec(s))) {
    if (m.index > last) out.push(s.slice(last, m.index))
    const t = m[0]
    if (t.startsWith('`')) out.push(<code key={key + k++} className="ic">{t.slice(1, -1)}</code>)
    else if (t.startsWith('**')) out.push(<strong key={key + k++}>{t.slice(2, -2)}</strong>)
    else if (t.startsWith('[')) {
      const [, label, href] = /\[([^\]]+)\]\(([^)]+)\)/.exec(t)!
      out.push(<a key={key + k++} href={href} onClick={(e) => { e.preventDefault(); if (href.startsWith('https:') && confirm(`Open ${href} in your browser?`)) window.open(href) }}>{label}</a>)
    } else out.push(<em key={key + k++}>{t.slice(1, -1)}</em>)
    last = m.index + t.length
  }
  if (last < s.length) out.push(s.slice(last))
  return out
}

export function Markdown({ text }: { text: string }) {
  const blocks: ReactNode[] = []
  const lines = text.split('\n')
  let i = 0
  let para: string[] = []
  const flush = (): void => {
    if (para.length) blocks.push(<p key={'p' + i + blocks.length}>{inline(para.join(' '), 'p' + blocks.length)}</p>)
    para = []
  }
  while (i < lines.length) {
    const l = lines[i]
    if (l.startsWith('```')) {
      flush()
      const lang = l.slice(3).trim()
      const code: string[] = []
      i++
      while (i < lines.length && !lines[i].startsWith('```')) code.push(lines[i++])
      i++
      blocks.push(<pre key={'c' + i} className="md-code mono" data-lang={lang}>{code.join('\n')}</pre>)
      continue
    }
    const h = /^(#{1,4})\s+(.*)/.exec(l)
    const li = /^\s*([-*]|\d+\.)\s+(.*)/.exec(l)
    if (h) { flush(); blocks.push(<div key={'h' + i} className="md-h">{inline(h[2], 'h' + i)}</div>) }
    else if (li) { flush(); blocks.push(<div key={'l' + i} className="md-li"><span className="sub">{/\d/.test(li[1]) ? li[1] : '•'}</span><span>{inline(li[2], 'l' + i)}</span></div>) }
    else if (!l.trim()) flush()
    else para.push(l)
    i++
  }
  flush()
  return <div className="md">{blocks}</div>
}
