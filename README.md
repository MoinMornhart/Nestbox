# Nestbox

**Virtuelle Maschinen einfach gemacht – komplett kostenlos.** Nestbox ist eine Desktop-App für Windows 10/11 (Home und Pro), mit der du ohne Vorwissen virtuelle Maschinen (VMs) erstellst, startest und verwaltest. Die VMs laufen mit kostenloser Software – **VirtualBox** (empfohlen, kann auch Windows 11) oder **QEMU** – und Nestbox versteckt die Technik hinter einer ruhigen, verständlichen Oberfläche. Eine Windows-Pro-Lizenz oder Hyper-V ist nicht nötig.

- In unter einer Minute zur laufenden VM: Betriebssystem wählen, ISO reinziehen, Name, Leistung, fertig.
- Einrichtungsprüfung beim ersten Start mit konkreten Lösungsschritten – VirtualBox oder QEMU werden auf Wunsch per Klick installiert (winget).
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

Für die VMs selbst (alles kostenlos, Nestbox installiert es auf Wunsch):

- **[VirtualBox](https://www.virtualbox.org/wiki/Downloads)** (empfohlen) – oder – **[QEMU für Windows](https://www.qemu.org/download/#windows)**.
- Virtualisierung im BIOS/UEFI aktiviert (Intel VT-x bzw. AMD-V/SVM). Läuft Windows selbst in einer VM (z. B. Proxmox), muss der Host sie durchreichen (Proxmox: CPU-Typ „host“).
- Für schnelles QEMU zusätzlich die Windows-Funktion „Windows-Hypervisor-Plattform“ (Nestbox schaltet sie auf Wunsch ein). Ohne Virtualisierung läuft nur QEMU, und zwar sehr langsam.

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

`npm run dev` startet nur die Oberfläche auf http://localhost:1420. Außerhalb von Tauri antwortet ein simuliertes Backend mit Beispieldaten, sodass sich alle Bildschirme ohne VirtualBox oder QEMU ausprobieren lassen. Szenarien per URL:

| URL-Parameter | Wirkung |
|---|---|
| `?mock=bereit` (Standard) | VirtualBox bereit, vier Beispiel-VMs |
| `?mock=leer` | Keine VMs |
| `?mock=einrichtung` | Weder VirtualBox noch QEMU installiert |
| `?mock=qemu` | Nur QEMU installiert |
| `?mock=vm` | Windows läuft selbst in einer VM ohne Virtualisierung |
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
  backend/vbox.rs         VBoxBackend (VBoxManage)
  backend/qemu.rs         QemuBackend (qemu-system, qemu-img)
  backend/qmp.rs          QMP-Client zur Steuerung laufender QEMU-VMs
  host.rs                 Einrichtungsprüfung, Installation per winget
  ps.rs                   PowerShell-Aufrufe (ohne Konsolenfenster, JSON-Ergebnisse)
  elevate.rs              Adminrechte per UAC nur bei Bedarf (elevated-command)
  store.rs / logger.rs    Einstellungen, VM-Liste, Log-Datei
```

Alle VM-Operationen laufen im Rust-Backend. PowerShell wird mit `powershell.exe -NoProfile -NonInteractive -Command …` aufgerufen (Ergebnisse per `ConvertTo-Json`, gelesen mit serde), VirtualBox und QEMU direkt über ihre Kommandozeilenprogramme – immer ohne Konsolenfenster. Die App selbst läuft ohne Adminrechte; eine UAC-Abfrage erscheint nur beim Installieren von VirtualBox/QEMU und beim Einschalten der Windows-Hypervisor-Plattform.

### VirtualBox

- VM mit UEFI, dynamischer VDI-Festplatte am SATA-Controller, NAT-Netzwerk, USB-Tablet für eine saubere Maus.
- Windows-Gäste: TPM 2.0 und Secure Boot (Microsoft-Schlüssel) – damit läuft Windows 11.
- Steuerung über `VBoxManage` (`startvm`, `controlvm pause/resume/acpipowerbutton/poweroff`, `snapshot take/restore/delete`).
- Status alle 2 Sekunden (`showvminfo --machinereadable`), CPU/RAM über den Prozess `VirtualBoxVM.exe`.
- Auf flüssiges Video getrimmt: 3D-Beschleunigung, 256 MB Grafikspeicher, HD-Audio, Nested Paging und große Speicherseiten. Über „…“ → „Gasterweiterungen installieren“ legt Nestbox die Treiber-CD ein (danach: flüssiges Bild, automatische Fenstergröße, Zwischenablage).
- Hinweis zu Streamingdiensten: Wegen des Kopierschutzes (DRM) liefern Netflix & Co. in VMs meist höchstens HD (720p–1080p), kein 4K/HDR.

### QEMU

- `-machine q35`, `-accel whpx` (Fallback `tcg`), UEFI über die mitgelieferte edk2-Firmware, Festplatten als qcow2, `usb-tablet` für eine saubere Maus.
- Steuerung über QMP (`-qmp tcp:127.0.0.1:<port>,server=on,wait=off`): Pause, Herunterfahren, Ausschalten, Sicherungspunkte (`savevm`/`loadvm`).
- Windows 11 ist mit QEMU ausgeblendet, weil es einen TPM-Chip verlangt, den QEMU unter Windows nicht bereitstellen kann (swtpm gibt es nicht offiziell für Windows) – dafür VirtualBox verwenden. Linux und ältere Windows-Versionen per eigener ISO funktionieren.

## Fortschritt

`fortschritt/index.html` zeigt den Entwicklungsstand inklusive Vorschaubildern (per Doppelklick im Browser öffnen).
