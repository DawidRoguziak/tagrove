use std::path::PathBuf;

use tauri::Manager;

use crate::{
    app::{locks::with_scan_and_thumb_lock, locks::with_scan_lock, state::AppState},
    db,
    models::{RemoveRootSummary, ScanRoot, ScanSummary},
    services::{progress::ProgressSink, scan_service, thumb_service},
    utils::paths::normalize_root_path,
};

#[tauri::command]
pub async fn scan_folder(path: String, app: tauri::AppHandle) -> Result<ScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        scan_folder_service(path, &state, &app)
    })
    .await
    .map_err(|e| format!("scan worker failed: {e}"))?
}

fn scan_folder_service(
    path: String,
    state: &AppState,
    progress: &impl ProgressSink,
) -> Result<ScanSummary, String> {
    with_scan_lock(state, |permit| {
        let normalized = normalize_root_path(&path);
        let root = PathBuf::from(&normalized);
        if !root.exists() || !root.is_dir() {
            return Err("Folder path does not exist or is not a directory".into());
        }
        // Return this connection before the scan takes its own, so a scan
        // occupies one pooled connection.
        {
            let conn = permit.connection()?;
            db::add_scan_root(&conn, &normalized)?;
        }
        scan_service::scan_roots(&[normalized], state, permit, progress)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_scan_roots(app: tauri::AppHandle) -> Result<Vec<ScanRoot>, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        list_scan_roots_service(&state)
    })
    .await
    .map_err(|e| format!("list_scan_roots worker failed: {e}"))?
}

fn list_scan_roots_service(state: &AppState) -> Result<Vec<ScanRoot>, String> {
    (|| {
        let conn = state.database.admit()?.connection()?;
        let roots = db::list_scan_root_settings(&conn)?;
        Ok(scan_service::sort_scan_roots_by_created_desc(roots))
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn add_scan_root(path: String, app: tauri::AppHandle) -> Result<(), String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        add_scan_root_service(path, &state)
    })
    .await
    .map_err(|e| format!("add_scan_root worker failed: {e}"))?
}

fn add_scan_root_service(path: String, state: &AppState) -> Result<(), String> {
    (|| {
        let normalized = normalize_root_path(&path);
        let root = PathBuf::from(&normalized);
        if !root.exists() || !root.is_dir() {
            return Err("Folder path does not exist or is not a directory".into());
        }

        let conn = state.database.admit()?.connection()?;
        db::add_scan_root(&conn, &normalized)?;
        Ok(())
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn remove_scan_root(
    path: String,
    app: tauri::AppHandle,
) -> Result<RemoveRootSummary, String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        remove_scan_root_service(path, &state)
    })
    .await
    .map_err(|e| format!("remove_scan_root worker failed: {e}"))?
}

fn remove_scan_root_service(path: String, state: &AppState) -> Result<RemoveRootSummary, String> {
    with_scan_and_thumb_lock(state, |permit| {
        let normalized = normalize_root_path(&path);
        let conn = permit.connection()?;

        // The revision bump is committed atomically with the removal inside
        // remove_scan_root_and_orphan_assets.
        let (removed_assets, thumbs) = db::remove_scan_root_and_orphan_assets(&conn, &normalized)?;
        let removed_thumbnails =
            thumb_service::delete_thumbnail_files_in_root(&state.thumbs_dir, thumbs);

        Ok(RemoveRootSummary {
            removed_assets,
            removed_thumbnails,
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rescan_all_roots(app: tauri::AppHandle) -> Result<ScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        rescan_all_roots_service(&state, &app)
    })
    .await
    .map_err(|e| format!("rescan worker failed: {e}"))?
}

fn rescan_all_roots_service(
    state: &AppState,
    progress: &impl ProgressSink,
) -> Result<ScanSummary, String> {
    with_scan_lock(state, |permit| {
        let roots = {
            let conn = permit.connection()?;
            db::list_scan_roots(&conn)?
        };
        scan_service::scan_roots(&roots, state, permit, progress)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_scan_root_auto_scan(
    path: String,
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_state = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_state.state::<AppState>();
        set_scan_root_auto_scan_service(path, enabled, &state)
    })
    .await
    .map_err(|e| format!("set_scan_root_auto_scan worker failed: {e}"))?
}

fn set_scan_root_auto_scan_service(
    path: String,
    enabled: bool,
    state: &AppState,
) -> Result<(), String> {
    let conn = state
        .database
        .admit()
        .and_then(|permit| permit.connection())
        .map_err(|e| e.to_string())?;
    db::set_scan_root_auto_scan(&conn, &normalize_root_path(&path), enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_startup_roots(app: tauri::AppHandle) -> Result<Option<ScanSummary>, String> {
    let startup = app
        .state::<crate::app::state::StartupScanState>()
        .inner()
        .clone();
    run_startup_scan(&startup, move |roots| {
        let state = app.state::<AppState>();
        with_scan_lock(&state, |permit| {
            scan_service::scan_roots(&roots, &state, permit, &app)
        })
        .map_err(|e| e.to_string())
    })
    .await
}

async fn run_startup_scan(
    startup: &crate::app::state::StartupScanState,
    scan: impl FnOnce(Vec<String>) -> Result<ScanSummary, String> + Send + 'static,
) -> Result<Option<ScanSummary>, String> {
    use crate::app::state::StartupScanRequest;
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
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{atomic::AtomicBool, Mutex, RwLock},
    };

    use tempfile::tempdir;

    use crate::{
        app::state::AppState, db, models::NewAsset, services::thumb_scheduler::ThumbnailScheduler,
    };

    use super::{
        add_scan_root_service, list_scan_roots_service, remove_scan_root_service,
        rescan_all_roots_service, scan_folder_service,
    };

    fn startup_state() -> crate::app::state::StartupScanState {
        crate::app::state::StartupScanState::new(vec![crate::models::ScanRoot {
            path: "/enabled".into(),
            auto_scan_on_startup: true,
        }])
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
                Ok(crate::models::ScanSummary {
                    completion: crate::models::ScanCompletion::Complete,
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
        let startup = crate::app::state::StartupScanState::new(vec![]);
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

    fn create_test_state(db_path: &Path, thumbs_dir: &Path) -> AppState {
        AppState {
            database: crate::services::db_pool::DatabaseRuntime::new(db_path.to_path_buf()),
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn new_asset(path: &Path, modified_at: i64, thumb_path: Option<&Path>) -> NewAsset {
        NewAsset {
            path: path.to_string_lossy().to_string(),
            kind: "image".to_string(),
            size_bytes: 10,
            modified_at,
            width: Some(100),
            height: Some(100),
            duration_ms: None,
            thumb_path: thumb_path.map(|value| value.to_string_lossy().to_string()),
        }
    }

    #[test]
    fn scan_commands_hold_one_connection_while_scanning() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        drop(conn);
        let root = tmp.path().join("library");
        fs::create_dir_all(&root).expect("create root");
        image::RgbImage::new(2, 2)
            .save(root.join("seed.png"))
            .expect("seed image");
        for index in 0..150 {
            fs::copy(
                root.join("seed.png"),
                root.join(format!("copy-{index}.png")),
            )
            .expect("copy image");
        }
        let state = create_test_state(&db_path, &thumbs_dir);
        let most = std::sync::atomic::AtomicUsize::new(0);
        let events = std::sync::atomic::AtomicUsize::new(0);
        let sink = |_progress: crate::models::ScanProgress| -> crate::error::AppResult<()> {
            events.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            most.fetch_max(
                state.database.checked_out_connections(),
                std::sync::atomic::Ordering::SeqCst,
            );
            Ok(())
        };

        let first =
            scan_folder_service(root.to_string_lossy().into(), &state, &sink).expect("scan folder");
        assert_eq!(first.indexed, 151);
        let again = rescan_all_roots_service(&state, &sink).expect("rescan");
        assert_eq!(again.indexed, 151);
        assert!(events.load(std::sync::atomic::Ordering::SeqCst) > 0);
        assert_eq!(most.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(state.database.checked_out_connections(), 0);
    }

    #[test]
    fn add_scan_root_rejects_invalid_folder_and_does_not_persist_root() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let state = create_test_state(&db_path, &thumbs_dir);
        let invalid_path = tmp
            .path()
            .join("missing-root")
            .to_string_lossy()
            .to_string();

        let error = add_scan_root_service(invalid_path, &state).expect_err("must fail");
        assert!(
            error.contains("does not exist") || error.contains("not a directory"),
            "unexpected error: {error}"
        );

        let roots = list_scan_roots_service(&state).expect("list roots");
        assert!(roots.is_empty());
    }

    #[test]
    fn add_scan_root_persists_valid_folder() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let scan_root = tmp.path().join("library");
        fs::create_dir_all(&scan_root).expect("create scan root");
        let state = create_test_state(&db_path, &thumbs_dir);
        add_scan_root_service(
            format!(
                "  {}{}",
                scan_root.to_string_lossy(),
                std::path::MAIN_SEPARATOR
            ),
            &state,
        )
        .expect("add valid root");

        let roots = list_scan_roots_service(&state).expect("list roots");
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, scan_root.to_string_lossy().to_string());
        assert!(!roots[0].auto_scan_on_startup);
    }

    #[test]
    fn remove_scan_root_deletes_assets_and_thumbnail_files() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let scan_root = tmp.path().join("library");
        fs::create_dir_all(&scan_root).expect("create scan root");
        db::add_scan_root(&conn, &scan_root.to_string_lossy()).expect("add scan root");

        let media_file = scan_root.join("asset.jpg");
        fs::write(&media_file, b"asset").expect("write media file");

        let thumb_file = thumbs_dir.join("thumb-a.jpg");
        fs::write(&thumb_file, b"thumb").expect("write thumb file");

        db::upsert_asset(&conn, &new_asset(&media_file, 42, Some(&thumb_file)))
            .expect("upsert asset");

        let state = create_test_state(&db_path, &thumbs_dir);
        let summary = remove_scan_root_service(scan_root.to_string_lossy().to_string(), &state)
            .expect("remove root");

        assert_eq!(summary.removed_assets, 1);
        assert_eq!(summary.removed_thumbnails, 1);
        assert!(!thumb_file.exists());

        let roots = list_scan_roots_service(&state).expect("list roots");
        assert!(roots.is_empty());

        let conn = db::open_connection(&db_path).expect("reopen db");
        let page = db::test_support::session_page(&conn, 0, 50, &[], &[], None, false)
            .expect("list assets");
        assert_eq!(page.total, 0);
    }
}
