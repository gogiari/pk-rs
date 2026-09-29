#!/bin/bash
set -euo pipefail

app='/Applications/PK Proxy Manager.app'
binary="$app/Contents/MacOS/pk"
bin_dir='/usr/local/bin'
receipt='dev.pk-proxy-manager.pkg'

if [[ ! -f "$binary" ]]; then
    echo "PK Proxy Manager app was not found at $app" >&2
    exit 1
fi
bundle_id=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$app/Contents/Info.plist" 2>/dev/null || true)
if [[ "$bundle_id" != dev.pk-proxy-manager.app ]]; then
    echo 'The app at the installation path does not match PK Proxy Manager.' >&2
    exit 1
fi

# Do not remove an executable while its daemon is still running.
pid_file="${PK_CONFIG_DIR:-$HOME/.config/pk}/pk.pid"
if [[ -f "$pid_file" ]]; then
    pid=$(cat "$pid_file")
    if [[ "$pid" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null; then
        echo 'PK Proxy Manager is running. Run pk stop, then run this uninstaller again.' >&2
        exit 1
    fi
fi

echo 'Removing PK Proxy Manager. macOS may ask for an administrator password.'
sudo /bin/bash -s -- "$app" "$bin_dir" "$receipt" <<'ROOT_SCRIPT'
set -euo pipefail
app=$1
bin_dir=$2
receipt=$3
binary="$app/Contents/MacOS/pk"
for name in pk codex-proxy grok-proxy claude-proxy agy-proxy ocx-proxy opencodex-proxy; do
    link="$bin_dir/$name"
    if [[ -L "$link" && $(readlink "$link") == '../../../Applications/PK Proxy Manager.app/Contents/MacOS/pk' ]]; then
        rm -- "$link"
    fi
done
rm -rf -- "$app"
rm -f -- '/Applications/Uninstall PK Proxy Manager.command'
pkgutil --forget "$receipt" >/dev/null 2>&1 || true
ROOT_SCRIPT
echo 'PK Proxy Manager was removed. Personal settings in ~/.config/pk were kept.'
