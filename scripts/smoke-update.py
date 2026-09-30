"""Verify UI update API with a real npm install into an isolated global prefix.

The npm CLI fixture redirects only the package spec to an offline local tarball;
no installed user package or running user daemon is changed.
"""
import io
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

repo = Path(__file__).resolve().parent.parent
binary = Path(sys.argv[1]).resolve()
node = shutil.which('node')
cli = subprocess.check_output([node, '-e', 'process.stdout.write(require(process.argv[1]).npmCli())', str(repo / 'npm/updater.cjs')], text=True).strip()
version = json.loads((repo / 'npm/package.json').read_text())['version']
relative_binary = 'binaries/win32-x64/pk.exe' if os.name == 'nt' else 'binaries/darwin-universal/pk' if sys.platform == 'darwin' else 'binaries/linux-x64/pk'

def free_port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]

with tempfile.TemporaryDirectory(prefix='pk-update-') as temporary:
    temp = Path(temporary)
    package = json.loads((repo / 'npm/package.json').read_text())
    package['scripts'] = {}
    files = {name: (repo / 'npm' / name).read_bytes() for name in ['launcher.cjs', 'updater.cjs', 'bin/pk.cjs']}
    files[relative_binary] = binary.read_bytes()
    files['package.json'] = json.dumps(package).encode()
    archive = temp / 'pk.tgz'
    with tarfile.open(archive, 'w:gz') as tar:
        for name, data in files.items():
            member = tarfile.TarInfo('package/' + name)
            member.size = len(data)
            member.mode = 0o755 if name == relative_binary or name.startswith('bin/') else 0o644
            tar.addfile(member, io.BytesIO(data))
    wrapper = temp / 'npm-cli.js'
    wrapper.write_text('''const {spawnSync}=require('node:child_process');
const fs=require('node:fs');
let args=process.argv.slice(2);
if(args[0]==='root' && process.env.PK_TEST_PREFLIGHT_FAILURE) process.exit(1);
if(args[0]==='install') {
  if(process.env.PK_TEST_INSTALL_FAILURE) {
    // Model an interrupted install that has already removed the old binary.
    fs.unlinkSync(process.env.PK_TEST_BINARY); process.exit(1);
  }
  args=args.map(arg=>arg==='@gomul82/pk@latest'?process.env.PK_TEST_TARBALL:arg);
  args.push('--offline','--no-audit','--no-fund','--ignore-scripts');
}
const result=spawnSync(process.execPath,[process.env.PK_TEST_REAL_NPM,...args],{stdio:'inherit',timeout:60000});
process.exit(result.status ?? 1);
''', encoding='utf-8')
    for mode in ['success', 'install_failure', 'preflight_failure']:
        prefix = temp / ('npm 한글 & $ ' + mode)
        modules = prefix / ('node_modules' if os.name == 'nt' else 'lib/node_modules')
        root = modules / '@gomul82/pk'
        for name, data in files.items():
            destination = root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        (root / relative_binary).chmod(0o755)
        config = temp / ('config-' + mode)
        config.mkdir()
        port = free_port()
        contents = f'web_port = {port}\nhttp_port = {free_port()}\nsocks_port = {free_port()}\nauto_connect = false\nauto_open_browser = false\n'
        (config / 'config.toml').write_text(contents, encoding='utf-8')
        environment = {**os.environ, 'PK_CONFIG_DIR': str(config), 'npm_execpath': str(wrapper), 'PK_TEST_REAL_NPM': cli, 'PK_TEST_TARBALL': str(archive), 'PK_TEST_BINARY': str(root / relative_binary)}
        if mode == 'install_failure': environment['PK_TEST_INSTALL_FAILURE'] = '1'
        if mode == 'preflight_failure': environment['PK_TEST_PREFLIGHT_FAILURE'] = '1'
        process = subprocess.Popen([node, str(root / 'bin/pk.cjs'), 'start', '--foreground'], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def api(method='GET', origin=None, host=None):
            headers = {'Content-Type': 'application/json'}
            if origin: headers['Origin'] = origin
            if host: headers['Host'] = host
            request = Request(f'http://127.0.0.1:{port}/api/update', data=b'{}' if method == 'POST' else None, headers=headers, method=method)
            with urlopen(request, timeout=25) as response:
                return response.status, json.load(response)
        try:
            deadline = time.monotonic() + 30
            while True:
                try: info = api()[1]; break
                except (URLError, TimeoutError):
                    assert process.poll() is None and time.monotonic() < deadline
                    time.sleep(0.2)
            assert info['available'] == (mode != 'preflight_failure'), info
            if mode == 'success' and len(sys.argv) > 2:
                subprocess.run(sys.argv[2:] + [f'http://127.0.0.1:{port}'], check=True, timeout=100)
            for headers in [{'origin': 'https://example.com'}, {'host': 'evil.example'}]:
                try: api('POST', **headers); raise AssertionError('foreign request accepted')
                except HTTPError as error: assert error.code == 403
            if mode == 'preflight_failure':
                try: api('POST'); raise AssertionError('invalid npm preflight accepted')
                except HTTPError as error: assert error.code == 400
                assert process.poll() is None
            else:
                status, job = api('POST')
                assert status == 202
                try: api('POST'); raise AssertionError('duplicate update accepted')
                except (HTTPError, URLError, ConnectionError) as error:
                    if isinstance(error, HTTPError): assert error.code == 400
                process.wait(timeout=30)
                deadline = time.monotonic() + 90
                while True:
                    try:
                        result = api()[1]['job']
                        if result and result['phase'] in ['complete', 'failed']: break
                    except (URLError, TimeoutError): pass
                    assert time.monotonic() < deadline, (config / 'npm-update/job.json').read_text()
                    time.sleep(0.3)
                assert result['id'] == job['id'], result
                assert result['phase'] == ('complete' if mode == 'success' else 'failed'), result
                if mode == 'install_failure': assert '기존 PK' in result['message'], result
                assert (config / 'config.toml').read_text(encoding='utf-8') == contents
                with urlopen(f'http://127.0.0.1:{port}/api/status') as response: assert json.load(response)['version'] == version
            print('UI npm update:', mode, 'passed', flush=True)
        finally:
            # A second request can race with shutdown. Let the helper finish before
            # removing its inputs or stopping the daemon it is about to restart.
            helper_pid_file = config / 'npm-update/helper.pid'
            job_file = config / 'npm-update/job.json'
            if helper_pid_file.exists():
                deadline = time.monotonic() + 100
                while time.monotonic() < deadline:
                    try:
                        if json.loads(job_file.read_text())['phase'] in ['complete', 'failed']: break
                    except (OSError, ValueError): pass
                    time.sleep(0.2)
                else:
                    helper_pid = int(helper_pid_file.read_text())
                    if os.name == 'nt': subprocess.run(['taskkill', '/PID', str(helper_pid), '/T', '/F'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    else:
                        import signal
                        try: os.killpg(helper_pid, signal.SIGTERM)
                        except ProcessLookupError: pass
            # Config and PID belong only to this test, including the fallback daemon.
            running = config / 'pk.pid'
            if running.exists():
                pid = int(running.read_text())
                if os.name == 'nt': subprocess.run(['taskkill', '/PID', str(pid), '/F'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                else:
                    import signal
                    try: os.kill(pid, signal.SIGTERM)
                    except ProcessLookupError: pass
                time.sleep(1)
            if process.poll() is None: process.kill()
            process.wait(timeout=10)
            # A completed helper has no open npm.log handles after a brief exit.
            time.sleep(0.5)
