//! The local page that shows the pairing QR code. It is one self-contained file (no scripts or
//! styles from the internet), opened in the default browser and deleted after the code expires.
use anyhow::Result;
use qrcode::{QrCode, render::svg};

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `uri` is the `acportal://pair?...` link the phone app scans; `code` and `address` are shown for
/// typing by hand.
pub fn render(uri: &str, code: &str, address: &str, seconds: u32) -> Result<String> {
    let qr = QrCode::new(uri.as_bytes())?
        .render::<svg::Color>()
        .min_dimensions(280, 280)
        .dark_color(svg::Color("#111111"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:">
<title>Pair a phone</title>
<style>
:root{{--bg:#fafafa;--card:#f0f0f2;--text:#171719;--muted:#696972;--green:#1f8a64}}
@media (prefers-color-scheme:dark){{:root{{--bg:#0a0a0a;--card:#161616;--text:#e8e8ea;--muted:#9c9ca4;--green:#72d5b0}}}}
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--text);font:16px/1.5 system-ui,Segoe UI,sans-serif}}
main{{background:var(--card);border-radius:20px;padding:32px;max-width:420px;text-align:center}}
h1{{margin:0 0 4px;font-size:22px}} p{{margin:8px 0;color:var(--muted)}}
.qr{{background:#fff;border-radius:12px;padding:12px;margin:20px auto;width:min(300px,100%);transition:opacity .3s}}
.qr svg{{width:100%;height:auto;display:block}}
.code{{font:600 24px/1.2 ui-monospace,Consolas,monospace;letter-spacing:.06em;color:var(--text);margin:12px 0 4px}}
#left{{color:var(--green);font-weight:600}} .expired .qr{{opacity:.08}} .expired #left{{color:#b3261e}}
</style></head><body><main id="m">
<h1>Pair a phone</h1>
<p>In ACP Portal, open Connections, choose Add connection, then Scan pairing QR code.</p>
<div class="qr" role="img" aria-label="Pairing QR code">{qr}</div>
<p>Or type this code:</p>
<div class="code">{code}</div>
<p>Address: {address}</p>
<p id="left">Expires in {seconds} seconds</p>
<p>The code works once. Close this page when you are done.</p>
</main>
<script>
var s={seconds};var el=document.getElementById("left");
var t=setInterval(function(){{s--;if(s<=0){{clearInterval(t);document.getElementById("m").className="expired";el.textContent="Code expired. Choose Pair a phone in the tray menu again.";}}else{{el.textContent="Expires in "+s+" seconds";}}}},1000);
</script></body></html>
"#,
        qr = qr,
        code = escape(code),
        address = escape(address),
        seconds = seconds
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_is_self_contained_and_shows_the_code_and_address() {
        let page = render(
            "acportal://pair?address=https%3A%2F%2Fh.example&code=ABCD-1234-EF56&name=Laptop",
            "ABCD-1234-EF56",
            "https://h.example",
            60,
        )
        .unwrap();
        assert!(page.contains("<svg") && page.contains("ABCD-1234-EF56"));
        assert!(page.contains("Address: https://h.example"));
        assert!(page.contains("var s=60;"));
        // Nothing is loaded from anywhere: no external src/href, and the CSP forbids connections.
        assert!(!page.contains("src=\"http") && !page.contains("href=\"http"));
        assert!(page.contains("default-src 'none'"));
    }
    #[test]
    fn untrusted_text_is_escaped() {
        let page = render("acportal://pair", "<script>x</script>", "a\"b&c", 60).unwrap();
        assert!(!page.contains("<script>x</script>"));
        assert!(page.contains("&lt;script&gt;") && page.contains("a&quot;b&amp;c"));
    }
}
