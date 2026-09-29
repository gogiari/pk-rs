#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
version=$(awk -F '"' '/^version = / { print $2; exit }' "$repo_dir/Cargo.toml")
if [[ $(uname -s) != Darwin ]]; then
    echo 'Build the macOS package on a Mac with Xcode Command Line Tools.' >&2
    exit 1
fi
for tool in npm cargo rustup lipo pkgbuild python3 sips iconutil; do
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

output_dir="$repo_dir/target/macos-dist"
mkdir -p "$output_dir"
stage="$work_dir/stage"
app="$stage/Applications/PK Proxy Manager.app"
binary="$app/Contents/MacOS/pk"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$stage/usr/local/bin"
lipo -create \
    "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/pk" \
    "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/pk" \
    -output "$binary"
chmod 755 "$binary"

python3 - "$app/Contents/Info.plist" "$version" <<'PY'
import plistlib
import sys

with open(sys.argv[1], 'wb') as output:
    plistlib.dump({
        'CFBundleDevelopmentRegion': 'ko',
        'CFBundleDisplayName': 'PK Proxy Manager',
        'CFBundleExecutable': 'pk',
        'CFBundleIconFile': 'PK',
        'CFBundleIdentifier': 'dev.pk-proxy-manager.app',
        'CFBundleInfoDictionaryVersion': '6.0',
        'CFBundleName': 'PK Proxy Manager',
        'CFBundlePackageType': 'APPL',
        'CFBundleShortVersionString': sys.argv[2],
        'CFBundleVersion': sys.argv[2],
        'LSMinimumSystemVersion': '11.0',
    }, output)
PY

icon_source="$repo_dir/web/src/assets/rocket-mark.png"
iconset="$work_dir/PK.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$icon_source" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    doubled=$((size * 2))
    sips -z "$doubled" "$doubled" "$icon_source" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/PK.icns"

install -m 755 "$repo_dir/packaging/macos/uninstall.sh" \
    "$stage/Applications/Uninstall PK Proxy Manager.command"
for name in pk codex-proxy grok-proxy claude-proxy agy-proxy ocx-proxy opencodex-proxy; do
    ln -s '../../../Applications/PK Proxy Manager.app/Contents/MacOS/pk' "$stage/usr/local/bin/$name"
done

if [[ -n ${PK_MACOS_APP_SIGN_IDENTITY:-} ]]; then
    codesign --force --timestamp --options runtime --sign "$PK_MACOS_APP_SIGN_IDENTITY" "$app"
    codesign --verify --verbose "$app"
fi

package="$output_dir/pk-proxy-manager_${version}_macos-universal.pkg"
pkg_args=(--root "$stage" --identifier dev.pk-proxy-manager.pkg
    --version "$version" --install-location / --ownership recommended)
if [[ -n ${PK_MACOS_INSTALLER_SIGN_IDENTITY:-} ]]; then
    pkg_args+=(--sign "$PK_MACOS_INSTALLER_SIGN_IDENTITY")
fi
pkgbuild "${pkg_args[@]}" "$package"
if [[ -n ${PK_MACOS_NOTARY_PROFILE:-} ]]; then
    if [[ -z ${PK_MACOS_APP_SIGN_IDENTITY:-} || -z ${PK_MACOS_INSTALLER_SIGN_IDENTITY:-} ]]; then
        echo 'Notarization requires both app and installer signing identities.' >&2
        exit 1
    fi
    xcrun notarytool submit "$package" --keychain-profile "$PK_MACOS_NOTARY_PROFILE" --wait
    xcrun stapler staple "$package"
fi
echo "macOS installer: $package"
