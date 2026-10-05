#!/usr/bin/env node
// Nimmt Vorschaubilder aller Bildschirme und Overlays auf (hell + dunkel).
// Voraussetzung: "npm run dev" läuft (Mock-Modus im Browser).
// Aufruf: node scripts/screenshots.mjs [filter]   z. B. "assistent" für nur diese Bilder
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { chromium } from "playwright";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outDir = path.join(root, "fortschritt", "bilder");
const BASE = process.env.NESTBOX_URL || "http://localhost:1420/";
const ISO = encodeURIComponent("C:\\Users\\Demo\\Downloads\\ubuntu-24.04.3-desktop-amd64.iso");
const filter = process.argv[2] || "";

const byText = (page, text) => page.getByRole("button", { name: text }).first();

/** name, Titel, Gruppe, URL-Parameter, optionale Aktionen */
const SHOTS = [
  ["einrichtung", "Einrichtung – noch nichts installiert", "Bildschirme", "?mock=einrichtung"],
  ["einrichtung-bereit", "Einrichtung – alles bereit", "Bildschirme", "?mock=bereit&view=setup"],
  ["einrichtung-qemu", "Einrichtung – nur QEMU installiert", "Bildschirme", "?mock=qemu"],
  ["einrichtung-hyperv", "Einrichtung – Hyper-V optional (Windows Pro)", "Bildschirme", "?mock=hyperv-einrichtung"],
  ["einrichtung-neustart", "Einrichtung – Neustart nötig", "Bildschirme", "?mock=neustart"],
  ["einrichtung-vm", "Einrichtung – Windows läuft selbst in einer VM", "Bildschirme", "?mock=vm",
    async (p) => { await p.getByText("So schaltest du sie ein").first().click(); await p.waitForTimeout(300); }],
  ["assistent-1", "Assistent 1 – Betriebssystem wählen", "Bildschirme", "?view=wizard"],
  ["assistent-1-download", "Assistent 1 – neueste ISO wird automatisch geladen", "Bildschirme", "?view=wizard&isoPause=17", async (p) => { await p.getByText("Ubuntu", { exact: true }).click(); await p.waitForTimeout(3800); }],
  ["assistent-1-bereit", "Assistent 1 – ISO schon geladen, ein Klick genügt", "Bildschirme", "?view=wizard&isos=ready", async (p) => { await p.waitForTimeout(900); await p.getByText("Linux Mint", { exact: true }).click(); await p.waitForTimeout(1500); }],
  ["assistent-1-windows", "Assistent 1 – Windows 11 einmal bei Microsoft laden", "Bildschirme", "?view=wizard", async (p) => { await p.getByText("Windows 11", { exact: true }).click(); await p.waitForTimeout(1500); }],
  ["assistent-1-eigene", "Assistent 1 – eigene Systeme als feste Kacheln", "Bildschirme", "?view=wizard&eigene=1", async (p) => { await p.getByText("Kali Linux", { exact: true }).click(); await p.waitForTimeout(900); }],
  ["assistent-1-merken", "Eigene ISO als festes System merken", "Overlays", "?view=wizard&iso=C:%5CUsers%5CDemo%5CDownloads%5Ckali-linux-2026.3-installer-amd64.iso", async (p) => { await p.waitForTimeout(700); await p.getByText("Als System merken").click(); await p.waitForTimeout(600); }],
  ["assistent-1-iso", "Assistent 1 – ISO übernommen", "Bildschirme", `?view=wizard&iso=${ISO}`],
  ["assistent-2", "Assistent 2 – Name", "Bildschirme", `?view=wizard&step=1&iso=${ISO}`],
  ["assistent-3", "Assistent 3 – Leistung (Erweitert)", "Bildschirme", `?view=wizard&step=2&advanced=1&iso=${ISO}`],
  ["assistent-4", "Assistent 4 – Zusammenfassung", "Bildschirme", `?view=wizard&step=3&iso=${ISO}`],
  ["hauptansicht", "Hauptansicht", "Bildschirme", "?view=dashboard"],
  ["hauptansicht-leer", "Hauptansicht – noch keine VMs", "Bildschirme", "?mock=leer&view=dashboard"],
  ["einstellungen", "Einstellungen", "Bildschirme", "?view=dashboard&dialog=app-settings"],

  ["erstellen-fortschritt", "Erstellen – Fortschrittsanzeige", "Overlays", `?view=wizard&step=3&langsam=1&iso=${ISO}`,
    async (p) => { await byText(p, "Erstellen & starten").click(); await p.waitForTimeout(400); }],
  ["erstellen-fertig", "Erstellen – fertig", "Overlays", `?view=wizard&step=3&iso=${ISO}`,
    async (p) => { await byText(p, "Erstellen & starten").click(); await p.getByText("läuft", { exact: false }).first().waitFor(); await p.waitForTimeout(600); }],
  ["erstellen-fehler", "Erstellen – Fehlermeldung", "Overlays", `?view=wizard&step=3&fail=create&iso=${ISO}`,
    async (p) => { await byText(p, "Erstellen & starten").click(); await p.getByText("Technische Details").waitFor(); await p.getByText("Technische Details").click(); await p.waitForTimeout(400); }],
  ["kontextmenue", "Kontextmenü einer VM", "Overlays", "?view=dashboard",
    async (p) => { await p.getByRole("button", { name: "Weitere Aktionen" }).first().click(); await p.waitForTimeout(350); }],
  ["dialog-sicherungspunkte", "Sicherungspunkte", "Overlays", "?view=dashboard&dialog=snapshots:a1", async (p) => p.waitForTimeout(600)],
  ["dialog-wiederherstellen", "Bestätigung – Wiederherstellen", "Overlays", "?view=dashboard&dialog=snapshots:a1",
    async (p) => { await byText(p, "Wiederherstellen").click(); await p.waitForTimeout(350); }],
  ["dialog-loeschen", "Bestätigung – Löschen", "Overlays", "?view=dashboard&dialog=delete:a1",
    async (p) => { await p.getByRole("checkbox").check(); await p.waitForTimeout(300); }],
  ["dialog-ausschalten", "Bestätigung – Sofort ausschalten", "Overlays", "?view=dashboard&dialog=poweroff:a1"],
  ["dialog-hyperv-import", "Hyper-V-VMs übernehmen", "Overlays", "?mock=hyperv&view=dashboard&dialog=hyperv-import", async (p) => p.waitForTimeout(900)],
  ["update-verfuegbar", "Update verfügbar (Hinweis oben rechts)", "Overlays", "?view=dashboard&update=1", async (p) => p.waitForTimeout(3800)],
  ["dialog-update", "Update installieren", "Overlays", "?view=dashboard&update=1&dialog=update", async (p) => p.waitForTimeout(3800)],
  ["dialog-umbenennen", "Umbenennen", "Overlays", "?view=dashboard&dialog=rename:c3"],
  ["dialog-gasterweiterungen", "Gasterweiterungen – flüssiges Bild", "Overlays", "?view=dashboard&dialog=guesttools:a1"],
  ["dialog-vm-einstellungen", "VM-Einstellungen", "Overlays", "?view=dashboard&dialog=settings:c3"],
  ["fehler-toast", "Fehlermeldung mit Lösungsvorschlag", "Overlays", "?view=dashboard&fail=start",
    async (p) => { await byText(p, "Starten").first().click(); await p.getByText("Technische Details").waitFor(); await p.getByText("Technische Details").click(); await p.waitForTimeout(400); }],
  ["tooltip", "Tooltip für Fachbegriffe", "Overlays", `?view=wizard&step=2&advanced=1&iso=${ISO}`,
    async (p) => { await p.getByText("?", { exact: true }).first().hover(); await p.waitForTimeout(700); }],
];

fs.mkdirSync(outDir, { recursive: true });
// Ohne Prototyp: Schlüssel wie „__proto__“ aus der Datei können so nichts verändern.
const titles = Object.create(null);
const titleFile = path.join(outDir, "titel.json");
try { Object.assign(titles, JSON.parse(fs.readFileSync(titleFile, "utf8"))); } catch {}

const browser = await chromium.launch({ channel: "msedge" }).catch(() => chromium.launch());
let order = 0;
for (const [name, title, group, query, action] of SHOTS) {
  order++;
  titles[name] = { title, group, order };
  if (filter && !name.includes(filter)) continue;
  for (const [scheme, suffix] of [["light", "hell"], ["dark", "dunkel"]]) {
    const ctx = await browser.newContext({ viewport: { width: 1180, height: 780 }, deviceScaleFactor: 1, colorScheme: scheme, locale: "de-DE" });
    const page = await ctx.newPage();
    try {
      await page.goto(BASE + query, { waitUntil: "networkidle" });
      await page.waitForTimeout(1300); // Mock-Verzögerungen + Animationen
      if (action) await action(page);
      await page.waitForTimeout(250);
      await page.screenshot({ path: path.join(outDir, `${name}-${suffix}.png`) });
      console.log("✓", name, suffix);
    } catch (e) {
      console.error("✗", name, suffix, e.message.split("\n")[0]);
    }
    await ctx.close();
  }
}
await browser.close();
fs.writeFileSync(titleFile, JSON.stringify(titles, null, 2));
execFileSync(process.execPath, [path.join(root, "scripts", "progress.mjs"), "--images"], { stdio: "inherit" });
