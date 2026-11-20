//! The production executor: an elastic pool of background threads and one timer
//! thread.
//!
//! Backend calls block: a scan holds the scan lock for its whole run and a bulk
//! render waits for the scheduler. A pool of fixed size would let a few long
//! operations starve the gallery's queries, so the pool grows while every worker
//! is busy (like the blocking pool the Tauri commands run on) and idle workers end
//! after [`KEEP_ALIVE`]. The growth stops at [`MAX_THREADS`], well below the 512
//! threads of that blocking pool: once that many tasks block, everything else
//! waits in the queue, timer firings included. The pool therefore protects only
//! against a handful of long operations. A source of work that can start any
//! number of blocking tasks bounds them itself, as the thumbnail fetcher does
//! ([`crate::thumbs::MAX_CONCURRENT_FETCHES`]): its requests can hang on an
//! unreachable mount, one per tile. Nothing here knows the core: workflow locks
//! are taken and released inside one task, by the use case it calls.

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use super::{Executor, Task};

/// Upper bound of worker threads; further tasks, and timers that fire, wait in
/// the queue.
pub const MAX_THREADS: usize = 64;
/// How long an idle worker waits for another task before it ends.
pub const KEEP_ALIVE: Duration = Duration::from_secs(10);

pub struct ThreadExecutor {
    pool: Arc<Pool>,
    timers: Arc<Timers>,
}

struct Pool {
    state: Mutex<PoolState>,
    /// Signals a queued task or the shutdown to idle workers.
    work: Condvar,
    /// Signals that no task is queued or running.
    quiet: Condvar,
    max_threads: usize,
    keep_alive: Duration,
}

#[derive(Default)]
struct PoolState {
    queue: VecDeque<Task>,
    threads: usize,
    idle: usize,
    busy: usize,
    shutdown: bool,
}

struct Timers {
    state: Mutex<TimerState>,
    changed: Condvar,
}

#[derive(Default)]
struct TimerState {
    heap: BinaryHeap<Timer>,
    next_sequence: u64,
    thread_started: bool,
    shutdown: bool,
}

struct Timer {
    due: Instant,
    sequence: u64,
    task: Task,
}

impl PartialEq for Timer {
    fn eq(&self, other: &Self) -> bool {
        self.due == other.due && self.sequence == other.sequence
    }
}

impl Eq for Timer {}

impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timer {
    /// Reversed, so the heap's greatest element is the timer due first; timers
    /// due at the same instant fire in the order they were set.
    fn cmp(&self, other: &Self) -> Ordering {
        (other.due, other.sequence).cmp(&(self.due, self.sequence))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Default for ThreadExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadExecutor {
    pub fn new() -> Self {
        Self::with_limits(MAX_THREADS, KEEP_ALIVE)
    }

    /// A pool of at most `max_threads` workers (at least one) whose idle workers
    /// end after `keep_alive`.
    pub fn with_limits(max_threads: usize, keep_alive: Duration) -> Self {
        Self {
            pool: Arc::new(Pool {
                state: Mutex::new(PoolState::default()),
                work: Condvar::new(),
                quiet: Condvar::new(),
                max_threads: max_threads.max(1),
                keep_alive,
            }),
            timers: Arc::new(Timers {
                state: Mutex::new(TimerState::default()),
                changed: Condvar::new(),
            }),
        }
    }

    /// Stops accepting work. Queued tasks and pending timers are dropped without
    /// running, tasks already running finish, and the threads end on their own;
    /// nothing is joined. Also runs when the executor is dropped.
    pub fn shutdown(&self) {
        let pending_timers = {
            let mut timers = lock(&self.timers.state);
            timers.shutdown = true;
            std::mem::take(&mut timers.heap)
        };
        self.timers.changed.notify_all();
        let queued = {
            let mut state = lock(&self.pool.state);
            state.shutdown = true;
            std::mem::take(&mut state.queue)
        };
        self.pool.work.notify_all();
        self.pool.quiet.notify_all();
        // Dropped outside the locks: a dropped task may answer its caller.
        drop(pending_timers);
        drop(queued);
    }

    /// Waits until no task is queued or running, for at most `timeout`. Pending
    /// timers do not count. Returns whether the pool became quiet.
    pub fn wait_until_quiet(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = lock(&self.pool.state);
        while state.busy > 0 || !state.queue.is_empty() {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            state = self
                .pool
                .quiet
                .wait_timeout(state, left)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
        true
    }

    /// Worker threads currently alive (busy or idle).
    pub fn threads(&self) -> usize {
        lock(&self.pool.state).threads
    }
}

impl Drop for ThreadExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Executor for ThreadExecutor {
    fn spawn(&self, task: Task) {
        self.pool.spawn(task);
    }

    fn spawn_after(&self, delay: Duration, task: Task) {
        let mut state = lock(&self.timers.state);
        if state.shutdown {
            drop(state);
            drop(task);
            return;
        }
        if !state.thread_started {
            let timers = Arc::clone(&self.timers);
            let pool = Arc::clone(&self.pool);
            let started = thread::Builder::new()
                .name("tagrove-timers".into())
                .spawn(move || timers.run(&pool));
            if started.is_err() {
                // Without a timer thread the timer can never fire.
                drop(state);
                drop(task);
                return;
            }
            state.thread_started = true;
        }
        let sequence = state.next_sequence;
        state.next_sequence += 1;
        state.heap.push(Timer {
            due: Instant::now() + delay,
            sequence,
            task,
        });
        drop(state);
        self.timers.changed.notify_all();
    }
}

impl Pool {
    fn spawn(self: &Arc<Self>, task: Task) {
        let mut state = lock(&self.state);
        if state.shutdown {
            drop(state);
            drop(task);
            return;
        }
        state.queue.push_back(task);
        // Every idle worker takes one queued task; a task beyond that needs a
        // new worker, or it would wait behind a long-running one.
        if state.queue.len() > state.idle && state.threads < self.max_threads {
            let pool = Arc::clone(self);
            let started = thread::Builder::new()
                .name("tagrove-backend".into())
                .spawn(move || pool.work());
            match started {
                Ok(_) => state.threads += 1,
                Err(_) if state.threads == 0 => {
                    // No worker exists to ever run the queue.
                    let orphaned = std::mem::take(&mut state.queue);
                    drop(state);
                    drop(orphaned);
                    return;
                }
                Err(_) => {}
            }
        }
        drop(state);
        self.work.notify_one();
    }

    fn work(self: Arc<Self>) {
        let mut state = lock(&self.state);
        loop {
            if let Some(task) = state.queue.pop_front() {
                state.busy += 1;
                drop(state);
                // A panicking task must not take the worker with it; whoever
                // waits for the task's reply is answered by its drop guard.
                let _ = catch_unwind(AssertUnwindSafe(task));
                state = lock(&self.state);
                state.busy -= 1;
                if state.busy == 0 && state.queue.is_empty() {
                    self.quiet.notify_all();
                }
                continue;
            }
            if state.shutdown {
                break;
            }
            state.idle += 1;
            let (guard, wait) = self
                .work
                .wait_timeout(state, self.keep_alive)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = guard;
            state.idle -= 1;
            if wait.timed_out() && state.queue.is_empty() {
                break;
            }
        }
        state.threads -= 1;
    }
}

impl Timers {
    /// Hands each timer to the pool when it is due, so a slow timer task never
    /// delays the timers after it.
    fn run(&self, pool: &Arc<Pool>) {
        let mut state = lock(&self.state);
        loop {
            if state.shutdown {
                let pending = std::mem::take(&mut state.heap);
                drop(state);
                drop(pending);
                return;
            }
            let now = Instant::now();
            match state.heap.peek().map(|timer| timer.due) {
                Some(due) if due <= now => {
                    let timer = state.heap.pop().expect("peeked timer");
                    drop(state);
                    pool.spawn(timer.task);
                    state = lock(&self.state);
                }
                Some(due) => {
                    state = self
                        .changed
                        .wait_timeout(state, due - now)
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .0;
                }
                None => {
                    state = self
                        .changed
                        .wait(state)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc,
        },
        time::Duration,
    };

    use super::ThreadExecutor;
    use crate::exec::Executor;

    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn tasks_run_off_the_calling_thread() {
        let executor = ThreadExecutor::new();
        let (sender, receiver) = mpsc::channel();
        let caller = std::thread::current().id();

        executor.spawn(Box::new(move || {
            sender.send(std::thread::current().id()).unwrap();
        }));

        assert_ne!(receiver.recv_timeout(WAIT).unwrap(), caller);
    }

    #[test]
    fn a_long_task_does_not_hold_back_the_tasks_behind_it() {
        let executor = ThreadExecutor::new();
        let (release, held) = mpsc::channel::<()>();
        let (entered_tx, entered) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();

        executor.spawn(Box::new(move || {
            entered_tx.send(()).unwrap();
            held.recv().unwrap();
        }));
        entered.recv_timeout(WAIT).unwrap();
        for index in 0..4 {
            let done_tx = done_tx.clone();
            executor.spawn(Box::new(move || done_tx.send(index).unwrap()));
        }

        let mut finished: Vec<i32> = (0..4).map(|_| done.recv_timeout(WAIT).unwrap()).collect();
        finished.sort_unstable();
        assert_eq!(finished, [0, 1, 2, 3]);
        release.send(()).unwrap();
        assert!(executor.wait_until_quiet(WAIT));
    }

    #[test]
    fn the_pool_never_grows_past_its_limit_and_still_runs_everything() {
        let executor = ThreadExecutor::with_limits(2, Duration::from_secs(5));
        let running = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let finished = Arc::new(AtomicUsize::new(0));

        for _ in 0..16 {
            let (running, most, finished) = (running.clone(), most.clone(), finished.clone());
            executor.spawn(Box::new(move || {
                most.fetch_max(running.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                running.fetch_sub(1, Ordering::SeqCst);
                finished.fetch_add(1, Ordering::SeqCst);
            }));
        }

        assert!(executor.wait_until_quiet(WAIT));
        assert_eq!(finished.load(Ordering::SeqCst), 16);
        assert!(most.load(Ordering::SeqCst) <= 2);
        assert!(executor.threads() <= 2);
    }

    #[test]
    fn idle_workers_end_after_the_keep_alive() {
        let executor = ThreadExecutor::with_limits(4, Duration::from_millis(20));
        executor.spawn(Box::new(|| {}));
        assert!(executor.wait_until_quiet(WAIT));

        let deadline = std::time::Instant::now() + WAIT;
        while executor.threads() > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(executor.threads(), 0);

        // The pool starts a worker again when work arrives.
        let (sender, receiver) = mpsc::channel();
        executor.spawn(Box::new(move || sender.send(()).unwrap()));
        receiver.recv_timeout(WAIT).unwrap();
    }

    #[test]
    fn a_panicking_task_leaves_the_pool_usable() {
        let executor = ThreadExecutor::with_limits(1, Duration::from_secs(5));
        let (sender, receiver) = mpsc::channel();

        executor.spawn(Box::new(|| panic!("synthetic task panic")));
        executor.spawn(Box::new(move || sender.send(7).unwrap()));

        assert_eq!(receiver.recv_timeout(WAIT).unwrap(), 7);
    }

    #[test]
    fn timers_fire_in_due_order_and_not_early() {
        let executor = ThreadExecutor::new();
        let (sender, receiver) = mpsc::channel();
        let started = std::time::Instant::now();

        for (label, delay) in [("late", 60_u64), ("early", 20), ("same", 20)] {
            let sender = sender.clone();
            executor.spawn_after(
                Duration::from_millis(delay),
                Box::new(move || sender.send((label, started.elapsed())).unwrap()),
            );
        }

        let fired: Vec<_> = (0..3)
            .map(|_| receiver.recv_timeout(WAIT).unwrap())
            .collect();
        assert!(fired.iter().all(|(_, at)| *at >= Duration::from_millis(20)));
        assert_eq!(fired[2].0, "late");
        assert!(fired[2].1 >= Duration::from_millis(60));
    }

    #[test]
    fn shutdown_drops_queued_tasks_and_pending_timers_without_running_them() {
        struct Dropped(mpsc::Sender<&'static str>, &'static str);
        impl Drop for Dropped {
            fn drop(&mut self) {
                let _ = self.0.send(self.1);
            }
        }

        let executor = ThreadExecutor::with_limits(1, Duration::from_secs(5));
        let (release, held) = mpsc::channel::<()>();
        let (entered_tx, entered) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        let ran = Arc::new(AtomicUsize::new(0));

        executor.spawn(Box::new(move || {
            entered_tx.send(()).unwrap();
            let _ = held.recv();
        }));
        entered.recv_timeout(WAIT).unwrap();
        let queued = Dropped(events_tx.clone(), "queued");
        let counter = ran.clone();
        executor.spawn(Box::new(move || {
            let _keep = &queued;
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        let timer = Dropped(events_tx, "timer");
        let counter = ran.clone();
        executor.spawn_after(
            Duration::from_secs(3600),
            Box::new(move || {
                let _keep = &timer;
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );

        executor.shutdown();

        let mut dropped = vec![
            events.recv_timeout(WAIT).unwrap(),
            events.recv_timeout(WAIT).unwrap(),
        ];
        dropped.sort_unstable();
        assert_eq!(dropped, ["queued", "timer"]);
        release.send(()).unwrap();
        assert!(executor.wait_until_quiet(WAIT));
        assert_eq!(ran.load(Ordering::SeqCst), 0);

        // Work offered after the shutdown is dropped as well.
        let (late_tx, late) = mpsc::channel();
        executor.spawn(Box::new(move || late_tx.send(()).unwrap()));
        assert!(late.recv_timeout(Duration::from_millis(50)).is_err());
    }
}
