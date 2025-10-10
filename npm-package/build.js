#!/usr/bin/env node
/**
 * Build script for local npm package development
 * This script builds the Rust binary and prepares it for npm packaging
 */

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const platform = process.platform;
const arch = process.arch;

// Map Node.js arch to Rust target arch
const archMap = {
  'x64': 'x86_64',
  'arm64': 'aarch64',
  'ia32': 'i686',
};

// Map platform to Rust target
const targetMap = {
  'darwin-x64': 'x86_64-apple-darwin',
  'darwin-arm64': 'aarch64-apple-darwin',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc',
};

const rustArch = archMap[arch] || arch;
const target = targetMap[`${platform}-${arch}`];

if (!target) {
  console.error(`Unsupported platform/architecture: ${platform}/${arch}`);
  process.exit(1);
}

console.log(`Building OsmiFlow for ${platform}/${arch} (target: ${target})...`);

try {
  // Change to codex-rs directory
  const codexRsPath = path.join(__dirname, '..', 'codex-rs');
  process.chdir(codexRsPath);
  
  // Build the Rust binary
  console.log('Building Rust binary...');
  execSync(`cargo build --release --target ${target} --bin codex`, {
    stdio: 'inherit'
  });
  
  // Create bin directory
  const binDir = path.join(__dirname, 'bin');
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }
  
  // Copy the binary
  const sourceBinary = path.join(
    codexRsPath,
    'target',
    target,
    'release',
    platform === 'win32' ? 'codex.exe' : 'codex'
  );
  
  const targetBinary = path.join(
    binDir,
    `osmiflow-${platform}-${arch}${platform === 'win32' ? '.exe' : ''}`
  );
  
  console.log(`Copying binary from ${sourceBinary} to ${targetBinary}...`);
  fs.copyFileSync(sourceBinary, targetBinary);
  
  // Make it executable on Unix systems
  if (platform !== 'win32') {
    fs.chmodSync(targetBinary, 0o755);
  }
  
  console.log('✓ Build complete!');
  console.log(`Binary available at: ${targetBinary}`);
  
} catch (error) {
  console.error('Build failed:', error);
  process.exit(1);
}