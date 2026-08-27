use std::sync::{Arc, Mutex, MutexGuard};

use libmpv2::{events::Event as MpvEvent, Format, Mpv};
use serde::Serialize;
use tauri::ipc::Channel;

#[derive(Debug, Clone, Serialize)]
pub struct VideoEvent {
    pub session_id: u64,
    #[serde(flatten)]
    pub payload: VideoEventPayload,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VideoEventPayload {
    Loading,
    Metadata {
        duration: f64,
        width: i64,
        height: i64,
    },
    Playing,
    Paused,
    Waiting,
    Time {
        current_time: f64,
    },
    Volume {
        volume: f64,
        muted: bool,
    },
    Rate {
        rate: f64,
    },
    Tracks,
    Fullscreen {
        fullscreen: bool,
    },
    Ended,
    Error {
        message: String,
    },
}

#[derive(Clone)]
pub struct VideoPlayerService {
    inner: Arc<Inner>,
}

struct Inner {
    mpv: Result<&'static Mpv, String>,
    operation: Mutex<()>,
    state: Mutex<PlayerState>,
}

#[derive(Default)]
struct PlayerState {
    next_open_token: u64,
    next_session_id: u64,
    latest_open_token: u64,
    active: Option<ActiveSession>,
}

struct ActiveSession {
    session_id: u64,
    _generation: u64,
    events: Channel<VideoEvent>,
    playback: PlaybackSnapshot,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PlaybackSnapshot {
    pub session_id: u64,
    pub duration: f64,
    pub current_time: f64,
    pub paused: bool,
    pub volume: f64,
    pub muted: bool,
    pub rate: f64,
    pub fullscreen: bool,
}

impl PlaybackSnapshot {
    fn new(session_id: u64) -> Self {
        Self {
            session_id,
            duration: 0.0,
            current_time: 0.0,
            paused: true,
            volume: 1.0,
            muted: false,
            rate: 1.0,
            fullscreen: false,
        }
    }

    fn apply(&mut self, payload: &VideoEventPayload) {
        match payload {
            VideoEventPayload::Metadata { duration, .. } => self.duration = *duration,
            VideoEventPayload::Playing => self.paused = false,
            VideoEventPayload::Paused | VideoEventPayload::Ended => self.paused = true,
            VideoEventPayload::Time { current_time } => self.current_time = *current_time,
            VideoEventPayload::Volume { volume, muted } => {
                self.volume = *volume;
                self.muted = *muted;
            }
            VideoEventPayload::Rate { rate } => self.rate = *rate,
            VideoEventPayload::Fullscreen { fullscreen } => self.fullscreen = *fullscreen,
            VideoEventPayload::Loading
            | VideoEventPayload::Waiting
            | VideoEventPayload::Tracks
            | VideoEventPayload::Error { .. } => {}
        }
    }
}

impl VideoPlayerService {
    pub fn new() -> Self {
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("vo", "libmpv")?;
            init.set_option("hwdec", "auto-safe")?;
            init.set_option("loop-file", "inf")?;
            init.set_option("volume", "100")?;
            init.set_option("mute", "no")?;
            Ok(())
        })
        .map(|mpv| &*Box::leak(Box::new(mpv)))
        .map_err(|error| format!("libmpv initialization failed: {error}"));
        if let Err(error) = &mpv {
            eprintln!("{error}");
        }

        let service = Self {
            inner: Arc::new(Inner {
                mpv,
                operation: Mutex::new(()),
                state: Mutex::new(PlayerState::default()),
            }),
        };
        service.start_event_thread();
        service
    }

    #[cfg(test)]
    fn unavailable(message: &str) -> Self {
        Self {
            inner: Arc::new(Inner {
                mpv: Err(message.to_string()),
                operation: Mutex::new(()),
                state: Mutex::new(PlayerState::default()),
            }),
        }
    }

    pub fn mpv(&self) -> Result<&'static Mpv, String> {
        self.inner.mpv.clone()
    }

    pub fn begin_open(&self) -> u64 {
        let _operation = self.operation();
        let mut state = self.state();
        state.next_open_token = state.next_open_token.saturating_add(1);
        state.latest_open_token = state.next_open_token;
        state.next_open_token
    }

    pub fn open(
        &self,
        open_token: u64,
        generation: u64,
        path: &str,
        events: Channel<VideoEvent>,
    ) -> Result<u64, String> {
        let _operation = self.operation();
        let mpv = self.mpv()?;
        let session_id = {
            let mut state = self.state();
            if state.latest_open_token != open_token {
                return Err("video open was superseded".to_string());
            }
            state.next_session_id = state.next_session_id.saturating_add(1);
            let session_id = state.next_session_id;
            state.active = Some(ActiveSession {
                session_id,
                _generation: generation,
                events,
                playback: PlaybackSnapshot::new(session_id),
            });
            session_id
        };

        self.send(session_id, VideoEventPayload::Loading);
        if let Err(error) = mpv.command("loadfile", &[path, "replace"]) {
            self.clear_if_current(session_id);
            return Err(format!("libmpv could not open the video: {error}"));
        }
        Ok(session_id)
    }

    pub fn control(&self, session_id: u64, command: PlayerCommand) -> Result<(), String> {
        let _operation = self.operation();
        self.require_current(session_id)?;
        let mpv = self.mpv()?;
        match command {
            PlayerCommand::Play => mpv.set_property("pause", false),
            PlayerCommand::Pause => mpv.set_property("pause", true),
            PlayerCommand::Seek(time) => mpv.set_property("time-pos", time),
            PlayerCommand::SetVolume(volume) => mpv.set_property("volume", volume * 100.0),
            PlayerCommand::SetMuted(muted) => mpv.set_property("mute", muted),
            PlayerCommand::SetRate(rate) => mpv.set_property("speed", rate),
            PlayerCommand::SelectAudioTrack(track) => mpv.set_property("aid", track),
            PlayerCommand::SelectSubtitleTrack(track) => mpv.set_property("sid", track),
        }
        .map_err(|error| format!("libmpv control failed: {error}"))
    }

    pub fn close(&self, session_id: u64) -> Result<(), String> {
        let _operation = self.operation();
        self.require_current(session_id)?;
        let result = if let Ok(mpv) = self.mpv() {
            mpv.command("stop", &[])
                .map_err(|error| format!("libmpv close failed: {error}"))
        } else {
            Ok(())
        };
        self.clear_if_current(session_id);
        result
    }

    pub fn require_current(&self, session_id: u64) -> Result<(), String> {
        let state = self.state();
        match &state.active {
            Some(active) if active.session_id == session_id => Ok(()),
            _ => Err("stale video session".to_string()),
        }
    }

    pub fn fullscreen_for(&self, session_id: u64) -> Result<bool, String> {
        let state = self.state();
        match &state.active {
            Some(active) if active.session_id == session_id => Ok(active.playback.fullscreen),
            _ => Err("stale video session".to_string()),
        }
    }

    pub fn active_fullscreen(&self) -> bool {
        self.state()
            .active
            .as_ref()
            .is_some_and(|active| active.playback.fullscreen)
    }

    pub fn set_fullscreen_state(&self, session_id: u64, fullscreen: bool) -> Result<(), String> {
        let mut state = self.state();
        match &mut state.active {
            Some(active) if active.session_id == session_id => {
                active.playback.fullscreen = fullscreen;
                Ok(())
            }
            _ => Err("stale video session".to_string()),
        }
    }

    pub fn send_fullscreen(&self, session_id: u64, fullscreen: bool) {
        self.send(session_id, VideoEventPayload::Fullscreen { fullscreen });
    }

    pub(crate) fn playback_snapshot(&self) -> Option<PlaybackSnapshot> {
        self.state().active.as_ref().map(|active| active.playback)
    }

    fn start_event_thread(&self) {
        let Ok(mpv) = self.mpv() else {
            return;
        };
        let Ok(event_client) = mpv.create_client(Some("media_tagger_events")) else {
            return;
        };
        for (id, name, format) in [
            (1, "time-pos", Format::Double),
            (2, "pause", Format::Flag),
            (3, "paused-for-cache", Format::Flag),
            (4, "volume", Format::Double),
            (5, "mute", Format::Flag),
            (6, "speed", Format::Double),
        ] {
            let _ = event_client.observe_property(name, format, id);
        }

        let service = self.clone();
        let _ = std::thread::Builder::new()
            .name("libmpv-events".to_string())
            .spawn(move || loop {
                let Some(event) = event_client.wait_event(-1.0) else {
                    continue;
                };
                match event {
                    Ok(MpvEvent::Shutdown) => break,
                    Ok(event) => service.handle_event(&event_client, event),
                    Err(error) => {
                        eprintln!("libmpv playback event failed: {error}");
                        service.send_current(VideoEventPayload::Error {
                            message: error.to_string(),
                        });
                    }
                }
            });
    }

    fn handle_event(&self, mpv: &Mpv, event: MpvEvent<'_>) {
        use libmpv2::events::PropertyData;

        match event {
            MpvEvent::StartFile => self.send_current(VideoEventPayload::Loading),
            MpvEvent::FileLoaded | MpvEvent::VideoReconfig => {
                self.send_current(VideoEventPayload::Metadata {
                    duration: mpv.get_property("duration").unwrap_or(0.0),
                    width: mpv.get_property("width").unwrap_or(0),
                    height: mpv.get_property("height").unwrap_or(0),
                });
                self.send_current(VideoEventPayload::Tracks);
            }
            MpvEvent::PlaybackRestart => self.send_current(VideoEventPayload::Playing),
            MpvEvent::Seek => self.send_current(VideoEventPayload::Waiting),
            MpvEvent::EndFile(reason) if reason == libmpv2::mpv_end_file_reason::Eof => {
                self.send_current(VideoEventPayload::Ended)
            }
            MpvEvent::PropertyChange { name, change, .. } => match (name, change) {
                ("time-pos", PropertyData::Double(current_time)) => {
                    self.send_current(VideoEventPayload::Time { current_time })
                }
                ("pause", PropertyData::Flag(true)) => self.send_current(VideoEventPayload::Paused),
                ("pause", PropertyData::Flag(false)) => {
                    self.send_current(VideoEventPayload::Playing)
                }
                ("paused-for-cache", PropertyData::Flag(true)) => {
                    self.send_current(VideoEventPayload::Waiting)
                }
                ("volume", PropertyData::Double(volume)) => {
                    let muted = mpv.get_property("mute").unwrap_or(false);
                    self.send_current(VideoEventPayload::Volume {
                        volume: volume / 100.0,
                        muted,
                    });
                }
                ("mute", PropertyData::Flag(muted)) => {
                    let volume = mpv.get_property::<f64>("volume").unwrap_or(100.0) / 100.0;
                    self.send_current(VideoEventPayload::Volume { volume, muted });
                }
                ("speed", PropertyData::Double(rate)) => {
                    self.send_current(VideoEventPayload::Rate { rate })
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn send_current(&self, payload: VideoEventPayload) {
        let mut state = self.state();
        if let Some(active) = &mut state.active {
            active.playback.apply(&payload);
            let _ = active.events.send(VideoEvent {
                session_id: active.session_id,
                payload,
            });
        }
    }

    fn send(&self, session_id: u64, payload: VideoEventPayload) {
        let mut state = self.state();
        if let Some(active) = &mut state.active {
            if active.session_id == session_id {
                active.playback.apply(&payload);
                let _ = active.events.send(VideoEvent {
                    session_id,
                    payload,
                });
            }
        }
    }

    fn clear_if_current(&self, session_id: u64) {
        let mut state = self.state();
        if state
            .active
            .as_ref()
            .is_some_and(|active| active.session_id == session_id)
        {
            state.active = None;
        }
    }

    fn operation(&self) -> MutexGuard<'_, ()> {
        self.inner
            .operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn state(&self) -> MutexGuard<'_, PlayerState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for VideoPlayerService {
    fn default() -> Self {
        Self::new()
    }
}

pub enum PlayerCommand {
    Play,
    Pause,
    Seek(f64),
    SetVolume(f64),
    SetMuted(bool),
    SetRate(f64),
    SelectAudioTrack(String),
    SelectSubtitleTrack(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activate_test_session(service: &VideoPlayerService, session_id: u64) {
        service.state().active = Some(ActiveSession {
            session_id,
            _generation: 1,
            events: Channel::new(|_| Ok(())),
            playback: PlaybackSnapshot::new(session_id),
        });
    }

    #[test]
    fn rejects_stale_sessions_and_superseded_opens() {
        let service = VideoPlayerService::unavailable("test player");
        let first = service.begin_open();
        let second = service.begin_open();
        assert_ne!(first, second);
        assert_eq!(
            service.require_current(1),
            Err("stale video session".into())
        );
    }

    #[test]
    fn tracks_fullscreen_only_for_the_active_session() {
        let service = VideoPlayerService::unavailable("test player");
        activate_test_session(&service, 7);

        assert_eq!(service.fullscreen_for(7), Ok(false));
        assert_eq!(service.set_fullscreen_state(7, true), Ok(()));
        assert_eq!(service.fullscreen_for(7), Ok(true));
        assert!(service.active_fullscreen());
        assert_eq!(
            service.set_fullscreen_state(8, false),
            Err("stale video session".into())
        );
        assert_eq!(service.fullscreen_for(7), Ok(true));

        service.clear_if_current(7);
        assert!(!service.active_fullscreen());
        assert_eq!(service.fullscreen_for(7), Err("stale video session".into()));
    }

    #[test]
    fn keeps_a_native_control_snapshot_in_sync_with_player_events() {
        let service = VideoPlayerService::unavailable("test player");
        activate_test_session(&service, 7);

        service.send(
            7,
            VideoEventPayload::Metadata {
                duration: 42.0,
                width: 1280,
                height: 720,
            },
        );
        service.send(7, VideoEventPayload::Playing);
        service.send(7, VideoEventPayload::Time { current_time: 12.5 });
        service.send(
            7,
            VideoEventPayload::Volume {
                volume: 0.4,
                muted: true,
            },
        );
        service.send(7, VideoEventPayload::Rate { rate: 1.5 });

        let snapshot = service.playback_snapshot().expect("active snapshot");
        assert_eq!(snapshot.session_id, 7);
        assert_eq!(snapshot.duration, 42.0);
        assert_eq!(snapshot.current_time, 12.5);
        assert!(!snapshot.paused);
        assert_eq!(snapshot.volume, 0.4);
        assert!(snapshot.muted);
        assert_eq!(snapshot.rate, 1.5);
    }
}
