//! ISO-Bibliothek: lädt die jeweils neueste Version der Betriebssysteme aus dem Katalog
//! direkt von den offiziellen Servern, prüft die Prüfsumme und merkt sie sich.
//! Gibt es eine neuere Version, wird sie beim nächsten Mal geladen und die alte gelöscht.
//!
//! Der Download läuft über curl.exe (seit Windows 10 1803 eingebaut): Fortsetzen nach
//! Abbruch, Weiterleitungen und Wiederholungen bringt es mit, ohne zusätzliche Bibliothek.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::logger;
use crate::ps::{self, quote};
use crate::store::Settings;

/// Neueste Version eines Systems laut Hersteller.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Latest {
    file_name: String,
    url: String,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsoStatus {
    /// ready = neueste Version liegt bereit · missing = noch nicht geladen
    /// · outdated = ältere Version vorhanden (offline nutzbar) · manual = nur von Hand (Windows)
    pub state: String,
    pub path: Option<String>,
    pub file_name: Option<String>,
    pub version: String,
    /// Offline oder Server nicht erreichbar – Status beruht nur auf vorhandenen Dateien
    pub offline: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsoProgress {
    /// resolve | download | verify | done
    pub phase: String,
    pub received: u64,
    pub total: u64,
    pub file_name: String,
}

/// Dateinamen-Muster je System (zum Finden und Aufräumen alter Versionen).
fn matches_os(os_id: &str, file: &str) -> bool {
    let f = file.to_lowercase();
    if !f.ends_with(".iso") {
        return false;
    }
    match os_id {
        "ubuntu" => f.starts_with("ubuntu-") && f.contains("desktop-amd64"),
        "mint" => f.starts_with("linuxmint-") && f.contains("cinnamon-64bit"),
        "fedora" => f.starts_with("fedora-workstation-live"),
        "windows11" => f.starts_with("win11"),
        _ => false,
    }
}

pub fn iso_dir(settings: &Settings) -> PathBuf {
    if !settings.iso_dir.trim().is_empty() {
        return PathBuf::from(&settings.iso_dir);
    }
    let vm = PathBuf::from(&settings.vm_dir);
    vm.parent()
        .map(|p| p.join("ISOs"))
        .unwrap_or_else(|| vm.join("ISOs"))
}

/// Ermittelt die neueste Version beim Hersteller (kleine Textdateien, kein großer Download).
fn resolve_latest(os_id: &str) -> AppResult<Latest> {
    let script = match os_id {
        // Neueste unterstützte LTS-Version; aus SHA256SUMS die höchste Punktversion.
        "ubuntu" => r#"
function Get-Text($u) { $r = Invoke-WebRequest $u -UseBasicParsing -TimeoutSec 20; if ($r.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($r.Content) } else { [string]$r.Content } }
$meta = Get-Text 'https://changelogs.ubuntu.com/meta-release-lts'
$dist = @([regex]::Matches($meta, '(?ms)^Dist: (\S+).*?^Supported: 1') | ForEach-Object { $_.Groups[1].Value })[-1]
$sums = Get-Text "https://releases.ubuntu.com/$dist/SHA256SUMS"
$best = [regex]::Matches($sums, '(?m)^([0-9a-f]{64}) \*?(ubuntu-([\d.]+)-desktop-amd64\.iso)\s*$') |
  Sort-Object { [version](($_.Groups[3].Value + '.0.0').Split('.')[0..2] -join '.') } | Select-Object -Last 1
if ($null -eq $best) { throw 'Keine Ubuntu-ISO gefunden.' }
[pscustomobject]@{ fileName = $best.Groups[2].Value; url = "https://releases.ubuntu.com/$dist/" + $best.Groups[2].Value; sha256 = $best.Groups[1].Value; version = $best.Groups[3].Value + ' LTS' } | ConvertTo-Json -Compress
"#
        .to_string(),
        "mint" => r#"
function Get-Text($u) { $r = Invoke-WebRequest $u -UseBasicParsing -TimeoutSec 20; if ($r.Content -is [byte[]]) { [Text.Encoding]::UTF8.GetString($r.Content) } else { [string]$r.Content } }
$base = 'https://mirrors.kernel.org/linuxmint/stable/'
$v = [regex]::Matches((Get-Text $base), 'href="(\d+(?:\.\d+)?)/"') | ForEach-Object { $_.Groups[1].Value } |
  Sort-Object { [version]($(if ($_ -match '\.') { $_ } else { "$_.0" })) } | Select-Object -Last 1
$m = [regex]::Match((Get-Text "$base$v/sha256sum.txt"), '(?m)^([0-9a-f]{64}) \*?(linuxmint-[\d.]+-cinnamon-64bit\.iso)')
if (-not $m.Success) { throw 'Keine Mint-ISO gefunden.' }
[pscustomobject]@{ fileName = $m.Groups[2].Value; url = "$base$v/" + $m.Groups[2].Value; sha256 = $m.Groups[1].Value; version = $v } | ConvertTo-Json -Compress
"#
        .to_string(),
        "fedora" => r#"
$r = Invoke-RestMethod 'https://fedoraproject.org/releases.json' -TimeoutSec 20
$best = $r | Where-Object { $_.variant -eq 'Workstation' -and $_.arch -eq 'x86_64' -and $_.link -like '*.iso' -and $_.version -match '^\d+$' } |
  Sort-Object { [int]$_.version } | Select-Object -Last 1
if ($null -eq $best) { throw 'Keine Fedora-ISO gefunden.' }
[pscustomobject]@{ fileName = Split-Path $best.link -Leaf; url = $best.link; sha256 = [string]$best.sha256; version = [string]$best.version } | ConvertTo-Json -Compress
"#
        .to_string(),
        _ => {
            return Err(AppError::new(
                "Für dieses System gibt es keinen automatischen Download",
                "Lade die ISO-Datei auf der offiziellen Seite herunter und ziehe sie ins Fenster.",
            ))
        }
    };
    ps::run_json::<Latest>("Neueste Version ermitteln", &script).map_err(|mut e| {
        e.title = "Die neueste Version konnte nicht ermittelt werden".into();
        e.hint = "Prüfe deine Internetverbindung. Alternativ kannst du die ISO-Datei selbst herunterladen und ins Fenster ziehen.".into();
        e
    })
}

/// Alle vorhandenen ISOs dieses Systems in der Bibliothek, neueste zuerst.
fn local_isos(dir: &Path, os_id: &str) -> Vec<PathBuf> {
    let mut list: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| matches_os(os_id, &e.file_name().to_string_lossy()))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    list.sort_by(|a, b| b.0.cmp(&a.0));
    list.into_iter().map(|(_, p)| p).collect()
}

/// Windows-11-ISO im Downloads-Ordner suchen (Microsoft bietet keinen festen Download-Link).
fn find_windows_download() -> Option<PathBuf> {
    let downloads = std::env::var("USERPROFILE")
        .ok()
        .map(|h| PathBuf::from(h).join("Downloads"))?;
    local_isos(&downloads, "windows11").into_iter().next()
}

pub fn status(settings: &Settings, os_id: &str) -> AppResult<IsoStatus> {
    let dir = iso_dir(settings);
    let local = local_isos(&dir, os_id);
    let file_status = |state: &str, p: &Path, version: String, offline: bool| IsoStatus {
        state: state.into(),
        path: Some(p.to_string_lossy().to_string()),
        file_name: p.file_name().map(|f| f.to_string_lossy().to_string()),
        version,
        offline,
    };

    if os_id == "windows11" {
        // Bibliothek zuerst, dann Downloads-Ordner (wird beim Benutzen in die Bibliothek verschoben).
        if let Some(p) = local.first().cloned().or_else(find_windows_download) {
            return Ok(file_status("ready", &p, String::new(), false));
        }
        return Ok(IsoStatus {
            state: "manual".into(),
            path: None,
            file_name: None,
            version: String::new(),
            offline: false,
        });
    }

    match resolve_latest(os_id) {
        Ok(latest) => {
            let target = dir.join(&latest.file_name);
            if target.is_file() {
                Ok(file_status("ready", &target, latest.version, false))
            } else if let Some(old) = local.first() {
                Ok(file_status("outdated", old, latest.version, false))
            } else {
                Ok(IsoStatus {
                    state: "missing".into(),
                    path: None,
                    file_name: Some(latest.file_name),
                    version: latest.version,
                    offline: false,
                })
            }
        }
        // Offline: Vorhandenes nutzen, sonst „fehlt“.
        Err(_) => match local.first() {
            Some(p) => Ok(file_status("ready", p, String::new(), true)),
            None => Ok(IsoStatus {
                state: "missing".into(),
                path: None,
                file_name: None,
                version: String::new(),
                offline: true,
            }),
        },
    }
}

fn running() -> &'static Mutex<HashMap<String, Child>> {
    static RUNNING: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    RUNNING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Bricht einen laufenden Download ab. Die Teildatei bleibt liegen und wird beim nächsten Mal fortgesetzt.
pub fn cancel(os_id: &str) {
    if let Some(mut child) = running()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(os_id)
    {
        let _ = child.kill();
        logger::info(&format!("ISO-Download abgebrochen: {os_id}"));
    }
}

fn free_bytes(dir: &Path) -> Option<u64> {
    let drive = dir
        .components()
        .next()?
        .as_os_str()
        .to_string_lossy()
        .to_string();
    let out = ps::run_quiet(&format!(
        "[System.IO.DriveInfo]::new({}).AvailableFreeSpace",
        quote(&drive)
    ))
    .ok()?;
    out.trim().parse().ok()
}

fn remote_size(url: &str) -> u64 {
    let out = ps::hidden_command("curl.exe")
        .args(["-sIL", "--max-time", "20", url])
        .output();
    let Ok(out) = out else { return 0 };
    // Bei Weiterleitungen gilt die letzte Content-Length.
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            l.to_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().to_string())
        })
        .filter_map(|v| v.parse::<u64>().ok())
        .last()
        .unwrap_or(0)
}

fn sha256_of(path: &Path) -> AppResult<String> {
    let out = ps::run(
        "Prüfsumme berechnen",
        &format!(
            "(Get-FileHash -LiteralPath {} -Algorithm SHA256).Hash",
            quote(&path.to_string_lossy())
        ),
    )?;
    Ok(out.trim().to_lowercase())
}

/// Lädt die neueste Version (falls nötig) und liefert den Pfad zur fertigen ISO.
pub fn ensure(
    settings: &Settings,
    os_id: &str,
    progress: impl Fn(IsoProgress),
) -> AppResult<String> {
    let dir = iso_dir(settings);
    fs::create_dir_all(&dir).map_err(|e| {
        AppError::new(
            "Der ISO-Ordner kann nicht angelegt werden",
            "Wähle in den Einstellungen einen anderen Ordner für ISO-Dateien.",
        )
        .with_details(format!("{}: {e}", dir.display()))
    })?;

    if os_id == "windows11" {
        return adopt_windows(&dir);
    }

    progress(IsoProgress {
        phase: "resolve".into(),
        received: 0,
        total: 0,
        file_name: String::new(),
    });
    let latest = resolve_latest(os_id)?;
    let target = dir.join(&latest.file_name);
    if target.is_file() {
        progress(IsoProgress {
            phase: "done".into(),
            received: 0,
            total: 0,
            file_name: latest.file_name.clone(),
        });
        return Ok(target.to_string_lossy().to_string());
    }

    let part = dir.join(format!("{}.part", latest.file_name));
    let total = remote_size(&latest.url);
    let have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if total > 0 {
        if let Some(free) = free_bytes(&dir) {
            let need = total.saturating_sub(have) + 512 * 1024 * 1024;
            if free < need {
                return Err(AppError::new(
                    format!("Nicht genug Speicherplatz für {}", latest.file_name),
                    format!(
                        "Die ISO braucht {:.1} GB, auf dem Laufwerk sind nur {:.1} GB frei. Wähle unter „Einstellungen → ISO-Dateien“ einen Ordner auf einem anderen Laufwerk oder schaffe Platz.",
                        need as f64 / 1e9,
                        free as f64 / 1e9
                    ),
                )
                .with_details(dir.display().to_string()));
            }
        }
    }

    // Teildatei schon vollständig (z. B. Umbenennen zuletzt fehlgeschlagen)? Dann nicht erneut laden.
    let complete = total > 0 && have >= total;
    if !complete {
        logger::info(&format!(
            "ISO-Download: {} ({} Bytes) nach {}",
            latest.url,
            total,
            dir.display()
        ));
        let mut cmd = ps::hidden_command("curl.exe");
        cmd.args([
            "-L",
            "--fail",
            "--silent",
            "--show-error",
            "--retry",
            "3",
            "--retry-delay",
            "3",
            "-C",
            "-",
            "-o",
        ])
        .arg(&part)
        .arg(&latest.url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
        let child = cmd.spawn().map_err(|e| {
        AppError::new("Der Download konnte nicht gestartet werden", "curl.exe fehlt – das ist ab Windows 10 (1803) eingebaut. Lade die ISO stattdessen von Hand herunter.")
            .with_details(e.to_string())
    })?;
        running()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(os_id.to_string(), child);

        // Fortschritt über die Dateigröße verfolgen, bis curl fertig ist.
        let exit = loop {
            std::thread::sleep(Duration::from_millis(400));
            let received = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            progress(IsoProgress {
                phase: "download".into(),
                received,
                total,
                file_name: latest.file_name.clone(),
            });
            let mut map = running().lock().unwrap_or_else(|e| e.into_inner());
            let Some(child) = map.get_mut(os_id) else {
                return Err(AppError::new(
                    "Download abgebrochen",
                    "Beim nächsten Versuch wird er an derselben Stelle fortgesetzt.",
                ));
            };
            if let Some(status) = child.try_wait()? {
                let mut child = map.remove(os_id).expect("eben gefunden");
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    use std::io::Read;
                    let _ = s.read_to_string(&mut err);
                }
                break (status, err);
            }
        };
        if !exit.0.success() {
            return Err(AppError::new(
            "Der Download ist fehlgeschlagen",
            "Prüfe deine Internetverbindung und versuche es noch einmal – bereits Geladenes wird fortgesetzt.",
        )
        .with_details(format!("{}\n{}", latest.url, exit.1.trim())));
        }
    }

    if !latest.sha256.is_empty() {
        let size = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        progress(IsoProgress {
            phase: "verify".into(),
            received: size,
            total: size,
            file_name: latest.file_name.clone(),
        });
        let actual = sha256_of(&part)?;
        if actual != latest.sha256.to_lowercase() {
            let _ = fs::remove_file(&part);
            return Err(AppError::new(
                "Die geladene Datei ist beschädigt",
                "Die Prüfsumme stimmt nicht mit der des Herstellers überein. Die Datei wurde gelöscht – versuche es noch einmal.",
            )
            .with_details(format!("erwartet {}\nerhalten {actual}", latest.sha256)));
        }
    }
    fs::rename(&part, &target)?;

    // Ältere Versionen desselben Systems aufräumen.
    for old in local_isos(&dir, os_id).into_iter().filter(|p| p != &target) {
        if fs::remove_file(&old).is_ok() {
            logger::info(&format!("Alte ISO gelöscht: {}", old.display()));
        }
    }
    progress(IsoProgress {
        phase: "done".into(),
        received: 0,
        total: 0,
        file_name: latest.file_name.clone(),
    });
    logger::info(&format!("ISO bereit: {}", target.display()));
    Ok(target.to_string_lossy().to_string())
}

/// Windows 11: Vorhandene ISO aus der Bibliothek nehmen oder aus „Downloads“ hineinverschieben.
fn adopt_windows(dir: &Path) -> AppResult<String> {
    if let Some(p) = local_isos(dir, "windows11").into_iter().next() {
        return Ok(p.to_string_lossy().to_string());
    }
    let Some(src) = find_windows_download() else {
        return Err(AppError::new(
            "Windows 11 muss einmal von Hand geladen werden",
            "Microsoft erlaubt keinen automatischen Download. Lade die ISO auf der Microsoft-Seite herunter – Nestbox findet sie danach im Downloads-Ordner von selbst.",
        ));
    };
    let target = dir.join(src.file_name().unwrap_or_default());
    // Verschieben klappt nur auf demselben Laufwerk; sonst einfach dort lassen und benutzen.
    match fs::rename(&src, &target) {
        Ok(()) => Ok(target.to_string_lossy().to_string()),
        Err(_) => Ok(src.to_string_lossy().to_string()),
    }
}
