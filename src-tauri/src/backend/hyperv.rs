//! Hyper-V-Backend (optional, nur Windows Pro/Enterprise/Education) – alles über PowerShell.
//! Damit lassen sich auch VMs steuern, die im Hyper-V-Manager angelegt wurden.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{remove_vm_files, safe_file_name, CreateSpec, PowerState, Progress, Snapshot, VmBackend, VmChanges, VmStatus};
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
    ps::run(what, &with_vm(vm, body)).map_err(|e| map_error(e, vm))
}

/// Hyper-V-Meldungen in verständliche Hinweise übersetzen.
fn map_error(mut e: AppError, vm: &VmRecord) -> AppError {
    let d = e.details.as_deref().unwrap_or("").to_lowercase();
    if d.contains(&MISSING.to_lowercase()) {
        return AppError::new(
            format!("„{}“ wurde in Hyper-V nicht gefunden", vm.name),
            "Die VM wurde vermutlich im Hyper-V-Manager gelöscht. Entferne sie über „…“ → „Löschen“ aus der Liste.",
        );
    }
    if d.contains("autorisierungsrichtlinie") || d.contains("authorization policy") || d.contains("zugriff verweigert") || d.contains("access is denied") {
        e.hint = "Dir fehlen die Rechte für Hyper-V. Öffne „Einstellungen → Einrichtung erneut prüfen“ und füge dich der Gruppe „Hyper-V-Administratoren“ hinzu – danach einmal ab- und wieder anmelden.".into();
    } else if d.contains("hyper-v-komponenten nicht ausgeführt") || d.contains("hyper-v components is not running") || d.contains("hypervisor is not running") {
        e.hint = "Der Hypervisor von Windows läuft nicht. Prüfe unter „Einrichtung“, ob die Virtualisierung aktiv ist, und starte den PC neu.".into();
    }
    e
}

fn is_access_denied(err: &AppError) -> bool {
    let d = err.details.as_deref().unwrap_or("").to_lowercase();
    d.contains("access is denied") || d.contains("zugriff verweigert") || d.contains("autorisierungsrichtlinie") || d.contains("authorization policy") || d.contains("0x80070005")
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

/// Eine VM aus dem Hyper-V-Manager, die Nestbox noch nicht kennt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperVCandidate {
    pub id: String,
    pub name: String,
    pub state: String,
    pub cpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub disk_path: String,
    pub path: String,
    pub windows: bool,
    pub created: String,
}

impl HyperVBackend {
    /// Gibt es in Hyper-V schon eine VM mit diesem Namen?
    pub fn name_exists(name: &str) -> AppResult<bool> {
        let out = ps::run("Namen prüfen", &format!("@(Get-VM -Name {} -ErrorAction SilentlyContinue).Count", quote(name)))?;
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

    /// Alle Hyper-V-VMs, die (noch) nicht in Nestbox sind.
    pub fn list_unmanaged(known_ids: &[String]) -> AppResult<Vec<HyperVCandidate>> {
        let known: Vec<String> = known_ids.iter().map(|i| quote(i)).collect();
        let script = format!(
            "$known = @({known})\r\n\
             $r = @(Get-VM | Where-Object {{ $known -notcontains $_.VMId.ToString() }} | ForEach-Object {{\r\n\
               $vm = $_\r\n\
               $hdd = Get-VMHardDiskDrive -VM $vm | Select-Object -First 1\r\n\
               $size = 0\r\n\
               if ($null -ne $hdd -and $hdd.Path) {{ try {{ $size = [uint64]((Get-VHD -Path $hdd.Path -ErrorAction Stop).Size / 1GB) }} catch {{}} }}\r\n\
               $fw = $null\r\n\
               if ($vm.Generation -eq 2) {{ try {{ $fw = Get-VMFirmware -VM $vm -ErrorAction Stop }} catch {{}} }}\r\n\
               $win = ($null -ne $fw -and [string]$fw.SecureBootTemplate -eq 'MicrosoftWindows') -or ([string]$vm.Name -match 'win')\r\n\
               [pscustomobject]@{{\r\n\
                 id = $vm.VMId.ToString(); name = $vm.Name; state = $vm.State.ToString()\r\n\
                 cpus = [uint32]$vm.ProcessorCount; memoryMb = [uint64]($vm.MemoryStartup / 1MB); diskGb = $size\r\n\
                 diskPath = [string]$hdd.Path; path = [string]$vm.Path; windows = [bool]$win\r\n\
                 created = $vm.CreationTime.ToUniversalTime().ToString('o')\r\n\
               }}\r\n\
             }})\r\n\
             ConvertTo-Json -InputObject $r -Compress -Depth 3",
            known = known.join(",")
        );
        let out = ps::run("Hyper-V-VMs suchen", &script)?;
        Ok(serde_json::from_str(if out.is_empty() { "[]" } else { &out })?)
    }

    /// Macht aus einer Hyper-V-VM einen Nestbox-Eintrag (die VM selbst bleibt unverändert).
    pub fn to_record(c: &HyperVCandidate) -> VmRecord {
        let disk = PathBuf::from(&c.disk_path);
        let dir = disk.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| c.path.clone());
        VmRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name: c.name.clone(),
            backend: BackendKind::Hyperv,
            os_family: if c.windows { OsFamily::Windows } else { OsFamily::Linux },
            os_id: "custom".into(),
            iso_path: None,
            cpus: c.cpus.max(1),
            memory_mb: c.memory_mb.max(512),
            disk_gb: c.disk_gb,
            dir,
            disk_path: c.disk_path.clone(),
            created: chrono::DateTime::parse_from_rfc3339(&c.created).map(|d| d.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now()),
            vbox_id: None,
            hyperv_id: Some(c.id.clone()),
            imported: true,
            qmp_port: None,
            snapshots: vec![],
        }
    }
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
            return Err(AppError::new("Für diesen Namen existiert schon eine virtuelle Festplatte", "Wähle einen anderen Namen.")
                .with_details(disk.display().to_string()));
        }
        fs::create_dir_all(&disk_dir)?;
        progress("folder", "Ordner vorbereiten", "done");

        // 2. Festplatte (dynamisch: belegt nur so viel Platz, wie wirklich genutzt wird)
        progress("disk", "Virtuelle Festplatte anlegen", "active");
        ps::run(
            "Virtuelle Festplatte anlegen",
            &format!("New-VHD -Path {} -SizeBytes {} -Dynamic | Out-Null", quote(&disk.to_string_lossy()), spec.disk_gb * 1024 * 1024 * 1024),
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
            vbox_id: None,
            hyperv_id: Some(created.id),
            imported: false,
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
            OsFamily::Linux => "Set-VMFirmware -VM $vm -EnableSecureBoot On -SecureBootTemplate 'MicrosoftUEFICertificateAuthority'".to_string(),
        };
        match run_vm("Sicherheit einrichten", &record, &security) {
            Ok(_) => {}
            // Der lokale Schlüsselschutz für das TPM braucht auf manchen Systemen Adminrechte.
            Err(e) if is_access_denied(&e) => {
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
                &format!("$dvd = Add-VMDvdDrive -VM $vm -Path {} -Passthru\r\nSet-VMFirmware -VM $vm -FirstBootDevice $dvd", quote(iso)),
            )
            .map_err(|mut e| {
                if is_access_denied(&e) {
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
        // Hyper-V öffnet kein Fenster von selbst – das VM-Fenster gleich mit anzeigen.
        let _ = self.open_console(vm);
        Ok(())
    }

    fn pause(&self, vm: &VmRecord) -> AppResult<()> {
        run_vm("VM pausieren", vm, "Suspend-VM -VM $vm")?;
        Ok(())
    }

    fn resume(&self, vm: &VmRecord) -> AppResult<()> {
        run_vm("VM fortsetzen", vm, "if ($vm.State -eq 'Saved') { Start-VM -VM $vm } else { Resume-VM -VM $vm }")?;
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
        // Aktuellen Namen aus Hyper-V holen, falls er im Hyper-V-Manager geändert wurde.
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
                    None => VmStatus { id: vm.id.clone(), state: PowerState::Missing, cpu_percent: 0.0, memory_used_mb: 0, uptime_seconds: 0 },
                }
            })
            .collect())
    }

    fn rename(&self, vm: &VmRecord, new_name: &str) -> AppResult<()> {
        if Self::name_exists(new_name)? {
            return Err(AppError::new(format!("Es gibt schon eine VM namens „{new_name}“"), "Wähle einen anderen Namen."));
        }
        run_vm("VM umbenennen", vm, &format!("Rename-VM -VM $vm -NewName {}", quote(new_name)))?;
        Ok(())
    }

    fn update(&self, vm: &VmRecord, changes: &VmChanges) -> AppResult<()> {
        let mut body = String::from("if ($vm.State -ne 'Off') { throw 'NESTBOX_NICHT_AUS' }\r\n");
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
                AppError::new("Die VM muss dafür ausgeschaltet sein", "Fahre die VM herunter und ändere die Einstellungen dann erneut.")
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
             if ($null -ne $hdd -and $vm.Generation -eq 2) { Set-VMFirmware -VM $vm -FirstBootDevice $hdd }",
        )?;
        Ok(())
    }

    fn install_guest_tools(&self, _vm: &VmRecord) -> AppResult<()> {
        Err(AppError::new(
            "Bei Hyper-V sind keine Gasterweiterungen nötig",
            "Windows und aktuelle Linux-Systeme bringen die Hyper-V-Treiber schon mit. Für das flüssigste Bild (z. B. für Videos) empfehlen wir VirtualBox mit Gasterweiterungen.",
        ))
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
            remove_vm_files(vm)?;
        }
        Ok(())
    }

    fn native_snapshots(&self, vm: &VmRecord) -> AppResult<Option<Vec<Snapshot>>> {
        let out = run_vm(
            "Sicherungspunkte laden",
            vm,
            "$r = @(Get-VMSnapshot -VM $vm | Sort-Object CreationTime -Descending | ForEach-Object {\r\n\
               [pscustomobject]@{ id = $_.Id.ToString(); name = $_.Name; created = $_.CreationTime.ToUniversalTime().ToString('o') }\r\n\
             })\r\n\
             ConvertTo-Json -InputObject $r -Compress",
        )?;
        let raw: Vec<RawSnapshot> = serde_json::from_str(if out.is_empty() { "[]" } else { &out })?;
        Ok(Some(raw.into_iter().map(|r| Snapshot { id: r.id, name: r.name, created: r.created, with_state: true }).collect()))
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
            &format!("Get-VMSnapshot -VM $vm | Where-Object {{ $_.Id.ToString() -eq {} }} | Remove-VMSnapshot -Confirm:$false", quote(snapshot_id)),
        )?;
        Ok(())
    }
}
