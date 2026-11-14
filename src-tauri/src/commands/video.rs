use anyhow::Context;
use serde::Deserialize;
use tauri::{ipc::Channel, State, WebviewWindow};

use media_tagger_core::services::video_source_service;

use crate::video::{
    player::{PlayerCommand, VideoEvent, VideoPlayerService},
    types::{VideoBounds, VideoControlLabels},
};

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VideoControl {
    Play,
    Pause,
    TogglePause,
    Seek { time: f64 },
    SeekRelative { seconds: f64 },
    SetVolume { volume: f64 },
    SetMuted { muted: bool },
    SetRate { rate: f64 },
    SelectAudioTrack { track_id: String },
    SelectSubtitleTrack { track_id: String },
    SetFullscreen { fullscreen: bool },
    ToggleFullscreen,
}

#[tauri::command]
// Tauri injects the window and both managed states in addition to the wire arguments.
#[allow(clippy::too_many_arguments)]
pub async fn open_video(
    asset_id: i64,
    request_id: u64,
    bounds: VideoBounds,
    control_labels: VideoControlLabels,
    on_event: Channel<VideoEvent>,
    window: WebviewWindow,
    player: State<'_, VideoPlayerService>,
    app: State<'_, media_tagger_core::app::state::AppState>,
) -> Result<u64, String> {
    if asset_id <= 0 {
        return Err("asset id must be positive".to_string());
    }
    let bounds = bounds.validate()?;
    let control_labels = control_labels.validate()?;
    player.require_pending(request_id)?;
    let database = app.database.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        let conn = database.admit()?.connection()?;
        video_source_service::resolve_video_path(&conn, asset_id)
            .map_err(media_tagger_core::error::AppError::from)
    })
    .await
    .map_err(|error| format!("video source worker failed: {error}"))?
    .map_err(|error| error.to_string())?;
    let source = path
        .to_str()
        .context("video path is not valid UTF-8")
        .map_err(|error| error.to_string())?
        .to_string();
    player.require_pending(request_id)?;
    crate::video::surface::ensure_ready(&window)?;
    let worker = player.inner().clone();
    let session_id =
        tauri::async_runtime::spawn_blocking(move || worker.open(request_id, source, on_event))
            .await
            .map_err(|error| format!("video worker failed: {error}"))??;
    if let Err(error) =
        crate::video::surface::activate(&window, &player, session_id, bounds, control_labels)
    {
        let _ = player.close(session_id);
        let _ = crate::video::surface::hide(&window, session_id);
        return Err(error);
    }
    Ok(session_id)
}

#[tauri::command]
pub fn set_video_bounds(
    session_id: u64,
    bounds: VideoBounds,
    window: WebviewWindow,
    player: State<'_, VideoPlayerService>,
) -> Result<(), String> {
    player.require_current(session_id)?;
    crate::video::surface::set_bounds(&window, &player, session_id, bounds.validate()?)
}

#[tauri::command]
pub fn control_video(
    session_id: u64,
    command: VideoControl,
    window: WebviewWindow,
    player: State<'_, VideoPlayerService>,
) -> Result<(), String> {
    let command = match command {
        VideoControl::Play => PlayerCommand::Play,
        VideoControl::Pause => PlayerCommand::Pause,
        VideoControl::TogglePause => PlayerCommand::TogglePause,
        VideoControl::Seek { time } if time.is_finite() && time >= 0.0 => PlayerCommand::Seek(time),
        VideoControl::SeekRelative { seconds } if seconds.is_finite() => {
            PlayerCommand::SeekRelative(seconds)
        }
        VideoControl::SetVolume { volume }
            if volume.is_finite() && (0.0..=1.0).contains(&volume) =>
        {
            PlayerCommand::SetVolume(volume)
        }
        VideoControl::SetMuted { muted } => PlayerCommand::SetMuted(muted),
        VideoControl::SetRate { rate } if rate.is_finite() && (0.25..=4.0).contains(&rate) => {
            PlayerCommand::SetRate(rate)
        }
        VideoControl::SelectAudioTrack { track_id } if !track_id.trim().is_empty() => {
            PlayerCommand::SelectAudioTrack(track_id)
        }
        VideoControl::SelectSubtitleTrack { track_id } if !track_id.trim().is_empty() => {
            PlayerCommand::SelectSubtitleTrack(track_id)
        }
        VideoControl::SetFullscreen { fullscreen } => {
            return crate::video::surface::set_fullscreen(
                &window,
                &player,
                session_id,
                crate::video::surface::FullscreenAction::Set(fullscreen),
            );
        }
        VideoControl::ToggleFullscreen => {
            return crate::video::surface::set_fullscreen(
                &window,
                &player,
                session_id,
                crate::video::surface::FullscreenAction::Toggle,
            );
        }
        _ => return Err("invalid video control value".to_string()),
    };
    player.control(session_id, command)
}

#[tauri::command]
pub fn close_video(
    session_id: u64,
    window: WebviewWindow,
    player: State<'_, VideoPlayerService>,
) -> Result<(), String> {
    let close_result = player.close(session_id);
    let hide_result = crate::video::surface::hide(&window, session_id);
    close_result.and(hide_result)
}

#[tauri::command]
pub fn begin_video_open(player: State<'_, VideoPlayerService>) -> u64 {
    player.begin_open()
}

#[tauri::command]
pub fn cancel_video_open(
    request_id: u64,
    window: WebviewWindow,
    player: State<'_, VideoPlayerService>,
) -> Result<(), String> {
    if let Some(session_id) = player.cancel_open(request_id) {
        let close = player.close(session_id);
        let hide = crate::video::surface::hide(&window, session_id);
        close.and(hide)
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn set_video_control_labels(
    session_id: u64,
    control_labels: VideoControlLabels,
    window: WebviewWindow,
    player: State<'_, VideoPlayerService>,
) -> Result<(), String> {
    crate::video::surface::set_control_labels(
        &window,
        &player,
        session_id,
        control_labels.validate()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_camel_case_track_control_fields() {
        assert!(matches!(
            serde_json::from_value::<VideoControl>(serde_json::json!({ "type": "togglePause" }))
                .unwrap(),
            VideoControl::TogglePause
        ));
        assert!(matches!(
            serde_json::from_value::<VideoControl>(serde_json::json!({ "type": "seekRelative", "seconds": -5 })).unwrap(),
            VideoControl::SeekRelative { seconds } if seconds == -5.0
        ));
        assert!(
            matches!(serde_json::from_value::<VideoControl>(serde_json::json!({
            "type": "selectAudioTrack", "trackId": "2"
        })).unwrap(), VideoControl::SelectAudioTrack { track_id } if track_id == "2")
        );
        assert!(
            matches!(serde_json::from_value::<VideoControl>(serde_json::json!({
            "type": "selectSubtitleTrack", "trackId": "no"
        })).unwrap(), VideoControl::SelectSubtitleTrack { track_id } if track_id == "no")
        );
    }
}
