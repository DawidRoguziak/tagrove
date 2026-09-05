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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoControlLabels {
    pub play: String,
    pub pause: String,
    pub mute: String,
    pub unmute: String,
    pub volume: String,
    pub seek: String,
    pub playback_rate: String,
    pub fullscreen: String,
    pub exit_fullscreen: String,
}

impl VideoControlLabels {
    fn validate(mut self) -> Result<Self, String> {
        for label in [
            &mut self.play,
            &mut self.pause,
            &mut self.mute,
            &mut self.unmute,
            &mut self.volume,
            &mut self.seek,
            &mut self.playback_rate,
            &mut self.fullscreen,
            &mut self.exit_fullscreen,
        ] {
            let trimmed = label.trim();
            if trimmed.is_empty()
                || trimmed.chars().count() > 120
                || trimmed.chars().any(char::is_control)
            {
                return Err("video control labels must contain 1-120 visible characters".into());
            }
            *label = trimmed.to_string();
        }
        Ok(self)
    }
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
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
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
    app: State<'_, crate::app::state::AppState>,
) -> Result<u64, String> {
    if asset_id <= 0 {
        return Err("asset id must be positive".to_string());
    }
    let bounds = bounds.validate()?;
    let control_labels = control_labels.validate()?;
    player.require_pending(request_id)?;
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
    player.require_pending(request_id)?;
    crate::video_surface::ensure_ready(&window)?;
    let worker = player.inner().clone();
    let session_id =
        tauri::async_runtime::spawn_blocking(move || worker.open(request_id, source, on_event))
            .await
            .map_err(|error| format!("video worker failed: {error}"))??;
    if let Err(error) =
        crate::video_surface::activate(&window, &player, session_id, bounds, control_labels)
    {
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
    crate::video_surface::set_bounds(&window, &player, session_id, bounds.validate()?)
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
            return crate::video_surface::set_fullscreen(
                &window,
                &player,
                session_id,
                crate::video_surface::FullscreenAction::Set(fullscreen),
            );
        }
        VideoControl::ToggleFullscreen => {
            return crate::video_surface::set_fullscreen(
                &window,
                &player,
                session_id,
                crate::video_surface::FullscreenAction::Toggle,
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
    let hide_result = crate::video_surface::hide(&window, session_id);
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
        let hide = crate::video_surface::hide(&window, session_id);
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
    crate::video_surface::set_control_labels(
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

    #[test]
    fn validates_and_trims_native_control_labels() {
        let labels = VideoControlLabels {
            play: " Play ".into(),
            pause: "Pause".into(),
            mute: "Mute".into(),
            unmute: "Unmute".into(),
            volume: "Volume".into(),
            seek: "Seek".into(),
            playback_rate: "Playback speed".into(),
            fullscreen: "Fullscreen".into(),
            exit_fullscreen: "Exit fullscreen".into(),
        }
        .validate()
        .expect("valid labels");
        assert_eq!(labels.play, "Play");

        let mut invalid = labels;
        invalid.seek = "\n".into();
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn parses_camel_case_track_control_fields() {
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
