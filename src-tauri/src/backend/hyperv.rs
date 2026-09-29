//! Hyper-V-Backend (Windows Pro/Enterprise/Education) – alles über PowerShell.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{safe_file_name, CreateSpec, PowerState, Progress, Snapshot, VmBackend, VmChanges, VmStatus};
use crate::elevate;
use crate::error::{AppError, AppResult};
use crate::logger;
use crate::ps::{self, quote};
use crate::store::{BackendKind, OsFamily, Settings, VmRecord};

pub struct HyperVBackend;

const MISSING: &str = "NESTBOX_VM_FEHLT";

/// Skript-Vorspann: holt die VM über ihre feste ID (überlebt Umbenennen).
fn with_vm(vm: &VmRecord, body: &str) -> String {
    let id = vm.hyperv_id.clone().unwrap_or_default();
    format!(
        "$vm = Get-VM -Id {id} -ErrorAction SilentlyContinue\r\n\
         if ($null -eq $vm) {{ throw '{MISSING}' }}\r\n\
         {body}",
        id = quote(&id)
    )
}

fn run_vm(what: &str, vm: &VmRecord, body: &str) -> AppResult<String> {
    ps::run(what, &with_vm(vm, body)).map_err(|e| map_missing(e, vm))
}

fn map_missing(e: AppError, vm: &VmRecord) -> AppError {
    if e.details.as_deref().unwrap_or("").contains(MISSING) {
        AppError::new(
            format!("„{}“ wurde in Hyper-V nicht gefunden", vm.name),
            "Die VM wurde vermutlich außerhalb von Nestbox gelöscht. Du kannst sie über das Menü „…“ → „Löschen“ aus der Liste entfernen.",
        )
    } else {
        e
    }
}

impl HyperVBackend {
    /// Gibt es in Hyper-V schon eine VM mit diesem Namen?
    pub fn name_exists(name: &str) -> AppResult<bool> {
        let out = ps::run(
            "Namen prüfen",
            &format!("@(Get-VM -Name {} -ErrorAction SilentlyContinue).Count", quote(name)),
        )?;
        Ok(out.trim() != "0" && !out.trim().is_empty())
    }

    fn vmconnect(name: &str) -> AppResult<()> {
        let exe = std::env::var("SystemRoot")
            .map(|r| PathBuf::from(r).join("System32").join("vmconnect.exe"))
            .unwrap_or_else(|_| PathBuf::from("vmconnect.exe"));
        logger::write("CMD", &format!("{} localhost \"{name}\"", exe.display()));
        match std::process::Command::new(&exe).arg("localhost").arg(name).spawn() {
            Ok(_) => Ok(()),
            // 740 = ERROR_ELEVATION_REQUIRED: vmconnect verlangt auf manchen Systemen Adminrechte.
            Err(e) if e.raw_os_error() == Some(740) => {
                elevate::spawn_program(&exe.to_string_lossy(), vec!["localhost".into(), name.into()]);
                Ok(())
            }
            Err(e) => Err(AppError::new(
                "Das VM-Fenster konnte nicht geöffnet werden",
                "Prüfe, ob die Hyper-V-Verwaltungstools installiert sind (Einrichtung → Hyper-V aktivieren).",
            )
            .with_details(e.to_string())),
        }
    }
}

#[derive(Deserialize)]
struct Created {
    id: String,
    switch: String,
}

#[derive(Deserialize)]
struct RawStatus {
    id: String,
    state: String,
    cpu: f64,
    mem: u64,
    up: u64,
}

#[derive(Deserialize)]
struct RawSnapshot {
    id: String,
    name: String,
    created: String,
}

impl VmBackend for HyperVBackend {
    fn create(&self, spec: &CreateSpec, settings: &Settings, id: &str, progress: Progress) -> AppResult<VmRecord> {
        let name = spec.name.trim().to_string();
        let vm_root = PathBuf::from(&settings.vm_dir);
        let dir = vm_root.join(safe_file_name(&name));
        let disk_dir = dir.join("Virtual Hard Disks");
        let disk = disk_dir.join(format!("{}.vhdx", safe_file_name(&name)));

        // 1. Ordner
        progress("folder", "Ordner vorbereiten", "active");
        if Self::name_exists(&name)? {
            return Err(AppError::new(
                format!("In Hyper-V gibt es schon eine VM namens „{name}“"),
                "Gehe einen Schritt zurück und wähle einen anderen Namen.",
            ));
        }
        if disk.exists() {
            return Err(AppError::new(
                "Für diesen Namen existiert schon eine virtuelle Festplatte",
                "Wähle einen anderen Namen oder lösche den alten Ordner.",
            )
            .with_details(disk.display().to_string()));
        }
        fs::create_dir_all(&disk_dir)?;
        progress("folder", "Ordner vorbereiten", "done");

        // 2. Festplatte (dynamisch: belegt nur so viel Platz, wie wirklich genutzt wird)
        progress("disk", "Virtuelle Festplatte anlegen", "active");
        ps::run(
            "Virtuelle Festplatte anlegen",
            &format!(
                "New-VHD -Path {} -SizeBytes {} -Dynamic | Out-Null",
                quote(&disk.to_string_lossy()),
                spec.disk_gb * 1024 * 1024 * 1024
            ),
        )?;
        progress("disk", "Virtuelle Festplatte anlegen", "done");

        // 3. VM anlegen (Generation 2, Default Switch, keine automatischen Checkpoints)
        progress("vm", "Virtuelle Maschine anlegen", "active");
        let created: Created = ps::run_json(
            "Virtuelle Maschine anlegen",
            &format!(
                "$sw = Get-VMSwitch -Name 'Default Switch' -ErrorAction SilentlyContinue\r\n\
                 if ($null -eq $sw) {{ $sw = Get-VMSwitch -ErrorAction SilentlyContinue | Select-Object -First 1 }}\r\n\
                 $p = @{{ Name = {name}; Generation = 2; MemoryStartupBytes = {mem}; VHDPath = {vhd}; Path = {path} }}\r\n\
                 if ($null -ne $sw) {{ $p.SwitchName = $sw.Name }}\r\n\
                 $vm = New-VM @p\r\n\
                 Set-VMProcessor -VM $vm -Count {cpus}\r\n\
                 Set-VMMemory -VM $vm -DynamicMemoryEnabled $false\r\n\
                 Set-VM -VM $vm -AutomaticCheckpointsEnabled $false -CheckpointType Standard -AutomaticStopAction ShutDown -AutomaticStartAction Nothing\r\n\
                 $swName = ''\r\n\
                 if ($null -ne $sw) {{ $swName = $sw.Name }}\r\n\
                 [pscustomobject]@{{ id = $vm.VMId.ToString(); switch = $swName }} | ConvertTo-Json -Compress",
                name = quote(&name),
                mem = spec.memory_mb * 1024 * 1024,
                vhd = quote(&disk.to_string_lossy()),
                path = quote(&vm_root.to_string_lossy()),
                cpus = spec.cpus,
            ),
        )
        .inspect_err(|_| {
            let _ = fs::remove_file(&disk);
        })?;
        if created.switch.is_empty() {
            logger::info("Kein virtueller Switch gefunden – VM ohne Netzwerk angelegt");
        }
        progress("vm", "Virtuelle Maschine anlegen", "done");

        let record = VmRecord {
            id: id.to_string(),
            name: name.clone(),
            backend: BackendKind::Hyperv,
            os_family: spec.os_family,
            os_id: spec.os_id.clone(),
            iso_path: spec.iso_path.clone(),
            cpus: spec.cpus,
            memory_mb: spec.memory_mb,
            disk_gb: spec.disk_gb,
            dir: dir.to_string_lossy().to_string(),
            disk_path: disk.to_string_lossy().to_string(),
            created: chrono::Utc::now(),
            hyperv_id: Some(created.id),
            qmp_port: None,
            snapshots: vec![],
        };

        // Ab hier gibt es die VM – bei Fehlern wieder aufräumen.
        let rollback = |e: AppError| {
            let _ = self.delete(&record, true);
            e
        };

        // 4. Sicherheit (Secure Boot, bei Windows zusätzlich TPM)
        progress("security", "Sicherheit einrichten", "active");
        let security = match spec.os_family {
            OsFamily::Windows => "Set-VMFirmware -VM $vm -EnableSecureBoot On -SecureBootTemplate 'MicrosoftWindows'\r\n\
                 Set-VMKeyProtector -VM $vm -NewLocalKeyProtector\r\n\
                 Enable-VMTPM -VM $vm"
                .to_string(),
            OsFamily::Linux => {
                "Set-VMFirmware -VM $vm -EnableSecureBoot On -SecureBootTemplate 'MicrosoftUEFICertificateAuthority'".to_string()
            }
        };
        match run_vm("Sicherheit einrichten", &record, &security) {
            Ok(_) => {}
            // Der lokale Schlüsselschutz für das TPM braucht auf manchen Systemen Adminrechte.
            Err(e) if ps::is_access_denied(&e) => {
                logger::info("TPM/Secure Boot: zweiter Versuch mit Adminrechten");
                elevate::run_ps("Sicherheit einrichten", &with_vm(&record, &security)).map_err(rollback)?;
            }
            Err(e) => return Err(rollback(e)),
        }
        progress("security", "Sicherheit einrichten", "done");

        // 5. Installationsmedium einlegen und als erstes Startgerät setzen
        if let Some(iso) = &spec.iso_path {
            progress("iso", "Installationsmedium einlegen", "active");
            run_vm(
                "Installationsmedium einlegen",
                &record,
                &format!(
                    "$dvd = Add-VMDvdDrive -VM $vm -Path {} -Passthru\r\n\
                     Set-VMFirmware -VM $vm -FirstBootDevice $dvd",
                    quote(iso)
                ),
            )
            .map_err(|e| {
                let mut e = e;
                if ps::is_access_denied(&e) {
                    e.hint = "Hyper-V darf die ISO-Datei nicht lesen. Kopiere sie in einen normalen lokalen Ordner (z. B. „Downloads“) – nicht auf ein Netzlaufwerk oder in einen Cloud-Ordner – und versuche es erneut.".into();
                }
                rollback(e)
            })?;
            progress("iso", "Installationsmedium einlegen", "done");
        }

        Ok(record)
    }

    fn start(&self, vm: &VmRecord) -> AppResult<()> {
        run_vm(
            "VM starten",
            vm,
            "if ($vm.State -eq 'Paused') { Resume-VM -VM $vm } elseif ($vm.State -ne 'Running') { Start-VM -VM $vm }",
        )?;
        Ok(())
    }

    fn pause(&self, vm: &VmRecord) -> AppResult<()> {
        run_vm("VM pausieren", vm, "Suspend-VM -VM $vm")?;
        Ok(())
    }

    fn resume(&self, vm: &VmRecord) -> AppResult<()> {
        run_vm(
            "VM fortsetzen",
            vm,
            "if ($vm.State -eq 'Saved') { Start-VM -VM $vm } else { Resume-VM -VM $vm }",
        )?;
        Ok(())
    }

    fn shutdown(&self, vm: &VmRecord) -> AppResult<()> {
        run_vm(
            "VM herunterfahren",
            vm,
            "if ($vm.State -eq 'Paused') { Resume-VM -VM $vm }\r\n\
             if ($vm.State -eq 'Saved') { Remove-VMSavedState -VM $vm; return }\r\n\
             if ($vm.State -ne 'Off') { Stop-VM -VM $vm -Force }",
        )
        .map_err(|mut e| {
            if !e.title.contains("nicht gefunden") {
                e.hint = "Das Betriebssystem in der VM hat nicht auf das Herunterfahren reagiert (z. B. weil es noch installiert wird). Nutze im Menü „…“ → „Sofort ausschalten“.".into();
            }
            e
        })?;
        Ok(())
    }

    fn power_off(&self, vm: &VmRecord) -> AppResult<()> {
        run_vm(
            "VM ausschalten",
            vm,
            "if ($vm.State -eq 'Saved') { Remove-VMSavedState -VM $vm } elseif ($vm.State -ne 'Off') { Stop-VM -VM $vm -TurnOff -Force }",
        )?;
        Ok(())
    }

    fn open_console(&self, vm: &VmRecord) -> AppResult<()> {
        // Aktuellen Namen aus Hyper-V holen, falls er außerhalb geändert wurde.
        let name = run_vm("VM-Fenster öffnen", vm, "$vm.Name").unwrap_or_else(|_| vm.name.clone());
        Self::vmconnect(name.trim())
    }

    fn status(&self, vms: &[VmRecord]) -> AppResult<Vec<VmStatus>> {
        let ids: Vec<String> = vms.iter().filter_map(|v| v.hyperv_id.clone()).map(|i| quote(&i)).collect();
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let script = format!(
            "$ids = @({})\r\n\
             $r = @(Get-VM | Where-Object {{ $ids -contains $_.VMId.ToString() }} | ForEach-Object {{\r\n\
               [pscustomobject]@{{ id = $_.VMId.ToString(); state = $_.State.ToString(); cpu = [double]$_.CPUUsage; mem = [uint64]($_.MemoryAssigned / 1MB); up = [uint64]$_.Uptime.TotalSeconds }}\r\n\
             }})\r\n\
             ConvertTo-Json -InputObject $r -Compress",
            ids.join(",")
        );
        let out = ps::run_quiet(&script)?;
        let raw: Vec<RawStatus> = serde_json::from_str(if out.is_empty() { "[]" } else { &out })?;
        Ok(vms
            .iter()
            .map(|vm| {
                let hid = vm.hyperv_id.clone().unwrap_or_default();
                match raw.iter().find(|r| r.id.eq_ignore_ascii_case(&hid)) {
                    Some(r) => VmStatus {
                        id: vm.id.clone(),
                        state: match r.state.as_str() {
                            "Running" | "RunningCritical" => PowerState::Running,
                            "Paused" | "PausedCritical" | "Pausing" | "Saved" | "SavedCritical" => PowerState::Paused,
                            "Off" | "OffCritical" => PowerState::Off,
                            "Starting" | "StartingCritical" | "Resuming" | "Reset" => PowerState::Starting,
                            "Stopping" | "StoppingCritical" => PowerState::Stopping,
                            "Saving" | "SavingCritical" => PowerState::Saving,
                            _ => PowerState::Unknown,
                        },
                        cpu_percent: r.cpu,
                        memory_used_mb: r.mem,
                        uptime_seconds: r.up,
                    },
                    None => VmStatus {
                        id: vm.id.clone(),
                        state: PowerState::Missing,
                        cpu_percent: 0.0,
                        memory_used_mb: 0,
                        uptime_seconds: 0,
                    },
                }
            })
            .collect())
    }

    fn rename(&self, vm: &VmRecord, new_name: &str) -> AppResult<()> {
        if Self::name_exists(new_name)? {
            return Err(AppError::new(
                format!("Es gibt schon eine VM namens „{new_name}“"),
                "Wähle einen anderen Namen.",
            ));
        }
        run_vm("VM umbenennen", vm, &format!("Rename-VM -VM $vm -NewName {}", quote(new_name)))?;
        Ok(())
    }

    fn update(&self, vm: &VmRecord, changes: &VmChanges) -> AppResult<()> {
        let mut body = String::from(
            "if ($vm.State -ne 'Off') { throw 'NESTBOX_NICHT_AUS' }\r\n",
        );
        body.push_str(&format!("Set-VMProcessor -VM $vm -Count {}\r\n", changes.cpus));
        body.push_str(&format!("Set-VMMemory -VM $vm -StartupBytes {}\r\n", changes.memory_mb * 1024 * 1024));
        if changes.disk_gb > vm.disk_gb {
            body.push_str(&format!(
                "$d = Get-VMHardDiskDrive -VM $vm | Select-Object -First 1\r\n\
                 if ($null -ne $d) {{ Resize-VHD -Path $d.Path -SizeBytes {} }}\r\n",
                changes.disk_gb * 1024 * 1024 * 1024
            ));
        }
        run_vm("Einstellungen speichern", vm, &body).map_err(|e| {
            if e.details.as_deref().unwrap_or("").contains("NESTBOX_NICHT_AUS") {
                AppError::new(
                    "Die VM muss dafür ausgeschaltet sein",
                    "Fahre die VM herunter und ändere die Einstellungen dann erneut.",
                )
            } else {
                e
            }
        })?;
        Ok(())
    }

    fn eject_iso(&self, vm: &VmRecord) -> AppResult<()> {
        run_vm(
            "Installationsmedium auswerfen",
            vm,
            "Get-VMDvdDrive -VM $vm | Set-VMDvdDrive -Path $null\r\n\
             $hdd = Get-VMHardDiskDrive -VM $vm | Select-Object -First 1\r\n\
             if ($null -ne $hdd) { Set-VMFirmware -VM $vm -FirstBootDevice $hdd }",
        )?;
        Ok(())
    }

    fn delete(&self, vm: &VmRecord, delete_disk: bool) -> AppResult<()> {
        let id = vm.hyperv_id.clone().unwrap_or_default();
        if !id.is_empty() {
            ps::run(
                "VM löschen",
                &format!(
                    "$vm = Get-VM -Id {} -ErrorAction SilentlyContinue\r\n\
                     if ($null -ne $vm) {{\r\n\
                       if ($vm.State -ne 'Off') {{ Stop-VM -VM $vm -TurnOff -Force }}\r\n\
                       Remove-VM -VM $vm -Force\r\n\
                     }}",
                    quote(&id)
                ),
            )?;
        }
        if delete_disk {
            remove_vm_files(vm, &[".vhdx", ".avhdx"])?;
        }
        Ok(())
    }

    fn list_snapshots(&self, vm: &VmRecord) -> AppResult<Vec<Snapshot>> {
        let out = run_vm(
            "Sicherungspunkte laden",
            vm,
            "$r = @(Get-VMSnapshot -VM $vm | Sort-Object CreationTime -Descending | ForEach-Object {\r\n\
               [pscustomobject]@{ id = $_.Id.ToString(); name = $_.Name; created = $_.CreationTime.ToUniversalTime().ToString('o') }\r\n\
             })\r\n\
             ConvertTo-Json -InputObject $r -Compress",
        )?;
        let raw: Vec<RawSnapshot> = serde_json::from_str(if out.is_empty() { "[]" } else { &out })?;
        Ok(raw
            .into_iter()
            .map(|r| Snapshot { id: r.id, name: r.name, created: r.created, with_state: true })
            .collect())
    }

    fn create_snapshot(&self, vm: &VmRecord, name: &str) -> AppResult<Snapshot> {
        let out = run_vm(
            "Sicherungspunkt erstellen",
            vm,
            &format!(
                "$s = Checkpoint-VM -VM $vm -SnapshotName {} -Passthru\r\n\
                 [pscustomobject]@{{ id = $s.Id.ToString(); name = $s.Name; created = $s.CreationTime.ToUniversalTime().ToString('o') }} | ConvertTo-Json -Compress",
                quote(name)
            ),
        )?;
        let r: RawSnapshot = serde_json::from_str(&out)?;
        Ok(Snapshot { id: r.id, name: r.name, created: r.created, with_state: true })
    }

    fn restore_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()> {
        run_vm(
            "Sicherungspunkt wiederherstellen",
            vm,
            &format!(
                "$s = Get-VMSnapshot -VM $vm | Where-Object {{ $_.Id.ToString() -eq {id} }}\r\n\
                 if ($null -eq $s) {{ throw 'Der Sicherungspunkt existiert nicht mehr.' }}\r\n\
                 if ($vm.State -ne 'Off') {{ Stop-VM -VM $vm -TurnOff -Force }}\r\n\
                 Restore-VMSnapshot -VMSnapshot $s -Confirm:$false",
                id = quote(snapshot_id)
            ),
        )?;
        Ok(())
    }

    fn delete_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()> {
        run_vm(
            "Sicherungspunkt löschen",
            vm,
            &format!(
                "Get-VMSnapshot -VM $vm | Where-Object {{ $_.Id.ToString() -eq {} }} | Remove-VMSnapshot -Confirm:$false",
                quote(snapshot_id)
            ),
        )?;
        Ok(())
    }
}

/// Löscht die Dateien einer VM. Der ganze Ordner wird nur entfernt, wenn er
/// eindeutig zu dieser VM gehört (von Nestbox angelegt, enthält die Festplatte).
pub fn remove_vm_files(vm: &VmRecord, disk_exts: &[&str]) -> AppResult<()> {
    let dir = PathBuf::from(&vm.dir);
    let disk = PathBuf::from(&vm.disk_path);
    // Nestbox legt jede VM in einem eigenen Unterordner an, der auch die Festplatte enthält.
    let owns_dir = !vm.dir.is_empty() && disk.starts_with(&dir) && dir.components().count() >= 3;

    // Hyper-V/QEMU geben Dateien manchmal erst nach einem Moment frei.
    let mut last_err = None;
    for _ in 0..10 {
        let res = if owns_dir && dir.exists() {
            fs::remove_dir_all(&dir)
        } else {
            let mut r = Ok(());
            if let Some(parent) = disk.parent() {
                if let Ok(entries) = fs::read_dir(parent) {
                    for e in entries.flatten() {
                        let p = e.path();
                        let n = p.to_string_lossy().to_lowercase();
                        if disk_exts.iter().any(|x| n.ends_with(x)) && p.file_stem() == disk.file_stem()
                            || p == disk
                        {
                            if let Err(err) = fs::remove_file(&p) {
                                r = Err(err);
                            }
                        }
                    }
                }
            }
            r
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
