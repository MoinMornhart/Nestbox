//! VirtualBox-Backend (kostenlos, läuft auf Windows Home und Pro).
//! Gesteuert über VBoxManage. Unterstützt TPM 2.0 und Secure Boot – damit auch Windows 11.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{bring_to_front, cpu_percent, process_info, remove_vm_files, run_tool, safe_file_name};
use super::{CreateSpec, PowerState, Progress, Snapshot, VmBackend, VmChanges, VmStatus};
use crate::error::{AppError, AppResult};
use crate::logger;
use crate::store::{BackendKind, OsFamily, Settings, VmRecord};

pub struct VBoxBackend {
    exe: PathBuf,
}

const VM_PROCESS: &str = "VirtualBoxVM.exe";
const CONTROLLER: &str = "SATA";

/// Sucht VBoxManage.exe (Installationspfad aus der Umgebung, Standardpfad, PATH).
pub fn find_vboxmanage() -> Option<PathBuf> {
    let exe = "VBoxManage.exe";
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("VBOX_MSI_INSTALL_PATH") {
        candidates.push(PathBuf::from(p).join(exe));
    }
    for var in ["ProgramFiles", "ProgramW6432"] {
        if let Some(pf) = std::env::var_os(var) {
            candidates.push(PathBuf::from(pf).join("Oracle").join("VirtualBox").join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|p| p.join(exe)));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Version, z. B. „7.1.4r165100“
pub fn version(exe: &Path) -> Option<String> {
    crate::ps::hidden_command(&exe.to_string_lossy())
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn not_installed() -> AppError {
    AppError::new(
        "VirtualBox ist nicht installiert",
        "Installiere VirtualBox unter „Einrichtung“ (ein Klick) oder von virtualbox.org und starte Nestbox neu.",
    )
}

fn ostype(spec: &CreateSpec) -> &'static str {
    match (spec.os_id.as_str(), spec.os_family) {
        ("ubuntu", _) | ("mint", _) => "Ubuntu_64",
        ("fedora", _) => "Fedora_64",
        ("windows11", _) => "Windows11_64",
        (_, OsFamily::Windows) => "Windows10_64",
        _ => "Linux_64",
    }
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Liest „Schlüssel=Wert“-Zeilen aus --machinereadable.
fn machine_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim_matches('"') == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// „UUID: xxxxxxxx-…“ aus der Ausgabe von createvm / snapshot take
fn parse_uuid(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let idx = l.find("UUID:")?;
        let id = l[idx + 5..].trim().trim_matches(|c: char| c == '.' || c == '\'' || c == '"');
        (id.len() >= 36).then(|| id[..36].to_string())
    })
}

impl VBoxBackend {
    pub fn from_settings(_settings: &Settings) -> AppResult<Self> {
        Ok(Self { exe: find_vboxmanage().ok_or_else(not_installed)? })
    }

    fn run(&self, what: &str, a: Vec<String>) -> AppResult<String> {
        run_tool(&self.exe, &a, what, "Versuche es noch einmal. Die technischen Details und die Log-Datei helfen bei der Fehlersuche.")
            .map_err(map_error)
    }

    fn id(vm: &VmRecord) -> String {
        vm.vbox_id.clone().unwrap_or_else(|| vm.name.clone())
    }

    /// running | paused | saved | poweroff | aborted | … (None = VM existiert nicht mehr)
    fn state(&self, vm: &VmRecord) -> Option<String> {
        let out = crate::ps::hidden_command(&self.exe.to_string_lossy())
            .args(["showvminfo", &Self::id(vm), "--machinereadable"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        machine_value(&String::from_utf8_lossy(&out.stdout), "VMState")
    }

    fn require_state(&self, vm: &VmRecord) -> AppResult<String> {
        self.state(vm).ok_or_else(|| {
            AppError::new(
                format!("„{}“ wurde in VirtualBox nicht gefunden", vm.name),
                "Die VM wurde vermutlich außerhalb von Nestbox gelöscht. Entferne sie über „…“ → „Löschen“ aus der Liste.",
            )
        })
    }

    fn is_active(state: &str) -> bool {
        matches!(state, "running" | "paused" | "starting" | "stopping" | "saving" | "restoring" | "stuck")
    }

    fn require_off(&self, vm: &VmRecord) -> AppResult<()> {
        let st = self.require_state(vm)?;
        if Self::is_active(&st) {
            return Err(AppError::new(
                "Die VM muss dafür ausgeschaltet sein",
                "Fahre die VM herunter und versuche es dann erneut.",
            ));
        }
        Ok(())
    }

    fn wait_for(&self, vm: &VmRecord, timeout: Duration, done: impl Fn(&str) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.state(vm) {
                Some(s) if done(&s) => return true,
                None => return true,
                _ => std::thread::sleep(Duration::from_millis(500)),
            }
        }
        false
    }

    fn start_gui(&self, vm: &VmRecord) -> AppResult<()> {
        self.run("VM starten", args(&["startvm", &Self::id(vm), "--type", "gui"]))?;
        Ok(())
    }
}

/// Typische VirtualBox-Meldungen in verständliche Hinweise übersetzen.
fn map_error(mut e: AppError) -> AppError {
    let d = e.details.as_deref().unwrap_or("").to_lowercase();
    e.hint = if d.contains("vt-x is not available") || d.contains("amd-v is not available") || d.contains("verr_vmx_no_vmx") || d.contains("verr_svm_no_svm") || d.contains("no hardware virtualization") {
        "VirtualBox braucht die Hardware-Virtualisierung des Prozessors. Schalte sie im BIOS/UEFI ein (bzw. – wenn dieses Windows selbst eine VM ist – im Host). Ohne sie funktioniert nur QEMU, und zwar langsam."
    } else if d.contains("not enough memory") || d.contains("verr_no_memory") || d.contains("nicht genügend") {
        "Es ist gerade nicht genug freier Arbeitsspeicher da. Schließe andere Programme oder VMs, oder gib der VM weniger Arbeitsspeicher."
    } else if d.contains("is already locked") || d.contains("session") && d.contains("locked") {
        "Die VM ist gerade in Benutzung. Warte einen Moment oder schließe das VirtualBox-Fenster und versuche es erneut."
    } else if d.contains("already exists") {
        "Es gibt bereits eine VM oder Datei mit diesem Namen. Wähle einen anderen Namen."
    } else if d.contains("could not find a registered machine") || d.contains("verr_file_not_found") {
        "Die VM oder eine ihrer Dateien wurde nicht gefunden. Wurde sie außerhalb von Nestbox gelöscht oder verschoben?"
    } else if d.contains("the machine is not mutable") || d.contains("must be powered off") || d.contains("invalid machine state") {
        "Die VM muss dafür ausgeschaltet sein. Fahre sie herunter und versuche es erneut."
    } else {
        return e;
    }
    .into();
    e
}

impl VmBackend for VBoxBackend {
    fn create(&self, spec: &CreateSpec, settings: &Settings, id: &str, progress: Progress) -> AppResult<VmRecord> {
        let name = spec.name.trim().to_string();
        let base = PathBuf::from(&settings.vm_dir);
        let dir = base.join(safe_file_name(&name));
        let disk = dir.join(format!("{}.vdi", safe_file_name(&name)));
        let windows = spec.os_family == OsFamily::Windows;

        // 1. VM registrieren (legt den Ordner an)
        progress("folder", "Ordner vorbereiten", "active");
        if dir.join(format!("{}.vbox", safe_file_name(&name))).exists() || disk.exists() {
            return Err(AppError::new(
                "Für diesen Namen gibt es schon VM-Dateien",
                "Gehe einen Schritt zurück und wähle einen anderen Namen.",
            )
            .with_details(dir.display().to_string()));
        }
        let out = self.run(
            "Virtuelle Maschine anlegen",
            args(&["createvm", "--name", &name, "--ostype", ostype(spec), "--basefolder", &base.to_string_lossy(), "--register"]),
        )?;
        let uuid = parse_uuid(&out).ok_or_else(|| {
            AppError::new("Virtuelle Maschine anlegen", "VirtualBox hat keine ID zurückgegeben. Versuche es erneut.").with_details(out.clone())
        })?;
        progress("folder", "Ordner vorbereiten", "done");

        let record = VmRecord {
            id: id.to_string(),
            name: name.clone(),
            backend: BackendKind::Virtualbox,
            os_family: spec.os_family,
            os_id: spec.os_id.clone(),
            iso_path: spec.iso_path.clone(),
            cpus: spec.cpus,
            memory_mb: spec.memory_mb,
            disk_gb: spec.disk_gb,
            dir: dir.to_string_lossy().to_string(),
            disk_path: disk.to_string_lossy().to_string(),
            created: chrono::Utc::now(),
            vbox_id: Some(uuid.clone()),
            qmp_port: None,
            snapshots: vec![],
        };
        // Ab hier gibt es die VM – bei Fehlern wieder vollständig aufräumen.
        let rollback = |e: AppError| {
            let _ = self.delete(&record, true);
            e
        };

        // 2. Festplatte (dynamisch: belegt nur so viel Platz, wie wirklich genutzt wird)
        progress("disk", "Virtuelle Festplatte anlegen", "active");
        self.run(
            "Virtuelle Festplatte anlegen",
            args(&["createmedium", "disk", "--filename", &disk.to_string_lossy(), "--size", &(spec.disk_gb * 1024).to_string(), "--format", "VDI", "--variant", "Standard"]),
        )
        .map_err(rollback)?;
        progress("disk", "Virtuelle Festplatte anlegen", "done");

        // 3. Hardware einstellen und Festplatte anschließen
        progress("vm", "Virtuelle Maschine einrichten", "active");
        let mem = spec.memory_mb.to_string();
        let cpus = spec.cpus.to_string();
        self.run(
            "Virtuelle Maschine einrichten",
            args(&[
                "modifyvm", &uuid,
                "--memory", &mem,
                "--cpus", &cpus,
                "--firmware", "efi",
                "--ioapic", "on",
                "--graphicscontroller", if windows { "vboxsvga" } else { "vmsvga" },
                // Für flüssiges Video: 3D-Beschleunigung und maximaler Grafikspeicher
                "--vram", "256",
                "--accelerate-3d", "on",
                "--nested-paging", "on",
                "--large-pages", "on",
                // Ton über das Standard-Audiogerät von Windows
                "--audio-driver", "default",
                "--audio-controller", "hda",
                "--audio-enabled", "on",
                "--audio-out", "on",
                "--nic1", "nat",
                "--usbohci", "on",
                "--mouse", "usbtablet",
                "--rtc-use-utc", if windows { "off" } else { "on" },
                "--clipboard-mode", "bidirectional",
                "--boot1", "dvd",
                "--boot2", "disk",
                "--boot3", "none",
                "--boot4", "none",
            ]),
        )
        .map_err(rollback)?;
        self.run(
            "Festplatte anschließen",
            args(&["storagectl", &uuid, "--name", CONTROLLER, "--add", "sata", "--controller", "IntelAhci", "--portcount", "2", "--bootable", "on"]),
        )
        .map_err(rollback)?;
        self.run(
            "Festplatte anschließen",
            args(&["storageattach", &uuid, "--storagectl", CONTROLLER, "--port", "0", "--device", "0", "--type", "hdd", "--medium", &disk.to_string_lossy()]),
        )
        .map_err(rollback)?;
        progress("vm", "Virtuelle Maschine einrichten", "done");

        // 4. Windows: TPM 2.0 und Secure Boot (Voraussetzung für Windows 11)
        if windows {
            progress("security", "TPM & Secure Boot einrichten", "active");
            self.run("TPM einrichten", args(&["modifyvm", &uuid, "--tpm-type", "2.0"])).map_err(rollback)?;
            // Secure Boot: Schlüssel von Microsoft/Oracle eintragen. Ältere VirtualBox-Versionen
            // kennen einzelne Befehle nicht – dann läuft Windows trotzdem (Secure Boot nur „fähig“).
            for step in [vec!["inituefivarstore"], vec!["enrollmssignatures"], vec!["enrollorclpk"], vec!["secureboot", "--enable"]] {
                let mut a = vec!["modifynvram".to_string(), uuid.clone()];
                a.extend(step.iter().map(|s| s.to_string()));
                if let Err(e) = self.run("Secure Boot einrichten", a) {
                    logger::info(&format!("Secure Boot übersprungen: {}", e.details.unwrap_or_default()));
                    break;
                }
            }
            progress("security", "TPM & Secure Boot einrichten", "done");
        }

        // 5. Installationsmedium
        if let Some(iso) = &spec.iso_path {
            progress("iso", "Installationsmedium einlegen", "active");
            self.run(
                "Installationsmedium einlegen",
                args(&["storageattach", &uuid, "--storagectl", CONTROLLER, "--port", "1", "--device", "0", "--type", "dvddrive", "--medium", iso]),
            )
            .map_err(rollback)?;
            progress("iso", "Installationsmedium einlegen", "done");
        }

        Ok(record)
    }

    fn start(&self, vm: &VmRecord) -> AppResult<()> {
        match self.require_state(vm)?.as_str() {
            "paused" => self.resume(vm),
            "running" | "starting" => Ok(()),
            _ => self.start_gui(vm),
        }
    }

    fn pause(&self, vm: &VmRecord) -> AppResult<()> {
        self.run("VM pausieren", args(&["controlvm", &Self::id(vm), "pause"]))?;
        Ok(())
    }

    fn resume(&self, vm: &VmRecord) -> AppResult<()> {
        if self.require_state(vm)? == "saved" {
            return self.start_gui(vm);
        }
        self.run("VM fortsetzen", args(&["controlvm", &Self::id(vm), "resume"]))?;
        Ok(())
    }

    fn shutdown(&self, vm: &VmRecord) -> AppResult<()> {
        let st = self.require_state(vm)?;
        if st == "saved" {
            self.run("VM herunterfahren", args(&["discardstate", &Self::id(vm)]))?;
            return Ok(());
        }
        if !Self::is_active(&st) {
            return Ok(());
        }
        if st == "paused" {
            self.resume(vm)?;
        }
        self.run("VM herunterfahren", args(&["controlvm", &Self::id(vm), "acpipowerbutton"]))?;
        if !self.wait_for(vm, Duration::from_secs(90), |s| !Self::is_active(s)) {
            return Err(AppError::new(
                "Die VM reagiert nicht auf das Herunterfahren",
                "Das Betriebssystem in der VM hat nicht reagiert (z. B. während der Installation). Nutze im Menü „…“ → „Sofort ausschalten“.",
            ));
        }
        Ok(())
    }

    fn power_off(&self, vm: &VmRecord) -> AppResult<()> {
        match self.require_state(vm)?.as_str() {
            "saved" => {
                self.run("VM ausschalten", args(&["discardstate", &Self::id(vm)]))?;
            }
            s if Self::is_active(s) => {
                self.run("VM ausschalten", args(&["controlvm", &Self::id(vm), "poweroff"]))?;
                self.wait_for(vm, Duration::from_secs(20), |s| !Self::is_active(s));
            }
            _ => {}
        }
        Ok(())
    }

    fn open_console(&self, vm: &VmRecord) -> AppResult<()> {
        let st = self.require_state(vm)?;
        if !Self::is_active(&st) {
            return Err(AppError::new(format!("„{}“ läuft gerade nicht", vm.name), "Starte die VM, dann öffnet sich ihr Fenster automatisch."));
        }
        if !bring_to_front(VM_PROCESS, &Self::id(vm))? {
            // Läuft ohne Fenster (z. B. aus VirtualBox heraus „headless“ gestartet) – Fenster dazuholen.
            self.run("VM-Fenster öffnen", args(&["startvm", &Self::id(vm), "--type", "separate"]))?;
        }
        Ok(())
    }

    fn status(&self, vms: &[VmRecord]) -> AppResult<Vec<VmStatus>> {
        let states: Vec<(String, Option<String>)> = vms.iter().map(|v| (v.id.clone(), self.state(v))).collect();
        let any_active = states.iter().any(|(_, s)| s.as_deref().map(Self::is_active).unwrap_or(false));
        let procs = if any_active { process_info(VM_PROCESS) } else { vec![] };
        Ok(vms
            .iter()
            .zip(states)
            .map(|(vm, (id, st))| {
                let state = match st.as_deref() {
                    None => PowerState::Missing,
                    Some("running") => PowerState::Running,
                    Some("paused") | Some("saved") => PowerState::Paused,
                    Some("poweroff") | Some("aborted") => PowerState::Off,
                    Some("starting") | Some("restoring") => PowerState::Starting,
                    Some("stopping") => PowerState::Stopping,
                    Some("saving") => PowerState::Saving,
                    Some(_) => PowerState::Unknown,
                };
                let p = procs.iter().find(|p| p.cmd.contains(&Self::id(vm)));
                let (cpu, mem, up) = match p {
                    Some(p) if state == PowerState::Running || state == PowerState::Paused => (cpu_percent(p.pid, p.cpu), p.mem, p.up),
                    _ => (0.0, 0, 0),
                };
                VmStatus { id, state, cpu_percent: cpu, memory_used_mb: mem.min(vm.memory_mb + 512), uptime_seconds: up }
            })
            .collect())
    }

    fn rename(&self, vm: &VmRecord, new_name: &str) -> AppResult<()> {
        self.require_off(vm)?;
        self.run("VM umbenennen", args(&["modifyvm", &Self::id(vm), "--name", new_name]))?;
        Ok(())
    }

    fn update(&self, vm: &VmRecord, changes: &VmChanges) -> AppResult<()> {
        self.require_off(vm)?;
        self.run(
            "Einstellungen speichern",
            args(&["modifyvm", &Self::id(vm), "--cpus", &changes.cpus.to_string(), "--memory", &changes.memory_mb.to_string()]),
        )?;
        if changes.disk_gb > vm.disk_gb {
            self.run(
                "Festplatte vergrößern",
                args(&["modifymedium", "disk", &vm.disk_path, "--resize", &(changes.disk_gb * 1024).to_string()]),
            )?;
        }
        Ok(())
    }

    fn eject_iso(&self, vm: &VmRecord) -> AppResult<()> {
        self.run(
            "Installationsmedium auswerfen",
            args(&["storageattach", &Self::id(vm), "--storagectl", CONTROLLER, "--port", "1", "--device", "0", "--type", "dvddrive", "--medium", "emptydrive", "--forceunmount"]),
        )?;
        // Startreihenfolge nur änderbar, wenn die VM aus ist – sonst reicht das leere Laufwerk.
        let _ = self.run("Startreihenfolge anpassen", args(&["modifyvm", &Self::id(vm), "--boot1", "disk", "--boot2", "dvd"]));
        Ok(())
    }

    fn install_guest_tools(&self, vm: &VmRecord) -> AppResult<()> {
        // „additions“ ist die CD mit den VirtualBox-Gasterweiterungen (Grafik-, Maus- und Zwischenablage-Treiber).
        self.run(
            "Gasterweiterungen einlegen",
            args(&["storageattach", &Self::id(vm), "--storagectl", CONTROLLER, "--port", "1", "--device", "0", "--type", "dvddrive", "--medium", "additions", "--forceunmount"]),
        )?;
        Ok(())
    }

    fn delete(&self, vm: &VmRecord, delete_disk: bool) -> AppResult<()> {
        if let Some(st) = self.state(vm) {
            if Self::is_active(&st) {
                let _ = self.run("VM ausschalten", args(&["controlvm", &Self::id(vm), "poweroff"]));
                self.wait_for(vm, Duration::from_secs(20), |s| !Self::is_active(s));
                // VirtualBox gibt die Sitzung erst kurz nach dem Ausschalten frei.
                std::thread::sleep(Duration::from_millis(1500));
            }
            if delete_disk {
                self.run("VM löschen", args(&["unregistervm", &Self::id(vm), "--delete"]))?;
            } else {
                self.run("VM löschen", args(&["unregistervm", &Self::id(vm)]))?;
                let _ = self.run("Festplatte abmelden", args(&["closemedium", "disk", &vm.disk_path]));
            }
        }
        if delete_disk {
            remove_vm_files(vm)?;
        }
        Ok(())
    }

    fn create_snapshot(&self, vm: &VmRecord, name: &str) -> AppResult<Snapshot> {
        let st = self.require_state(vm)?;
        let live = Self::is_active(&st);
        let mut a = args(&["snapshot", &Self::id(vm), "take", name]);
        if live {
            a.push("--live".into());
        }
        let out = self.run("Sicherungspunkt erstellen", a)?;
        let id = parse_uuid(&out).ok_or_else(|| {
            AppError::new("Sicherungspunkt erstellen", "VirtualBox hat keine ID zurückgegeben.").with_details(out.clone())
        })?;
        Ok(Snapshot { id, name: name.to_string(), created: chrono::Utc::now().to_rfc3339(), with_state: live || st == "saved" })
    }

    fn restore_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()> {
        // VirtualBox stellt nur bei ausgeschalteter VM wieder her.
        let st = self.require_state(vm)?;
        if Self::is_active(&st) {
            self.run("VM anhalten", args(&["controlvm", &Self::id(vm), "poweroff"]))?;
            self.wait_for(vm, Duration::from_secs(20), |s| !Self::is_active(s));
            std::thread::sleep(Duration::from_millis(1500));
        }
        self.run("Sicherungspunkt wiederherstellen", args(&["snapshot", &Self::id(vm), "restore", snapshot_id]))?;
        Ok(())
    }

    fn delete_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()> {
        self.run("Sicherungspunkt löschen", args(&["snapshot", &Self::id(vm), "delete", snapshot_id]))?;
        Ok(())
    }
}
