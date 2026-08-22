use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

use tauri::{ipc::Channel, Emitter, State};

use crate::{
    app::state::AppState,
    db,
    error::AppResult,
    models::{
        ThumbnailBatchItem, ThumbnailBatchResult, ThumbnailRenderSummary, ThumbnailStreamEvent,
    },
    services::{
        progress::emit_progress,
        thumb_scheduler::{ThumbnailPriority, ThumbnailTask, ThumbnailTaskResult},
    },
    thumbs,
};

const BULK_IN_FLIGHT_LIMIT: usize = 24;
const RESULT_WAIT_TIMEOUT_MS: u64 = 50;
/// Wall-clock budget for a single demand call waiting on its scheduler job.
/// Covers a 45 s video render plus one zero-seek fallback and queueing delay;
/// after it, the command fails in a controlled way instead of hanging.
const DEMAND_JOB_TIMEOUT: Duration = Duration::from_secs(180);
/// Maximum time a batch coordinator may see no completed result at all before
/// declaring the pipeline stalled (worker loss, poisoned queues) and failing.
const RESULT_STALL_TIMEOUT: Duration = Duration::from_secs(180);
/// Backend cap for the legacy page endpoint. The production frontend sends at
/// most 64 IDs per invoke; this bound stops other IPC callers from enqueueing
/// unbounded work through `ensure_page_thumbnails`.
pub const MAX_PAGE_BATCH: usize = 256;

#[derive(Clone, Copy)]
enum BulkRenderMode {
    All,
    FailedOnly,
}

struct PendingRenderTask {
    asset_path: String,
    modified_at: i64,
    version: crate::thumbs::SourceVersion,
}

struct PendingPageTask {
    modified_at: i64,
    version: crate::thumbs::SourceVersion,
}

fn source_version_for(asset: &crate::models::ThumbnailAsset) -> crate::thumbs::SourceVersion {
    crate::thumbs::SourceVersion::new(
        asset.path.clone(),
        asset.size_bytes,
        asset.fingerprint_mtime_ns,
    )
}

pub fn render_all_thumbnails<R: tauri::Runtime>(
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
) -> AppResult<ThumbnailRenderSummary> {
    render_bulk_thumbnails(state, app, BulkRenderMode::All)
}

pub fn render_failed_thumbnails<R: tauri::Runtime>(
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
) -> AppResult<ThumbnailRenderSummary> {
    render_bulk_thumbnails(state, app, BulkRenderMode::FailedOnly)
}

pub fn cancel_render_all_thumbnails(state: &State<AppState>) -> AppResult<bool> {
    let running = state.thumbnail_render_all_running.load(Ordering::SeqCst);
    if running {
        state
            .thumbnail_render_all_cancel_requested
            .store(true, Ordering::SeqCst);
    }
    Ok(running)
}

fn render_bulk_thumbnails<R: tauri::Runtime>(
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
    mode: BulkRenderMode,
) -> AppResult<ThumbnailRenderSummary> {
    if state
        .thumbnail_render_all_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Thumbnail render is already running".into());
    }

    state
        .thumbnail_render_all_cancel_requested
        .store(false, Ordering::SeqCst);

    let result = render_bulk_thumbnails_inner(state, app, mode);

    state
        .thumbnail_render_all_running
        .store(false, Ordering::SeqCst);
    state
        .thumbnail_render_all_cancel_requested
        .store(false, Ordering::SeqCst);

    result
}

fn render_bulk_thumbnails_inner<R: tauri::Runtime>(
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
    mode: BulkRenderMode,
) -> AppResult<ThumbnailRenderSummary> {
    let conn = db::open_connection(&state.db_path)?;
    let assets = match mode {
        BulkRenderMode::All => db::list_assets_for_thumbnail_render(&conn)?,
        BulkRenderMode::FailedOnly => db::list_failed_assets_for_thumbnail_render(&conn)?,
    };
    let skip_failed_ids = match mode {
        BulkRenderMode::All => db::list_failed_thumbnail_asset_ids(&conn)?
            .into_iter()
            .collect::<HashSet<_>>(),
        BulkRenderMode::FailedOnly => HashSet::new(),
    };
    let total = assets.len();

    let _ = emit_progress(
        app,
        mode.start_phase(),
        0,
        total,
        format!("{} for {total} assets", mode.start_message()),
    );

    let mut generated = 0usize;
    let mut failed = 0usize;
    let mut skipped_failed = 0usize;
    let mut processed = 0usize;
    let mut cancelled = false;
    let mut updates = Vec::<(i64, Option<String>, crate::thumbs::SourceVersion)>::new();
    let mut pending = HashMap::<i64, PendingRenderTask>::new();
    let (completion_tx, completion_rx) = mpsc::channel::<ThumbnailTaskResult>();
    let generation = state.thumbnail_generation.load(Ordering::SeqCst);

    for asset in assets {
        if state
            .thumbnail_render_all_cancel_requested
            .load(Ordering::SeqCst)
        {
            cancelled = true;
            break;
        }

        let source_path = PathBuf::from(&asset.path);
        let version = source_version_for(&asset);
        let target = thumbs::thumb_target(&state.thumbs_dir, &version);

        if !source_path.exists() {
            failed += 1;
            processed += 1;
            updates.push((asset.id, None, version));
            let _ = db::record_thumbnail_failure(
                &conn,
                asset.id,
                asset.modified_at,
                Some("source file not found"),
            );
            if let Some(existing) = asset.thumb_path {
                let _ = fs::remove_file(existing);
            }
            emit_render_progress(app, mode.progress_phase(), processed, total);
            continue;
        }

        if target.exists() {
            generated += 1;
            processed += 1;
            updates.push((
                asset.id,
                Some(target.to_string_lossy().to_string()),
                version,
            ));
            let _ = db::clear_thumbnail_failure(&conn, asset.id);
            emit_render_progress(app, mode.progress_phase(), processed, total);
            continue;
        }

        if matches!(mode, BulkRenderMode::All) && skip_failed_ids.contains(&asset.id) {
            skipped_failed += 1;
            processed += 1;
            emit_render_progress(app, mode.progress_phase(), processed, total);
            continue;
        }

        state.thumb_scheduler.enqueue_with_sender(
            ThumbnailTask {
                asset_id: asset.id,
                source_path,
                target_path: target,
                kind: asset.kind,
                duration_ms: asset.duration_ms,
                source_version: version.clone(),
            },
            ThumbnailPriority::Low,
            completion_tx.clone(),
        )?;

        pending.insert(
            asset.id,
            PendingRenderTask {
                asset_path: asset.path,
                modified_at: asset.modified_at,
                version,
            },
        );

        while pending.len() >= BULK_IN_FLIGHT_LIMIT {
            let Some(result) = wait_next_render_result(
                &completion_rx,
                &state.thumbnail_render_all_cancel_requested,
            )?
            else {
                cancelled = true;
                break;
            };

            let Some(task) = pending.remove(&result.asset_id) else {
                continue;
            };

            process_bulk_result(
                &conn,
                app,
                mode,
                total,
                &mut processed,
                &mut generated,
                &mut failed,
                &mut updates,
                task,
                result,
            );
        }

        if cancelled {
            break;
        }
    }

    if cancelled {
        drain_ready_render_results(
            &conn,
            app,
            mode,
            total,
            &completion_rx,
            &mut pending,
            &mut processed,
            &mut generated,
            &mut failed,
            &mut updates,
        );
    } else {
        while !pending.is_empty() {
            let Some(result) = wait_next_render_result(
                &completion_rx,
                &state.thumbnail_render_all_cancel_requested,
            )?
            else {
                cancelled = true;
                break;
            };

            let Some(task) = pending.remove(&result.asset_id) else {
                continue;
            };

            process_bulk_result(
                &conn,
                app,
                mode,
                total,
                &mut processed,
                &mut generated,
                &mut failed,
                &mut updates,
                task,
                result,
            );
        }
    }

    // A cleared-thumbnail workflow (generation bump) between enqueue and
    // commit invalidates every collected result: publishing them would
    // resurrect references the user explicitly removed.
    let stale_ids = publish_thumbnail_updates(
        &conn,
        &updates,
        state.thumbnail_generation.load(Ordering::SeqCst),
        generation,
    )?;

    // Results whose compare-and-set lost against a re-index (or that were
    // dropped by a generation reset) must not leave stale-version targets on
    // disk. Best-effort removal; orphans are handled by explicit cleanup.
    remove_dropped_thumbnail_files(&updates, &stale_ids);

    let _ = emit_progress(
        app,
        mode.done_phase(),
        processed,
        total,
        if cancelled {
            format!(
                "{} cancelled. Ready: {generated}, failed: {failed}, skipped failed: {skipped_failed}",
                mode.done_message_prefix()
            )
        } else {
            format!(
                "{} complete. Ready: {generated}, failed: {failed}, skipped failed: {skipped_failed}",
                mode.done_message_prefix()
            )
        },
    );

    Ok(ThumbnailRenderSummary {
        generated,
        failed,
        skipped_failed,
        processed,
        total,
        cancelled,
    })
}

#[allow(clippy::too_many_arguments)]
fn process_bulk_result(
    conn: &rusqlite::Connection,
    app: &tauri::AppHandle<impl tauri::Runtime>,
    mode: BulkRenderMode,
    total: usize,
    processed: &mut usize,
    generated: &mut usize,
    failed: &mut usize,
    updates: &mut Vec<(i64, Option<String>, crate::thumbs::SourceVersion)>,
    task: PendingRenderTask,
    result: ThumbnailTaskResult,
) {
    *processed += 1;
    if let Some(path) = result.thumb_path {
        *generated += 1;
        updates.push((result.asset_id, Some(path), task.version));
        let _ = db::clear_thumbnail_failure(conn, result.asset_id);
    } else {
        *failed += 1;
        updates.push((result.asset_id, None, task.version));
        let _ = db::record_thumbnail_failure(
            conn,
            result.asset_id,
            task.modified_at,
            Some("thumbnail generation failed"),
        );
        let _ = emit_progress(
            app,
            "thumbs-warn",
            *processed,
            total,
            format!("Cannot render thumbnail for {}", task.asset_path),
        );
    }
    emit_render_progress(app, mode.progress_phase(), *processed, total);
}

fn wait_next_render_result(
    completion_receiver: &Receiver<ThumbnailTaskResult>,
    cancel_requested: &std::sync::atomic::AtomicBool,
) -> AppResult<Option<ThumbnailTaskResult>> {
    wait_next_render_result_with_stall(completion_receiver, cancel_requested, RESULT_STALL_TIMEOUT)
}

fn wait_next_render_result_with_stall(
    completion_receiver: &Receiver<ThumbnailTaskResult>,
    cancel_requested: &std::sync::atomic::AtomicBool,
    stall_timeout: Duration,
) -> AppResult<Option<ThumbnailTaskResult>> {
    // The stall deadline makes every accepted job terminate with success,
    // cancellation, or a controlled error even if the scheduler loses its
    // workers or a result is otherwise never delivered.
    let last_progress = Instant::now();
    loop {
        if cancel_requested.load(Ordering::SeqCst) {
            return Ok(None);
        }

        match completion_receiver.recv_timeout(Duration::from_millis(RESULT_WAIT_TIMEOUT_MS)) {
            Ok(result) => {
                return Ok(Some(result));
            }
            Err(RecvTimeoutError::Timeout) => {
                if last_progress.elapsed() > stall_timeout {
                    return Err(format!(
                        "thumbnail job timed out after {}s without any completed result",
                        stall_timeout.as_secs()
                    )
                    .into());
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("thumbnail completion channel disconnected".into())
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn drain_ready_render_results(
    conn: &rusqlite::Connection,
    app: &tauri::AppHandle<impl tauri::Runtime>,
    mode: BulkRenderMode,
    total: usize,
    completion_receiver: &Receiver<ThumbnailTaskResult>,
    pending: &mut HashMap<i64, PendingRenderTask>,
    processed: &mut usize,
    generated: &mut usize,
    failed: &mut usize,
    updates: &mut Vec<(i64, Option<String>, crate::thumbs::SourceVersion)>,
) {
    while let Ok(result) = completion_receiver.try_recv() {
        let Some(task) = pending.remove(&result.asset_id) else {
            continue;
        };
        process_bulk_result(
            conn, app, mode, total, processed, generated, failed, updates, task, result,
        );
    }
}

pub fn ensure_asset_thumbnail(asset_id: i64, state: &State<AppState>) -> AppResult<Option<String>> {
    let conn = db::open_connection(&state.db_path)?;
    let Some(asset) = db::get_asset_for_thumbnail(&conn, asset_id)? else {
        return Ok(None);
    };

    if let Some(existing) = asset.thumb_path.clone() {
        let existing_path = PathBuf::from(&existing);
        if existing_path.exists() {
            let _ = db::clear_thumbnail_failure(&conn, asset.id);
            return Ok(Some(existing));
        }
    }

    let source_path = PathBuf::from(&asset.path);
    let version = source_version_for(&asset);
    if !source_path.exists() {
        let _ =
            db::update_asset_thumbnail_path_if_version_matches(&conn, asset.id, None, &version)?;
        let _ = db::record_thumbnail_failure(
            &conn,
            asset.id,
            asset.modified_at,
            Some("source file not found"),
        );
        if let Some(existing) = asset.thumb_path {
            let _ = fs::remove_file(existing);
        }
        return Ok(None);
    }

    let target = thumbs::thumb_target(&state.thumbs_dir, &version);
    if target.exists() {
        let target_str = target.to_string_lossy().to_string();
        db::update_asset_thumbnail_path_if_version_matches(
            &conn,
            asset.id,
            Some(&target_str),
            &version,
        )?;
        let _ = db::clear_thumbnail_failure(&conn, asset.id);
        return Ok(Some(target_str));
    }

    let receiver = state.thumb_scheduler.enqueue(
        ThumbnailTask {
            asset_id: asset.id,
            source_path,
            target_path: target,
            kind: asset.kind,
            duration_ms: asset.duration_ms,
            source_version: version.clone(),
        },
        ThumbnailPriority::High,
    )?;

    // Bounded wait: a lost worker or wedged scheduler must surface as an
    // error instead of hanging the command (and its read lock) forever.
    let deadline = Instant::now() + DEMAND_JOB_TIMEOUT;
    let result = loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "thumbnail job for asset {asset_id} timed out after {}s",
                        DEMAND_JOB_TIMEOUT.as_secs()
                    )
                    .into());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("thumbnail worker disconnected".into())
            }
        }
    };

    if let Some(path) = result.thumb_path {
        match db::update_asset_thumbnail_path_if_version_matches(
            &conn,
            asset.id,
            Some(&path),
            &version,
        )? {
            db::ThumbnailCasOutcome::Applied => {
                let _ = db::clear_thumbnail_failure(&conn, asset.id);
                Ok(Some(path))
            }
            db::ThumbnailCasOutcome::VersionMismatch => {
                // The asset was re-indexed while rendering; this file belongs
                // to a superseded version and a fresh demand call will redo it.
                let _ = fs::remove_file(&path);
                Ok(None)
            }
        }
    } else {
        let _ =
            db::update_asset_thumbnail_path_if_version_matches(&conn, asset.id, None, &version)?;
        let _ = db::record_thumbnail_failure(
            &conn,
            asset.id,
            asset.modified_at,
            Some("thumbnail generation failed"),
        );
        Ok(None)
    }
}

pub fn ensure_page_thumbnails<R: tauri::Runtime>(
    asset_ids: Vec<i64>,
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
) -> AppResult<ThumbnailBatchResult> {
    let high_priority = asset_ids.iter().copied().collect::<HashSet<_>>();
    ensure_page_thumbnails_with_sink(asset_ids, &high_priority, state, app, |item| {
        let _ = app.emit("thumbnail-ready", item);
    })
}

pub fn ensure_thumbnails_stream<R: tauri::Runtime>(
    request_id: u64,
    visible_ids: Vec<i64>,
    prefetch_ids: Vec<i64>,
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
    channel: Channel<ThumbnailStreamEvent>,
) -> AppResult<()> {
    // The frontend sends its queue generation as the request id. A lower id
    // belongs to a reset/abandoned generation; reject it instead of rendering
    // work nobody will consume anymore.
    let mut latest = state.thumbnail_latest_request_id.load(Ordering::SeqCst);
    while request_id > latest {
        match state.thumbnail_latest_request_id.compare_exchange(
            latest,
            request_id,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => break,
            Err(current) => latest = current,
        }
    }
    if request_id != 0 && request_id < state.thumbnail_latest_request_id.load(Ordering::SeqCst) {
        return Ok(());
    }

    let mut seen = HashSet::new();
    let visible = visible_ids
        .into_iter()
        .filter(|id| *id > 0 && seen.insert(*id))
        .take(64)
        .collect::<Vec<_>>();
    let high_priority = visible.iter().copied().collect::<HashSet<_>>();
    let mut ids = visible;
    ids.extend(
        prefetch_ids
            .into_iter()
            .filter(|id| *id > 0 && seen.insert(*id))
            .take(64),
    );
    let result = ensure_page_thumbnails_with_sink(ids, &high_priority, state, app, |item| {
        let _ = channel.send(ThumbnailStreamEvent::Ready(item.clone()));
    })?;
    for asset_id in &result.failed {
        let _ = channel.send(ThumbnailStreamEvent::Failed {
            asset_id: *asset_id,
        });
    }
    let _ = channel.send(ThumbnailStreamEvent::Done {
        ready: result.ready.len(),
        failed: result.failed.len(),
    });
    Ok(())
}

fn ensure_page_thumbnails_with_sink<R: tauri::Runtime>(
    asset_ids: Vec<i64>,
    high_priority: &HashSet<i64>,
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
    mut on_ready: impl FnMut(&ThumbnailBatchItem),
) -> AppResult<ThumbnailBatchResult> {
    let started = std::time::Instant::now();
    let conn = db::open_connection(&state.db_path)?;
    let mut seen = HashSet::new();
    let ids = asset_ids
        .into_iter()
        .filter(|asset_id| seen.insert(*asset_id))
        .take(MAX_PAGE_BATCH)
        .collect::<Vec<_>>();
    let total = ids.len();

    let _ = emit_progress(
        app,
        "thumbs-page-start",
        0,
        total,
        format!("Thumbnail page render started for {total} assets"),
    );

    let mut processed = 0usize;
    let mut ready = Vec::new();
    let mut failed = Vec::new();
    let mut updates = Vec::<(i64, Option<String>, crate::thumbs::SourceVersion)>::new();
    let mut pending = HashMap::<i64, PendingPageTask>::new();
    let (completion_tx, completion_rx) = mpsc::channel::<ThumbnailTaskResult>();
    let generation = state.thumbnail_generation.load(Ordering::SeqCst);
    let mut assets_by_id = db::get_assets_for_thumbnails_by_ids(&conn, &ids)?
        .into_iter()
        .map(|asset| (asset.id, asset))
        .collect::<HashMap<_, _>>();

    for asset_id in ids {
        let Some(asset) = assets_by_id.remove(&asset_id) else {
            failed.push(asset_id);
            processed += 1;
            emit_page_progress(app, processed, total);
            continue;
        };

        if let Some(existing) = asset.thumb_path.clone() {
            let existing_path = PathBuf::from(&existing);
            if existing_path.exists() {
                let item = ThumbnailBatchItem {
                    asset_id: asset.id,
                    thumb_path: existing,
                };
                on_ready(&item);
                ready.push(item);
                let _ = db::clear_thumbnail_failure(&conn, asset.id);
                processed += 1;
                emit_page_progress(app, processed, total);
                continue;
            }
        }

        let source_path = PathBuf::from(&asset.path);
        let version = source_version_for(&asset);
        if !source_path.exists() {
            failed.push(asset.id);
            updates.push((asset.id, None, version));
            let _ = db::record_thumbnail_failure(
                &conn,
                asset.id,
                asset.modified_at,
                Some("source file not found"),
            );
            processed += 1;
            emit_page_progress(app, processed, total);
            continue;
        }

        let target = thumbs::thumb_target(&state.thumbs_dir, &version);
        if target.exists() {
            let target_str = target.to_string_lossy().to_string();
            let item = ThumbnailBatchItem {
                asset_id: asset.id,
                thumb_path: target_str.clone(),
            };
            on_ready(&item);
            ready.push(item);
            updates.push((asset.id, Some(target_str), version));
            let _ = db::clear_thumbnail_failure(&conn, asset.id);
            processed += 1;
            emit_page_progress(app, processed, total);
            continue;
        }

        state.thumb_scheduler.enqueue_with_sender(
            ThumbnailTask {
                asset_id: asset.id,
                source_path,
                target_path: target,
                kind: asset.kind,
                duration_ms: asset.duration_ms,
                source_version: version.clone(),
            },
            if high_priority.contains(&asset.id) {
                ThumbnailPriority::High
            } else {
                ThumbnailPriority::Low
            },
            completion_tx.clone(),
        )?;

        pending.insert(
            asset.id,
            PendingPageTask {
                modified_at: asset.modified_at,
                version,
            },
        );
    }

    // Bounded stall wait mirrors the bulk coordinator: every accepted job
    // ends with a result or a controlled error, never an infinite block.
    let mut last_progress = Instant::now();
    while !pending.is_empty() {
        match completion_rx.recv_timeout(Duration::from_millis(RESULT_WAIT_TIMEOUT_MS)) {
            Ok(result) => {
                last_progress = Instant::now();
                let Some(task) = pending.remove(&result.asset_id) else {
                    continue;
                };

                processed += 1;
                if let Some(path) = result.thumb_path {
                    let item = ThumbnailBatchItem {
                        asset_id: result.asset_id,
                        thumb_path: path.clone(),
                    };
                    on_ready(&item);
                    ready.push(item);
                    updates.push((result.asset_id, Some(path), task.version));
                    let _ = db::clear_thumbnail_failure(&conn, result.asset_id);
                } else {
                    failed.push(result.asset_id);
                    updates.push((result.asset_id, None, task.version));
                    let _ = db::record_thumbnail_failure(
                        &conn,
                        result.asset_id,
                        task.modified_at,
                        Some("thumbnail generation failed"),
                    );
                }
                emit_page_progress(app, processed, total);
            }
            Err(RecvTimeoutError::Timeout) => {
                if last_progress.elapsed() > RESULT_STALL_TIMEOUT {
                    return Err(format!(
                        "thumbnail job timed out after {}s without any completed result",
                        RESULT_STALL_TIMEOUT.as_secs()
                    )
                    .into());
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("thumbnail worker disconnected".into())
            }
        }
    }

    let stale_ids = publish_thumbnail_updates(
        &conn,
        &updates,
        state.thumbnail_generation.load(Ordering::SeqCst),
        generation,
    )?;

    remove_dropped_thumbnail_files(&updates, &stale_ids);

    let _ = emit_progress(
        app,
        "thumbs-page-done",
        total,
        total,
        format!(
            "Thumbnail page render complete. Ready: {}, failed: {}",
            ready.len(),
            failed.len()
        ),
    );

    if std::env::var_os("MEDIATAGGER_PERF").is_some() {
        eprintln!(
            "[perf] name=thumbnail-page elapsed_ms={} ready={} failed={}",
            started.elapsed().as_millis(),
            ready.len(),
            failed.len()
        );
    }
    Ok(ThumbnailBatchResult { ready, failed })
}

/// Publishes collected thumbnail results. Each row is written through a
/// compare-and-set guarded by the source version captured at enqueue time.
/// When the generation advanced meanwhile (thumbnail reset), nothing is
/// committed and every ID is reported stale so its produced file is removed.
fn publish_thumbnail_updates(
    conn: &rusqlite::Connection,
    updates: &[(i64, Option<String>, crate::thumbs::SourceVersion)],
    current_generation: u64,
    captured_generation: u64,
) -> anyhow::Result<Vec<i64>> {
    if current_generation != captured_generation {
        return Ok(updates.iter().map(|(asset_id, _, _)| *asset_id).collect());
    }
    db::update_asset_thumbnail_paths_batch_versioned(conn, updates)
}

fn remove_dropped_thumbnail_files(
    updates: &[(i64, Option<String>, crate::thumbs::SourceVersion)],
    stale_ids: &[i64],
) {
    for (asset_id, path, _) in updates {
        if !stale_ids.contains(asset_id) {
            continue;
        }
        if let Some(path) = path {
            let _ = fs::remove_file(path);
        }
    }
}

fn emit_render_progress(
    app: &tauri::AppHandle<impl tauri::Runtime>,
    phase: &str,
    processed: usize,
    total: usize,
) {
    if processed.is_multiple_of(25) || processed == total {
        let _ = emit_progress(
            app,
            phase,
            processed,
            total,
            format!("Rendering thumbnails {processed}/{total}"),
        );
    }
}

fn emit_page_progress(app: &tauri::AppHandle<impl tauri::Runtime>, processed: usize, total: usize) {
    if processed < total && !processed.is_multiple_of(16) {
        return;
    }
    if processed.is_multiple_of(25) || processed == total {
        let _ = emit_progress(
            app,
            "thumbs-page",
            processed,
            total,
            format!("Rendering page thumbnails {processed}/{total}"),
        );
    }
}

pub fn clear_all_thumbnails<R: tauri::Runtime>(
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
) -> AppResult<usize> {
    // Invalidate every in-flight render before removing anything: results
    // completing after this point are dropped instead of written back.
    state.thumbnail_generation.fetch_add(1, Ordering::SeqCst);
    let conn = db::open_connection(&state.db_path)?;
    let thumbs = db::clear_all_thumbnail_paths(&conn)?;
    let total = thumbs.len();
    let _ = emit_progress(
        app,
        "thumbs-clear",
        0,
        total,
        "Removing all thumbnails".to_string(),
    );
    let removed =
        delete_thumbnail_files_with_progress(app, &state.thumbs_dir, thumbs, total, "thumbs-clear");
    Ok(removed)
}

pub fn delete_thumbnail_files_in_root(thumbs_root: &Path, paths: Vec<String>) -> usize {
    paths
        .into_iter()
        .filter(|path| remove_thumbnail_file(thumbs_root, Path::new(path)))
        .count()
}

pub fn delete_thumbnail_files_with_progress(
    app: &tauri::AppHandle<impl tauri::Runtime>,
    thumbs_root: &Path,
    paths: Vec<String>,
    total: usize,
    phase: &str,
) -> usize {
    let mut removed = 0usize;
    for (idx, path) in paths.into_iter().enumerate() {
        if remove_thumbnail_file(thumbs_root, Path::new(&path)) {
            removed += 1;
        }
        let processed = idx + 1;
        if processed.is_multiple_of(25) || processed == total {
            let _ = emit_progress(
                app,
                phase,
                processed,
                total,
                format!("Processed {processed}/{total}"),
            );
        }
    }
    removed
}

fn remove_thumbnail_file(thumbs_root: &Path, candidate: &Path) -> bool {
    let Ok(canonical_root) = thumbs_root.canonicalize() else {
        return false;
    };
    let Ok(metadata) = fs::symlink_metadata(candidate) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    let Ok(canonical_candidate) = candidate.canonicalize() else {
        return false;
    };
    if !canonical_candidate.starts_with(&canonical_root) {
        return false;
    }
    fs::remove_file(canonical_candidate).is_ok()
}

#[cfg(test)]
fn delete_thumbnail_files(paths: Vec<String>) -> usize {
    let Some(root) = paths
        .first()
        .and_then(|path| Path::new(path).parent())
        .map(Path::to_path_buf)
    else {
        return 0;
    };
    delete_thumbnail_files_in_root(&root, paths)
}

impl BulkRenderMode {
    fn start_phase(self) -> &'static str {
        match self {
            Self::All => "thumbs-start",
            Self::FailedOnly => "thumbs-failed-start",
        }
    }

    fn progress_phase(self) -> &'static str {
        match self {
            Self::All => "thumbs",
            Self::FailedOnly => "thumbs-failed",
        }
    }

    fn done_phase(self) -> &'static str {
        match self {
            Self::All => "thumbs-done",
            Self::FailedOnly => "thumbs-failed-done",
        }
    }

    fn start_message(self) -> &'static str {
        match self {
            Self::All => "Thumbnail render started",
            Self::FailedOnly => "Failed-thumbnail retry started",
        }
    }

    fn done_message_prefix(self) -> &'static str {
        match self {
            Self::All => "Thumbnail render",
            Self::FailedOnly => "Failed-thumbnail retry",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, AtomicU64},
            mpsc, Arc, Mutex, RwLock,
        },
    };

    use tempfile::tempdir;

    use crate::{
        app::state::AppState,
        db,
        models::NewAsset,
        services::thumb_scheduler::{ThumbnailScheduler, ThumbnailTask},
    };

    use super::{
        cancel_render_all_thumbnails, delete_thumbnail_files, ensure_asset_thumbnail,
        publish_thumbnail_updates, wait_next_render_result_with_stall, ThumbnailTaskResult,
    };

    fn create_test_state(db_path: &Path, thumbs_dir: &Path) -> AppState {
        AppState {
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: std::sync::atomic::AtomicU64::new(0),
            thumbnail_latest_request_id: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn as_state<'a>(state: &'a AppState) -> tauri::State<'a, AppState> {
        unsafe { std::mem::transmute::<&'a AppState, tauri::State<'a, AppState>>(state) }
    }

    fn new_asset(path: &Path, modified_at: i64, thumb_path: Option<&Path>) -> NewAsset {
        NewAsset {
            path: path.to_string_lossy().to_string(),
            kind: "image".to_string(),
            size_bytes: 10,
            modified_at,
            width: Some(100),
            height: Some(100),
            duration_ms: None,
            thumb_path: thumb_path.map(|value| value.to_string_lossy().to_string()),
        }
    }

    #[test]
    fn wait_next_render_result_reads_completion_event() {
        let cancel_requested = AtomicBool::new(false);
        let (sender, receiver) = mpsc::channel();
        sender
            .send(ThumbnailTaskResult {
                asset_id: 42,
                thumb_path: Some("C:/tmp/thumb-42.jpg".to_string()),
                source_version: crate::thumbs::SourceVersion::new("C:/tmp/source-42.jpg", 10, 0),
            })
            .expect("send completion");

        let result = wait_next_render_result_with_stall(
            &receiver,
            &cancel_requested,
            std::time::Duration::from_secs(180),
        )
        .expect("wait result")
        .expect("completion item");

        assert_eq!(result.asset_id, 42);
        assert_eq!(result.thumb_path.as_deref(), Some("C:/tmp/thumb-42.jpg"));
    }

    #[test]
    fn wait_next_render_result_returns_none_when_cancelled() {
        let cancel_requested = AtomicBool::new(true);
        let (_sender, receiver) = mpsc::channel::<ThumbnailTaskResult>();

        let result = wait_next_render_result_with_stall(
            &receiver,
            &cancel_requested,
            std::time::Duration::from_secs(180),
        )
        .expect("wait result");
        assert!(result.is_none());
    }

    #[test]
    fn wait_next_render_result_returns_error_when_channel_disconnected() {
        let cancel_requested = AtomicBool::new(false);
        let (sender, receiver) = mpsc::channel::<ThumbnailTaskResult>();
        drop(sender);

        let error = wait_next_render_result_with_stall(
            &receiver,
            &cancel_requested,
            std::time::Duration::from_secs(180),
        )
        .expect_err("disconnected error");
        assert!(
            error.to_string().contains("disconnected"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn ensure_asset_thumbnail_returns_existing_thumbnail_path() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let source_path = tmp.path().join("missing-source.jpg");
        let existing_thumb = thumbs_dir.join("ready-thumb.jpg");
        fs::write(&existing_thumb, b"thumb").expect("write existing thumb");

        db::upsert_asset(&conn, &new_asset(&source_path, 77, Some(&existing_thumb)))
            .expect("upsert asset");

        let state = create_test_state(&db_path, &thumbs_dir);
        let state_ref = as_state(&state);
        let result = ensure_asset_thumbnail(1, &state_ref).expect("ensure thumb");

        assert_eq!(
            result.as_deref(),
            Some(existing_thumb.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn cancel_render_all_thumbnails_returns_false_when_not_running() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let state = create_test_state(&db_path, &thumbs_dir);
        let state_ref = as_state(&state);
        let accepted = cancel_render_all_thumbnails(&state_ref).expect("cancel result");
        assert!(!accepted);
        assert!(!state
            .thumbnail_render_all_cancel_requested
            .load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn cancel_render_all_thumbnails_sets_cancel_flag_when_running() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let state = create_test_state(&db_path, &thumbs_dir);
        state
            .thumbnail_render_all_running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let state_ref = as_state(&state);
        let accepted = cancel_render_all_thumbnails(&state_ref).expect("cancel result");
        assert!(accepted);
        assert!(state
            .thumbnail_render_all_cancel_requested
            .load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn delete_thumbnail_files_counts_only_existing_files() {
        let tmp = tempdir().expect("tempdir");
        let existing_a = tmp.path().join("thumb-a.jpg");
        let existing_b = tmp.path().join("thumb-b.jpg");
        let missing = tmp.path().join("thumb-missing.jpg");
        fs::write(&existing_a, b"a").expect("write thumb a");
        fs::write(&existing_b, b"b").expect("write thumb b");

        let removed = delete_thumbnail_files(vec![
            existing_a.to_string_lossy().to_string(),
            existing_b.to_string_lossy().to_string(),
            missing.to_string_lossy().to_string(),
        ]);

        assert_eq!(removed, 2);
        assert!(!existing_a.exists());
        assert!(!existing_b.exists());
    }

    fn insert_scanned_asset(
        conn: &rusqlite::Connection,
        path: &Path,
        size_bytes: i64,
        modified_at: i64,
        mtime_ns: i64,
    ) {
        db::add_scan_root(conn, path.parent().unwrap().to_string_lossy().as_ref())
            .expect("add scan root");
        db::upsert_scanned_asset(
            conn,
            &NewAsset {
                path: path.to_string_lossy().to_string(),
                kind: "image".to_string(),
                size_bytes,
                modified_at,
                width: Some(4),
                height: Some(4),
                duration_ms: None,
                thumb_path: None,
            },
            mtime_ns,
            path.parent().unwrap().to_string_lossy().as_ref(),
            1,
        )
        .expect("insert scanned asset");
    }

    /// Race from the acceptance criteria: a render completes for the version
    /// captured at enqueue time while a scan has already re-indexed a newer
    /// file version. The stale result must not be published and its file must
    /// be removed.
    #[test]
    fn demand_result_rendered_for_old_version_is_dropped_after_reindex() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let source_path = tmp.path().join("asset.jpg");
        fs::write(&source_path, b"version-one").expect("write source");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        insert_scanned_asset(&conn, &source_path, 11, 5, 5_000_000_000);
        drop(conn);

        let started = Arc::new(Mutex::new(false));
        let release = Arc::new(Mutex::new(false));
        let started_clone = Arc::clone(&started);
        let release_clone = Arc::clone(&release);

        let scheduler = ThumbnailScheduler::with_processor(
            1,
            PathBuf::from("ffmpeg"),
            Arc::new(move |_, task: &ThumbnailTask| {
                *started_clone.lock().expect("started lock") = true;
                loop {
                    if *release_clone.lock().expect("release lock") {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                // Simulate a successful render of the OLD version.
                if let Some(parent) = task.target_path.parent() {
                    fs::create_dir_all(parent).expect("mkdir");
                }
                fs::write(&task.target_path, b"stale-render").expect("render");
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );

        let state = AppState {
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: scheduler,
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: AtomicU64::new(0),
            thumbnail_latest_request_id: AtomicU64::new(0),
        };

        let handle = std::thread::spawn(move || {
            let state_ref = as_state(&state);
            ensure_asset_thumbnail(1, &state_ref)
        });

        while !*started.lock().expect("started lock") {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        // Scan re-indexes the same file (same second, new fingerprint) while
        // the old-version render is still in flight.
        let conn = db::open_connection(&db_path).expect("reopen db");
        insert_scanned_asset(&conn, &source_path, 22, 5, 5_999_999_999);
        drop(conn);

        *release.lock().expect("release lock") = true;

        let result = handle.join().expect("demand thread").expect("no error");
        assert_eq!(
            result, None,
            "a result rendered for a superseded version must not be returned"
        );

        let conn = db::open_connection(&db_path).expect("reopen db");
        let asset = db::get_asset_for_thumbnail(&conn, 1)
            .expect("read asset")
            .expect("asset exists");
        assert_eq!(asset.thumb_path, None, "stale path must not be stored");
        drop(conn);

        let stale_target = crate::thumbs::thumb_target(
            &thumbs_dir,
            &crate::thumbs::SourceVersion::new(
                source_path.to_string_lossy().to_string(),
                11,
                5_000_000_000,
            ),
        );
        assert!(
            !stale_target.exists(),
            "the produced stale-version target must be cleaned up"
        );
    }

    #[test]
    fn wait_next_render_result_times_out_when_no_result_arrives() {
        let cancel_requested = AtomicBool::new(false);
        let (_sender, receiver) = mpsc::channel::<ThumbnailTaskResult>();

        let error = wait_next_render_result_with_stall(
            &receiver,
            &cancel_requested,
            std::time::Duration::from_millis(120),
        )
        .expect_err("stall must produce a controlled error");

        assert!(
            error.to_string().contains("timed out"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn publish_updates_drops_everything_after_generation_reset() {
        let conn = rusqlite::Connection::open_in_memory().expect("db");
        db::init_schema(&conn).expect("schema");

        let version = crate::thumbs::SourceVersion::new("C:/media/a.jpg", 10, 0);
        let updates = vec![(1_i64, Some("C:/thumbs/a.jpg".to_string()), version.clone())];

        // Same generation: normal CAS commit (row missing -> reported stale).
        let stale =
            publish_thumbnail_updates(&conn, &updates, 7, 7).expect("publish same generation");
        assert_eq!(stale, vec![1]);

        // Generation advanced meanwhile: nothing is committed at all.
        let stale = publish_thumbnail_updates(&conn, &updates, 8, 7).expect("publish after reset");
        assert_eq!(stale, vec![1]);
    }
}
