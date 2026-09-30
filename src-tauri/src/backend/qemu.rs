//! QEMU-Backend (Fallback für Windows Home) mit WHPX-Beschleunigung,
//! UEFI (OVMF/edk2), qcow2-Festplatten und Steuerung über QMP.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;

use super::qmp::{self, Qmp};
use super::{bring_to_front, cpu_percent, process_info, remove_vm_files, safe_file_name};
use super::{CreateSpec, PowerState, Progress, Snapshot, VmBackend, VmChanges, VmStatus};
use crate::error::{AppError, AppResult};
use crate::host::find_qemu;
use crate::logger;
use crate::ps::hidden_command;
use crate::store::{BackendKind, OsFamily, Settings, VmRecord};

pub struct QemuBackend {
    exe: PathBuf,
}

/// Die UEFI-Firmware, die mit den Windows-Builds von QEMU ausgeliefert wird.
pub fn firmware_paths(qemu_exe: &Path) -> Option<(PathBuf, PathBuf)> {
    let base = qemu_exe.parent()?;
    for share in [base.join("share"), base.to_path_buf(), base.join("..").join("share").join("qemu")] {
        let code = share.join("edk2-x86_64-code.fd");
        let vars = share.join("edk2-i386-vars.fd");
        if code.is_file() && vars.is_file() {
            return Some((code, vars));
        }
    }
    None
}

fn not_installed() -> AppError {
    AppError::new(
        "QEMU ist nicht installiert",
        "Lade QEMU für Windows herunter (Einrichtung → „QEMU herunterladen“), installiere es mit den Standardeinstellungen und starte Nestbox neu.",
    )
}

/// Pfade in -drive-Optionen: Kommas müssen verdoppelt werden.
fn opt_path(p: &str) -> String {
    p.replace(',', ",,")
}

impl QemuBackend {
    pub fn from_settings(settings: &Settings) -> AppResult<Self> {
        let exe = find_qemu(settings).ok_or_else(not_installed)?;
        Ok(Self { exe })
    }

    fn img_exe(&self) -> PathBuf {
        self.exe.with_file_name("qemu-img.exe")
    }

    fn run_img(&self, what: &str, args: &[&str]) -> AppResult<String> {
        let exe = self.img_exe();
        logger::write("QEMU-IMG", &format!("\"{}\" {}", exe.display(), args.join(" ")));
        let out = hidden_command(&exe.to_string_lossy()).args(args).output().map_err(|e| {
            AppError::new(what, "qemu-img.exe wurde nicht gefunden. Installiere QEMU neu.").with_details(e.to_string())
        })?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            logger::error(&format!("{what}: {stderr}"));
            let hint = if stderr.contains("lock") {
                "Die Festplatte ist gerade in Benutzung. Schalte die VM aus und versuche es noch einmal."
            } else {
                "Prüfe, ob genug Speicherplatz frei ist. Details stehen in der Log-Datei."
            };
            return Err(AppError::new(what, hint).with_details(stderr));
        }
        Ok(stdout)
    }

    fn port(vm: &VmRecord) -> u16 {
        vm.qmp_port.unwrap_or(0)
    }

    /// Läuft QEMU für diese VM? (QMP-Port nimmt Verbindungen an)
    fn connect(vm: &VmRecord, timeout: Duration) -> Option<Qmp> {
        let port = Self::port(vm);
        if port == 0 {
            return None;
        }
        Qmp::connect(port, timeout).ok()
    }

    fn require_running(vm: &VmRecord) -> AppResult<Qmp> {
        Self::connect(vm, Duration::from_secs(300)).ok_or_else(|| {
            AppError::new(format!("„{}“ läuft gerade nicht", vm.name), "Starte die VM zuerst.")
        })
    }

    fn is_running(vm: &VmRecord) -> bool {
        Self::port(vm) != 0 && !qmp::port_is_free(Self::port(vm)) && Self::connect(vm, Duration::from_secs(2)).is_some()
    }

    fn require_off(vm: &VmRecord) -> AppResult<()> {
        if Self::is_running(vm) {
            return Err(AppError::new(
                "Die VM muss dafür ausgeschaltet sein",
                "Fahre die VM herunter und versuche es dann erneut.",
            ));
        }
        Ok(())
    }

    fn vars_path(vm: &VmRecord) -> PathBuf {
        PathBuf::from(&vm.dir).join("efivars.qcow2")
    }

    /// Baut die QEMU-Befehlszeile.
    fn args(&self, vm: &VmRecord, port: u16, loadvm: Option<&str>) -> AppResult<Vec<String>> {
        let (code, _) = firmware_paths(&self.exe).ok_or_else(|| {
            AppError::new(
                "Die UEFI-Firmware von QEMU fehlt",
                "Installiere QEMU neu (vollständige Installation), damit die Datei „edk2-x86_64-code.fd“ vorhanden ist.",
            )
        })?;
        let windows = vm.os_family == OsFamily::Windows;
        let mut a: Vec<String> = vec![
            "-name".into(),
            format!("{},process=nestbox", vm.name.replace(',', ",,")),
            "-uuid".into(),
            vm.id.clone(),
            "-machine".into(),
            "q35".into(),
            "-accel".into(),
            "whpx,kernel-irqchip=off".into(),
            "-accel".into(),
            "tcg".into(),
            "-smp".into(),
            vm.cpus.to_string(),
            "-m".into(),
            vm.memory_mb.to_string(),
            "-drive".into(),
            format!("if=pflash,format=raw,readonly=on,file={}", opt_path(&code.to_string_lossy())),
            "-drive".into(),
            format!("if=pflash,format=qcow2,file={}", opt_path(&Self::vars_path(vm).to_string_lossy())),
            "-drive".into(),
            format!("if=none,id=hd0,format=qcow2,discard=unmap,file={}", opt_path(&vm.disk_path)),
        ];
        // Windows kennt ohne Zusatztreiber kein virtio – dort SATA (AHCI), bei Linux das schnellere virtio.
        a.push("-device".into());
        a.push(if windows { "ide-hd,drive=hd0,bus=ide.0,bootindex=1".into() } else { "virtio-blk-pci,drive=hd0,bootindex=1".into() });
        let cd = match &vm.iso_path {
            Some(iso) if Path::new(iso).is_file() => format!("if=none,id=cd0,media=cdrom,readonly=on,file={}", opt_path(iso)),
            _ => "if=none,id=cd0,media=cdrom,readonly=on".into(),
        };
        a.extend([
            "-drive".into(),
            cd,
            "-device".into(),
            "ide-cd,drive=cd0,id=cdrom0,bus=ide.1,bootindex=0".into(),
            "-nic".into(),
            "user,model=e1000e".into(),
            "-device".into(),
            "qemu-xhci".into(),
            "-device".into(),
            "usb-tablet".into(),
            // Linux: virtio-Grafik (beliebige Auflösungen), Windows kennt ohne Treiber nur Standard-VGA
            "-vga".into(),
            if windows { "std".into() } else { "virtio".into() },
            // Ton über HD-Audio
            "-audiodev".into(),
            "dsound,id=snd0".into(),
            "-device".into(),
            "intel-hda".into(),
            "-device".into(),
            "hda-duplex,audiodev=snd0".into(),
            "-rtc".into(),
            if windows { "base=localtime".into() } else { "base=utc".into() },
            "-display".into(),
            "gtk,window-close=off".into(),
            "-qmp".into(),
            format!("tcp:127.0.0.1:{port},server=on,wait=off"),
        ]);
        if let Some(tag) = loadvm {
            a.push("-loadvm".into());
            a.push(tag.into());
        }
        Ok(a)
    }

    /// Startet den QEMU-Prozess. Gibt den tatsächlich genutzten QMP-Port zurück.
    pub fn launch(&self, vm: &VmRecord, loadvm: Option<&str>) -> AppResult<u16> {
        if Self::is_running(vm) {
            return Ok(Self::port(vm));
        }
        let mut port = Self::port(vm);
        if port == 0 || !qmp::port_is_free(port) {
            port = qmp::free_port();
        }
        let args = self.args(vm, port, loadvm)?;
        let log_path = PathBuf::from(&vm.dir).join("qemu.log");
        let log = fs::File::create(&log_path)?;
        logger::write(
            "QEMU",
            &format!(
                "\"{}\" {}",
                self.exe.display(),
                args.iter().map(|x| if x.contains(' ') { format!("\"{x}\"") } else { x.clone() }).collect::<Vec<_>>().join(" ")
            ),
        );
        let mut child = hidden_command(&self.exe.to_string_lossy())
            .args(&args)
            .current_dir(&vm.dir)
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()
            .map_err(|e| AppError::new("QEMU konnte nicht gestartet werden", "Installiere QEMU neu.").with_details(e.to_string()))?;

        // Kurz warten: Bricht QEMU sofort ab, zeigen wir den Grund an.
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Ok(Some(status)) = child.try_wait() {
                let text = fs::read_to_string(&log_path).unwrap_or_default();
                logger::error(&format!("QEMU beendet ({status}): {text}"));
                let hint = if text.contains("whpx") || text.contains("WHPX") {
                    "Die Windows-Hypervisor-Plattform ist nicht bereit. Aktiviere sie unter „Einrichtung“ und starte den PC neu."
                } else if text.to_lowercase().contains("could not open") || text.contains("Permission denied") {
                    "Eine Datei der VM konnte nicht geöffnet werden. Prüfe, ob die ISO-Datei noch am selben Ort liegt."
                } else {
                    "Details findest du unten und in der Log-Datei."
                };
                return Err(AppError::new("Die VM ist direkt nach dem Start beendet worden", hint).with_details(text));
            }
            if Qmp::connect(port, Duration::from_secs(2)).is_ok() {
                return Ok(port);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Ok(port)
    }

    fn wait_until_off(vm: &VmRecord, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if Self::connect(vm, Duration::from_secs(1)).is_none() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        false
    }

    fn snapshot_img(&self, vm: &VmRecord, op: &str, tag: &str, what: &str) -> AppResult<()> {
        self.run_img(what, &["snapshot", op, tag, &vm.disk_path])?;
        let vars = Self::vars_path(vm);
        if vars.is_file() {
            // Die UEFI-Variablen gehören zum Zustand dazu; Fehler hier sind nicht kritisch.
            let _ = self.run_img(what, &["snapshot", op, tag, &vars.to_string_lossy()]);
        }
        Ok(())
    }
}

impl VmBackend for QemuBackend {
    fn create(&self, spec: &CreateSpec, settings: &Settings, id: &str, progress: Progress) -> AppResult<VmRecord> {
        let name = spec.name.trim().to_string();
        let dir = PathBuf::from(&settings.vm_dir).join(safe_file_name(&name));
        let disk = dir.join(format!("{}.qcow2", safe_file_name(&name)));
        let (_, vars_template) = firmware_paths(&self.exe).ok_or_else(|| {
            AppError::new(
                "Die UEFI-Firmware von QEMU fehlt",
                "Installiere QEMU neu (vollständige Installation), damit „edk2-i386-vars.fd“ vorhanden ist.",
            )
        })?;

        progress("folder", "Ordner vorbereiten", "active");
        if disk.exists() {
            return Err(AppError::new(
                "Für diesen Namen existiert schon eine virtuelle Festplatte",
                "Gehe einen Schritt zurück und wähle einen anderen Namen.",
            )
            .with_details(disk.display().to_string()));
        }
        fs::create_dir_all(&dir)?;
        progress("folder", "Ordner vorbereiten", "done");

        let cleanup = |e: AppError| {
            let _ = fs::remove_dir_all(&dir);
            e
        };

        progress("disk", "Virtuelle Festplatte anlegen", "active");
        self.run_img(
            "Virtuelle Festplatte anlegen",
            &["create", "-f", "qcow2", &disk.to_string_lossy(), &format!("{}G", spec.disk_gb)],
        )
        .map_err(cleanup)?;
        progress("disk", "Virtuelle Festplatte anlegen", "done");

        // UEFI-Variablen als qcow2, damit Sicherungspunkte (savevm) den ganzen Zustand erfassen.
        progress("vm", "Virtuelle Maschine anlegen", "active");
        let vars = dir.join("efivars.qcow2");
        self.run_img(
            "UEFI-Firmware vorbereiten",
            &["convert", "-f", "raw", "-O", "qcow2", &vars_template.to_string_lossy(), &vars.to_string_lossy()],
        )
        .map_err(cleanup)?;
        progress("vm", "Virtuelle Maschine anlegen", "done");

        if spec.iso_path.is_some() {
            progress("iso", "Installationsmedium einlegen", "active");
            progress("iso", "Installationsmedium einlegen", "done");
        }

        Ok(VmRecord {
            id: id.to_string(),
            name,
            backend: BackendKind::Qemu,
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
            hyperv_id: None,
            imported: false,
            qmp_port: Some(qmp::free_port()),
            snapshots: vec![],
        })
    }

    fn start(&self, vm: &VmRecord) -> AppResult<()> {
        if let Some(mut q) = Self::connect(vm, Duration::from_secs(5)) {
            if q.status()? == "paused" {
                q.execute("cont", None)?;
            }
            return Ok(());
        }
        self.launch(vm, None)?;
        Ok(())
    }

    fn pause(&self, vm: &VmRecord) -> AppResult<()> {
        Self::require_running(vm)?.execute("stop", None)?;
        Ok(())
    }

    fn resume(&self, vm: &VmRecord) -> AppResult<()> {
        Self::require_running(vm)?.execute("cont", None)?;
        Ok(())
    }

    fn shutdown(&self, vm: &VmRecord) -> AppResult<()> {
        let mut q = Self::require_running(vm)?;
        if q.status()? == "paused" {
            q.execute("cont", None)?;
        }
        q.execute("system_powerdown", None)?;
        drop(q);
        if !Self::wait_until_off(vm, Duration::from_secs(90)) {
            return Err(AppError::new(
                "Die VM reagiert nicht auf das Herunterfahren",
                "Das Betriebssystem in der VM hat nicht reagiert (z. B. während der Installation). Nutze im Menü „…“ → „Sofort ausschalten“.",
            ));
        }
        Ok(())
    }

    fn power_off(&self, vm: &VmRecord) -> AppResult<()> {
        if let Some(mut q) = Self::connect(vm, Duration::from_secs(5)) {
            // „quit“ beendet QEMU sofort; die Verbindung bricht dabei erwartungsgemäß ab.
            let _ = q.execute("quit", None);
        }
        Self::wait_until_off(vm, Duration::from_secs(15));
        Ok(())
    }

    fn open_console(&self, vm: &VmRecord) -> AppResult<()> {
        if !Self::is_running(vm) {
            return Err(AppError::new(format!("„{}“ läuft gerade nicht", vm.name), "Starte die VM, dann öffnet sich ihr Fenster automatisch."));
        }
        // Das QEMU-Fenster existiert bereits – in den Vordergrund holen.
        bring_to_front("qemu-system-x86_64.exe", &vm.id)?;
        Ok(())
    }

    fn status(&self, vms: &[VmRecord]) -> AppResult<Vec<VmStatus>> {
        let mut states: Vec<(String, PowerState)> = Vec::new();
        for vm in vms {
            let state = match Self::connect(vm, Duration::from_millis(1500)) {
                None => PowerState::Off,
                Some(mut q) => match q.status().as_deref() {
                    Ok("running") => PowerState::Running,
                    Ok("paused") | Ok("suspended") => PowerState::Paused,
                    Ok("save-vm") | Ok("postmigrate") | Ok("finish-migrate") => PowerState::Saving,
                    Ok("restore-vm") | Ok("prelaunch") | Ok("inmigrate") => PowerState::Starting,
                    Ok("shutdown") => PowerState::Stopping,
                    // Verbindung belegt (z. B. während eines Sicherungspunkts)
                    Err(_) => PowerState::Running,
                    Ok(_) => PowerState::Unknown,
                },
            };
            states.push((vm.id.clone(), state));
        }

        let any_running = states.iter().any(|(_, s)| *s != PowerState::Off);
        let procs = if any_running { process_info("qemu-system-x86_64.exe") } else { vec![] };
        Ok(states
            .into_iter()
            .map(|(id, state)| {
                let (cpu, mem, up) = match procs.iter().find(|p| p.cmd.contains(&id)) {
                    Some(p) => (cpu_percent(p.pid, p.cpu), p.mem, p.up),
                    None => (0.0, 0, 0),
                };
                VmStatus { id, state, cpu_percent: cpu, memory_used_mb: mem, uptime_seconds: up }
            })
            .collect())
    }

    fn rename(&self, _vm: &VmRecord, _new_name: &str) -> AppResult<()> {
        // Der Name wird beim nächsten Start übernommen (-name); Dateien behalten ihren Namen.
        Ok(())
    }

    fn update(&self, vm: &VmRecord, changes: &VmChanges) -> AppResult<()> {
        Self::require_off(vm)?;
        if changes.disk_gb > vm.disk_gb {
            self.run_img("Festplatte vergrößern", &["resize", &vm.disk_path, &format!("{}G", changes.disk_gb)])?;
        }
        Ok(())
    }

    fn eject_iso(&self, vm: &VmRecord) -> AppResult<()> {
        if let Some(mut q) = Self::connect(vm, Duration::from_secs(5)) {
            q.execute("eject", Some(json!({ "id": "cdrom0", "force": true })))?;
        }
        Ok(())
    }

    fn install_guest_tools(&self, _vm: &VmRecord) -> AppResult<()> {
        Err(AppError::new(
            "Für QEMU gibt es keine Gasterweiterungen in Nestbox",
            "Für flüssiges Video empfehlen wir VirtualBox: Installiere es unter „Einstellungen → Einrichtung erneut prüfen“ und lege die VM damit neu an.",
        ))
    }

    fn delete(&self, vm: &VmRecord, delete_disk: bool) -> AppResult<()> {
        if Self::is_running(vm) {
            self.power_off(vm)?;
        }
        if delete_disk {
            remove_vm_files(vm)?;
        }
        Ok(())
    }

    fn create_snapshot(&self, vm: &VmRecord, name: &str) -> AppResult<Snapshot> {
        let tag = format!("nb{}", chrono::Utc::now().format("%Y%m%d%H%M%S"));
        let with_state = match Self::connect(vm, Duration::from_secs(600)) {
            Some(mut q) => {
                q.hmp(&format!("savevm {tag}"))?;
                true
            }
            None => {
                self.snapshot_img(vm, "-c", &tag, "Sicherungspunkt erstellen")?;
                false
            }
        };
        Ok(Snapshot { id: tag, name: name.to_string(), created: chrono::Utc::now().to_rfc3339(), with_state })
    }

    fn restore_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()> {
        let meta = vm.snapshots.iter().find(|s| s.id == snapshot_id).ok_or_else(|| {
            AppError::new("Der Sicherungspunkt existiert nicht mehr", "Lade die Liste neu.")
        })?;
        match Self::connect(vm, Duration::from_secs(600)) {
            Some(mut q) if meta.with_state => {
                q.hmp(&format!("loadvm {snapshot_id}"))?;
            }
            Some(_) => {
                return Err(AppError::new(
                    "Die VM muss dafür ausgeschaltet sein",
                    "Dieser Sicherungspunkt wurde bei ausgeschalteter VM erstellt. Fahre die VM herunter und stelle ihn dann wieder her.",
                ));
            }
            None if meta.with_state => {
                // Gespeicherten Zustand direkt beim Start laden.
                self.launch(vm, Some(snapshot_id))?;
            }
            None => self.snapshot_img(vm, "-a", snapshot_id, "Sicherungspunkt wiederherstellen")?,
        }
        Ok(())
    }

    fn delete_snapshot(&self, vm: &VmRecord, snapshot_id: &str) -> AppResult<()> {
        match Self::connect(vm, Duration::from_secs(300)) {
            Some(mut q) => {
                q.hmp(&format!("delvm {snapshot_id}"))?;
            }
            None => self.snapshot_img(vm, "-d", snapshot_id, "Sicherungspunkt löschen")?,
        }
        Ok(())
    }
}
