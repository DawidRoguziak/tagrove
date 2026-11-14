//! Library transfer and maintenance use cases: CSV tags, clearing the
//! library and database bundles. Called by `commands/import_export.rs`.

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
    services::{
        backup_service, csv_service,
        progress::{emit_progress, ProgressSink},
        thumb_service,
    },
};

pub fn export_tags_csv_service(path: String, state: &AppState) -> Result<CsvExportSummary, String> {
    csv_service::export_tags_csv(
        &path,
        &state.db_path,
        &state.database.admit().map_err(|e| e.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub fn import_tags_csv_service(path: String, state: &AppState) -> Result<CsvImportSummary, String> {
    csv_service::import_tags_csv(&path, &state.database.admit().map_err(|e| e.to_string())?)
        .map_err(|error| error.to_string())
}

pub fn clear_library_data_service(
    state: &AppState,
    progress: &impl ProgressSink,
) -> Result<ClearLibrarySummary, String> {
    with_scan_and_thumb_lock(state, |permit| {
        let conn = permit.connection()?;
        let (removed_assets, removed_roots, thumbs) = db::clear_library_data(&conn)?;

        let total = thumbs.len();
        let _ = emit_progress(
            progress,
            "library-clear",
            0,
            total,
            "Clearing thumbnails and indexed library data".to_string(),
        );

        let removed_thumbnails = thumb_service::delete_thumbnail_files_with_progress(
            progress,
            &state.thumbs_dir,
            thumbs,
            total,
            "library-clear",
        );

        let _ = emit_progress(
            progress,
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

pub fn export_db_bundle_service(
    path: String,
    state: &AppState,
) -> Result<DbBundleExportSummary, String> {
    with_database_maintenance(state, |maintenance| {
        backup_service::export_db_bundle(path, state, maintenance)
    })
    .map_err(|e| e.to_string())
}

pub fn import_db_bundle_service(
    path: String,
    root_mappings: Vec<DbRootMapping>,
    state: &AppState,
) -> Result<DbBundleImportSummary, String> {
    with_database_maintenance(state, |maintenance| {
        backup_service::import_db_bundle(path, root_mappings, state, maintenance)
    })
    .map_err(|e| e.to_string())
}

pub fn inspect_db_bundle_service(
    path: String,
    state: &AppState,
) -> Result<DbBundleInspection, String> {
    backup_service::inspect_db_bundle(path, state).map_err(|e| e.to_string())
}
