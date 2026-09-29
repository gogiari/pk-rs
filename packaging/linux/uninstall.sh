#!/bin/sh
set -eu

bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
marker_dir="$data_dir/pk-proxy-manager"
marker="$marker_dir/installed"

if [ ! -f "$marker" ]; then
    printf 'No PK Proxy Manager user installation marker found.\n' >&2
    exit 1
fi
if [ -n "${PK_CONFIG_DIR:-}" ]; then
    pid_file="$PK_CONFIG_DIR/pk.pid"
elif [ -n "${APPDATA:-}" ]; then
    pid_file="$APPDATA/pk/pk.pid"
else
    pid_file="$HOME/.config/pk/pk.pid"
fi
if [ -f "$pid_file" ]; then
    pid=$(cat "$pid_file")
    case "$pid" in
        ''|*[!0-9]*) ;;
        *) if kill -0 "$pid" 2>/dev/null; then
            printf 'PK Proxy Manager is running. Stop it before uninstalling.\n' >&2
            exit 1
        fi ;;
    esac
fi
for name in codex-proxy grok-proxy claude-proxy agy-proxy ocx-proxy opencodex-proxy; do
    if [ -L "$bin_dir/$name" ] && [ "$(readlink "$bin_dir/$name")" = pk ]; then
        rm -f -- "$bin_dir/$name"
    fi
done
rm -f -- "$bin_dir/pk"
rm -f -- "$data_dir/applications/pk.desktop" "$data_dir/icons/pk.png"
rm -f -- "$marker"
rmdir "$marker_dir" 2>/dev/null || true
printf 'Removed PK Proxy Manager user installation.\n'
