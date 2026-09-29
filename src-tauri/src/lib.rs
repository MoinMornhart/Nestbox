mod backend;
mod commands;
mod elevate;
mod error;
mod host;
mod logger;
mod ps;
mod store;

use std::sync::{Arc, Mutex};

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let store = store::Store::load();
    logger::info(&format!(
        "Nestbox {} gestartet · Daten: {} · VMs: {}",
        env!("CARGO_PKG_VERSION"),
        store::data_dir().display(),
        store.settings.vm_dir
    ));
    let state = Arc::new(AppState { store: Mutex::new(store) });

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_host_info,
            commands::install_software,
            commands::fix_enable_whpx,
            commands::restart_computer,
            commands::get_settings,
            commands::save_settings,
            commands::get_log_path,
            commands::list_vms,
            commands::vm_status,
            commands::check_vm_name,
            commands::inspect_iso,
            commands::create_vm,
            commands::start_vm,
            commands::pause_vm,
            commands::resume_vm,
            commands::shutdown_vm,
            commands::poweroff_vm,
            commands::open_console,
            commands::rename_vm,
            commands::update_vm,
            commands::eject_iso,
            commands::delete_vm,
            commands::list_snapshots,
            commands::create_snapshot,
            commands::restore_snapshot,
            commands::delete_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("Nestbox konnte nicht gestartet werden");
}


/// Tests gegen das echte System. Werden nur auf Wunsch ausgeführt:
///   cargo test -- --ignored --nocapture --test-threads=1
#[cfg(test)]
mod systemtest {
    use crate::backend::{self, qemu::QemuBackend, vbox::VBoxBackend, CreateSpec, VmBackend, VmChanges};
    use crate::store::{BackendKind, OsFamily, Settings, SnapshotMeta};

    #[test]
    #[ignore]
    fn einrichtung_pruefen() {
        let mut s = Settings::default();
        let info = crate::host::detect(&mut s).expect("Erkennung fehlgeschlagen");
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    }

    #[test]
    #[ignore]
    fn qemu_installieren() {
        crate::host::install_software(BackendKind::Qemu).expect("Installation");
    }

    #[test]
    #[ignore]
    fn vbox_installieren() {
        crate::host::install_software(BackendKind::Virtualbox).expect("Installation");
    }

    fn test_settings(name: &str) -> Settings {
        let dir = std::env::temp_dir().join(name);
        Settings { vm_dir: dir.to_string_lossy().to_string(), ..Settings::default() }
    }

    fn spec(name: &str) -> CreateSpec {
        CreateSpec { name: name.into(), os_family: OsFamily::Linux, os_id: "custom".into(), iso_path: None, cpus: 1, memory_mb: 512, disk_gb: 1 }
    }

    /// Kompletter Ablauf: anlegen, starten, pausieren, Sicherungspunkt, ausschalten,
    /// wiederherstellen, ändern, umbenennen, löschen.
    fn lebenszyklus(b: &dyn VmBackend, settings: &Settings, name: &str, can_run: bool) {
        let step = |s: &str, l: &str, st: &str| println!("  [{st}] {s}: {l}");
        let mut vm = b.create(&spec(name), settings, &uuid::Uuid::new_v4().to_string(), &step).expect("anlegen");
        println!("angelegt in {}", vm.dir);
        let res = (|| -> Result<(), crate::error::AppError> {
            if can_run {
                b.start(&vm)?;
                std::thread::sleep(std::time::Duration::from_secs(3));
                println!("läuft: {:?}", b.status(std::slice::from_ref(&vm))?);
                b.pause(&vm)?;
                println!("pausiert: {:?}", b.status(std::slice::from_ref(&vm))?[0].state);
                b.resume(&vm)?;
            }
            let snap = b.create_snapshot(&vm, "Testpunkt")?;
            println!("sicherungspunkt: {snap:?}");
            vm.snapshots.push(SnapshotMeta { id: snap.id.clone(), name: snap.name.clone(), created: chrono::Utc::now(), with_state: snap.with_state });
            println!("liste: {:?}", backend::list_snapshots(&vm));
            b.power_off(&vm)?;
            println!("aus: {:?}", b.status(std::slice::from_ref(&vm))?[0].state);
            b.restore_snapshot(&vm, &snap.id)?;
            b.power_off(&vm)?;
            b.delete_snapshot(&vm, &snap.id)?;
            b.update(&vm, &VmChanges { cpus: 2, memory_mb: 1024, disk_gb: 2 })?;
            b.rename(&vm, &format!("{name}-2"))?;
            vm.name = format!("{name}-2");
            println!("ende: {:?}", b.status(std::slice::from_ref(&vm))?);
            Ok(())
        })();
        b.delete(&vm, true).expect("löschen");
        println!("gelöscht, Ordner vorhanden: {}", std::path::Path::new(&vm.dir).exists());
        res.expect("Ablauf");
    }

    #[test]
    #[ignore]
    fn qemu_lebenszyklus() {
        let s = test_settings("nestbox-test-qemu");
        let mut vm_settings = s.clone();
        vm_settings.backend = crate::store::BackendChoice::Qemu;
        let b = QemuBackend::from_settings(&vm_settings).expect("QEMU");
        lebenszyklus(&b, &s, "Nestbox-Test-QEMU", true);
    }

    #[test]
    #[ignore]
    fn vbox_lebenszyklus() {
        let s = test_settings("nestbox-test-vbox");
        let b = VBoxBackend::from_settings(&s).expect("VirtualBox");
        // Ohne Hardware-Virtualisierung kann VirtualBox nicht starten – dann nur die Verwaltung testen.
        let mut probe = Settings::default();
        let can_run = crate::host::detect(&mut probe).map(|h| h.virtualization_enabled).unwrap_or(false);
        lebenszyklus(&b, &s, "Nestbox-Test-VBox", can_run);
    }
}
