//! Tauri-Commands: die einzige Schnittstelle zwischen Oberfläche und VM-Operationen.
//! Alles, was PowerShell oder QEMU aufruft, läuft in einem Hintergrund-Thread.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::backend::qemu::QemuBackend;
use crate::backend::vbox::VBoxBackend;
use crate::backend::{self as backend, qmp, CreateProgress, CreateSpec, PowerState, Snapshot, VmBackend, VmChanges, VmStatus};
use crate::error::{AppError, AppResult};
use crate::host::{self, HostInfo};
use crate::logger;
use crate::store::{BackendKind, Settings, SnapshotMeta, Store, VmRecord};

pub struct AppState {
    pub store: Mutex<Store>,
}

pub type Shared<'a> = State<'a, Arc<AppState>>;

/// Führt blockierende Arbeit (PowerShell, QEMU) außerhalb des UI-Threads aus.
async fn blocking<T, F>(state: &Shared<'_>, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(Arc<AppState>) -> AppResult<T> + Send + 'static,
{
    let st = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || f(st))
        .await
        .map_err(|e| AppError::new("Interner Fehler", "Starte Nestbox neu.").with_details(e.to_string()))?
}

fn settings(st: &AppState) -> Settings {
    st.store.lock().unwrap_or_else(|e| e.into_inner()).settings.clone()
}

fn record(st: &AppState, id: &str) -> AppResult<VmRecord> {
    st.store
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .vm(id)
        .cloned()
        .ok_or_else(|| AppError::new("Diese VM gibt es nicht mehr", "Die Liste wird neu geladen."))
}

fn save_record(st: &AppState, vm: VmRecord) -> AppResult<()> {
    let mut store = st.store.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = store.vm_mut(&vm.id) {
        *r = vm;
    } else {
        store.vms.push(vm);
    }
    store.save_vms()
}

fn backend_for(kind: BackendKind, settings: &Settings) -> AppResult<Box<dyn VmBackend>> {
    Ok(match kind {
        BackendKind::Virtualbox => Box::new(VBoxBackend::from_settings(settings)?),
        BackendKind::Qemu => Box::new(QemuBackend::from_settings(settings)?),
    })
}

fn with_backend<T>(st: &AppState, id: &str, f: impl FnOnce(&dyn VmBackend, &VmRecord) -> AppResult<T>) -> AppResult<T> {
    let vm = record(st, id)?;
    let backend = backend_for(vm.backend, &settings(st))?;
    f(backend.as_ref(), &vm)
}

/// QEMU: Ist der gespeicherte QMP-Port von einem anderen Programm belegt, einen neuen wählen.
fn ensure_qemu_port(st: &AppState, vm: &mut VmRecord) -> AppResult<()> {
    if vm.backend != BackendKind::Qemu {
        return Ok(());
    }
    let port = vm.qmp_port.unwrap_or(0);
    let ours = port != 0 && qmp::Qmp::connect(port, std::time::Duration::from_secs(1)).is_ok();
    if port == 0 || (!ours && !qmp::port_is_free(port)) {
        vm.qmp_port = Some(qmp::free_port());
        save_record(st, vm.clone())?;
    }
    Ok(())
}

// ───────────────────────────── Einrichtung ─────────────────────────────

#[tauri::command]
pub async fn get_host_info(state: Shared<'_>) -> AppResult<HostInfo> {
    blocking(&state, |st| {
        let mut s = settings(&st);
        let before = s.reboot_pending_since;
        let info = host::detect(&mut s)?;
        if s.reboot_pending_since != before {
            let mut store = st.store.lock().unwrap_or_else(|e| e.into_inner());
            store.settings.reboot_pending_since = s.reboot_pending_since;
            store.save_settings()?;
        }
        Ok(info)
    })
    .await
}

/// Installiert VirtualBox oder QEMU per winget.
#[tauri::command]
pub async fn install_software(state: Shared<'_>, kind: BackendKind) -> AppResult<()> {
    blocking(&state, move |_| host::install_software(kind)).await
}

/// Schaltet die Windows-Hypervisor-Plattform (für schnelles QEMU) ein.
#[tauri::command]
pub async fn fix_enable_whpx(state: Shared<'_>) -> AppResult<()> {
    blocking(&state, move |st| {
        let mut s = settings(&st);
        host::enable_whpx(&mut s)?;
        let mut store = st.store.lock().unwrap_or_else(|e| e.into_inner());
        store.settings.reboot_pending_since = s.reboot_pending_since;
        store.save_settings()
    })
    .await
}

#[tauri::command]
pub async fn restart_computer(state: Shared<'_>) -> AppResult<()> {
    blocking(&state, |_| host::restart_computer()).await
}

// ───────────────────────────── Einstellungen ─────────────────────────────

#[tauri::command]
pub fn get_settings(state: Shared<'_>) -> Settings {
    settings(&state)
}

#[tauri::command]
pub fn save_settings(state: Shared<'_>, new_settings: Settings) -> AppResult<Settings> {
    let dir = new_settings.vm_dir.trim().to_string();
    if dir.is_empty() {
        return Err(AppError::new("Bitte wähle einen Ordner für die VMs", "Klicke auf „Ändern“ und wähle einen Ordner aus."));
    }
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::new("Der VM-Ordner kann nicht angelegt werden", "Wähle einen Ordner, in dem du schreiben darfst.")
            .with_details(e.to_string())
    })?;
    let mut store = state.store.lock().unwrap_or_else(|e| e.into_inner());
    let pending = store.settings.reboot_pending_since;
    store.settings = new_settings;
    store.settings.vm_dir = dir;
    store.settings.reboot_pending_since = pending;
    store.save_settings()?;
    logger::info("Einstellungen gespeichert");
    Ok(store.settings.clone())
}

#[tauri::command]
pub fn get_log_path() -> String {
    logger::log_file().to_string_lossy().to_string()
}

// ───────────────────────────── VMs ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VmView {
    #[serde(flatten)]
    pub record: VmRecord,
    pub status: VmStatus,
}

fn off_status(id: &str) -> VmStatus {
    VmStatus { id: id.into(), state: PowerState::Unknown, cpu_percent: 0.0, memory_used_mb: 0, uptime_seconds: 0 }
}

fn collect_status(st: &AppState) -> Vec<VmStatus> {
    let (vms, s) = {
        let store = st.store.lock().unwrap_or_else(|e| e.into_inner());
        (store.vms.clone(), store.settings.clone())
    };
    let mut out = Vec::new();
    for kind in [BackendKind::Virtualbox, BackendKind::Qemu] {
        let group: Vec<VmRecord> = vms.iter().filter(|v| v.backend == kind).cloned().collect();
        if group.is_empty() {
            continue;
        }
        match backend_for(kind, &s).and_then(|b| b.status(&group)) {
            Ok(list) => out.extend(list),
            Err(e) => {
                logger::error(&format!("Status: {} – {}", e.title, e.details.unwrap_or_default()));
                out.extend(group.iter().map(|v| off_status(&v.id)));
            }
        }
    }
    out
}

#[tauri::command]
pub async fn list_vms(state: Shared<'_>) -> AppResult<Vec<VmView>> {
    blocking(&state, |st| {
        let status = collect_status(&st);
        let store = st.store.lock().unwrap_or_else(|e| e.into_inner());
        Ok(store
            .vms
            .iter()
            .map(|r| VmView {
                record: r.clone(),
                status: status.iter().find(|s| s.id == r.id).cloned().unwrap_or_else(|| off_status(&r.id)),
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn vm_status(state: Shared<'_>) -> AppResult<Vec<VmStatus>> {
    blocking(&state, |st| Ok(collect_status(&st))).await
}

/// Prüft einen Namen, bevor die VM angelegt oder umbenannt wird. Liefert eine Fehlermeldung oder null.
#[tauri::command]
pub async fn check_vm_name(state: Shared<'_>, name: String, backend: BackendKind, except_id: Option<String>) -> AppResult<Option<String>> {
    blocking(&state, move |st| {
        let n = name.trim();
        if n.is_empty() {
            return Ok(Some("Bitte gib einen Namen ein.".into()));
        }
        if n.chars().count() > 60 {
            return Ok(Some("Der Name darf höchstens 60 Zeichen lang sein.".into()));
        }
        if n.chars().any(|c| matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\'')) {
            return Ok(Some("Diese Zeichen sind nicht erlaubt: < > : \" / \\ | ? * '".into()));
        }
        let store_has = {
            let store = st.store.lock().unwrap_or_else(|e| e.into_inner());
            store.vms.iter().any(|v| v.name.eq_ignore_ascii_case(n) && Some(&v.id) != except_id.as_ref())
        };
        if store_has {
            return Ok(Some("Du hast schon eine VM mit diesem Namen.".into()));
        }
        let _ = backend;
        if except_id.is_none() {
            let dir = std::path::Path::new(&settings(&st).vm_dir).join(crate::backend::safe_file_name(n));
            if dir.exists() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(false) {
                return Ok(Some("Im VM-Ordner gibt es schon einen Ordner mit diesem Namen.".into()));
            }
        }
        Ok(None)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsoInfo {
    pub path: String,
    pub file_name: String,
    pub size_mb: u64,
}

#[tauri::command]
pub fn inspect_iso(path: String) -> AppResult<IsoInfo> {
    let p = std::path::Path::new(&path);
    let meta = std::fs::metadata(p).map_err(|e| {
        AppError::new("Die Datei wurde nicht gefunden", "Wähle die ISO-Datei erneut aus.").with_details(e.to_string())
    })?;
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !meta.is_file() || ext != "iso" {
        return Err(AppError::new(
            "Das ist keine ISO-Datei",
            "Wähle eine Datei mit der Endung „.iso“ – das ist das Abbild einer Installations-DVD, wie es die Betriebssystem-Hersteller zum Download anbieten.",
        ));
    }
    Ok(IsoInfo {
        path: path.clone(),
        file_name: p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
        size_mb: meta.len() / 1024 / 1024,
    })
}

#[tauri::command]
pub async fn create_vm(state: Shared<'_>, spec: CreateSpec, backend: BackendKind, on_progress: Channel<CreateProgress>) -> AppResult<VmView> {
    blocking(&state, move |st| {
        let s = settings(&st);
        std::fs::create_dir_all(&s.vm_dir).map_err(|e| {
            AppError::new("Der VM-Ordner kann nicht angelegt werden", "Wähle in den Einstellungen einen anderen Ordner.")
                .with_details(e.to_string())
        })?;
        logger::info(&format!("VM anlegen: {} ({:?}, {} Kerne, {} MB, {} GB)", spec.name, backend, spec.cpus, spec.memory_mb, spec.disk_gb));
        let b = backend_for(backend, &s)?;
        let id = uuid::Uuid::new_v4().to_string();
        let send = |step: &str, label: &str, state_: &str| {
            let _ = on_progress.send(CreateProgress { step: step.into(), label: label.into(), state: state_.into() });
        };
        let mut rec = b.create(&spec, &s, &id, &send)?;
        save_record(&st, rec.clone())?;

        send("start", "VM starten", "active");
        ensure_qemu_port(&st, &mut rec)?;
        b.start(&rec)?;
        send("start", "VM starten", "done");
        logger::info(&format!("VM „{}“ angelegt und gestartet", rec.name));

        let status = b.status(std::slice::from_ref(&rec)).ok().and_then(|mut v| v.pop()).unwrap_or_else(|| off_status(&rec.id));
        Ok(VmView { record: rec, status })
    })
    .await
}

#[tauri::command]
pub async fn start_vm(state: Shared<'_>, id: String) -> AppResult<()> {
    blocking(&state, move |st| {
        let mut vm = record(&st, &id)?;
        ensure_qemu_port(&st, &mut vm)?;
        backend_for(vm.backend, &settings(&st))?.start(&vm)
    })
    .await
}

#[tauri::command]
pub async fn pause_vm(state: Shared<'_>, id: String) -> AppResult<()> {
    blocking(&state, move |st| with_backend(&st, &id, |b, vm| b.pause(vm))).await
}

#[tauri::command]
pub async fn resume_vm(state: Shared<'_>, id: String) -> AppResult<()> {
    blocking(&state, move |st| with_backend(&st, &id, |b, vm| b.resume(vm))).await
}

#[tauri::command]
pub async fn shutdown_vm(state: Shared<'_>, id: String) -> AppResult<()> {
    blocking(&state, move |st| with_backend(&st, &id, |b, vm| b.shutdown(vm))).await
}

#[tauri::command]
pub async fn poweroff_vm(state: Shared<'_>, id: String) -> AppResult<()> {
    blocking(&state, move |st| with_backend(&st, &id, |b, vm| b.power_off(vm))).await
}

#[tauri::command]
pub async fn open_console(state: Shared<'_>, id: String) -> AppResult<()> {
    blocking(&state, move |st| with_backend(&st, &id, |b, vm| b.open_console(vm))).await
}

#[tauri::command]
pub async fn rename_vm(state: Shared<'_>, id: String, name: String) -> AppResult<()> {
    blocking(&state, move |st| {
        let name = name.trim().to_string();
        let mut vm = record(&st, &id)?;
        if vm.name == name {
            return Ok(());
        }
        backend_for(vm.backend, &settings(&st))?.rename(&vm, &name)?;
        vm.name = name;
        save_record(&st, vm)
    })
    .await
}

#[tauri::command]
pub async fn update_vm(state: Shared<'_>, id: String, changes: VmChanges) -> AppResult<()> {
    blocking(&state, move |st| {
        let mut vm = record(&st, &id)?;
        if changes.disk_gb < vm.disk_gb {
            return Err(AppError::new(
                "Die Festplatte kann nicht verkleinert werden",
                "Du kannst sie nur vergrößern. Wähle mindestens die aktuelle Größe.",
            ));
        }
        backend_for(vm.backend, &settings(&st))?.update(&vm, &changes)?;
        vm.cpus = changes.cpus;
        vm.memory_mb = changes.memory_mb;
        vm.disk_gb = changes.disk_gb;
        save_record(&st, vm)
    })
    .await
}

#[tauri::command]
pub async fn eject_iso(state: Shared<'_>, id: String) -> AppResult<()> {
    blocking(&state, move |st| {
        let mut vm = record(&st, &id)?;
        backend_for(vm.backend, &settings(&st))?.eject_iso(&vm)?;
        vm.iso_path = None;
        save_record(&st, vm)
    })
    .await
}

/// Gasterweiterungen (Treiber für flüssiges Bild) in die VM einlegen.
#[tauri::command]
pub async fn install_guest_tools(state: Shared<'_>, id: String) -> AppResult<()> {
    blocking(&state, move |st| with_backend(&st, &id, |b, vm| b.install_guest_tools(vm))).await
}

#[tauri::command]
pub async fn delete_vm(state: Shared<'_>, id: String, delete_disk: bool) -> AppResult<()> {
    blocking(&state, move |st| {
        let vm = record(&st, &id)?;
        logger::info(&format!("VM löschen: {} (Festplatte löschen: {delete_disk})", vm.name));
        // Auch wenn die VM im Backend schon fehlt (oder QEMU deinstalliert wurde): Eintrag trotzdem entfernen.
        let result = match backend_for(vm.backend, &settings(&st)) {
            Ok(b) => b.delete(&vm, delete_disk),
            Err(_) if delete_disk => backend::remove_vm_files(&vm),
            Err(_) => Ok(()),
        };
        {
            let mut store = st.store.lock().unwrap_or_else(|e| e.into_inner());
            store.vms.retain(|v| v.id != id);
            store.save_vms()?;
        }
        result
    })
    .await
}

// ───────────────────────────── Sicherungspunkte ─────────────────────────────

#[tauri::command]
pub async fn list_snapshots(state: Shared<'_>, id: String) -> AppResult<Vec<Snapshot>> {
    blocking(&state, move |st| Ok(backend::list_snapshots(&record(&st, &id)?))).await
}

#[tauri::command]
pub async fn create_snapshot(state: Shared<'_>, id: String, name: String) -> AppResult<Snapshot> {
    blocking(&state, move |st| {
        let mut vm = record(&st, &id)?;
        let name = if name.trim().is_empty() {
            format!("Sicherungspunkt vom {}", chrono::Local::now().format("%d.%m.%Y, %H:%M"))
        } else {
            name.trim().to_string()
        };
        let snap = backend_for(vm.backend, &settings(&st))?.create_snapshot(&vm, &name)?;
        vm.snapshots.push(SnapshotMeta {
                id: snap.id.clone(),
                name: snap.name.clone(),
                created: chrono::Utc::now(),
                with_state: snap.with_state,
            });
        save_record(&st, vm)?;
        Ok(snap)
    })
    .await
}

#[tauri::command]
pub async fn restore_snapshot(state: Shared<'_>, id: String, snapshot_id: String) -> AppResult<()> {
    blocking(&state, move |st| {
        let mut vm = record(&st, &id)?;
        ensure_qemu_port(&st, &mut vm)?;
        backend_for(vm.backend, &settings(&st))?.restore_snapshot(&vm, &snapshot_id)
    })
    .await
}

#[tauri::command]
pub async fn delete_snapshot(state: Shared<'_>, id: String, snapshot_id: String) -> AppResult<()> {
    blocking(&state, move |st| {
        let mut vm = record(&st, &id)?;
        backend_for(vm.backend, &settings(&st))?.delete_snapshot(&vm, &snapshot_id)?;
        vm.snapshots.retain(|s| s.id != snapshot_id);
        save_record(&st, vm)?;
        Ok(())
    })
    .await
}
