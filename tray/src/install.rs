//! Downloads the host (`acpd`) from the project's latest GitHub release and installs it in a per-user
//! folder, so the tray works on its own. Nothing is downloaded until you choose it in the menu.
//!
//! Safety: the address is fixed to this project's repository, downloads must stay on https GitHub hosts
//! after redirects, the file must match the size and SHA-256 listed in the release's `SHA256SUMS.txt`
//! (and GitHub's own digest when it publishes one), and it is written to a temporary file next to the
//! destination and moved into place only when everything matches.
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const RELEASE_URL: &str = "https://api.github.com/repos/0xbasinas/acportal/releases/latest";
/// The host is a few tens of megabytes; anything near this is not the file we expect.
const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

/// File name of the host in the release for this system, or None when no build is published.
pub fn release_asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("acpd-windows-x86_64.exe"),
        ("linux", "x86_64") => Some("acpd-linux-x86_64"),
        _ => None,
    }
}

/// Name the installed program gets: `acpd.exe` or `acpd`.
pub fn installed_name() -> &'static str {
    if cfg!(windows) { "acpd.exe" } else { "acpd" }
}

/// Where the host is installed: `%LOCALAPPDATA%\acpd\bin` on Windows, `~/.local/share/acpd/bin` on
/// Linux (next to the host's own `state` folder).
pub fn install_dir() -> PathBuf {
    acpd::config::default_data_directory().join("bin")
}

#[derive(Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub asset_url: String,
    pub asset_size: u64,
    /// GitHub's `sha256:` digest of the asset, when it publishes one.
    pub asset_digest: Option<String>,
    pub sums_url: String,
}

/// Addresses a download may use, before and after redirects.
pub fn is_trusted(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', ':', '?']).next().unwrap_or_default();
    host == "github.com" || host == "api.github.com" || host.ends_with(".githubusercontent.com")
}

/// Reads the GitHub release JSON for the asset named `asset_name`.
pub fn parse_release(json: &str, asset_name: &str) -> Result<Release> {
    let value: serde_json::Value =
        serde_json::from_str(json).context("read the release information")?;
    let assets = value["assets"]
        .as_array()
        .context("the release lists no files")?;
    let find = |name: &str| assets.iter().find(|asset| asset["name"] == name);
    let asset =
        find(asset_name).with_context(|| format!("the latest release has no {asset_name}"))?;
    let sums = find("SHA256SUMS.txt").context("the latest release has no SHA256SUMS.txt")?;
    let url = |asset: &serde_json::Value| {
        asset["browser_download_url"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let release = Release {
        tag: value["tag_name"].as_str().unwrap_or("unknown").to_string(),
        asset_url: url(asset),
        asset_size: asset["size"].as_u64().unwrap_or(0),
        asset_digest: asset["digest"]
            .as_str()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .map(str::to_lowercase),
        sums_url: url(sums),
    };
    if !is_trusted(&release.asset_url) || !is_trusted(&release.sums_url) {
        bail!("the release points at an unexpected address")
    }
    Ok(release)
}

/// Finds `name` in `sha256sum` output (`<hex>  <name>`), returning the lower-case hash.
pub fn parse_sums(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_lowercase())
    })
}

/// Writes `bytes` to `dir/name` only if their SHA-256 equals `expected`. The file is staged under a
/// temporary name and renamed, so a failed or interrupted install never leaves a broken program.
pub fn install_bytes(dir: &Path, name: &str, bytes: &[u8], expected: &str) -> Result<PathBuf> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected.to_lowercase() {
        bail!("the download did not match its checksum, so it was discarded")
    }
    std::fs::create_dir_all(dir).context("create the install folder")?;
    let target = dir.join(name);
    let staged = dir.join(format!(".{name}.download"));
    let write = || -> Result<()> {
        let mut file = std::fs::File::create(&staged)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        #[cfg(unix)]
        std::fs::set_permissions(&staged, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
        std::fs::rename(&staged, &target).with_context(|| {
            format!(
                "replace {}. If the host is running, stop it first.",
                target.display()
            )
        })
    };
    let result = write();
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result.map(|()| target)
}

fn get(url: &str) -> Result<ureq::Response> {
    if !is_trusted(url) {
        bail!("refusing to download from an unexpected address")
    }
    let response = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build()
        .get(url)
        .set("User-Agent", "acpd-tray")
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(code, _) => {
                anyhow::anyhow!("GitHub answered {code}. Try again later.")
            }
            other => anyhow::anyhow!("GitHub could not be reached: {other}"),
        })?;
    if !is_trusted(response.get_url()) {
        bail!("the download was redirected to an unexpected address")
    }
    Ok(response)
}

fn read_limited(response: ureq::Response, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("the download is larger than expected")
    }
    Ok(bytes)
}

/// Downloads and installs the latest host. Returns the installed path and the release tag.
pub fn download_latest() -> Result<(PathBuf, String)> {
    let asset = release_asset_name()
        .context("There is no host build for this system in the release yet.")?;
    let info = read_limited(get(RELEASE_URL)?, 2 * 1024 * 1024)?;
    let release = parse_release(&String::from_utf8_lossy(&info), asset)?;
    let sums = read_limited(get(&release.sums_url)?, 64 * 1024)?;
    let expected = parse_sums(&String::from_utf8_lossy(&sums), asset)
        .with_context(|| format!("SHA256SUMS.txt has no checksum for {asset}"))?;
    if let Some(digest) = &release.asset_digest
        && *digest != expected
    {
        bail!("GitHub's checksum and SHA256SUMS.txt disagree, so nothing was installed")
    }
    let bytes = read_limited(get(&release.asset_url)?, MAX_DOWNLOAD_BYTES)?;
    if release.asset_size != 0 && bytes.len() as u64 != release.asset_size {
        bail!("the download was incomplete. Try again.")
    }
    let path = install_bytes(&install_dir(), installed_name(), &bytes, &expected)?;
    Ok((path, release.tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    const RELEASE: &str = r#"{"tag_name":"v0.1.0","assets":[
      {"name":"acpd-windows-x86_64.exe","size":13854720,"digest":"sha256:ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789","browser_download_url":"https://github.com/0xbasinas/acportal/releases/download/v0.1.0/acpd-windows-x86_64.exe"},
      {"name":"SHA256SUMS.txt","size":358,"browser_download_url":"https://github.com/0xbasinas/acportal/releases/download/v0.1.0/SHA256SUMS.txt"}]}"#;
    #[test]
    fn release_is_read_for_the_wanted_asset() {
        let release = parse_release(RELEASE, "acpd-windows-x86_64.exe").unwrap();
        assert_eq!(release.tag, "v0.1.0");
        assert_eq!(release.asset_size, 13854720);
        assert_eq!(
            release.asset_digest.as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
        assert!(release.sums_url.ends_with("SHA256SUMS.txt"));
        assert!(
            parse_release(RELEASE, "acpd-linux-x86_64")
                .unwrap_err()
                .to_string()
                .contains("no acpd-linux")
        );
        assert!(parse_release("{}", "x").is_err());
    }
    #[test]
    fn a_release_pointing_elsewhere_is_refused() {
        let evil = RELEASE.replace(
            "https://github.com/0xbasinas",
            "https://evil.example.com/0xbasinas",
        );
        assert!(
            parse_release(&evil, "acpd-windows-x86_64.exe")
                .unwrap_err()
                .to_string()
                .contains("unexpected address")
        );
    }
    #[test]
    fn only_https_github_addresses_are_trusted() {
        for ok in [
            "https://github.com/a/b",
            "https://api.github.com/repos/a/b",
            "https://release-assets.githubusercontent.com/x?y=1",
            "https://objects.githubusercontent.com:443/x",
        ] {
            assert!(is_trusted(ok), "{ok}");
        }
        for bad in [
            "http://github.com/a",
            "https://github.com.evil.example/a",
            "https://evilgithub.com/a",
            "https://notgithubusercontent.com/a",
            "ftp://github.com/a",
            "",
        ] {
            assert!(!is_trusted(bad), "{bad}");
        }
    }
    #[test]
    fn checksum_lines_are_matched_by_exact_file_name() {
        let hash = "a".repeat(64);
        let sums = format!(
            "{hash}  acpd-linux-x86_64\n{}  acpd-tray-linux-x86_64\n{hash} *acpd-windows-x86_64.exe\n",
            "b".repeat(64)
        );
        assert_eq!(
            parse_sums(&sums, "acpd-linux-x86_64").as_deref(),
            Some(hash.as_str())
        );
        assert_eq!(
            parse_sums(&sums, "acpd-windows-x86_64.exe").as_deref(),
            Some(hash.as_str())
        );
        assert_eq!(
            parse_sums(&sums, "acpd-tray-linux-x86_64"),
            Some("b".repeat(64))
        );
        assert_eq!(parse_sums(&sums, "acpd"), None);
        assert_eq!(parse_sums("short  acpd", "acpd"), None);
    }
    #[test]
    fn a_matching_download_is_installed_and_a_wrong_one_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"pretend program";
        let good = format!("{:x}", Sha256::digest(bytes));
        let bad = install_bytes(&dir.path().join("bin"), "acpd", bytes, &"0".repeat(64));
        assert!(bad.unwrap_err().to_string().contains("checksum"));
        assert!(
            !dir.path().join("bin").exists(),
            "nothing is created on a mismatch"
        );
        let target =
            install_bytes(&dir.path().join("bin"), "acpd", bytes, &good.to_uppercase()).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        assert_eq!(
            std::fs::read_dir(dir.path().join("bin")).unwrap().count(),
            1,
            "no staged file is left"
        );
        // Installing again replaces the file.
        let again = install_bytes(&dir.path().join("bin"), "acpd", bytes, &good).unwrap();
        assert_eq!(again, target);
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(&target).unwrap().permissions()
            ) & 0o777,
            0o755
        );
    }
    /// Downloads the real latest release from GitHub. Needs the network, so it only runs on request:
    /// `XDG_DATA_HOME=<scratch folder> cargo test -- --ignored real_download` (the variable keeps it
    /// out of your real install folder).
    #[test]
    #[ignore]
    fn real_download_installs_a_working_host() {
        assert!(
            std::env::var_os("XDG_DATA_HOME").is_some(),
            "set XDG_DATA_HOME to a scratch folder first"
        );
        let (path, tag) = download_latest().unwrap();
        assert!(tag.starts_with('v'));
        assert!(path.starts_with(install_dir()));
        let output = std::process::Command::new(&path)
            .arg("--version")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("acpd "));
    }
    #[test]
    fn install_location_is_under_the_data_folder() {
        assert!(install_dir().ends_with(Path::new("acpd").join("bin")));
        assert_eq!(
            installed_name(),
            if cfg!(windows) { "acpd.exe" } else { "acpd" }
        );
    }
}
