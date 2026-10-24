# Stand: Nestbox (Übergabe 2026-10-24)

## Was es ist
Windows-Desktop-App (10/11, Home und Pro), mit der man ohne Vorwissen virtuelle Maschinen erstellt und verwaltet – nach Vorbild Hyper-V, aber einfacher und schön bedienbar. Zu Beginn Betriebssystem wählen (Kachel) oder eigene ISO reinziehen. Oberfläche auf Deutsch.

## Aktueller Stand (Version 0.9.8)
- Backends: **VirtualBox** (kostenloser Standard, Installation per Klick über winget), **QEMU** als Alternative, **Hyper-V optional** für Windows Pro inkl. Übernahme vorhandener Hyper-V-VMs.
- Assistent: Ubuntu, Linux Mint, Fedora, Windows 11 oder eigene ISO; Leistung „Leicht/Ausgewogen/Leistungsstark“ aus der Hardware berechnet (max. 50 %).
- ISO-Bibliothek: lädt neueste Ubuntu/Mint/Fedora-ISO selbst und prüft sie per Prüfsumme; Windows-11-ISO wird aus „Downloads“ übernommen.
- Eigene ISOs als feste Systeme („+ Eigenes System“, in Einstellungen unter `customOs`).
- Sicherungspunkte (Snapshots), Hell/Dunkel, 3D/Audio/Gasterweiterungen bei VirtualBox, Windows 11 als Gast mit TPM/Secure Boot.
- Signierte Update-Funktion über GitHub-Releases, Installer-Build per GitHub Actions, Single-Instance.

## Zuletzt gemacht
- 2026-10-24: Fehler aus Issue #15 behoben (Branch `claude/lucid-bell-812b5d`, noch nicht in `main`): „VM-Fenster öffnen“ schlug bei VirtualBox fehl und CPU/RAM standen falsch. Ursache: VirtualBox schützt seine VM-Prozesse, deren Befehlszeile ist nicht lesbar. Jetzt Erkennung über Fenstertitel und Prozess-Kette (`backend/vbox.rs`), minimierte Fenster werden wiederhergestellt.
- 2026-10-24: Cloud-Start-Hook (Philips Skills automatisch in Cloud-Sitzungen, `.claude/hooks/skills-cloud.sh`).
- 2026-10-05: 0.9.8 eigene ISOs als feste Systeme; Sicherheitsbefunde behoben (Actions auf Commit-SHAs); Abhängigkeiten aktualisiert (Tauri 2.12.1).
- 2026-10-05: 0.9.7 ISO-Bibliothek; 0.9.6 Links in der App öffnen wieder.
- 2026-10-01: 0.9.5 Admin-Aktionen warten auf UAC-Skript (behebt „keine Rückmeldung“).
- 2026-09-30: 0.9.4 Update-Funktion; Hyper-V als optionales Backend.

## Offen / Nächste Schritte
- Fix für Issue #15 auf echtem PC testen (VirtualBox-VM starten → „Anzeigen“, auch minimiert; CPU/RAM-Werte plausibel?), dann nach `main` und als 0.9.9 veröffentlichen. Unklar: ob der Arbeitsspeicher-Wert von VirtualBox den ganzen Gast-RAM enthält.
- Issues #13–#15 von JONIMONI09 (Browser-Version, Speicherort/„leichte Version“, eigene VM-Technik/KI-Schnittstelle): Philip entscheidet, ob umgesetzt wird.
- Aus `UEBERGABE.md` (01.10.) noch als „zu testen“ markiert – ob inzwischen getestet, ist **unklar**:
  - Einrichtung → Hyper-V → „Mich hinzufügen“, danach ab-/anmelden.
  - VirtualBox-Installation und Anlegen einer VM auf echtem PC.
  - Update-Knopf (installierte App soll neuere Version selbst finden).
- `UEBERGABE.md` ist veraltet (nennt iCloud als Projektort) – Projekte liegen jetzt in `C:\Projekte` + GitHub.
- Bekannte Fehler: siehe offene Issues auf GitHub (#15 enthält zwei echte Fehler, Fix auf dem Branch oben).

## Wichtige Infos
- Technik: Tauri 2 (Rust) + React 19 + TypeScript + Vite + Tailwind 4; VM-Steuerung im Rust-Backend (PowerShell/VBoxManage/QEMU).
- Start lokal: `start.bat` (prüft Abhängigkeiten) bzw. `npm install` → `npm run tauri dev`; Build: `npm run tauri build`.
- Hilfsskripte: `npm run screenshots` (Playwright), `npm run progress` (Fortschrittsseite `fortschritt/index.html`).
- Workflows: `check.yml` (Prüfen/Kompilieren auf GitHub, nicht lokal – C: hat wenig Platz), `release.yml` (bei Tag `v*`: Installer + `latest.json`), `vibeworks-check.yml`.
- Neue Version: Nummer in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` erhöhen, `.github/update-notes.md` füllen, Tag `vX.Y.Z` pushen.
- npm- und Rust-Versionen der Tauri-Plugins müssen in major.minor übereinstimmen.
- Update-Signaturschlüssel liegt NUR lokal bei Philip (`C:\Projekte\Nestbox-Update-Schluessel`), nie ins Repo.
- Log der installierten App: `%LOCALAPPDATA%\Nestbox\logs\nestbox.log`.

## Philips Vorgaben
- Repo **öffentlich** (github.com/MoinMornhart/Nestbox); **Update-Funktion: ja**.
- Commits nur mit `MoinMornhart <297179352+MoinMornhart@users.noreply.github.com>`, nie mit privater E-Mail.
- Kommunikation und UI auf Deutsch; einfach, keine Fachbegriffe, jede Fehlermeldung mit Lösungsvorschlag.
- „Verfasst mit Claude 🤖“ in Git-Texten: im Repo **nicht** festgelegt (bisherige Commits ohne) – unklar, ggf. Philip fragen.

Diese Datei wird beim Arbeiten aktuell gehalten: nach jedem größeren Schritt „Aktueller Stand“ und „Offen“ anpassen.
