//! The deterministic executor for tests: nothing runs until the test runs it.
//!
//! Spawned tasks wait in a queue and run on the test's own thread, one at a time,
//! in order or out of order ([`ManualExecutor::run_nth`]), which is how a test
//! makes a late reply overtake an earlier one. Timers wait on a virtual clock that
//! only [`ManualExecutor::advance`] moves.

use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use super::{Executor, Task};

#[derive(Default)]
pub struct ManualExecutor {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    now: Duration,
    next_sequence: u64,
    ready: VecDeque<Task>,
    timers: Vec<Timer>,
}

struct Timer {
    due: Duration,
    sequence: u64,
    task: Task,
}

impl ManualExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Tasks waiting to run.
    pub fn pending(&self) -> usize {
        self.state().ready.len()
    }

    /// Timers that have not fired yet.
    pub fn pending_timers(&self) -> usize {
        self.state().timers.len()
    }

    /// Time on the virtual clock since the executor was created.
    pub fn now(&self) -> Duration {
        self.state().now
    }

    /// Delay until the next timer fires, if one is pending.
    pub fn next_timer_in(&self) -> Option<Duration> {
        let state = self.state();
        state
            .timers
            .iter()
            .map(|timer| timer.due.saturating_sub(state.now))
            .min()
    }

    /// Runs the oldest waiting task. Returns whether there was one.
    pub fn run_next(&self) -> bool {
        self.run_nth(0)
    }

    /// Runs the waiting task at `index` (0 is the oldest), ahead of the tasks
    /// before it. Returns whether there was one.
    pub fn run_nth(&self, index: usize) -> bool {
        let task = self.state().ready.remove(index);
        match task {
            Some(task) => {
                // Outside the lock: the task may spawn more work.
                task();
                true
            }
            None => false,
        }
    }

    /// Runs waiting tasks, oldest first, until none is left, including tasks
    /// spawned meanwhile. Returns how many ran.
    pub fn run_all(&self) -> usize {
        let mut ran = 0;
        while self.run_next() {
            ran += 1;
        }
        ran
    }

    /// Drops the waiting task at `index` without running it, as an executor that
    /// shuts down does. Returns whether there was one.
    pub fn drop_nth(&self, index: usize) -> bool {
        let task = self.state().ready.remove(index);
        task.is_some()
    }

    /// Moves the virtual clock forward and queues the timers that became due, in
    /// the order they are due (set order for equal times). Returns how many
    /// fired; they run with the next `run_*` call.
    pub fn advance(&self, by: Duration) -> usize {
        let mut state = self.state();
        state.now += by;
        let now = state.now;
        let (mut due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut state.timers)
            .into_iter()
            .partition(|timer| timer.due <= now);
        state.timers = waiting;
        due.sort_by_key(|timer| (timer.due, timer.sequence));
        let fired = due.len();
        state.ready.extend(due.into_iter().map(|timer| timer.task));
        fired
    }

    /// [`advance`](Self::advance), then [`run_all`](Self::run_all). Returns how
    /// many tasks ran.
    pub fn advance_and_run(&self, by: Duration) -> usize {
        self.advance(by);
        self.run_all()
    }
}

impl Executor for ManualExecutor {
    fn spawn(&self, task: Task) {
        self.state().ready.push_back(task);
    }

    fn spawn_after(&self, delay: Duration, task: Task) {
        let mut state = self.state();
        let sequence = state.next_sequence;
        state.next_sequence += 1;
        let due = state.now + delay;
        state.timers.push(Timer {
            due,
            sequence,
            task,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use super::ManualExecutor;
    use crate::exec::Executor;

    fn recorder() -> (
        Arc<Mutex<Vec<&'static str>>>,
        impl Fn(&'static str) -> super::Task,
    ) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&log);
        (log, move |label| {
            let sink = Arc::clone(&sink);
            Box::new(move || sink.lock().unwrap().push(label))
        })
    }

    #[test]
    fn nothing_runs_until_the_test_runs_it() {
        let executor = ManualExecutor::new();
        let (log, task) = recorder();

        executor.spawn(task("a"));
        executor.spawn(task("b"));
        assert_eq!(executor.pending(), 2);
        assert!(log.lock().unwrap().is_empty());

        assert!(executor.run_next());
        assert_eq!(*log.lock().unwrap(), ["a"]);
        assert_eq!(executor.run_all(), 1);
        assert_eq!(*log.lock().unwrap(), ["a", "b"]);
        assert!(!executor.run_next());
    }

    #[test]
    fn a_later_task_can_run_first() {
        let executor = ManualExecutor::new();
        let (log, task) = recorder();
        executor.spawn(task("first"));
        executor.spawn(task("second"));
        executor.spawn(task("third"));

        assert!(executor.run_nth(2));
        assert!(executor.run_nth(1));
        assert!(!executor.run_nth(1));
        assert!(executor.run_nth(0));

        assert_eq!(*log.lock().unwrap(), ["third", "second", "first"]);
    }

    #[test]
    fn run_all_includes_tasks_spawned_by_tasks() {
        let executor = Arc::new(ManualExecutor::new());
        let (log, task) = recorder();
        let inner = Arc::clone(&executor);
        let follow_up = task("follow-up");
        executor.spawn(Box::new(move || inner.spawn(follow_up)));

        assert_eq!(executor.run_all(), 2);
        assert_eq!(*log.lock().unwrap(), ["follow-up"]);
    }

    #[test]
    fn timers_wait_for_the_virtual_clock_and_fire_in_due_order() {
        let executor = ManualExecutor::new();
        let (log, task) = recorder();
        executor.spawn_after(Duration::from_millis(2000), task("slow"));
        executor.spawn_after(Duration::from_millis(500), task("quick"));
        executor.spawn_after(Duration::from_millis(500), task("quick-too"));

        assert_eq!(executor.run_all(), 0);
        assert_eq!(executor.next_timer_in(), Some(Duration::from_millis(500)));
        assert_eq!(executor.advance(Duration::from_millis(499)), 0);
        assert_eq!(executor.advance(Duration::from_millis(1)), 2);
        assert_eq!(executor.pending_timers(), 1);
        assert_eq!(executor.run_all(), 2);
        assert_eq!(*log.lock().unwrap(), ["quick", "quick-too"]);

        assert_eq!(executor.advance_and_run(Duration::from_millis(1500)), 1);
        assert_eq!(*log.lock().unwrap(), ["quick", "quick-too", "slow"]);
        assert_eq!(executor.now(), Duration::from_millis(2000));
        assert_eq!(executor.next_timer_in(), None);
    }

    #[test]
    fn a_timer_set_later_is_measured_from_the_current_virtual_time() {
        let executor = ManualExecutor::new();
        let (log, task) = recorder();
        executor.advance(Duration::from_secs(10));
        executor.spawn_after(Duration::from_secs(1), task("timer"));

        assert_eq!(executor.advance_and_run(Duration::from_millis(999)), 0);
        assert_eq!(executor.advance_and_run(Duration::from_millis(1)), 1);
        assert_eq!(*log.lock().unwrap(), ["timer"]);
    }

    #[test]
    fn a_dropped_task_never_runs() {
        let executor = ManualExecutor::new();
        let (log, task) = recorder();
        executor.spawn(task("dropped"));
        executor.spawn(task("kept"));

        assert!(executor.drop_nth(0));
        assert_eq!(executor.run_all(), 1);
        assert_eq!(*log.lock().unwrap(), ["kept"]);
    }
}
