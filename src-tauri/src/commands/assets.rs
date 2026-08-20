use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;

use serde::Deserialize;
use tauri::{Manager, State};

use crate::{
    app::{locks::with_scan_and_thumb_lock, state::AppState},
    db::{self, list_assets_with_meta as db_list_assets_with_meta, AssetMetaFilter},
    models::{
        AssetDetails, AssetPage, AssetQueryPageResult, BulkMediaGroupSummary, BulkTagMergeSummary,
        DeleteAssetSummary, DuplicateScanSummary, RenameAssetSummary, SetAssetTagsSummary,
        StartAssetQueryResult, TagListPage,
    },
    services::{
        asset_query_service, db_pool, media_server::MediaServerState, progress::emit_progress,
        thumb_service,
    },
    utils::tags::normalize_tags,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum AssetMetaFilterInput {
    #[serde(rename = "hasNoTags")]
    HasNoTags {
        #[serde(rename = "tagCount")]
        tag_count: i64,
    },
    #[serde(rename = "groupName")]
    GroupName {
        #[serde(rename = "groupName")]
        group_name: String,
    },
}

#[tauri::command]
pub async fn start_asset_query(
    tags_and: Vec<String>,
    tags_not: Vec<String>,
    kind: Option<String>,
    favorites_only: bool,
    meta_filter: Option<AssetMetaFilterInput>,
    generation: u64,
    page_size: usize,
    state: State<'_, AppState>,
) -> Result<StartAssetQueryResult, String> {
    let db_path = state.db_path.clone();
    let filters = normalize_query_filters(tags_and, tags_not, kind, favorites_only, meta_filter)?;
    let _ = generation;
    tauri::async_runtime::spawn_blocking(move || {
        asset_query_service::manager().start(&db_path, filters, page_size)
    })
    .await
    .map_err(|e| format!("asset query worker failed: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_asset_query_page(
    session_id: u64,
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<AssetQueryPageResult, String> {
    let db_path = state.db_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        asset_query_service::manager().page(&db_path, session_id, offset, limit)
    })
    .await
    .map_err(|e| format!("asset page worker failed: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_asset_details(
    asset_id: i64,
    state: State<'_, AppState>,
) -> Result<Option<AssetDetails>, String> {
    let db_path = state.db_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db_pool::connection(&db_path)?;
        db::get_asset_details(&conn, asset_id).map_err(Into::into)
    })
    .await
    .map_err(|e| format!("asset details worker failed: {e}"))?
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn get_video_stream_url(
    asset_id: i64,
    media_server: State<'_, MediaServerState>,
) -> Result<String, String> {
    if asset_id <= 0 {
        return Err("Asset id must be positive".to_string());
    }
    media_server.video_url(asset_id).map_err(|error| error.to_string())
}

fn normalize_query_filters(
    tags_and: Vec<String>,
    tags_not: Vec<String>,
    kind: Option<String>,
    favorites_only: bool,
    meta_filter: Option<AssetMetaFilterInput>,
) -> Result<asset_query_service::AssetQueryFilters, String> {
    let normalized_kind = kind.and_then(|value| {
        let lowered = value.trim().to_lowercase();
        matches!(lowered.as_str(), "image" | "gif" | "video").then_some(lowered)
    });
    let normalized_meta_filter = match meta_filter {
        Some(AssetMetaFilterInput::HasNoTags { tag_count }) if tag_count >= 0 => {
            Some(AssetMetaFilter::HasNoTags { tag_count })
        }
        Some(AssetMetaFilterInput::HasNoTags { .. }) => {
            return Err("Invalid hasNoTags meta filter".to_string())
        }
        Some(AssetMetaFilterInput::GroupName { group_name }) => {
            let group_name = group_name.trim().to_string();
            if group_name.is_empty() {
                return Err("Invalid groupName meta filter".to_string());
            }
            Some(AssetMetaFilter::GroupName { group_name })
        }
        None => None,
    };
    Ok(asset_query_service::AssetQueryFilters {
        tags_and: normalize_tags(tags_and),
        tags_not: normalize_tags(tags_not),
        kind: normalized_kind,
        favorites_only,
        meta_filter: normalized_meta_filter,
    })
}

#[tauri::command]
pub fn list_assets(
    offset: i64,
    limit: i64,
    tags_and: Vec<String>,
    tags_not: Vec<String>,
    kind: Option<String>,
    favorites_only: bool,
    meta_filter: Option<AssetMetaFilterInput>,
    state: State<AppState>,
) -> Result<AssetPage, String> {
    (|| {
        let conn = db::open_connection(&state.db_path)?;
        let normalized_tags = normalize_tags(tags_and);
        let normalized_tags_not = normalize_tags(tags_not);
        let normalized_kind = kind.and_then(|value| {
            let lowered = value.trim().to_lowercase();
            match lowered.as_str() {
                "image" | "gif" | "video" => Some(lowered),
                _ => None,
            }
        });
        let normalized_meta_filter = match meta_filter {
            Some(AssetMetaFilterInput::HasNoTags { tag_count }) => {
                if tag_count < 0 {
                    return Err("Invalid hasNoTags meta filter".into());
                }
                Some(AssetMetaFilter::HasNoTags { tag_count })
            }
            Some(AssetMetaFilterInput::GroupName { group_name }) => {
                let trimmed = group_name.trim();
                if trimmed.is_empty() {
                    return Err("Invalid groupName meta filter".into());
                }
                Some(AssetMetaFilter::GroupName {
                    group_name: trimmed.to_string(),
                })
            }
            None => None,
        };

        db_list_assets_with_meta(
            &conn,
            offset.max(0),
            limit.clamp(1, 500),
            &normalized_tags,
            &normalized_tags_not,
            normalized_kind.as_deref(),
            favorites_only,
            normalized_meta_filter.as_ref(),
        )
        .map_err(Into::into)
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn set_asset_tags(
    asset_id: i64,
    tags: Vec<String>,
    state: State<AppState>,
) -> Result<SetAssetTagsSummary, String> {
    (|| {
        let mut conn = db::open_connection(&state.db_path)?;
        db::set_asset_tags_with_revision(&mut conn, asset_id, &tags).map_err(Into::into)
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn merge_asset_tags_bulk(
    asset_ids: Vec<i64>,
    tags: Vec<String>,
    state: State<AppState>,
) -> Result<BulkTagMergeSummary, String> {
    (|| {
        let mut conn = db::open_connection(&state.db_path)?;
        let mut seen = HashSet::new();
        let normalized_asset_ids = asset_ids
            .into_iter()
            .filter(|asset_id| *asset_id > 0)
            .filter(|asset_id| seen.insert(*asset_id))
            .collect::<Vec<_>>();
        let normalized_tags = normalize_tags(tags);

        let (results, revision) = db::merge_asset_tags_bulk_with_revision(
            &mut conn,
            &normalized_asset_ids,
            &normalized_tags,
        )?;
        let processed_asset_ids = results
            .iter()
            .map(|result| result.asset_id)
            .collect::<Vec<_>>();
        let updated_asset_ids = results
            .iter()
            .filter(|result| result.changed)
            .map(|result| result.asset_id)
            .collect::<Vec<_>>();

        Ok(BulkTagMergeSummary {
            processed_assets: processed_asset_ids.len(),
            updated_assets: updated_asset_ids.len(),
            processed_asset_ids,
            updated_asset_ids,
            results,
            revision,
        })
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn set_asset_favorite(
    asset_id: i64,
    is_favorite: bool,
    state: State<AppState>,
) -> Result<(), String> {
    (|| {
        let conn = db::open_connection(&state.db_path)?;
        db::set_asset_favorite(&conn, asset_id, is_favorite)?;
        db::bump_library_revision(&conn)?;
        Ok(())
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn set_asset_media_group(
    asset_id: i64,
    media_group_key: Option<String>,
    media_group_order: Option<f64>,
    state: State<AppState>,
) -> Result<(), String> {
    (|| {
        let conn = db::open_connection(&state.db_path)?;
        let normalized_key = media_group_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let normalized_order = media_group_order.filter(|value| value.is_finite());
        db::set_asset_media_group(&conn, asset_id, normalized_key.as_deref(), normalized_order)?;
        db::bump_library_revision(&conn)?;
        Ok(())
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkMediaGroupUpdateInput {
    pub asset_id: i64,
    pub media_group_order: Option<f64>,
}

#[tauri::command]
pub fn set_assets_media_group_bulk(
    updates: Vec<BulkMediaGroupUpdateInput>,
    media_group_key: Option<String>,
    state: State<AppState>,
) -> Result<BulkMediaGroupSummary, String> {
    (|| {
        let conn = db::open_connection(&state.db_path)?;
        let normalized_key = media_group_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let mut seen = HashSet::new();
        let normalized_updates = updates
            .into_iter()
            .filter(|update| update.asset_id > 0)
            .filter(|update| seen.insert(update.asset_id))
            .map(|update| {
                (
                    update.asset_id,
                    update.media_group_order.filter(|value| value.is_finite()),
                )
            })
            .collect::<Vec<_>>();

        let (processed_assets, updated_assets) =
            db::set_assets_media_group_bulk(&conn, &normalized_updates, normalized_key.as_deref())?;
        if updated_assets > 0 {
            db::bump_library_revision(&conn)?;
        }

        Ok(BulkMediaGroupSummary {
            processed_assets,
            updated_assets,
            media_group_key: normalized_key,
        })
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn list_tags(
    query: String,
    offset: i64,
    limit: i64,
    state: State<AppState>,
) -> Result<TagListPage, String> {
    (|| {
        let conn = db::open_connection(&state.db_path)?;
        db::list_tags_page(&conn, &query, offset, limit).map_err(Into::into)
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn delete_asset(asset_id: i64, state: State<AppState>) -> Result<DeleteAssetSummary, String> {
    with_scan_and_thumb_lock(&state, || {
        let conn = db::open_connection(&state.db_path)?;
        let Some((asset_path, thumb_path)) = db::delete_asset_by_id_with_thumb(&conn, asset_id)?
        else {
            return Err(format!("Asset with id {asset_id} not found").into());
        };
        db::bump_library_revision(&conn)?;

        let media_path = PathBuf::from(&asset_path);
        let removed_media_file = if media_path.exists() {
            fs::remove_file(&media_path).is_ok()
        } else {
            false
        };

        let removed_thumbnails =
            thumb_service::delete_thumbnail_files(thumb_path.into_iter().collect());

        Ok(DeleteAssetSummary {
            removed_assets: 1,
            removed_thumbnails,
            removed_media_file,
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn find_duplicate_assets(app: tauri::AppHandle) -> Result<DuplicateScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        (|| {
            let conn = db::open_connection(&state.db_path)?;
            let groups = db::list_duplicate_groups(&conn)?;
            let total = groups.len();

            let _ = emit_progress(
                &app,
                "duplicates-scan",
                0,
                total,
                format!("Searching duplicates for {total} file names"),
            );

            let duplicate_assets = groups.iter().map(|group| group.assets.len()).sum::<usize>();

            let duplicate_groups = groups.len();
            let _ = emit_progress(
                &app,
                "duplicates-scan-done",
                total,
                total,
                format!(
                "Duplicate scan complete. Groups: {duplicate_groups}, assets: {duplicate_assets}"
            ),
            );

            Ok(DuplicateScanSummary {
                groups,
                duplicate_groups,
                duplicate_assets,
            })
        })()
        .map_err(|e: crate::error::AppError| e.to_string())
    })
    .await
    .map_err(|e| format!("duplicate query worker failed: {e}"))?
}

#[tauri::command]
pub fn rename_asset_file(
    asset_id: i64,
    new_file_name: String,
    state: State<AppState>,
) -> Result<RenameAssetSummary, String> {
    with_scan_and_thumb_lock(&state, || {
        let normalized_file_name = normalize_new_file_name(&new_file_name)?;
        let conn = db::open_connection(&state.db_path)?;

        let Some((old_path, _old_thumb_path)) =
            db::get_asset_path_and_thumb_by_id(&conn, asset_id)?
        else {
            return Err(format!("Asset with id {asset_id} not found").into());
        };

        let old_path_buf = PathBuf::from(&old_path);
        let parent = old_path_buf
            .parent()
            .ok_or_else(|| "Cannot resolve parent directory for asset".to_string())?;
        let new_path_buf = parent.join(&normalized_file_name);
        let new_path = new_path_buf.to_string_lossy().to_string();

        if old_path.eq_ignore_ascii_case(&new_path) {
            return Err("New file name is the same as current one".into());
        }

        if new_path_buf.exists() {
            return Err("Target file already exists".into());
        }

        fs::rename(&old_path_buf, &new_path_buf).with_context(|| {
            format!(
                "cannot rename media file from '{}' to '{}'",
                old_path_buf.display(),
                new_path_buf.display()
            )
        })?;

        let update_result =
            db::rename_asset_file_by_id(&conn, asset_id, &new_path, &normalized_file_name);

        let removed_thumbnails = match update_result {
            Ok(Some((_previous_path, thumb_path))) => {
                thumb_service::delete_thumbnail_files(thumb_path.into_iter().collect())
            }
            Ok(None) => {
                let _ = fs::rename(&new_path_buf, &old_path_buf);
                return Err(format!("Asset with id {asset_id} not found").into());
            }
            Err(error) => {
                let _ = fs::rename(&new_path_buf, &old_path_buf);
                return Err(error.into());
            }
        };
        db::bump_library_revision(&conn)?;

        Ok(RenameAssetSummary {
            asset_id,
            old_path,
            new_path,
            removed_thumbnails,
        })
    })
    .map_err(|e| e.to_string())
}

fn normalize_new_file_name(raw: &str) -> Result<String, crate::error::AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("File name cannot be empty".into());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("File name is invalid".into());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("File name cannot contain directory separators".into());
    }
    if trimmed.contains(':')
        || trimmed.contains('*')
        || trimmed.contains('?')
        || trimmed.contains('"')
    {
        return Err("File name contains invalid characters".into());
    }
    if trimmed.contains('<') || trimmed.contains('>') || trimmed.contains('|') {
        return Err("File name contains invalid characters".into());
    }

    let path = Path::new(trimmed);
    let Some(file_name) = path.file_name() else {
        return Err("File name is invalid".into());
    };

    Ok(file_name.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::normalize_new_file_name;

    #[test]
    fn normalize_new_file_name_trims_valid_input() {
        let normalized = normalize_new_file_name("  renamed.jpg  ").expect("must normalize");
        assert_eq!(normalized, "renamed.jpg");
    }

    #[test]
    fn normalize_new_file_name_rejects_empty_or_dot_names() {
        assert!(normalize_new_file_name("   ")
            .expect_err("empty")
            .to_string()
            .contains("empty"));
        assert!(normalize_new_file_name(".")
            .expect_err("dot")
            .to_string()
            .contains("invalid"));
        assert!(normalize_new_file_name("..")
            .expect_err("dot dot")
            .to_string()
            .contains("invalid"));
    }

    #[test]
    fn normalize_new_file_name_rejects_directory_separators() {
        assert!(normalize_new_file_name("nested/name.jpg")
            .expect_err("slash")
            .to_string()
            .contains("directory separators"));
        assert!(normalize_new_file_name("nested\\name.jpg")
            .expect_err("backslash")
            .to_string()
            .contains("directory separators"));
    }

    #[test]
    fn normalize_new_file_name_rejects_windows_reserved_chars() {
        assert!(normalize_new_file_name("bad:name.jpg")
            .expect_err("colon")
            .to_string()
            .contains("invalid characters"));
        assert!(normalize_new_file_name("bad|name.jpg")
            .expect_err("pipe")
            .to_string()
            .contains("invalid characters"));
    }
}
