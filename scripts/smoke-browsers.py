"""Exercise browser CLI/API with isolated settings, a fake executable and SOCKS server.

Usage: python scripts/smoke-browsers.py target/debug/pk.exe
Requires rustc to compile the argv-capturing browser fixture.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix="pk-browser-smoke-") as temporary:
    root = Path(temporary)
    fixture_dir = root / "custom browser 한글"
    fixture_dir.mkdir()
    fixture = fixture_dir / ("browser.exe" if os.name == "nt" else "browser")
    source = root / "fixture.rs"
    source.write_text('''fn main() {
        let args = std::env::args().skip(1).collect::<Vec<_>>().join("\\n");
        std::fs::write(std::env::var_os("PK_BROWSER_CAPTURE").unwrap(), args).unwrap();
        let code = std::env::var("PK_BROWSER_EXIT").unwrap_or_default().parse().unwrap_or(0);
        std::process::exit(code);
    }''', encoding="utf-8")
    subprocess.run(["rustc", "--crate-name", "browser_fixture", str(source), "-o", str(fixture)], check=True)
    config_dir = root / "settings"
    config_dir.mkdir()
    web_port, http_port = free_port(), free_port()
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen()
    listener.settimeout(0.2)
    socks_port = listener.getsockname()[1]
    ready = threading.Event()
    ready.set()
    stopping = threading.Event()

    def serve():
        while not stopping.is_set():
            try:
                stream, _ = listener.accept()
            except socket.timeout:
                continue
            with stream:
                stream.settimeout(1)
                try:
                    greeting = stream.recv(3)
                    if greeting == bytes([5, 1, 0]):
                        stream.sendall(bytes([5, 0 if ready.is_set() else 255]))
                except (TimeoutError, OSError):
                    pass

    worker = threading.Thread(target=serve, daemon=True)
    worker.start()
    capture = root / "argv.txt"
    environment = os.environ.copy()
    environment.update(PK_CONFIG_DIR=str(config_dir), PK_BROWSER_CAPTURE=str(capture))
    (config_dir / "config.toml").write_text(
        f'web_port = {web_port}\nhttp_port = {http_port}\nsocks_port = {socks_port}\n'
        'auto_connect = false\nauto_open_browser = false\n', encoding="utf-8")

    def cli(*args, success=True):
        result = subprocess.run([str(binary), "browser", *args], env=environment, capture_output=True, text=True, encoding="utf-8", timeout=20)
        assert (result.returncode == 0) == success, result.stderr
        return result.stdout

    def api(path, body=None, status=200, origin=None):
        headers = {"Content-Type": "application/json"}
        if origin:
            headers["Origin"] = origin
        request = Request(f"http://127.0.0.1:{web_port}{path}", data=json.dumps(body).encode() if body is not None else None, headers=headers)
        try:
            with urlopen(request, timeout=20) as response:
                assert response.status == status
                return json.load(response)
        except HTTPError as error:
            assert error.code == status, (error.code, error.read())
            return error.read().decode()

    process = subprocess.Popen([str(binary), "start", "--foreground"], env=environment, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    try:
        deadline = time.monotonic() + 15
        while True:
            try:
                api("/api/status")
                break
            except URLError:
                assert time.monotonic() < deadline and process.poll() is None
                time.sleep(0.2)
        initial = api("/api/browsers")
        system_default = api("/api/browser-default")
        assert system_default["preferred"] is None
        assert [item["kind"] for item in initial] == ["chrome", "edge", "firefox", "safari"]
        safari = initial[-1]
        assert safari["system_proxy"] is not None
        if sys.platform != "darwin":
            assert not safari["supported"] and not safari["available"]
            assert not safari["system_proxy"]["ready"]
            api("/api/browsers/safari/launch", {}, status=400)
            api("/api/browsers/safari/settings", {"launcher": {"mode": "executable", "path": str(fixture)}}, status=400)
            cli("safari", success=False)
        for kind in ("chrome", "edge", "firefox"):
            cli("set", kind, str(fixture))
            url = f"https://example.com/{kind}?a=1&b=2"
            cli(kind, url)
            args = capture.read_text(encoding="utf-8").splitlines()
            assert args[-1] == url
            if kind == "firefox":
                assert "--no-remote" in args
                profile = Path(args[args.index("--profile") + 1])
                prefs = (profile / "user.js").read_text()
                assert f'user_pref("network.proxy.socks_port", {socks_port});' in prefs
                assert 'user_pref("network.proxy.socks5_remote_dns", true);' in prefs
            else:
                assert f"--proxy-server=socks5://127.0.0.1:{socks_port}" in args
                profile = Path(next(arg.split("=", 1)[1] for arg in args if arg.startswith("--user-data-dir=")))
            assert profile == config_dir / "browser-profiles" / kind
            api(f"/api/browsers/{kind}/launch", {})
            assert capture.read_text(encoding="utf-8").splitlines()[-1] == "about:blank"
        assert all(item["saved"] and item["available"] for item in api("/api/browsers") if item["kind"] != "safari")
        # The OS default uses the manual path for that kind; never change OS settings.
        if system_default["effective"] in ("chrome", "edge", "firefox"):
            cli()
            args = capture.read_text(encoding="utf-8").splitlines()
            assert any(system_default["effective"] in arg for arg in args if "browser-profiles" in arg)
        elif sys.platform != "darwin":
            cli(success=False)
        api("/api/browser-default", {"browser": "edge"})
        assert api("/api/browser-default")["preferred"] == "edge"
        assert api("/api/browser-default")["effective"] == "edge"
        cli()
        args = capture.read_text(encoding="utf-8").splitlines()
        assert args[-1] == "about:blank" and any("edge" in arg for arg in args if "browser-profiles" in arg)
        default_url = "http://example.com/default?a=1&b=2"
        cli(default_url)
        assert capture.read_text(encoding="utf-8").splitlines()[-1] == default_url
        cli("firefox")
        assert "--no-remote" in capture.read_text(encoding="utf-8")
        result = api("/api/browser-default/launch", {"url": default_url})
        assert result["kind"] == "edge"
        assert capture.read_text(encoding="utf-8").splitlines()[-1] == default_url
        before_default = (config_dir / "config.toml").read_bytes()
        api("/api/browser-default", {"browser": "brave"}, status=422)
        api("/api/browser-default", {"browser": None}, status=403, origin="https://example.com")
        api("/api/browser-default/launch", {}, status=403, origin="https://example.com")
        api("/api/browser-default/launch", {"url": "--no-proxy-server"}, status=400)
        if sys.platform != "darwin":
            api("/api/browser-default", {"browser": "safari"}, status=400)
        assert (config_dir / "config.toml").read_bytes() == before_default
        before = (config_dir / "config.toml").read_bytes()
        api("/api/browsers/edge/settings", {"launcher": {"mode": "executable", "path": str(root / "missing.exe")}}, status=400)
        assert (config_dir / "config.toml").read_bytes() == before
        api("/api/browsers/edge/launch", {}, status=403, origin="https://example.com")
        api("/api/browsers/edge/launch", {"url": "--no-proxy-server"}, status=400)
        capture.unlink()
        ready.clear()
        api("/api/browsers/edge/launch", {}, status=400)
        cli("edge", success=False)
        cli(success=False)
        api("/api/browser-default/launch", {}, status=400)
        assert not capture.exists(), "A non-SOCKS listener must not launch the browser"
        ready.set()
        config = api("/api/config")
        api("/api/config", config)
        assert api("/api/browser-default")["preferred"] == "edge", "General settings must preserve default preference"
        assert all(item["saved"] for item in api("/api/browsers") if item["kind"] != "safari"), "General settings must preserve CLI browser settings"
        api("/api/browsers/edge/settings", {"launcher": None})
        assert next(item for item in api("/api/browsers") if item["kind"] == "edge")["saved"] is None
        cli("set", "edge", str(fixture))
        assert api("/api/browser-default")["preferred"] == "edge", "Browser path saves must preserve default preference"
        api("/api/browser-default", {"browser": None})
        assert api("/api/browser-default")["preferred"] is None
        assert api("/api/browser-default")["effective"] == system_default["effective"]
        cli("reset", "edge")
        cli("set", "edge", str(fixture))
        # An optional caller can exercise the live isolated dashboard.
        if len(sys.argv) > 2:
            subprocess.run(sys.argv[2:] + [f"http://127.0.0.1:{web_port}", str(fixture)], check=True, timeout=60)
        print("Browser CLI/API: custom Unicode paths, profiles, proxy ports, saved settings, reset, URL/origin checks and SOCKS failure passed")
    finally:
        subprocess.run([str(binary), "stop"], env=environment, capture_output=True, timeout=12)
        if process.poll() is None:
            process.kill()
        process.wait(timeout=5)
        stopping.set()
        worker.join(timeout=2)
        listener.close()
