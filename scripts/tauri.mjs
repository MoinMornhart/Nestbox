#!/usr/bin/env node
// Startet die Tauri-CLI. Alle Build-Dateien bleiben im Projektordner (src-tauri/target),
// damit das komplette Projekt an einem Ort liegt. Überschreibbar mit CARGO_TARGET_DIR.
import { spawn } from "node:child_process";
import path from "node:path";
import os from "node:os";

const root = path.join(import.meta.dirname, "..");
const env = { ...process.env };
// Frisch installiertes Rust liegt oft noch nicht im PATH dieser Sitzung.
const cargoBin = path.join(os.homedir(), ".cargo", "bin");
if (!(env.PATH || env.Path || "").includes(cargoBin)) env.PATH = `${cargoBin}${path.delimiter}${env.PATH || env.Path || ""}`;
if (process.argv[2] === "build") {
  const target = env.CARGO_TARGET_DIR || path.join(root, "src-tauri", "target");
  console.log(`\nNestbox: Der Installer landet unter ${path.join(target, "release", "bundle")}\n`);
}

const bin = path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const child = spawn(process.execPath, [bin, ...process.argv.slice(2)], { stdio: "inherit", env });
child.on("exit", (code) => process.exit(code ?? 1));
