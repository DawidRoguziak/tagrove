use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Mutex, RwLock,
    },
};

use crate::services::thumb_scheduler::ThumbnailScheduler;

pub struct AppState {
    pub database: std::sync::Arc<crate::services::db_pool::DatabaseRuntime>,
    pub db_path: PathBuf,
    pub thumbs_dir: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub scan_lock: Mutex<()>,
    pub thumb_lock: RwLock<()>,
    pub thumb_scheduler: ThumbnailScheduler,
    pub thumbnail_render_all_running: AtomicBool,
    pub thumbnail_render_all_cancel_requested: AtomicBool,
    /// Monotonic epoch guarding thumbnail publication. Cleared-thumbnail
    /// workflows bump it so results rendered before the reset are dropped
    /// instead of being written back to SQLite.
    pub thumbnail_generation: AtomicU64,
}

/// The launch snapshot is consumed even if scanning fails; only a new process retries.
pub struct StartupScanState(Mutex<Vec<String>>);

impl StartupScanState {
    pub fn new(roots: Vec<crate::models::ScanRoot>) -> Self {
        Self(Mutex::new(
            roots
                .into_iter()
                .filter(|root| root.auto_scan_on_startup)
                .map(|root| root.path)
                .collect(),
        ))
    }

    pub fn take_roots(&self) -> Result<Vec<String>, String> {
        let mut roots = self
            .0
            .lock()
            .map_err(|e| format!("startup scan lock failed: {e}"))?;
        Ok(std::mem::take(&mut *roots))
    }
}

#[cfg(test)]
mod startup_tests {
    use super::StartupScanState;
    use crate::{db, models::ScanRoot};

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
                .map(|_| scope.spawn(|| state.take_roots().unwrap()))
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(roots, ["/enabled"]);
        assert!(state.take_roots().unwrap().is_empty());
        let next_launch = StartupScanState::new(db::list_scan_root_settings(&conn).unwrap());
        assert_eq!(next_launch.take_roots().unwrap(), ["/disabled"]);
        assert!(StartupScanState::new(vec![ScanRoot {
            path: "/off".into(),
            auto_scan_on_startup: false
        }])
        .take_roots()
        .unwrap()
        .is_empty());
    }
}
