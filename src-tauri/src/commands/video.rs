use anyhow::Context;
use serde::Deserialize;
use tauri::{ipc::Channel, State, WebviewWindow};

use crate::services::{
    video_player_service::{PlayerCommand, VideoEvent, VideoPlayerService},
    video_source_service,
};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl VideoBounds {
    pub fn validate(self) -> Result<Self, String> {
        if [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(|value| value.is_finite() && value >= 0.0)
        {
            Ok(self)
        } else {
            Err("video bounds must be finite and non-negative".to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VideoControl {
    Play,
    Pause,
    Seek { time: f64 },
    SetVolume { volume: f64 },
    SetMuted { muted: bool },
    SetRate { rate: f64 },
    SelectAudioTrack { track_id: String },
    SelectSubtitleTrack { track_id: String },
    SetFullscreen { fullscreen: bool },
}

#[tauri::command]
pub async fn open_video(
    asset_id: i64,
    generation: u64,
    bounds: VideoBounds,
    on_event: Channel<VideoEvent>,
    window: WebviewWindow,
    player: State<'_, VideoPlayerService>,
    app: State<'_, crate::app::state::AppState>,
) -> Result<u64, String> {
    if asset_id <= 0 {
        return Err("asset id must be positive".to_string());
    }
    let bounds = bounds.validate()?;
    let open_token = player.begin_open();
    let db_path = app.db_path.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        video_source_service::resolve_video_path(&db_path, asset_id)
    })
    .await
    .map_err(|error| format!("video source worker failed: {error}"))?
    .map_err(|error| error.to_string())?;
    let source = path
        .to_str()
        .context("video path is not valid UTF-8")
        .map_err(|error| error.to_string())?
        .to_string();
    crate::video_surface::ensure_ready(&window)?;
    let session_id = player.open(open_token, generation, &source, on_event)?;
    if let Err(error) = crate::video_surface::set_bounds(&window, session_id, bounds) {
        let _ = player.close(session_id);
        let _ = crate::video_surface::hide(&window, session_id);
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
    crate::video_surface::set_bounds(&window, session_id, bounds.validate()?)
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
        VideoControl::Seek { time } if time.is_finite() && time >= 0.0 => PlayerCommand::Seek(time),
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
            player.require_current(session_id)?;
            window
                .set_fullscreen(fullscreen)
                .map_err(|error| error.to_string())?;
            player.send_fullscreen(session_id, fullscreen);
            return Ok(());
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
    let hide_result = crate::video_surface::hide(&window, session_id);
    close_result.and(hide_result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bounds() {
        assert!(VideoBounds {
            x: 0.0,
            y: 1.0,
            width: 2.0,
            height: 3.0
        }
        .validate()
        .is_ok());
        for value in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(VideoBounds {
                x: value,
                y: 0.0,
                width: 1.0,
                height: 1.0
            }
            .validate()
            .is_err());
        }
    }
}
