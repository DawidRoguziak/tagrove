use std::collections::HashSet;

use serde::Deserialize;
use tauri::{Manager, State};

use crate::{
    app::{locks::with_scan_and_thumb_lock, state::AppState},
    db::{self, list_assets_with_meta as db_list_assets_with_meta, AssetMetaFilter},
    models::{
        AssetDetails, AssetPage, AssetQueryPageResult, AssetSummary, BulkMediaGroupSummary,
        BulkTagMergeSummary, DeleteAssetSummary, DuplicateResolutionBatchInput,
        DuplicateResolutionBatchSummary, DuplicateScanSummary, RenameAssetSummary,
        SetAssetTagsSummary, StartAssetQueryResult, TagListPage,
    },
    services::{asset_mutation_service, asset_query_service, progress::emit_progress},
    utils::tags::normalize_and_validate_tags,
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
#[allow(clippy::too_many_arguments)]
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
    let database = state.database.clone();
    let filters = normalize_query_filters(tags_and, tags_not, kind, favorites_only, meta_filter)?;
    // Register arrival order before scheduling blocking work so a slower
    // scheduler cannot invert supersession between two requests.
    let request_id = database.queries.begin_request(generation);
    tauri::async_runtime::spawn_blocking(move || {
        database.queries.start(
            &database.admit()?,
            filters,
            page_size,
            request_id,
            generation,
        )
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
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        database
            .queries
            .page(&database.admit()?, session_id, offset, limit)
    })
    .await
    .map_err(|e| format!("asset page worker failed: {e}"))?
    .map_err(|e| e.to_string())
}

fn validate_summary_batch(asset_ids: &[i64]) -> Result<(), String> {
    if asset_ids.len() > 256 {
        return Err("At most 256 asset IDs are allowed".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn get_asset_summaries_by_ids(
    asset_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<AssetSummary>, String> {
    validate_summary_batch(&asset_ids)?;
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = database.admit()?.connection()?;
        db::list_asset_summaries_by_ids(&conn, &asset_ids).map_err(Into::into)
    })
    .await
    .map_err(|e| format!("asset summaries worker failed: {e}"))?
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn get_asset_details(
    asset_id: i64,
    state: State<'_, AppState>,
) -> Result<Option<AssetDetails>, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = database.admit()?.connection()?;
        db::get_asset_details(&conn, asset_id).map_err(Into::into)
    })
    .await
    .map_err(|e| format!("asset details worker failed: {e}"))?
    .map_err(|e: crate::error::AppError| e.to_string())
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
        tags_and: normalize_and_validate_tags(tags_and).map_err(|error| error.to_string())?,
        tags_not: normalize_and_validate_tags(tags_not).map_err(|error| error.to_string())?,
        kind: normalized_kind,
        favorites_only,
        meta_filter: normalized_meta_filter,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn list_assets(
    offset: i64,
    limit: i64,
    tags_and: Vec<String>,
    tags_not: Vec<String>,
    kind: Option<String>,
    favorites_only: bool,
    meta_filter: Option<AssetMetaFilterInput>,
    app: tauri::AppHandle,
) -> Result<AssetPage, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        list_assets_service(
            offset,
            limit,
            tags_and,
            tags_not,
            kind,
            favorites_only,
            meta_filter,
            &state,
        )
    })
    .await
    .map_err(|e| format!("list_assets worker failed: {e}"))?
}

#[allow(clippy::too_many_arguments)]
fn list_assets_service(
    offset: i64,
    limit: i64,
    tags_and: Vec<String>,
    tags_not: Vec<String>,
    kind: Option<String>,
    favorites_only: bool,
    meta_filter: Option<AssetMetaFilterInput>,
    state: &AppState,
) -> Result<AssetPage, String> {
    (|| {
        let conn = state.database.admit()?.connection()?;
        let normalized_tags = normalize_and_validate_tags(tags_and)?;
        let normalized_tags_not = normalize_and_validate_tags(tags_not)?;
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
pub async fn set_asset_tags(
    asset_id: i64,
    tags: Vec<String>,
    app: tauri::AppHandle,
) -> Result<SetAssetTagsSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        set_asset_tags_service(asset_id, tags, &state)
    })
    .await
    .map_err(|e| format!("set_asset_tags worker failed: {e}"))?
}

fn set_asset_tags_service(
    asset_id: i64,
    tags: Vec<String>,
    state: &AppState,
) -> Result<SetAssetTagsSummary, String> {
    (|| {
        let mut conn = state.database.admit()?.connection()?;
        let tags = normalize_and_validate_tags(tags)?;
        db::set_asset_tags_with_revision(&mut conn, asset_id, &tags).map_err(Into::into)
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn merge_asset_tags_bulk(
    asset_ids: Vec<i64>,
    tags: Vec<String>,
    app: tauri::AppHandle,
) -> Result<BulkTagMergeSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        merge_asset_tags_bulk_service(asset_ids, tags, &state)
    })
    .await
    .map_err(|e| format!("merge_asset_tags_bulk worker failed: {e}"))?
}

fn merge_asset_tags_bulk_service(
    asset_ids: Vec<i64>,
    tags: Vec<String>,
    state: &AppState,
) -> Result<BulkTagMergeSummary, String> {
    (|| {
        let mut conn = state.database.admit()?.connection()?;
        let mut seen = HashSet::new();
        let normalized_asset_ids = asset_ids
            .into_iter()
            .filter(|asset_id| *asset_id > 0)
            .filter(|asset_id| seen.insert(*asset_id))
            .collect::<Vec<_>>();
        let normalized_tags = normalize_and_validate_tags(tags)?;

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
pub async fn toggle_assets_favorite_bulk(
    asset_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<crate::models::BulkFavoriteSummary, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = database.admit()?.connection()?;
        db::toggle_assets_favorite_bulk(&mut conn, &asset_ids).map_err(crate::error::AppError::from)
    })
    .await
    .map_err(|e| format!("bulk favorite worker failed: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_asset_favorite(
    asset_id: i64,
    is_favorite: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        set_asset_favorite_service(asset_id, is_favorite, &state)
    })
    .await
    .map_err(|e| format!("set_asset_favorite worker failed: {e}"))?
}

fn set_asset_favorite_service(
    asset_id: i64,
    is_favorite: bool,
    state: &AppState,
) -> Result<(), String> {
    (|| {
        let mut conn = state.database.admit()?.connection()?;
        db::set_asset_favorite_with_revision(&mut conn, asset_id, is_favorite)?;
        Ok(())
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn set_asset_media_group(
    asset_id: i64,
    media_group_key: Option<String>,
    media_group_order: Option<f64>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        set_asset_media_group_service(asset_id, media_group_key, media_group_order, &state)
    })
    .await
    .map_err(|e| format!("set_asset_media_group worker failed: {e}"))?
}

fn set_asset_media_group_service(
    asset_id: i64,
    media_group_key: Option<String>,
    media_group_order: Option<f64>,
    state: &AppState,
) -> Result<(), String> {
    (|| {
        let mut conn = state.database.admit()?.connection()?;
        let normalized_key = media_group_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let normalized_order = media_group_order.filter(|value| value.is_finite());
        db::set_asset_media_group_with_revision(
            &mut conn,
            asset_id,
            normalized_key.as_deref(),
            normalized_order,
        )?;
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
pub async fn set_assets_media_group_bulk(
    updates: Vec<BulkMediaGroupUpdateInput>,
    media_group_key: Option<String>,
    app: tauri::AppHandle,
) -> Result<BulkMediaGroupSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        set_assets_media_group_bulk_service(updates, media_group_key, &state)
    })
    .await
    .map_err(|e| format!("set_assets_media_group_bulk worker failed: {e}"))?
}

fn set_assets_media_group_bulk_service(
    updates: Vec<BulkMediaGroupUpdateInput>,
    media_group_key: Option<String>,
    state: &AppState,
) -> Result<BulkMediaGroupSummary, String> {
    (|| {
        let mut conn = state.database.admit()?.connection()?;
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

        let (processed_asset_ids, updated_assets) = db::set_assets_media_group_bulk(
            &mut conn,
            &normalized_updates,
            normalized_key.as_deref(),
        )?;

        Ok(BulkMediaGroupSummary {
            processed_assets: processed_asset_ids.len(),
            processed_asset_ids,
            updated_assets,
            media_group_key: normalized_key,
        })
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn list_tags(
    query: String,
    offset: i64,
    limit: i64,
    app: tauri::AppHandle,
) -> Result<TagListPage, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        list_tags_service(query, offset, limit, &state)
    })
    .await
    .map_err(|e| format!("list_tags worker failed: {e}"))?
}

fn list_tags_service(
    query: String,
    offset: i64,
    limit: i64,
    state: &AppState,
) -> Result<TagListPage, String> {
    (|| {
        let conn = state.database.admit()?.connection()?;
        db::list_tags_page(&conn, &query, offset, limit).map_err(Into::into)
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn delete_asset(
    asset_id: i64,
    app: tauri::AppHandle,
) -> Result<DeleteAssetSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        delete_asset_service(asset_id, &state)
    })
    .await
    .map_err(|e| format!("delete_asset worker failed: {e}"))?
}

fn delete_asset_service(asset_id: i64, state: &AppState) -> Result<DeleteAssetSummary, String> {
    with_scan_and_thumb_lock(state, |permit| {
        let conn = permit.durable_connection()?;
        Ok(asset_mutation_service::delete_asset(
            &conn,
            &state.thumbs_dir,
            asset_id,
        )?)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn find_duplicate_assets(app: tauri::AppHandle) -> Result<DuplicateScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        (|| {
            let conn = state.database.admit()?.connection()?;
            let groups = db::list_duplicate_groups(&conn)?;
            let revision = db::current_library_revision(&conn)?;
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
                revision,
            })
        })()
        .map_err(|e: crate::error::AppError| e.to_string())
    })
    .await
    .map_err(|e| format!("duplicate query worker failed: {e}"))?
}

#[tauri::command]
pub async fn rename_asset_file(
    asset_id: i64,
    new_file_name: String,
    app: tauri::AppHandle,
) -> Result<RenameAssetSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        rename_asset_file_service(asset_id, new_file_name, &state)
    })
    .await
    .map_err(|e| format!("rename_asset_file worker failed: {e}"))?
}

fn rename_asset_file_service(
    asset_id: i64,
    new_file_name: String,
    state: &AppState,
) -> Result<RenameAssetSummary, String> {
    with_scan_and_thumb_lock(state, |permit| {
        let conn = permit.durable_connection()?;
        Ok(asset_mutation_service::rename_asset(
            &conn,
            &state.thumbs_dir,
            asset_id,
            new_file_name,
        )?)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_duplicate_resolution_batch(
    input: DuplicateResolutionBatchInput,
    app: tauri::AppHandle,
) -> Result<DuplicateResolutionBatchSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        apply_duplicate_resolution_batch_service(input, &state)
    })
    .await
    .map_err(|e| format!("apply_duplicate_resolution_batch worker failed: {e}"))?
}

fn apply_duplicate_resolution_batch_service(
    input: DuplicateResolutionBatchInput,
    state: &AppState,
) -> Result<DuplicateResolutionBatchSummary, String> {
    with_scan_and_thumb_lock(state, |permit| {
        let conn = permit.durable_connection()?;
        Ok(asset_mutation_service::apply_duplicate_resolution_batch(
            &conn,
            &state.thumbs_dir,
            input,
        )?)
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
fn normalize_new_file_name(raw: &str) -> Result<String, crate::error::AppError> {
    asset_mutation_service::validate_file_name(raw).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::normalize_new_file_name;

    #[test]
    fn selection_summary_batch_limit_and_group_response_contract() {
        assert!(super::validate_summary_batch(&[]).is_ok());
        assert!(super::validate_summary_batch(&[1; 256]).is_ok());
        assert!(super::validate_summary_batch(&[1; 257]).is_err());
        let response = crate::models::BulkMediaGroupSummary {
            processed_asset_ids: vec![3, 1],
            processed_assets: 2,
            updated_assets: 1,
            media_group_key: Some("trip".into()),
        };
        let json = serde_json::to_value(response).expect("serialize");
        assert_eq!(json["processed_asset_ids"], serde_json::json!([3, 1]));
        assert_eq!(json["processed_assets"], 2);
    }

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
