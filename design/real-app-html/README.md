# Real app screens as HTML

One standalone HTML page for each screenshot in [`../real-app/`](../real-app/README.md): 38 pages, dark and light, named like the screenshots (`01-home-new-session.html` … `38-host-switcher.html`). Open `index.html` for a grid of all of them.

They are not Penpot exports. Each page is plain, semantic-ish markup that uses the **desktop app's own stylesheet** (`styles.css` is `desktop/src/index.css` plus a short block for stills, at the end) and the app's own class names (`.side`, `.topbar`, `.approval`, `.diff`, `.ctx-pop`, …), so tokens, spacing, radii and states come from the same source as the running app. Icons are the Lucide icons the app uses, inlined. Inter and JetBrains Mono are bundled in `fonts/` (SIL OFL 1.1), so the pages look the same offline.

## Viewing

Open a page directly, or serve the folder:

```bash
cd design/real-app-html
python3 -m http.server 8802   # then http://127.0.0.1:8802/index.html
```

Pages fill the window like the app does. The screenshots were taken at 1282 × 802 CSS pixels; use that size to compare. A long conversation starts scrolled to its end (a few lines of script) like the screenshot.

## How close they are

Checked by rendering every page in headless Chromium at 1282 × 802 (scale 1.25) and diffing it with its screenshot: layout, colours and sizes line up pixel for pixel; the remaining difference is text. The screenshots were taken on Windows (system font rendering, Cascadia Mono in the terminal), these pages render with the bundled fonts. Between 0.4 % (settings) and 3.2 % (long conversations) of pixels differ by more than a small threshold, almost all of it glyph edges.

Known gaps:

- The terminal dock is two lines of text, not xterm.js, in the app's ANSI palette. The cursor, text selection and any caret in a text box are not drawn.
- Nothing is interactive. A hover or focus state in a screenshot (the highlighted ended session, the focused composer, the open host switcher) is drawn with a class (`is-hover`, `is-focus`, `.open`).
- The command palette lists 16 rows (17 in the light conversation); the screenshots only show the first ten or so, so the rows below them are plausible, not captured. The palette footer says the count the screenshot says.

## Regenerating

The pages are generated; edit `build.py` (markup per screen) or `styles.css`, not the `.html` files:

```bash
python3 build.py
```

`lucide_icons.py` holds the inner SVG of the icons used (Lucide, ISC licence, from `lucide-static` 0.468.0). When the app changes, refresh `styles.css` from `desktop/src/index.css` and re-append the block under `static replica additions`, then compare against new screenshots.
