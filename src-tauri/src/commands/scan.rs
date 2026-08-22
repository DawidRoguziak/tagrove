use std::path::PathBuf;

use tauri::{Manager, State};

use crate::{
    app::{locks::with_scan_and_thumb_lock, locks::with_scan_lock, state::AppState},
    db,
    models::{RemoveRootSummary, ScanSummary},
    services::{scan_service, thumb_service},
    utils::paths::normalize_root_path,
};

#[tauri::command]
pub async fn scan_folder(
    path: String,
    app: tauri::AppHandle,
) -> Result<ScanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        with_scan_lock(&state, || {
            let normalized = normalize_root_path(&path);
            let root = PathBuf::from(&normalized);
            if !root.exists() || !root.is_dir() {
                return Err("Folder path does not exist or is not a directory".into());
            }
            let conn = db::open_connection(&state.db_path)?;
            db::add_scan_root(&conn, &normalized)?;
            scan_service::scan_roots(&[normalized], &state, &app)
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("scan worker failed: {e}"))?
}

#[tauri::command]
pub fn list_scan_roots(state: State<AppState>) -> Result<Vec<String>, String> {
    (|| {
        let conn = db::open_connection(&state.db_path)?;
        let roots = db::list_scan_roots(&conn)?;
        Ok(scan_service::sort_scan_roots_by_created_desc(roots))
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn add_scan_root(path: String, state: State<AppState>) -> Result<(), String> {
    (|| {
        let normalized = normalize_root_path(&path);
        let root = PathBuf::from(&normalized);
        if !root.exists() || !root.is_dir() {
            return Err("Folder path does not exist or is not a directory".into());
        }

        let conn = db::open_connection(&state.db_path)?;
        db::add_scan_root(&conn, &normalized)?;
        Ok(())
    })()
    .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub fn remove_scan_root(path: String, state: State<AppState>) -> Result<RemoveRootSummary, String> {
    with_scan_and_thumb_lock(&state, || {
        let normalized = normalize_root_path(&path);
        let conn = db::open_connection(&state.db_path)?;

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
        with_scan_lock(&state, || {
            let conn = db::open_connection(&state.db_path)?;
            let roots = db::list_scan_roots(&conn)?;
            scan_service::scan_roots(&roots, &state, &app)
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("rescan worker failed: {e}"))?
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

    use super::{add_scan_root, list_scan_roots, remove_scan_root};

    fn create_test_state(db_path: &Path, thumbs_dir: &Path) -> AppState {
        AppState {
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: std::sync::atomic::AtomicU64::new(0),
            thumbnail_latest_request_id: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn as_state<'a>(state: &'a AppState) -> tauri::State<'a, AppState> {
        unsafe { std::mem::transmute::<&'a AppState, tauri::State<'a, AppState>>(state) }
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

        let error = add_scan_root(invalid_path, as_state(&state)).expect_err("must fail");
        assert!(
            error.contains("does not exist") || error.contains("not a directory"),
            "unexpected error: {error}"
        );

        let roots = list_scan_roots(as_state(&state)).expect("list roots");
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
        add_scan_root(
            format!(
                "  {}{}",
                scan_root.to_string_lossy(),
                std::path::MAIN_SEPARATOR
            ),
            as_state(&state),
        )
        .expect("add valid root");

        let roots = list_scan_roots(as_state(&state)).expect("list roots");
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], scan_root.to_string_lossy().to_string());
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
        let summary = remove_scan_root(scan_root.to_string_lossy().to_string(), as_state(&state))
            .expect("remove root");

        assert_eq!(summary.removed_assets, 1);
        assert_eq!(summary.removed_thumbnails, 1);
        assert!(!thumb_file.exists());

        let roots = list_scan_roots(as_state(&state)).expect("list roots");
        assert!(roots.is_empty());

        let conn = db::open_connection(&db_path).expect("reopen db");
        let page = db::list_assets(&conn, 0, 50, &[], &[], None, false).expect("list assets");
        assert_eq!(page.total, 0);
    }
}
