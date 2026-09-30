//! Einrichtungsprüfung: Windows-Version, Virtualisierung (BIOS bzw. verschachtelt),
//! VirtualBox, QEMU, Windows-Hypervisor-Plattform. Plus die Aktionen zum Beheben.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backend::vbox;
use crate::elevate;
use crate::error::{AppError, AppResult};
use crate::logger;
use crate::ps::{self, hidden_command};
use crate::store::{BackendChoice, BackendKind, Settings};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHost {
    caption: String,
    edition_id: String,
    build: String,
    whpx_feature: String,
    virtualization_firmware: bool,
    vmx: bool,
    windows_hypervisor: bool,
    machine: String,
    total_memory_mb: u64,
    logical_cores: u32,
    cpu_name: String,
    free_disk_gb: f64,
    last_boot: String,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub windows_name: String,
    pub edition_id: String,
    pub build: String,
    pub is_home: bool,
    /// Windows-Hypervisor-Plattform (WHPX) für schnelles QEMU: enabled | disabled | unavailable
    pub whpx_feature: String,
    /// Darf der Prozessor VMs ausführen? (BIOS bzw. – in einer VM – verschachtelte Virtualisierung)
    pub virtualization_enabled: bool,
    /// Läuft der Hypervisor von Windows (Voraussetzung für WHPX)?
    pub windows_hypervisor_running: bool,
    /// Läuft dieses Windows selbst in einer VM?
    pub is_virtual_machine: bool,
    pub machine_name: String,
    pub vbox_path: Option<String>,
    pub vbox_version: Option<String>,
    pub qemu_path: Option<String>,
    pub qemu_firmware: bool,
    /// QEMU ist vorhanden, startet aber nicht (z. B. abgebrochene Installation)
    pub qemu_broken: bool,
    /// QEMU läuft beschleunigt (WHPX aktiv) statt rein in Software
    pub qemu_accelerated: bool,
    pub winget_available: bool,
    pub total_memory_mb: u64,
    pub logical_cores: u32,
    pub cpu_name: String,
    pub free_disk_gb: f64,
    pub reboot_pending: bool,
    pub vbox_ready: bool,
    pub qemu_ready: bool,
    pub active_backend: BackendKind,
    pub backend_choice: BackendChoice,
}

const DETECT: &str = r#"
# Jede Abfrage hat einen Ausweichweg: Wird eine davon verweigert (z. B. WMI ohne
# Adminrechte), liefert sie einen sicheren Standardwert statt die Prüfung abzubrechen.
$ErrorActionPreference = 'Continue'
$warn = New-Object System.Collections.Generic.List[string]
function Try-Get([scriptblock]$block, $default, [string]$what) {
  try { $r = & $block; if ($null -eq $r) { return $default }; return $r }
  catch { $warn.Add($what + ': ' + $_.Exception.Message); return $default }
}
function Get-FeatureState([string]$name) {
  try {
    $f = Get-WindowsOptionalFeature -Online -FeatureName $name -ErrorAction Stop
    if ($null -eq $f) { return 'unavailable' }
    if ($f.State -eq 'Enabled' -or $f.State -eq 'EnablePending') { return 'enabled' }
    return 'disabled'
  } catch {}
  try {
    $c = Get-CimInstance Win32_OptionalFeature -Filter ('Name=' + [char]39 + $name + [char]39) -ErrorAction Stop
    if ($null -ne $c) { switch ($c.InstallState) { 1 { return 'enabled' } 2 { return 'disabled' } default { return 'unavailable' } } }
  } catch { $warn.Add('Windows-Funktion ' + $name + ': ' + $_.Exception.Message) }
  # Nicht abfragbar: als „aus“ melden – schlimmstenfalls bietet Nestbox das Einschalten an,
  # und DISM ändert bei einer bereits aktiven Funktion nichts.
  return 'disabled'
}
$os = Try-Get { Get-CimInstance Win32_OperatingSystem -ErrorAction Stop } $null 'Betriebssystem'
$cs = Try-Get { Get-CimInstance Win32_ComputerSystem -ErrorAction Stop } $null 'Computer'
$cpu = @(Try-Get { Get-CimInstance Win32_Processor -ErrorAction Stop } @() 'Prozessor')
$cv = Try-Get { Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -ErrorAction Stop } $null 'Windows-Version'
$build = [string]$cv.CurrentBuild
$caption = if ($os) { [string]$os.Caption } else { $n = [string]$cv.ProductName; if ([int]('0' + $build) -ge 22000) { $n -replace 'Windows 10', 'Windows 11' } else { $n } }
$memBytes = if ($cs) { [double]$cs.TotalPhysicalMemory } else { Try-Get { Add-Type -AssemblyName Microsoft.VisualBasic; [double](New-Object Microsoft.VisualBasic.Devices.ComputerInfo).TotalPhysicalMemory } 0 'Arbeitsspeicher' }
$cores = if ($cs -and $cs.NumberOfLogicalProcessors) { [uint32]$cs.NumberOfLogicalProcessors } else { [uint32][Environment]::ProcessorCount }
$cpuName = if ($cpu.Count -gt 0) { ([string]$cpu[0].Name).Trim() } else { [string](Try-Get { (Get-ItemProperty 'HKLM:\HARDWARE\DESCRIPTION\System\CentralProcessor\0' -ErrorAction Stop).ProcessorNameString } 'Unbekannter Prozessor' 'Prozessorname') }
# Unbekannt = nicht blockieren: Dann meldet VirtualBox beim Start selbst, falls etwas fehlt.
$virtKnown = $cpu.Count -gt 0
$fw = if ($virtKnown) { [bool]($cpu | Where-Object { $_.VirtualizationFirmwareEnabled } | Select-Object -First 1) } else { $true }
$vmx = if ($virtKnown) { [bool]($cpu | Where-Object { $_.VMMonitorModeExtensions } | Select-Object -First 1) } else { $true }
$free = Try-Get { [System.IO.DriveInfo]::new(__DRIVE__).AvailableFreeSpace } 0 'Speicherplatz'
$winHv = Try-Get { @(Get-CimInstance Win32_PerfRawData_HvStats_HyperVHypervisor -ErrorAction Stop).Count -gt 0 } $false 'Hypervisor'
# Startzeit ohne WMI: Der Hochleistungszähler läuft seit dem Hochfahren.
$boot = if ($os) { $os.LastBootUpTime } else { (Get-Date).AddSeconds(-[System.Diagnostics.Stopwatch]::GetTimestamp() / [System.Diagnostics.Stopwatch]::Frequency) }
[pscustomobject]@{
  caption = $caption
  editionId = [string]$cv.EditionID
  build = $build
  whpxFeature = Get-FeatureState 'HypervisorPlatform'
  virtualizationFirmware = $fw
  vmx = $vmx
  windowsHypervisor = [bool]$winHv
  machine = if ($cs) { ([string]$cs.Manufacturer + ' ' + [string]$cs.Model).Trim() } else { '' }
  totalMemoryMb = [uint64]($memBytes / 1MB)
  logicalCores = $cores
  cpuName = $cpuName
  freeDiskGb = [math]::Round($free / 1GB, 1)
  lastBoot = $boot.ToUniversalTime().ToString('o')
  warnings = @($warn)
} | ConvertTo-Json -Compress
"#;

/// Sucht qemu-system-x86_64.exe: eingestellter Ordner, Standardpfad, PATH.
pub fn find_qemu(settings: &Settings) -> Option<PathBuf> {
    let exe = "qemu-system-x86_64.exe";
    let mut candidates: Vec<PathBuf> = Vec::new();
    if !settings.qemu_dir.trim().is_empty() {
        candidates.push(Path::new(settings.qemu_dir.trim()).join(exe));
    }
    for var in ["ProgramFiles", "ProgramW6432"] {
        if let Some(pf) = std::env::var_os(var) {
            candidates.push(PathBuf::from(pf).join("qemu").join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|p| p.join(exe)));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Startet das Programm überhaupt (fehlende DLLs, beschädigte Installation)?
fn runs(exe: &Path) -> bool {
    hidden_command(&exe.to_string_lossy()).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn winget_available() -> bool {
    hidden_command("winget").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

pub fn detect(settings: &mut Settings) -> AppResult<HostInfo> {
    let drive = Path::new(&settings.vm_dir)
        .components()
        .next()
        .map(|c| format!("{}\\", c.as_os_str().to_string_lossy()))
        .unwrap_or_else(|| "C:\\".into());
    let script = DETECT.replace("__DRIVE__", &ps::quote(&drive));
    let raw: RawHost = ps::run_json("Einrichtung prüfen", &script).map_err(|mut e| {
        e.title = "Nestbox konnte deinen PC nicht prüfen".into();
        e.hint = "Windows PowerShell wird blockiert (z. B. durch eine Firmen-Richtlinie oder ein Sicherheitsprogramm). Starte Nestbox einmal mit Rechtsklick → „Als Administrator ausführen“. Hilft das nicht, schick die technischen Details an den Entwickler.".into();
        e
    })?;
    for w in &raw.warnings {
        logger::info(&format!("Einrichtung – Teilprüfung übersprungen: {w}"));
    }
    Ok(build_info(raw, settings))
}

fn build_info(raw: RawHost, settings: &mut Settings) -> HostInfo {
    // Neustart erledigt? Dann Hinweis entfernen (der Aufrufer speichert die Einstellungen).
    if let Some(since) = settings.reboot_pending_since {
        if let Ok(boot) = chrono::DateTime::parse_from_rfc3339(&raw.last_boot) {
            if boot.with_timezone(&chrono::Utc) > since {
                settings.reboot_pending_since = None;
            }
        }
    }
    let reboot_pending = settings.reboot_pending_since.is_some();

    // Läuft der Windows-Hypervisor, blendet er die Virtualisierungsbefehle der CPU aus –
    // die Virtualisierung ist dann trotzdem aktiv. „HypervisorPresent“ taugt nicht als Beweis:
    // Es ist auch wahr, wenn Windows selbst als Gast in einer VM läuft.
    let virtualization_enabled = raw.windows_hypervisor || (raw.vmx && raw.virtualization_firmware);
    let m = raw.machine.to_lowercase();
    let is_virtual_machine = ["qemu", "kvm", "vmware", "virtualbox", "innotek", "virtual machine", "proxmox", "xen", "parallels"]
        .iter()
        .any(|k| m.contains(k))
        || raw.cpu_name.to_lowercase().contains("qemu");

    let vbox = vbox::find_vboxmanage();
    let vbox_version = vbox.as_deref().and_then(vbox::version);
    let qemu = find_qemu(settings);
    // Nicht nur „vorhanden“, sondern „startet wirklich“ – eine abgebrochene Installation
    // hinterlässt Programmdateien ohne ihre DLLs.
    let qemu_works = qemu.as_ref().map(|p| runs(&p.with_file_name("qemu-img.exe"))).unwrap_or(false);
    let qemu_firmware = qemu_works && qemu.as_ref().map(|p| crate::backend::qemu::firmware_paths(p).is_some()).unwrap_or(false);
    let qemu_accelerated = raw.whpx_feature == "enabled" && raw.windows_hypervisor;

    let vbox_ready = vbox.is_some() && virtualization_enabled && !reboot_pending;
    // QEMU läuft notfalls auch ohne Beschleunigung (dann langsam).
    let qemu_ready = qemu.is_some() && qemu_firmware && !reboot_pending;

    let active_backend = match settings.backend {
        BackendChoice::Virtualbox => BackendKind::Virtualbox,
        BackendChoice::Qemu => BackendKind::Qemu,
        BackendChoice::Auto => {
            if vbox_ready {
                BackendKind::Virtualbox
            } else if qemu_ready {
                BackendKind::Qemu
            } else if vbox.is_some() || virtualization_enabled {
                BackendKind::Virtualbox
            } else {
                BackendKind::Qemu
            }
        }
    };

    HostInfo {
        windows_name: raw.caption.replace("Microsoft ", ""),
        is_home: raw.edition_id.to_lowercase().starts_with("core"),
        edition_id: raw.edition_id,
        build: raw.build,
        whpx_feature: raw.whpx_feature,
        virtualization_enabled,
        windows_hypervisor_running: raw.windows_hypervisor,
        is_virtual_machine,
        machine_name: raw.machine,
        vbox_path: vbox.map(|p| p.to_string_lossy().to_string()),
        vbox_version,
        qemu_path: qemu.as_ref().map(|p| p.to_string_lossy().to_string()),
        qemu_broken: qemu.is_some() && !qemu_works,
        qemu_firmware,
        qemu_accelerated,
        winget_available: winget_available(),
        total_memory_mb: raw.total_memory_mb,
        logical_cores: raw.logical_cores,
        cpu_name: raw.cpu_name,
        free_disk_gb: raw.free_disk_gb,
        reboot_pending,
        vbox_ready,
        qemu_ready,
        active_backend,
        backend_choice: settings.backend,
    }
}

/// Installiert VirtualBox oder QEMU über winget. Den UAC-Dialog zeigt das Installationsprogramm selbst.
pub fn install_software(kind: BackendKind) -> AppResult<()> {
    let (id, label) = match kind {
        BackendKind::Virtualbox => ("Oracle.VirtualBox", "VirtualBox"),
        BackendKind::Qemu => ("SoftwareFreedomConservancy.QEMU", "QEMU"),
    };
    // „--source winget“: nur die normale winget-Quelle. Sonst fragt winget auch den Microsoft
    // Store – schlägt der fehl (z. B. Zertifikatsfehler), bricht die ganze Installation ab.
    let args = ["install", "--id", id, "--exact", "--source", "winget", "--silent", "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"];
    logger::write("WINGET", &format!("winget {}", args.join(" ")));
    let out = hidden_command("winget").args(args).output().map_err(|e| {
        AppError::new(
            format!("{label} konnte nicht installiert werden"),
            "Der Windows-Paketmanager (winget) fehlt. Lade das Programm über den Link „Download-Seite“ herunter.",
        )
        .with_details(e.to_string())
    })?;
    let raw_text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    // Fortschrittsbalken und Drehzeichen von winget entfernen – übrig bleiben die eigentlichen Meldungen.
    let text = raw_text
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains('█') && !l.contains('▒') && !matches!(*l, "-" | "\\" | "|" | "/"))
        .collect::<Vec<_>>()
        .join("\n");
    logger::write("WINGET", &ps::truncate(&text, 3000));
    // 0x8A15002B: bereits installiert, kein Update verfügbar – auch in Ordnung.
    let code = out.status.code().unwrap_or(-1);
    if out.status.success() || code == -1978335189 || text.contains("bereits installiert") || text.contains("already installed") {
        return Ok(());
    }
    let lower = text.to_lowercase();
    let hint = if text.contains("1602") || lower.contains("cancel") || lower.contains("abgebrochen") {
        "Die Installation wurde abgebrochen. Bestätige die Windows-Abfrage („Möchten Sie zulassen …?“) mit „Ja“."
    } else if lower.contains("certificate") || lower.contains("zertifikat") || lower.contains("0x8a15005e") {
        "Windows konnte keine sichere Verbindung zum Download-Server aufbauen (Zertifikatsfehler). Prüfe Datum und Uhrzeit des PCs und ob ein Virenscanner oder Proxy HTTPS-Verbindungen umleitet – oder installiere das Programm über die Download-Seite."
    } else if lower.contains("disk") || lower.contains("speicherplatz") {
        "Auf dem Laufwerk ist nicht genug Platz frei. Schaffe etwas Platz und versuche es erneut."
    } else {
        "Versuche es noch einmal oder installiere das Programm über die Download-Seite von Hand."
    };
    Err(AppError::new(format!("{label} konnte nicht installiert werden"), hint).with_details(text))
}

/// Aktiviert die Windows-Hypervisor-Plattform per DISM (UAC). Danach ist ein Neustart nötig.
pub fn enable_whpx(settings: &mut Settings) -> AppResult<()> {
    let script = "$p = Start-Process -FilePath dism.exe -ArgumentList '/online','/Enable-Feature','/FeatureName:HypervisorPlatform','/All','/NoRestart','/Quiet' -Wait -PassThru -WindowStyle Hidden\r\n\
         if ($p.ExitCode -ne 0 -and $p.ExitCode -ne 3010) { throw ('DISM ist mit Code ' + $p.ExitCode + ' fehlgeschlagen.') }\r\n\
         'ok'";
    elevate::run_ps("Windows-Hypervisor-Plattform aktivieren", script)?;
    settings.reboot_pending_since = Some(chrono::Utc::now());
    Ok(())
}

pub fn restart_computer() -> AppResult<()> {
    ps::run("Neustart", "shutdown.exe /r /t 5 /c 'Nestbox: Neustart zum Abschließen der Einrichtung'")?;
    Ok(())
}
