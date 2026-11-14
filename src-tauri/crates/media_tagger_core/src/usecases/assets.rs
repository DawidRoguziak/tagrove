//! Asset use cases: query sessions, summaries and details, metadata writes,
//! delete, rename, duplicates and tags. Called by `commands/assets.rs`.

use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Deserialize;

use crate::{
    app::{
        locks::{try_with_scan_and_thumb_lock, with_scan_and_thumb_lock},
        state::AppState,
    },
    db::{self, AssetMetaFilter},
    error::AppResult,
    models::{
        AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySectionsResult,
        AssetSummary, BulkFavoriteSummary, BulkMediaGroupSummary, BulkTagMergeSummary,
        DeleteAssetSummary, DuplicateResolutionBatchInput, DuplicateResolutionBatchSummary,
        DuplicateScanSummary, RenameAssetSummary, SetAssetTagsSummary, StartAssetQueryResult,
        TagListPage,
    },
    services::{
        asset_mutation_service, asset_query_service,
        db_pool::DatabaseRuntime,
        progress::{emit_progress, ProgressSink},
    },
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

/// Validation and admission of `start_asset_query`, run by the command before
/// its blocking task. Returns the normalized filters and the request token.
#[allow(clippy::too_many_arguments)]
pub fn begin_asset_query_service(
    database: &Arc<DatabaseRuntime>,
    tags_and: Vec<String>,
    tags_not: Vec<String>,
    kind: Option<String>,
    favorites_only: bool,
    meta_filter: Option<AssetMetaFilterInput>,
    owner_id: &str,
    generation: u64,
) -> Result<(asset_query_service::AssetQueryFilters, u64), String> {
    let filters = normalize_query_filters(tags_and, tags_not, kind, favorites_only, meta_filter)?;
    if owner_id.is_empty() || owner_id.len() > 128 {
        return Err("Invalid query owner ID".into());
    }
    // Async command admission may be reordered. Compare generations within
    // the frontend owner before assigning the process-wide request token.
    let request_id = database.queries.begin_request(owner_id, generation);
    Ok((filters, request_id))
}

pub fn start_asset_query_service(
    database: &Arc<DatabaseRuntime>,
    filters: asset_query_service::AssetQueryFilters,
    page_size: usize,
    request_id: u64,
    generation: u64,
    anchor_offset: Option<usize>,
) -> AppResult<StartAssetQueryResult> {
    database.queries.start_at(
        &database.admit()?,
        filters,
        page_size,
        request_id,
        generation,
        anchor_offset,
    )
}

pub fn get_asset_query_page_service(
    database: &Arc<DatabaseRuntime>,
    session_id: u64,
    offset: usize,
    limit: usize,
) -> AppResult<AssetQueryPageResult> {
    database
        .queries
        .page(&database.admit()?, session_id, offset, limit)
}

pub fn get_asset_query_position_service(
    database: &Arc<DatabaseRuntime>,
    session_id: u64,
    asset_id: i64,
) -> AppResult<AssetQueryPositionResult> {
    database
        .queries
        .position(&database.admit()?, session_id, asset_id)
}

/// Ordered session IDs for `start..=end` as raw bytes (see
/// `asset_query_service::id_range`): little-endian f64 `[count, ids..]`, or an
/// empty body when the session is stale.
pub fn get_asset_query_id_range_service(
    database: &Arc<DatabaseRuntime>,
    session_id: u64,
    start: usize,
    end: usize,
) -> AppResult<Vec<u8>> {
    let bytes = database
        .queries
        .id_range(&database.admit()?, session_id, start, end)?;
    Ok(bytes.unwrap_or_default())
}

/// Timeline sections of a query session (`asset_query_service::sections`).
pub fn get_asset_query_sections_service(
    database: &Arc<DatabaseRuntime>,
    session_id: u64,
) -> AppResult<AssetQuerySectionsResult> {
    database.queries.sections(&database.admit()?, session_id)
}

pub fn validate_summary_batch(asset_ids: &[i64]) -> Result<(), String> {
    if asset_ids.len() > 256 {
        return Err("At most 256 asset IDs are allowed".to_string());
    }
    Ok(())
}

pub fn get_asset_summaries_by_ids_service(
    database: &Arc<DatabaseRuntime>,
    asset_ids: &[i64],
) -> AppResult<Vec<AssetSummary>> {
    let conn = database.admit()?.connection()?;
    db::list_asset_summaries_by_ids(&conn, asset_ids).map_err(Into::into)
}

pub fn get_asset_details_service(
    database: &Arc<DatabaseRuntime>,
    asset_id: i64,
) -> AppResult<Option<AssetDetails>> {
    let conn = database.admit()?.connection()?;
    db::get_asset_details(&conn, asset_id).map_err(Into::into)
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

pub fn set_asset_tags_service(
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

pub fn merge_asset_tags_bulk_service(
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

pub fn toggle_assets_favorite_bulk_service(
    database: &Arc<DatabaseRuntime>,
    asset_ids: &[i64],
) -> AppResult<BulkFavoriteSummary> {
    let mut conn = database.admit()?.user_write_connection()?;
    db::toggle_assets_favorite_bulk(&mut conn, asset_ids).map_err(crate::error::AppError::from)
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

pub fn set_asset_favorite_service(
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

pub fn set_asset_media_group_service(
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

pub fn set_assets_media_group_bulk_service(
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

pub fn list_tags_service(
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

/// Rejection text of a delete that found a scan or another combined workflow
/// running. It is backend display text in the `i18n:<key>` format, which the
/// frontend translates; nothing branches on it.
pub(crate) const LIBRARY_BUSY_ERROR: &str = "i18n:common.libraryBusy";

/// Fails fast instead of waiting for `scan_lock`: a scan holds it for its whole
/// run, and the lightbox keeps its confirmation locked while the delete is
/// pending. Nothing is staged or written before the lock is taken.
pub fn delete_asset_service(asset_id: i64, state: &AppState) -> Result<DeleteAssetSummary, String> {
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

pub fn find_duplicate_assets_service(
    state: &AppState,
    progress: &impl ProgressSink,
) -> Result<DuplicateScanSummary, String> {
    (|| {
        let conn = state.database.admit()?.connection()?;
        let groups = db::list_duplicate_groups(&conn)?;
        let revision = db::current_library_revision(&conn)?;
        let total = groups.len();

        let _ = emit_progress(
            progress,
            "duplicates-scan",
            0,
            total,
            format!("Searching duplicates for {total} file names"),
        );

        let duplicate_assets = groups.iter().map(|group| group.assets.len()).sum::<usize>();

        let duplicate_groups = groups.len();
        let _ = emit_progress(
            progress,
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
}

pub fn rename_asset_file_service(
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

pub fn apply_duplicate_resolution_batch_service(
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

    /// The validation errors reach the user as backend text; the literals pin
    /// their wording (ADR 0034: error texts are part of the wire contract).
    #[test]
    fn validation_errors_keep_their_wire_text() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let database = crate::services::db_pool::DatabaseRuntime::new(tmp.path().join("media.db"));
        let begin = |meta_filter: Option<super::AssetMetaFilterInput>, owner_id: &str| {
            super::begin_asset_query_service(
                &database,
                Vec::new(),
                Vec::new(),
                None,
                false,
                meta_filter,
                owner_id,
                1,
            )
            .map(|_| ())
        };
        assert_eq!(begin(None, ""), Err("Invalid query owner ID".to_string()));
        assert_eq!(
            begin(None, &"o".repeat(129)),
            Err("Invalid query owner ID".to_string())
        );
        assert_eq!(begin(None, &"o".repeat(128)), Ok(()));
        assert_eq!(
            begin(
                Some(super::AssetMetaFilterInput::HasNoTags { tag_count: -1 }),
                "owner"
            ),
            Err("Invalid hasNoTags meta filter".to_string())
        );
        assert_eq!(
            super::validate_summary_batch(&[1; 257]),
            Err("At most 256 asset IDs are allowed".to_string())
        );
        assert_eq!(super::LIBRARY_BUSY_ERROR, "i18n:common.libraryBusy");
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
