#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
dist="$repo_dir/target/linux-dist"
version=$(awk -F '"' '/^version = / { print $2; exit }' "$repo_dir/Cargo.toml")
deb="$dist/pk-proxy-manager_${version}_amd64.deb"
archive="$dist/pk-proxy-manager_${version}_linux-x86_64.tar.gz"
test -f "$deb"
test -f "$archive"
test -f "$dist/pk-proxy-manager-${version}-1.x86_64.rpm"
test -f "$dist/pk-proxy-manager_${version}_x86_64.apk"
test -f "$dist/pk-proxy-manager-${version}-1-x86_64.pkg.tar.zst"

temp=$(mktemp -d /tmp/pk-linux-verify.XXXXXX)
pid=''
cleanup() {
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
    case "$temp" in /tmp/pk-linux-verify.*) rm -rf -- "$temp" ;; esac
}
trap cleanup EXIT

dpkg-deb -x "$deb" "$temp/deb"
test -x "$temp/deb/usr/bin/pk"
test "$(readlink "$temp/deb/usr/bin/codex-proxy")" = /usr/bin/pk
test "$(dpkg-deb -f "$deb" Architecture)" = amd64
test "$(dpkg-deb -f "$deb" Depends)" = openssh-client
"$temp/deb/usr/bin/pk" --help >/dev/null
file "$temp/deb/usr/bin/pk" | grep -q 'static-pie linked'

mkdir -p "$temp/home"
tar -xzf "$archive" -C "$temp"
HOME="$temp/home" sh "$temp/pk-linux-x86_64/install.sh"
test -x "$temp/home/.local/bin/pk"
test "$(readlink "$temp/home/.local/bin/codex-proxy")" = pk
test -f "$temp/home/.local/share/applications/pk.desktop"
HOME="$temp/home" sh "$temp/pk-linux-x86_64/uninstall.sh"
test ! -e "$temp/home/.local/bin/pk"
test ! -e "$temp/home/.local/share/applications/pk.desktop"

mkdir -p "$temp/config"
cat > "$temp/config/config.toml" <<EOF
ssh_target = "test@127.0.0.1"
http_port = 13129
socks_port = 11081
web_port = 18254
auto_connect = false
auto_open_browser = false
EOF
PK_CONFIG_DIR="$temp/config" "$temp/deb/usr/bin/pk" start -f > "$temp/service.log" 2>&1 &
pid=$!
ready=0
for _ in $(seq 1 40); do
    if curl -fsS http://127.0.0.1:18254/ -o "$temp/response.html" 2>/dev/null; then
        ready=1
        break
    fi
    sleep 0.2
done
test "$ready" -eq 1
grep -q '<html' "$temp/response.html"

echo 'Linux packages, user install, and dashboard smoke test passed.'
