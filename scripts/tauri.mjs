#!/usr/bin/env node
// Startet die Tauri-CLI und legt den Rust-Build-Ordner außerhalb des (iCloud-)Projektordners ab,
// damit nicht mehrere GB Build-Artefakte synchronisiert werden.
// Überschreibbar mit der Umgebungsvariable CARGO_TARGET_DIR.
import { spawn } from "node:child_process";
import path from "node:path";
import os from "node:os";

const base = process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local");
const env = { ...process.env };
if (!env.CARGO_TARGET_DIR) env.CARGO_TARGET_DIR = path.join(base, "Nestbox", "build", "target");
if (process.argv[2] === "build") {
  console.log(`\nNestbox: Der Installer landet unter ${path.join(env.CARGO_TARGET_DIR, "release", "bundle")}\n`);
}

const bin = path.join(import.meta.dirname, "..", "node_modules", "@tauri-apps", "cli", "tauri.js");
const child = spawn(process.execPath, [bin, ...process.argv.slice(2)], { stdio: "inherit", env });
child.on("exit", (code) => process.exit(code ?? 1));
