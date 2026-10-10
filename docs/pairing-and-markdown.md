# Easier pairing and Markdown replies

10 October 2026 accessibility follow-up: paragraphs expose named review actions for each inline link; standalone links expose an ordinary button action. Native TalkBack reaches the existing confirmation and Cancel for standalone and independently selected inline links in both themes at normal and actual double font. No browser opens without confirmation. Visible formatting remains unchanged. Broader rich-content lifecycle/accessibility remains open in [TODO](../TODO.md).

Implemented locally on `codex/session-model-picker`, 9 October 2026. Existing host authentication, certificate trust and explicit pairing remain in force.

## Pair a phone

Run the updated host executable on your computer with the same configuration file as the running daemon. Supply its phone-reachable HTTPS address:

```powershell
.\target\debug\acpd.exe --config "$env:LOCALAPPDATA\ACPPortal-PhoneTest\config.toml" pair --address https://YOUR-TAILSCALE-HOST --qr --name "Workstation"
```

Replace the address and configuration path with your deployment values. Tailscale Serve can supply the trusted HTTPS address; this command does not configure Serve or TLS. Without native setup, bare `acpd pair` produces the manual code. After [native setup](native-cli-and-motion.md), it uses the saved HTTPS address and displays a QR; `acpd pair --manual` suppresses the QR. Invalid presentation arguments are rejected before replacing a current code.

In Android, open Add connection and choose Scan pairing QR code. Scanning fills the address, optional name and code. Review them and tap Pair connection explicitly. The QR carries a one-use code that expires after 60 seconds, not a device token. Treat the terminal QR and code as secrets; do not save them in logs or screenshots.

Google Code Scanner delegates scanning to Google Play services without an app camera permission. The scanner module may need downloading on first use. Cancellation or scanner failure preserves manual entries. Manual entry accepts lowercase codes and spaces or omitted hyphens; address validation and host authentication remain authoritative. Codes and raw scans are excluded from saved instance state. HTTPS is required for remote hosts; existing debug local-address exceptions remain unchanged.

## Agent replies

Agent text renders native Compose headings, emphasis, lists, quotes, code and GFM tables. User messages, permission text and tool content retain their existing presentation. Stored messages and transcript exports preserve the original source.

CommonMark parsing runs off Main with an 80 ms streaming coalescing delay. Formatting is limited to 65,536 characters, 2,000 AST nodes and depth 32. Larger or more complex replies remain complete plain text with a visible fallback label. These limits bound formatting work, not overall Android heap acceptance.

HTML remains literal text and images remain alt text/URLs without remote fetching. Only HTTP/HTTPS links without embedded credentials can open, after an explicit confirmation. Rendering uses no WebView. Pinned additions are CommonMark Java/tables 0.27.1, Google Code Scanner 16.1.0 and Rust qrcode 0.14.1 without default features.

## Verification limits

See [TODO_DONE](../TODO_DONE.md) for exact terminal results. Scanner fixtures inject QR results; they do not prove camera capture or Play-services module availability. Real-phone scanning, native TalkBack, broader streaming/large-provider pressure and current supported-build Actions remain open. Physical-phone testing is user-owned.
