#!/bin/sh
# Removes ACP Portal (the acpd host and the tray app) from this Linux account.
#
# Stops the host and the tray, deletes the programs, the links in ~/.local/bin, the desktop entry and
# icon, and any leftover pairing page. By default your settings and paired devices are KEPT, so
# installing again picks up where you left off. Add --purge to delete those too.
#
# One line:
#   curl -fsSL https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/uninstall.sh | sh
# With options:
#   curl -fsSL https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/uninstall.sh | sh -s -- --purge
#
# Not removed (they belong to other programs): the phone app, and Tailscale Serve. To stop publishing
# the host over Tailscale run:  tailscale serve reset   (this clears ALL Serve settings).
#
# Options:  --purge   also delete your settings (config.toml, agents.json, launcher.json) and the data
#                     folder (paired devices, saved sessions, folders and agents added from the phone)
#           --yes     do not ask for confirmation before --purge deletes anything
#           --only host|tray   remove just the host or just the tray (default: both).
#                              Cannot be combined with --purge, which removes everything.
# Environment: ACPD_ONLY, ACPD_INSTALL_DIR (default ${XDG_DATA_HOME:-~/.local/share}/acpd/bin)

set -eu

PURGE=0
YES=0
ONLY="${ACPD_ONLY:-both}"
while [ $# -gt 0 ]; do
    case "$1" in
        --purge) PURGE=1; shift ;;
        --yes|-y) YES=1; shift ;;
        --only) [ $# -ge 2 ] || { echo "--only needs host or tray" >&2; exit 2; }; ONLY="$2"; shift 2 ;;
        -h|--help) sed -n '2,21p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1 (try --help)" >&2; exit 2 ;;
    esac
done

say() { printf '  %s\n' "$*"; }

case "$ONLY" in
    host) names="acpd"; label="the acpd host" ;;
    tray) names="acpd-tray"; label="the tray app" ;;
    both) names="acpd acpd-tray"; label="ACP Portal (host and tray)" ;;
    *) echo "--only must be host or tray (or leave it out for both), got: $ONLY" >&2; exit 2 ;;
esac
has() { case " $names " in *" $1 "*) return 0 ;; esac; return 1; }
if [ "$PURGE" = 1 ] && [ "$ONLY" != both ]; then
    echo "--purge removes everything, so it cannot be combined with --only. Run it without --only." >&2
    exit 2
fi

# The same folders the host and the tray use.
config_home="${XDG_CONFIG_HOME:-$HOME/.config}"
data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
settings_dir="$config_home/acpd"
data_dir="$data_home/acpd"
dir="${ACPD_INSTALL_DIR:-$data_dir/bin}"
bin_links="$HOME/.local/bin"
desktop_file="$data_home/applications/acp-portal.desktop"
icon_file="$data_home/icons/hicolor/scalable/apps/acp-portal.svg"

echo "Uninstalling $label"
if [ "$PURGE" = 1 ]; then
    echo
    echo "  --purge will also delete:"
    echo "    $settings_dir   (settings, agent list)"
    echo "    $data_dir   (paired devices, saved sessions, folders and agents added from the phone)"
    if [ "$YES" != 1 ]; then
        # When piped from curl, stdin is the script, so ask on the terminal.
        # (A failed redirect on a special builtin ends the shell, so the check runs in a subshell.)
        if [ -r /dev/tty ] && ( : < /dev/tty ) 2>/dev/null; then
            printf '  Type DELETE to continue: '
            read -r answer < /dev/tty || answer=""
        else
            echo "Refusing to purge without confirmation. Run it from a terminal, or add --yes." >&2
            exit 1
        fi
        if [ "$answer" != "DELETE" ]; then echo "  Cancelled. Nothing was removed."; exit 0; fi
    fi
    echo
fi

# Stop the host and the tray first.
if has acpd && [ -x "$dir/acpd" ]; then "$dir/acpd" daemon stop >/dev/null 2>&1 || true; fi
if has acpd-tray; then pkill -x acpd-tray >/dev/null 2>&1 || true; fi

removed=0
for name in $names; do
    # Only remove a link that points at our install folder, never someone else's file.
    if [ -L "$bin_links/$name" ] && [ "$(readlink "$bin_links/$name")" = "$dir/$name" ]; then rm -f "$bin_links/$name"; fi
    if [ -e "$dir/$name" ]; then rm -f "$dir/$name"; removed=$((removed + 1)); fi
done
if [ "$removed" -gt 0 ]; then say "Removed $removed program file(s) from $dir"; else say "No programs found in $dir"; fi
rmdir "$dir" 2>/dev/null || true

if has acpd-tray; then
    if [ -e "$desktop_file" ] || [ -e "$icon_file" ]; then
        rm -f "$desktop_file" "$icon_file"
        say "Removed the desktop entry and icon."
    fi
    rm -rf "${TMPDIR:-/tmp}"/acpd-pair-* 2>/dev/null || true
fi

if [ "$PURGE" = 1 ]; then
    for folder in "$settings_dir" "$data_dir"; do
        # Refuse anything that is not a folder literally named acpd, whatever the environment says.
        case "$folder" in */acpd) ;; *) echo "Not deleting $folder (it is not an acpd folder)." >&2; continue ;; esac
        if [ -d "$folder" ]; then rm -rf "$folder"; say "Deleted $folder"; fi
    done
    echo
    echo "Everything was removed. Paired phones must pair again after a reinstall."
else
    echo
    echo "Your settings and paired devices were kept:"
    echo "  $settings_dir"
    echo "  $data_dir/state"
    echo "Delete them too with:  sh uninstall.sh --purge"
fi
echo "To stop publishing the host over Tailscale (this clears ALL Serve settings): tailscale serve reset"
