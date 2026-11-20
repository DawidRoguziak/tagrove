//! The startup scan as a single flight, without a framework runtime.
//!
//! The counterpart of `src-tauri/src/adapter/startup_scan.rs`: the roots with
//! "scan on startup" are a snapshot taken when the profile was opened; the first
//! caller scans them, every caller meanwhile waits for that same scan, and the
//! result (success, failure or a worker panic) is kept for the rest of the
//! process and replayed, never retried. A launch without such roots finishes at
//! once with `None`.
//!
//! The scan runs on the calling thread, a background task of the executor, so a
//! view that goes away cannot abandon it: nothing cancels a running task.

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Condvar, Mutex, MutexGuard},
};

use media_tagger_core::models::{ScanRoot, ScanSummary};

use crate::{
    backend::{BackendError, BackendResult},
    model::panic_text,
};

/// `None`: the launch snapshot had no root to scan.
pub type StartupScanResult = BackendResult<Option<ScanSummary>>;

pub struct StartupScan {
    phase: Mutex<Phase>,
    finished: Condvar,
}

enum Phase {
    Pending(Vec<String>),
    Running,
    Finished(StartupScanResult),
}

/// What a caller has to do, decided under one lock.
#[derive(Debug)]
pub enum StartupScanClaim {
    /// This caller scans the roots and then calls [`StartupScan::finish`].
    Start(Vec<String>),
    /// Another caller is scanning; [`StartupScan::wait`] returns its result.
    Observe,
    /// The scan of this launch already ended with this result.
    Finished(StartupScanResult),
}

impl StartupScan {
    /// The launch snapshot: only roots with `auto_scan_on_startup` are scanned,
    /// and later changes of that setting belong to the next launch.
    pub fn new(roots: Vec<ScanRoot>) -> Self {
        let roots: Vec<String> = roots
            .into_iter()
            .filter(|root| root.auto_scan_on_startup)
            .map(|root| root.path)
            .collect();
        Self {
            phase: Mutex::new(if roots.is_empty() {
                Phase::Finished(Ok(None))
            } else {
                Phase::Pending(roots)
            }),
            finished: Condvar::new(),
        }
    }

    fn phase(&self) -> MutexGuard<'_, Phase> {
        // No user work runs under this mutex, so a poisoned one is still valid.
        self.phase
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Takes the roots exactly once; every later caller observes or replays.
    pub fn claim(&self) -> StartupScanClaim {
        let mut phase = self.phase();
        match &mut *phase {
            Phase::Pending(roots) => {
                let roots = std::mem::take(roots);
                *phase = Phase::Running;
                StartupScanClaim::Start(roots)
            }
            Phase::Running => StartupScanClaim::Observe,
            Phase::Finished(result) => StartupScanClaim::Finished(result.clone()),
        }
    }

    /// Records the terminal result and releases every observer. The first result
    /// stays; a second call changes nothing.
    pub fn finish(&self, result: StartupScanResult) {
        let mut phase = self.phase();
        if !matches!(*phase, Phase::Finished(_)) {
            *phase = Phase::Finished(result);
        }
        drop(phase);
        self.finished.notify_all();
    }

    /// Blocks until the scan has finished and returns its retained result.
    pub fn wait(&self) -> StartupScanResult {
        let mut phase = self.phase();
        loop {
            if let Phase::Finished(result) = &*phase {
                return result.clone();
            }
            phase = self
                .finished
                .wait(phase)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// Joins the startup scan: runs `scan` when this is the first caller, waits
    /// for the running scan otherwise, and returns the retained result when it
    /// has ended. A panic in `scan` is recorded as a `WorkerFailed` error.
    pub fn run(
        &self,
        scan: impl FnOnce(Vec<String>) -> BackendResult<ScanSummary>,
    ) -> StartupScanResult {
        match self.claim() {
            StartupScanClaim::Finished(result) => result,
            StartupScanClaim::Observe => self.wait(),
            StartupScanClaim::Start(roots) => {
                let result = catch_unwind(AssertUnwindSafe(|| scan(roots)))
                    .unwrap_or_else(|payload| {
                        Err(BackendError::worker_failed(
                            "startup scan",
                            &panic_text(payload.as_ref()),
                        ))
                    })
                    .map(Some);
                self.finish(result.clone());
                result
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

    use media_tagger_core::models::{ScanCompletion, ScanRoot, ScanSummary};

    use super::{StartupScan, StartupScanClaim};
    use crate::backend::{BackendError, BackendErrorKind};

    fn root(path: &str, auto_scan_on_startup: bool) -> ScanRoot {
        ScanRoot {
            path: path.to_owned(),
            auto_scan_on_startup,
        }
    }

    fn enabled() -> StartupScan {
        StartupScan::new(vec![root("/enabled", true)])
    }

    fn summary(indexed: usize) -> ScanSummary {
        ScanSummary {
            completion: ScanCompletion::Complete,
            indexed,
            removed: 0,
            failed: 0,
        }
    }

    #[test]
    fn the_launch_snapshot_holds_only_enabled_roots() {
        let startup = StartupScan::new(vec![
            root("/enabled", true),
            root("/disabled", false),
            root("/also-enabled", true),
        ]);

        let StartupScanClaim::Start(roots) = startup.claim() else {
            panic!("the first caller starts the scan");
        };
        assert_eq!(roots, ["/enabled", "/also-enabled"]);
    }

    #[test]
    fn the_roots_are_handed_out_once_under_contention() {
        let startup = enabled();

        let started = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| match startup.claim() {
                        StartupScanClaim::Start(roots) => roots,
                        StartupScanClaim::Observe => Vec::new(),
                        StartupScanClaim::Finished(_) => panic!("the scan has not finished"),
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });

        assert_eq!(started, ["/enabled"]);
        assert!(matches!(startup.claim(), StartupScanClaim::Observe));
    }

    #[test]
    fn a_launch_without_enabled_roots_never_scans() {
        for roots in [Vec::new(), vec![root("/off", false)]] {
            let startup = StartupScan::new(roots);
            for _ in 0..2 {
                let result = startup.run(|_| panic!("no startup work"));
                assert!(result.unwrap().is_none());
            }
        }
    }

    #[test]
    fn observers_share_the_running_scan_and_later_callers_replay_it() {
        let startup = Arc::new(enabled());
        let scans = Arc::new(AtomicUsize::new(0));
        let (entered_tx, entered) = mpsc::channel();
        let (release, held) = mpsc::channel::<()>();

        let first = {
            let (startup, scans) = (Arc::clone(&startup), Arc::clone(&scans));
            std::thread::spawn(move || {
                startup.run(move |roots| {
                    assert_eq!(roots, ["/enabled"]);
                    scans.fetch_add(1, Ordering::SeqCst);
                    entered_tx.send(()).unwrap();
                    held.recv().unwrap();
                    Ok(summary(3))
                })
            })
        };
        entered.recv_timeout(Duration::from_secs(10)).unwrap();

        let (observed_tx, observed) = mpsc::channel();
        let observer = {
            let startup = Arc::clone(&startup);
            std::thread::spawn(move || {
                let result = startup.run(|_| panic!("must not rescan"));
                observed_tx.send(()).unwrap();
                result
            })
        };
        assert!(
            observed.recv_timeout(Duration::from_millis(100)).is_err(),
            "a running startup scan must stay observable instead of answering early"
        );

        release.send(()).unwrap();
        assert_eq!(first.join().unwrap().unwrap().unwrap().indexed, 3);
        assert_eq!(observer.join().unwrap().unwrap().unwrap().indexed, 3);
        let replay = startup.run(|_| panic!("must not rescan"));
        assert_eq!(replay.unwrap().unwrap().indexed, 3);
        assert_eq!(scans.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failures_and_worker_panics_are_retained_without_a_retry() {
        for panic_worker in [false, true] {
            let startup = enabled();

            let error = startup
                .run(move |_| {
                    if panic_worker {
                        panic!("synthetic worker panic");
                    }
                    Err(BackendError::rejected("synthetic scan failure"))
                })
                .unwrap_err();

            if panic_worker {
                assert_eq!(error.kind(), BackendErrorKind::WorkerFailed);
                assert_eq!(
                    error.text(),
                    "startup scan worker failed: synthetic worker panic"
                );
            } else {
                assert_eq!(error.text(), "synthetic scan failure");
            }
            assert_eq!(
                startup.run(|_| panic!("must not retry")).unwrap_err(),
                error
            );
        }
    }

    #[test]
    fn the_first_terminal_result_stays() {
        let startup = enabled();
        assert!(matches!(startup.claim(), StartupScanClaim::Start(_)));

        startup.finish(Ok(Some(summary(5))));
        startup.finish(Err(BackendError::rejected("late")));

        assert_eq!(startup.wait().unwrap().unwrap().indexed, 5);
        assert!(matches!(
            startup.claim(),
            StartupScanClaim::Finished(Ok(Some(_)))
        ));
    }
}
