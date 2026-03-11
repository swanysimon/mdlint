#!/usr/bin/env node
'use strict';

const { spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');

function isMusl() {
  try {
    return fs.readFileSync('/proc/self/maps', 'utf8').includes('musl');
  } catch {
    return false;
  }
}

function getBinaryName() {
  const arch = process.arch === 'x64' ? 'x64' : process.arch === 'arm64' ? 'arm64' : null;
  if (!arch) return null;

  switch (process.platform) {
    case 'linux':
      return `mdlint-linux-${arch}${isMusl() ? '-musl' : ''}`;
    case 'darwin':
      return `mdlint-darwin-${arch}`;
    case 'win32':
      return `mdlint-win32-${arch}.exe`;
    default:
      return null;
  }
}

const binaryName = getBinaryName();
if (!binaryName) {
  console.error(`mdlint: unsupported platform ${process.platform}-${process.arch}`);
  process.exit(1);
}

const binaryPath = path.join(__dirname, binaryName);
if (!fs.existsSync(binaryPath)) {
  console.error(`mdlint: binary not found at ${binaryPath}. Try reinstalling: npm install markdownlint-rs`);
  process.exit(1);
}

const result = spawnSync(binaryPath, process.argv.slice(2), { stdio: 'inherit' });

if (result.error) {
  console.error(`Failed to run mdlint: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 0);
