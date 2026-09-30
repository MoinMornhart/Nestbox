//! Gemeinsame Schnittstelle für alle Virtualisierungs-Backends (VirtualBox, QEMU)
//! und Hilfsfunktionen, die beide nutzen.

pub mod hyperv;
pub mod qemu;
pub mod qmp;
pub mod vbox;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::logger;
use crate::ps::{self, hidden_command, quote};
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
    /// CPU-Auslastung der VM in Prozent des ganzen PCs (0–100)
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
    /// Treiber-CD für flüssige Grafik, Ton und Zwischenablage in die VM einlegen
    fn install_guest_tools(&self, vm: &VmRecord) -> AppResult<()>;
    fn delete(&self, vm: &VmRecord, delete_disk: bool) -> AppResult<()>;
    /// Sicherungspunkte direkt aus dem Backend (Hyper-V). None = Nestbox-Liste verwenden.
    fn native_snapshots(&self, _vm: &VmRecord) -> AppResult<Option<Vec<Snapshot>>> {
        Ok(None)
    }
    /// Liefert die Metadaten des neuen Sicherungspunkts (Nestbox speichert sie in der VM-Liste)
    fn create_snapshot(&self, vm: &VmRecord, name: &str) -> AppResult<Snapshot>;
    fn restore_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()>;
    fn delete_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()>;
}

/// Sicherungspunkte stehen bei beiden Backends in der VM-Liste von Nestbox.
pub fn list_snapshots(vm: &VmRecord) -> Vec<Snapshot> {
    let mut list: Vec<Snapshot> = vm
        .snapshots
        .iter()
        .map(|s| Snapshot { id: s.id.clone(), name: s.name.clone(), created: s.created.to_rfc3339(), with_state: s.with_state })
        .collect();
    list.sort_by(|a, b| b.created.cmp(&a.created));
    list
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

/// Führt ein Hilfsprogramm (VBoxManage, qemu-img) ohne Konsolenfenster aus und protokolliert es.
/// `hint` ist der Lösungsvorschlag, falls das Programm fehlschlägt.
pub fn run_tool(exe: &Path, args: &[String], what: &str, hint: &str) -> AppResult<String> {
    let shown: Vec<String> = args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect();
    let tag = exe.file_stem().map(|s| s.to_string_lossy().to_uppercase()).unwrap_or_default();
    logger::write(&tag, &format!("\"{}\" {}", exe.display(), shown.join(" ")));
    let out = hidden_command(&exe.to_string_lossy()).args(args).output().map_err(|e| {
        AppError::new(what, format!("{} wurde nicht gefunden. Installiere das Programm neu (Einrichtung).", exe.display()))
            .with_details(e.to_string())
    })?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() {
        logger::error(&format!("{what}: {stderr}{stdout}"));
        return Err(AppError::new(what, hint).with_details(format!("{stderr}{stdout}")));
    }
    if !stdout.trim().is_empty() {
        logger::write(&format!("{tag}-OK"), &ps::truncate(stdout.trim(), 1500));
    }
    Ok(stdout)
}

// ───────────── Prozesse (für CPU-/RAM-Anzeige und „Fenster zeigen“) ─────────────

#[derive(Debug, Deserialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub cmd: String,
    pub cpu: u64,
    pub mem: u64,
    pub up: u64,
}

/// Alle laufenden Prozesse eines Programms (z. B. „VirtualBoxVM.exe“) mit CPU-Zeit und RAM.
pub fn process_info(image: &str) -> Vec<ProcInfo> {
    let script = format!(
        "$r = @(Get-CimInstance Win32_Process -Filter ('Name=' + [char]39 + {img} + [char]39) | ForEach-Object {{\r\n\
           [pscustomobject]@{{ pid = [uint32]$_.ProcessId; cmd = [string]$_.CommandLine; cpu = [uint64]($_.KernelModeTime + $_.UserModeTime); mem = [uint64]($_.WorkingSetSize / 1MB); up = [uint64]((Get-Date) - $_.CreationDate).TotalSeconds }}\r\n\
         }})\r\n\
         ConvertTo-Json -InputObject $r -Compress",
        img = quote(image)
    );
    ps::run_quiet(&script)
        .ok()
        .and_then(|o| serde_json::from_str::<Vec<ProcInfo>>(if o.is_empty() { "[]" } else { &o }).ok())
        .unwrap_or_default()
}

static CPU_SAMPLES: Mutex<Option<HashMap<u32, (u64, Instant)>>> = Mutex::new(None);

/// CPU-Auslastung eines Prozesses seit der letzten Messung, in Prozent des ganzen PCs.
pub fn cpu_percent(pid: u32, cpu_time_100ns: u64) -> f64 {
    let cores = std::thread::available_parallelism().map(|n| n.get() as f64).unwrap_or(1.0);
    let mut guard = CPU_SAMPLES.lock().unwrap_or_else(|e| e.into_inner());
    let samples = guard.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    let pct = match samples.get(&pid) {
        Some((prev, t)) => {
            let wall = now.duration_since(*t).as_secs_f64();
            let used = cpu_time_100ns.saturating_sub(*prev) as f64 / 10_000_000.0;
            if wall > 0.0 { (used / wall / cores * 100.0).clamp(0.0, 100.0) } else { 0.0 }
        }
        None => 0.0,
    };
    samples.insert(pid, (cpu_time_100ns, now));
    (pct * 10.0).round() / 10.0
}

/// Holt das Fenster eines Prozesses in den Vordergrund, dessen Befehlszeile `marker` enthält.
pub fn bring_to_front(image: &str, marker: &str) -> AppResult<bool> {
    let out = ps::run(
        "VM-Fenster anzeigen",
        &format!(
            "$p = Get-CimInstance Win32_Process -Filter ('Name=' + [char]39 + {img} + [char]39) | Where-Object {{ $_.CommandLine -like ('*' + {m} + '*') }} | Select-Object -First 1\r\n\
             if ($null -eq $p) {{ 'nein' }} else {{ $null = (New-Object -ComObject WScript.Shell).AppActivate([int]$p.ProcessId); 'ja' }}",
            img = quote(image),
            m = quote(marker)
        ),
    )?;
    Ok(out.trim() == "ja")
}

/// Löscht die Dateien einer VM. Der ganze Ordner wird nur entfernt, wenn er
/// eindeutig zu dieser VM gehört (von Nestbox angelegt, enthält die Festplatte).
pub fn remove_vm_files(vm: &VmRecord) -> AppResult<()> {
    if vm.imported {
        // Übernommene VMs: Nestbox kennt deren Ordnerstruktur nicht – nur die Festplatte selbst löschen.
        let disk = PathBuf::from(&vm.disk_path);
        if disk.is_file() {
            fs::remove_file(&disk)?;
        }
        return Ok(());
    }
    let dir = PathBuf::from(&vm.dir);
    let disk = PathBuf::from(&vm.disk_path);
    let owns_dir = !vm.dir.is_empty() && disk.starts_with(&dir) && dir.components().count() >= 3;

    // Die Virtualisierung gibt Dateien manchmal erst nach einem Moment frei.
    let mut last_err = None;
    for _ in 0..10 {
        let res = if owns_dir {
            if dir.exists() { fs::remove_dir_all(&dir) } else { Ok(()) }
        } else if disk.exists() {
            fs::remove_file(&disk)
        } else {
            Ok(())
        };
        match res {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }
    Err(AppError::new(
        "Die VM wurde entfernt, aber ihre Dateien konnten nicht gelöscht werden",
        format!("Lösche den Ordner „{}“ bei Bedarf von Hand.", vm.dir),
    )
    .with_details(last_err.map(|e| e.to_string()).unwrap_or_default()))
}
