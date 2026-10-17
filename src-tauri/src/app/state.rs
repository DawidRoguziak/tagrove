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

/// Immutable until the desktop process exits, including after restore or clear.
#[derive(Default)]
pub struct StartupPopularTags(Vec<String>);

impl StartupPopularTags {
    pub fn new(tags: Vec<String>) -> Self {
        Self(tags)
    }

    pub fn tags(&self) -> &[String] {
        &self.0
    }
}

/// The launch snapshot and its terminal result outlive every frontend owner.
#[derive(Clone)]
pub struct StartupScanState(std::sync::Arc<Mutex<StartupScanPhase>>);

type StartupScanResult = Result<Option<crate::models::ScanSummary>, String>;
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
    pub fn new(roots: Vec<crate::models::ScanRoot>) -> Self {
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

#[cfg(test)]
mod startup_tests {
    use super::{StartupPopularTags, StartupScanRequest, StartupScanState};
    use crate::{db, models::ScanRoot};

    fn popular_tags_snapshot(conn: &rusqlite::Connection) -> StartupPopularTags {
        StartupPopularTags::new(db::list_popular_tags(conn).unwrap())
    }

    fn tag_fixture() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO assets(id, path, kind, size_bytes, modified_at) VALUES
             (1, '/one.png', 'image', 1, 1),
             (2, '/two.mp4', 'video', 1, 1),
             (3, '/three.gif', 'gif', 1, 1);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn popular_tags_omit_empty_library_and_unused_tags() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        assert!(popular_tags_snapshot(&conn).tags().is_empty());
        conn.execute("INSERT INTO tags(name) VALUES ('unused')", [])
            .unwrap();
        assert!(popular_tags_snapshot(&conn).tags().is_empty());
    }

    #[test]
    fn popular_tags_count_assignments_across_media_kinds_and_break_ties_by_name() {
        let conn = tag_fixture();
        conn.execute_batch(
            "INSERT INTO tags(id, name) VALUES (1, 'zebra'), (2, 'Beta'), (3, 'alpha'), (4, 'unused');
             INSERT INTO asset_tags(asset_id, tag_id) VALUES
             (1, 1), (2, 1), (3, 1), (1, 2), (2, 2), (1, 3), (3, 3);",
        )
        .unwrap();
        assert_eq!(
            popular_tags_snapshot(&conn).tags(),
            ["zebra", "alpha", "Beta"]
        );
    }

    #[test]
    fn popular_tags_limit_to_ten_after_ranking() {
        let conn = tag_fixture();
        for id in (1..=12).rev() {
            conn.execute(
                "INSERT INTO tags(id, name) VALUES (?1, ?2)",
                rusqlite::params![id, format!("tag-{id:02}")],
            )
            .unwrap();
            conn.execute("INSERT INTO asset_tags VALUES (1, ?1)", [id])
                .unwrap();
        }
        conn.execute("INSERT INTO asset_tags VALUES (2, 12)", [])
            .unwrap();
        let expected: Vec<_> = std::iter::once("tag-12".to_string())
            .chain((1..=9).map(|id| format!("tag-{id:02}")))
            .collect();
        assert_eq!(popular_tags_snapshot(&conn).tags(), expected);
    }

    #[test]
    fn popular_tags_snapshot_survives_assignment_changes_and_clearing_until_next_launch() {
        let conn = tag_fixture();
        db::set_asset_tags(&conn, 1, &["cat".into()]).unwrap();
        let startup = popular_tags_snapshot(&conn);
        db::set_asset_tags(&conn, 1, &["dog".into()]).unwrap();
        db::set_asset_tags(&conn, 2, &["dog".into(), "bird".into()]).unwrap();
        assert_eq!(startup.tags(), ["cat"]);
        assert_eq!(popular_tags_snapshot(&conn).tags(), ["dog", "bird"]);
        conn.execute_batch("DELETE FROM asset_tags; DELETE FROM tags; DELETE FROM assets;")
            .unwrap();
        assert_eq!(startup.tags(), ["cat"]);
        assert!(popular_tags_snapshot(&conn).tags().is_empty());
        drop(conn);
        assert_eq!(startup.tags(), ["cat"]);
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
}
