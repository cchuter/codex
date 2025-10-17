#!/usr/bin/env node
const fs = require("fs");
const path = require("path");
const { platform, arch } = process;

// Map Node.js arch to our binary naming
const archMap = {
  x64: "x64",
  arm64: "arm64",
  ia32: "x86",
  arm: "arm",
};

const platformMap = {
  darwin: "darwin",
  linux: "linux",
  win32: "win32",
};

const mappedArch = archMap[arch] || arch;
const mappedPlatform = platformMap[platform] || platform;

const binaryName = `osmiflow-${mappedPlatform}-${mappedArch}${platform === "win32" ? ".exe" : ""}`;
const binaryPath = path.join(__dirname, "bin", binaryName);
const targetPath = path.join(
  __dirname,
  "bin",
  `osmiflow${platform === "win32" ? ".exe" : ""}`,
);

if (!fs.existsSync(binaryPath)) {
  console.error(`
Error: Unsupported platform/architecture combination: ${platform}/${arch}
OsmiFlow supports:
- macOS (x64, arm64)
- Linux (x64)
- Windows (x64)

Please open an issue at https://github.com/cchuter/codex/issues if you need support for your platform.
`);
  process.exit(1);
}

try {
  // Remove existing symlink/file if it exists
  if (fs.existsSync(targetPath)) {
    fs.unlinkSync(targetPath);
  }

  // Create symlink to the appropriate binary
  if (platform === "win32") {
    // On Windows, copy the file instead of symlinking
    fs.copyFileSync(binaryPath, targetPath);
  } else {
    // On Unix systems, create a symlink
    fs.symlinkSync(binaryName, targetPath);
  }

  // Make sure it's executable (Unix only)
  if (platform !== "win32") {
    fs.chmodSync(targetPath, 0o755);
  }

  console.log(`✓ OsmiFlow installed successfully for ${platform}/${arch}`);
} catch (error) {
  console.error("Failed to set up OsmiFlow binary:", error);
  process.exit(1);
}
