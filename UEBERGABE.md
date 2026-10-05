# Nestbox – Stand & Übergabe

Diese Datei hält fest, wo das Projekt steht, damit eine neue Sitzung (auch auf einem anderen PC) direkt weitermachen kann.
Zuletzt aktualisiert: 01.10.2026

## Wo liegt was?

| Was | Ort |
|---|---|
| Projekt (Code, Fortschrittsseite, Installer) | `iCloudDrive\Morni Archiv\Projekte\Nestbox` |
| Update-Signaturschlüssel (geheim, Sicherung) | `iCloudDrive\Morni Archiv\Projekte\Nestbox-Update-Schluessel` (siehe `LIESMICH.txt` dort) |
| GitHub-Repo (öffentlich) | https://github.com/MoinMornhart/Nestbox |
| Neuester Installer | https://github.com/MoinMornhart/Nestbox/releases/latest/download/Nestbox-Setup.exe |
| Log der installierten App | `%LOCALAPPDATA%\Nestbox\logs\nestbox.log` |

## Aktueller Stand (Version 0.9.7)

- VirtualBox ist der kostenlose Standard, QEMU die Alternative. Hyper-V ist optional für Windows Pro, inklusive Übernahme vorhandener VMs aus dem Hyper-V-Manager.
- Update-Funktion: Nestbox sucht beim Start nach neuen Versionen und aktualisiert sich per Klick. Die Updates sind signiert.
- 0.9.7: ISOs von Ubuntu, Mint und Fedora lädt Nestbox selbst (immer neueste Version, Ordner: Einstellungen → ISO-Dateien). Windows 11 wird aus „Downloads“ übernommen.
- 0.9.6: Links in der App öffnen sich wieder (fehlende Opener-Freigabe). 0.9.5 behebt „keine Rückmeldung“ bei Admin-Aktionen, z. B. „Mich hinzufügen“ zur Gruppe Hyper-V-Administratoren.

## Offen / als Nächstes

- In 0.9.5 testen: Einrichtung → Hyper-V → „Mich hinzufügen“, danach ab- und wieder anmelden.
- Testen: VirtualBox-Installation und das Anlegen einer VM.
- Testen: Update-Knopf. Ab 0.9.4 sollte die installierte App 0.9.5 selbst finden.

## Regeln für die Arbeit

- Commits nur mit `MoinMornhart <297179352+MoinMornhart@users.noreply.github.com>`, nie mit der privaten E-Mail.
- Kommunikation auf Deutsch.
- Build-Ausgaben nicht lokal ablegen. C: hat wenig Platz, deshalb kompiliert der Workflow „Prüfen“ auf GitHub.
- Neue Version: Versionsnummer in `package.json`, `src-tauri/tauri.conf.json` und `src-tauri/Cargo.toml` erhöhen, `.github/update-notes.md` füllen, Tag `vX.Y.Z` pushen.
- npm- und Rust-Versionen der Tauri-Plugins müssen in major.minor übereinstimmen, sonst bricht der Build ab.
