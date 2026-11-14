use tauri::Manager;

use crate::{
    app::{locks::with_thumb_lock, locks::with_thumb_operation, state::AppState},
    models::{ThumbnailRenderSummary, VideoToolStatus},
    services::thumb_service,
    thumbs,
};

// Every thumbnail command performs blocking work (SQLite, filesystem scans,
// scheduler waits). Each body runs on the blocking thread pool via
// `spawn_blocking`, and the thumb lock is acquired inside that blocking task
// so no lock is ever held across an await. Render services take the shared
// guard per short section (`thumb_read_section`), not while waiting.
// On-demand gallery thumbnails are served by the `thumb://` scheme
// (adapter/thumb_scheme.rs over services/thumb_protocol.rs), not by a command.

#[tauri::command]
pub async fn get_video_tool_status(app: tauri::AppHandle) -> Result<VideoToolStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (ffmpeg_available, ffprobe_available) = thumbs::video_tool_status(&state.ffmpeg_path);
        Ok(VideoToolStatus {
            ffmpeg_available,
            ffprobe_available,
        })
    })
    .await
    .map_err(|e| format!("video tool status worker failed: {e}"))?
}

#[tauri::command]
pub async fn render_all_thumbnails(
    app: tauri::AppHandle,
) -> Result<ThumbnailRenderSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        with_thumb_operation(&state, |permit| {
            thumb_service::render_all_thumbnails(&state, permit, &app)
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("thumbnail render worker failed: {e}"))?
}

#[tauri::command]
pub async fn render_failed_thumbnails(
    app: tauri::AppHandle,
) -> Result<ThumbnailRenderSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        with_thumb_operation(&state, |permit| {
            thumb_service::render_failed_thumbnails(&state, permit, &app)
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("thumbnail render worker failed: {e}"))?
}

#[tauri::command]
pub async fn cancel_render_all_thumbnails(app: tauri::AppHandle) -> Result<bool, String> {
    let state = app.state::<AppState>();
    thumb_service::cancel_render_all_thumbnails(&state).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_all_thumbnails(app: tauri::AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        with_thumb_lock(&state, |permit| {
            thumb_service::clear_all_thumbnails(&state, permit, &app)
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("thumbnail clear worker failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Mutex, RwLock,
        },
    };

    use tempfile::tempdir;

    use crate::{app::state::AppState, db, services::thumb_scheduler::ThumbnailScheduler};

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
            thumbnail_generation: AtomicU64::new(0),
        }
    }

    #[test]
    fn cancel_render_all_thumbnails_returns_false_when_not_running() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let state = create_test_state(&db_path, &thumbs_dir);
        let accepted = crate::services::thumb_service::cancel_render_all_thumbnails(&state)
            .expect("cancel command result");
        assert!(!accepted);
        assert!(!state
            .thumbnail_render_all_cancel_requested
            .load(Ordering::SeqCst));
    }

    #[test]
    fn cancel_render_all_thumbnails_returns_true_and_sets_cancel_flag_when_running() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");

        let state = create_test_state(&db_path, &thumbs_dir);
        state
            .thumbnail_render_all_running
            .store(true, Ordering::SeqCst);

        let accepted = crate::services::thumb_service::cancel_render_all_thumbnails(&state)
            .expect("cancel command result");
        assert!(accepted);
        assert!(state
            .thumbnail_render_all_cancel_requested
            .load(Ordering::SeqCst));
    }
}
