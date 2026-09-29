//! PowerShell-Aufrufe: powershell.exe -NoProfile -NonInteractive -Command …
//! Ohne Konsolenfenster (CREATE_NO_WINDOW), Ergebnisse als JSON.

use std::process::{Command, Output};

use serde::de::DeserializeOwned;

use crate::error::{AppError, AppResult};
use crate::logger;

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Setzt einen Wert sicher in einfache Anführungszeichen (PowerShell-Literal).
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''").replace(['\u{2018}', '\u{2019}'], "''"))
}

/// Erzeugt ein `Command` ohne sichtbares Konsolenfenster.
pub fn hidden_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn wrap(script: &str) -> String {
    // UTF-8-Ausgabe (Umlaute in Fehlermeldungen), Fehler als reiner Text auf stderr.
    format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         $ProgressPreference = 'SilentlyContinue'; \
         $ErrorActionPreference = 'Stop'; \
         try {{ {script} }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}"
    )
}

fn exec(script: &str) -> std::io::Result<Output> {
    hidden_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &wrap(script)])
        .output()
}

/// Führt ein Skript aus und liefert stdout. `what` beschreibt die Aktion für die Fehlermeldung.
pub fn run(what: &str, script: &str) -> AppResult<String> {
    logger::write("PS", script);
    let out = exec(script).map_err(|e| {
        AppError::new(
            "PowerShell konnte nicht gestartet werden",
            "Nestbox braucht Windows PowerShell. Prüfe, ob powershell.exe vorhanden ist.",
        )
        .with_details(e.to_string())
    })?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        logger::error(&format!("{what}: {stderr}"));
        return Err(AppError::from_ps(what, &stderr));
    }
    if !stdout.is_empty() {
        logger::write("PS-OK", &truncate(&stdout, 2000));
    }
    Ok(stdout)
}

/// Führt ein Skript aus, das JSON ausgibt (… | ConvertTo-Json), und parst es.
pub fn run_json<T: DeserializeOwned>(what: &str, script: &str) -> AppResult<T> {
    let out = run(what, script)?;
    let text = if out.is_empty() { "null" } else { out.as_str() };
    serde_json::from_str(text).map_err(|e| {
        AppError::new(what, "Die Antwort von Windows war unerwartet. Versuche es noch einmal.")
            .with_details(format!("{e}\n\nAusgabe:\n{}", truncate(text, 2000)))
    })
}

/// Wie `run`, protokolliert aber nichts (für die 2-Sekunden-Statusabfrage, damit das Log lesbar bleibt).
pub fn run_quiet(script: &str) -> AppResult<String> {
    let out = exec(script)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::from_ps("Status konnte nicht abgefragt werden", &stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t} …")
    }
}

/// Ist die Fehlermeldung ein Rechteproblem? Dann lohnt sich ein zweiter Versuch mit UAC.
pub fn is_access_denied(err: &AppError) -> bool {
    let d = err.details.as_deref().unwrap_or("").to_lowercase();
    d.contains("access is denied")
        || d.contains("zugriff verweigert")
        || d.contains("autorisierungsrichtlinie")
        || d.contains("authorization policy")
        || d.contains("0x80070005")
        || d.contains("keine berechtigung")
}
