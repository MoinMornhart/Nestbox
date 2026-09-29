//! Einrichtungsprüfung: Windows-Edition, Hyper-V, WHPX, Virtualisierung im BIOS,
//! Gruppenrechte, QEMU. Plus die Aktionen, um fehlende Punkte zu beheben.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::elevate;
use crate::error::AppResult;
use crate::ps;
use crate::store::{BackendChoice, BackendKind, Settings};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHost {
    caption: String,
    edition_id: String,
    build: String,
    hyperv_feature: String,
    hyperv_module: bool,
    vmms_running: bool,
    whpx_feature: String,
    virtualization_firmware: bool,
    vmx: bool,
    hypervisor_present: bool,
    windows_hypervisor: bool,
    machine: String,
    in_group_token: bool,
    in_group_member: bool,
    is_admin: bool,
    total_memory_mb: u64,
    logical_cores: u32,
    cpu_name: String,
    free_disk_gb: f64,
    last_boot: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub windows_name: String,
    pub edition_id: String,
    pub build: String,
    pub is_home: bool,
    /// enabled | disabled | unavailable
    pub hyperv_feature: String,
    pub hyperv_module: bool,
    pub vmms_running: bool,
    pub whpx_feature: String,
    pub virtualization_enabled: bool,
    pub hypervisor_present: bool,
    /// Läuft der Hypervisor von Windows selbst (Voraussetzung für Hyper-V und WHPX)?
    pub windows_hypervisor_running: bool,
    /// Läuft dieses Windows selbst in einer VM? Dann braucht es verschachtelte Virtualisierung.
    pub is_virtual_machine: bool,
    pub machine_name: String,
    /// Mitglied in „Hyper-V-Administratoren“ und in der aktuellen Anmeldung wirksam
    pub hyperv_group_ok: bool,
    /// Mitglied, aber erst nach Ab-/Anmelden wirksam
    pub hyperv_group_needs_relogin: bool,
    pub is_admin: bool,
    pub qemu_path: Option<String>,
    pub qemu_firmware: bool,
    pub total_memory_mb: u64,
    pub logical_cores: u32,
    pub cpu_name: String,
    pub free_disk_gb: f64,
    pub reboot_pending: bool,
    pub hyperv_ready: bool,
    pub qemu_ready: bool,
    pub active_backend: BackendKind,
    pub backend_choice: BackendChoice,
}

const DETECT: &str = r#"
function Get-FeatureState([string]$name) {
  try {
    $f = Get-WindowsOptionalFeature -Online -FeatureName $name -ErrorAction Stop
    if ($null -eq $f) { return 'unavailable' }
    if ($f.State -eq 'Enabled' -or $f.State -eq 'EnablePending') { return 'enabled' }
    return 'disabled'
  } catch {
    $c = Get-CimInstance Win32_OptionalFeature -Filter ('Name=''' + $name + '''')
    if ($null -eq $c) { return 'unavailable' }
    switch ($c.InstallState) { 1 { return 'enabled' } 2 { return 'disabled' } default { return 'unavailable' } }
  }
}
$os = Get-CimInstance Win32_OperatingSystem
$cs = Get-CimInstance Win32_ComputerSystem
$cpu = @(Get-CimInstance Win32_Processor)
$cv = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$me = [Security.Principal.WindowsIdentity]::GetCurrent()
$inToken = @($me.Groups | Where-Object { $_.Value -eq 'S-1-5-32-578' }).Count -gt 0
$isMember = $inToken
try {
  $isMember = $inToken -or (@(Get-LocalGroupMember -SID 'S-1-5-32-578' -ErrorAction Stop | Where-Object { $_.SID.Value -eq $me.User.Value }).Count -gt 0)
} catch {}
$isAdmin = (New-Object Security.Principal.WindowsPrincipal $me).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$vmms = Get-Service -Name vmms -ErrorAction SilentlyContinue
$free = 0
$winHv = $false
try { $winHv = @(Get-CimInstance Win32_PerfRawData_HvStats_HyperVHypervisor -ErrorAction Stop).Count -gt 0 } catch {}
try { $free = [System.IO.DriveInfo]::new(__DRIVE__).AvailableFreeSpace } catch {}
[pscustomobject]@{
  caption = [string]$os.Caption
  editionId = [string]$cv.EditionID
  build = [string]$cv.CurrentBuild
  hypervFeature = Get-FeatureState 'Microsoft-Hyper-V'
  hypervModule = [bool](Get-Module -ListAvailable -Name Hyper-V)
  vmmsRunning = ($null -ne $vmms -and $vmms.Status -eq 'Running')
  whpxFeature = Get-FeatureState 'HypervisorPlatform'
  virtualizationFirmware = [bool]($cpu | Where-Object { $_.VirtualizationFirmwareEnabled } | Select-Object -First 1)
  vmx = [bool]($cpu | Where-Object { $_.VMMonitorModeExtensions } | Select-Object -First 1)
  hypervisorPresent = [bool]$cs.HypervisorPresent
  windowsHypervisor = $winHv
  machine = ([string]$cs.Manufacturer + ' ' + [string]$cs.Model).Trim()
  inGroupToken = $inToken
  inGroupMember = $isMember
  isAdmin = $isAdmin
  totalMemoryMb = [uint64]($cs.TotalPhysicalMemory / 1MB)
  logicalCores = [uint32]$cs.NumberOfLogicalProcessors
  cpuName = ([string]$cpu[0].Name).Trim()
  freeDiskGb = [math]::Round($free / 1GB, 1)
  lastBoot = $os.LastBootUpTime.ToUniversalTime().ToString('o')
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
        for p in std::env::split_paths(&path) {
            candidates.push(p.join(exe));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub fn detect(settings: &mut Settings) -> AppResult<HostInfo> {
    let drive = Path::new(&settings.vm_dir)
        .components()
        .next()
        .map(|c| format!("{}\\", c.as_os_str().to_string_lossy()))
        .unwrap_or_else(|| "C:\\".into());
    let script = DETECT.replace("__DRIVE__", &ps::quote(&drive));
    let raw: RawHost = ps::run_json("Einrichtung prüfen", &script)?;
    Ok(build_info(raw, settings))
}

fn build_info(raw: RawHost, settings: &mut Settings) -> HostInfo {
    // Neustart erledigt? Dann Hinweis entfernen.
    if let Some(since) = settings.reboot_pending_since {
        if let Ok(boot) = chrono::DateTime::parse_from_rfc3339(&raw.last_boot) {
            if boot.with_timezone(&chrono::Utc) > since {
                settings.reboot_pending_since = None;
            }
        }
    }
    let reboot_pending = settings.reboot_pending_since.is_some();

    let is_home = raw.edition_id.to_lowercase().starts_with("core");
    // Läuft der Windows-Hypervisor, blendet er die Virtualisierungsbefehle der CPU aus –
    // dann ist die Virtualisierung trotzdem aktiv. Achtung: "HypervisorPresent" ist auch
    // wahr, wenn Windows selbst als Gast in einer VM läuft, und taugt daher nicht als Beweis.
    let virtualization_enabled = raw.windows_hypervisor || (raw.vmx && raw.virtualization_firmware);
    let m = raw.machine.to_lowercase();
    let is_virtual_machine = ["qemu", "kvm", "vmware", "virtualbox", "innotek", "virtual machine", "proxmox", "xen", "parallels"]
        .iter()
        .any(|k| m.contains(k))
        || raw.cpu_name.to_lowercase().contains("qemu");
    let hyperv_group_ok = raw.in_group_token || elevate::is_elevated();
    let hyperv_group_needs_relogin = raw.in_group_member && !raw.in_group_token;

    let qemu = find_qemu(settings);
    let qemu_firmware = qemu
        .as_ref()
        .map(|p| crate::backend::qemu::firmware_paths(p).is_some())
        .unwrap_or(false);

    let hyperv_ready = raw.hyperv_feature == "enabled"
        && raw.hyperv_module
        && raw.vmms_running
        && hyperv_group_ok
        && virtualization_enabled
        && raw.windows_hypervisor
        && !reboot_pending;
    let qemu_ready = qemu.is_some() && qemu_firmware && raw.whpx_feature == "enabled" && virtualization_enabled && !reboot_pending;

    let active_backend = match settings.backend {
        BackendChoice::Hyperv => BackendKind::Hyperv,
        BackendChoice::Qemu => BackendKind::Qemu,
        BackendChoice::Auto => {
            if !is_home && raw.hyperv_feature != "unavailable" {
                BackendKind::Hyperv
            } else {
                BackendKind::Qemu
            }
        }
    };

    HostInfo {
        windows_name: raw.caption.replace("Microsoft ", ""),
        edition_id: raw.edition_id,
        build: raw.build,
        is_home,
        hyperv_feature: raw.hyperv_feature,
        hyperv_module: raw.hyperv_module,
        vmms_running: raw.vmms_running,
        whpx_feature: raw.whpx_feature,
        virtualization_enabled,
        hypervisor_present: raw.hypervisor_present,
        windows_hypervisor_running: raw.windows_hypervisor,
        is_virtual_machine,
        machine_name: raw.machine,
        hyperv_group_ok,
        hyperv_group_needs_relogin,
        is_admin: raw.is_admin,
        qemu_path: qemu.map(|p| p.to_string_lossy().to_string()),
        qemu_firmware,
        total_memory_mb: raw.total_memory_mb,
        logical_cores: raw.logical_cores,
        cpu_name: raw.cpu_name,
        free_disk_gb: raw.free_disk_gb,
        reboot_pending,
        hyperv_ready,
        qemu_ready,
        active_backend,
        backend_choice: settings.backend,
    }
}

/// Fügt den aktuellen Benutzer der Gruppe „Hyper-V-Administratoren“ hinzu (UAC).
/// Die Gruppe wird über ihre SID angesprochen, damit es auf jeder Sprachversion klappt.
pub fn add_to_hyperv_group() -> AppResult<()> {
    let sid: String = ps::run("Benutzerkonto ermitteln", "[Security.Principal.WindowsIdentity]::GetCurrent().User.Value")?;
    let script = format!(
        "$sid = {sid}\r\n\
         $already = @(Get-LocalGroupMember -SID 'S-1-5-32-578' | Where-Object {{ $_.SID.Value -eq $sid }}).Count -gt 0\r\n\
         if (-not $already) {{ Add-LocalGroupMember -SID 'S-1-5-32-578' -Member $sid }}\r\n\
         'ok'",
        sid = ps::quote(sid.trim())
    );
    elevate::run_ps("Zur Gruppe „Hyper-V-Administratoren“ hinzufügen", &script)?;
    Ok(())
}

/// Aktiviert eine optionale Windows-Funktion per DISM (UAC). Danach ist ein Neustart nötig.
pub fn enable_feature(feature: &str, settings: &mut Settings) -> AppResult<()> {
    let script = format!(
        "$p = Start-Process -FilePath dism.exe -ArgumentList '/online','/Enable-Feature','/FeatureName:{feature}','/All','/NoRestart','/Quiet' -Wait -PassThru -WindowStyle Hidden\r\n\
         if ($p.ExitCode -ne 0 -and $p.ExitCode -ne 3010) {{ throw ('DISM ist mit Code ' + $p.ExitCode + ' fehlgeschlagen.') }}\r\n\
         'ok'"
    );
    let label = if feature == "HypervisorPlatform" { "Windows-Hypervisor-Plattform aktivieren" } else { "Hyper-V aktivieren" };
    elevate::run_ps(label, &script)?;
    settings.reboot_pending_since = Some(chrono::Utc::now());
    Ok(())
}

pub fn restart_computer() -> AppResult<()> {
    ps::run("Neustart", "shutdown.exe /r /t 5 /c 'Nestbox: Neustart zum Abschließen der Einrichtung'")?;
    Ok(())
}
