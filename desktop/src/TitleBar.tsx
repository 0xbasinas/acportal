// The app's own title bar (the window has no system frame): the acportal logo (the same ring and
// prompt chevron as the phone app, assets/logo), the app name and the window title the app sets.
// The whole bar drags the window; the system's window buttons sit over its right end (Windows,
// Linux) or at its left (macOS traffic lights), so those areas are left free.
import { useEffect, useState } from 'react'

/** The logo in the theme's colours (dark: assets/logo/logo-dark.svg, light: logo-light.svg). */
export function Logo({ size = 18 }: { size?: number }) {
  return (
    <svg className="app-logo" width={size} height={size} viewBox="0 0 512 512" aria-hidden="true">
      <rect width="512" height="512" rx="112" fill="var(--logo-bg)" />
      <circle cx="256" cy="256" r="152" fill="none" stroke="var(--logo-ring)" strokeWidth="36" />
      <path d="M222 188L296 256L222 324" fill="none" stroke="var(--logo-mark)" strokeWidth="36" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function TitleBar() {
  const [title, setTitle] = useState(document.title)
  useEffect(() => {
    const el = document.querySelector('title')
    if (!el) return
    const watch = new MutationObserver(() => setTitle(document.title))
    watch.observe(el, { childList: true, characterData: true, subtree: true })
    return () => watch.disconnect()
  }, [])
  const mac = navigator.userAgent.includes('Mac')
  const detail = title && title !== 'acportal' ? title.replace(/ · acportal$/, '') : ''
  return (
    <header className={'titlebar' + (mac ? ' mac' : '')}>
      <Logo />
      <span className="titlebar-name">acportal</span>
      {detail && <span className="titlebar-detail ell" title={detail}>{detail}</span>}
    </header>
  )
}
