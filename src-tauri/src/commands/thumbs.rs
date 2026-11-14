use tauri::Manager;

use crate::{
    app::state::AppState,
    models::{ThumbnailRenderSummary, VideoToolStatus},
    services::thumb_service,
    usecases::thumbs,
};

// Every thumbnail command performs blocking work (SQLite, filesystem scans,
// scheduler waits). Each body runs on the blocking thread pool via
// `spawn_blocking`, and the use case takes the thumb lock inside that blocking
// task so no lock is ever held across an await (usecases/thumbs.rs).
// On-demand gallery thumbnails are served by the `thumb://` scheme
// (adapter/thumb_scheme.rs over services/thumb_protocol.rs), not by a command.

#[tauri::command]
pub async fn get_video_tool_status(app: tauri::AppHandle) -> Result<VideoToolStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        thumbs::get_video_tool_status_service(&state)
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
        thumbs::render_all_thumbnails_service(&state, &app)
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
        thumbs::render_failed_thumbnails_service(&state, &app)
    })
    .await
    .map_err(|e| format!("thumbnail render worker failed: {e}"))?
}

/// Takes no lock: it only sets the cancel flag of a running bulk render.
#[tauri::command]
pub async fn cancel_render_all_thumbnails(app: tauri::AppHandle) -> Result<bool, String> {
    let state = app.state::<AppState>();
    thumb_service::cancel_render_all_thumbnails(&state).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_all_thumbnails(app: tauri::AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        thumbs::clear_all_thumbnails_service(&state, &app)
    })
    .await
    .map_err(|e| format!("thumbnail clear worker failed: {e}"))?
}
