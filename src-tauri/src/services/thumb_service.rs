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
    models::ThumbnailRenderSummary,
    services::{
        db_pool::OperationPermit,
        progress::emit_progress,
        thumb_scheduler::{ThumbnailPriority, ThumbnailTask, ThumbnailTaskResult},
    },
    thumbs,
};

const RESULT_WAIT_TIMEOUT_MS: u64 = 50;
/// Maximum time a batch coordinator may see no completed result at all before
/// declaring the pipeline stalled (worker loss, poisoned queues) and failing.
const RESULT_STALL_TIMEOUT: Duration = Duration::from_secs(180);
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
    // `thumb_path` is untrusted: only a file owned by the thumbnail root is
    // kept for an unreachable source, so a stale publication of it can only
    // remove a cache file.
    let thumbs_root = state.thumbs_dir.canonicalize().ok();

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
        // Current thumbnails counted as generated without a write; a clear
        // since this page was read makes them stale like the written ones.
        let mut unwritten_ready = 0usize;
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
            // `exists` is false for an unmounted root and for EACCES alike.
            let source_reachable = source_path.exists();

            // An unreachable source keeps the recorded thumbnail the demand
            // path still serves: an offline drive or an unreadable folder must
            // not cost its cached thumbnails or leave failure markers that
            // make later runs skip it once the source is back.
            if !source_reachable {
                if let Some(existing) = asset.thumb_path.as_deref().filter(|existing| {
                    thumbs_root
                        .as_deref()
                        .is_some_and(|root| is_owned_thumbnail_file(root, Path::new(existing)))
                }) {
                    generated += 1;
                    processed += 1;
                    if previously_failed {
                        writes.push(ready_write(
                            asset.id,
                            existing.to_owned(),
                            version,
                            asset.record_version,
                        ));
                    } else {
                        unwritten_ready += 1;
                    }
                    emit_render_progress(app, mode.progress_phase(), processed, total);
                    continue;
                }
            }

            if target.exists() {
                generated += 1;
                processed += 1;
                let target = target.to_string_lossy();
                // An already-current row is not re-published: the write would
                // change nothing and still cost an IMMEDIATE transaction slot.
                if previously_failed || asset.thumb_path.as_deref() != Some(target.as_ref()) {
                    writes.push(ready_write(
                        asset.id,
                        target.into_owned(),
                        version,
                        asset.record_version,
                    ));
                } else {
                    unwritten_ready += 1;
                }
                emit_render_progress(app, mode.progress_phase(), processed, total);
                continue;
            }

            if !source_reachable {
                // No usable thumbnail is recorded, so there is no file to keep.
                failed += 1;
                processed += 1;
                writes.push(failed_write(
                    asset.id,
                    version,
                    asset.record_version,
                    SOURCE_MISSING,
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

            let legacy_thumb =
                legacy_thumbnail(asset.thumb_path.as_deref(), &target, thumbs_root.as_deref());
            state.thumb_scheduler.enqueue_with_sender(
                ThumbnailTask {
                    operation: Some(permit.clone()),
                    asset_id: asset.id,
                    source_path,
                    target_path: target,
                    kind: asset.kind,
                    duration_ms: asset.duration_ms,
                    source_version: version.clone(),
                    legacy_thumb,
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
        if state.thumbnail_generation.load(Ordering::SeqCst) != generation {
            generated -= unwritten_ready;
            stale += unwritten_ready;
        }
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
            &state.thumbs_dir,
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
        &state.thumbs_dir,
        std::slice::from_ref(&write),
        state.thumbnail_generation.load(Ordering::SeqCst),
        captured_generation,
    )?;
    Ok(if stale.is_empty() { path } else { None })
}

/// A thumbnail file read for one `thumb://` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedThumbnail {
    pub bytes: Vec<u8>,
    /// Token of the source version the bytes belong to.
    pub version: String,
}

/// Immediate outcome of a `thumb://` request.
pub enum ThumbnailRequestOutcome {
    Ready(ServedThumbnail),
    /// No asset, missing source, or a failure marker for this record version.
    NotFound,
    /// A render was enqueued; its result reaches the callback.
    Pending,
}

/// How a `thumb://` request may render.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ThumbnailRequestMode {
    /// Serve an existing file only (fast-scroll pause).
    pub cached_only: bool,
    /// Overscan tile: render at High (FIFO) behind every Interactive request.
    pub prefetch: bool,
}

/// A finished render waiting to be published and served.
pub struct PendingThumbnail {
    asset_id: i64,
    version: crate::thumbs::SourceVersion,
    record_version: i64,
    generation: u64,
    result: ThumbnailTaskResult,
    /// The recorded legacy path when the row had no failure marker: a result
    /// that keeps it (an unreachable source) needs no write.
    unchanged_path: Option<String>,
}

fn version_token(version: &crate::thumbs::SourceVersion) -> String {
    thumbs::thumb_version_token(&version.path, version.size_bytes, version.mtime_ns)
}

/// Reads a published thumbnail. Runs inside a thumbnail read section so clear,
/// delete and rename cannot remove the file between the check and the read.
fn read_served(path: &Path, version: &crate::thumbs::SourceVersion) -> Option<ServedThumbnail> {
    fs::read(path).ok().map(|bytes| ServedThumbnail {
        bytes,
        version: version_token(version),
    })
}

/// First phase of a `thumb://` request: serves an existing thumbnail from one
/// short read section, or enqueues an Interactive (LIFO) render whose result
/// is passed to `on_rendered` on a worker thread. Never waits for a render.
/// With `mode.cached_only` (fast-scroll pause) a missing thumbnail is
/// `NotFound` and nothing is written or enqueued.
pub fn request_thumbnail(
    asset_id: i64,
    mode: ThumbnailRequestMode,
    state: &AppState,
    permit: &OperationPermit,
    on_rendered: impl FnOnce(PendingThumbnail) + Send + 'static,
) -> AppResult<ThumbnailRequestOutcome> {
    let _thumbs = thumb_read_section(state);
    let generation = state.thumbnail_generation.load(Ordering::SeqCst);
    let conn = permit.connection()?;
    let Some((asset, has_failure)) =
        db::get_assets_for_thumbnails_by_ids(&conn, &[asset_id])?.pop()
    else {
        return Ok(ThumbnailRequestOutcome::NotFound);
    };
    let version = source_version_for(&asset);
    let target = thumbs::thumb_target(&state.thumbs_dir, &version);
    // A recorded path other than the current target is a thumbnail of an
    // earlier cache version (or a restored one). Its picture may predate the
    // current rendering rules, so it is not served as current (ADR 0020).
    let legacy = asset
        .thumb_path
        .clone()
        .filter(|existing| Path::new(existing) != target);

    if let Some(existing) = asset.thumb_path.clone().filter(|_| legacy.is_none()) {
        if let Some(served) = read_served(Path::new(&existing), &version) {
            // Path and version come from one snapshot; a stale failure
            // marker is cleared so the bulk "failed" count stays honest.
            if has_failure && !mode.cached_only {
                let write = ready_write(asset.id, existing, version, asset.record_version);
                publish_one(state, &conn, write, generation)?;
            }
            return Ok(ThumbnailRequestOutcome::Ready(served));
        }
    }

    if let Some(served) = read_served(&target, &version) {
        if mode.cached_only {
            return Ok(ThumbnailRequestOutcome::Ready(served));
        }
        let write = ready_write(
            asset.id,
            target.to_string_lossy().to_string(),
            version,
            asset.record_version,
        );
        // A lost compare-and-set means the record changed; the bytes still
        // match the version the response is labelled with.
        publish_one(state, &conn, write, generation)?;
        return Ok(ThumbnailRequestOutcome::Ready(served));
    }

    if mode.cached_only {
        return Ok(ThumbnailRequestOutcome::NotFound);
    }

    // A render that already failed in this process is not repeated on every
    // tile mount; the "render failed thumbnails" action retries explicitly.
    // A marker left by an earlier run gets one on-demand retry.
    if has_failure
        && state
            .thumb_scheduler
            .failed_interactively(asset.id, asset.record_version)
    {
        return Ok(ThumbnailRequestOutcome::NotFound);
    }

    // An owned legacy file goes to the worker, which checks the source outside
    // this section: a hung mount must not hold the thumbnail read guard. The
    // worker adopts, redraws, or keeps serving it for an unreachable source.
    let legacy_thumb = legacy.as_deref().and_then(|existing| {
        let root = state.thumbs_dir.canonicalize().ok();
        legacy_thumbnail(Some(existing), &target, root.as_deref())
    });
    let source_path = PathBuf::from(&asset.path);
    if legacy_thumb.is_none() && !source_path.exists() {
        let write = failed_write(asset.id, version, asset.record_version, SOURCE_MISSING);
        publish_one(state, &conn, write, generation)?;
        return Ok(ThumbnailRequestOutcome::NotFound);
    }

    let record_version = asset.record_version;
    let pending_version = version.clone();
    // Only a legacy file can come back unchanged (an unreachable source); a
    // rendered current target always goes through the compare-and-set.
    let unchanged_path = legacy.filter(|_| !has_failure);
    state.thumb_scheduler.enqueue_with_callback(
        ThumbnailTask {
            operation: Some(permit.clone()),
            asset_id: asset.id,
            source_path,
            target_path: target,
            kind: asset.kind,
            duration_ms: asset.duration_ms,
            source_version: version,
            legacy_thumb,
        },
        if mode.prefetch {
            ThumbnailPriority::High
        } else {
            ThumbnailPriority::Interactive
        },
        Box::new(move |result| {
            on_rendered(PendingThumbnail {
                asset_id,
                version: pending_version,
                record_version,
                generation,
                result,
                unchanged_path,
            })
        }),
    )?;
    Ok(ThumbnailRequestOutcome::Pending)
}

/// Second phase: publishes a finished render through the usual
/// compare-and-set and reads the file for the response.
pub fn finish_thumbnail_request(
    pending: PendingThumbnail,
    state: &AppState,
    permit: &OperationPermit,
) -> AppResult<Option<ServedThumbnail>> {
    Ok(finish_thumbnail_requests(vec![pending], state, permit)?
        .pop()
        .flatten())
}

/// Publishes finished renders together: one transaction per captured clear
/// generation, under one read section, like the former page stream. Fewer
/// `IMMEDIATE` transactions mean fewer write-lock acquisitions under scans.
/// Returns the served bytes per input, in order; `None` when stale or failed.
pub fn finish_thumbnail_requests(
    pending: Vec<PendingThumbnail>,
    state: &AppState,
    permit: &OperationPermit,
) -> AppResult<Vec<Option<ServedThumbnail>>> {
    let mut by_generation: Vec<(u64, Vec<ThumbnailWrite>)> = Vec::new();
    for item in &pending {
        if item.result.thumb_path.is_some() && item.result.thumb_path == item.unchanged_path {
            // Rewriting the recorded path would change nothing and still cost
            // an IMMEDIATE transaction on every request for that tile.
            continue;
        }
        let write = match &item.result.thumb_path {
            Some(path) => ready_write(
                item.asset_id,
                path.clone(),
                item.version.clone(),
                item.record_version,
            ),
            None => {
                state
                    .thumb_scheduler
                    .remember_interactive_failure(item.asset_id, item.record_version);
                failed_write(
                    item.asset_id,
                    item.version.clone(),
                    item.record_version,
                    RENDER_FAILED,
                )
            }
        };
        match by_generation
            .iter_mut()
            .find(|(generation, _)| *generation == item.generation)
        {
            Some((_, writes)) => writes.push(write),
            None => by_generation.push((item.generation, vec![write])),
        }
    }

    let _thumbs = thumb_read_section(state);
    let conn = permit.connection()?;
    let current = state.thumbnail_generation.load(Ordering::SeqCst);
    // Keyed per generation group: a pre-clear and a post-clear request for
    // one asset can share a render job, and only the pre-clear one is stale.
    let mut stale = HashSet::new();
    for (generation, writes) in &by_generation {
        stale.extend(
            publish_thumbnail_results(&conn, &state.thumbs_dir, writes, current, *generation)?
                .into_iter()
                .map(|asset_id| (*generation, asset_id)),
        );
    }
    Ok(pending
        .iter()
        .map(|item| {
            let path = item.result.thumb_path.as_deref()?;
            if stale.contains(&(item.generation, item.asset_id)) {
                return None;
            }
            let path = Path::new(path);
            if path == thumbs::thumb_target(&state.thumbs_dir, &item.version) {
                read_served(path, &item.version)
            } else {
                // A legacy file kept for an unreachable source may predate the
                // current rendering rules (ADR 0020). An empty token never
                // matches the requested one, so the response is `no-store`
                // and the tile asks again once the source is back.
                fs::read(path).ok().map(|bytes| ServedThumbnail {
                    bytes,
                    version: String::new(),
                })
            }
        })
        .collect())
}

/// Publishes collected thumbnail results: paths and failure markers in one
/// transaction, each guarded by the source version captured at enqueue time.
/// When the generation advanced meanwhile (thumbnail reset), nothing is
/// committed and every ID is reported stale. Produced files of results whose
/// source version is no longer current are removed best-effort, and only when
/// they are owned thumbnail files (`thumb_path` is untrusted). Callers hold
/// `thumb_read_section`.
fn publish_thumbnail_results(
    conn: &rusqlite::Connection,
    thumbs_root: &Path,
    writes: &[ThumbnailWrite],
    current_generation: u64,
    captured_generation: u64,
) -> anyhow::Result<Vec<i64>> {
    if writes.is_empty() {
        return Ok(Vec::new());
    }
    let ready_path = |write: &ThumbnailWrite| match &write.outcome {
        ThumbnailWriteOutcome::Ready(path) => Some(path.clone()),
        ThumbnailWriteOutcome::Failed(_) => None,
    };
    let mut obsolete = Vec::new();
    let stale = if current_generation != captured_generation {
        // The clear already swept the directory. Jobs are keyed by target, so
        // a post-clear request can share this render and publish the same
        // file: a target of a still-current source version is kept (a later
        // request reconciles it). Nothing is removed when the rows cannot be
        // read.
        let ids: Vec<i64> = writes.iter().map(|write| write.asset_id).collect();
        if let Ok(rows) = db::get_assets_for_thumbnails_by_ids(conn, &ids) {
            let current: HashMap<i64, crate::thumbs::SourceVersion> = rows
                .into_iter()
                .map(|(asset, _)| (asset.id, source_version_for(&asset)))
                .collect();
            obsolete.extend(
                writes
                    .iter()
                    .filter(|write| current.get(&write.asset_id) != Some(&write.version))
                    .filter_map(ready_path),
            );
        }
        ids
    } else {
        let stale = db::commit_thumbnail_results(conn, writes)?;
        obsolete.extend(
            writes
                .iter()
                .filter(|write| stale.contains(&write.asset_id))
                .filter_map(ready_path),
        );
        stale
    };
    if !obsolete.is_empty() {
        if let Ok(canonical_root) = thumbs_root.canonicalize() {
            for path in obsolete {
                remove_owned_thumbnail_file(&canonical_root, Path::new(&path));
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
    // A counting pass gives the progress total, so no path is buffered; the
    // second pass deletes as it walks, against a root canonicalized once.
    let walk = || {
        walkdir::WalkDir::new(&state.thumbs_dir)
            .follow_links(false)
            .contents_first(true)
            .into_iter()
            .filter_map(Result::ok)
    };
    let total = walk().filter(|entry| entry.file_type().is_file()).count();
    let _ = emit_progress(
        app,
        "thumbs-clear",
        0,
        total,
        "Removing all thumbnails".to_string(),
    );
    let canonical_root = state.thumbs_dir.canonicalize().ok();
    let (mut processed, mut reported, mut removed) = (0usize, 0usize, 0usize);
    for entry in walk() {
        if entry.file_type().is_file() {
            if canonical_root
                .as_deref()
                .is_some_and(|root| remove_owned_thumbnail_file(root, entry.path()))
            {
                removed += 1;
            }
            processed += 1;
            if processed.is_multiple_of(25) || processed == total {
                emit_clear_progress(app, processed, total.max(processed));
                reported = processed;
            }
        } else if entry.file_type().is_dir() && entry.depth() > 0 {
            // Yielded after its contents. Only an empty directory is removed;
            // failed file cleanup remains retryable.
            let _ = fs::remove_dir(entry.path());
        }
    }
    // A render still finishing can change the file count between the passes.
    if processed < total || reported != processed {
        emit_clear_progress(app, processed, processed);
    }
    Ok(removed)
}

fn emit_clear_progress(
    app: &impl crate::services::progress::ProgressSink,
    processed: usize,
    total: usize,
) {
    let _ = emit_progress(
        app,
        "thumbs-clear",
        processed,
        total,
        format!("Processed {processed}/{total}"),
    );
}

pub fn delete_thumbnail_files_in_root(thumbs_root: &Path, paths: Vec<String>) -> usize {
    let Ok(canonical_root) = thumbs_root.canonicalize() else {
        return 0;
    };
    paths
        .into_iter()
        .filter(|path| remove_owned_thumbnail_file(&canonical_root, Path::new(path)))
        .count()
}

pub fn delete_thumbnail_files_with_progress(
    app: &impl crate::services::progress::ProgressSink,
    thumbs_root: &Path,
    paths: Vec<String>,
    total: usize,
    phase: &str,
) -> usize {
    let canonical_root = thumbs_root.canonicalize().ok();
    let mut removed = 0usize;
    for (idx, path) in paths.into_iter().enumerate() {
        if canonical_root
            .as_deref()
            .is_some_and(|root| remove_owned_thumbnail_file(root, Path::new(&path)))
        {
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

/// The recorded thumbnail a render task may adopt or must supersede: an owned
/// file other than the current target (ADR 0020).
fn legacy_thumbnail(
    recorded: Option<&str>,
    target: &Path,
    canonical_root: Option<&Path>,
) -> Option<PathBuf> {
    let recorded = Path::new(recorded?);
    (recorded != target
        && canonical_root.is_some_and(|root| is_owned_thumbnail_file(root, recorded)))
    .then(|| recorded.to_path_buf())
}

/// Whether `candidate` is a regular file (not a symbolic link) inside the
/// already canonical thumbnail root.
fn is_owned_thumbnail_file(canonical_root: &Path, candidate: &Path) -> bool {
    fs::symlink_metadata(candidate)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        && candidate
            .canonicalize()
            .is_ok_and(|canonical| canonical.starts_with(canonical_root))
}

/// Removes `candidate` when it is an owned thumbnail file of the already
/// canonical root; callers canonicalize the root once per operation.
fn remove_owned_thumbnail_file(canonical_root: &Path, candidate: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(candidate) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    let Ok(canonical_candidate) = candidate.canonicalize() else {
        return false;
    };
    if !canonical_candidate.starts_with(canonical_root) {
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
        publish_thumbnail_results, wait_next_render_result_with_stall, ThumbnailTaskResult,
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
    fn request_thumbnail_serves_existing_thumbnail_bytes() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let source_path = tmp.path().join("missing-source.jpg");
        db::upsert_asset(&conn, &new_asset(&source_path, 77, None)).expect("upsert asset");
        let existing_thumb = crate::thumbs::thumb_target(
            &thumbs_dir,
            &super::source_version_for(&db::get_asset_for_thumbnail(&conn, 1).unwrap().unwrap()),
        );
        fs::write(&existing_thumb, b"thumb").expect("write existing thumb");
        conn.execute(
            "UPDATE assets SET thumb_path = ?1 WHERE id = 1",
            [existing_thumb.to_string_lossy()],
        )
        .expect("record thumb path");

        let state = create_test_state(&db_path, &thumbs_dir);
        let state_ref = &state;
        let served = request_now(state_ref, 1)
            .expect("request thumb")
            .expect("served");
        assert_eq!(served.bytes, b"thumb");
        let version =
            super::source_version_for(&db::get_asset_for_thumbnail(&conn, 1).unwrap().unwrap());
        assert_eq!(served.version, super::version_token(&version));
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
                                legacy_thumb: None,
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
                        legacy_thumb: None,
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
            request_now(state_ref, 1)
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
        let root = Path::new("C:/thumbs");
        let stale = publish_thumbnail_results(&conn, root, &updates, 7, 7)
            .expect("publish same generation");
        assert_eq!(stale, vec![1]);

        // Generation advanced meanwhile: nothing is committed at all.
        let stale =
            publish_thumbnail_results(&conn, root, &updates, 8, 7).expect("publish after reset");
        assert_eq!(stale, vec![1]);
    }
    #[test]
    fn stale_render_is_never_served_and_a_retry_renders_again() {
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
        assert_eq!(
            request_now(&state, 1).unwrap(),
            None,
            "a render for a superseded version is never served"
        );
        let served = request_now(&state, 1)
            .unwrap()
            .expect("retry renders again");
        assert_eq!(served.bytes, b"thumbnail");
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

    /// Runs both phases of a `thumb://` request, waiting for a render.
    fn request_now(
        state: &AppState,
        asset_id: i64,
    ) -> crate::error::AppResult<Option<super::ServedThumbnail>> {
        crate::app::locks::with_thumb_operation(state, |permit| {
            let (tx, rx) = mpsc::channel();
            match super::request_thumbnail(
                asset_id,
                Default::default(),
                state,
                permit,
                move |pending| {
                    let _ = tx.send(pending);
                },
            )? {
                super::ThumbnailRequestOutcome::Ready(served) => Ok(Some(served)),
                super::ThumbnailRequestOutcome::NotFound => Ok(None),
                super::ThumbnailRequestOutcome::Pending => {
                    let pending = rx
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .expect("render result");
                    super::finish_thumbnail_request(pending, state, permit)
                }
            }
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
        let slow = std::thread::spawn(move || request_now(&slow_state, 1));
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
        let next = std::thread::spawn(move || request_now(&next_state, 2));
        let next_job = started.recv_timeout(WAIT).unwrap();
        assert_eq!(next_job.asset_id, 2);
        next_job.release.send(()).unwrap();
        assert!(next.join().unwrap().unwrap().is_some());
        assert!(!slow.is_finished(), "the first render is still running");

        slow_job.release.send(()).unwrap();
        assert!(slow.join().unwrap().unwrap().is_some());
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
        let served = request_now(&state, 1).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        conn.execute_batch("ROLLBACK").unwrap();
        assert_eq!(served.expect("served").bytes, b"thumb");
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

        // A marker from an earlier run gets one on-demand retry.
        assert!(request_now(&state, 1).unwrap().is_some());
        assert!(db::get_asset_for_thumbnail(&conn, 1)
            .unwrap()
            .unwrap()
            .thumb_path
            .is_some());
        assert!(db::list_failed_thumbnail_asset_ids(&conn)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn render_failed_in_this_run_is_not_repeated_on_every_request() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let source = dir.path().join("broken.png");
        fs::write(&source, b"not an image").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &new_asset(&source, 3, None)).unwrap();
        let mut state = create_test_state(&db_path, &thumbs);
        let renders = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = renders.clone();
        state.thumb_scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );

        assert_eq!(request_now(&state, 1).unwrap(), None);
        assert_eq!(request_now(&state, 1).unwrap(), None);
        assert_eq!(renders.load(Ordering::SeqCst), 1);
        assert_eq!(db::list_failed_thumbnail_asset_ids(&conn).unwrap(), vec![1]);
    }

    #[test]
    fn missing_asset_and_missing_source_are_not_found() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &new_asset(&dir.path().join("gone.png"), 3, None)).unwrap();
        let state = create_test_state(&db_path, &thumbs);

        assert_eq!(request_now(&state, 99).unwrap(), None);
        assert_eq!(request_now(&state, 1).unwrap(), None);
        assert_eq!(db::list_failed_thumbnail_asset_ids(&conn).unwrap(), vec![1]);
    }

    fn cached_only() -> super::ThumbnailRequestMode {
        super::ThumbnailRequestMode {
            cached_only: true,
            prefetch: false,
        }
    }

    #[test]
    fn cached_only_request_never_renders_or_writes() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let source = dir.path().join("source.png");
        fs::write(&source, b"source").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &new_asset(&source, 3, None)).unwrap();
        let mut state = create_test_state(&db_path, &thumbs);
        state.thumb_scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(|_, _| panic!("cached-only requests must not render")),
        );
        let permit = state.database.admit().unwrap();
        let outcome = super::request_thumbnail(1, cached_only(), &state, &permit, |_| {
            panic!("no render callback")
        })
        .unwrap();
        assert!(matches!(outcome, super::ThumbnailRequestOutcome::NotFound));
        assert!(db::list_failed_thumbnail_asset_ids(&conn)
            .unwrap()
            .is_empty());

        // An existing file is served from cache as usual.
        let version =
            super::source_version_for(&db::get_asset_for_thumbnail(&conn, 1).unwrap().unwrap());
        fs::write(crate::thumbs::thumb_target(&thumbs, &version), b"cached").unwrap();
        let outcome = super::request_thumbnail(1, cached_only(), &state, &permit, |_| {}).unwrap();
        assert!(
            matches!(outcome, super::ThumbnailRequestOutcome::Ready(ref served) if served.bytes == b"cached")
        );
    }

    #[test]
    fn finished_renders_are_published_together_and_each_served() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        for name in ["a.png", "b.png"] {
            let source = dir.path().join(name);
            fs::write(&source, b"source").unwrap();
            db::upsert_asset(&conn, &new_asset(&source, 1, None)).unwrap();
        }
        let mut state = create_test_state(&db_path, &thumbs);
        state.thumb_scheduler = ThumbnailScheduler::with_processor(
            2,
            "ffmpeg".into(),
            Arc::new(|_, task| {
                fs::write(&task.target_path, format!("thumb-{}", task.asset_id)).unwrap();
                Some(task.target_path.to_string_lossy().into())
            }),
        );
        let permit = state.database.admit().unwrap();
        let (tx, rx) = mpsc::channel();
        for id in [1, 2] {
            let tx = tx.clone();
            let outcome =
                super::request_thumbnail(id, Default::default(), &state, &permit, move |pending| {
                    let _ = tx.send(pending);
                })
                .unwrap();
            assert!(matches!(outcome, super::ThumbnailRequestOutcome::Pending));
        }
        let mut pending: Vec<_> = (0..2)
            .map(|_| rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap())
            .collect();
        pending.sort_by_key(|item| item.asset_id);
        let served = super::finish_thumbnail_requests(pending, &state, &permit).unwrap();
        let bytes: Vec<_> = served.into_iter().map(|item| item.unwrap().bytes).collect();
        assert_eq!(bytes, vec![b"thumb-1".to_vec(), b"thumb-2".to_vec()]);
        for id in [1, 2] {
            assert!(db::get_asset_for_thumbnail(&conn, id)
                .unwrap()
                .unwrap()
                .thumb_path
                .is_some());
        }
    }

    /// A state whose renderer writes a placeholder file and counts renders.
    fn counting_bulk_state(
        db_path: &Path,
        thumbs: &Path,
    ) -> (AppState, Arc<std::sync::atomic::AtomicUsize>) {
        let mut state = create_test_state(db_path, thumbs);
        let renders = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = renders.clone();
        state.thumb_scheduler = ThumbnailScheduler::with_processor(
            2,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                counter.fetch_add(1, Ordering::SeqCst);
                fs::write(&task.target_path, b"thumbnail").unwrap();
                Some(task.target_path.to_string_lossy().into())
            }),
        );
        (state, renders)
    }

    fn render_all(state: &AppState) -> crate::models::ThumbnailRenderSummary {
        let permit = state.database.admit().unwrap();
        super::render_all_thumbnails(state, &permit, &|_| Ok(())).unwrap()
    }

    fn thumb_path_of(conn: &rusqlite::Connection, id: i64) -> Option<String> {
        db::get_asset_for_thumbnail(conn, id)
            .unwrap()
            .unwrap()
            .thumb_path
    }

    #[cfg(unix)]
    #[test]
    fn render_all_keeps_the_cached_thumbnail_of_an_unreadable_source() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let folder = dir.path().join("folder");
        fs::create_dir(&folder).unwrap();
        let source = folder.join("source.png");
        fs::write(&source, b"source").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &new_asset(&source, 1, None)).unwrap();
        let (state, renders) = counting_bulk_state(&db_path, &thumbs);

        assert_eq!(render_all(&state).generated, 1);
        let cached = thumb_path_of(&conn, 1).expect("rendered path");

        // Restores the folder even when an assertion below panics, so the
        // temporary directory can still be removed.
        struct Readable<'a>(&'a Path);
        impl Drop for Readable<'_> {
            fn drop(&mut self) {
                let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
            }
        }
        let readable = Readable(&folder);
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o000)).unwrap();
        // Root bypasses directory permissions; the case cannot be modelled then.
        let unreadable = !source.exists();
        let offline = render_all(&state);
        drop(readable);
        if !unreadable {
            return;
        }
        assert_eq!(
            (offline.generated, offline.failed, offline.stale),
            (1, 0, 0)
        );
        assert_eq!(thumb_path_of(&conn, 1).as_deref(), Some(cached.as_str()));
        assert_eq!(fs::read(&cached).unwrap(), b"thumbnail");
        assert!(db::list_failed_thumbnail_asset_ids(&conn)
            .unwrap()
            .is_empty());

        let readable = render_all(&state);
        assert_eq!((readable.generated, readable.skipped_failed), (1, 0));
        assert_eq!(renders.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn render_all_clears_a_stale_marker_of_an_offline_source_and_keeps_its_file() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        // A recorded file outside the deterministic target (for example from
        // an earlier cache version) is what the demand path keeps serving.
        let recorded = thumbs.join("recorded.jpg");
        fs::write(&recorded, b"recorded").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        let gone = dir.path().join("offline").join("gone.png");
        db::upsert_asset(&conn, &new_asset(&gone, 3, Some(&recorded))).unwrap();
        db::record_thumbnail_failure(&conn, 1, 3, Some("earlier failure")).unwrap();
        let (state, renders) = counting_bulk_state(&db_path, &thumbs);

        let summary = render_all(&state);
        assert_eq!((summary.generated, summary.failed), (1, 0));
        assert_eq!(
            thumb_path_of(&conn, 1),
            Some(recorded.to_string_lossy().into_owned())
        );
        assert_eq!(fs::read(&recorded).unwrap(), b"recorded");
        assert!(db::list_failed_thumbnail_asset_ids(&conn)
            .unwrap()
            .is_empty());
        assert_eq!(renders.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn render_all_without_a_cached_thumbnail_still_records_a_missing_source() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        // The recorded path no longer names a file.
        let vanished = thumbs.join("vanished.jpg");
        db::upsert_asset(
            &conn,
            &new_asset(&dir.path().join("gone.png"), 3, Some(&vanished)),
        )
        .unwrap();

        // A recorded file outside the thumbnail root is untrusted: it is not
        // kept as this asset's thumbnail, and it is never removed.
        let foreign = dir.path().join("foreign.jpg");
        fs::write(&foreign, b"not a cache file").unwrap();
        db::upsert_asset(
            &conn,
            &new_asset(&dir.path().join("gone-too.png"), 3, Some(&foreign)),
        )
        .unwrap();
        db::record_thumbnail_failure(&conn, 2, 3, Some("earlier failure")).unwrap();
        let (state, renders) = counting_bulk_state(&db_path, &thumbs);

        let summary = render_all(&state);
        assert_eq!((summary.generated, summary.failed), (0, 2));
        assert_eq!(thumb_path_of(&conn, 1), None);
        assert_eq!(thumb_path_of(&conn, 2), None);
        assert_eq!(fs::read(&foreign).unwrap(), b"not a cache file");
        assert_eq!(
            db::list_failed_thumbnail_asset_ids(&conn).unwrap(),
            vec![1, 2]
        );
        assert_eq!(renders.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn render_all_does_not_rewrite_rows_whose_thumbnail_is_current() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        for id in 0..3 {
            let source = dir.path().join(format!("source-{id}.png"));
            fs::write(&source, b"source").unwrap();
            db::upsert_asset(&conn, &new_asset(&source, 1, None)).unwrap();
        }
        let (state, renders) = counting_bulk_state(&db_path, &thumbs);
        assert_eq!(render_all(&state).generated, 3);
        let paths: Vec<_> = (1..=3).map(|id| thumb_path_of(&conn, id)).collect();

        // Another writer holds the SQLite write lock. A run with nothing to
        // change must not wait for it (busy timeout) or fail.
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let started = std::time::Instant::now();
        let permit = state.database.admit().unwrap();
        let current = super::render_all_thumbnails(&state, &permit, &|_| Ok(()));
        let elapsed = started.elapsed();
        conn.execute_batch("ROLLBACK").unwrap();
        let current = current.expect("no write is attempted for current rows");
        assert!(elapsed < std::time::Duration::from_secs(2), "{elapsed:?}");
        assert_eq!(
            (current.generated, current.processed, current.stale),
            (3, 3, 0)
        );
        assert_eq!(renders.load(Ordering::SeqCst), 3);

        // A row that lost its path and a row with a stale failure marker are
        // still repaired from the existing target.
        conn.execute("UPDATE assets SET thumb_path = NULL WHERE id = 1", [])
            .unwrap();
        db::record_thumbnail_failure(&conn, 2, 1, Some("stale marker")).unwrap();
        let repaired = render_all(&state);
        assert_eq!((repaired.generated, repaired.failed), (3, 0));
        assert_eq!(
            (1..=3)
                .map(|id| thumb_path_of(&conn, id))
                .collect::<Vec<_>>(),
            paths
        );
        assert!(db::list_failed_thumbnail_asset_ids(&conn)
            .unwrap()
            .is_empty());
        assert_eq!(renders.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn a_clear_during_render_all_counts_unwritten_current_rows_as_stale() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let source = dir.path().join("source.png");
        fs::write(&source, b"source").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &new_asset(&source, 1, None)).unwrap();
        let (state, _) = counting_bulk_state(&db_path, &thumbs);
        assert_eq!(render_all(&state).generated, 1);

        // The per-asset progress event fires after the page was read; bumping
        // the clear epoch there models Clear all running between page and
        // publication.
        let permit = state.database.admit().unwrap();
        let generation = &state.thumbnail_generation;
        let summary = super::render_all_thumbnails(
            &state,
            &permit,
            &|progress: crate::models::ScanProgress| {
                if progress.phase == "thumbs" {
                    generation.fetch_add(1, Ordering::SeqCst);
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!((summary.generated, summary.stale), (0, 1));
    }

    /// One `thumb://` request enqueued before Clear all and one after it, for
    /// the same asset, sharing one render job (jobs are keyed by target).
    struct SharedClearJob {
        _dir: tempfile::TempDir,
        db_path: PathBuf,
        state: AppState,
        before_clear: super::PendingThumbnail,
        after_clear: super::PendingThumbnail,
        target: PathBuf,
    }

    fn shared_job_across_a_clear() -> SharedClearJob {
        use crate::services::thumb_scheduler::test_support::{
            controlled_scheduler, wait_for_pending, WAIT,
        };
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
        let (scheduler, started) = controlled_scheduler(16);
        state.thumb_scheduler = scheduler;

        let enqueue = |state: &AppState| {
            let (tx, rx) = mpsc::channel();
            let outcome = crate::app::locks::with_thumb_operation(state, |permit| {
                super::request_thumbnail(1, Default::default(), state, permit, move |pending| {
                    let _ = tx.send(pending);
                })
            })
            .unwrap();
            assert!(matches!(outcome, super::ThumbnailRequestOutcome::Pending));
            rx
        };
        let before_rx = enqueue(&state);
        let job = started.recv_timeout(WAIT).unwrap();
        assert_eq!(job.asset_id, 1);
        crate::app::locks::with_thumb_lock(&state, |permit| {
            super::clear_all_thumbnails(&state, permit, &|_| Ok(()))
        })
        .unwrap();
        let after_rx = enqueue(&state);
        wait_for_pending(&state.thumb_scheduler, 1);
        assert!(
            started.try_recv().is_err(),
            "the post-clear request joins the pending job"
        );
        job.release.send(()).unwrap();
        let before_clear = before_rx.recv_timeout(WAIT).unwrap();
        let after_clear = after_rx.recv_timeout(WAIT).unwrap();
        assert_eq!((before_clear.generation, after_clear.generation), (0, 1));
        let target = PathBuf::from(after_clear.result.thumb_path.clone().unwrap());
        assert!(target.is_file());
        SharedClearJob {
            _dir: dir,
            db_path,
            state,
            before_clear,
            after_clear,
            target,
        }
    }

    fn finish(
        state: &AppState,
        pending: Vec<super::PendingThumbnail>,
    ) -> Vec<Option<super::ServedThumbnail>> {
        crate::app::locks::with_thumb_operation(state, |permit| {
            super::finish_thumbnail_requests(pending, state, permit)
        })
        .unwrap()
    }

    fn assert_post_clear_publication(db_path: &Path, target: &Path) {
        assert!(target.is_file(), "the published file must stay");
        let conn = db::open_connection(db_path).unwrap();
        assert_eq!(
            thumb_path_of(&conn, 1).as_deref(),
            Some(target.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn a_pre_clear_waiter_in_the_same_batch_keeps_the_post_clear_publication() {
        let job = shared_job_across_a_clear();
        let served = finish(&job.state, vec![job.before_clear, job.after_clear]);
        assert!(served[0].is_none(), "the pre-clear result is stale");
        assert_eq!(
            served[1].as_ref().map(|served| served.bytes.as_slice()),
            Some(&b"thumbnail"[..]),
            "the post-clear result is served"
        );
        assert_post_clear_publication(&job.db_path, &job.target);
    }

    #[test]
    fn a_pre_clear_waiter_published_first_keeps_the_shared_target() {
        let job = shared_job_across_a_clear();
        assert!(finish(&job.state, vec![job.before_clear])[0].is_none());
        assert!(job.target.is_file(), "the current-version target stays");
        assert!(finish(&job.state, vec![job.after_clear])[0].is_some());
        assert_post_clear_publication(&job.db_path, &job.target);
    }

    #[test]
    fn a_pre_clear_waiter_published_last_does_not_remove_the_served_file() {
        let job = shared_job_across_a_clear();
        assert!(finish(&job.state, vec![job.after_clear])[0].is_some());
        assert!(finish(&job.state, vec![job.before_clear])[0].is_none());
        assert_post_clear_publication(&job.db_path, &job.target);
    }

    #[test]
    fn a_pre_clear_result_of_a_reindexed_version_is_still_removed() {
        let job = shared_job_across_a_clear();
        let conn = db::open_connection(&job.db_path).unwrap();
        conn.execute(
            "UPDATE assets SET fingerprint_mtime_ns = fingerprint_mtime_ns + 1, record_version = record_version + 1",
            [],
        )
        .unwrap();
        assert!(finish(&job.state, vec![job.before_clear])[0].is_none());
        assert!(
            !job.target.exists(),
            "an obsolete-version target is removed"
        );
    }

    #[test]
    fn stale_cleanup_removes_only_owned_thumbnail_files() {
        let dir = tempdir().unwrap();
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let owned = thumbs.join("owned.jpg");
        let outside = dir.path().join("outside.jpg");
        fs::write(&owned, b"owned").unwrap();
        fs::write(&outside, b"outside").unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        let version = crate::thumbs::SourceVersion::new("/media/missing.jpg", 10, 0);
        let writes: Vec<_> = [(1, &owned), (2, &outside)]
            .into_iter()
            .map(|(id, path)| {
                super::ready_write(id, path.to_string_lossy().into(), version.clone(), 1)
            })
            .collect();
        // Missing rows lose the compare-and-set, so both results are stale.
        let stale = publish_thumbnail_results(&conn, &thumbs, &writes, 3, 3).unwrap();
        assert_eq!(stale, vec![1, 2]);
        assert!(!owned.exists(), "an owned stale file is removed");
        assert!(outside.exists(), "an untrusted path outside the root stays");
    }

    /// A library whose rows reference thumbnails of an earlier cache version:
    /// one JPEG per Exif orientation (`None` means no Exif) and a video.
    struct LegacyLibrary {
        _dir: tempfile::TempDir,
        db_path: PathBuf,
        thumbs: PathBuf,
        legacy: Vec<PathBuf>,
    }

    fn legacy_library(orientations: &[Option<u16>], with_video: bool) -> LegacyLibrary {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("media.db");
        let thumbs = dir.path().join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        let mut legacy = Vec::new();
        for (index, orientation) in orientations.iter().enumerate() {
            let source = dir.path().join(format!("photo-{index}.jpg"));
            crate::thumbs::test_support::write_oriented(
                &source,
                &image::DynamicImage::ImageRgb8(image::RgbImage::new(1_200, 900)),
                image::ImageFormat::Jpeg,
                *orientation,
            );
            let old = thumbs.join(format!("legacy-{index}.jpg"));
            fs::write(&old, format!("legacy {index}")).unwrap();
            db::upsert_asset(&conn, &new_asset(&source, 1, Some(&old))).unwrap();
            legacy.push(old);
        }
        if with_video {
            let source = dir.path().join("clip.mp4");
            fs::write(&source, b"not decoded").unwrap();
            let old = thumbs.join("legacy-video.jpg");
            fs::write(&old, b"legacy video").unwrap();
            let mut asset = new_asset(&source, 1, Some(&old));
            asset.kind = "video".into();
            db::upsert_asset(&conn, &asset).unwrap();
            legacy.push(old);
        }
        LegacyLibrary {
            _dir: dir,
            db_path,
            thumbs,
            legacy,
        }
    }

    fn current_target(conn: &rusqlite::Connection, thumbs: &Path, id: i64) -> PathBuf {
        let asset = db::get_asset_for_thumbnail(conn, id).unwrap().unwrap();
        crate::thumbs::thumb_target(thumbs, &super::source_version_for(&asset))
    }

    #[test]
    fn legacy_thumbnail_of_an_unrotated_image_is_adopted_without_rendering() {
        let library = legacy_library(&[Some(1)], false);
        let conn = db::open_connection(&library.db_path).unwrap();
        let state = create_test_state(&library.db_path, &library.thumbs);
        let target = current_target(&conn, &library.thumbs, 1);

        // A fast-scroll request does not serve the old picture as current.
        let permit = state.database.admit().unwrap();
        let outcome = super::request_thumbnail(1, cached_only(), &state, &permit, |_| {}).unwrap();
        assert!(matches!(outcome, super::ThumbnailRequestOutcome::NotFound));
        drop(permit);

        let served = request_now(&state, 1).unwrap().expect("served");
        assert_eq!(served.bytes, b"legacy 0", "the recorded picture is reused");
        assert!(!library.legacy[0].exists(), "moved, not copied");
        assert_eq!(fs::read(&target).unwrap(), b"legacy 0");
        assert_eq!(
            thumb_path_of(&conn, 1),
            Some(target.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn legacy_thumbnail_of_a_rotated_image_is_rendered_again() {
        let library = legacy_library(&[Some(6)], false);
        let conn = db::open_connection(&library.db_path).unwrap();
        let state = create_test_state(&library.db_path, &library.thumbs);
        let target = current_target(&conn, &library.thumbs, 1);

        let served = request_now(&state, 1).unwrap().expect("served");
        let thumb = image::load_from_memory(&served.bytes).expect("a rendered JPEG");
        assert_eq!((thumb.width(), thumb.height()), (293, 390));
        assert!(
            !library.legacy[0].exists(),
            "the superseded file is removed"
        );
        assert_eq!(
            thumb_path_of(&conn, 1),
            Some(target.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn render_all_adopts_unchanged_legacy_thumbnails_and_renders_rotated_ones() {
        let library = legacy_library(&[None, Some(8), Some(2)], true);
        let conn = db::open_connection(&library.db_path).unwrap();
        let state = create_test_state(&library.db_path, &library.thumbs);

        let summary = render_all(&state);
        assert_eq!((summary.generated, summary.failed), (4, 0));
        for (id, old) in (1..=4).zip(&library.legacy) {
            let target = current_target(&conn, &library.thumbs, id);
            assert_eq!(
                thumb_path_of(&conn, id),
                Some(target.to_string_lossy().into_owned())
            );
            assert!(
                !old.exists(),
                "asset {id}: the legacy file is moved or removed"
            );
        }
        let bytes = |id| fs::read(current_target(&conn, &library.thumbs, id)).unwrap();
        assert_eq!(bytes(1), b"legacy 0", "no Exif: adopted");
        assert_eq!(bytes(4), b"legacy video", "video frames did not change");
        // Rotated and mirrored sources are drawn again under the new rules.
        let rotated = image::load_from_memory(&bytes(2)).unwrap();
        assert_eq!((rotated.width(), rotated.height()), (293, 390));
        let mirrored = image::load_from_memory(&bytes(3)).unwrap();
        assert_eq!((mirrored.width(), mirrored.height()), (390, 293));
    }

    #[test]
    fn legacy_thumbnail_of_an_unreachable_source_is_still_served() {
        let library = legacy_library(&[Some(6)], false);
        let conn = db::open_connection(&library.db_path).unwrap();
        let source = db::get_asset_for_thumbnail(&conn, 1).unwrap().unwrap().path;
        fs::remove_file(&source).unwrap();
        let state = create_test_state(&library.db_path, &library.thumbs);

        // The source is checked by the worker, outside the read section, so a
        // cached-only request does not look at it.
        let permit = state.database.admit().unwrap();
        let outcome = super::request_thumbnail(1, cached_only(), &state, &permit, |_| {}).unwrap();
        assert!(matches!(outcome, super::ThumbnailRequestOutcome::NotFound));
        drop(permit);
        let served = request_now(&state, 1).unwrap().unwrap();
        assert_eq!(served.bytes, b"legacy 0");
        // The old picture is never labelled as the current version, so WebKit
        // does not keep it for the tile's URL once the source is back.
        let token =
            super::source_version_for(&db::get_asset_for_thumbnail(&conn, 1).unwrap().unwrap());
        let response = crate::services::thumb_protocol::image_response(
            served,
            &super::version_token(&token),
            "tauri://localhost",
        );
        assert_eq!(
            response.headers()[tauri::http::header::CACHE_CONTROL],
            "no-store"
        );
        assert!(library.legacy[0].exists());
        assert_eq!(
            thumb_path_of(&conn, 1),
            Some(library.legacy[0].to_string_lossy().into_owned())
        );
        assert!(db::list_failed_thumbnail_asset_ids(&conn)
            .unwrap()
            .is_empty());
    }
}
