'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const test = require('node:test');
const { binaryPath, commandArgs } = require('../launcher.cjs');

test('selects the native binary for every supported platform', () => {
  assert.equal(binaryPath('win32', 'x64'), path.join(__dirname, '..', 'binaries', 'win32-x64', 'pk.exe'));
  assert.equal(binaryPath('darwin', 'x64'), path.join(__dirname, '..', 'binaries', 'darwin-universal', 'pk'));
  assert.equal(binaryPath('darwin', 'arm64'), path.join(__dirname, '..', 'binaries', 'darwin-universal', 'pk'));
  assert.equal(binaryPath('linux', 'x64'), path.join(__dirname, '..', 'binaries', 'linux-x64', 'pk'));
  assert.throws(() => binaryPath('linux', 'arm64'), /Unsupported platform/);
});

test('maps npm proxy commands to the matching Rust subcommands', () => {
  assert.deepEqual(commandArgs('pk', ['status']), ['status']);
  assert.deepEqual(commandArgs('codex', ['--help']), ['codex', '--help']);
  assert.deepEqual(commandArgs('ocx', []), ['ocx']);
});
