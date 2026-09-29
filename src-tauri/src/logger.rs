//! Einfache Log-Datei unter %LOCALAPPDATA%\Nestbox\logs\nestbox.log.
//! Jeder PowerShell- und QEMU-Befehl wird hier mit Ergebnis protokolliert.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::store::data_dir;

static LOCK: Mutex<()> = Mutex::new(());
const MAX_SIZE: u64 = 5 * 1024 * 1024;

pub fn log_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn log_file() -> PathBuf {
    log_dir().join("nestbox.log")
}

pub fn write(kind: &str, text: &str) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = log_dir();
    let _ = fs::create_dir_all(&dir);
    let file = log_file();
    // Bei 5 MB rotieren, damit die Datei nicht endlos wächst.
    if fs::metadata(&file).map(|m| m.len() > MAX_SIZE).unwrap_or(false) {
        let _ = fs::rename(&file, dir.join("nestbox.old.log"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&file) {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let body = text.trim_end().replace('\n', "\n    ");
        let _ = writeln!(f, "[{ts}] {kind:<8} {body}");
    }
}

pub fn info(text: &str) {
    write("INFO", text);
}

pub fn error(text: &str) {
    write("FEHLER", text);
}
