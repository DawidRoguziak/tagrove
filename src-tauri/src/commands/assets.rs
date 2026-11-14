use tauri::{Manager, State};

use crate::{
    app::state::AppState,
    models::{
        AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetSummary,
        BulkMediaGroupSummary, BulkTagMergeSummary, DeleteAssetSummary,
        DuplicateResolutionBatchInput, DuplicateResolutionBatchSummary, DuplicateScanSummary,
        RenameAssetSummary, SetAssetTagsSummary, StartAssetQueryResult, TagListPage,
    },
    usecases::assets,
};

pub use crate::usecases::assets::{AssetMetaFilterInput, BulkMediaGroupUpdateInput};

#[tauri::command]
pub fn get_startup_popular_tags(
    state: State<'_, crate::app::state::StartupPopularTags>,
) -> Vec<String> {
    state.tags().to_vec()
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
    // Validation and the per-owner request token stay ahead of the blocking
    // task: async command admission may be reordered.
    let (filters, request_id) = assets::begin_asset_query_service(
        &database,
        tags_and,
        tags_not,
        kind,
        favorites_only,
        meta_filter,
        &owner_id,
        generation,
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        assets::start_asset_query_service(
            &database,
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
        assets::get_asset_query_page_service(&database, session_id, offset, limit)
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
        assets::get_asset_query_position_service(&database, session_id, asset_id)
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
        assets::get_asset_query_id_range_service(&database, session_id, start, end)
    })
    .await
    .map_err(|e| format!("asset ID range worker failed: {e}"))?
    .map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Timeline sections of a query session (`asset_query_service::sections`).
#[tauri::command]
pub async fn get_asset_query_sections(
    session_id: u64,
    state: State<'_, AppState>,
) -> Result<crate::models::AssetQuerySectionsResult, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        assets::get_asset_query_sections_service(&database, session_id)
    })
    .await
    .map_err(|e| format!("asset sections worker failed: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_asset_summaries_by_ids(
    asset_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<AssetSummary>, String> {
    assets::validate_summary_batch(&asset_ids)?;
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        assets::get_asset_summaries_by_ids_service(&database, &asset_ids)
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
        assets::get_asset_details_service(&database, asset_id)
    })
    .await
    .map_err(|e| format!("asset details worker failed: {e}"))?
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
        assets::set_asset_tags_service(asset_id, tags, &state)
    })
    .await
    .map_err(|e| format!("set_asset_tags worker failed: {e}"))?
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
        assets::merge_asset_tags_bulk_service(asset_ids, tags, &state)
    })
    .await
    .map_err(|e| format!("merge_asset_tags_bulk worker failed: {e}"))?
}

#[tauri::command]
pub async fn toggle_assets_favorite_bulk(
    asset_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<crate::models::BulkFavoriteSummary, String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        assets::toggle_assets_favorite_bulk_service(&database, &asset_ids)
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

#[tauri::command]
pub async fn set_asset_favorite(
    asset_id: i64,
    is_favorite: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        assets::set_asset_favorite_service(asset_id, is_favorite, &state)
    })
    .await
    .map_err(|e| format!("set_asset_favorite worker failed: {e}"))?
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
        assets::set_asset_media_group_service(asset_id, media_group_key, media_group_order, &state)
    })
    .await
    .map_err(|e| format!("set_asset_media_group worker failed: {e}"))?
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
        assets::set_assets_media_group_bulk_service(updates, media_group_key, &state)
    })
    .await
    .map_err(|e| format!("set_assets_media_group_bulk worker failed: {e}"))?
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
        assets::list_tags_service(query, offset, limit, &state)
    })
    .await
    .map_err(|e| format!("list_tags worker failed: {e}"))?
}

#[tauri::command]
pub async fn delete_asset(
    asset_id: i64,
    app: tauri::AppHandle,
) -> Result<DeleteAssetSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        assets::delete_asset_service(asset_id, &state)
    })
    .await
    .map_err(|e| format!("delete_asset worker failed: {e}"))?
}

#[tauri::command]
pub async fn find_duplicate_assets(app: tauri::AppHandle) -> Result<DuplicateScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        assets::find_duplicate_assets_service(&state, &app)
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
        assets::rename_asset_file_service(asset_id, new_file_name, &state)
    })
    .await
    .map_err(|e| format!("rename_asset_file worker failed: {e}"))?
}

#[tauri::command]
pub async fn apply_duplicate_resolution_batch(
    input: DuplicateResolutionBatchInput,
    app: tauri::AppHandle,
) -> Result<DuplicateResolutionBatchSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        assets::apply_duplicate_resolution_batch_service(input, &state)
    })
    .await
    .map_err(|e| format!("apply_duplicate_resolution_batch worker failed: {e}"))?
}
