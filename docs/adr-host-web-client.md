# ADR: a web client served by the host

**Status:** Proposed
**Date:** 18 October 2026
**Deciders:** the user. No code from this ADR until they accept it.

## Context

Several comparable projects offer a browser client next to their native apps. Mux has a server mode with a responsive UI for mobile. OpenCode runs `opencode web` and its 2.0 release (reported 12 October 2026) puts the terminal, desktop and web apps on one shared background service. Happy ships an Expo app that also builds for the web, and a web client is part of its distribution. OpenCode's web UI was reported to be not mobile-responsive in a third-party client's notes. See [research notes](adr-ade-index.md#research-notes).

This project has a Compose Android app and an Electron desktop app. An iPhone, a Chromebook, a borrowed laptop and a guest ([adr-session-sharing](adr-session-sharing.md)) have no client. The desktop UI is React (`desktop/src/`), so a large part could run in a browser, but it depends on Electron's main process for host connections (`electron/host.ts`), the browser pane and the tray.

## Decision

Ship a **small web client served by the host**, read-mostly first, using the same API and token model.

1. The host serves static files at `/app/` (off by default: `[web] enabled = true`). Files are built in CI from the shared React code, embedded in the binary, and served with a strict CSP (`default-src 'self'`, no inline script, `frame-ancestors 'none'`), `X-Content-Type-Options` and no third-party requests.
2. Same transport rules as everything else: the listener must already be TLS if remote (existing requirement). The page is public static code; it holds no secrets until the person signs in.
3. **Sign-in** is pairing: the user enters or scans a pairing code (60 seconds, one use) and the page gets a device token. The token lives in memory and `sessionStorage` only by default, so closing the tab signs out; "Remember this device" asks first and uses an HttpOnly-free token in IndexedDB with a visible warning on shared computers. Guest links ([adr-session-sharing](adr-session-sharing.md)) use the same page.
4. **Scope of the first cut**: session list, a live conversation, approvals, task list and diffs, using the existing REST and WebSocket. No terminal, preview, browser or file picker in the first cut. These need their own checks (the desktop terminal uses xterm.js; the web client could reuse it later).
5. **Share code, not fork it**: extract the transport-independent parts of the desktop UI (components, `acp.ts`, `turns.ts`, `Markdown.tsx`) into a package both builds import, with a thin adapter replacing `window.api` (Electron IPC) with `fetch` and `WebSocket`. If extraction proves too tangled, the web client is a separate small app and the ADR is revisited.
6. The host adds a `Content-Security-Policy` report-only line to its logs only as counts, and no analytics anywhere.

## Options Considered

### Option A: host-served static client over the existing API (chosen)

| Dimension | Assessment |
|-----------|------------|
| Complexity | Medium to high; the extraction is the large part. |
| Cost | Binary grows by the bundle size. |
| Scalability | n/a |
| Team familiarity | React code already exists. |

**Pros:** works on any device with a browser, including iOS; the guest case needs it; no store review.
**Cons:** a browser holds the token in a less protected place than the Android Keystore; a third client to keep in step.

### Option B: a Progressive Web App wrapper and nothing else

**Pros:** installs to a home screen. **Cons:** still needs the web client; this is an extra on top of Option A.

### Option C: a native iOS app

**Pros:** best experience on iPhone. **Cons:** another codebase and a store account; not now.

## Consequences

- `docs/security.md` gets a web-client section: where the token lives, the CSP, and that cross-origin pages cannot call the API without the bearer token (the API already requires `Authorization`, which a cross-site page cannot add to a simple request).
- CORS: the page and API share an origin, so no CORS headers are needed and none are added.
- Tests: the CSP header on every static response; a request to `/app/` without a token still returns only static files; the page works against the mock host in a headless browser run.

## Action Items

1. [ ] User reviews this ADR.
2. [ ] Spike: how much of `desktop/src` imports Electron APIs? Count before committing to the extraction.
3. [ ] Host: static serving, headers, build step in CI.
4. [ ] Web: sign-in, session list, conversation, approvals.
