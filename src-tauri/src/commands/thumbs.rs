use tauri::{ipc::Channel, State};

use crate::{
    app::{
        locks::{with_thumb_lock, with_thumb_read_lock},
        state::AppState,
    },
    models::{ThumbnailBatchResult, ThumbnailRenderSummary, ThumbnailStreamEvent, VideoToolStatus},
    services::thumb_service,
    thumbs,
};

#[tauri::command(async)]
pub fn get_video_tool_status(state: State<AppState>) -> VideoToolStatus {
    let (ffmpeg_available, ffprobe_available) = thumbs::video_tool_status(&state.ffmpeg_path);
    VideoToolStatus {
        ffmpeg_available,
        ffprobe_available,
    }
}

#[tauri::command(async)]
pub fn render_all_thumbnails(
    state: State<AppState>,
    app: tauri::AppHandle,
) -> Result<ThumbnailRenderSummary, String> {
    with_thumb_read_lock(&state, || {
        thumb_service::render_all_thumbnails(&state, &app)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn render_failed_thumbnails(
    state: State<AppState>,
    app: tauri::AppHandle,
) -> Result<ThumbnailRenderSummary, String> {
    with_thumb_read_lock(&state, || {
        thumb_service::render_failed_thumbnails(&state, &app)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn cancel_render_all_thumbnails(state: State<AppState>) -> Result<bool, String> {
    with_thumb_read_lock(&state, || {
        thumb_service::cancel_render_all_thumbnails(&state)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn clear_all_thumbnails(
    state: State<AppState>,
    app: tauri::AppHandle,
) -> Result<usize, String> {
    with_thumb_lock(&state, || thumb_service::clear_all_thumbnails(&state, &app))
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn ensure_asset_thumbnail(
    asset_id: i64,
    state: State<AppState>,
) -> Result<Option<String>, String> {
    with_thumb_read_lock(&state, || {
        thumb_service::ensure_asset_thumbnail(asset_id, &state)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn ensure_page_thumbnails(
    asset_ids: Vec<i64>,
    state: State<AppState>,
    app: tauri::AppHandle,
) -> Result<ThumbnailBatchResult, String> {
    with_thumb_read_lock(&state, || {
        thumb_service::ensure_page_thumbnails(asset_ids, &state, &app)
    })
    .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn ensure_thumbnails(
    request_id: u64,
    visible_ids: Vec<i64>,
    prefetch_ids: Vec<i64>,
    on_event: Channel<ThumbnailStreamEvent>,
    state: State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let _ = request_id;
    with_thumb_read_lock(&state, || {
        thumb_service::ensure_thumbnails_stream(visible_ids, prefetch_ids, &state, &app, on_event)
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex, RwLock,
        },
    };

    use tempfile::tempdir;

    use crate::{app::state::AppState, db, services::thumb_scheduler::ThumbnailScheduler};

    use super::cancel_render_all_thumbnails;

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
        }
    }

    fn as_state<'a>(state: &'a AppState) -> tauri::State<'a, AppState> {
        unsafe { std::mem::transmute::<&'a AppState, tauri::State<'a, AppState>>(state) }
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
        let accepted =
            cancel_render_all_thumbnails(as_state(&state)).expect("cancel command result");
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

        let accepted =
            cancel_render_all_thumbnails(as_state(&state)).expect("cancel command result");
        assert!(accepted);
        assert!(state
            .thumbnail_render_all_cancel_requested
            .load(Ordering::SeqCst));
    }
}
