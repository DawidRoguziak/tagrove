use tauri::Manager;

use media_tagger_core::{
    app::state::AppState,
    models::{
        ClearLibrarySummary, CsvExportSummary, CsvImportSummary, DbBundleExportSummary,
        DbBundleImportSummary, DbBundleInspection, DbRootMapping,
    },
    usecases::import_export,
};

use crate::adapter::events::TauriProgress;

#[tauri::command]
pub async fn export_tags_csv(
    path: String,
    app: tauri::AppHandle,
) -> Result<CsvExportSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        import_export::export_tags_csv_service(path, &state)
    })
    .await
    .map_err(|e| format!("export_tags_csv worker failed: {e}"))?
}

#[tauri::command]
pub async fn import_tags_csv(
    path: String,
    app: tauri::AppHandle,
) -> Result<CsvImportSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        import_export::import_tags_csv_service(path, &state)
    })
    .await
    .map_err(|e| format!("import_tags_csv worker failed: {e}"))?
}

#[tauri::command]
pub async fn clear_library_data(app: tauri::AppHandle) -> Result<ClearLibrarySummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        import_export::clear_library_data_service(&state, &TauriProgress(&app))
    })
    .await
    .map_err(|e| format!("clear_library_data worker failed: {e}"))?
}

#[tauri::command]
pub async fn export_db_bundle(
    path: String,
    app: tauri::AppHandle,
) -> Result<DbBundleExportSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        import_export::export_db_bundle_service(path, &state)
    })
    .await
    .map_err(|e| format!("export_db_bundle worker failed: {e}"))?
}

#[tauri::command]
pub async fn import_db_bundle(
    path: String,
    root_mappings: Vec<DbRootMapping>,
    app: tauri::AppHandle,
) -> Result<DbBundleImportSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        import_export::import_db_bundle_service(path, root_mappings, &state)
    })
    .await
    .map_err(|e| format!("import_db_bundle worker failed: {e}"))?
}

#[tauri::command]
pub async fn inspect_db_bundle(
    path: String,
    app: tauri::AppHandle,
) -> Result<DbBundleInspection, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        import_export::inspect_db_bundle_service(path, &state)
    })
    .await
    .map_err(|e| format!("inspect_db_bundle worker failed: {e}"))?
}
