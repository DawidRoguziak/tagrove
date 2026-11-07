use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};

use crate::{
    app::state::AppState,
    db,
    error::AppResult,
    indexer,
    models::{ScanCompletion, ScanSummary},
    services::{
        db_pool::{OperationPermit, PooledConnection},
        progress::emit_progress,
    },
};

#[cfg(test)]
use crate::db::delete_stale_assets_by_prefix;

const MAX_SCAN_WORKERS: usize = 8;
const SCAN_QUEUE_CAPACITY: usize = 1024;
const SCAN_DB_BATCH_SIZE: usize = 512;
// While discovery dispatches one full batch, workers may finish the already
// queued tasks plus that batch before the next result drain.
const SCAN_RESULT_QUEUE_CAPACITY: usize =
    SCAN_QUEUE_CAPACITY + SCAN_DB_BATCH_SIZE + MAX_SCAN_WORKERS;
/// Minimum spacing of library-revision bumps from committed scan batches.
const SCAN_REVISION_INTERVAL: Duration = Duration::from_secs(1);

/// Decides which committed scan batches carry a library-revision bump.
///
/// Query sessions and duplicate snapshots live only in process memory, so the
/// revision has to move before readers can observe new rows for long, not with
/// every commit. The first batch bumps at once, later batches at most once per
/// interval, and the scan always bumps when it ends or fails.
#[derive(Debug)]
struct ScanRevisionThrottle {
    interval: Duration,
    last_bump: Option<Instant>,
    unbumped_commits: bool,
    bumps: usize,
}

impl ScanRevisionThrottle {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_bump: None,
            unbumped_commits: false,
            bumps: 0,
        }
    }

    /// Whether the batch about to commit must bump inside its transaction.
    fn due(&self, now: Instant) -> bool {
        self.last_bump
            .is_none_or(|last| now.saturating_duration_since(last) >= self.interval)
    }

    /// Records a committed query-visible change and whether it bumped.
    fn committed(&mut self, bumped: bool, now: Instant) {
        if bumped {
            self.last_bump = Some(now);
            self.unbumped_commits = false;
            self.bumps += 1;
        } else {
            self.unbumped_commits = true;
        }
    }
}

/// The scan's single pooled connection and its write policy: IMMEDIATE
/// transactions, yielding to waiting user writes, and throttled revisions.
struct ScanWriter<'a> {
    conn: PooledConnection,
    permit: &'a OperationPermit,
    revisions: ScanRevisionThrottle,
    yields: usize,
    yield_time: Duration,
}

impl<'a> ScanWriter<'a> {
    fn new(permit: &'a OperationPermit) -> AppResult<Self> {
        Ok(Self {
            conn: permit.connection()?,
            permit,
            revisions: ScanRevisionThrottle::new(SCAN_REVISION_INTERVAL),
            yields: 0,
            yield_time: Duration::ZERO,
        })
    }

    /// Lets queued favorite/tag/group writes commit before the next batch.
    fn yield_to_user_writes(&mut self) {
        let waited = self.permit.yield_to_user_writes();
        if !waited.is_zero() {
            self.yields += 1;
            self.yield_time += waited;
        }
    }

    fn begin_immediate(&self) -> rusqlite::Result<rusqlite::Transaction<'_>> {
        rusqlite::Transaction::new_unchecked(&self.conn, rusqlite::TransactionBehavior::Immediate)
    }

    /// The unconditional bump after all roots; its failure fails the scan.
    fn finish(&mut self) -> AppResult<()> {
        db::bump_library_revision(&self.conn)?;
        self.revisions.committed(true, Instant::now());
        Ok(())
    }
}

impl Drop for ScanWriter<'_> {
    fn drop(&mut self) {
        // An error or panic must not leave committed rows behind an unchanged
        // revision for the rest of the process. Best effort: the error that
        // ended the scan is already being reported.
        if self.revisions.unbumped_commits {
            let _ = db::bump_library_revision(&self.conn);
        }
    }
}

pub fn sort_scan_roots_by_created_desc(
    roots: Vec<crate::models::ScanRoot>,
) -> Vec<crate::models::ScanRoot> {
    let mut roots_with_created_at = roots
        .into_iter()
        .map(|path| {
            let created_at = std::fs::metadata(&path.path)
                .and_then(|metadata| metadata.created())
                .ok();
            (path, created_at)
        })
        .collect::<Vec<_>>();

    roots_with_created_at.sort_by(
        |(left_path, left_created), (right_path, right_created)| match (left_created, right_created)
        {
            (Some(left), Some(right)) => right
                .cmp(left)
                .then_with(|| left_path.path.cmp(&right_path.path)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left_path.path.cmp(&right_path.path),
        },
    );

    roots_with_created_at
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

pub fn scan_roots(
    roots: &[String],
    state: &AppState,
    permit: &crate::services::db_pool::OperationPermit,
    app: &impl crate::services::progress::ProgressSink,
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
    let mut writer = ScanWriter::new(permit)?;

    for (root_idx, root_str) in roots.iter().enumerate() {
        let root = PathBuf::from(root_str);
        if !is_valid_scan_root(&root) {
            failed += 1;
            completion = ScanCompletion::Partial;
            let _ = emit_progress(
                app,
                "scan-skip",
                root_idx + 1,
                roots.len(),
                format!("Skipping invalid folder: {root_str}"),
            );
            continue;
        }

        let generation = scan_generation().saturating_add(root_idx as i64);
        let scan_worker_count = resolve_scan_worker_count(usize::MAX);
        let (task_sender, task_receiver) =
            mpsc::sync_channel::<(PathBuf, String, indexer::FileFingerprint)>(SCAN_QUEUE_CAPACITY);
        let (result_sender, result_receiver) =
            mpsc::sync_channel::<indexer::IndexResult>(SCAN_RESULT_QUEUE_CAPACITY);
        let shared_task_receiver = Arc::new(Mutex::new(task_receiver));
        let mut worker_handles = Vec::with_capacity(scan_worker_count);

        for worker_idx in 0..scan_worker_count {
            let receiver = Arc::clone(&shared_task_receiver);
            let sender = result_sender.clone();
            let ffmpeg_path = state.ffmpeg_path.clone();
            let operation = permit.clone();
            worker_handles.push(
                thread::Builder::new()
                    .name(format!("scan-worker-{worker_idx}"))
                    .spawn(move || {
                        let _operation = operation;
                        loop {
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
        let mut discovery_batch =
            Vec::<(PathBuf, String, indexer::FileFingerprint)>::with_capacity(SCAN_DB_BATCH_SIZE);
        let discovery = indexer::visit_supported_files(&root, |path, kind| {
            discovered_count += 1;
            let fingerprint = match indexer::read_file_fingerprint(&path) {
                Ok(value) => value,
                Err(_) => {
                    discovery_failures += 1;
                    if discovered_count.is_multiple_of(100) {
                        let _ = emit_progress(
                            app,
                            "counting",
                            discovered_count,
                            discovered_count,
                            format!(
                                "[{}/{}] Discovered {discovered_count} files",
                                root_idx + 1,
                                roots.len()
                            ),
                        );
                    }
                    return Ok(());
                }
            };
            discovery_batch.push((path, kind, fingerprint));
            if discovery_batch.len() >= SCAN_DB_BATCH_SIZE {
                dispatch_discovery_batch(
                    &mut writer,
                    &mut discovery_batch,
                    &task_sender,
                    &mut unchanged_ids,
                    &mut unchanged_count,
                    &mut queued,
                    root_str,
                    generation,
                )?;
                drain_ready_scan_results(
                    &mut writer,
                    &result_receiver,
                    &mut pending_writes,
                    &mut completed_changed,
                    &mut changed_indexed,
                    &mut changed_failed,
                    root_str,
                    generation,
                )?;
            }
            if discovered_count.is_multiple_of(100) {
                let _ = emit_progress(
                    app,
                    "counting",
                    discovered_count,
                    discovered_count,
                    format!(
                        "[{}/{}] Discovered {discovered_count} files",
                        root_idx + 1,
                        roots.len()
                    ),
                );
            }
            Ok(())
        })?;
        let total = discovery.discovered;
        dispatch_discovery_batch(
            &mut writer,
            &mut discovery_batch,
            &task_sender,
            &mut unchanged_ids,
            &mut unchanged_count,
            &mut queued,
            root_str,
            generation,
        )?;
        drain_ready_scan_results(
            &mut writer,
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
            writer.yield_to_user_writes();
            db::touch_scan_root_assets(&writer.conn, &unchanged_ids, root_str, generation)?;
            unchanged_count += unchanged_ids.len();
            unchanged_ids.clear();
        }
        while completed_changed < queued {
            let result = result_receiver
                .recv()
                .map_err(|e| format!("scan worker disconnected: {e}"))?;
            accept_scan_result(
                &mut writer,
                result,
                &mut pending_writes,
                &mut completed_changed,
                &mut changed_indexed,
                &mut changed_failed,
                root_str,
                generation,
            )?;
            let processed = unchanged_count + completed_changed;
            if processed.is_multiple_of(100) || completed_changed == queued {
                let _ = emit_progress(
                    app,
                    "scanning",
                    processed,
                    total,
                    format!(
                        "[{}/{}] Scanning {processed}/{total}",
                        root_idx + 1,
                        roots.len()
                    ),
                );
            }
        }
        changed_indexed +=
            flush_scan_batch(&mut writer, &mut pending_writes, root_str, generation)?;
        indexed += unchanged_count + changed_indexed;
        failed += discovery.traversal_errors + discovery_failures + changed_failed;

        for handle in worker_handles {
            handle
                .join()
                .map_err(|_| "scan worker panicked".to_string())?;
        }

        let root_scan_complete =
            discovery.is_complete() && discovery_failures == 0 && changed_failed == 0;
        if root_scan_complete {
            writer.yield_to_user_writes();
            let removed_for_root =
                db::prune_completed_scan_root_generation(&writer.conn, root_str, generation)?;
            if removed_for_root > 0 {
                // The prune bumped inside its own transaction.
                writer.revisions.committed(true, Instant::now());
            }
            removed += removed_for_root;
            let _ = emit_progress(
                app,
                "cleanup",
                total,
                total,
                format!(
                    "[{}/{}] Cleanup finished, removed {removed_for_root} stale entries",
                    root_idx + 1,
                    roots.len()
                ),
            );
        } else {
            completion = ScanCompletion::Partial;
            let warning_count = discovery.traversal_errors + discovery_failures + changed_failed;
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

    writer.finish()?;
    db::optimize(&writer.conn)?;
    if std::env::var_os("MEDIATAGGER_PERF").is_some() {
        eprintln!(
            "[perf] name=scan elapsed_ms={} indexed={indexed} removed={removed} failed={failed} revision_bumps={} user_write_yields={} yield_ms={}",
            started.elapsed().as_millis(),
            writer.revisions.bumps,
            writer.yields,
            writer.yield_time.as_millis()
        );
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

#[allow(clippy::too_many_arguments)]
fn dispatch_discovery_batch(
    writer: &mut ScanWriter<'_>,
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
    let existing = db::list_asset_fingerprints_by_paths(&writer.conn, &paths)?;
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
        writer.yield_to_user_writes();
        db::touch_scan_root_assets(&writer.conn, unchanged_ids, root, generation)?;
        *unchanged_count += unchanged_ids.len();
        unchanged_ids.clear();
    }
    Ok(())
}

fn flush_scan_batch(
    writer: &mut ScanWriter<'_>,
    pending: &mut Vec<indexer::IndexedItem>,
    root: &str,
    generation: i64,
) -> AppResult<usize> {
    if pending.is_empty() {
        return Ok(0);
    }
    writer.yield_to_user_writes();
    let bump = writer.revisions.due(Instant::now());
    let tx = writer.begin_immediate()?;
    for item in pending.iter() {
        db::upsert_scanned_asset(
            &tx,
            &item.asset,
            item.fingerprint_mtime_ns,
            root,
            generation,
        )?;
    }
    // Sessions are in-memory, so a crash cannot leave a stale one behind; a
    // live process sees these rows after the next throttled, final, or
    // error-path bump (`ScanRevisionThrottle`, `ScanWriter::drop`).
    if bump {
        db::bump_library_revision_in_tx(&tx)?;
    }
    tx.commit()?;
    writer.revisions.committed(bump, Instant::now());
    let written = pending.len();
    pending.clear();
    Ok(written)
}

#[allow(clippy::too_many_arguments)]
fn drain_ready_scan_results(
    writer: &mut ScanWriter<'_>,
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
            writer, result, pending, completed, indexed, failed, root, generation,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn accept_scan_result(
    writer: &mut ScanWriter<'_>,
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
        *indexed += flush_scan_batch(writer, pending, root, generation)?;
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
    fn revision_throttle_bumps_first_batch_then_once_per_interval() {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let second = Duration::from_secs(1);
        let mut throttle = super::ScanRevisionThrottle::new(second);
        assert!(throttle.due(start), "the first batch always bumps");
        throttle.committed(true, start);
        assert!(!throttle.unbumped_commits);

        for offset in [1, 400, 999] {
            let now = start + Duration::from_millis(offset);
            assert!(!throttle.due(now), "batch at +{offset} ms must not bump");
            throttle.committed(false, now);
        }
        assert!(throttle.unbumped_commits);

        let later = start + second;
        assert!(throttle.due(later));
        throttle.committed(true, later);
        assert!(!throttle.unbumped_commits);
        assert_eq!(throttle.bumps, 2);
    }

    fn writer_fixture() -> (
        tempfile::TempDir,
        std::sync::Arc<crate::services::db_pool::DatabaseRuntime>,
    ) {
        let dir = tempdir().expect("tempdir");
        let runtime = crate::services::db_pool::DatabaseRuntime::new(dir.path().join("media.db"));
        db::init_schema(&runtime.admit().unwrap().connection().unwrap()).unwrap();
        (dir, runtime)
    }

    fn revision(runtime: &std::sync::Arc<crate::services::db_pool::DatabaseRuntime>) -> i64 {
        db::current_library_revision(&runtime.admit().unwrap().connection().unwrap()).unwrap()
    }

    #[test]
    fn scan_writer_bumps_on_early_exit_only_when_commits_are_uncovered() {
        let (_dir, runtime) = writer_fixture();
        let permit = runtime.admit().unwrap();
        let before = revision(&runtime);

        // Error/panic exit after an unbumped batch: Drop bumps once.
        let mut writer = super::ScanWriter::new(&permit).unwrap();
        writer.revisions.committed(true, std::time::Instant::now());
        writer.revisions.committed(false, std::time::Instant::now());
        drop(writer);
        assert_eq!(revision(&runtime), before + 1);

        // Every commit already covered: no extra bump.
        let mut writer = super::ScanWriter::new(&permit).unwrap();
        writer.revisions.committed(true, std::time::Instant::now());
        drop(writer);
        assert_eq!(revision(&runtime), before + 1);

        // The success path always bumps, and Drop then has nothing to do.
        let mut writer = super::ScanWriter::new(&permit).unwrap();
        writer.revisions.committed(false, std::time::Instant::now());
        writer.finish().unwrap();
        drop(writer);
        assert_eq!(revision(&runtime), before + 2);
    }

    #[test]
    fn scan_holds_one_connection_and_throttles_batch_revisions() {
        let (dir, runtime) = writer_fixture();
        let root = dir.path().join("library");
        fs::create_dir_all(&root).unwrap();
        let seed = root.join("seed.png");
        image::RgbImage::new(2, 2).save(&seed).unwrap();
        // Three changed-file batches (512 + 512 + 476).
        for index in 1..1_500 {
            fs::copy(&seed, root.join(format!("copy-{index}.png"))).unwrap();
        }
        db::add_scan_root(
            &runtime.admit().unwrap().connection().unwrap(),
            &root.to_string_lossy(),
        )
        .unwrap();
        let state = crate::app::state::AppState {
            database: runtime.clone(),
            db_path: dir.path().join("media.db"),
            thumbs_dir: dir.path().join("thumbs"),
            ffmpeg_path: "ffmpeg".into(),
            scan_lock: std::sync::Mutex::new(()),
            thumb_lock: std::sync::RwLock::new(()),
            thumb_scheduler: crate::services::thumb_scheduler::ThumbnailScheduler::new(
                1,
                "ffmpeg".into(),
            ),
            thumbnail_render_all_running: std::sync::atomic::AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: std::sync::atomic::AtomicBool::new(false),
            thumbnail_generation: std::sync::atomic::AtomicU64::new(0),
        };
        let before = revision(&runtime);
        let most_checked_out = std::sync::atomic::AtomicUsize::new(0);
        let sink = |_progress: crate::models::ScanProgress| -> crate::error::AppResult<()> {
            most_checked_out.fetch_max(
                runtime.checked_out_connections(),
                std::sync::atomic::Ordering::SeqCst,
            );
            Ok(())
        };
        let started = std::time::Instant::now();
        let permit = runtime.admit().unwrap();
        let summary =
            super::scan_roots(&[root.to_string_lossy().into()], &state, &permit, &sink).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(summary.indexed, 1_500);
        assert_eq!(
            most_checked_out.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let bumps = revision(&runtime) - before;
        // First batch plus the final bump, and at most one per elapsed second.
        assert!(bumps >= 2, "{bumps}");
        assert!(
            bumps <= 2 + elapsed.as_secs() as i64,
            "{bumps} in {elapsed:?}"
        );
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
        let page = db::test_support::session_page(&conn, 0, 50, &[], &[], None, false)
            .expect("list assets");
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].path, current.to_string_lossy().to_string());
    }
}
