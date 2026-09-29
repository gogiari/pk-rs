#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
version=$(awk -F '"' '/^version = / { print $2; exit }' "$repo_dir/Cargo.toml")
arch=$(uname -m)
if [[ "$arch" != x86_64 ]]; then
    echo "This build currently supports x86_64 Linux only (found $arch)." >&2
    exit 1
fi
for tool in npm cargo rustup nfpm cc tar; do
    command -v "$tool" >/dev/null || { echo "Missing build tool: $tool" >&2; exit 1; }
done

work_dir=$(mktemp -d)
trap 'rm -rf -- "$work_dir"' EXIT
web_build="$work_dir/web"
mkdir -p "$web_build"
cp -a "$repo_dir/web/package.json" "$repo_dir/web/package-lock.json" \
    "$repo_dir/web/tsconfig.json" "$repo_dir/web/vite.config.ts" \
    "$repo_dir/web/index.html" "$repo_dir/web/src" "$web_build/"

echo 'Building Linux frontend...'
(cd "$web_build" && npm ci && npm run build)
mkdir -p "$repo_dir/web/dist"
cp -a "$web_build/dist/." "$repo_dir/web/dist/"

echo 'Building static Linux executable...'
rustup target add x86_64-unknown-linux-musl
export CARGO_TARGET_DIR="${PK_LINUX_TARGET_DIR:-$HOME/.cache/pk-rs/cargo-target}"
(cd "$repo_dir" && cargo build --release --bin pk --target x86_64-unknown-linux-musl)
binary="$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/release/pk"
output_dir="$repo_dir/target/linux-dist"
mkdir -p "$output_dir"

aliases=(codex-proxy grok-proxy claude-proxy agy-proxy ocx-proxy opencodex-proxy)
desktop="$repo_dir/packaging/linux/pk.desktop"
icon="$repo_dir/web/src/assets/rocket-mark.png"

echo 'Building distro packages...'
package_config="$work_dir/nfpm.yaml"
cat > "$package_config" <<EOF
name: pk-proxy-manager
arch: amd64
platform: linux
version: $version
maintainer: PK Proxy Manager <local@localhost>
description: Local SSH tunnel and HTTP proxy manager with a web dashboard.
license: Unspecified
contents:
  - src: $binary
    dst: /usr/bin/pk
    file_info:
      mode: 0755
  - src: $desktop
    dst: /usr/share/applications/pk.desktop
    file_info:
      mode: 0644
  - src: $icon
    dst: /usr/share/pixmaps/pk.png
    file_info:
      mode: 0644
EOF
for alias in "${aliases[@]}"; do
    cat >> "$package_config" <<EOF
  - src: /usr/bin/pk
    dst: /usr/bin/$alias
    type: symlink
EOF
done
cat >> "$package_config" <<'EOF'
overrides:
  deb:
    depends: [openssh-client]
  rpm:
    depends: [openssh-clients]
  apk:
    depends: [openssh-client]
  archlinux:
    depends: [openssh]
EOF
nfpm package -f "$package_config" -p deb -t "$output_dir/pk-proxy-manager_${version}_amd64.deb"
nfpm package -f "$package_config" -p rpm -t "$output_dir/pk-proxy-manager-${version}-1.x86_64.rpm"
nfpm package -f "$package_config" -p apk -t "$output_dir/pk-proxy-manager_${version}_x86_64.apk"
nfpm package -f "$package_config" -p archlinux -t "$output_dir/pk-proxy-manager-${version}-1-x86_64.pkg.tar.zst"

echo 'Building generic Linux archive...'
bundle="$work_dir/pk-linux-x86_64"
mkdir -p "$bundle"
install -m 755 "$binary" "$bundle/pk"
install -m 755 "$repo_dir/packaging/linux/install.sh" "$bundle/install.sh"
install -m 755 "$repo_dir/packaging/linux/uninstall.sh" "$bundle/uninstall.sh"
install -m 644 "$icon" "$bundle/pk.png"
cat > "$bundle/README.txt" <<EOF
PK Proxy Manager $version for x86_64 Linux

Run ./install.sh to install into your user account, then launch PK Proxy Manager
from your desktop menu or run ~/.local/bin/pk ui.
An OpenSSH client (ssh command) is required for tunnels.
Run ./uninstall.sh to remove the user installation.
EOF
tar -C "$work_dir" -czf "$output_dir/pk-proxy-manager_${version}_linux-x86_64.tar.gz" pk-linux-x86_64

echo "Linux packages are ready in $output_dir"
