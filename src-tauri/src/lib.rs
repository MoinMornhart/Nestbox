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
            commands::fix_hyperv_group,
            commands::fix_enable_feature,
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
    use crate::backend::hyperv::HyperVBackend;
    use crate::backend::{CreateSpec, VmBackend, VmChanges};
    use crate::store::{OsFamily, Settings};

    #[test]
    #[ignore]
    fn einrichtung_pruefen() {
        let mut s = Settings::default();
        let info = crate::host::detect(&mut s).expect("Erkennung fehlgeschlagen");
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    }

    /// Kompletter Ablauf mit einer Test-VM ohne ISO: anlegen, starten, pausieren,
    /// Sicherungspunkt, ausschalten, ändern, umbenennen, löschen.
    #[test]
    #[ignore]
    fn hyperv_lebenszyklus() {
        let dir = std::env::temp_dir().join("nestbox-systemtest");
        let settings = Settings { vm_dir: dir.to_string_lossy().to_string(), ..Settings::default() };
        let b = HyperVBackend;
        let spec = CreateSpec {
            name: "Nestbox-Systemtest".into(),
            os_family: OsFamily::Linux,
            os_id: "custom".into(),
            iso_path: None,
            cpus: 1,
            memory_mb: 512,
            disk_gb: 1,
        };
        let step = |s: &str, l: &str, st: &str| println!("  [{st}] {s}: {l}");
        let mut vm = b.create(&spec, &settings, "test-id", &step).expect("anlegen");
        println!("angelegt: {:?}", vm.hyperv_id);
        let res = (|| -> Result<(), crate::error::AppError> {
            b.start(&vm)?;
            println!("status: {:?}", b.status(std::slice::from_ref(&vm))?);
            b.pause(&vm)?;
            println!("pausiert: {:?}", b.status(std::slice::from_ref(&vm))?[0].state);
            b.resume(&vm)?;
            let snap = b.create_snapshot(&vm, "Testpunkt")?;
            println!("sicherungspunkte: {:?}", b.list_snapshots(&vm)?);
            b.power_off(&vm)?;
            b.restore_snapshot(&vm, &snap.id)?;
            b.power_off(&vm)?;
            b.delete_snapshot(&vm, &snap.id)?;
            b.update(&vm, &VmChanges { cpus: 2, memory_mb: 1024, disk_gb: 2 })?;
            b.rename(&vm, "Nestbox-Systemtest-2")?;
            vm.name = "Nestbox-Systemtest-2".into();
            println!("status: {:?}", b.status(std::slice::from_ref(&vm))?);
            Ok(())
        })();
        b.delete(&vm, true).expect("löschen");
        println!("gelöscht, Ordner vorhanden: {}", std::path::Path::new(&vm.dir).exists());
        res.expect("Ablauf");
    }
}
