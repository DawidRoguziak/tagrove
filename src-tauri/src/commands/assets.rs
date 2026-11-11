use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use serde::Deserialize;
use tauri::{Manager, State};

use crate::{
    app::{
        locks::{try_with_scan_and_thumb_lock, with_scan_and_thumb_lock},
        state::AppState,
    },
    db::{self, AssetMetaFilter},
    models::{
        AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetSummary,
        BulkMediaGroupSummary, BulkTagMergeSummary, DeleteAssetSummary,
        DuplicateResolutionBatchInput, DuplicateResolutionBatchSummary, DuplicateScanSummary,
        RenameAssetSummary, SetAssetTagsSummary, StartAssetQueryResult, TagListPage,
    },
    services::{asset_mutation_service, asset_query_service, progress::emit_progress},
    utils::tags::normalize_and_validate_tags,
};

#[tauri::command]
pub fn get_startup_popular_tags(
    state: State<'_, crate::app::state::StartupPopularTags>,
) -> Vec<String> {
    state.tags().to_vec()
}

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
    owner_id: String,
    generation: u64,
    page_size: usize,
    anchor_offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<StartAssetQueryResult, String> {
    let database = state.database.clone();
    let filters = normalize_query_filters(tags_and, tags_not, kind, favorites_only, meta_filter)?;
    if owner_id.is_empty() || owner_id.len() > 128 {
        return Err("Invalid query owner ID".into());
    }
    // Async command admission may be reordered. Compare generations within
    // the frontend owner before assigning the process-wide request token.
    let request_id = database.queries.begin_request(&owner_id, generation);
    tauri::async_runtime::spawn_blocking(move || {
        database.queries.start_at(
            &database.admit()?,
            filters,
            page_size,
            request_id,
            generation,
            anchor_offset,
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

#[tauri::command]
pub async fn get_asset_query_position(
    session_id: u64,
    asset_id: i64,
    state: State<'_, AppState>,
) -> Result<AssetQueryPositionResult, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        database
            .queries
            .position(&database.admit()?, session_id, asset_id)
    })
    .await
    .map_err(|e| format!("asset position worker failed: {e}"))?
    .map_err(|e| e.to_string())
}

/// Ordered session IDs for `start..=end` as raw bytes (see
/// `asset_query_service::id_range`): little-endian f64 `[count, ids..]`, or an
/// empty body when the session is stale.
#[tauri::command]
pub async fn get_asset_query_id_range(
    session_id: u64,
    start: usize,
    end: usize,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    let database = state.database.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        database
            .queries
            .id_range(&database.admit()?, session_id, start, end)
    })
    .await
    .map_err(|e| format!("asset ID range worker failed: {e}"))?
    .map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes.unwrap_or_default()))
}

/// Timeline sections of a query session (`asset_query_service::sections`).
#[tauri::command]
pub async fn get_asset_query_sections(
    session_id: u64,
    state: State<'_, AppState>,
) -> Result<crate::models::AssetQuerySectionsResult, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        database.queries.sections(&database.admit()?, session_id)
    })
    .await
    .map_err(|e| format!("asset sections worker failed: {e}"))?
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
        let tags = normalize_and_validate_tags(tags)?;
        let started = Instant::now();
        let mut conn = state.database.admit()?.user_write_connection()?;
        let pool_wait = started.elapsed();
        let summary = db::set_asset_tags_with_revision(&mut conn, asset_id, &tags)?;
        log_user_write_perf("set_asset_tags", started, pool_wait);
        Ok(summary)
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
        let mut seen = HashSet::new();
        let normalized_asset_ids = asset_ids
            .into_iter()
            .filter(|asset_id| *asset_id > 0)
            .filter(|asset_id| seen.insert(*asset_id))
            .collect::<Vec<_>>();
        let normalized_tags = normalize_and_validate_tags(tags)?;
        let mut conn = state.database.admit()?.user_write_connection()?;

        let (results, revision, query_impact, vocabulary_changed) =
            db::merge_asset_tags_bulk_with_revision(
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
            vocabulary_changed,
            query_impact,
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
        let mut conn = database.admit()?.user_write_connection()?;
        db::toggle_assets_favorite_bulk(&mut conn, &asset_ids).map_err(crate::error::AppError::from)
    })
    .await
    .map_err(|e| format!("bulk favorite worker failed: {e}"))?
    .map_err(|e| e.to_string())
}

/// Bulk tag, favorite or media-group write on a selection spec (stage 09).
/// Resolved against the spec's query session; `stale` writes nothing.
#[tauri::command]
pub async fn apply_selection_mutation(
    selection: crate::services::asset_query_service::SelectionSpec,
    mutation: crate::services::selection_service::SelectionMutation,
    state: State<'_, AppState>,
) -> Result<crate::models::SelectionMutationResult, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::selection_service::apply_selection_mutation(
            &database, &selection, mutation,
        )
    })
    .await
    .map_err(|e| format!("selection mutation worker failed: {e}"))?
    .map_err(|e| e.to_string())
}

/// `MEDIATAGGER_PERF=1` timing of one user metadata write: the wait for a
/// pooled connection and the write transaction. No paths or values.
fn log_user_write_perf(name: &str, started: Instant, pool_wait: Duration) {
    if std::env::var_os("MEDIATAGGER_PERF").is_some() {
        let write = started.elapsed().saturating_sub(pool_wait);
        eprintln!(
            "[perf] name={name} pool_wait_ms={:.2} write_ms={:.2}",
            pool_wait.as_secs_f64() * 1_000.0,
            write.as_secs_f64() * 1_000.0
        );
    }
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
        let started = Instant::now();
        let mut conn = state.database.admit()?.user_write_connection()?;
        let pool_wait = started.elapsed();
        db::set_asset_favorite_with_revision(&mut conn, asset_id, is_favorite)?;
        log_user_write_perf("set_asset_favorite", started, pool_wait);
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
        let normalized_key = media_group_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let normalized_order = media_group_order.filter(|value| value.is_finite());
        let mut conn = state.database.admit()?.user_write_connection()?;
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

        let mut conn = state.database.admit()?.user_write_connection()?;
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

/// Rejection text of a delete that found a scan or another combined workflow
/// running. It is backend display text in the `i18n:<key>` format, which the
/// frontend translates; nothing branches on it.
pub(crate) const LIBRARY_BUSY_ERROR: &str = "i18n:common.libraryBusy";

/// Fails fast instead of waiting for `scan_lock`: a scan holds it for its whole
/// run, and the lightbox keeps its confirmation locked while the delete is
/// pending. Nothing is staged or written before the lock is taken.
fn delete_asset_service(asset_id: i64, state: &AppState) -> Result<DeleteAssetSummary, String> {
    try_with_scan_and_thumb_lock(state, |permit| {
        let conn = permit.durable_connection()?;
        Ok(asset_mutation_service::delete_asset(
            &conn,
            &state.thumbs_dir,
            asset_id,
        )?)
    })
    .map_err(|e| e.to_string())?
    .ok_or_else(|| LIBRARY_BUSY_ERROR.to_string())
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
    use std::path::PathBuf;

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

    fn delete_test_state(tmp: &std::path::Path) -> (crate::app::state::AppState, i64, PathBuf) {
        use crate::{db, models::NewAsset, services::thumb_scheduler::ThumbnailScheduler};
        use std::sync::{
            atomic::{AtomicBool, AtomicU64},
            Mutex, RwLock,
        };
        let root = tmp.join("library");
        std::fs::create_dir_all(&root).expect("create root");
        let source = root.join("photo.jpg");
        std::fs::write(&source, b"source").expect("write source");
        let db_path = tmp.join("media.db");
        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        let root_text = root.to_string_lossy().into_owned();
        db::add_scan_root(&conn, &root_text).expect("add scan root");
        let asset = NewAsset {
            path: source.to_string_lossy().into_owned(),
            kind: "image".to_string(),
            size_bytes: 6,
            modified_at: 1,
            width: None,
            height: None,
            duration_ms: None,
            thumb_path: None,
        };
        db::upsert_scanned_asset(&conn, &asset, 1, &root_text, 1).expect("index asset");
        let asset_id = conn
            .query_row("SELECT id FROM assets", [], |row| row.get(0))
            .expect("asset id");
        drop(conn);
        let state = crate::app::state::AppState {
            database: crate::services::db_pool::DatabaseRuntime::new(db_path.clone()),
            db_path,
            thumbs_dir: tmp.join("thumbs"),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: AtomicU64::new(0),
        };
        (state, asset_id, source)
    }

    fn indexed_assets(state: &crate::app::state::AppState) -> i64 {
        let conn = state.database.admit().unwrap().connection().unwrap();
        conn.query_row("SELECT count(*) FROM assets", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn delete_fails_fast_while_a_scan_holds_the_scan_lock_and_changes_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (state, asset_id, source) = delete_test_state(tmp.path());
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            // The scan side takes the lock through the same helper a scan uses.
            let state = &state;
            let scan = scope.spawn(move || {
                crate::app::locks::with_scan_lock(state, |_| {
                    held_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap();
            });
            held_rx.recv().unwrap();

            let error = super::delete_asset_service(asset_id, state)
                .expect_err("a delete during a scan must not wait for it");
            assert_eq!(error, super::LIBRARY_BUSY_ERROR);
            assert!(source.exists(), "the source is not staged or removed");
            assert_eq!(indexed_assets(state), 1);
            let conn = state.database.admit().unwrap().connection().unwrap();
            assert!(crate::db::list_pending_file_operations(&conn)
                .unwrap()
                .is_empty());
            drop(conn);
            assert_eq!(state.database.checked_out_connections(), 0);

            release_tx.send(()).unwrap();
            scan.join().unwrap();
        });

        let summary = super::delete_asset_service(asset_id, &state).expect("delete after the scan");
        assert!(matches!(
            summary.source_status,
            crate::models::DeleteSourceStatus::Deleted
        ));
        assert!(!source.exists());
        assert_eq!(indexed_assets(&state), 0);
    }

    #[test]
    fn delete_recovers_a_scan_lock_poisoned_by_a_panicked_holder() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (state, asset_id, source) = delete_test_state(tmp.path());
        std::thread::scope(|scope| {
            let _ = scope
                .spawn(|| {
                    let _guard = state.scan_lock.lock().unwrap();
                    panic!("poison the scan lock");
                })
                .join();
        });
        assert!(state.scan_lock.is_poisoned());

        super::delete_asset_service(asset_id, &state).expect("a poisoned lock is not busy");
        assert!(!source.exists());
        assert_eq!(indexed_assets(&state), 0);
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
