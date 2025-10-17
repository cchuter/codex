#!/usr/bin/env node
const { spawn } = require("child_process");
const path = require("path");

const binary = path.join(__dirname, "bin", "osmiflow");
const args = process.argv.slice(2);

const child = spawn(binary, args, {
  stdio: "inherit",
  shell: false,
});

child.on("exit", (code) => {
  process.exit(code);
});

child.on("error", (err) => {
  console.error("Failed to start OsmiFlow:", err);
  process.exit(1);
});
