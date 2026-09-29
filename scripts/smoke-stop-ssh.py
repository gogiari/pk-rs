"""Check that stopping PK also terminates its SSH child (Unix only)."""

import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time


assert os.name == "posix"
binary = Path(sys.argv[1]).resolve()
work_dir = Path(tempfile.mkdtemp(prefix="pk-ssh-smoke-")).resolve()
assert work_dir.is_relative_to(Path(tempfile.gettempdir()).resolve())
fake_ssh = work_dir / "ssh"
fake_ssh.write_text('#!/bin/sh\necho $$ > "$PK_TEST_SSH_PID"\nexec sleep 120\n')
fake_ssh.chmod(0o755)
ssh_pid_file = work_dir / "ssh.pid"


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


web_port, http_port, socks_port = (free_port() for _ in range(3))
(work_dir / "config.toml").write_text(
    'ssh_target = "test@example.com"\n'
    f"web_port = {web_port}\nhttp_port = {http_port}\nsocks_port = {socks_port}\n"
    "auto_connect = true\nauto_open_browser = false\n",
    encoding="utf-8",
)
environment = os.environ.copy()
environment.update(
    PK_CONFIG_DIR=str(work_dir),
    PK_TEST_SSH_PID=str(ssh_pid_file),
    PATH=str(work_dir) + os.pathsep + environment["PATH"],
)
daemon = subprocess.Popen(
    [str(binary), "start", "--foreground"],
    env=environment,
    stdin=subprocess.DEVNULL,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
ssh_pid = None
try:
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline and not ssh_pid_file.exists():
        if daemon.poll() is not None:
            raise RuntimeError("PK exited before starting SSH")
        time.sleep(0.1)
    if not ssh_pid_file.exists():
        raise RuntimeError("SSH child did not start")
    ssh_pid = int(ssh_pid_file.read_text())
    subprocess.run([str(binary), "stop"], env=environment, check=True, stdout=subprocess.DEVNULL, timeout=12)
    daemon.wait(timeout=5)
    try:
        os.kill(ssh_pid, 0)
    except ProcessLookupError:
        print("SSH child ended with PK")
    else:
        raise RuntimeError(f"SSH child remains after pk stop: PID {ssh_pid}")
finally:
    if ssh_pid is not None:
        try:
            os.kill(ssh_pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if daemon.poll() is None:
        daemon.kill()
    daemon.wait(timeout=5)
    for name in ("ssh", "ssh.pid", "config.toml", "pk.pid", "pk.log"):
        (work_dir / name).unlink(missing_ok=True)
    work_dir.rmdir()
