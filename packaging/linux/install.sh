#!/bin/sh
set -eu

bundle_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
app_dir="$data_dir/applications"
icon_dir="$data_dir/icons"
marker_dir="$data_dir/pk-proxy-manager"
marker="$marker_dir/installed"

if [ -e "$bin_dir/pk" ] && [ ! -f "$marker" ]; then
    printf 'Refusing to replace an existing %s without an installation marker.\n' "$bin_dir/pk" >&2
    exit 1
fi
for alias in codex-proxy grok-proxy claude-proxy agy-proxy ocx-proxy opencodex-proxy; do
    if [ -e "$bin_dir/$alias" ] || [ -L "$bin_dir/$alias" ]; then
        if [ ! -L "$bin_dir/$alias" ] || [ "$(readlink "$bin_dir/$alias")" != pk ]; then
            printf 'Refusing to replace an existing %s.\n' "$bin_dir/$alias" >&2
            exit 1
        fi
    fi
done
if [ ! -f "$marker" ] && { [ -e "$app_dir/pk.desktop" ] || [ -e "$icon_dir/pk.png" ]; }; then
    printf 'Refusing to replace an existing PK desktop entry or icon.\n' >&2
    exit 1
fi

mkdir -p "$bin_dir" "$app_dir" "$icon_dir" "$marker_dir"
install -m 755 "$bundle_dir/pk" "$bin_dir/pk"
install -m 644 "$bundle_dir/pk.png" "$icon_dir/pk.png"

for alias in codex-proxy grok-proxy claude-proxy agy-proxy ocx-proxy opencodex-proxy; do
    ln -sfn pk "$bin_dir/$alias"
done

cat > "$app_dir/pk.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=PK Proxy Manager
Comment=Local SSH tunnel and HTTP proxy dashboard
Exec="$bin_dir/pk" ui
Icon=$icon_dir/pk.png
Terminal=false
Categories=Network;Utility;
EOF
printf '%s\n' 'pk-proxy-manager' > "$marker"

printf 'Installed PK Proxy Manager to %s\n' "$bin_dir"
case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) printf 'Add %s to PATH to use pk in a new terminal.\n' "$bin_dir" ;;
esac
