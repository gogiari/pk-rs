'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

function binaryPath(platform = process.platform, arch = process.arch) {
  let target;
  if (platform === 'win32' && arch === 'x64') {
    target = ['win32-x64', 'pk.exe'];
  } else if (platform === 'darwin' && (arch === 'x64' || arch === 'arm64')) {
    target = ['darwin-universal', 'pk'];
  } else if (platform === 'linux' && arch === 'x64') {
    target = ['linux-x64', 'pk'];
  } else {
    throw new Error(`Unsupported platform: ${platform}-${arch}. See https://github.com/gogiari/pk-rs/releases/latest`);
  }
  return path.join(__dirname, 'binaries', ...target);
}

function commandArgs(command, args) {
  return command === 'pk' ? args : [command, ...args];
}

function run(command) {
  const args = process.argv.slice(2);
  if (command === 'pk' && (args[0] === 'install' || args[0] === 'uninstall')) {
    console.error(`This npm installation already registers all commands. Use "npm uninstall -g @gomul82/pk" to remove it.`);
    process.exitCode = 1;
    return;
  }

  try {
    const executable = binaryPath();
    if (!fs.existsSync(executable)) {
      throw new Error(`Missing ${executable}. Reinstall @gomul82/pk or download the native installer from GitHub Releases.`);
    }
    const result = spawnSync(executable, commandArgs(command, args), {
      stdio: 'inherit',
      env: { ...process.env, PK_INSTALL_SOURCE: 'npm' },
    });
    if (result.error) throw result.error;
    process.exitCode = result.status ?? (result.signal === 'SIGINT' ? 130 : 1);
  } catch (error) {
    console.error(`@gomul82/pk: ${error.message}`);
    process.exitCode = 1;
  }
}

module.exports = { binaryPath, commandArgs, run };
