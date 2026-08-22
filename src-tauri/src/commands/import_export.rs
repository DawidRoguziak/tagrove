use std::{
    fs,
    path::{Path, PathBuf},
};

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
    services::{backup_service, progress::emit_progress, thumb_service},
    utils::tags::{merge_tags, normalize_tags, parse_csv_tags},
};

fn parse_csv_favorite(raw: &str) -> Option<bool> {
    let normalized = raw.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }

    match normalized.as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn parse_csv_group_order(raw: &str) -> Option<Option<f64>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(None);
    }

    match trimmed.parse::<f64>() {
        Ok(value) if value.is_finite() => Some(Some(value)),
        _ => None,
    }
}

#[tauri::command(async)]
pub fn export_tags_csv(path: String, state: State<AppState>) -> Result<CsvExportSummary, String> {
    (|| {
        let target_path = PathBuf::from(path.trim());
        if target_path.as_os_str().is_empty() {
            return Err("CSV export path is empty".into());
        }

        if let Some(parent) = target_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let conn = db::open_connection(&state.db_path)?;
        let mut writer = csv::WriterBuilder::new()
            .has_headers(true)
            .from_path(&target_path)?;
        writer.write_record([
            "file_name",
            "tags",
            "favorite",
            "media_group_key",
            "media_group_order",
        ])?;

        let mut written_rows = 0usize;
        db::for_each_asset_for_csv_export(&conn, |row| {
            let file_name = Path::new(&row.path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();

            writer.write_record([
                file_name,
                row.tags.join(" "),
                if row.is_favorite {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
                row.media_group_key.unwrap_or_default(),
                row.media_group_order
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ])?;
            written_rows += 1;
            Ok(())
        })?;

        writer.flush()?;

        Ok(CsvExportSummary { rows: written_rows })
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command(async)]
pub fn import_tags_csv(path: String, state: State<AppState>) -> Result<CsvImportSummary, String> {
    (|| {
        let source_path = PathBuf::from(path.trim());
        if !source_path.exists() || !source_path.is_file() {
            return Err("CSV import path does not exist or is not a file".into());
        }

        let conn = db::open_connection(&state.db_path)?;
        // All applied rows plus the revision bump share one transaction so a
        // committed mutation can never lose its invalidation bump.
        let tx = conn.unchecked_transaction()?;
        let mut reader = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_path(&source_path)?;

        let headers = reader.headers()?.clone();
        let file_name_idx = headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case("file_name"))
            .unwrap_or(0);
        let tags_idx = headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case("tags"))
            .unwrap_or(1);
        let favorite_idx = headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case("favorite"));
        let media_group_key_idx = headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case("media_group_key"));
        let media_group_order_idx = headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case("media_group_order"));

        let mut rows_read = 0usize;
        let mut rows_applied = 0usize;
        let mut assets_matched = 0usize;
        let mut assets_updated = 0usize;
        let mut find_assets_stmt = tx.prepare(
            "SELECT id FROM assets WHERE file_name_key = ?1 ORDER BY id ASC",
        )?;

        for row in reader.records() {
            let record = row?;
            rows_read += 1;

            let file_name = record.get(file_name_idx).unwrap_or("").trim();
            if file_name.is_empty() {
                continue;
            }

            let imported_tags = normalize_tags(parse_csv_tags(record.get(tags_idx).unwrap_or("")));
            let imported_favorite =
                favorite_idx.and_then(|index| parse_csv_favorite(record.get(index).unwrap_or("")));
            let imported_media_group_key = media_group_key_idx.map(|index| {
                let value = record.get(index).unwrap_or("").trim();
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                }
            });
            let imported_media_group_order = media_group_order_idx
                .and_then(|index| parse_csv_group_order(record.get(index).unwrap_or("")));

            if imported_tags.is_empty()
                && imported_favorite.is_none()
                && imported_media_group_key.is_none()
                && imported_media_group_order.is_none()
            {
                continue;
            }

            let key = file_name.to_lowercase();
            let asset_ids = {
                let rows = find_assets_stmt.query_map(rusqlite::params![key], |row| {
                    row.get::<_, i64>(0)
                })?;
                let mut ids = Vec::new();
                for row in rows {
                    ids.push(row?);
                }
                ids
            };
            if asset_ids.is_empty() {
                continue;
            }

            rows_applied += 1;
            for asset_id in &asset_ids {
                assets_matched += 1;
                let existing = db::list_asset_tags(&tx, *asset_id)?;
                let merged = merge_tags(&existing, &imported_tags);
                let normalized_existing = normalize_tags(existing);
                let mut changed = false;

                if merged != normalized_existing {
                    db::set_asset_tags_in_tx(&tx, *asset_id, &merged)?;
                    changed = true;
                }

                if let Some(next_favorite) = imported_favorite {
                    let current_favorite = db::get_asset_favorite(&tx, *asset_id)?;
                    if current_favorite != next_favorite {
                        db::set_asset_favorite(&tx, *asset_id, next_favorite)?;
                        changed = true;
                    }
                }

                if imported_media_group_key.is_some() || imported_media_group_order.is_some() {
                    let (current_group_key, current_group_order) =
                        db::get_asset_media_group(&tx, *asset_id)?;
                    let next_group_key = imported_media_group_key
                        .clone()
                        .unwrap_or(current_group_key.clone());
                    let next_group_order =
                        imported_media_group_order.unwrap_or(current_group_order);

                    if current_group_key != next_group_key
                        || current_group_order != next_group_order
                    {
                        db::set_asset_media_group(
                            &tx,
                            *asset_id,
                            next_group_key.as_deref(),
                            next_group_order,
                        )?;
                        changed = true;
                    }
                }

                if changed {
                    assets_updated += 1;
                }
            }
        }

        drop(find_assets_stmt);

        db::cleanup_orphan_tags(&tx)?;
        if assets_updated > 0 {
            db::bump_library_revision_in_tx(&tx)?;
        }
        tx.commit()?;

        Ok(CsvImportSummary {
            rows_read,
            rows_applied,
            assets_matched,
            assets_updated,
        })
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::{parse_csv_favorite, parse_csv_group_order};

    #[test]
    fn parse_csv_favorite_accepts_common_truthy_and_falsy_values() {
        assert_eq!(parse_csv_favorite("1"), Some(true));
        assert_eq!(parse_csv_favorite(" yes "), Some(true));
        assert_eq!(parse_csv_favorite("OFF"), Some(false));
        assert_eq!(parse_csv_favorite("0"), Some(false));
    }

    #[test]
    fn parse_csv_favorite_returns_none_for_blank_or_unknown_value() {
        assert_eq!(parse_csv_favorite(""), None);
        assert_eq!(parse_csv_favorite("maybe"), None);
    }

    #[test]
    fn parse_csv_group_order_handles_empty_valid_and_invalid_numbers() {
        assert_eq!(parse_csv_group_order("   "), Some(None));
        assert_eq!(parse_csv_group_order("2.5"), Some(Some(2.5)));
        assert_eq!(parse_csv_group_order("NaN"), None);
        assert_eq!(parse_csv_group_order("abc"), None);
    }
}
