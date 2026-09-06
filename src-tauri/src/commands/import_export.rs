use tauri::Manager;

use crate::{
    app::{
        locks::{with_database_maintenance, with_scan_and_thumb_lock},
        state::AppState,
    },
    db,
    models::{
        ClearLibrarySummary, CsvExportSummary, CsvImportSummary, DbBundleExportSummary,
        DbBundleImportSummary, DbBundleInspection, DbRootMapping,
    },
    services::{backup_service, csv_service, progress::emit_progress, thumb_service},
};

#[tauri::command]
pub async fn export_tags_csv(
    path: String,
    app: tauri::AppHandle,
) -> Result<CsvExportSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        export_tags_csv_service(path, &state)
    })
    .await
    .map_err(|e| format!("export_tags_csv worker failed: {e}"))?
}

fn export_tags_csv_service(path: String, state: &AppState) -> Result<CsvExportSummary, String> {
    csv_service::export_tags_csv(
        &path,
        &state.db_path,
        &state.database.admit().map_err(|e| e.to_string())?,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn import_tags_csv(
    path: String,
    app: tauri::AppHandle,
) -> Result<CsvImportSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        import_tags_csv_service(path, &state)
    })
    .await
    .map_err(|e| format!("import_tags_csv worker failed: {e}"))?
}

fn import_tags_csv_service(path: String, state: &AppState) -> Result<CsvImportSummary, String> {
    csv_service::import_tags_csv(&path, &state.database.admit().map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn clear_library_data(app: tauri::AppHandle) -> Result<ClearLibrarySummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        clear_library_data_service(&state, app)
    })
    .await
    .map_err(|e| format!("clear_library_data worker failed: {e}"))?
}

fn clear_library_data_service(
    state: &AppState,
    app: tauri::AppHandle,
) -> Result<ClearLibrarySummary, String> {
    with_scan_and_thumb_lock(state, |permit| {
        let conn = permit.connection()?;
        let (removed_assets, removed_roots, thumbs) = db::clear_library_data(&conn)?;

        let total = thumbs.len();
        let _ = emit_progress(
            &app,
            "library-clear",
            0,
            total,
            "Clearing thumbnails and indexed library data".to_string(),
        );

        let removed_thumbnails = thumb_service::delete_thumbnail_files_with_progress(
            &app,
            &state.thumbs_dir,
            thumbs,
            total,
            "library-clear",
        );

        let _ = emit_progress(
            &app,
            "library-clear-done",
            total,
            total,
            format!(
                "Library cleared. Assets: {removed_assets}, roots: {removed_roots}, thumbnails: {removed_thumbnails}"
            ),
        );

        Ok(ClearLibrarySummary {
            removed_assets,
            removed_roots,
            removed_thumbnails,
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_db_bundle(
    path: String,
    app: tauri::AppHandle,
) -> Result<DbBundleExportSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        export_db_bundle_service(path, &state)
    })
    .await
    .map_err(|e| format!("export_db_bundle worker failed: {e}"))?
}

fn export_db_bundle_service(
    path: String,
    state: &AppState,
) -> Result<DbBundleExportSummary, String> {
    with_database_maintenance(state, |maintenance| {
        backup_service::export_db_bundle(path, state, maintenance)
    })
    .map_err(|e| e.to_string())
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
        import_db_bundle_service(path, root_mappings, &state)
    })
    .await
    .map_err(|e| format!("import_db_bundle worker failed: {e}"))?
}

fn import_db_bundle_service(
    path: String,
    root_mappings: Vec<DbRootMapping>,
    state: &AppState,
) -> Result<DbBundleImportSummary, String> {
    with_database_maintenance(state, |maintenance| {
        backup_service::import_db_bundle(path, root_mappings, state, maintenance)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn inspect_db_bundle(
    path: String,
    app: tauri::AppHandle,
) -> Result<DbBundleInspection, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        inspect_db_bundle_service(path, &state)
    })
    .await
    .map_err(|e| format!("inspect_db_bundle worker failed: {e}"))?
}

fn inspect_db_bundle_service(path: String, state: &AppState) -> Result<DbBundleInspection, String> {
    backup_service::inspect_db_bundle(path, state).map_err(|e| e.to_string())
}
