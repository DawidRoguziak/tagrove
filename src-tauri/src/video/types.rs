//! Inputs of the native video commands that the GTK surface also uses:
//! `VideoBounds` (`open_video`, `set_video_bounds`) and `VideoControlLabels`
//! (`open_video`, `set_video_control_labels`). The serde shape is frozen by
//! `dto_snapshot` in `tests/architecture_boundaries.rs`.

use serde::Deserialize;

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
    pub(crate) fn validate(mut self) -> Result<Self, String> {
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
}
