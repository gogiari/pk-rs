#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [[ $(uname -s) != Darwin ]]; then
    echo 'Build the macOS executable on a Mac with Xcode Command Line Tools.' >&2
    exit 1
fi
for tool in npm cargo rustup lipo; do
    command -v "$tool" >/dev/null || { echo "Missing build tool: $tool" >&2; exit 1; }
done

work_dir=$(mktemp -d)
trap 'rm -rf -- "$work_dir"' EXIT
web_build="$work_dir/web"
mkdir -p "$web_build"
cp -R "$repo_dir/web/package.json" "$repo_dir/web/package-lock.json" \
    "$repo_dir/web/tsconfig.json" "$repo_dir/web/vite.config.ts" \
    "$repo_dir/web/index.html" "$repo_dir/web/src" "$web_build/"

echo 'Building frontend...'
(cd "$web_build" && npm ci && npm run build)
mkdir -p "$repo_dir/web/dist"
cp -R "$web_build/dist/." "$repo_dir/web/dist/"

echo 'Building Intel and Apple Silicon binaries...'
rustup target add x86_64-apple-darwin aarch64-apple-darwin
export CARGO_TARGET_DIR="${PK_MACOS_TARGET_DIR:-$repo_dir/target/macos-cargo}"
(cd "$repo_dir" && cargo build --release --bin pk --target x86_64-apple-darwin)
(cd "$repo_dir" && cargo build --release --bin pk --target aarch64-apple-darwin)

output_dir="$repo_dir/target/npm-assets"
npm_binary_dir="$repo_dir/npm/binaries/darwin-universal"
mkdir -p "$output_dir" "$npm_binary_dir"
binary="$output_dir/npm-pk-macos-universal"
lipo -create \
    "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/pk" \
    "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/pk" \
    -output "$binary"
chmod 755 "$binary"
install -m 755 "$binary" "$npm_binary_dir/pk"

echo "npm executable: $npm_binary_dir/pk"
