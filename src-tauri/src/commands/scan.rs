use tauri::Manager;

use media_tagger_core::{
    app::state::AppState,
    models::{RemoveRootSummary, ScanRoot, ScanSummary},
    usecases::scan,
};

use crate::adapter::{
    events::TauriProgress,
    startup_scan::{run_startup_scan, StartupScanState},
};

#[tauri::command]
pub async fn scan_folder(path: String, app: tauri::AppHandle) -> Result<ScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        scan::scan_folder_service(path, &state, &TauriProgress(&app))
    })
    .await
    .map_err(|e| format!("scan worker failed: {e}"))?
}

#[tauri::command]
pub async fn list_scan_roots(app: tauri::AppHandle) -> Result<Vec<ScanRoot>, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        scan::list_scan_roots_service(&state)
    })
    .await
    .map_err(|e| format!("list_scan_roots worker failed: {e}"))?
}

#[tauri::command]
pub async fn add_scan_root(path: String, app: tauri::AppHandle) -> Result<(), String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        scan::add_scan_root_service(path, &state)
    })
    .await
    .map_err(|e| format!("add_scan_root worker failed: {e}"))?
}

#[tauri::command]
pub async fn remove_scan_root(
    path: String,
    app: tauri::AppHandle,
) -> Result<RemoveRootSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        scan::remove_scan_root_service(path, &state)
    })
    .await
    .map_err(|e| format!("remove_scan_root worker failed: {e}"))?
}

#[tauri::command]
pub async fn rescan_all_roots(app: tauri::AppHandle) -> Result<ScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        scan::rescan_all_roots_service(&state, &TauriProgress(&app))
    })
    .await
    .map_err(|e| format!("rescan worker failed: {e}"))?
}

#[tauri::command]
pub async fn set_scan_root_auto_scan(
    path: String,
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        scan::set_scan_root_auto_scan_service(path, enabled, &state)
    })
    .await
    .map_err(|e| format!("set_scan_root_auto_scan worker failed: {e}"))?
}

#[tauri::command]
pub async fn scan_startup_roots(app: tauri::AppHandle) -> Result<Option<ScanSummary>, String> {
    let startup = app.state::<StartupScanState>().inner().clone();
    run_startup_scan(&startup, move |roots| {
        let state = app.state::<AppState>();
        scan::scan_startup_roots_service(&roots, &state, &TauriProgress(&app))
    })
    .await
}
