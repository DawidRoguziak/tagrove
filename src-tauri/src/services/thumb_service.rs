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

use crate::{
    app::{locks::thumb_read_section, state::AppState},
    db::{self, ThumbnailWrite, ThumbnailWriteOutcome},
    error::AppResult,
    models::{
        ThumbnailBatchItem, ThumbnailBatchResult, ThumbnailRenderSummary, ThumbnailStreamEvent,
    },
    services::{
        db_pool::OperationPermit,
        progress::emit_progress,
        thumb_scheduler::{ThumbnailPriority, ThumbnailTask, ThumbnailTaskResult},
    },
    thumbs,
};

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
/// Most demand results published in one transaction. A batch holds whatever
/// is already finished when the first result arrives, so it adds no delay.
const DEMAND_PUBLISH_BATCH: usize = 64;
/// Rows per bulk publication transaction; background bulk work yields to
/// user metadata writes between these transactions.
const BULK_PUBLISH_CHUNK: usize = 256;
const SOURCE_MISSING: &str = "source file not found";
const RENDER_FAILED: &str = "thumbnail generation failed";

#[derive(Clone, Copy)]
enum BulkRenderMode {
    All,
    FailedOnly,
}

struct PendingRenderTask {
    asset_path: String,
    record_version: i64,
    version: crate::thumbs::SourceVersion,
}

struct PendingPageTask {
    record_version: i64,
    version: crate::thumbs::SourceVersion,
}

struct BulkRenderStateGuard<'a>(&'a AppState);

impl Drop for BulkRenderStateGuard<'_> {
    fn drop(&mut self) {
        self.0
            .thumbnail_render_all_cancel_requested
            .store(false, Ordering::SeqCst);
        self.0
            .thumbnail_render_all_running
            .store(false, Ordering::SeqCst);
    }
}

fn source_version_for(asset: &crate::models::ThumbnailAsset) -> crate::thumbs::SourceVersion {
    crate::thumbs::SourceVersion::new(
        asset.path.clone(),
        asset.size_bytes,
        asset.fingerprint_mtime_ns,
    )
}

fn ready_write(
    asset_id: i64,
    path: String,
    version: crate::thumbs::SourceVersion,
    record_version: i64,
) -> ThumbnailWrite {
    ThumbnailWrite {
        asset_id,
        version,
        record_version,
        outcome: ThumbnailWriteOutcome::Ready(path),
    }
}

fn failed_write(
    asset_id: i64,
    version: crate::thumbs::SourceVersion,
    record_version: i64,
    error: &'static str,
) -> ThumbnailWrite {
    ThumbnailWrite {
        asset_id,
        version,
        record_version,
        outcome: ThumbnailWriteOutcome::Failed(error),
    }
}

pub fn render_all_thumbnails(
    state: &AppState,
    permit: &OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
) -> AppResult<ThumbnailRenderSummary> {
    render_bulk_thumbnails(state, permit, app, BulkRenderMode::All)
}

pub fn render_failed_thumbnails(
    state: &AppState,
    permit: &OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
) -> AppResult<ThumbnailRenderSummary> {
    render_bulk_thumbnails(state, permit, app, BulkRenderMode::FailedOnly)
}

pub fn cancel_render_all_thumbnails(state: &AppState) -> AppResult<bool> {
    let running = state.thumbnail_render_all_running.load(Ordering::SeqCst);
    if running {
        state
            .thumbnail_render_all_cancel_requested
            .store(true, Ordering::SeqCst);
    }
    Ok(running)
}

fn render_bulk_thumbnails(
    state: &AppState,
    permit: &OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
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

    // Drop the scheduler scope before allowing another bulk coordinator in.
    let _running = BulkRenderStateGuard(state);
    let _bulk = state.thumb_scheduler.enter_bulk_mode()?;
    render_bulk_thumbnails_inner(state, permit, app, mode)
}

fn render_bulk_thumbnails_inner(
    state: &AppState,
    permit: &OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
    mode: BulkRenderMode,
) -> AppResult<ThumbnailRenderSummary> {
    let (ceiling, total): (i64, usize) = {
        let _thumbs = thumb_read_section(state);
        permit.connection()?.query_row(
            "SELECT COALESCE(MAX(a.id), 0), COUNT(*) FROM assets a WHERE ?1 = 0 OR EXISTS (SELECT 1 FROM thumbnail_failures tf WHERE tf.asset_id = a.id AND tf.source_record_version = a.record_version)",
            [matches!(mode, BulkRenderMode::FailedOnly)], |row| Ok((row.get(0)?, row.get(1)?)))?
    };
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
    let mut writes = Vec::<ThumbnailWrite>::new();
    let mut pending = HashMap::<i64, PendingRenderTask>::new();
    let (completion_tx, completion_rx) = mpsc::channel::<ThumbnailTaskResult>();

    let in_flight_limit = state.thumb_scheduler.bulk_in_flight_limit();
    let candidate_page_size = 512.max(in_flight_limit);

    let mut stale = 0usize;
    let mut after = 0;
    loop {
        // Each page captures the clear epoch with its rows: a clear between
        // pages drops only the pages that started before it.
        let (candidates, generation) = {
            let _thumbs = thumb_read_section(state);
            (
                db::thumbnail_candidates_page(
                    &*permit.connection()?,
                    after,
                    ceiling,
                    candidate_page_size,
                    matches!(mode, BulkRenderMode::FailedOnly),
                )?,
                state.thumbnail_generation.load(Ordering::SeqCst),
            )
        };
        if candidates.is_empty() {
            break;
        }
        after = candidates.last().expect("nonempty page").0.id;
        for (asset, previously_failed) in candidates {
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
                writes.push(failed_write(
                    asset.id,
                    version,
                    asset.record_version,
                    SOURCE_MISSING,
                ));
                if let Some(existing) = asset.thumb_path {
                    let _thumbs = thumb_read_section(state);
                    let _ = remove_thumbnail_file(&state.thumbs_dir, Path::new(&existing));
                }
                emit_render_progress(app, mode.progress_phase(), processed, total);
                continue;
            }

            if target.exists() {
                generated += 1;
                processed += 1;
                writes.push(ready_write(
                    asset.id,
                    target.to_string_lossy().to_string(),
                    version,
                    asset.record_version,
                ));
                emit_render_progress(app, mode.progress_phase(), processed, total);
                continue;
            }

            if matches!(mode, BulkRenderMode::All) && previously_failed {
                skipped_failed += 1;
                processed += 1;
                emit_render_progress(app, mode.progress_phase(), processed, total);
                continue;
            }

            state.thumb_scheduler.enqueue_with_sender(
                ThumbnailTask {
                    operation: Some(permit.clone()),
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
                    record_version: asset.record_version,
                    version,
                },
            );

            while pending.len() >= in_flight_limit {
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
                    app,
                    mode,
                    total,
                    &mut processed,
                    &mut generated,
                    &mut failed,
                    &mut writes,
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
                app,
                mode,
                total,
                &completion_rx,
                &mut pending,
                &mut processed,
                &mut generated,
                &mut failed,
                &mut writes,
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
                    app,
                    mode,
                    total,
                    &mut processed,
                    &mut generated,
                    &mut failed,
                    &mut writes,
                    task,
                    result,
                );
            }
        }

        // Paths and failure markers commit together. A clear (epoch bump)
        // since this page was read drops every collected result, and
        // compare-and-set losers against a re-index are dropped too; their
        // produced files are removed.
        let stale_ids = publish_bulk_results(state, permit, &writes, generation)?;
        for id in &stale_ids {
            if writes.iter().any(|write| {
                write.asset_id == *id && matches!(write.outcome, ThumbnailWriteOutcome::Ready(_))
            }) {
                generated -= 1;
            } else {
                failed -= 1;
            }
        }
        stale += stale_ids.len();
        writes.clear();
        if cancelled {
            break;
        }
    }

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
        stale,
        generated,
        failed,
        skipped_failed,
        processed,
        total,
        cancelled,
    })
}

/// Commits one bulk page in bounded transactions, each under the shared
/// thumbnail guard, yielding to user metadata writes before each one.
fn publish_bulk_results(
    state: &AppState,
    permit: &OperationPermit,
    writes: &[ThumbnailWrite],
    captured_generation: u64,
) -> AppResult<Vec<i64>> {
    let mut stale = Vec::new();
    for chunk in writes.chunks(BULK_PUBLISH_CHUNK) {
        permit.yield_to_user_writes();
        let _thumbs = thumb_read_section(state);
        stale.extend(publish_thumbnail_results(
            &*permit.connection()?,
            chunk,
            state.thumbnail_generation.load(Ordering::SeqCst),
            captured_generation,
        )?);
    }
    Ok(stale)
}

#[allow(clippy::too_many_arguments)]
fn process_bulk_result(
    app: &impl crate::services::progress::ProgressSink,
    mode: BulkRenderMode,
    total: usize,
    processed: &mut usize,
    generated: &mut usize,
    failed: &mut usize,
    writes: &mut Vec<ThumbnailWrite>,
    task: PendingRenderTask,
    result: ThumbnailTaskResult,
) {
    *processed += 1;
    if let Some(path) = result.thumb_path {
        *generated += 1;
        writes.push(ready_write(
            result.asset_id,
            path,
            task.version,
            task.record_version,
        ));
    } else {
        *failed += 1;
        writes.push(failed_write(
            result.asset_id,
            task.version,
            task.record_version,
            RENDER_FAILED,
        ));
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
    app: &impl crate::services::progress::ProgressSink,
    mode: BulkRenderMode,
    total: usize,
    completion_receiver: &Receiver<ThumbnailTaskResult>,
    pending: &mut HashMap<i64, PendingRenderTask>,
    processed: &mut usize,
    generated: &mut usize,
    failed: &mut usize,
    writes: &mut Vec<ThumbnailWrite>,
) {
    while let Ok(result) = completion_receiver.try_recv() {
        let Some(task) = pending.remove(&result.asset_id) else {
            continue;
        };
        process_bulk_result(
            app, mode, total, processed, generated, failed, writes, task, result,
        );
    }
}

/// Publishes one result; returns the path when a ready result was accepted.
fn publish_one(
    state: &AppState,
    conn: &rusqlite::Connection,
    write: ThumbnailWrite,
    captured_generation: u64,
) -> AppResult<Option<String>> {
    let path = match &write.outcome {
        ThumbnailWriteOutcome::Ready(path) => Some(path.clone()),
        ThumbnailWriteOutcome::Failed(_) => None,
    };
    let stale = publish_thumbnail_results(
        conn,
        std::slice::from_ref(&write),
        state.thumbnail_generation.load(Ordering::SeqCst),
        captured_generation,
    )?;
    Ok(if stale.is_empty() { path } else { None })
}

pub fn ensure_asset_thumbnail(
    asset_id: i64,
    state: &AppState,
    permit: &OperationPermit,
) -> AppResult<Option<String>> {
    let (receiver, version, record_version, generation) = {
        let _thumbs = thumb_read_section(state);
        let generation = state.thumbnail_generation.load(Ordering::SeqCst);
        let conn = permit.connection()?;
        let Some((asset, has_failure)) =
            db::get_assets_for_thumbnails_by_ids(&conn, &[asset_id])?.pop()
        else {
            return Ok(None);
        };
        let version = source_version_for(&asset);

        if let Some(existing) = asset.thumb_path.clone() {
            if Path::new(&existing).exists() {
                // Path and version come from one snapshot: nothing to write
                // unless a stale failure marker must be cleared.
                if !has_failure {
                    return Ok(Some(existing));
                }
                let write = ready_write(asset.id, existing, version, asset.record_version);
                return publish_one(state, &conn, write, generation);
            }
        }

        let source_path = PathBuf::from(&asset.path);
        if !source_path.exists() {
            let write = failed_write(asset.id, version, asset.record_version, SOURCE_MISSING);
            publish_one(state, &conn, write, generation)?;
            if let Some(existing) = asset.thumb_path {
                let _ = remove_thumbnail_file(&state.thumbs_dir, Path::new(&existing));
            }
            return Ok(None);
        }

        let target = thumbs::thumb_target(&state.thumbs_dir, &version);
        if target.exists() {
            let write = ready_write(
                asset.id,
                target.to_string_lossy().to_string(),
                version,
                asset.record_version,
            );
            return publish_one(state, &conn, write, generation);
        }

        let receiver = state.thumb_scheduler.enqueue(
            ThumbnailTask {
                operation: Some(permit.clone()),
                asset_id: asset.id,
                source_path,
                target_path: target,
                kind: asset.kind,
                duration_ms: asset.duration_ms,
                source_version: version.clone(),
            },
            ThumbnailPriority::High,
        )?;
        (receiver, version, asset.record_version, generation)
    };

    // Bounded wait without the thumbnail guard: a lost worker or wedged
    // scheduler must surface as an error instead of hanging the command.
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

    // A result for a superseded version (re-index, rename, delete) loses its
    // compare-and-set; its produced file is removed and a later demand redoes it.
    let write = match result.thumb_path {
        Some(path) => ready_write(asset_id, path, version, record_version),
        None => failed_write(asset_id, version, record_version, RENDER_FAILED),
    };
    let _thumbs = thumb_read_section(state);
    publish_one(state, &*permit.connection()?, write, generation)
}

pub fn ensure_page_thumbnails(
    asset_ids: Vec<i64>,
    state: &AppState,
    permit: &OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
) -> AppResult<ThumbnailBatchResult> {
    let high_priority = asset_ids.iter().copied().collect::<HashSet<_>>();
    ensure_page_thumbnails_with_sink(asset_ids, &high_priority, state, permit, app, |item| {
        let _ = app.thumbnail_ready(item);
    })
}

pub fn ensure_thumbnails_stream(
    _request_id: u64,
    visible_ids: Vec<i64>,
    prefetch_ids: Vec<i64>,
    state: &AppState,
    permit: &OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
    mut send: impl FnMut(ThumbnailStreamEvent),
) -> AppResult<()> {
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
    let result =
        ensure_page_thumbnails_with_sink(ids, &high_priority, state, permit, app, |item| {
            send(ThumbnailStreamEvent::Ready(item.clone()));
        })?;
    for asset_id in &result.failed {
        send(ThumbnailStreamEvent::Failed {
            asset_id: *asset_id,
        });
    }
    for asset_id in &result.stale {
        send(ThumbnailStreamEvent::Stale {
            asset_id: *asset_id,
        });
    }
    send(ThumbnailStreamEvent::Done {
        stale: result.stale.len(),
        ready: result.ready.len(),
        failed: result.failed.len(),
    });
    Ok(())
}

/// Collected outcomes of one demand call, in publication order.
#[derive(Default)]
struct DemandOutcomes {
    ready: Vec<ThumbnailBatchItem>,
    failed: Vec<i64>,
    stale: Vec<i64>,
}

impl DemandOutcomes {
    /// Sorts committed writes into ready/failed/stale; `ready` is announced
    /// only after its version-checked commit.
    fn record(
        &mut self,
        writes: Vec<ThumbnailWrite>,
        stale_ids: &[i64],
        on_ready: &mut impl FnMut(&ThumbnailBatchItem),
    ) {
        for write in writes {
            if stale_ids.contains(&write.asset_id) {
                self.stale.push(write.asset_id);
                continue;
            }
            match write.outcome {
                ThumbnailWriteOutcome::Ready(thumb_path) => {
                    self.ready_now(
                        ThumbnailBatchItem {
                            asset_id: write.asset_id,
                            thumb_path,
                        },
                        on_ready,
                    );
                }
                ThumbnailWriteOutcome::Failed(_) => self.failed.push(write.asset_id),
            }
        }
    }

    fn ready_now(
        &mut self,
        item: ThumbnailBatchItem,
        on_ready: &mut impl FnMut(&ThumbnailBatchItem),
    ) {
        on_ready(&item);
        self.ready.push(item);
    }
}

fn ensure_page_thumbnails_with_sink(
    asset_ids: Vec<i64>,
    high_priority: &HashSet<i64>,
    state: &AppState,
    permit: &OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
    mut on_ready: impl FnMut(&ThumbnailBatchItem),
) -> AppResult<ThumbnailBatchResult> {
    let started = std::time::Instant::now();
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
    let mut outcomes = DemandOutcomes::default();
    let mut pending = HashMap::<i64, PendingPageTask>::new();
    let (completion_tx, completion_rx) = mpsc::channel::<ThumbnailTaskResult>();

    // One guarded section reads the snapshot, publishes everything that is
    // already decided, and enqueues the rest. Rendering is awaited unguarded.
    let mut existing_ready = Vec::new();
    let mut writes = Vec::new();
    let generation = {
        let _thumbs = thumb_read_section(state);
        let generation = state.thumbnail_generation.load(Ordering::SeqCst);
        let conn = permit.connection()?;
        let mut assets_by_id = db::get_assets_for_thumbnails_by_ids(&conn, &ids)?
            .into_iter()
            .map(|entry| (entry.0.id, entry))
            .collect::<HashMap<_, _>>();

        for asset_id in &ids {
            let Some((asset, has_failure)) = assets_by_id.remove(asset_id) else {
                outcomes.failed.push(*asset_id);
                processed += 1;
                emit_page_progress(app, processed, total);
                continue;
            };
            let version = source_version_for(&asset);

            if let Some(existing) = asset.thumb_path.clone() {
                if Path::new(&existing).exists() {
                    // The stored path was read with its source version in one
                    // snapshot, so it is ready without a write unless a stale
                    // failure marker must be cleared.
                    if has_failure {
                        writes.push(ready_write(
                            asset.id,
                            existing,
                            version,
                            asset.record_version,
                        ));
                    } else {
                        existing_ready.push(ThumbnailBatchItem {
                            asset_id: asset.id,
                            thumb_path: existing,
                        });
                    }
                    processed += 1;
                    emit_page_progress(app, processed, total);
                    continue;
                }
            }

            let source_path = PathBuf::from(&asset.path);
            if !source_path.exists() {
                writes.push(failed_write(
                    asset.id,
                    version,
                    asset.record_version,
                    SOURCE_MISSING,
                ));
                processed += 1;
                emit_page_progress(app, processed, total);
                continue;
            }

            let target = thumbs::thumb_target(&state.thumbs_dir, &version);
            if target.exists() {
                writes.push(ready_write(
                    asset.id,
                    target.to_string_lossy().to_string(),
                    version,
                    asset.record_version,
                ));
                processed += 1;
                emit_page_progress(app, processed, total);
                continue;
            }

            state.thumb_scheduler.enqueue_with_sender(
                ThumbnailTask {
                    operation: Some(permit.clone()),
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
                    record_version: asset.record_version,
                    version,
                },
            );
        }

        let stale_ids = publish_thumbnail_results(
            &conn,
            &writes,
            state.thumbnail_generation.load(Ordering::SeqCst),
            generation,
        )?;
        drop(conn);
        drop(_thumbs);
        for item in existing_ready {
            outcomes.ready_now(item, &mut on_ready);
        }
        outcomes.record(std::mem::take(&mut writes), &stale_ids, &mut on_ready);
        generation
    };

    // Bounded stall wait mirrors the bulk coordinator: every accepted job
    // ends with a result or a controlled error, never an infinite block.
    let mut last_progress = Instant::now();
    while !pending.is_empty() {
        match completion_rx.recv_timeout(Duration::from_millis(RESULT_WAIT_TIMEOUT_MS)) {
            Ok(first) => {
                last_progress = Instant::now();
                // Publish this result together with every result that is
                // already waiting, in one transaction.
                let mut next = Some(first);
                while let Some(result) = next.take() {
                    if let Some(task) = pending.remove(&result.asset_id) {
                        processed += 1;
                        writes.push(match result.thumb_path {
                            Some(path) => ready_write(
                                result.asset_id,
                                path,
                                task.version,
                                task.record_version,
                            ),
                            None => failed_write(
                                result.asset_id,
                                task.version,
                                task.record_version,
                                RENDER_FAILED,
                            ),
                        });
                        emit_page_progress(app, processed, total);
                    }
                    if writes.len() < DEMAND_PUBLISH_BATCH {
                        next = completion_rx.try_recv().ok();
                    }
                }
                let stale_ids = {
                    let _thumbs = thumb_read_section(state);
                    publish_thumbnail_results(
                        &*permit.connection()?,
                        &writes,
                        state.thumbnail_generation.load(Ordering::SeqCst),
                        generation,
                    )?
                };
                outcomes.record(std::mem::take(&mut writes), &stale_ids, &mut on_ready);
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

    let DemandOutcomes {
        ready,
        failed,
        stale,
    } = outcomes;

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
    Ok(ThumbnailBatchResult {
        ready,
        failed,
        stale,
    })
}

/// Publishes collected thumbnail results: paths and failure markers in one
/// transaction, each guarded by the source version captured at enqueue time.
/// When the generation advanced meanwhile (thumbnail reset), nothing is
/// committed and every ID is reported stale. Produced files of stale results
/// are removed best-effort. Callers hold `thumb_read_section`.
fn publish_thumbnail_results(
    conn: &rusqlite::Connection,
    writes: &[ThumbnailWrite],
    current_generation: u64,
    captured_generation: u64,
) -> anyhow::Result<Vec<i64>> {
    if writes.is_empty() {
        return Ok(Vec::new());
    }
    let stale = if current_generation != captured_generation {
        writes.iter().map(|write| write.asset_id).collect()
    } else {
        db::commit_thumbnail_results(conn, writes)?
    };
    for write in writes {
        if let ThumbnailWriteOutcome::Ready(path) = &write.outcome {
            if stale.contains(&write.asset_id) {
                let _ = fs::remove_file(path);
            }
        }
    }
    Ok(stale)
}

fn emit_render_progress(
    app: &impl crate::services::progress::ProgressSink,
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

fn emit_page_progress(
    app: &impl crate::services::progress::ProgressSink,
    processed: usize,
    total: usize,
) {
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

pub fn clear_all_thumbnails(
    state: &AppState,
    permit: &crate::services::db_pool::OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
) -> AppResult<usize> {
    // Invalidate every in-flight render before removing anything: results
    // completing after this point are dropped instead of written back.
    state.thumbnail_generation.fetch_add(1, Ordering::SeqCst);
    let conn = permit.connection()?;
    db::clear_all_thumbnail_paths(&conn)?;
    // Rescans and pruning can release a reference before its old cache file is
    // removed. Sweep the owned directory, without following directory symlinks.
    let mut thumbs = Vec::new();
    let mut directories = Vec::new();
    for entry in walkdir::WalkDir::new(&state.thumbs_dir)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            thumbs.push(entry.path().to_string_lossy().into_owned());
        } else if entry.file_type().is_dir() && entry.depth() > 0 {
            directories.push(entry.into_path());
        }
    }
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
    for directory in directories {
        // Only empty directories are removed; failed file cleanup remains retryable.
        let _ = fs::remove_dir(directory);
    }
    Ok(removed)
}

pub fn delete_thumbnail_files_in_root(thumbs_root: &Path, paths: Vec<String>) -> usize {
    paths
        .into_iter()
        .filter(|path| remove_thumbnail_file(thumbs_root, Path::new(path)))
        .count()
}

pub fn delete_thumbnail_files_with_progress(
    app: &impl crate::services::progress::ProgressSink,
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
            atomic::{AtomicBool, AtomicU64, Ordering},
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
        cancel_render_all_thumbnails, delete_thumbnail_files, delete_thumbnail_files_in_root,
        ensure_asset_thumbnail, publish_thumbnail_results, wait_next_render_result_with_stall,
        ThumbnailTaskResult,
    };

    fn create_test_state(db_path: &Path, thumbs_dir: &Path) -> AppState {
        AppState {
            database: crate::services::db_pool::DatabaseRuntime::new(db_path.to_path_buf()),
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: std::sync::atomic::AtomicU64::new(0),
        }
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
        let state_ref = &state;
        let result = ensure_asset_thumbnail(1, state_ref, &state.database.admit().unwrap())
            .expect("ensure thumb");

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
        let state_ref = &state;
        let accepted = cancel_render_all_thumbnails(state_ref).expect("cancel result");
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

        let state_ref = &state;
        let accepted = cancel_render_all_thumbnails(state_ref).expect("cancel result");
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

    #[test]
    fn delete_thumbnail_files_does_not_remove_file_outside_thumbnail_root() {
        let tmp = tempdir().expect("tempdir");
        let thumbs = tmp.path().join("thumbs");
        let outside = tmp.path().join("outside.jpg");
        fs::create_dir_all(&thumbs).expect("create thumbs");
        fs::write(&outside, b"outside").expect("write outside file");

        let removed =
            delete_thumbnail_files_in_root(&thumbs, vec![outside.to_string_lossy().into_owned()]);

        assert_eq!(removed, 0);
        assert_eq!(fs::read(&outside).expect("outside survives"), b"outside");
    }

    fn controlled_bulk_state(
        retry: bool,
    ) -> (
        tempfile::TempDir,
        Arc<AppState>,
        mpsc::Receiver<crate::services::thumb_scheduler::test_support::StartedJob>,
    ) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        for id in 0..60 {
            let source = dir.path().join(format!("source-{id}.png"));
            fs::write(&source, b"controlled processor fixture").unwrap();
            db::upsert_asset(&conn, &new_asset(&source, 1, None)).unwrap();
        }
        if retry {
            for id in 1..=60 {
                db::record_thumbnail_failure(&conn, id, 1, Some("retry fixture")).unwrap();
            }
        }
        let mut state = create_test_state(&db_path, &dir.path().join("thumbs"));
        let (scheduler, started) =
            crate::services::thumb_scheduler::test_support::controlled_scheduler(16);
        state.thumb_scheduler = scheduler;
        (dir, Arc::new(state), started)
    }

    fn assert_browsing_admission(
        state: &AppState,
        started: &mpsc::Receiver<crate::services::thumb_scheduler::test_support::StartedJob>,
    ) {
        use crate::services::thumb_scheduler::{test_support::WAIT, ThumbnailPriority};
        for (kind, limit) in [("image", 8), ("video", 2)] {
            let mut results = Vec::new();
            for id in 0..12 {
                results.push(
                    state
                        .thumb_scheduler
                        .enqueue(
                            ThumbnailTask {
                                operation: None,
                                asset_id: id,
                                source_path: "browsing-source".into(),
                                target_path: format!("browsing-{kind}-{id}.jpg").into(),
                                kind: kind.into(),
                                duration_ms: None,
                                source_version: crate::thumbs::SourceVersion::new(
                                    "browsing-source",
                                    1,
                                    0,
                                ),
                            },
                            ThumbnailPriority::High,
                        )
                        .unwrap(),
                );
            }
            let running: Vec<_> = (0..limit)
                .map(|_| started.recv_timeout(WAIT).unwrap())
                .collect();
            assert!(started
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err());
            for job in running {
                job.release.send(()).unwrap();
            }
            for _ in limit..12 {
                started
                    .recv_timeout(WAIT)
                    .unwrap()
                    .release
                    .send(())
                    .unwrap();
            }
            for result in results {
                result.recv_timeout(WAIT).unwrap();
            }
        }
        assert!(!state.thumbnail_render_all_running.load(Ordering::SeqCst));
        assert!(!state
            .thumbnail_render_all_cancel_requested
            .load(Ordering::SeqCst));
    }

    #[test]
    fn both_bulk_actions_expand_submission_window_and_restore_browsing_after_success() {
        use crate::services::thumb_scheduler::test_support::{wait_for_pending, WAIT};
        for retry in [false, true] {
            let (_dir, state, started) = controlled_bulk_state(retry);
            let worker_state = Arc::clone(&state);
            let (done_tx, done_rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let permit = worker_state.database.admit().unwrap();
                let render = if retry {
                    super::render_failed_thumbnails
                } else {
                    super::render_all_thumbnails
                };
                done_tx
                    .send(render(&worker_state, &permit, &|_| Ok(())))
                    .unwrap();
            });
            wait_for_pending(&state.thumb_scheduler, 48);
            let running: Vec<_> = (0..16)
                .map(|_| started.recv_timeout(WAIT).unwrap())
                .collect();
            assert!(started
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err());
            for job in running {
                job.release.send(()).unwrap();
            }
            for _ in 16..60 {
                started
                    .recv_timeout(WAIT)
                    .unwrap()
                    .release
                    .send(())
                    .unwrap();
            }
            let summary = done_rx.recv_timeout(WAIT).unwrap().unwrap();
            worker.join().unwrap();
            assert_eq!(
                (summary.generated, summary.processed, summary.failed),
                (60, 60, 0)
            );
            assert!(!summary.cancelled);
            assert_browsing_admission(&state, &started);
        }
    }

    #[test]
    fn bulk_cancellation_restores_browsing_before_remaining_jobs_finish() {
        use crate::services::thumb_scheduler::test_support::{wait_for_pending, WAIT};
        let (_dir, state, started) = controlled_bulk_state(false);
        let worker_state = Arc::clone(&state);
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let permit = worker_state.database.admit().unwrap();
            done_tx
                .send(super::render_all_thumbnails(
                    &worker_state,
                    &permit,
                    &|_| Ok(()),
                ))
                .unwrap();
        });
        wait_for_pending(&state.thumb_scheduler, 48);
        let mut running: Vec<_> = (0..16)
            .map(|_| started.recv_timeout(WAIT).unwrap())
            .collect();
        let permit = state.database.admit().unwrap();
        assert!(super::render_failed_thumbnails(&state, &permit, &|_| Ok(())).is_err());
        assert!(cancel_render_all_thumbnails(&state).unwrap());
        let summary = done_rx.recv_timeout(WAIT).unwrap().unwrap();
        worker.join().unwrap();
        assert!(summary.cancelled);
        assert_eq!(summary.processed, 0);
        for job in running.drain(..8) {
            job.release.send(()).unwrap();
        }
        assert!(started
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        running.pop().unwrap().release.send(()).unwrap();
        let next = started.recv_timeout(WAIT).unwrap();
        assert!(started
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        next.release.send(()).unwrap();
        for job in running {
            job.release.send(()).unwrap();
        }
        for _ in 17..48 {
            started
                .recv_timeout(WAIT)
                .unwrap()
                .release
                .send(())
                .unwrap();
        }
        wait_for_pending(&state.thumb_scheduler, 0);
        assert_browsing_admission(&state, &started);
    }

    #[test]
    fn bulk_error_and_unwind_restore_browsing_and_running_flags() {
        for panic in [false, true] {
            let (_dir, state, started) = controlled_bulk_state(false);
            let permit = state.database.admit().unwrap();
            let sink = |_: crate::models::ScanProgress| -> crate::error::AppResult<()> {
                if panic {
                    panic!("injected coordinator panic");
                }
                permit
                    .connection()
                    .unwrap()
                    .execute("DROP TABLE assets", [])
                    .unwrap();
                Ok(())
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::render_all_thumbnails(&state, &permit, &sink)
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            assert_browsing_admission(&state, &started);
        }
    }

    #[test]
    fn bulk_enqueue_error_restores_limits_while_shared_jobs_are_running() {
        use crate::services::thumb_scheduler::{
            test_support::{wait_for_pending, WAIT},
            ThumbnailPriority, MAX_PENDING_JOBS,
        };
        let (_dir, state, started) = controlled_bulk_state(false);
        for id in 0..MAX_PENDING_JOBS {
            state
                .thumb_scheduler
                .enqueue(
                    ThumbnailTask {
                        operation: None,
                        asset_id: id as i64,
                        source_path: "shared-source".into(),
                        target_path: format!("shared-{id}.jpg").into(),
                        kind: "video".into(),
                        duration_ms: None,
                        source_version: crate::thumbs::SourceVersion::new("shared-source", 1, 0),
                    },
                    ThumbnailPriority::Low,
                )
                .unwrap();
        }
        let mut running: Vec<_> = (0..2)
            .map(|_| started.recv_timeout(WAIT).unwrap())
            .collect();
        let extra = Mutex::new(Vec::new());
        let sink = |_: crate::models::ScanProgress| -> crate::error::AppResult<()> {
            // Wait until the bulk scope has woken all 16 workers before the
            // coordinator hits its first enqueue against the full scheduler.
            extra
                .lock()
                .unwrap()
                .extend((2..16).map(|_| started.recv_timeout(WAIT).unwrap()));
            Ok(())
        };
        let permit = state.database.admit().unwrap();
        assert!(super::render_all_thumbnails(&state, &permit, &sink)
            .unwrap_err()
            .to_string()
            .contains("full"));
        running.extend(extra.into_inner().unwrap());
        for job in running.drain(..14) {
            job.release.send(()).unwrap();
        }
        assert!(started
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        running.pop().unwrap().release.send(()).unwrap();
        let next = started.recv_timeout(WAIT).unwrap();
        assert!(started
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        next.release.send(()).unwrap();
        running.pop().unwrap().release.send(()).unwrap();
        for _ in 17..MAX_PENDING_JOBS {
            started
                .recv_timeout(WAIT)
                .unwrap()
                .release
                .send(())
                .unwrap();
        }
        wait_for_pending(&state.thumb_scheduler, 0);
        assert_browsing_admission(&state, &started);
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
            database: crate::services::db_pool::DatabaseRuntime::new(db_path.to_path_buf()),
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: scheduler,
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: AtomicU64::new(0),
        };

        let handle = std::thread::spawn(move || {
            let state_ref = &state;
            ensure_asset_thumbnail(1, state_ref, &state.database.admit().unwrap())
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
        let updates = vec![super::ready_write(
            1,
            "C:/thumbs/a.jpg".to_string(),
            version.clone(),
            1,
        )];

        // Same generation: normal CAS commit (row missing -> reported stale).
        let stale =
            publish_thumbnail_results(&conn, &updates, 7, 7).expect("publish same generation");
        assert_eq!(stale, vec![1]);

        // Generation advanced meanwhile: nothing is committed at all.
        let stale = publish_thumbnail_results(&conn, &updates, 8, 7).expect("publish after reset");
        assert_eq!(stale, vec![1]);
    }
    #[test]
    fn stale_stream_result_is_never_ready_and_frontend_reload_can_retry() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let source = dir.path().join("source.png");
        fs::write(&source, b"source").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        insert_scanned_asset(&conn, &source, 6, 1, 1_000_000_000);
        drop(conn);
        let mut state = create_test_state(&db_path, &thumbs);
        let worker_db = db_path.clone();
        let first = std::sync::atomic::AtomicBool::new(true);
        state.thumb_scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                if first.swap(false, Ordering::SeqCst) {
                    let conn = db::open_connection(&worker_db).unwrap();
                    conn.execute("UPDATE assets SET fingerprint_mtime_ns = fingerprint_mtime_ns + 1, record_version = record_version + 1", []).unwrap();
                }
                fs::write(&task.target_path, b"thumbnail").unwrap();
                Some(task.target_path.to_string_lossy().into())
            }),
        );
        let permit = state.database.admit().unwrap();
        let mut events = Vec::new();
        super::ensure_thumbnails_stream(
            77,
            vec![1],
            vec![],
            &state,
            &permit,
            &|_| Ok(()),
            |event| events.push(event),
        )
        .unwrap();
        assert!(!events
            .iter()
            .any(|event| matches!(event, crate::models::ThumbnailStreamEvent::Ready(_))));
        assert!(events.iter().any(|event| matches!(
            event,
            crate::models::ThumbnailStreamEvent::Done {
                ready: 0,
                failed: 0,
                stale: 1
            }
        )));
        events.clear();
        super::ensure_thumbnails_stream(
            1,
            vec![1],
            vec![],
            &state,
            &permit,
            &|_| Ok(()),
            |event| events.push(event),
        )
        .unwrap();
        assert!(events.iter().any(|event| matches!(
            event,
            crate::models::ThumbnailStreamEvent::Done {
                ready: 1,
                failed: 0,
                stale: 0
            }
        )));
        assert!(
            db::get_asset_for_thumbnail(&permit.connection().unwrap(), 1)
                .unwrap()
                .unwrap()
                .thumb_path
                .is_some()
        );
    }

    #[test]
    fn cancellation_reaches_atomics_while_maintenance_and_workflow_locks_are_held() {
        let dir = tempdir().unwrap();
        let state = Arc::new(create_test_state(
            &dir.path().join("media.db"),
            &dir.path().join("thumbs"),
        ));
        state
            .thumbnail_render_all_running
            .store(true, Ordering::SeqCst);
        let maintenance = state.database.maintenance().unwrap();
        let scan = state.scan_lock.lock().unwrap();
        let thumb = state.thumb_lock.write().unwrap();
        let worker_state = state.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            tx.send(super::cancel_render_all_thumbnails(&worker_state).unwrap())
                .unwrap()
        });
        assert!(rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap());
        drop(thumb);
        drop(scan);
        drop(maintenance);
        worker.join().unwrap();
    }

    fn stream_events(
        state: &AppState,
        visible: Vec<i64>,
    ) -> crate::error::AppResult<Vec<crate::models::ThumbnailStreamEvent>> {
        let mut events = Vec::new();
        crate::app::locks::with_thumb_operation(state, |permit| {
            super::ensure_thumbnails_stream(
                1,
                visible,
                vec![],
                state,
                permit,
                &|_| Ok(()),
                |event| events.push(event),
            )
        })?;
        Ok(events)
    }

    fn done_counts(
        events: &[crate::models::ThumbnailStreamEvent],
    ) -> Option<(usize, usize, usize)> {
        events.iter().find_map(|event| match event {
            crate::models::ThumbnailStreamEvent::Done {
                ready,
                failed,
                stale,
            } => Some((*ready, *failed, *stale)),
            _ => None,
        })
    }

    #[test]
    fn waiting_exclusive_thumb_lock_does_not_stall_new_thumbnail_work() {
        use crate::services::thumb_scheduler::test_support::{controlled_scheduler, WAIT};
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        for name in ["slow.png", "next.png"] {
            let source = dir.path().join(name);
            fs::write(&source, b"source").unwrap();
            db::upsert_asset(&conn, &new_asset(&source, 1, None)).unwrap();
        }
        drop(conn);
        let mut state = create_test_state(&db_path, &dir.path().join("thumbs"));
        let (scheduler, started) = controlled_scheduler(16);
        state.thumb_scheduler = scheduler;
        let state = Arc::new(state);

        let slow_state = state.clone();
        let slow = std::thread::spawn(move || stream_events(&slow_state, vec![1]));
        let slow_job = started.recv_timeout(WAIT).unwrap();
        assert_eq!(slow_job.asset_id, 1);

        // Delete/rename/root removal take scan, then the exclusive thumb lock.
        let writer_state = state.clone();
        let (locked_tx, locked_rx) = mpsc::channel();
        let writer = std::thread::spawn(move || {
            crate::app::locks::with_scan_and_thumb_lock(&writer_state, |_| {
                locked_tx.send(()).unwrap();
                Ok(())
            })
            .unwrap();
        });
        locked_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("the exclusive writer must not wait for a running render");
        writer.join().unwrap();

        let next_state = state.clone();
        let next = std::thread::spawn(move || stream_events(&next_state, vec![2]));
        let next_job = started.recv_timeout(WAIT).unwrap();
        assert_eq!(next_job.asset_id, 2);
        next_job.release.send(()).unwrap();
        let next_events = next.join().unwrap().unwrap();
        assert_eq!(done_counts(&next_events), Some((1, 0, 0)));
        assert!(!slow.is_finished(), "the first render is still running");

        slow_job.release.send(()).unwrap();
        let slow_events = slow.join().unwrap().unwrap();
        assert_eq!(done_counts(&slow_events), Some((1, 0, 0)));
    }

    #[test]
    fn existing_thumbnail_is_ready_without_the_sqlite_write_lock() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let existing = thumbs.join("existing.jpg");
        fs::write(&existing, b"thumb").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(
            &conn,
            &new_asset(&dir.path().join("source.png"), 1, Some(&existing)),
        )
        .unwrap();
        let state = create_test_state(&db_path, &thumbs);

        // Another writer holds the write lock (as a scan batch would).
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let started = std::time::Instant::now();
        let events = stream_events(&state, vec![1]).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        conn.execute_batch("ROLLBACK").unwrap();
        assert_eq!(done_counts(&events), Some((1, 0, 0)));
    }

    #[test]
    fn rendered_result_clears_its_failure_marker_with_the_path() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let source = dir.path().join("source.png");
        fs::write(&source, b"source").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &new_asset(&source, 3, None)).unwrap();
        db::record_thumbnail_failure(&conn, 1, 3, Some("earlier failure")).unwrap();
        let mut state = create_test_state(&db_path, &thumbs);
        state.thumb_scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(|_, task| {
                fs::write(&task.target_path, b"thumbnail").unwrap();
                Some(task.target_path.to_string_lossy().into())
            }),
        );

        let events = stream_events(&state, vec![1]).unwrap();
        assert_eq!(done_counts(&events), Some((1, 0, 0)));
        assert!(db::get_asset_for_thumbnail(&conn, 1)
            .unwrap()
            .unwrap()
            .thumb_path
            .is_some());
        assert!(db::list_failed_thumbnail_asset_ids(&conn)
            .unwrap()
            .is_empty());
    }
}
