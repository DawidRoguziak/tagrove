//! Process-owned startup scan: the launch snapshot of enabled roots, shared
//! waiting on `tauri::async_runtime` channels, and the retained result. The
//! scan itself is the core's synchronous `scan_service::scan_roots`.

use std::sync::Mutex;

use media_tagger_core::models::ScanSummary;

/// The launch snapshot and its terminal result outlive every frontend owner.
#[derive(Clone)]
pub struct StartupScanState(std::sync::Arc<Mutex<StartupScanPhase>>);

type StartupScanResult = Result<Option<media_tagger_core::models::ScanSummary>, String>;
type StartupScanReceiver = tauri::async_runtime::Receiver<StartupScanResult>;

enum StartupScanPhase {
    Pending(Vec<String>),
    Running(Vec<tauri::async_runtime::Sender<StartupScanResult>>),
    Finished(StartupScanResult),
}

pub enum StartupScanRequest {
    Start(Vec<String>, StartupScanReceiver),
    Observe(StartupScanReceiver),
    Finished(StartupScanResult),
}

impl StartupScanState {
    pub fn new(roots: Vec<media_tagger_core::models::ScanRoot>) -> Self {
        let roots: Vec<_> = roots
            .into_iter()
            .filter(|root| root.auto_scan_on_startup)
            .map(|root| root.path)
            .collect();
        Self(std::sync::Arc::new(Mutex::new(if roots.is_empty() {
            StartupScanPhase::Finished(Ok(None))
        } else {
            StartupScanPhase::Pending(roots)
        })))
    }

    pub fn subscribe(&self) -> Result<StartupScanRequest, String> {
        let mut phase = self
            .0
            .lock()
            .map_err(|e| format!("startup scan lock failed: {e}"))?;
        match &mut *phase {
            StartupScanPhase::Pending(roots) => {
                let roots = std::mem::take(roots);
                let (sender, receiver) = tauri::async_runtime::channel(1);
                *phase = StartupScanPhase::Running(vec![sender]);
                Ok(StartupScanRequest::Start(roots, receiver))
            }
            StartupScanPhase::Running(waiters) => {
                let (sender, receiver) = tauri::async_runtime::channel(1);
                waiters.retain(|waiter| !waiter.is_closed());
                waiters.push(sender);
                Ok(StartupScanRequest::Observe(receiver))
            }
            StartupScanPhase::Finished(result) => Ok(StartupScanRequest::Finished(result.clone())),
        }
    }

    pub fn finish(&self, result: StartupScanResult) {
        // No user work runs under this mutex. Retain the result even if every
        // WebView that requested it has since gone away.
        let mut phase = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let StartupScanPhase::Running(waiters) =
            std::mem::replace(&mut *phase, StartupScanPhase::Finished(result.clone()))
        {
            for waiter in waiters {
                let _ = waiter.try_send(result.clone());
            }
        }
    }
}

pub async fn run_startup_scan(
    startup: &StartupScanState,
    scan: impl FnOnce(Vec<String>) -> Result<ScanSummary, String> + Send + 'static,
) -> Result<Option<ScanSummary>, String> {
    let mut receiver = match startup.subscribe()? {
        StartupScanRequest::Finished(result) => return result,
        StartupScanRequest::Observe(receiver) => receiver,
        StartupScanRequest::Start(roots, receiver) => {
            let startup = startup.clone();
            // A dropped command observer must not abandon completion publication.
            tauri::async_runtime::spawn(async move {
                let result = tauri::async_runtime::spawn_blocking(move || scan(roots))
                    .await
                    .map_err(|e| format!("startup scan worker failed: {e}"))
                    .and_then(|result| result)
                    .map(Some);
                startup.finish(result);
            });
            receiver
        }
    };
    receiver
        .recv()
        .await
        .ok_or_else(|| "startup scan completion unavailable".to_string())?
}

#[cfg(test)]
mod tests {
    use super::{StartupScanRequest, StartupScanState};
    use media_tagger_core::{db, models::ScanRoot};

    fn startup_state() -> StartupScanState {
        StartupScanState::new(vec![media_tagger_core::models::ScanRoot {
            path: "/enabled".into(),
            auto_scan_on_startup: true,
        }])
    }

    #[test]
    fn launch_snapshot_only_consumes_enabled_roots_once_under_contention() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        db::add_scan_root(&conn, "/enabled").unwrap();
        db::add_scan_root(&conn, "/disabled").unwrap();
        db::set_scan_root_auto_scan(&conn, "/enabled", true).unwrap();
        let state = StartupScanState::new(db::list_scan_root_settings(&conn).unwrap());
        // Later checkbox changes belong to the next launch.
        db::set_scan_root_auto_scan(&conn, "/disabled", true).unwrap();
        db::set_scan_root_auto_scan(&conn, "/enabled", false).unwrap();
        let roots = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| match state.subscribe().unwrap() {
                        StartupScanRequest::Start(roots, _) => roots,
                        StartupScanRequest::Observe(_) => vec![],
                        StartupScanRequest::Finished(_) => panic!("scan has not finished"),
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(roots, ["/enabled"]);
        assert!(matches!(
            state.subscribe().unwrap(),
            StartupScanRequest::Observe(_)
        ));
        let next_launch = StartupScanState::new(db::list_scan_root_settings(&conn).unwrap());
        let StartupScanRequest::Start(roots, _) = next_launch.subscribe().unwrap() else {
            panic!("new launch must start");
        };
        assert_eq!(roots, ["/disabled"]);
        assert!(matches!(
            StartupScanState::new(vec![ScanRoot {
                path: "/off".into(),
                auto_scan_on_startup: false
            }])
            .subscribe()
            .unwrap(),
            StartupScanRequest::Finished(Ok(None))
        ));
    }

    #[test]
    fn startup_scan_observers_share_completion_and_replay_without_rescanning() {
        use std::{
            future::Future,
            task::{Context, Waker},
        };
        let startup = startup_state();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first_state = startup.clone();
        let first = tauri::async_runtime::spawn(async move {
            super::run_startup_scan(&first_state, move |roots| {
                assert_eq!(roots, ["/enabled"]);
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(media_tagger_core::models::ScanSummary {
                    completion: media_tagger_core::models::ScanCompletion::Complete,
                    indexed: 3,
                    removed: 0,
                    failed: 0,
                })
            })
            .await
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        tauri::async_runtime::block_on(async {
            // Poll the real command helper while the worker is blocked. This
            // proves the replacement waits instead of returning null.
            let mut replacement = std::pin::pin!(super::run_startup_scan(&startup, |_| panic!(
                "must not rescan"
            )));
            let pending = replacement
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            first.abort();
            release_tx.send(()).unwrap();
            assert!(
                pending.is_pending(),
                "running startup scan must remain observable"
            );
            let result = replacement.await.unwrap().unwrap();
            assert_eq!(result.indexed, 3);
            let replay = super::run_startup_scan(&startup, |_| panic!("must not rescan"))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::to_value(replay).unwrap(),
                serde_json::to_value(result).unwrap()
            );
        });
    }

    #[test]
    fn startup_scan_retains_failures_and_worker_panics_without_retry() {
        for panic_worker in [false, true] {
            let startup = startup_state();
            tauri::async_runtime::block_on(async {
                let error = super::run_startup_scan(&startup, move |_| {
                    if panic_worker {
                        panic!("synthetic worker panic");
                    }
                    Err("synthetic scan failure".into())
                })
                .await
                .unwrap_err();
                assert!(error.contains(if panic_worker {
                    "startup scan worker failed"
                } else {
                    "synthetic scan failure"
                }));
                assert_eq!(
                    super::run_startup_scan(&startup, |_| panic!("must not retry"))
                        .await
                        .unwrap_err(),
                    error
                );
            });
        }
    }

    #[test]
    fn startup_scan_without_enabled_roots_never_starts_worker() {
        let startup = StartupScanState::new(vec![]);
        tauri::async_runtime::block_on(async {
            for _ in 0..2 {
                assert!(
                    super::run_startup_scan(&startup, |_| panic!("no startup work"))
                        .await
                        .unwrap()
                        .is_none()
                );
            }
        });
    }
}
