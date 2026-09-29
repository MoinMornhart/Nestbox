//! Aktionen mit Administratorrechten – über das Crate `elevated-command`.
//! Die App selbst läuft ohne Adminrechte; der UAC-Dialog erscheint nur, wenn
//! eine Aktion ihn wirklich braucht (z. B. Windows-Hypervisor-Plattform aktivieren).
//!
//! Da ein erhöhter Prozess seine Ausgabe nicht an uns zurückgeben kann, schreibt
//! das Skript sein Ergebnis in eine temporäre Datei, die wir danach lesen.

use std::fs;
use std::process::Command as StdCommand;

use elevated_command::Command as ElevatedCommand;

use crate::error::{AppError, AppResult};
use crate::logger;
use crate::ps;

pub fn is_elevated() -> bool {
    ElevatedCommand::is_elevated()
}

/// Führt ein PowerShell-Skript mit Adminrechten aus (UAC-Dialog).
/// Liefert die Textausgabe des Skripts.
pub fn run_ps(what: &str, script: &str) -> AppResult<String> {
    logger::write("PS-ADMIN", script);

    // Läuft Nestbox ausnahmsweise schon als Admin, ist kein UAC-Dialog nötig.
    if is_elevated() {
        return ps::run(what, script);
    }

    let tmp = std::env::temp_dir().join(format!("nestbox-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&tmp)?;
    let script_path = tmp.join("aktion.ps1");
    let out_path = tmp.join("ergebnis.txt");

    let wrapped = format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         $ProgressPreference = 'SilentlyContinue'\r\n\
         $out = {out}\r\n\
         try {{\r\n\
           $r = & {{\r\n{script}\r\n}} | Out-String\r\n\
           Set-Content -LiteralPath $out -Value ('OK' + [Environment]::NewLine + $r) -Encoding UTF8\r\n\
           exit 0\r\n\
         }} catch {{\r\n\
           Set-Content -LiteralPath $out -Value ('ERR' + [Environment]::NewLine + $_.Exception.Message) -Encoding UTF8\r\n\
           exit 1\r\n\
         }}\r\n",
        out = ps::quote(&out_path.to_string_lossy()),
    );
    // UTF-8 mit BOM, damit Windows PowerShell 5.1 Umlaute richtig liest.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(wrapped.as_bytes());
    fs::write(&script_path, bytes)?;

    // Hinweis: elevated-command fügt die Argumente mit Leerzeichen zusammen,
    // Pfade werden deshalb selbst in Anführungszeichen gesetzt.
    let mut cmd = StdCommand::new("powershell.exe");
    cmd.args([
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-WindowStyle".to_string(),
        "Hidden".to_string(),
        "-File".to_string(),
        format!("\"{}\"", script_path.to_string_lossy()),
    ]);

    let result = ElevatedCommand::new(cmd).output();
    let text = fs::read_to_string(&out_path).unwrap_or_default();
    let _ = fs::remove_dir_all(&tmp);

    if let Err(e) = result {
        logger::error(&format!("{what}: UAC abgelehnt oder fehlgeschlagen: {e}"));
        return Err(AppError::new(
            format!("{what} – Administratorrechte wurden nicht erteilt"),
            "Bestätige die Windows-Abfrage („Möchten Sie zulassen …?“) mit „Ja“. Ohne Adminrechte kann diese Einstellung nicht geändert werden.",
        )
        .with_details(e.to_string()));
    }

    let text = text.trim_start_matches('\u{feff}');
    let (status, body) = text.split_once('\n').unwrap_or((text, ""));
    match status.trim() {
        "OK" => {
            logger::write("PS-OK", body);
            Ok(body.trim().to_string())
        }
        "ERR" => {
            logger::error(&format!("{what}: {body}"));
            Err(AppError::from_ps(what, body))
        }
        _ => {
            logger::error(&format!("{what}: keine Rückmeldung vom Admin-Skript"));
            Err(AppError::new(
                format!("{what} – keine Rückmeldung"),
                "Die Aktion wurde möglicherweise abgebrochen. Versuche es noch einmal und bestätige die Windows-Abfrage mit „Ja“.",
            ))
        }
    }
}

