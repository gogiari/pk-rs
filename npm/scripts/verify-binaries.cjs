'use strict';

const fs = require('node:fs');
const path = require('node:path');

const packageRoot = path.resolve(__dirname, '..');
for (const [relativePath, signature] of [
  ['binaries/win32-x64/pk.exe', '4d5a'],
  ['binaries/darwin-universal/pk', 'cafebabe'],
  ['binaries/linux-x64/pk', '7f454c46'],
]) {
  const file = path.join(packageRoot, relativePath);
  if (!fs.existsSync(file) || !fs.statSync(file).isFile() || fs.statSync(file).size === 0) {
    throw new Error(`Missing npm package binary: ${relativePath}`);
  }
  const descriptor = fs.openSync(file, 'r');
  const header = Buffer.alloc(signature.length / 2);
  try {
    fs.readSync(descriptor, header, 0, header.length, 0);
  } finally {
    fs.closeSync(descriptor);
  }
  if (header.toString('hex') !== signature) {
    throw new Error(`Wrong binary format in npm package: ${relativePath}`);
  }
}
console.log('All npm package binaries are present.');
