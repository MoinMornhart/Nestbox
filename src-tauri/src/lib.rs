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
