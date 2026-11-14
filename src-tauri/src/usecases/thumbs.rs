//! Thumbnail maintenance use cases: tool status, bulk renders and clearing.
//! Called by `commands/thumbs.rs`.
//!
//! The thumb lock is taken here, inside the command's blocking task, so it is
//! never held across an await. Render services take the shared guard per
//! short section (`thumb_read_section`), not while waiting.

use crate::{
    app::{locks::with_thumb_lock, locks::with_thumb_operation, state::AppState},
    models::{ThumbnailRenderSummary, VideoToolStatus},
    services::{progress::ProgressSink, thumb_service},
    thumbs,
};

pub fn get_video_tool_status_service(state: &AppState) -> Result<VideoToolStatus, String> {
    let (ffmpeg_available, ffprobe_available) = thumbs::video_tool_status(&state.ffmpeg_path);
    Ok(VideoToolStatus {
        ffmpeg_available,
        ffprobe_available,
    })
}

pub fn render_all_thumbnails_service(
    state: &AppState,
    progress: &impl ProgressSink,
) -> Result<ThumbnailRenderSummary, String> {
    with_thumb_operation(state, |permit| {
        thumb_service::render_all_thumbnails(state, permit, progress)
    })
    .map_err(|e| e.to_string())
}

pub fn render_failed_thumbnails_service(
    state: &AppState,
    progress: &impl ProgressSink,
) -> Result<ThumbnailRenderSummary, String> {
    with_thumb_operation(state, |permit| {
        thumb_service::render_failed_thumbnails(state, permit, progress)
    })
    .map_err(|e| e.to_string())
}

pub fn clear_all_thumbnails_service(
    state: &AppState,
    progress: &impl ProgressSink,
) -> Result<usize, String> {
    with_thumb_lock(state, |permit| {
        thumb_service::clear_all_thumbnails(state, permit, progress)
    })
    .map_err(|e| e.to_string())
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
