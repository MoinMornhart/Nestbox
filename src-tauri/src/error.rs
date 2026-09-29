use serde::Serialize;

/// Fehler, wie ihn die Oberfläche anzeigt: verständlicher Titel, konkreter
/// Lösungsvorschlag und (ausklappbar) die technischen Details.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub title: String,
    pub hint: String,
    pub details: Option<String>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(title: impl Into<String>, hint: impl Into<String>) -> Self {
        Self { title: title.into(), hint: hint.into(), details: None }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        let d = details.into();
        if !d.trim().is_empty() {
            self.details = Some(d.trim().to_string());
        }
        self
    }

    /// Übersetzt typische PowerShell-Fehlermeldungen in einen
    /// verständlichen Hinweis. `title` beschreibt, was gerade versucht wurde.
    pub fn from_ps(title: impl Into<String>, stderr: &str) -> Self {
        let lower = stderr.to_lowercase();
        let hint = if lower.contains("zugriff verweigert") || lower.contains("access is denied") || lower.contains("keine berechtigung") {
            "Windows hat den Zugriff verweigert. Prüfe, ob der VM-Ordner beschreibbar ist, oder wähle in den Einstellungen einen anderen Ordner."
        } else if lower.contains("nicht genügend arbeitsspeicher")
            || lower.contains("not enough memory")
            || lower.contains("insufficient system resources")
        {
            "Es ist gerade nicht genug freier Arbeitsspeicher da. Schließe andere Programme oder VMs, oder gib der VM in den Einstellungen weniger Arbeitsspeicher."
        } else if lower.contains("already exists") || lower.contains("bereits vorhanden") || lower.contains("ist bereits") {
            "Es gibt bereits etwas mit diesem Namen. Wähle einen anderen Namen."
        } else if lower.contains("wird von einem anderen prozess verwendet") || lower.contains("being used by another process") {
            "Eine Datei ist gerade von einem anderen Programm geöffnet. Schließe das Programm und versuche es erneut."
        } else if lower.contains("ungültigen zustand") || lower.contains("invalid state") {
            "Die VM ist gerade im falschen Zustand für diese Aktion. Warte einen Moment oder schalte sie zuerst aus."
        } else {
            "Versuche es noch einmal. Wenn der Fehler bleibt, hilft die Log-Datei (Einstellungen → „Log-Ordner öffnen“) bei der Fehlersuche."
        };
        AppError::new(title, hint).with_details(stderr)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new(
            "Eine Datei oder ein Ordner konnte nicht gelesen oder geschrieben werden",
            "Prüfe, ob genug Speicherplatz frei ist und ob der Ordner nicht schreibgeschützt ist.",
        )
        .with_details(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new(
            "Eine Antwort konnte nicht gelesen werden",
            "Versuche es noch einmal. Bleibt der Fehler, schau in die Log-Datei.",
        )
        .with_details(e.to_string())
    }
}
