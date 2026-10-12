#!/bin/sh
# Installs ACP Portal (the acpd host and the tray app) for the current user. No root needed.
#
# Downloads acpd and acpd-tray from the project's GitHub release, checks them against the release's
# SHA256SUMS.txt, and installs them in ~/.local/share/acpd/bin with links in ~/.local/bin and a desktop
# entry for the tray. Run it again to update. Nothing outside your home folder is changed.
#
# Quick install (the script is short; read it first if you like):
#   curl -fsSL https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.sh | sh
# With options:
#   curl -fsSL https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.sh | sh -s -- --version v0.1.1
#
# Options:  --version vX.Y.Z   install that release (default: the latest)
#           --only host|tray   install (or, with --uninstall, remove) just the host or just the tray
#                              (default: both). A tray-only install can download the host from its menu.
#           --uninstall        remove the programs, links and desktop entry (settings and state are kept)
#           --no-links         do not create links in ~/.local/bin
#           --no-desktop       do not create the desktop entry
# Environment: ACPD_VERSION, ACPD_ONLY, ACPD_INSTALL_DIR (default ${XDG_DATA_HOME:-~/.local/share}/acpd/bin)

set -eu

REPO="0xbasinas/acportal"
VERSION="${ACPD_VERSION:-}"
ONLY="${ACPD_ONLY:-both}"
UNINSTALL=0
LINKS=1
DESKTOP=1

while [ $# -gt 0 ]; do
    case "$1" in
        --version) [ $# -ge 2 ] || { echo "--version needs a value like v0.1.1" >&2; exit 2; }; VERSION="$2"; shift 2 ;;
        --only) [ $# -ge 2 ] || { echo "--only needs host or tray" >&2; exit 2; }; ONLY="$2"; shift 2 ;;
        --uninstall) UNINSTALL=1; shift ;;
        --no-links) LINKS=0; shift ;;
        --no-desktop) DESKTOP=0; shift ;;
        -h|--help) sed -n '2,20p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1 (try --help)" >&2; exit 2 ;;
    esac
done

say() { printf '  %s\n' "$*"; }
die() { printf 'Error: %s\n' "$*" >&2; exit 1; }

data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
dir="${ACPD_INSTALL_DIR:-$data_home/acpd/bin}"
bin_links="$HOME/.local/bin"
desktop_file="$data_home/applications/acp-portal.desktop"
icon_file="$data_home/icons/hicolor/scalable/apps/acp-portal.svg"

# What this run covers: the programs it installs or removes, and their release file names.
case "$ONLY" in
    host) names="acpd"; pairs="acpd-linux-x86_64:acpd"; label="the acpd host" ;;
    tray) names="acpd-tray"; pairs="acpd-tray-linux-x86_64:acpd-tray"; label="the tray app" ;;
    both) names="acpd acpd-tray"; pairs="acpd-linux-x86_64:acpd acpd-tray-linux-x86_64:acpd-tray"; label="ACP Portal (host and tray)" ;;
    *) die "--only must be host or tray (or leave it out for both), got: $ONLY" ;;
esac
has() { case " $names " in *" $1 "*) return 0 ;; esac; return 1; }

if [ "$UNINSTALL" = 1 ]; then
    echo "Uninstalling $label from $dir"
    if has acpd && [ -x "$dir/acpd" ]; then "$dir/acpd" daemon stop >/dev/null 2>&1 || true; fi
    if has acpd-tray; then pkill -x acpd-tray >/dev/null 2>&1 || true; fi
    for name in $names; do
        # Only remove a link that points at our install folder.
        if [ -L "$bin_links/$name" ] && [ "$(readlink "$bin_links/$name")" = "$dir/$name" ]; then rm -f "$bin_links/$name"; fi
        rm -f "$dir/$name"
    done
    if has acpd-tray; then rm -f "$desktop_file" "$icon_file"; fi
    rmdir "$dir" 2>/dev/null || true
    say "Removed $label, with its links and desktop entry."
    say "Your settings (${XDG_CONFIG_HOME:-$HOME/.config}/acpd) and paired devices ($data_home/acpd/state) were kept."
    exit 0
fi

[ "$(uname -s)" = "Linux" ] || die "This installer is for Linux. On Windows use scripts/install.ps1."
case "$(uname -m)" in x86_64|amd64) ;; *) die "Only x86_64 builds are published (this is $(uname -m))." ;; esac
if [ -n "$VERSION" ]; then
    case "$VERSION" in v[0-9]*) ;; *) die "Version must look like v0.1.1, got: $VERSION" ;; esac
    base="https://github.com/$REPO/releases/download/$VERSION"
else
    base="https://github.com/$REPO/releases/latest/download"
fi

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -q --https-only -O "$2" "$1"; }
else
    die "Install curl or wget first."
fi
if command -v sha256sum >/dev/null 2>&1; then
    hash_of() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
    hash_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
    die "Install sha256sum (coreutils) first."
fi

echo "Installing $label (${VERSION:-latest release}) into $dir"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM

say "Downloading checksums..."
fetch "$base/SHA256SUMS.txt" "$work/SHA256SUMS.txt" || die "Could not download SHA256SUMS.txt from $base."

for pair in $pairs; do
    asset="${pair%%:*}"
    name="${pair##*:}"
    want="$(awk -v f="$asset" '{ n=$2; sub(/^\*/, "", n); if (n == f) { print tolower($1); exit } }' "$work/SHA256SUMS.txt")"
    case "$want" in *[!0-9a-f]*|"") die "SHA256SUMS.txt has no valid checksum for $asset." ;; esac
    [ "${#want}" = 64 ] || die "SHA256SUMS.txt has no valid checksum for $asset."
    say "Downloading $asset..."
    fetch "$base/$asset" "$work/$name" || die "Could not download $asset."
    [ "$(hash_of "$work/$name")" = "$want" ] || die "$asset did not match its checksum, so nothing was installed."
done
say "Checksums match."

mkdir -p "$dir"
# A running program is stopped first, so the files can be replaced.
if has acpd && [ -x "$dir/acpd" ]; then "$dir/acpd" daemon stop >/dev/null 2>&1 || true; fi
if has acpd-tray; then pkill -x acpd-tray >/dev/null 2>&1 || true; fi
for name in $names; do
    chmod 755 "$work/$name"
    mv -f "$work/$name" "$dir/$name"
done
say "Installed $(echo $names | sed 's/ / and /') in $dir"

if [ "$LINKS" = 1 ]; then
    mkdir -p "$bin_links"
    for name in $names; do
        if [ -e "$bin_links/$name" ] && [ ! -L "$bin_links/$name" ]; then
            say "Left $bin_links/$name alone (it is not a link)."
        else
            ln -sf "$dir/$name" "$bin_links/$name"
        fi
    done
    case ":$PATH:" in *":$bin_links:"*) ;; *) say "Add $bin_links to your PATH to run $(echo $names | sed 's/ / and /') from anywhere." ;; esac
fi

if [ "$DESKTOP" = 1 ] && has acpd-tray; then
    mkdir -p "$data_home/applications" "$(dirname "$icon_file")"
    # The icon is optional: only keep it if what came back is an SVG.
    if fetch "https://raw.githubusercontent.com/$REPO/main/assets/logo/logo-dark.svg" "$work/icon.svg" 2>/dev/null && head -c 200 "$work/icon.svg" | grep -q '<svg'; then
        mv -f "$work/icon.svg" "$icon_file"
        icon="acp-portal"
    else
        icon="utilities-terminal"
    fi
    cat > "$desktop_file" <<EOF
[Desktop Entry]
Type=Application
Name=ACP Portal
Comment=Set up, start and pair the ACP Portal host
Exec=$dir/acpd-tray
Icon=$icon
Terminal=false
Categories=Development;Utility;
EOF
    say "Added a desktop entry: ACP Portal."
fi

if has acpd-tray && command -v ldconfig >/dev/null 2>&1 && ! ldconfig -p 2>/dev/null | grep -Eq 'libayatana-appindicator3|libappindicator3'; then
    say "Note: the tray needs the AppIndicator and GTK 3 libraries (Debian/Ubuntu: libayatana-appindicator3-1 libgtk-3-0 libxdo3)."
fi

echo
echo "Done. Next:"
case "$ONLY" in
    host)
        echo "  acpd setup --address https://YOUR-HOST ; acpd start --bg ; acpd pair" ;;
    tray)
        echo "  1. Start ACP Portal from your applications menu (or run acpd-tray)."
        echo "  2. In its menu: Download host (acpd)... (unless acpd is already installed), Set up with Tailscale, Start host, Pair a phone." ;;
    *)
        echo "  1. Start ACP Portal from your applications menu (or run acpd-tray)."
        echo "  2. In its menu: Set up with Tailscale, Start host, Pair a phone."
        echo "  Or use the command line: acpd setup --address https://YOUR-HOST ; acpd start --bg ; acpd pair" ;;
esac
