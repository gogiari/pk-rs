"""Run real WSL-to-Windows browser CLI/API tests without opening a real browser.

Run from Windows: python scripts/smoke-wsl-browsers.py <Linux pk path> [distro] [node UI-check.cjs]
Requires a built Linux pk executable in WSL and rustc on Windows.
"""
import os
import json
import shutil
import time
from pathlib import Path
import subprocess
import sys
import tempfile

assert os.name == "nt", "Run this driver from Windows; the browser fixture must be a Windows executable."
binary = sys.argv[1]
distro = sys.argv[2] if len(sys.argv) > 2 else "Ubuntu"
wsl = ["wsl.exe", "--distribution", distro, "--exec"]


def linux_path(path):
    return subprocess.check_output(wsl + ["wslpath", "-u", str(path)], text=True, encoding="utf-8").strip()


with tempfile.TemporaryDirectory(prefix="pk-wsl-browser-smoke-") as temporary:
    root = Path(temporary)
    fixture_dir = root / "custom browser 한글"
    fixture_dir.mkdir()
    source = root / "fixture.rs"
    fixture = fixture_dir / "browser.exe"
    source.write_text('''fn main() {
        let args = std::env::args().skip(1).collect::<Vec<_>>().join("\\n");
        std::fs::write(std::env::var_os("PK_BROWSER_CAPTURE").unwrap(), args).unwrap();
    }''', encoding="utf-8")
    subprocess.run(["rustc", "--crate-name", "wsl_browser_fixture", str(source), "-o", str(fixture)], check=True)
    smoke_script = Path(__file__).resolve().with_name("smoke-browsers.py")
    handoff = root / "ui-handoff.json"
    ui = len(sys.argv) > 3
    command = wsl + ["env", "PK_BROWSER_TEST_ROOT=" + linux_path(root),
        "PK_BROWSER_TEST_WINDOWS_EXE=" + linux_path(fixture),
        *(["PK_BROWSER_TEST_UI_HANDOFF=" + linux_path(handoff)] if ui else []),
        "python3", linux_path(smoke_script), binary]
    process = subprocess.Popen(command)
    try:
        if ui:
            deadline = time.monotonic() + 240
            while not handoff.exists():
                assert process.poll() is None and time.monotonic() < deadline, "WSL backend did not become ready for the UI driver"
                time.sleep(0.2)
            data = json.loads(handoff.read_text(encoding="utf-8"))
            result = subprocess.run([shutil.which(sys.argv[3]), str(Path(sys.argv[4]).resolve()), data["url"], data["fixture"]], timeout=100)
            handoff.with_suffix(".reply.json").write_text(json.dumps({"success": result.returncode == 0}), encoding="utf-8")
        assert process.wait(timeout=300) == 0, "WSL browser integration checks failed"
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)
    print("WSL2 detected Windows browsers/default, translated custom Unicode paths/profiles, and launched Windows fixtures through the WSL SOCKS proxy")
