use tauri::State;

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

#[tauri::command(async)]
pub fn export_tags_csv(path: String, state: State<AppState>) -> Result<CsvExportSummary, String> {
    csv_service::export_tags_csv(&path, &state.db_path).map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn import_tags_csv(path: String, state: State<AppState>) -> Result<CsvImportSummary, String> {
    csv_service::import_tags_csv(&path, &state.db_path).map_err(|error| error.to_string())
}

#[tauri::command(async)]
pub fn clear_library_data(
    state: State<AppState>,
    app: tauri::AppHandle,
) -> Result<ClearLibrarySummary, String> {
    with_scan_and_thumb_lock(&state, || {
        let conn = db::open_connection(&state.db_path)?;
        let (removed_assets, removed_roots, thumbs) = db::clear_library_data(&conn)?;
        db::bump_library_revision(&conn)?;

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

#[tauri::command(async)]
pub fn export_db_bundle(
    path: String,
    state: State<AppState>,
) -> Result<DbBundleExportSummary, String> {
    with_database_maintenance(&state, || backup_service::export_db_bundle(path, &state))
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn import_db_bundle(
    path: String,
    root_mappings: Vec<DbRootMapping>,
    state: State<AppState>,
) -> Result<DbBundleImportSummary, String> {
    with_database_maintenance(&state, || {
        backup_service::import_db_bundle(path, root_mappings, &state)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn inspect_db_bundle(
    path: String,
    state: State<AppState>,
) -> Result<DbBundleInspection, String> {
    backup_service::inspect_db_bundle(path, &state).map_err(|e| e.to_string())
}
