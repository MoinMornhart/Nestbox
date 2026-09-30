//! App-Einstellungen und VM-Liste unter %LOCALAPPDATA%\Nestbox\.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

pub fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData").join("Local"))
        .join("Nestbox")
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\"))
}

pub fn default_vm_dir() -> PathBuf {
    home_dir().join("Nestbox").join("VMs")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    Virtualbox,
    Qemu,
    Hyperv,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BackendChoice {
    Auto,
    Virtualbox,
    Qemu,
    Hyperv,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OsFamily {
    Windows,
    Linux,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub vm_dir: String,
    pub backend: BackendChoice,
    /// Ordner, in dem qemu-system-x86_64.exe liegt (leer = automatisch suchen)
    pub qemu_dir: String,
    /// Wurde eine Windows-Funktion aktiviert, die einen Neustart braucht? (Zeitpunkt)
    pub reboot_pending_since: Option<chrono::DateTime<chrono::Utc>>,
    pub setup_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            vm_dir: default_vm_dir().to_string_lossy().to_string(),
            backend: BackendChoice::Auto,
            qemu_dir: String::new(),
            reboot_pending_since: None,
            setup_done: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub name: String,
    pub created: chrono::DateTime<chrono::Utc>,
    /// QEMU: Sicherungspunkt enthält den laufenden Zustand (savevm) statt nur der Festplatte
    #[serde(default)]
    pub with_state: bool,
}

/// Eine von Nestbox verwaltete VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmRecord {
    pub id: String,
    pub name: String,
    pub backend: BackendKind,
    pub os_family: OsFamily,
    /// ubuntu | mint | fedora | windows11 | custom
    pub os_id: String,
    pub iso_path: Option<String>,
    pub cpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    /// Ordner mit allen Dateien der VM
    pub dir: String,
    pub disk_path: String,
    pub created: chrono::DateTime<chrono::Utc>,
    /// VirtualBox: UUID der VM
    #[serde(default)]
    pub vbox_id: Option<String>,
    /// Hyper-V: VMId (GUID)
    #[serde(default)]
    pub hyperv_id: Option<String>,
    /// Aus dem Hyper-V-Manager übernommen (nicht von Nestbox angelegt) – Dateien nie selbst löschen
    #[serde(default)]
    pub imported: bool,
    /// QEMU: Port für die QMP-Steuerung
    #[serde(default)]
    pub qmp_port: Option<u16>,
    /// Sicherungspunkte mit Name und Datum
    #[serde(default)]
    pub snapshots: Vec<SnapshotMeta>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct VmList {
    vms: Vec<VmRecord>,
}

pub struct Store {
    pub settings: Settings,
    pub vms: Vec<VmRecord>,
}

fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

fn vms_path() -> PathBuf {
    data_dir().join("vms.json")
}

fn write_atomic(path: &PathBuf, content: &str) -> AppResult<()> {
    fs::create_dir_all(data_dir())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

impl Store {
    pub fn load() -> Self {
        let settings = fs::read_to_string(settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let vms = fs::read_to_string(vms_path())
            .ok()
            .and_then(|s| serde_json::from_str::<VmList>(&s).ok())
            .map(|l| l.vms)
            .unwrap_or_default();
        Self { settings, vms }
    }

    pub fn save_settings(&self) -> AppResult<()> {
        write_atomic(&settings_path(), &serde_json::to_string_pretty(&self.settings)?)
    }

    pub fn save_vms(&self) -> AppResult<()> {
        let list = VmList { vms: self.vms.clone() };
        write_atomic(&vms_path(), &serde_json::to_string_pretty(&list)?)
    }

    pub fn vm(&self, id: &str) -> Option<&VmRecord> {
        self.vms.iter().find(|v| v.id == id)
    }

    pub fn vm_mut(&mut self, id: &str) -> Option<&mut VmRecord> {
        self.vms.iter_mut().find(|v| v.id == id)
    }
}
