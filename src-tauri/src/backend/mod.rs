//! Gemeinsame Schnittstelle für alle Virtualisierungs-Backends.

pub mod hyperv;
pub mod qemu;
pub mod qmp;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::store::{OsFamily, Settings, VmRecord};

/// Was der Nutzer im Assistenten gewählt hat.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSpec {
    pub name: String,
    pub os_family: OsFamily,
    pub os_id: String,
    pub iso_path: Option<String>,
    pub cpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PowerState {
    Running,
    Paused,
    Off,
    Starting,
    Stopping,
    Saving,
    Unknown,
    /// Die VM existiert im Backend nicht mehr (z. B. außerhalb von Nestbox gelöscht)
    Missing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VmStatus {
    pub id: String,
    pub state: PowerState,
    /// CPU-Auslastung der VM in Prozent (0–100)
    pub cpu_percent: f64,
    pub memory_used_mb: u64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    /// ISO-8601
    pub created: String,
    pub with_state: bool,
}

/// Fortschritt beim Anlegen – ein Schritt nach dem anderen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProgress {
    pub step: String,
    pub label: String,
    /// active | done | error
    pub state: String,
}

pub type Progress<'a> = &'a dyn Fn(&str, &str, &str);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmChanges {
    pub cpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
}

pub trait VmBackend: Send + Sync {
    /// Legt die VM an (Festplatte, Konfiguration, Installationsmedium).
    fn create(&self, spec: &CreateSpec, settings: &Settings, id: &str, progress: Progress) -> AppResult<VmRecord>;
    fn start(&self, vm: &VmRecord) -> AppResult<()>;
    fn pause(&self, vm: &VmRecord) -> AppResult<()>;
    fn resume(&self, vm: &VmRecord) -> AppResult<()>;
    /// Sauber herunterfahren (wie der Ein/Aus-Knopf am PC)
    fn shutdown(&self, vm: &VmRecord) -> AppResult<()>;
    /// Sofort ausschalten (wie Stecker ziehen)
    fn power_off(&self, vm: &VmRecord) -> AppResult<()>;
    /// Bildschirm der VM anzeigen
    fn open_console(&self, vm: &VmRecord) -> AppResult<()>;
    /// Status aller VMs dieses Backends auf einmal abfragen
    fn status(&self, vms: &[VmRecord]) -> AppResult<Vec<VmStatus>>;
    fn rename(&self, vm: &VmRecord, new_name: &str) -> AppResult<()>;
    fn update(&self, vm: &VmRecord, changes: &VmChanges) -> AppResult<()>;
    fn eject_iso(&self, vm: &VmRecord) -> AppResult<()>;
    fn delete(&self, vm: &VmRecord, delete_disk: bool) -> AppResult<()>;
    fn list_snapshots(&self, vm: &VmRecord) -> AppResult<Vec<Snapshot>>;
    /// Liefert die Metadaten des neuen Sicherungspunkts
    fn create_snapshot(&self, vm: &VmRecord, name: &str) -> AppResult<Snapshot>;
    fn restore_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()>;
    fn delete_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()>;
}

/// Ersetzt Zeichen, die in Datei- und VM-Namen Probleme machen.
pub fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '-' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() { "VM".into() } else { trimmed }
}
