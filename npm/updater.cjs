'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const { binaryPath } = require('./launcher.cjs');

function npmCli() {
  const bin = path.dirname(process.execPath);
  const candidates = [
    process.env.npm_execpath,
    path.join(bin, 'node_modules/npm/bin/npm-cli.js'),
    path.join(bin, '../lib/node_modules/npm/bin/npm-cli.js'),
    '/usr/share/nodejs/npm/bin/npm-cli.js',
  ];
  const primary = candidates.find(file => file && path.basename(file) === 'npm-cli.js' && fs.existsSync(file));
  if (primary) return fs.realpathSync(primary);
  for (const folder of (process.env.PATH || '').split(path.delimiter)) {
    for (const name of process.platform === 'win32' ? ['npm.cmd', 'npm'] : ['npm']) {
      try {
        const file = fs.realpathSync(path.join(folder, name));
        if (path.basename(file) === 'npm-cli.js') return file;
        const nearby = path.join(path.dirname(file), 'node_modules/npm/bin/npm-cli.js');
        if (fs.existsSync(nearby)) return fs.realpathSync(nearby);
      } catch { /* Continue looking in other PATH directories. */ }
    }
  }
  throw new Error('npm을 찾지 못했습니다. PK를 실행한 환경에 Node.js와 npm을 설치하세요.');
}

function plan(root = __dirname) {
  root = fs.realpathSync(root);
  const modules = path.resolve(root, '../..');
  if (path.basename(root) !== 'pk' || path.basename(path.dirname(root)) !== '@gomul82' || path.basename(modules) !== 'node_modules') {
    throw new Error('npm 전역 설치에서만 UI 업데이트를 사용할 수 있습니다. npm install -g @gomul82/pk@latest를 실행하세요.');
  }
  const prefix = process.platform === 'win32' ? path.dirname(modules) : path.resolve(modules, '../..');
  if (process.platform !== 'win32' && path.basename(path.dirname(modules)) !== 'lib') {
    throw new Error('현재 PK와 Node.js의 설치 환경이 다릅니다. WSL에서는 Linux npm으로 PK를 설치하세요.');
  }
  fs.accessSync(modules, fs.constants.W_OK);
  fs.accessSync(path.dirname(root), fs.constants.W_OK);
  fs.accessSync(root, fs.constants.W_OK);
  if (!fs.existsSync(binaryPath())) throw new Error('npm 설치 파일이 불완전합니다. 터미널에서 npm install -g @gomul82/pk@latest 후 pk restart를 실행하세요.');
  const cli = npmCli();
  const check = spawnSync(process.execPath, [cli, 'root', '--global', '--prefix', prefix], { encoding: 'utf8', windowsHide: true, timeout: 15000 });
  if (check.status !== 0 || fs.realpathSync(check.stdout.trim()) !== fs.realpathSync(modules)) {
    throw new Error('npm 전역 설치 위치를 확인하지 못했습니다. 터미널에서 npm 업데이트를 실행하세요.');
  }
  return { root, prefix, cli, node: process.execPath, binary: binaryPath(), version: JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version };
}

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

function installEnvironment(environment, ports) {
  // PK's own proxy is unavailable during replacement; keep other proxies intact.
  return Object.fromEntries(Object.entries(environment).filter(([key, value]) => {
    if (!/^(https?_proxy|all_proxy|npm_config_(https?_proxy|proxy))$/i.test(key)) return true;
    try {
      const proxy = new URL(value);
      return !(['127.0.0.1', 'localhost', '[::1]'].includes(proxy.hostname) && ports.includes(Number(proxy.port)));
    } catch { return true; }
  }));
}

async function run(jobFile) {
  const job = JSON.parse(fs.readFileSync(jobFile, 'utf8'));
  const folder = path.dirname(jobFile);
  const write = (phase, message) => {
    const temporary = jobFile + '.tmp';
    fs.writeFileSync(temporary, JSON.stringify({ ...job, phase, message }));
    fs.renameSync(temporary, jobFile);
  };
  const backup = path.join(folder, process.platform === 'win32' ? 'previous-pk.exe' : 'previous-pk');
  const log = fs.openSync(path.join(folder, 'npm.log'), 'w');
  let stopped = false;
  let restarted = false;
  const start = async binary => {
    const child = spawn(binary, ['daemon-internal'], { detached: true, stdio: 'ignore', windowsHide: true, env: { ...process.env, PK_CONFIG_DIR: job.config_dir } });
    await new Promise((resolve, reject) => { child.once('spawn', resolve); child.once('error', reject); });
    child.unref();
    for (let n = 0; n < 60; n++) {
      await sleep(500);
      try {
        const response = await fetch(`http://127.0.0.1:${job.port}/api/status`, { signal: AbortSignal.timeout(1000) });
        if (response.ok) {
          const info = await response.json();
          if (binary === backup || info.version === JSON.parse(fs.readFileSync(path.join(job.root, 'package.json'), 'utf8')).version) return info.version;
        }
      } catch { /* Wait for the new dashboard. */ }
    }
    try { child.kill(); } catch { /* It may already have exited. */ }
    throw new Error('PK 재시작을 확인하지 못했습니다. 터미널에서 pk ui를 실행하세요.');
  };
  try {
    // The server exits only after this ready marker, so setup failures keep it running.
    fs.copyFileSync(job.binary, backup);
    fs.chmodSync(backup, 0o755);
    write('ready', 'PK 종료를 기다리는 중…');
    for (let n = 0; ; n++) {
      try { process.kill(job.pid, 0); } catch (error) { if (error.code === 'ESRCH') break; throw error; }
      if (n >= 120) throw new Error('PK가 종료되지 않아 업데이트를 취소했습니다.');
      await sleep(250);
    }
    stopped = true;
    write('installing', 'npm으로 새 버전을 설치하는 중…');
    const result = spawnSync(job.node, [job.cli, 'install', '--global', '--prefix', job.prefix, '@gomul82/pk@latest', '--registry', 'https://registry.npmjs.org'], {
      cwd: folder, stdio: ['ignore', log, log], windowsHide: true, timeout: 240000,
      env: installEnvironment(process.env, job.proxy_ports),
    });
    if (result.error || result.status !== 0) {
      throw new Error('npm 설치에 실패했습니다. 설치 권한이나 네트워크를 확인하세요. 자세한 내용: ' + path.join(folder, 'npm.log'));
    }
    write('restarting', '업데이트한 PK를 시작하는 중…');
    const version = await start(job.binary);
    restarted = true;
    write('complete', `v${version} 업데이트 완료. 필요하면 프록시를 다시 연결하세요.`);
  } catch (error) {
    let message = error.message;
    if (stopped && !restarted) {
      try { await start(backup); message += ' 기존 PK를 다시 시작했습니다.'; }
      catch { message += ' 터미널에서 npm install -g @gomul82/pk@latest 후 pk ui를 실행하세요.'; }
    }
    write('failed', message);
  } finally { fs.closeSync(log); }
}

if (require.main === module) {
  if (process.argv[2] === 'check') {
    try { console.log(JSON.stringify({ available: true, ...plan() })); }
    catch (error) { console.log(JSON.stringify({ available: false, reason: error.code === 'EACCES' || error.code === 'EPERM' ? '설치 폴더에 쓰기 권한이 없습니다. 터미널에서 npm 업데이트를 실행하세요.' : error.message })); }
  } else if (process.argv[2] === 'run') {
    run(process.argv[3]).catch(error => { console.error(error.message); process.exitCode = 1; });
  }
}

module.exports = { npmCli, plan, installEnvironment };
