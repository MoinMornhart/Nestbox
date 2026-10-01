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

    // Wichtig: elevated-command nutzt ShellExecuteW und kehrt sofort zurück, ohne auf das
    // Ende des Admin-Skripts zu warten – das Ergebnis war dann noch nicht geschrieben
    // (Fehler „keine Rückmeldung“). Start-Process -Wait wartet wirklich und meldet einen
    // Abbruch im UAC-Dialog als Fehler.
    let launcher = format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         try {{\r\n\
           $p = Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File',{arg})\r\n\
           'EXIT ' + $p.ExitCode\r\n\
         }} catch {{\r\n\
           # 0x800704C7 (ERROR_CANCELLED): „Nein“ im UAC-Dialog\r\n\
           if ($_.Exception.Message -match 'abgebrochen|canceled|cancelled' -or $_.Exception.HResult -eq -2147023673) {{ 'ABGEBROCHEN' }} else {{ throw }}\r\n\
         }}",
        arg = ps::quote(&format!("\"{}\"", script_path.to_string_lossy())),
    );
    let launched = ps::run_quiet(&launcher);
    let text = fs::read_to_string(&out_path).unwrap_or_default();
    let _ = fs::remove_dir_all(&tmp);

    match &launched {
        Ok(o) if o.trim() == "ABGEBROCHEN" => {
            logger::error(&format!("{what}: UAC-Abfrage abgelehnt"));
            return Err(AppError::new(
                format!("{what} – Administratorrechte wurden nicht erteilt"),
                "Bestätige die Windows-Abfrage („Möchten Sie zulassen …?“) mit „Ja“. Ohne Adminrechte kann diese Einstellung nicht geändert werden.",
            ));
        }
        Ok(o) => logger::write("PS-ADMIN", o.trim()),
        Err(e) => {
            logger::error(&format!("{what}: Admin-Skript konnte nicht gestartet werden: {}", e.details.as_deref().unwrap_or(&e.title)));
            return Err(AppError::new(
                format!("{what} – das Admin-Skript konnte nicht gestartet werden"),
                "Versuche es noch einmal. Klappt es weiterhin nicht, starte Nestbox einmal per Rechtsklick → „Als Administrator ausführen“.",
            )
            .with_details(e.details.clone().unwrap_or_default()));
        }
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


/// Startet ein Programm mit Adminrechten, ohne auf das Ende zu warten (z. B. vmconnect).
pub fn spawn_program(program: &str, args: Vec<String>) {
    let program = program.to_string();
    std::thread::spawn(move || {
        let mut cmd = StdCommand::new(&program);
        cmd.args(args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }));
        if let Err(e) = ElevatedCommand::new(cmd).output() {
            logger::error(&format!("{program} mit Adminrechten starten: {e}"));
        }
    });
}
