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
}

struct SchedulerState {
    concurrency: ThumbnailConcurrency,
    bulk_scopes: usize,
    active_jobs: usize,
    jobs: HashMap<String, ScheduledJob>,
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
    sender: Sender<ThumbnailTaskResult>,
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

        self.enqueue_waiter(key, task, priority, JobWaiter { asset_id, sender })?;

        Ok(receiver)
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

        self.enqueue_waiter(key, task, priority, JobWaiter { asset_id, sender })
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
            let should_promote = matches!(priority, ThumbnailPriority::High)
                && state
                    .jobs
                    .get(&key)
                    .map(|job| matches!(job.priority, ThumbnailPriority::Low))
                    .unwrap_or(false);

            if should_promote {
                if let Some(idx) = state
                    .low_queue
                    .iter()
                    .position(|queued_key| queued_key == &key)
                {
                    let _ = state.low_queue.remove(idx);
                    state.high_queue.push_back(key.clone());
                }
            }

            let existing = state
                .jobs
                .get_mut(&key)
                .expect("existing thumbnail job to be present");
            if should_promote {
                existing.priority = ThumbnailPriority::High;
            }
            existing.waiters.push(waiter);
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

        match priority {
            ThumbnailPriority::High => state.high_queue.push_back(key),
            ThumbnailPriority::Low => state.low_queue.push_back(key),
        }

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
                high_queue: VecDeque::new(),
                low_queue: VecDeque::new(),
                active_videos: 0,
            }),
            condvar: Condvar::new(),
            ffmpeg_path,
            processor,
            worker_spawn_failures: AtomicUsize::new(0),
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
                    if task.kind == "video" {
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
            if task.kind == "video" {
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
            let _ = waiter.sender.send(ThumbnailTaskResult {
                asset_id: waiter.asset_id,
                thumb_path: thumb_path.clone(),
                source_version: task.source_version.clone(),
            });
        }
    }
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
    for high_priority in [true, false] {
        let position = {
            let queue = if high_priority {
                &state.high_queue
            } else {
                &state.low_queue
            };
            queue.iter().position(|key| {
                state
                    .jobs
                    .get(key)
                    .map(|job| job.task.kind != "video" || state.active_videos < video_limit)
                    .unwrap_or(true)
            })
        };
        if let Some(position) = position {
            return if high_priority {
                state.high_queue.remove(position)
            } else {
                state.low_queue.remove(position)
            };
        }
    }
    None
}

fn process_thumbnail_task(ffmpeg_path: &Path, task: &ThumbnailTask) -> Option<String> {
    if !task.source_path.exists() {
        return None;
    }

    if !task.target_path.exists() {
        if task.kind == "video" {
            let seek_seconds = thumbs::resolve_video_seek_seconds(task.duration_ms);
            let primary_result = thumbs::create_video_thumb(
                ffmpeg_path,
                &task.source_path,
                &task.target_path,
                seek_seconds,
            );

            if (primary_result.is_err() || !task.target_path.exists()) && seek_seconds > 0.0 {
                let _ = thumbs::create_video_thumb(
                    ffmpeg_path,
                    &task.source_path,
                    &task.target_path,
                    0.0,
                );
            }
        } else {
            let _ = thumbs::create_image_thumb(&task.source_path, &task.target_path);
        }
    }

    if !task.target_path.exists() {
        return None;
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

    use super::{ThumbnailPriority, ThumbnailScheduler, ThumbnailTask};

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
