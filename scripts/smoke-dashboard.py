"""Run the packaged executable with isolated settings and verify its local dashboard."""

import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
from urllib.error import URLError
from urllib.request import urlopen


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


binary = Path(sys.argv[1]).resolve()
assert binary.is_file(), f"Missing executable: {binary}"
config_dir = Path(tempfile.mkdtemp(prefix="pk-smoke-")).resolve()
assert config_dir.is_relative_to(Path(tempfile.gettempdir()).resolve())
web_port, http_port, socks_port = (free_port() for _ in range(3))
(config_dir / "config.toml").write_text(
    'ssh_target = "test@example.com"\n'
    f"web_port = {web_port}\nhttp_port = {http_port}\nsocks_port = {socks_port}\n"
    "auto_connect = false\nauto_open_browser = false\n",
    encoding="utf-8",
)
environment = os.environ.copy()
environment["PK_CONFIG_DIR"] = str(config_dir)
process = subprocess.Popen(
    [str(binary), "start", "--foreground"],
    env=environment,
    stdin=subprocess.DEVNULL,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
)
try:
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        try:
            with urlopen(f"http://127.0.0.1:{web_port}/api/status", timeout=1) as response:
                assert response.status == 200
                assert json.load(response)["ssh_target"] == "test@example.com"
                break
        except (URLError, TimeoutError):
            if process.poll() is not None:
                raise RuntimeError("The dashboard exited before becoming ready")
            time.sleep(0.2)
    else:
        raise RuntimeError("The dashboard did not become ready")

    subprocess.run(
        [str(binary), "stop"],
        env=environment,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        timeout=12,
        check=True,
    )
    process.wait(timeout=5)
    print(f"Dashboard HTTP 200 and clean stop on port {web_port}")
finally:
    if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
    for filename in ("config.toml", "pk.pid", "pk.log"):
        (config_dir / filename).unlink(missing_ok=True)
    config_dir.rmdir()
