use std::{
    collections::{HashMap, VecDeque},
    num::NonZeroUsize,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Condvar, Mutex,
    },
    thread,
};

use crate::thumbs;

/// Hard cap on distinct queued and running jobs combined. Enqueueing
/// beyond this bound fails fast instead of growing memory without limits.
pub const MAX_PENDING_JOBS: usize = 2048;
pub const MAX_WAITERS_PER_JOB: usize = 256;

#[derive(Clone, Copy)]
struct ThumbnailConcurrency {
    browsing: usize,
    bulk: usize,
}

impl ThumbnailConcurrency {
    fn from_parallelism(parallelism: Option<NonZeroUsize>) -> Self {
        Self {
            browsing: parallelism
                .map(|count| count.get().saturating_sub(2).clamp(2, 8))
                .unwrap_or(4),
            bulk: parallelism.map(NonZeroUsize::get).unwrap_or(4),
        }
    }

    fn bulk_in_flight_limit(self) -> usize {
        self.bulk.saturating_mul(3).clamp(24, MAX_PENDING_JOBS)
    }
}

/// Restores browsing admission even when the bulk coordinator unwinds.
#[must_use]
pub(crate) struct BulkModeGuard {
    shared: Arc<SchedulerShared>,
}

impl Drop for BulkModeGuard {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.bulk_scopes -= 1;
        drop(state);
        self.shared.condvar.notify_all();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailPriority {
    /// `thumb://` requests: served newest first (LIFO), ahead of High and Low,
    /// so tiles that just came into view render before ones scrolled past.
    Interactive,
    High,
    Low,
}

#[derive(Debug, Clone)]
pub struct ThumbnailTask {
    pub operation: Option<crate::services::db_pool::OperationPermit>,
    pub asset_id: i64,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub kind: String,
    pub duration_ms: Option<i64>,
    /// Exact source version the render belongs to; echoed back in the result
    /// and used as the SQL compare-and-set guard on publication.
    pub source_version: crate::thumbs::SourceVersion,
    /// Owned thumbnail file recorded for this source version under an earlier
    /// cache version. The default processor moves it to the target when the
    /// current rules draw the same picture, and removes it after a fresh render.
    pub legacy_thumb: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ThumbnailTaskResult {
    pub asset_id: i64,
    pub thumb_path: Option<String>,
    /// Version snapshot taken when the task was created. If it no longer
    /// matches the database record, the caller must discard the file.
    pub source_version: crate::thumbs::SourceVersion,
}

#[derive(Clone)]
pub struct ThumbnailScheduler {
    shared: Arc<SchedulerShared>,
}

struct SchedulerShared {
    state: Mutex<SchedulerState>,
    condvar: Condvar,
    ffmpeg_path: PathBuf,
    processor: TaskProcessor,
    worker_spawn_failures: AtomicUsize,
    /// `(asset_id, record_version)` pairs whose on-demand render failed in
    /// this process. `thumb://` answers them with 404 instead of re-rendering
    /// on every tile mount; a failure marker from an earlier run gets one
    /// retry. The bulk "render failed" action does not consult this set.
    interactive_failures: Mutex<std::collections::HashSet<(i64, i64)>>,
}

struct SchedulerState {
    concurrency: ThumbnailConcurrency,
    bulk_scopes: usize,
    active_jobs: usize,
    jobs: HashMap<String, ScheduledJob>,
    /// LIFO: new and repeated requests go to the front.
    interactive_queue: VecDeque<String>,
    high_queue: VecDeque<String>,
    low_queue: VecDeque<String>,
    active_videos: usize,
}

struct ScheduledJob {
    task: ThumbnailTask,
    priority: ThumbnailPriority,
    waiters: Vec<JobWaiter>,
}

struct JobWaiter {
    asset_id: i64,
    sink: WaiterSink,
}

/// Callback invoked once with the job result, on the worker thread.
pub type ThumbnailResultCallback = Box<dyn FnOnce(ThumbnailTaskResult) + Send + 'static>;

enum WaiterSink {
    Channel(Sender<ThumbnailTaskResult>),
    Callback(ThumbnailResultCallback),
}

impl WaiterSink {
    fn deliver(self, result: ThumbnailTaskResult) {
        match self {
            Self::Channel(sender) => {
                let _ = sender.send(result);
            }
            Self::Callback(callback) => callback(result),
        }
    }
}

type TaskProcessor = Arc<dyn Fn(&Path, &ThumbnailTask) -> Option<String> + Send + Sync + 'static>;

impl ThumbnailScheduler {
    pub fn from_available_parallelism(ffmpeg_path: PathBuf) -> Self {
        Self::with_concurrency_and_processor(
            ThumbnailConcurrency::from_parallelism(thread::available_parallelism().ok()),
            ffmpeg_path,
            Arc::new(process_thumbnail_task),
        )
    }

    pub fn new(worker_count: usize, ffmpeg_path: PathBuf) -> Self {
        Self::with_processor(worker_count, ffmpeg_path, Arc::new(process_thumbnail_task))
    }

    pub fn enqueue(
        &self,
        task: ThumbnailTask,
        priority: ThumbnailPriority,
    ) -> Result<Receiver<ThumbnailTaskResult>, String> {
        let asset_id = task.asset_id;
        let key = task.target_path.to_string_lossy().to_string();
        let (sender, receiver) = mpsc::channel();

        self.enqueue_waiter(
            key,
            task,
            priority,
            JobWaiter {
                asset_id,
                sink: WaiterSink::Channel(sender),
            },
        )?;

        Ok(receiver)
    }

    /// Enqueues a job whose result is handed to `callback` on the worker
    /// thread, so the caller never parks a thread while the render runs.
    /// On error the callback is dropped without being called.
    pub fn enqueue_with_callback(
        &self,
        task: ThumbnailTask,
        priority: ThumbnailPriority,
        callback: ThumbnailResultCallback,
    ) -> Result<(), String> {
        let asset_id = task.asset_id;
        let key = task.target_path.to_string_lossy().to_string();
        self.enqueue_waiter(
            key,
            task,
            priority,
            JobWaiter {
                asset_id,
                sink: WaiterSink::Callback(callback),
            },
        )
    }

    /// Number of worker threads that failed to start. A scheduler that could
    /// not launch its workers must be treated as unhealthy: demand calls would
    /// block forever waiting for results that can never arrive.
    pub fn worker_spawn_failures(&self) -> usize {
        self.shared.worker_spawn_failures.load(Ordering::SeqCst)
    }

    pub(crate) fn enter_bulk_mode(&self) -> Result<BulkModeGuard, String> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|e| format!("thumbnail scheduler lock error: {e}"))?;
        state.bulk_scopes += 1;
        drop(state);
        self.shared.condvar.notify_all();
        Ok(BulkModeGuard {
            shared: Arc::clone(&self.shared),
        })
    }

    pub(crate) fn bulk_in_flight_limit(&self) -> usize {
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.concurrency.bulk_in_flight_limit()
    }

    pub fn remember_interactive_failure(&self, asset_id: i64, record_version: i64) {
        self.shared
            .interactive_failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((asset_id, record_version));
    }

    pub fn failed_interactively(&self, asset_id: i64, record_version: i64) -> bool {
        self.shared
            .interactive_failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&(asset_id, record_version))
    }

    pub fn is_healthy(&self) -> bool {
        self.worker_spawn_failures() == 0
    }

    pub fn enqueue_with_sender(
        &self,
        task: ThumbnailTask,
        priority: ThumbnailPriority,
        sender: Sender<ThumbnailTaskResult>,
    ) -> Result<(), String> {
        let asset_id = task.asset_id;
        let key = task.target_path.to_string_lossy().to_string();

        self.enqueue_waiter(
            key,
            task,
            priority,
            JobWaiter {
                asset_id,
                sink: WaiterSink::Channel(sender),
            },
        )
    }

    fn enqueue_waiter(
        &self,
        key: String,
        task: ThumbnailTask,
        priority: ThumbnailPriority,
        waiter: JobWaiter,
    ) -> Result<(), String> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|e| format!("thumbnail scheduler lock error: {e}"))?;

        if !state.jobs.contains_key(&key) && state.jobs.len() >= MAX_PENDING_JOBS {
            return Err(format!(
                "thumbnail scheduler queue is full ({MAX_PENDING_JOBS} pending jobs)"
            ));
        }

        if state.jobs.contains_key(&key) {
            if state
                .jobs
                .get(&key)
                .is_some_and(|job| job.waiters.len() >= MAX_WAITERS_PER_JOB)
            {
                return Err(format!(
                    "thumbnail job waiter limit reached ({MAX_WAITERS_PER_JOB} waiters)"
                ));
            }
            let current = state
                .jobs
                .get(&key)
                .map(|job| job.priority)
                .expect("existing thumbnail job to be present");
            let target = stronger_priority(current, priority);
            // A queued job moves to the target queue; an Interactive repeat
            // also moves to the front (LIFO). Running jobs are in no queue.
            if target != current || target == ThumbnailPriority::Interactive {
                if let Some(queued) = remove_from_queue(&mut state, current, &key) {
                    push_to_queue(&mut state, target, queued);
                    if let Some(job) = state.jobs.get_mut(&key) {
                        job.priority = target;
                    }
                }
            }

            let existing = state
                .jobs
                .get_mut(&key)
                .expect("existing thumbnail job to be present");
            existing.waiters.push(waiter);
            if target == ThumbnailPriority::Interactive {
                drop(state);
                self.shared.condvar.notify_one();
            }
            return Ok(());
        }

        state.jobs.insert(
            key.clone(),
            ScheduledJob {
                task: task.clone(),
                priority,
                waiters: vec![waiter],
            },
        );

        push_to_queue(&mut state, priority, key);

        drop(state);
        self.shared.condvar.notify_one();
        Ok(())
    }

    pub(crate) fn with_processor(
        worker_count: usize,
        ffmpeg_path: PathBuf,
        processor: TaskProcessor,
    ) -> Self {
        Self::with_concurrency_and_processor(
            ThumbnailConcurrency {
                browsing: worker_count.max(1),
                bulk: worker_count.max(1),
            },
            ffmpeg_path,
            processor,
        )
    }

    fn with_concurrency_and_processor(
        concurrency: ThumbnailConcurrency,
        ffmpeg_path: PathBuf,
        processor: TaskProcessor,
    ) -> Self {
        let shared = Arc::new(SchedulerShared {
            state: Mutex::new(SchedulerState {
                concurrency,
                bulk_scopes: 0,
                active_jobs: 0,
                jobs: HashMap::new(),
                interactive_queue: VecDeque::new(),
                high_queue: VecDeque::new(),
                low_queue: VecDeque::new(),
                active_videos: 0,
            }),
            condvar: Condvar::new(),
            ffmpeg_path,
            processor,
            worker_spawn_failures: AtomicUsize::new(0),
            interactive_failures: Mutex::new(std::collections::HashSet::new()),
        });

        let threads = concurrency.browsing.max(concurrency.bulk);
        for idx in 0..threads {
            let worker_shared = Arc::clone(&shared);
            let spawn_result = thread::Builder::new()
                .name(format!("thumb-worker-{idx}"))
                .spawn(move || worker_loop(worker_shared));
            if spawn_result.is_err() {
                shared.worker_spawn_failures.fetch_add(1, Ordering::SeqCst);
            }
        }

        Self { shared }
    }
}

fn worker_loop(shared: Arc<SchedulerShared>) {
    loop {
        let (key, task) = {
            let mut state = match shared.state.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };

            loop {
                if let Some(key) = pop_runnable_key(&mut state) {
                    let task = if let Some(job) = state.jobs.get(&key) {
                        job.task.clone()
                    } else {
                        continue;
                    };
                    if uses_video_slot(&task) {
                        state.active_videos += 1;
                    }
                    state.active_jobs += 1;
                    break (key, task);
                }

                state = match shared.condvar.wait(state) {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
            }
        };

        // A panicking processor must never leave a job without a terminal
        // result: waiters would block forever and a bulk command holding the
        // coordination channel would hang. Convert the panic into a failed
        // result (None path) and always deliver to every waiter.
        let thumb_path = catch_unwind(AssertUnwindSafe(|| {
            (shared.processor)(shared.ffmpeg_path.as_path(), &task)
        }))
        .unwrap_or_else(|_| {
            eprintln!(
                "thumbnail processor panicked for asset {}",
                task.source_version.path
            );
            None
        });

        let waiters = {
            let mut state = match shared.state.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            if uses_video_slot(&task) {
                state.active_videos = state.active_videos.saturating_sub(1);
            }
            state.active_jobs -= 1;
            let waiters = state
                .jobs
                .remove(&key)
                .map(|job| job.waiters)
                .unwrap_or_default();
            shared.condvar.notify_all();
            waiters
        };

        for waiter in waiters {
            let result = ThumbnailTaskResult {
                asset_id: waiter.asset_id,
                thumb_path: thumb_path.clone(),
                source_version: task.source_version.clone(),
            };
            // A panicking callback must not take the worker thread down.
            let _ = catch_unwind(AssertUnwindSafe(|| waiter.sink.deliver(result)));
        }
    }
}

/// Whether a job counts against the video cap. A video job carrying a legacy
/// thumbnail normally only renames that file (ADR 0020), so it must not wait
/// behind ffmpeg renders; if the rename fails it renders outside the cap.
fn uses_video_slot(task: &ThumbnailTask) -> bool {
    task.kind == "video" && task.legacy_thumb.is_none()
}

fn pop_runnable_key(state: &mut SchedulerState) -> Option<String> {
    let (total_limit, video_limit) = if state.bulk_scopes > 0 {
        (state.concurrency.bulk, state.concurrency.bulk)
    } else {
        (state.concurrency.browsing, 2)
    };
    if state.active_jobs >= total_limit {
        return None;
    }
    for priority in [
        ThumbnailPriority::Interactive,
        ThumbnailPriority::High,
        ThumbnailPriority::Low,
    ] {
        let position = queue_for(state, priority).iter().position(|key| {
            state
                .jobs
                .get(key)
                .map(|job| !uses_video_slot(&job.task) || state.active_videos < video_limit)
                .unwrap_or(true)
        });
        if let Some(position) = position {
            return queue_for_mut(state, priority).remove(position);
        }
    }
    None
}

fn priority_rank(priority: ThumbnailPriority) -> u8 {
    match priority {
        ThumbnailPriority::Interactive => 2,
        ThumbnailPriority::High => 1,
        ThumbnailPriority::Low => 0,
    }
}

fn stronger_priority(a: ThumbnailPriority, b: ThumbnailPriority) -> ThumbnailPriority {
    if priority_rank(b) > priority_rank(a) {
        b
    } else {
        a
    }
}

fn queue_for(state: &SchedulerState, priority: ThumbnailPriority) -> &VecDeque<String> {
    match priority {
        ThumbnailPriority::Interactive => &state.interactive_queue,
        ThumbnailPriority::High => &state.high_queue,
        ThumbnailPriority::Low => &state.low_queue,
    }
}

fn queue_for_mut(state: &mut SchedulerState, priority: ThumbnailPriority) -> &mut VecDeque<String> {
    match priority {
        ThumbnailPriority::Interactive => &mut state.interactive_queue,
        ThumbnailPriority::High => &mut state.high_queue,
        ThumbnailPriority::Low => &mut state.low_queue,
    }
}

fn push_to_queue(state: &mut SchedulerState, priority: ThumbnailPriority, key: String) {
    match priority {
        ThumbnailPriority::Interactive => state.interactive_queue.push_front(key),
        ThumbnailPriority::High => state.high_queue.push_back(key),
        ThumbnailPriority::Low => state.low_queue.push_back(key),
    }
}

/// Removes a queued key; `None` when the job is running (in no queue).
fn remove_from_queue(
    state: &mut SchedulerState,
    priority: ThumbnailPriority,
    key: &str,
) -> Option<String> {
    let queue = queue_for_mut(state, priority);
    let index = queue.iter().position(|queued| queued == key)?;
    queue.remove(index)
}

/// Moves a thumbnail of an earlier cache version to the current target when
/// the current renderer would draw the same picture (ADR 0020): video frames
/// did not change, and an image is only drawn differently when it carries an
/// EXIF orientation transform. An orientation that cannot be read keeps the
/// recorded picture, as the demand path served it before.
fn adopt_legacy_thumbnail(task: &ThumbnailTask, legacy: &Path) -> bool {
    let same_picture = task.kind == "video"
        || !matches!(
            thumbs::image_orientation(&task.source_path),
            Ok(orientation) if orientation != image::metadata::Orientation::NoTransforms
        );
    same_picture
        && legacy != task.target_path
        && is_owned_legacy(legacy, &task.target_path)
        && std::fs::rename(legacy, &task.target_path).is_ok()
}

/// Whether `legacy` is a regular file (not a symbolic link) inside the
/// thumbnail directory of `target`. `thumb_path` is untrusted, so this is
/// checked again right before the file is moved, removed or served.
fn is_owned_legacy(legacy: &Path, target: &Path) -> bool {
    std::fs::symlink_metadata(legacy).is_ok_and(|metadata| metadata.file_type().is_file())
        && target
            .parent()
            .and_then(|root| root.canonicalize().ok())
            .is_some_and(|root| {
                legacy
                    .canonicalize()
                    .is_ok_and(|canonical| canonical.starts_with(root))
            })
}

fn process_thumbnail_task(ffmpeg_path: &Path, task: &ThumbnailTask) -> Option<String> {
    if !task.source_path.exists() {
        // An unreachable source can be neither checked nor rendered, so its
        // recorded legacy thumbnail stays in use (ADR 0018, ADR 0020).
        return task
            .legacy_thumb
            .as_deref()
            .filter(|path| is_owned_legacy(path, &task.target_path))
            .map(|path| path.to_string_lossy().into_owned());
    }

    if !task.target_path.exists() {
        if let Some(legacy) = &task.legacy_thumb {
            adopt_legacy_thumbnail(task, legacy);
        }
    }

    if !task.target_path.exists() {
        if task.kind == "video" {
            let seek_seconds = thumbs::resolve_video_seek_seconds(task.duration_ms);
            // Fast keyframe pass first, then the exact frame, then the first frame.
            let attempts = [
                (seek_seconds, thumbs::VideoFrameSeek::Keyframe),
                (seek_seconds, thumbs::VideoFrameSeek::Exact),
                (0.0, thumbs::VideoFrameSeek::Exact),
            ];
            for (index, (seek, frame_seek)) in attempts.into_iter().enumerate() {
                if index == 2 && seek_seconds <= 0.0 {
                    break;
                }
                let result = thumbs::create_video_thumb(
                    ffmpeg_path,
                    &task.source_path,
                    &task.target_path,
                    seek,
                    frame_seek,
                );
                if result.is_ok() && task.target_path.exists() {
                    break;
                }
            }
        } else {
            let _ = thumbs::create_image_thumb(&task.source_path, &task.target_path);
        }
    }

    if !task.target_path.exists() {
        return None;
    }

    // A freshly rendered target supersedes the recorded file. Requests that
    // still read the old reference fall through to the target.
    if let Some(legacy) = task
        .legacy_thumb
        .as_deref()
        .filter(|path| *path != task.target_path && is_owned_legacy(path, &task.target_path))
    {
        let _ = std::fs::remove_file(legacy);
    }

    Some(task.target_path.to_string_lossy().to_string())
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::time::{Duration, Instant};

    pub const WAIT: Duration = Duration::from_secs(5);

    pub struct StartedJob {
        pub asset_id: i64,
        pub release: Sender<()>,
    }

    pub fn controlled_scheduler(cpus: usize) -> (ThumbnailScheduler, Receiver<StartedJob>) {
        let (started_tx, started_rx) = mpsc::channel();
        let scheduler = ThumbnailScheduler::with_concurrency_and_processor(
            ThumbnailConcurrency::from_parallelism(NonZeroUsize::new(cpus)),
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                let (release, wait) = mpsc::channel();
                started_tx
                    .send(StartedJob {
                        asset_id: task.asset_id,
                        release,
                    })
                    .unwrap();
                wait.recv_timeout(WAIT).expect("test must release the job");
                // Callers that serve the file need it on disk. Only absolute
                // (temporary-directory) targets; synthetic relative names must
                // not create files in the working directory.
                if task.target_path.is_absolute() {
                    if let Some(parent) = task.target_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&task.target_path, b"thumbnail");
                }
                Some(task.target_path.to_string_lossy().into_owned())
            }),
        );
        assert!(scheduler.is_healthy());
        (scheduler, started_rx)
    }

    pub fn wait_for_pending(scheduler: &ThumbnailScheduler, expected: usize) {
        let deadline = Instant::now() + WAIT;
        loop {
            let count = scheduler.shared.state.lock().unwrap().jobs.len();
            if count == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "expected {expected} pending jobs, got {count}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc, Mutex,
        },
        time::Duration,
    };

    use super::{
        ThumbnailPriority, ThumbnailResultCallback, ThumbnailScheduler, ThumbnailTask,
        ThumbnailTaskResult,
    };
    use std::sync::mpsc::Sender;

    fn task(asset_id: i64, target: &str) -> ThumbnailTask {
        let source = format!("C:/tmp/source-{asset_id}.jpg");
        ThumbnailTask {
            operation: None,
            asset_id,
            source_path: source.clone().into(),
            target_path: target.into(),
            kind: "image".to_string(),
            duration_ms: None,
            source_version: crate::thumbs::SourceVersion::new(source, 1, 0),
            legacy_thumb: None,
        }
    }

    #[test]
    fn scheduler_deduplicates_inflight_jobs() {
        let starts = Arc::new(AtomicUsize::new(0));
        let release_once = Arc::new(Mutex::new(false));
        let starts_clone = Arc::clone(&starts);
        let release_once_clone = Arc::clone(&release_once);

        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                starts_clone.fetch_add(1, Ordering::SeqCst);
                loop {
                    if *release_once_clone.lock().expect("release lock") {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );

        let first = scheduler
            .enqueue(task(1, "C:/tmp/shared-thumb.jpg"), ThumbnailPriority::Low)
            .expect("first enqueue");
        let second = scheduler
            .enqueue(task(1, "C:/tmp/shared-thumb.jpg"), ThumbnailPriority::High)
            .expect("second enqueue");

        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        *release_once.lock().expect("release lock") = true;

        let first_result = first
            .recv_timeout(Duration::from_secs(2))
            .expect("first result timeout");
        let second_result = second
            .recv_timeout(Duration::from_secs(2))
            .expect("second result timeout");

        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(
            first_result.thumb_path,
            Some("C:/tmp/shared-thumb.jpg".to_string())
        );
        assert_eq!(
            second_result.thumb_path,
            Some("C:/tmp/shared-thumb.jpg".to_string())
        );
    }

    #[test]
    fn scheduler_enqueue_with_sender_emits_all_waiters() {
        let starts = Arc::new(AtomicUsize::new(0));
        let starts_clone = Arc::clone(&starts);

        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                starts_clone.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(15));
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );

        let (sender, receiver) = mpsc::channel();
        scheduler
            .enqueue_with_sender(
                task(101, "C:/tmp/shared-sender-thumb.jpg"),
                ThumbnailPriority::Low,
                sender.clone(),
            )
            .expect("enqueue with sender one");
        scheduler
            .enqueue_with_sender(
                task(202, "C:/tmp/shared-sender-thumb.jpg"),
                ThumbnailPriority::High,
                sender,
            )
            .expect("enqueue with sender two");

        let first = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("first sender result timeout");
        let second = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("second sender result timeout");

        let mut ids = vec![first.asset_id, second.asset_id];
        ids.sort_unstable();

        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(ids, vec![101, 202]);
        assert_eq!(
            first.thumb_path,
            Some("C:/tmp/shared-sender-thumb.jpg".to_string())
        );
        assert_eq!(
            second.thumb_path,
            Some("C:/tmp/shared-sender-thumb.jpg".to_string())
        );
    }

    #[test]
    fn scheduler_prioritizes_high_queue_after_current_job() {
        let release_first = Arc::new(Mutex::new(false));
        let order = Arc::new(Mutex::new(Vec::<i64>::new()));
        let first_started = Arc::new(AtomicUsize::new(0));
        let release_first_clone = Arc::clone(&release_first);
        let order_clone = Arc::clone(&order);
        let first_started_clone = Arc::clone(&first_started);

        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                if task.asset_id == 10 {
                    first_started_clone.store(1, Ordering::SeqCst);
                    loop {
                        if *release_first_clone.lock().expect("release first lock") {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                order_clone.lock().expect("order lock").push(task.asset_id);
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );

        let one = scheduler
            .enqueue(task(10, "C:/tmp/one.jpg"), ThumbnailPriority::Low)
            .expect("enqueue one");

        while first_started.load(Ordering::SeqCst) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }

        let two = scheduler
            .enqueue(task(20, "C:/tmp/two.jpg"), ThumbnailPriority::Low)
            .expect("enqueue two");
        let three = scheduler
            .enqueue(task(30, "C:/tmp/three.jpg"), ThumbnailPriority::High)
            .expect("enqueue three");

        *release_first.lock().expect("release first lock") = true;

        let _ = one
            .recv_timeout(Duration::from_secs(2))
            .expect("one result timeout");
        let _ = two
            .recv_timeout(Duration::from_secs(2))
            .expect("two result timeout");
        let _ = three
            .recv_timeout(Duration::from_secs(2))
            .expect("three result timeout");

        let processed = order.lock().expect("order lock").clone();
        assert_eq!(processed, vec![10, 30, 20]);
    }

    #[test]
    fn interactive_jobs_run_newest_first_and_ahead_of_high() {
        let release_first = Arc::new(Mutex::new(false));
        let order = Arc::new(Mutex::new(Vec::<i64>::new()));
        let first_started = Arc::new(AtomicUsize::new(0));
        let (release_clone, order_clone, started_clone) = (
            Arc::clone(&release_first),
            Arc::clone(&order),
            Arc::clone(&first_started),
        );
        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                if task.asset_id == 10 {
                    started_clone.store(1, Ordering::SeqCst);
                    while !*release_clone.lock().expect("release lock") {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                order_clone.lock().expect("order lock").push(task.asset_id);
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );
        let (done_tx, done_rx) = mpsc::channel::<ThumbnailTaskResult>();
        let callback = |tx: &Sender<ThumbnailTaskResult>| -> ThumbnailResultCallback {
            let tx = tx.clone();
            Box::new(move |result| {
                let _ = tx.send(result);
            })
        };
        scheduler
            .enqueue_with_callback(
                task(10, "C:/tmp/a.jpg"),
                ThumbnailPriority::Interactive,
                callback(&done_tx),
            )
            .expect("enqueue first");
        while first_started.load(Ordering::SeqCst) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        scheduler
            .enqueue_with_sender(
                task(20, "C:/tmp/b.jpg"),
                ThumbnailPriority::High,
                done_tx.clone(),
            )
            .expect("enqueue high");
        for (id, target) in [
            (30, "C:/tmp/c.jpg"),
            (40, "C:/tmp/d.jpg"),
            (50, "C:/tmp/e.jpg"),
        ] {
            scheduler
                .enqueue_with_callback(
                    task(id, target),
                    ThumbnailPriority::Interactive,
                    callback(&done_tx),
                )
                .expect("enqueue interactive");
        }
        // A repeated request moves its job back to the front.
        scheduler
            .enqueue_with_callback(
                task(30, "C:/tmp/c.jpg"),
                ThumbnailPriority::Interactive,
                callback(&done_tx),
            )
            .expect("repeat interactive");
        *release_first.lock().expect("release lock") = true;

        for _ in 0..6 {
            done_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("result");
        }
        assert_eq!(*order.lock().expect("order lock"), vec![10, 30, 50, 40, 20]);
    }

    #[test]
    fn interactive_request_promotes_queued_low_job() {
        let release_first = Arc::new(Mutex::new(false));
        let order = Arc::new(Mutex::new(Vec::<i64>::new()));
        let first_started = Arc::new(AtomicUsize::new(0));
        let (release_clone, order_clone, started_clone) = (
            Arc::clone(&release_first),
            Arc::clone(&order),
            Arc::clone(&first_started),
        );
        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                if task.asset_id == 10 {
                    started_clone.store(1, Ordering::SeqCst);
                    while !*release_clone.lock().expect("release lock") {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                order_clone.lock().expect("order lock").push(task.asset_id);
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );
        let (tx, rx) = mpsc::channel::<ThumbnailTaskResult>();
        scheduler
            .enqueue_with_sender(task(10, "C:/tmp/a.jpg"), ThumbnailPriority::Low, tx.clone())
            .expect("first");
        while first_started.load(Ordering::SeqCst) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        scheduler
            .enqueue_with_sender(
                task(20, "C:/tmp/b.jpg"),
                ThumbnailPriority::High,
                tx.clone(),
            )
            .expect("high");
        scheduler
            .enqueue_with_sender(task(30, "C:/tmp/c.jpg"), ThumbnailPriority::Low, tx.clone())
            .expect("low");
        scheduler
            .enqueue_with_sender(
                task(30, "C:/tmp/c.jpg"),
                ThumbnailPriority::Interactive,
                tx.clone(),
            )
            .expect("promote");
        *release_first.lock().expect("release lock") = true;
        for _ in 0..4 {
            rx.recv_timeout(Duration::from_secs(2)).expect("result");
        }
        assert_eq!(*order.lock().expect("order lock"), vec![10, 30, 20]);
    }

    /// Render order of a steady scroll that stops: every tile entered the
    /// overscan first, so all of them were queued at High (`p=1`) in scroll
    /// order. Tiles 9-16 are visible at the stop, tiles 17-20 are still in the
    /// overscan ahead and tiles 1-8 were scrolled past. With `reveal`, the
    /// gallery requests the visible tiles again without `p=1`, row by row as
    /// they came into view (ADR 0021).
    fn steady_scroll_render_order(reveal: bool) -> Vec<i64> {
        let order = Arc::new(Mutex::new(Vec::<i64>::new()));
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let order_clone = Arc::clone(&order);
        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                if task.asset_id == 0 {
                    started_tx.send(()).expect("start signal");
                    release_rx
                        .lock()
                        .expect("release lock")
                        .recv_timeout(Duration::from_secs(5))
                        .expect("release");
                }
                order_clone.lock().expect("order lock").push(task.asset_id);
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );
        let (tx, rx) = mpsc::channel::<ThumbnailTaskResult>();
        // The worker is busy, so the scroll builds a backlog.
        scheduler
            .enqueue_with_sender(
                task(0, "C:/tmp/busy.jpg"),
                ThumbnailPriority::High,
                tx.clone(),
            )
            .expect("busy job");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("busy job started");
        let target = |id: i64| format!("C:/tmp/scroll-{id}.jpg");
        for id in 1..=20 {
            scheduler
                .enqueue_with_sender(task(id, &target(id)), ThumbnailPriority::High, tx.clone())
                .expect("prefetch");
        }
        let mut waiters = 21;
        if reveal {
            for id in 9..=16 {
                scheduler
                    .enqueue_with_sender(
                        task(id, &target(id)),
                        ThumbnailPriority::Interactive,
                        tx.clone(),
                    )
                    .expect("visible request");
                waiters += 1;
            }
        }
        release_tx.send(()).expect("release busy job");
        for _ in 0..waiters {
            rx.recv_timeout(Duration::from_secs(5)).expect("result");
        }
        let processed = order.lock().expect("order lock").clone();
        processed
    }

    #[test]
    fn visible_requests_for_queued_prefetches_render_before_the_scrolled_past_backlog() {
        // Without a new request the visible tiles wait behind every
        // scrolled-past prefetch (High is FIFO and nothing promotes them).
        let sticky: Vec<i64> = (0..=20).collect();
        assert_eq!(steady_scroll_render_order(false), sticky);
        // A visible request moves each queued job to the front of Interactive:
        // the visible tiles render first, the row that came into view last
        // first, then the remaining prefetches in their FIFO order.
        let mut revealed = vec![0];
        revealed.extend((9..=16).rev());
        revealed.extend(1..=8);
        revealed.extend(17..=20);
        assert_eq!(steady_scroll_render_order(true), revealed);
    }

    #[test]
    fn scheduler_processes_jobs_in_parallel() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let active_clone = Arc::clone(&active);
        let max_seen_clone = Arc::clone(&max_seen);

        let scheduler = ThumbnailScheduler::with_processor(
            4,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                let current = active_clone.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen_clone.fetch_max(current, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(40));
                active_clone.fetch_sub(1, Ordering::SeqCst);
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );

        let mut receivers = Vec::new();
        for idx in 0..8 {
            let receiver = scheduler
                .enqueue(
                    task(100 + idx, &format!("C:/tmp/parallel-{idx}.jpg")),
                    ThumbnailPriority::Low,
                )
                .expect("enqueue parallel task");
            receivers.push(receiver);
        }

        for receiver in receivers {
            let _ = receiver
                .recv_timeout(Duration::from_secs(3))
                .expect("parallel task timeout");
        }

        assert!(
            max_seen.load(Ordering::SeqCst) >= 2,
            "expected at least 2 concurrent jobs"
        );
    }

    #[test]
    fn scheduler_delivers_failed_result_when_processor_panics() {
        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                if task.asset_id == 7 {
                    panic!("decoder exploded");
                }
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );

        let panicked = scheduler
            .enqueue(task(7, "C:/tmp/panic.jpg"), ThumbnailPriority::High)
            .expect("panic enqueue");
        let healthy = scheduler
            .enqueue(task(8, "C:/tmp/healthy.jpg"), ThumbnailPriority::High)
            .expect("healthy enqueue");

        let panic_result = panicked
            .recv_timeout(Duration::from_secs(2))
            .expect("panicking job must still deliver a terminal result");
        assert_eq!(panic_result.asset_id, 7);
        assert_eq!(panic_result.thumb_path, None);

        let ok_result = healthy
            .recv_timeout(Duration::from_secs(2))
            .expect("healthy result timeout");
        assert!(ok_result.thumb_path.is_some());
    }

    #[test]
    fn cpu_capacity_preserves_browsing_and_scales_bulk_window() {
        for (cpus, browsing, bulk, window) in [
            (None, 4, 4, 24),
            (Some(1), 2, 1, 24),
            (Some(2), 2, 2, 24),
            (Some(8), 6, 8, 24),
            (Some(16), 8, 16, 48),
            (Some(32), 8, 32, 96),
            (Some(256), 8, 256, 768),
            (Some(1024), 8, 1024, 2048),
            (Some(usize::MAX), 8, usize::MAX, 2048),
        ] {
            let limits = super::ThumbnailConcurrency::from_parallelism(
                cpus.and_then(std::num::NonZeroUsize::new),
            );
            assert_eq!(
                (limits.browsing, limits.bulk, limits.bulk_in_flight_limit()),
                (browsing, bulk, window)
            );
        }
    }

    #[test]
    fn bulk_runs_one_job_per_cpu_including_videos() {
        use super::test_support::{controlled_scheduler, WAIT};
        for cpus in [1, 2, 8, 16, 32] {
            let (scheduler, started) = controlled_scheduler(cpus);
            for kind in ["image", "video"] {
                let bulk = scheduler.enter_bulk_mode().unwrap();
                let mut results = Vec::new();
                for id in 0..cpus + 1 {
                    let mut job = task(id as i64, &format!("bulk-{kind}-{id}.jpg"));
                    job.kind = kind.into();
                    results.push(scheduler.enqueue(job, ThumbnailPriority::Low).unwrap());
                }
                let running: Vec<_> = (0..cpus)
                    .map(|_| started.recv_timeout(WAIT).unwrap())
                    .collect();
                assert!(started.recv_timeout(Duration::from_millis(50)).is_err());
                for job in running {
                    job.release.send(()).unwrap();
                }
                started
                    .recv_timeout(WAIT)
                    .unwrap()
                    .release
                    .send(())
                    .unwrap();
                for result in results {
                    assert!(result.recv_timeout(WAIT).unwrap().thumb_path.is_some());
                }
                drop(bulk);
            }
        }
    }

    #[test]
    fn default_processor_never_moves_or_serves_a_legacy_file_outside_the_thumbnail_dir() {
        let dir = tempfile::tempdir().unwrap();
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        let outside = dir.path().join("outside.jpg");
        std::fs::write(&outside, b"user file").unwrap();
        let target = thumbs.join("target.jpg");
        let mut job = task(1, &target.to_string_lossy());
        job.kind = "video".into();
        job.legacy_thumb = Some(outside.clone());
        let ffmpeg = std::path::Path::new("/nonexistent/ffmpeg");

        // Unreachable source: the outside file is not served.
        assert_eq!(super::process_thumbnail_task(ffmpeg, &job), None);
        // Reachable source: the outside file is not adopted or removed.
        let source = dir.path().join("clip.mp4");
        std::fs::write(&source, b"not decoded").unwrap();
        job.source_path = source;
        assert_eq!(super::process_thumbnail_task(ffmpeg, &job), None);
        assert_eq!(std::fs::read(&outside).unwrap(), b"user file");
        assert!(!target.exists());

        // The same file inside the thumbnail directory is adopted.
        let inside = thumbs.join("legacy.jpg");
        std::fs::rename(&outside, &inside).unwrap();
        job.legacy_thumb = Some(inside.clone());
        assert_eq!(
            super::process_thumbnail_task(ffmpeg, &job),
            Some(target.to_string_lossy().into_owned())
        );
        assert!(!inside.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"user file");
    }

    #[test]
    fn a_video_job_that_adopts_a_legacy_thumbnail_does_not_wait_for_a_video_slot() {
        use super::test_support::{controlled_scheduler, WAIT};
        let (scheduler, started) = controlled_scheduler(16);
        let mut results = Vec::new();
        for id in 0..3 {
            let mut job = task(id, &format!("render-video-{id}.jpg"));
            job.kind = "video".into();
            results.push(
                scheduler
                    .enqueue(job, ThumbnailPriority::Interactive)
                    .unwrap(),
            );
        }
        let running: Vec<_> = (0..2)
            .map(|_| started.recv_timeout(WAIT).unwrap())
            .collect();
        // Both browsing video slots are taken by renders; a third render waits.
        assert!(started.recv_timeout(Duration::from_millis(50)).is_err());

        let mut adopt = task(10, "adopt-video.jpg");
        adopt.kind = "video".into();
        adopt.legacy_thumb = Some("legacy-video.jpg".into());
        results.push(
            scheduler
                .enqueue(adopt, ThumbnailPriority::Interactive)
                .unwrap(),
        );
        let adopting = started.recv_timeout(WAIT).unwrap();
        assert_eq!(adopting.asset_id, 10);
        adopting.release.send(()).unwrap();

        // The third render still waits for a video slot.
        assert!(started.recv_timeout(Duration::from_millis(50)).is_err());
        for job in running {
            job.release.send(()).unwrap();
        }
        started
            .recv_timeout(WAIT)
            .unwrap()
            .release
            .send(())
            .unwrap();
        for result in results {
            assert!(result.recv_timeout(WAIT).unwrap().thumb_path.is_some());
        }
    }

    #[test]
    fn browsing_limits_return_while_bulk_jobs_are_still_running() {
        use super::test_support::{controlled_scheduler, WAIT};
        let (scheduler, started) = controlled_scheduler(16);
        for (kind, browsing_limit) in [("image", 8), ("video", 2)] {
            let mut results = Vec::new();
            for id in 0..20 {
                let mut job = task(id, &format!("transition-{kind}-{id}.jpg"));
                job.kind = kind.into();
                results.push(scheduler.enqueue(job, ThumbnailPriority::Low).unwrap());
            }
            let mut running: Vec<_> = (0..browsing_limit)
                .map(|_| started.recv_timeout(WAIT).unwrap())
                .collect();
            assert!(started.recv_timeout(Duration::from_millis(50)).is_err());
            let bulk = scheduler.enter_bulk_mode().unwrap();
            running.extend((browsing_limit..16).map(|_| started.recv_timeout(WAIT).unwrap()));
            assert!(started.recv_timeout(Duration::from_millis(50)).is_err());
            // Unwinding with active decoders must restore admission too.
            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let _bulk = bulk;
                panic!("injected bulk coordinator unwind");
            }));
            assert!(unwind.is_err());
            // Finishing down to the browsing cap must not admit any queued job.
            for job in running.drain(..16 - browsing_limit) {
                job.release.send(()).unwrap();
            }
            assert!(started.recv_timeout(Duration::from_millis(50)).is_err());
            running.pop().unwrap().release.send(()).unwrap();
            let next = started.recv_timeout(WAIT).unwrap();
            assert!(started.recv_timeout(Duration::from_millis(50)).is_err());
            next.release.send(()).unwrap();
            for job in running {
                job.release.send(()).unwrap();
            }
            for _ in 0..3 {
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
    }

    #[test]
    fn bulk_keeps_priority_promotion_deduplication_and_queue_bound() {
        use super::test_support::{controlled_scheduler, WAIT};
        let (scheduler, started) = controlled_scheduler(1);
        let _bulk = scheduler.enter_bulk_mode().unwrap();
        let (tx, rx) = mpsc::channel();
        scheduler
            .enqueue_with_sender(task(0, "queue-0.jpg"), ThumbnailPriority::Low, tx.clone())
            .unwrap();
        let first = started.recv_timeout(WAIT).unwrap();
        for id in 1..super::MAX_PENDING_JOBS {
            scheduler
                .enqueue_with_sender(
                    task(id as i64, &format!("queue-{id}.jpg")),
                    ThumbnailPriority::Low,
                    tx.clone(),
                )
                .unwrap();
        }
        assert!(scheduler
            .enqueue(task(9999, "overflow.jpg"), ThumbnailPriority::High)
            .unwrap_err()
            .contains("full"));
        // Attaching a visible waiter still works at capacity and promotes the job.
        scheduler
            .enqueue_with_sender(task(9999, "queue-50.jpg"), ThumbnailPriority::High, tx)
            .unwrap();
        first.release.send(()).unwrap();
        let promoted = started.recv_timeout(WAIT).unwrap();
        assert_eq!(promoted.asset_id, 50);
        promoted.release.send(()).unwrap();
        for _ in 2..super::MAX_PENDING_JOBS {
            started
                .recv_timeout(WAIT)
                .unwrap()
                .release
                .send(())
                .unwrap();
        }
        let mut ids = Vec::new();
        for _ in 0..=super::MAX_PENDING_JOBS {
            ids.push(rx.recv_timeout(WAIT).unwrap().asset_id);
        }
        assert_eq!(ids.iter().filter(|&&id| id == 50).count(), 1);
        assert!(ids.contains(&9999));
        assert!(started.try_recv().is_err());
    }

    #[test]
    fn scheduler_result_carries_expected_source_version() {
        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(|_, task| Some(task.target_path.to_string_lossy().to_string())),
        );

        let receiver = scheduler
            .enqueue(task(9, "C:/tmp/versioned.jpg"), ThumbnailPriority::High)
            .expect("enqueue");

        let result = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("result timeout");
        assert_eq!(
            result.source_version,
            crate::thumbs::SourceVersion::new("C:/tmp/source-9.jpg", 1, 0)
        );
    }

    #[test]
    fn scheduler_rejects_enqueue_when_queue_is_full() {
        let release = Arc::new(Mutex::new(false));
        let release_clone = Arc::clone(&release);
        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                loop {
                    if *release_clone.lock().expect("release lock") {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );

        // First job occupies the only worker; everything after fills the queue.
        let _running = scheduler
            .enqueue(task(1, "C:/tmp/queue-running.jpg"), ThumbnailPriority::Low)
            .expect("first enqueue");

        let mut receivers = Vec::new();
        for idx in 0..super::MAX_PENDING_JOBS - 1 {
            receivers.push(
                scheduler
                    .enqueue(
                        task(idx as i64 + 2, &format!("C:/tmp/queue-{idx}.jpg")),
                        ThumbnailPriority::Low,
                    )
                    .expect("queued enqueue"),
            );
        }

        let overflow = scheduler.enqueue(
            task(99_999, "C:/tmp/queue-overflow.jpg"),
            ThumbnailPriority::Low,
        );
        assert!(
            overflow.is_err(),
            "enqueue past the queue bound must fail fast"
        );
        assert!(overflow.unwrap_err().contains("full"));

        *release.lock().expect("release lock") = true;
    }

    #[test]
    fn scheduler_rejects_waiter_when_job_waiter_limit_is_full() {
        let release = Arc::new(Mutex::new(false));
        let release_clone = Arc::clone(&release);
        let scheduler = ThumbnailScheduler::with_processor(
            1,
            "ffmpeg".into(),
            Arc::new(move |_, task| {
                while !*release_clone.lock().expect("release lock") {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Some(task.target_path.to_string_lossy().to_string())
            }),
        );

        let mut receivers = Vec::new();
        for asset_id in 0..super::MAX_WAITERS_PER_JOB {
            receivers.push(
                scheduler
                    .enqueue(
                        task(asset_id as i64 + 1, "C:/tmp/waiter-limit.jpg"),
                        ThumbnailPriority::Low,
                    )
                    .expect("waiter within limit"),
            );
        }

        let overflow = scheduler.enqueue(
            task(99_999, "C:/tmp/waiter-limit.jpg"),
            ThumbnailPriority::High,
        );
        assert!(overflow.unwrap_err().contains("waiter limit"));

        *release.lock().expect("release lock") = true;
    }
}
