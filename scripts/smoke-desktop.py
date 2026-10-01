"""App-scoped launcher smoke test on Windows, with optional real WSL interop.

python scripts/smoke-desktop.py target/release/pk.exe [Ubuntu]
Build npm/binaries/linux-x64/pk first to include the WSL case.
No desktop login, SSH credentials, or system proxy settings are used.
"""
import json
import hashlib
import os
from pathlib import Path
import shutil
import socketserver
import subprocess
import sys
import tempfile
import threading
import time


class Socks(socketserver.BaseRequestHandler):
    def handle(self):
        self.request.recv(1024)
        self.request.sendall(b"\x05\x00")


class Http(socketserver.BaseRequestHandler):
    def handle(self):
        data = b""
        while b"\r\n\r\n" not in data:
            chunk = self.request.recv(1024)
            if not chunk:
                return  # launcher's port readiness check
            data += chunk
        assert data.startswith(b"GET http://example.com/ HTTP/1.1\r\n"), data
        self.request.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")


def run(*args, env=None):
    result = subprocess.run(args, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)
    if result.returncode:
        raise RuntimeError(f"{args}: {result.stdout}\n{result.stderr}")
    return result.stdout


def wsl_path(path, distro):
    return run("wsl.exe", "-d", distro, "--exec", "wslpath", "-a", "-u", str(path)).strip()


def source():
    return r'''
#![cfg_attr(windows, windows_subsystem="windows")]
use std::{env,fs,net::TcpStream,io::{Read,Write},time::Duration};
fn main() {
    let proxy=env::var("HTTP_PROXY").unwrap();
    assert_eq!(proxy,env::var("HTTPS_PROXY").unwrap());
    assert_eq!(proxy,env::var("http_proxy").unwrap());
    assert_eq!(proxy,env::var("https_proxy").unwrap());
    assert!(env::var("ALL_PROXY").is_err());
    assert!(env::var("all_proxy").is_err());
    let args=env::args().skip(1).collect::<Vec<_>>();
    assert!(args.contains(&format!("--proxy-server={}",proxy)));
    assert!(args.iter().any(|a|a.starts_with("--user-data-dir=")));
    assert!(args.iter().any(|a|a.starts_with("--proxy-bypass-list=")));
    let mut socket=TcpStream::connect(proxy.strip_prefix("http://").unwrap()).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    socket.write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n").unwrap();
    let mut response=String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    let home=env::var("CODEX_HOME").unwrap();
    assert!(std::path::Path::new(&home).is_dir());
    let report=env::current_exe().unwrap().parent().unwrap().join(if cfg!(windows) {"windows.json"} else {"wsl.json"});
    fs::write(report,format!("{{\"proxy\":{:?},\"codex_home\":{:?}}}",proxy,home)).unwrap();
    std::thread::sleep(Duration::from_secs(3));
}
'''


def main():
    if os.name != "nt":
        raise SystemExit("Run this Windows-to-WSL test from Windows.")
    binary = Path(sys.argv[1]).resolve()
    distro = sys.argv[2] if len(sys.argv) > 2 else None
    original_proxy = {key: os.environ.get(key) for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY")}
    servers = [socketserver.ThreadingTCPServer(("127.0.0.1", 0), cls) for cls in (Socks, Http)]
    for server in servers:
        server.daemon_threads = True
        threading.Thread(target=server.serve_forever, daemon=True).start()
    # Keep artifacts available for diagnosing a failed test; never delete arbitrary paths.
    root = Path(tempfile.mkdtemp(prefix="pk-desktop-smoke-한글 "))
    environment = {**os.environ, "PK_CONFIG_DIR": str(root), "ALL_PROXY": "sentinel", "all_proxy": "sentinel"}
    (root / "config.toml").write_text(f"http_port = {servers[1].server_address[1]}\nsocks_port = {servers[0].server_address[1]}\nauto_connect = false\n", encoding="utf-8")
    compiler = shutil.which("rustc") or str(Path.home() / ".cargo/bin/rustc.exe")
    code_text = source()
    fixture_key = hashlib.sha256(code_text.encode()).hexdigest()[:16]
    cache = Path(__file__).resolve().parents[1] / "target/desktop-fixture" / fixture_key
    cache.mkdir(parents=True, exist_ok=True)
    code = cache / "fixture.rs"
    code.write_text(code_text, encoding="utf-8")
    try:
        for target in (["windows", "wsl"] if distro else ["windows"]):
            report = root / f"{target}.json"
            app = root / ("fake.exe" if target == "windows" else "fake-linux")
            if target == "windows":
                cached_app = cache / "windows.exe"
                if not cached_app.exists():
                    run(compiler, "--edition", "2021", str(code), "-o", str(cached_app))
                shutil.copyfile(cached_app, app)
                arguments = [str(binary), "desktop", "windows", "--path", str(app)]
            else:
                # Link on the native Linux filesystem; linking on DrvFS is slow
                # under concurrent builds. The copied executable still exercises
                # Unicode and spaces in the actual app launch path.
                home = run("wsl.exe", "-d", distro, "--exec", "printenv", "HOME").strip()
                native_cache = f"{home}/.cache/pk-desktop-fixture/{fixture_key}"
                run("wsl.exe", "-d", distro, "--exec", "mkdir", "-p", native_cache)
                check = subprocess.run(["wsl.exe", "-d", distro, "--exec", "test", "-x", native_cache + "/fake"], timeout=30)
                if check.returncode:
                    run("wsl.exe", "-d", distro, "--exec", home + "/.cargo/bin/rustc", "--edition", "2021", wsl_path(code, distro), "-o", native_cache + "/fake")
                run("wsl.exe", "-d", distro, "--exec", "cp", native_cache + "/fake", wsl_path(app, distro))
                arguments = [str(binary), "desktop", "wsl", "--distro", distro, "--path", wsl_path(app, distro)]
            run(*arguments, env=environment)
            deadline = time.monotonic() + 60
            while not report.exists() and time.monotonic() < deadline:
                time.sleep(0.1)
            assert report.exists(), (root / "desktop/desktop.log").read_text(encoding="utf-8", errors="replace")
            result = json.loads(report.read_text(encoding="utf-8"))
            assert result["proxy"].startswith("http://127.0.0.1:")
            assert result["proxy"] != f"http://127.0.0.1:{servers[1].server_address[1]}"
            print(f"PASS {target}: separate app proxy, env, profile, and HTTP response through PK")
        assert original_proxy == {key: os.environ.get(key) for key in original_proxy}
        print("PASS: parent proxy environment unchanged")
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()
        print(f"Reports: {root}")


if __name__ == "__main__":
    main()
