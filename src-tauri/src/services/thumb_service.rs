use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
    },
    time::Duration,
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

#[derive(Clone, Copy)]
enum BulkRenderMode {
    All,
    FailedOnly,
}

struct PendingRenderTask {
    asset_path: String,
    modified_at: i64,
}

struct PendingPageTask {
    modified_at: i64,
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
    let mut updates = Vec::<(i64, Option<String>)>::new();
    let mut pending = HashMap::<i64, PendingRenderTask>::new();
    let (completion_tx, completion_rx) = mpsc::channel::<ThumbnailTaskResult>();

    for asset in assets {
        if state
            .thumbnail_render_all_cancel_requested
            .load(Ordering::SeqCst)
        {
            cancelled = true;
            break;
        }

        let source_path = PathBuf::from(&asset.path);
        let target = thumbs::thumb_target(&state.thumbs_dir, &source_path, asset.modified_at);

        if !source_path.exists() {
            failed += 1;
            processed += 1;
            updates.push((asset.id, None));
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
            updates.push((asset.id, Some(target.to_string_lossy().to_string())));
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
            },
            ThumbnailPriority::Low,
            completion_tx.clone(),
        )?;

        pending.insert(
            asset.id,
            PendingRenderTask {
                asset_path: asset.path,
                modified_at: asset.modified_at,
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

    db::update_asset_thumbnail_paths_batch(&conn, &updates)?;

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
    updates: &mut Vec<(i64, Option<String>)>,
    task: PendingRenderTask,
    result: ThumbnailTaskResult,
) {
    *processed += 1;
    if let Some(path) = result.thumb_path {
        *generated += 1;
        updates.push((result.asset_id, Some(path)));
        let _ = db::clear_thumbnail_failure(conn, result.asset_id);
    } else {
        *failed += 1;
        updates.push((result.asset_id, None));
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
    loop {
        if cancel_requested.load(Ordering::SeqCst) {
            return Ok(None);
        }

        match completion_receiver.recv_timeout(Duration::from_millis(RESULT_WAIT_TIMEOUT_MS)) {
            Ok(result) => return Ok(Some(result)),
            Err(RecvTimeoutError::Timeout) => continue,
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
    updates: &mut Vec<(i64, Option<String>)>,
) {
    loop {
        match completion_receiver.try_recv() {
            Ok(result) => {
                let Some(task) = pending.remove(&result.asset_id) else {
                    continue;
                };
                process_bulk_result(
                    conn, app, mode, total, processed, generated, failed, updates, task, result,
                );
            }
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
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
    if !source_path.exists() {
        db::update_asset_thumbnail_path_if_changed(&conn, asset.id, None)?;
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

    let target = thumbs::thumb_target(&state.thumbs_dir, &source_path, asset.modified_at);
    if target.exists() {
        let target_str = target.to_string_lossy().to_string();
        db::update_asset_thumbnail_path_if_changed(&conn, asset.id, Some(&target_str))?;
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
        },
        ThumbnailPriority::High,
    )?;

    let result = receiver
        .recv()
        .map_err(|e| format!("thumbnail worker disconnected: {e}"))?;

    if let Some(path) = result.thumb_path {
        db::update_asset_thumbnail_path_if_changed(&conn, asset.id, Some(&path))?;
        let _ = db::clear_thumbnail_failure(&conn, asset.id);
        Ok(Some(path))
    } else {
        db::update_asset_thumbnail_path_if_changed(&conn, asset.id, None)?;
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
    visible_ids: Vec<i64>,
    prefetch_ids: Vec<i64>,
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
    channel: Channel<ThumbnailStreamEvent>,
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
    let result = ensure_page_thumbnails_with_sink(ids, &high_priority, state, app, |item| {
        let _ = channel.send(ThumbnailStreamEvent::Ready(item.clone()));
    })?;
    for asset_id in &result.failed {
        let _ = channel.send(ThumbnailStreamEvent::Failed { asset_id: *asset_id });
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
    let mut updates = Vec::new();
    let mut pending = HashMap::<i64, PendingPageTask>::new();
    let (completion_tx, completion_rx) = mpsc::channel::<ThumbnailTaskResult>();
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
        if !source_path.exists() {
            failed.push(asset.id);
            updates.push((asset.id, None));
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

        let target = thumbs::thumb_target(&state.thumbs_dir, &source_path, asset.modified_at);
        if target.exists() {
            let target_str = target.to_string_lossy().to_string();
            let item = ThumbnailBatchItem {
                asset_id: asset.id,
                thumb_path: target_str.clone(),
            };
            on_ready(&item);
            ready.push(item);
            updates.push((asset.id, Some(target_str)));
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
            },
        );
    }

    while !pending.is_empty() {
        let result = completion_rx
            .recv()
            .map_err(|e| format!("thumbnail worker disconnected: {e}"))?;
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
            updates.push((result.asset_id, Some(path)));
            let _ = db::clear_thumbnail_failure(&conn, result.asset_id);
        } else {
            failed.push(result.asset_id);
            updates.push((result.asset_id, None));
            let _ = db::record_thumbnail_failure(
                &conn,
                result.asset_id,
                task.modified_at,
                Some("thumbnail generation failed"),
            );
        }
        emit_page_progress(app, processed, total);
    }

    db::update_asset_thumbnail_paths_batch(&conn, &updates)?;

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

fn emit_render_progress(
    app: &tauri::AppHandle<impl tauri::Runtime>,
    phase: &str,
    processed: usize,
    total: usize,
) {
    if processed % 25 == 0 || processed == total {
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
    if processed < total && processed % 16 != 0 {
        return;
    }
    if processed % 25 == 0 || processed == total {
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
    let removed = delete_thumbnail_files_with_progress(
        app,
        &state.thumbs_dir,
        thumbs,
        total,
        "thumbs-clear",
    );
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
        if processed % 25 == 0 || processed == total {
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
        sync::{atomic::AtomicBool, mpsc, Mutex, RwLock},
    };

    use tempfile::tempdir;

    use crate::{
        app::state::AppState, db, models::NewAsset, services::thumb_scheduler::ThumbnailScheduler,
    };

    use super::{
        cancel_render_all_thumbnails, delete_thumbnail_files, ensure_asset_thumbnail,
        wait_next_render_result, ThumbnailTaskResult,
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
            })
            .expect("send completion");

        let result = wait_next_render_result(&receiver, &cancel_requested)
            .expect("wait result")
            .expect("completion item");

        assert_eq!(result.asset_id, 42);
        assert_eq!(result.thumb_path.as_deref(), Some("C:/tmp/thumb-42.jpg"));
    }

    #[test]
    fn wait_next_render_result_returns_none_when_cancelled() {
        let cancel_requested = AtomicBool::new(true);
        let (_sender, receiver) = mpsc::channel::<ThumbnailTaskResult>();

        let result = wait_next_render_result(&receiver, &cancel_requested).expect("wait result");
        assert!(result.is_none());
    }

    #[test]
    fn wait_next_render_result_returns_error_when_channel_disconnected() {
        let cancel_requested = AtomicBool::new(false);
        let (sender, receiver) = mpsc::channel::<ThumbnailTaskResult>();
        drop(sender);

        let error =
            wait_next_render_result(&receiver, &cancel_requested).expect_err("disconnected error");
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
}
