#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
arch=$(uname -m)
if [[ "$arch" != x86_64 ]]; then
    echo "This build currently supports x86_64 Linux only (found $arch)." >&2
    exit 1
fi
for tool in npm cargo rustup cc; do
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
output_dir="$repo_dir/target/npm-assets"
npm_binary_dir="$repo_dir/npm/binaries/linux-x64"
mkdir -p "$output_dir" "$npm_binary_dir"
install -m 755 "$binary" "$output_dir/npm-pk-linux-x64"
install -m 755 "$binary" "$npm_binary_dir/pk"

echo "npm executable: $npm_binary_dir/pk"
