//! Local operator QR presentation. No credential or code is persisted here.
use anyhow::{Result, bail};
use reqwest::Url;

pub fn validate_pairing_address(address: &str) -> Result<Url> {
    let url =
        Url::parse(address).map_err(|_| anyhow::anyhow!("use a phone-reachable HTTPS address"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || address.len() > 2048
        || address.chars().any(char::is_control)
    {
        bail!("use an HTTPS host address without a path, credentials, query or fragment")
    }
    Ok(url)
}

pub fn pairing_uri(address: &str, code: &str, name: &str) -> Result<String> {
    let address = validate_pairing_address(address)?;
    if name.encode_utf16().count() > 120 || name.chars().any(char::is_control) {
        bail!("connection name must be at most 120 characters without control characters")
    }
    let mut uri = Url::parse("acportal://pair")?;
    uri.query_pairs_mut()
        .append_pair("address", address.as_str().trim_end_matches('/'))
        .append_pair("code", code)
        .append_pair("name", name);
    Ok(uri.into())
}

pub fn terminal_qr(uri: &str) -> Result<String> {
    use qrcode::render::unicode::Dense1x2;
    Ok(qrcode::QrCode::new(uri.as_bytes())?
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_fields_round_trip_and_terminal_has_quiet_zone() {
        let uri = pairing_uri(
            "https://host.example:8443",
            "ABCD-1234-EF56",
            "Work & laptop",
        )
        .unwrap();
        let url = Url::parse(&uri).unwrap();
        let fields: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(fields["address"], "https://host.example:8443");
        assert_eq!(fields["code"], "ABCD-1234-EF56");
        assert_eq!(fields["name"], "Work & laptop");
        let qr = terminal_qr(&uri).unwrap();
        assert!(qr.lines().count() > 20);
        assert!(qr.lines().next().unwrap().chars().all(|c| c == '█'));
    }
    #[test]
    fn remote_cleartext_credentials_and_ambiguous_addresses_are_rejected() {
        for value in [
            "http://host.example",
            "https://user:password@host.example",
            "https://host.example/path",
            "https://host.example?token=secret",
            "https://host.example#fragment",
            "invalid",
        ] {
            assert!(validate_pairing_address(value).is_err());
        }
        assert!(pairing_uri("https://host.example", "fixture", "bad\nname").is_err());
    }
}
