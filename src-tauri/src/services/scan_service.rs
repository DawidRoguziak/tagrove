use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::UNIX_EPOCH,
};

use tauri::State;

use crate::{
    app::state::AppState,
    db,
    error::AppResult,
    indexer,
    models::{ScanCompletion, ScanSummary},
    services::progress::emit_progress,
};

#[cfg(test)]
use crate::db::delete_stale_assets_by_prefix;

const MAX_SCAN_WORKERS: usize = 8;
const SCAN_QUEUE_CAPACITY: usize = 1024;
const SCAN_DB_BATCH_SIZE: usize = 512;

pub fn sort_scan_roots_by_created_desc(roots: Vec<String>) -> Vec<String> {
    let mut roots_with_created_at = roots
        .into_iter()
        .map(|path| {
            let created_at = std::fs::metadata(&path)
                .and_then(|metadata| metadata.created())
                .ok();
            (path, created_at)
        })
        .collect::<Vec<_>>();

    roots_with_created_at.sort_by(|(left_path, left_created), (right_path, right_created)| {
        match (left_created, right_created) {
            (Some(left), Some(right)) => right
                .cmp(left)
                .then_with(|| left_path.cmp(right_path)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left_path.cmp(right_path),
        }
    });

    roots_with_created_at
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

pub fn scan_roots<R: tauri::Runtime>(
    roots: &[String],
    state: &State<AppState>,
    app: &tauri::AppHandle<R>,
) -> AppResult<ScanSummary> {
    if roots.is_empty() {
        let _ = emit_progress(
            app,
            "scan-empty",
            0,
            0,
            "No scan folders configured".to_string(),
        );
        return Ok(empty_scan_summary());
    }

    let started = std::time::Instant::now();
    let mut indexed = 0usize;
    let mut removed = 0usize;
    let mut failed = 0usize;
    let mut completion = ScanCompletion::Complete;
    let conn = db::open_connection(&state.db_path)?;

    for (root_idx, root_str) in roots.iter().enumerate() {
        let root = PathBuf::from(root_str);
        if !is_valid_scan_root(&root) {
            failed += 1;
            completion = ScanCompletion::Partial;
            let _ = emit_progress(app, "scan-skip", root_idx + 1, roots.len(), format!("Skipping invalid folder: {root_str}"));
            continue;
        }

        let generation = scan_generation().saturating_add(root_idx as i64);
        let scan_worker_count = resolve_scan_worker_count(usize::MAX);
        let (task_sender, task_receiver) = mpsc::sync_channel::<(
            PathBuf,
            String,
            indexer::FileFingerprint,
        )>(SCAN_QUEUE_CAPACITY);
        let (result_sender, result_receiver) = mpsc::channel::<indexer::IndexResult>();
        let shared_task_receiver = Arc::new(Mutex::new(task_receiver));
        let mut worker_handles = Vec::with_capacity(scan_worker_count);

        for worker_idx in 0..scan_worker_count {
            let receiver = Arc::clone(&shared_task_receiver);
            let sender = result_sender.clone();
            let ffmpeg_path = state.ffmpeg_path.clone();
            worker_handles.push(
                thread::Builder::new()
                    .name(format!("scan-worker-{worker_idx}"))
                    .spawn(move || loop {
                        let task = match receiver.lock() {
                            Ok(receiver) => receiver.recv(),
                            Err(_) => return,
                        };
                        let (path, kind, fingerprint) = match task {
                            Ok(task) => task,
                            Err(_) => break,
                        };
                        let result = indexer::scan_one_with_fingerprint(
                            path,
                            kind,
                            ffmpeg_path.as_path(),
                            fingerprint,
                        );
                        if sender.send(result).is_err() {
                            break;
                        }
                    })?,
            );
        }
        drop(result_sender);

        let mut unchanged_ids = Vec::<i64>::new();
        let mut unchanged_count = 0usize;
        let mut queued = 0usize;
        let mut discovery_failures = 0usize;
        let mut discovered_count = 0usize;
        let mut completed_changed = 0usize;
        let mut changed_indexed = 0usize;
        let mut changed_failed = 0usize;
        let mut pending_writes = Vec::with_capacity(SCAN_DB_BATCH_SIZE);
        let mut discovery_batch = Vec::<(PathBuf, String, indexer::FileFingerprint)>::with_capacity(SCAN_DB_BATCH_SIZE);
        let discovery = indexer::visit_supported_files(&root, |path, kind| {
            discovered_count += 1;
            let fingerprint = match indexer::read_file_fingerprint(&path) {
                Ok(value) => value,
                Err(_) => {
                    discovery_failures += 1;
                    if discovered_count % 100 == 0 {
                        let _ = emit_progress(
                            app,
                            "counting",
                            discovered_count,
                            discovered_count,
                            format!("[{}/{}] Discovered {discovered_count} files", root_idx + 1, roots.len()),
                        );
                    }
                    return Ok(());
                }
            };
            discovery_batch.push((path, kind, fingerprint));
            if discovery_batch.len() >= SCAN_DB_BATCH_SIZE {
                dispatch_discovery_batch(
                    &conn,
                    &mut discovery_batch,
                    &task_sender,
                    &mut unchanged_ids,
                    &mut unchanged_count,
                    &mut queued,
                    root_str,
                    generation,
                )?;
                drain_ready_scan_results(
                    &conn,
                    &result_receiver,
                    &mut pending_writes,
                    &mut completed_changed,
                    &mut changed_indexed,
                    &mut changed_failed,
                    root_str,
                    generation,
                )?;
            }
            if discovered_count % 100 == 0 {
                let _ = emit_progress(
                    app,
                    "counting",
                    discovered_count,
                    discovered_count,
                    format!("[{}/{}] Discovered {discovered_count} files", root_idx + 1, roots.len()),
                );
            }
            Ok(())
        })?;
        let total = discovery.discovered;
        dispatch_discovery_batch(
            &conn,
            &mut discovery_batch,
            &task_sender,
            &mut unchanged_ids,
            &mut unchanged_count,
            &mut queued,
            root_str,
            generation,
        )?;
        drain_ready_scan_results(
            &conn,
            &result_receiver,
            &mut pending_writes,
            &mut completed_changed,
            &mut changed_indexed,
            &mut changed_failed,
            root_str,
            generation,
        )?;
        drop(task_sender);

        if !unchanged_ids.is_empty() {
            db::touch_scan_root_assets(&conn, &unchanged_ids, root_str, generation)?;
            unchanged_count += unchanged_ids.len();
            unchanged_ids.clear();
        }
        while completed_changed < queued {
            let result = result_receiver
                .recv()
                .map_err(|e| format!("scan worker disconnected: {e}"))?;
            accept_scan_result(
                &conn,
                result,
                &mut pending_writes,
                &mut completed_changed,
                &mut changed_indexed,
                &mut changed_failed,
                root_str,
                generation,
            )?;
            let processed = unchanged_count + completed_changed;
            if processed % 100 == 0 || completed_changed == queued {
                let _ = emit_progress(
                    app,
                    "scanning",
                    processed,
                    total,
                    format!("[{}/{}] Scanning {processed}/{total}", root_idx + 1, roots.len()),
                );
            }
        }
        changed_indexed += flush_scan_batch(&conn, &mut pending_writes, root_str, generation)?;
        indexed += unchanged_count + changed_indexed;
        failed += discovery.traversal_errors + discovery_failures + changed_failed;

        for handle in worker_handles {
            handle.join().map_err(|_| "scan worker panicked".to_string())?;
        }

        let root_scan_complete =
            discovery.is_complete() && discovery_failures == 0 && changed_failed == 0;
        if root_scan_complete {
            let removed_for_root =
                db::prune_completed_scan_root_generation(&conn, root_str, generation)?;
            removed += removed_for_root;
            let _ = emit_progress(
                app,
                "cleanup",
                total,
                total,
                format!("[{}/{}] Cleanup finished, removed {removed_for_root} stale entries", root_idx + 1, roots.len()),
            );
        } else {
            completion = ScanCompletion::Partial;
            let warning_count =
                discovery.traversal_errors + discovery_failures + changed_failed;
            let _ = emit_progress(
                app,
                "cleanup",
                total,
                total,
                format!(
                    "[{}/{}] Partial scan finished with {warning_count} warning(s); cleanup skipped and existing records preserved",
                    root_idx + 1,
                    roots.len()
                ),
            );
        }
    }

    db::bump_library_revision(&conn)?;
    db::optimize(&conn)?;
    if std::env::var_os("MEDIATAGGER_PERF").is_some() {
        eprintln!("[perf] name=scan elapsed_ms={} indexed={indexed} removed={removed} failed={failed}", started.elapsed().as_millis());
    }
    Ok(ScanSummary {
        completion,
        indexed,
        removed,
        failed,
    })
}

fn scan_generation() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64
}

fn dispatch_discovery_batch(
    conn: &rusqlite::Connection,
    batch: &mut Vec<(PathBuf, String, indexer::FileFingerprint)>,
    sender: &mpsc::SyncSender<(PathBuf, String, indexer::FileFingerprint)>,
    unchanged_ids: &mut Vec<i64>,
    unchanged_count: &mut usize,
    queued: &mut usize,
    root: &str,
    generation: i64,
) -> anyhow::Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let paths = batch
        .iter()
        .map(|(path, _, _)| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let existing = db::list_asset_fingerprints_by_paths(conn, &paths)?;
    for (path, kind, fingerprint) in batch.drain(..) {
        let path_text = path.to_string_lossy().to_string();
        let current = existing.get(&path_text);
        if current.is_some_and(|value| {
            value.kind == kind
                && value.size_bytes == fingerprint.size_bytes
                && value.modified_at_ns == fingerprint.modified_at_ns
        }) {
            if let Some(current) = current {
                unchanged_ids.push(current.id);
            }
        } else {
            sender
                .send((path, kind, fingerprint))
                .map_err(|e| anyhow::anyhow!("scan task queue disconnected: {e}"))?;
            *queued += 1;
        }
    }
    if unchanged_ids.len() >= SCAN_DB_BATCH_SIZE {
        db::touch_scan_root_assets(conn, unchanged_ids, root, generation)?;
        *unchanged_count += unchanged_ids.len();
        unchanged_ids.clear();
    }
    Ok(())
}

fn flush_scan_batch(
    conn: &rusqlite::Connection,
    pending: &mut Vec<indexer::IndexedItem>,
    root: &str,
    generation: i64,
) -> AppResult<usize> {
    if pending.is_empty() {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    for item in pending.iter() {
        db::upsert_scanned_asset(
            &tx,
            &item.asset,
            item.fingerprint_mtime_ns,
            root,
            generation,
        )?;
    }
    // Every committed indexing batch invalidates query sessions immediately so
    // a crash can never leave indexed rows without a revision bump.
    db::bump_library_revision_in_tx(&tx)?;
    tx.commit()?;
    let written = pending.len();
    pending.clear();
    Ok(written)
}

fn drain_ready_scan_results(
    conn: &rusqlite::Connection,
    receiver: &mpsc::Receiver<indexer::IndexResult>,
    pending: &mut Vec<indexer::IndexedItem>,
    completed: &mut usize,
    indexed: &mut usize,
    failed: &mut usize,
    root: &str,
    generation: i64,
) -> AppResult<()> {
    while let Ok(result) = receiver.try_recv() {
        accept_scan_result(
            conn, result, pending, completed, indexed, failed, root, generation,
        )?;
    }
    Ok(())
}

fn accept_scan_result(
    conn: &rusqlite::Connection,
    result: indexer::IndexResult,
    pending: &mut Vec<indexer::IndexedItem>,
    completed: &mut usize,
    indexed: &mut usize,
    failed: &mut usize,
    root: &str,
    generation: i64,
) -> AppResult<()> {
    *completed += 1;
    *failed += result.failed;
    if let Some(item) = result.item {
        pending.push(item);
    }
    if pending.len() >= SCAN_DB_BATCH_SIZE {
        *indexed += flush_scan_batch(conn, pending, root, generation)?;
    }
    Ok(())
}

fn resolve_scan_worker_count(total_files: usize) -> usize {
    let available_parallelism = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4);
    resolve_scan_worker_count_with_parallelism(total_files, available_parallelism)
}

fn resolve_scan_worker_count_with_parallelism(
    total_files: usize,
    available_parallelism: usize,
) -> usize {
    if total_files == 0 {
        return 1;
    }

    let workers = available_parallelism.clamp(1, MAX_SCAN_WORKERS);
    workers.min(total_files)
}

fn empty_scan_summary() -> ScanSummary {
    ScanSummary {
        completion: ScanCompletion::Complete,
        indexed: 0,
        removed: 0,
        failed: 0,
    }
}

fn is_valid_scan_root(root: &Path) -> bool {
    root.exists() && root.is_dir()
}

#[cfg(test)]
fn cleanup_stale_assets_for_root(
    conn: &rusqlite::Connection,
    root: &str,
    now_unix: i64,
) -> AppResult<usize> {
    Ok(delete_stale_assets_by_prefix(conn, root, now_unix)?)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use crate::{db, models::NewAsset};

    use super::{
        cleanup_stale_assets_for_root, empty_scan_summary, is_valid_scan_root,
        resolve_scan_worker_count_with_parallelism,
    };

    fn new_asset(path: &Path, modified_at: i64) -> NewAsset {
        NewAsset {
            path: path.to_string_lossy().to_string(),
            kind: "image".to_string(),
            size_bytes: 10,
            modified_at,
            width: Some(100),
            height: Some(100),
            duration_ms: None,
            thumb_path: None,
        }
    }

    #[test]
    fn resolve_scan_worker_count_uses_single_worker_for_empty_batches() {
        assert_eq!(resolve_scan_worker_count_with_parallelism(0, 8), 1);
    }

    #[test]
    fn resolve_scan_worker_count_does_not_exceed_file_count() {
        assert_eq!(resolve_scan_worker_count_with_parallelism(3, 12), 3);
    }

    #[test]
    fn resolve_scan_worker_count_respects_available_parallelism() {
        assert_eq!(resolve_scan_worker_count_with_parallelism(200, 6), 6);
    }

    #[test]
    fn resolve_scan_worker_count_falls_back_to_at_least_one_worker() {
        assert_eq!(resolve_scan_worker_count_with_parallelism(5, 0), 1);
    }

    #[test]
    fn empty_scan_summary_returns_zeroed_counts() {
        let summary = empty_scan_summary();
        assert_eq!(summary.indexed, 0);
        assert_eq!(summary.removed, 0);
        assert_eq!(summary.failed, 0);
    }

    #[test]
    fn is_valid_scan_root_detects_missing_and_existing_directories() {
        let tmp = tempdir().expect("tempdir");
        let valid_root = tmp.path().join("library");
        fs::create_dir_all(&valid_root).expect("create valid root");
        let invalid_root = tmp.path().join("missing-root");

        assert!(is_valid_scan_root(&valid_root));
        assert!(!is_valid_scan_root(&invalid_root));
    }

    #[test]
    fn cleanup_stale_assets_for_root_removes_missing_files_under_root() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let root = tmp.path().join("library");
        fs::create_dir_all(&root).expect("create root");
        let current = root.join("current.png");
        image::RgbImage::new(2, 2)
            .save(&current)
            .expect("save current image");
        db::upsert_asset(&conn, &new_asset(&current, 10)).expect("seed current asset");

        let stale = root.join("stale.png");
        db::upsert_asset(&conn, &new_asset(&stale, 1)).expect("seed stale asset");

        conn.execute(
            "UPDATE assets SET indexed_at = 10 WHERE path = ?1",
            rusqlite::params![current.to_string_lossy().to_string()],
        )
        .expect("set current indexed_at");
        conn.execute(
            "UPDATE assets SET indexed_at = 1 WHERE path = ?1",
            rusqlite::params![stale.to_string_lossy().to_string()],
        )
        .expect("set stale indexed_at");

        let removed = cleanup_stale_assets_for_root(&conn, &root.to_string_lossy(), 5)
            .expect("cleanup stale");
        assert_eq!(removed, 1);

        let conn = db::open_connection(&db_path).expect("reopen db");
        let page = db::list_assets(&conn, 0, 50, &[], &[], None, false).expect("list assets");
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].path, current.to_string_lossy().to_string());
    }
}
