#!/usr/bin/env node
// Aktualisiert fortschritt/status.json und rendert daraus fortschritt/index.html
// (eigenständige Datei, Daten eingebettet, Auto-Refresh alle 5 s).
//
// Beispiele:
//   node scripts/progress.mjs --phase 3=active --now "Baue das Hyper-V-Backend" --log "VmBackend-Trait angelegt" --files src-tauri/src/backend/mod.rs
//   node scripts/progress.mjs --wait "Frage zu Git" --problem "Rust fehlt|Rust per winget installieren"
//   node scripts/progress.mjs --clear-wait --clear-problems
//   node scripts/progress.mjs --images   (Galerie aus fortschritt/bilder/ neu einlesen)
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dir = path.join(root, "fortschritt");
const statusFile = path.join(dir, "status.json");
const imgDir = path.join(dir, "bilder");

const PHASES = [
  "Projektstruktur",
  "Einrichtungsprüfung",
  "Hyper-V-Backend",
  "Erstell-Assistent",
  "Hauptansicht",
  "Snapshots",
  "QEMU-Backend",
  "Feinschliff & README",
  "Test & Start",
];

function load() {
  if (fs.existsSync(statusFile)) return JSON.parse(fs.readFileSync(statusFile, "utf8"));
  return {
    phases: PHASES.map((name) => ({ name, state: "open" })),
    now: "Starte das Projekt",
    files: [],
    log: [],
    waiting: [],
    problems: [],
    images: [],
    updated: null,
  };
}

const now = () => new Date();
const hhmm = (d) => d.toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
const s = load();
const args = process.argv.slice(2);

for (let i = 0; i < args.length; i++) {
  const a = args[i];
  const v = args[i + 1];
  switch (a) {
    case "--phase": {
      i++;
      const [n, st] = v.split("=");
      const idx = Number(n) - 1;
      if (st === "active") s.phases.forEach((p, j) => { if (p.state === "active" && j !== idx) p.state = "done"; });
      s.phases[idx].state = st;
      break;
    }
    case "--now": i++; s.now = v; break;
    case "--log": i++; s.log.unshift({ t: now().toISOString(), text: v }); s.log = s.log.slice(0, 200); break;
    case "--files": i++;
      for (const f of v.split(",").map((x) => x.trim()).filter(Boolean)) {
        s.files = s.files.filter((x) => x.path !== f);
        s.files.unshift({ path: f, t: now().toISOString() });
      }
      s.files = s.files.slice(0, 15);
      break;
    case "--wait": i++; s.waiting.push({ t: now().toISOString(), text: v }); break;
    case "--clear-wait": s.waiting = []; break;
    case "--problem": { i++; const [text, fix] = v.split("|"); s.problems.push({ t: now().toISOString(), text, fix: fix || "" }); break; }
    case "--solve": { i++; const p = s.problems.find((x) => x.text.startsWith(v)); if (p) p.solved = now().toISOString(); break; }
    case "--clear-problems": s.problems = []; break;
    case "--images": break;
    default: console.error("Unbekannte Option", a);
  }
}

// Galerie immer aus dem Bildordner einlesen (Titel aus bilder/titel.json, Zeitstempel = mtime)
let titles = {};
try { titles = JSON.parse(fs.readFileSync(path.join(imgDir, "titel.json"), "utf8")); } catch {}
s.images = fs.existsSync(imgDir)
  ? fs.readdirSync(imgDir).filter((f) => f.endsWith(".png")).map((f) => {
      const st = fs.statSync(path.join(imgDir, f));
      const base = f.replace(/-(hell|dunkel)\.png$/, "");
      const meta = titles[base] || {};
      return { file: f, base, title: meta.title || base, group: meta.group || "Bildschirme", order: meta.order ?? 999,
        theme: f.includes("-dunkel") ? "dunkel" : "hell", t: st.mtime.toISOString() };
    }).sort((a, b) => a.order - b.order || a.base.localeCompare(b.base) || a.theme.localeCompare(b.theme))
  : [];

s.updated = now().toISOString();
fs.mkdirSync(dir, { recursive: true });
fs.writeFileSync(statusFile, JSON.stringify(s, null, 2));

const done = s.phases.filter((p) => p.state === "done").length;
const active = s.phases.filter((p) => p.state === "active").length;
const pct = Math.round(((done + active * 0.5) / s.phases.length) * 100);
const esc = (t) => String(t).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
const time = (iso) => hhmm(new Date(iso));
const dt = (iso) => new Date(iso).toLocaleString("de-DE", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
const icon = { done: "✓", active: "●", open: "○" };
const label = { done: "erledigt", active: "in Arbeit", open: "offen" };

const groups = {};
for (const img of s.images) (groups[img.group] ||= []).push(img);
const openProblems = s.problems.filter((p) => !p.solved);

const html = `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="refresh" content="5">
<title>Nestbox – Fortschritt</title>
<style>
:root{--bg:#faf7f2;--card:#fff;--text:#2b2520;--muted:#8a7f74;--line:#ece5dc;--accent:#c8763a;--accent-soft:#f6e9dc;--ok:#3f8f5a;--ok-soft:#e4f2e8;--warn:#b7791f;--warn-soft:#fdf1d8;--err:#c0442f;--err-soft:#fbe5e0}
@media (prefers-color-scheme:dark){:root{--bg:#1a1714;--card:#231f1b;--text:#f1ebe4;--muted:#a0968b;--line:#342e28;--accent:#e0955a;--accent-soft:#3a2c20;--ok:#6cc08a;--ok-soft:#1f3226;--warn:#e7b35a;--warn-soft:#3a2f18;--err:#ef7a64;--err-soft:#3d221c}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--text);font:15px/1.55 "Segoe UI Variable","Segoe UI",system-ui,-apple-system,sans-serif}
main{max-width:1100px;margin:0 auto;padding:40px 16px 80px}
header{display:flex;align-items:center;gap:14px;margin-bottom:28px}
header svg{width:44px;height:44px;flex:none}
h1{font-size:26px;margin:0;letter-spacing:-.02em}
.sub{color:var(--muted);font-size:13px}
h2{font-size:13px;text-transform:uppercase;letter-spacing:.08em;color:var(--muted);margin:0 0 12px;font-weight:600}
.card{background:var(--card);border:1px solid var(--line);border-radius:16px;padding:20px 22px;margin-bottom:18px}
.grid{display:grid;grid-template-columns:1fr 1fr;gap:18px}
.grid>.card{margin:0}
@media (max-width:760px){.grid{grid-template-columns:1fr}}
.bar{height:12px;background:var(--line);border-radius:99px;overflow:hidden}
.bar>div{height:100%;background:linear-gradient(90deg,var(--accent),#e7a86f);border-radius:99px;transition:width .6s}
.pct{font-size:40px;font-weight:650;letter-spacing:-.03em;line-height:1}
.now{font-size:18px;font-weight:500}
.now .dot{display:inline-block;width:10px;height:10px;border-radius:50%;background:var(--accent);margin-right:10px;animation:p 1.6s infinite}
@keyframes p{50%{opacity:.3}}
ol.phases{list-style:none;margin:0;padding:0}
ol.phases li{display:flex;align-items:center;gap:12px;padding:8px 0;border-bottom:1px solid var(--line)}
ol.phases li:last-child{border:0}
.st{width:26px;height:26px;border-radius:50%;display:grid;place-items:center;font-size:13px;flex:none;background:var(--line);color:var(--muted)}
.done .st{background:var(--ok-soft);color:var(--ok)}
.active .st{background:var(--accent-soft);color:var(--accent)}
.active{font-weight:600}
.tag{margin-left:auto;font-size:12px;color:var(--muted)}
ul.plain{list-style:none;margin:0;padding:0}
ul.plain li{padding:6px 0;border-bottom:1px solid var(--line);display:flex;gap:12px}
ul.plain li:last-child{border:0}
.t{color:var(--muted);font-variant-numeric:tabular-nums;font-size:13px;flex:none;min-width:64px}
code{font-family:"Cascadia Code",Consolas,monospace;font-size:13px;word-break:break-all}
.wait{background:var(--warn-soft);border:2px solid var(--warn)}
.wait h2{color:var(--warn)}
.wait p{margin:4px 0;font-size:16px;font-weight:500}
.prob{background:var(--err-soft);border-color:var(--err)}
.prob h2{color:var(--err)}
.prob .fix{color:var(--muted);font-size:13px}
.log{max-height:420px;overflow:auto}
.gallery h3{font-size:14px;margin:18px 0 10px}
.thumbs{display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:14px}
.thumbs a{display:block;text-decoration:none;color:inherit;border:1px solid var(--line);border-radius:12px;overflow:hidden;background:var(--bg);transition:transform .15s}
.thumbs a:hover{transform:translateY(-2px)}
.thumbs img{width:100%;aspect-ratio:16/10;object-fit:cover;object-position:top;display:block}
.thumbs div{padding:8px 10px;font-size:13px}
.thumbs small{color:var(--muted);display:block}
.empty{color:var(--muted);font-size:14px}
</style>
</head>
<body>
<main>
<header>
<svg viewBox="0 0 64 64" aria-hidden="true"><rect width="64" height="64" rx="16" fill="#c8763a"/><path d="M32 12 50 28v22a2 2 0 0 1-2 2H16a2 2 0 0 1-2-2V28z" fill="#fff8f0"/><path d="M10 30 32 11l22 19" fill="none" stroke="#fff8f0" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/><circle cx="32" cy="34" r="6" fill="#c8763a"/><rect x="30.5" y="43" width="3" height="6" rx="1.5" fill="#c8763a"/></svg>
<div><h1>Nestbox – Fortschritt</h1><div class="sub">Zuletzt aktualisiert: ${esc(dt(s.updated))} · aktualisiert sich alle 5 Sekunden</div></div>
</header>

${s.waiting.length ? `<section class="card wait"><h2>⚠ Warte auf dich</h2>${s.waiting.map((w) => `<p>${esc(w.text)}</p>`).join("")}<div class="sub">Bitte im Chat antworten.</div></section>` : ""}

<section class="card">
<div style="display:flex;align-items:baseline;justify-content:space-between;margin-bottom:12px"><h2 style="margin:0">Gesamtfortschritt</h2><span class="pct">${pct}%</span></div>
<div class="bar"><div style="width:${pct}%"></div></div>
</section>

<section class="card"><h2>Gerade dabei</h2><div class="now"><span class="dot"></span>${esc(s.now)}</div></section>

${openProblems.length ? `<section class="card prob"><h2>Probleme</h2><ul class="plain">${openProblems.map((p) => `<li><span class="t">${time(p.t)}</span><div>${esc(p.text)}${p.fix ? `<div class="fix">Lösungsweg: ${esc(p.fix)}</div>` : ""}</div></li>`).join("")}</ul></section>` : ""}

<div class="grid">
<section class="card"><h2>Phasen</h2><ol class="phases">${s.phases.map((p, i) => `<li class="${p.state}"><span class="st">${icon[p.state]}</span>${i + 1}. ${esc(p.name)}<span class="tag">${label[p.state]}</span></li>`).join("")}</ol></section>
<section class="card"><h2>Zuletzt bearbeitete Dateien</h2>${s.files.length ? `<ul class="plain">${s.files.map((f) => `<li><span class="t">${time(f.t)}</span><code>${esc(f.path)}</code></li>`).join("")}</ul>` : `<p class="empty">Noch keine Dateien.</p>`}</section>
</div>

<section class="card" style="margin-top:18px"><h2>Aktivitätsprotokoll</h2><div class="log">${s.log.length ? `<ul class="plain">${s.log.map((l) => `<li><span class="t">${time(l.t)}</span><span>${esc(l.text)}</span></li>`).join("")}</ul>` : `<p class="empty">Noch keine Einträge.</p>`}</div></section>

${s.problems.some((p) => p.solved) ? `<section class="card"><h2>Gelöste Probleme</h2><ul class="plain">${s.problems.filter((p) => p.solved).map((p) => `<li><span class="t">${time(p.solved)}</span><span>✓ ${esc(p.text)}</span></li>`).join("")}</ul></section>` : ""}

<section class="card gallery"><h2>Design &amp; Overlays</h2>
${s.images.length ? Object.entries(groups).map(([g, imgs]) => `<h3>${esc(g)}</h3><div class="thumbs">${imgs.map((im) => `<a href="bilder/${esc(im.file)}" target="_blank" title="Groß öffnen"><img loading="lazy" src="bilder/${esc(im.file)}?v=${Date.parse(im.t)}" alt="${esc(im.title)}"><div>${esc(im.title)} <small>${im.theme} · ${esc(dt(im.t))}</small></div></a>`).join("")}</div>`).join("") : `<p class="empty">Vorschaubilder folgen, sobald die Oberfläche steht.</p>`}
</section>
</main>
</body>
</html>
`;
fs.writeFileSync(path.join(dir, "index.html"), html);
console.log(`Fortschritt: ${pct}%`);
