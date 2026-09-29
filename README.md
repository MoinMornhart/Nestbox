# Nestbox

**Virtuelle Maschinen einfach gemacht.** Nestbox ist eine Desktop-App für Windows 10/11, mit der du ohne Vorwissen virtuelle Maschinen (VMs) erstellst, startest und verwaltest. Sie nutzt Hyper-V (Windows Pro/Enterprise/Education) oder QEMU (Windows Home) und versteckt die Technik hinter einer ruhigen, verständlichen Oberfläche.

- In unter einer Minute zur laufenden VM: Betriebssystem wählen, ISO reinziehen, Name, Leistung, fertig.
- Einrichtungsprüfung beim ersten Start mit konkreten Lösungsschritten (Hyper-V aktivieren, Berechtigungen, BIOS).
- VMs starten, pausieren, herunterfahren, umbenennen, anpassen und löschen.
- Sicherungspunkte (Snapshots) erstellen und wiederherstellen.
- Hell/Dunkel automatisch, Oberfläche auf Deutsch.

## Voraussetzungen

| Programm | Installation |
|---|---|
| Node.js LTS | `winget install OpenJS.NodeJS.LTS` oder [nodejs.org](https://nodejs.org) |
| Rust (stable) | `winget install Rustlang.Rustup` oder [rustup.rs](https://rustup.rs) |
| Visual Studio Build Tools mit „Desktopentwicklung mit C++“ | `winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` |
| Microsoft Edge WebView2 | Bei Windows 11 bereits enthalten, sonst `winget install Microsoft.EdgeWebView2Runtime` |

Für die VMs selbst:

- **Windows Pro/Enterprise/Education:** Hyper-V (Nestbox schaltet es auf Wunsch ein).
- **Windows Home:** [QEMU für Windows](https://www.qemu.org/download/#windows) und die Windows-Funktion „Windows-Hypervisor-Plattform“ (Nestbox schaltet sie auf Wunsch ein).
- Virtualisierung im BIOS/UEFI aktiviert (Intel VT-x bzw. AMD-V/SVM).

## Starten

Am einfachsten per Doppelklick auf **`start.bat`**. Das Skript prüft alle Voraussetzungen, führt `npm install` aus und startet die App.

Oder von Hand:

```bash
npm install
npm run tauri dev      # Entwicklung
npm run tauri build    # Installer erzeugen
```

Der erste Start dauert einige Minuten, weil Rust alles einmal kompiliert.

> **Build-Ordner:** Alle Build-Dateien liegen im Projekt unter `src-tauri/target` (mehrere GB). Den fertigen Installer findest du unter `src-tauri/target/release/bundle/` (NSIS-Setup und MSI).

`start.bat build` erzeugt den Installer ebenfalls.

### Oberfläche im Browser (Mock-Modus)

`npm run dev` startet nur die Oberfläche auf http://localhost:1420. Außerhalb von Tauri antwortet ein simuliertes Backend mit Beispieldaten, sodass sich alle Bildschirme ohne Hyper-V ausprobieren lassen. Szenarien per URL:

| URL-Parameter | Wirkung |
|---|---|
| `?mock=bereit` (Standard) | Hyper-V bereit, vier Beispiel-VMs |
| `?mock=leer` | Keine VMs |
| `?mock=einrichtung` | Hyper-V aus, Berechtigung fehlt |
| `?mock=home` | Windows Home mit QEMU |
| `?mock=neustart` | Neustart ausstehend |
| `?fail=start` / `?fail=create` / `?fail=snapshot` | Fehlermeldungen erzwingen |
| `?view=wizard&step=2` | Direkt in einen Assistenten-Schritt |

`npm run screenshots` nimmt (bei laufendem `npm run dev`) Vorschaubilder aller Bildschirme in hell und dunkel auf.

## Daten & Logs

| Was | Wo |
|---|---|
| App-Einstellungen | `%LOCALAPPDATA%\Nestbox\settings.json` |
| VM-Liste | `%LOCALAPPDATA%\Nestbox\vms.json` |
| Log-Datei (alle PowerShell-/QEMU-Befehle) | `%LOCALAPPDATA%\Nestbox\logs\nestbox.log` |
| VM-Dateien (Standard, änderbar) | `%USERPROFILE%\Nestbox\VMs\<Name>\` |

## Architektur

```
src/                      React + TypeScript + Tailwind (Oberfläche)
  lib/api.ts              einzige Verbindung zum Backend (Tauri-Commands)
  lib/mock.ts             simuliertes Backend für den Browser
  screens/                Einrichtung, Assistent, Hauptansicht, Dialoge
src-tauri/src/            Rust-Backend
  commands.rs             alle Tauri-Commands
  backend/mod.rs          Trait VmBackend
  backend/hyperv.rs       HyperVBackend (PowerShell)
  backend/qemu.rs         QemuBackend (qemu-system, qemu-img)
  backend/qmp.rs          QMP-Client zur Steuerung laufender QEMU-VMs
  host.rs                 Einrichtungsprüfung und Korrekturen
  ps.rs                   PowerShell-Aufrufe (ohne Konsolenfenster, JSON-Ergebnisse)
  elevate.rs              Adminrechte per UAC nur bei Bedarf (elevated-command)
  store.rs / logger.rs    Einstellungen, VM-Liste, Log-Datei
```

Alle VM-Operationen laufen im Rust-Backend. PowerShell wird mit `powershell.exe -NoProfile -NonInteractive -Command …` aufgerufen, Ergebnisse kommen per `ConvertTo-Json` zurück und werden mit serde gelesen. Die App selbst läuft ohne Adminrechte; nur für „Hyper-V aktivieren“, „Zur Gruppe Hyper-V-Administratoren hinzufügen“ und „Windows-Hypervisor-Plattform aktivieren“ erscheint eine UAC-Abfrage.

### Hyper-V

- VMs der Generation 2 mit dynamischer VHDX, Netzwerk über den „Default Switch“.
- Windows-Gäste: Secure-Boot-Vorlage „MicrosoftWindows“, lokaler Schlüsselschutz und vTPM (Windows 11).
- Linux-Gäste: Secure-Boot-Vorlage „MicrosoftUEFICertificateAuthority“.
- Automatische Checkpoints sind aus; Sicherungspunkte gibt es nur manuell (Standard-Checkpoints inkl. Arbeitsspeicher).
- Anzeige über `vmconnect.exe`, Status alle 2 Sekunden über `Get-VM`.

### QEMU

- `-machine q35`, `-accel whpx` (Fallback `tcg`), UEFI über die mitgelieferte edk2-Firmware, Festplatten als qcow2, `usb-tablet` für eine saubere Maus.
- Steuerung über QMP (`-qmp tcp:127.0.0.1:<port>,server=on,wait=off`): Pause, Herunterfahren, Ausschalten, Sicherungspunkte (`savevm`/`loadvm`).
- Windows 11 ist mit QEMU ausgeblendet, weil es einen TPM-Chip verlangt, den QEMU unter Windows nicht bereitstellen kann (swtpm gibt es nicht offiziell für Windows). Linux und ältere Windows-Versionen per eigener ISO funktionieren.

## Fortschritt

`fortschritt/index.html` zeigt den Entwicklungsstand inklusive Vorschaubildern (per Doppelklick im Browser öffnen).
