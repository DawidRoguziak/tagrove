use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::video_events::{NativeEvent, VideoEventClient};
use libmpv2::Mpv;
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
    PointerActivity,
    Loading,
    Metadata {
        duration: f64,
        width: i64,
        height: i64,
    },
    Snapshot {
        state: PlaybackSnapshot,
    },
    Fullscreen {
        fullscreen: bool,
    },
    Error {
        message: String,
    },
    ControlError {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct PlaybackSnapshot {
    pub session_id: u64,
    pub duration: f64,
    pub current_time: f64,
    pub paused: bool,
    pub seeking: bool,
    pub buffering: bool,
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
            seeking: false,
            buffering: false,
            volume: 1.0,
            muted: false,
            rate: 1.0,
            fullscreen: false,
        }
    }

    fn refresh(&mut self, mpv: &Mpv) {
        self.duration = mpv.get_property("duration").unwrap_or(0.0);
        self.current_time = mpv.get_property("time-pos").unwrap_or(0.0);
        self.paused = mpv.get_property("pause").unwrap_or(true);
        self.seeking = mpv.get_property("seeking").unwrap_or(false);
        self.buffering = mpv.get_property("paused-for-cache").unwrap_or(false);
        self.volume = mpv.get_property::<f64>("volume").unwrap_or(100.0) / 100.0;
        self.muted = mpv.get_property("mute").unwrap_or(false);
        self.rate = mpv.get_property("speed").unwrap_or(1.0);
    }
}

struct ActiveSession {
    request_id: u64,
    events: Channel<VideoEvent>,
    playback: PlaybackSnapshot,
}

#[derive(Default)]
struct PlayerState {
    next_request_id: u64,
    pending: Option<u64>,
    next_session_id: u64,
    active: Option<ActiveSession>,
}

#[derive(Clone)]
pub struct VideoPlayerService {
    inner: Arc<Inner>,
}

struct Inner {
    // The GTK render context borrows this handle for the application lifetime.
    mpv: Result<&'static Mpv, String>,
    worker: mpsc::Sender<WorkerCommand>,
    state: Mutex<PlayerState>,
}

enum WorkerCommand {
    #[cfg(test)]
    Shutdown(mpsc::SyncSender<()>),
    Open {
        request_id: u64,
        path: String,
        events: Channel<VideoEvent>,
        reply: mpsc::SyncSender<Result<u64, String>>,
    },
    Control {
        session_id: u64,
        command: PlayerCommand,
    },
    Close {
        session_id: u64,
    },
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

struct PlaybackWorker {
    service: VideoPlayerService,
    mpv: &'static Mpv,
    session: Option<(u64, VideoEventClient)>,
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
        Self::with_mpv(mpv)
    }

    fn with_mpv(mpv: Result<&'static Mpv, String>) -> Self {
        let (tx, rx) = mpsc::channel();
        let service = Self {
            inner: Arc::new(Inner {
                mpv,
                worker: tx,
                state: Mutex::new(PlayerState::default()),
            }),
        };
        if let Ok(mpv) = service.mpv() {
            let worker_service = service.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("libmpv-playback".into())
                .spawn(move || {
                    PlaybackWorker {
                        service: worker_service,
                        mpv,
                        session: None,
                    }
                    .run(rx);
                })
            {
                eprintln!("libmpv worker failed to start: {error}");
            }
        }
        service
    }

    pub fn mpv(&self) -> Result<&'static Mpv, String> {
        self.inner.mpv.clone()
    }

    pub fn begin_open(&self) -> u64 {
        let mut state = self.state();
        state.next_request_id += 1;
        state.pending = Some(state.next_request_id);
        state.next_request_id
    }

    pub fn require_pending(&self, request_id: u64) -> Result<(), String> {
        if self.state().pending == Some(request_id) {
            Ok(())
        } else {
            Err("video open was cancelled or superseded".into())
        }
    }

    // Cancellation also finds a session committed by the worker before open_video returned.
    pub fn cancel_open(&self, request_id: u64) -> Option<u64> {
        let session_id = {
            let mut state = self.state();
            if state.pending == Some(request_id) {
                state.pending = None;
            }
            if state
                .active
                .as_ref()
                .is_some_and(|active| active.request_id == request_id)
            {
                state.active.take().map(|active| active.playback.session_id)
            } else {
                None
            }
        };
        if let Some(session_id) = session_id {
            if let Err(error) = self.enqueue(WorkerCommand::Close { session_id }) {
                eprintln!("{error}");
            }
        }
        session_id
    }

    pub fn open(
        &self,
        request_id: u64,
        path: String,
        events: Channel<VideoEvent>,
    ) -> Result<u64, String> {
        self.mpv()?;
        self.require_pending(request_id)?;
        let (tx, rx) = mpsc::sync_channel(1);
        self.enqueue(WorkerCommand::Open {
            request_id,
            path,
            events,
            reply: tx,
        })?;
        rx.recv().map_err(|_| "libmpv worker stopped".to_string())?
    }

    // GTK callbacks enqueue commands; they never block the render thread on libmpv.
    pub fn control(&self, session_id: u64, command: PlayerCommand) -> Result<(), String> {
        self.require_current(session_id)?;
        self.enqueue(WorkerCommand::Control {
            session_id,
            command,
        })
    }

    pub fn close(&self, session_id: u64) -> Result<(), String> {
        if self.retire(session_id) {
            self.enqueue(WorkerCommand::Close { session_id })
        } else {
            Ok(())
        } // Closing a retired session is intentionally idempotent.
    }

    fn retire(&self, session_id: u64) -> bool {
        let mut state = self.state();
        if state
            .active
            .as_ref()
            .is_none_or(|active| active.playback.session_id != session_id)
        {
            return false;
        }
        let active = state.active.take().expect("current session");
        if state.pending == Some(active.request_id) {
            state.pending = None;
        }
        true
    }

    pub fn require_current(&self, session_id: u64) -> Result<(), String> {
        self.with_session(session_id, |_| Ok(()))
    }

    // GTK calls this *inside* its main-thread task. Check and side effect share one lock.
    pub(crate) fn with_session<T>(
        &self,
        session_id: u64,
        operation: impl FnOnce(&mut PlaybackSnapshot) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut state = self.state();
        let active = state
            .active
            .as_mut()
            .filter(|active| active.playback.session_id == session_id)
            .ok_or_else(|| "stale video session".to_string())?;
        operation(&mut active.playback)
    }

    pub(crate) fn playback_snapshot(&self) -> Option<PlaybackSnapshot> {
        self.state().active.as_ref().map(|active| active.playback)
    }

    pub(crate) fn pointer_activity(&self, session_id: u64) {
        self.send(session_id, VideoEventPayload::PointerActivity);
    }

    pub(crate) fn send_fullscreen(&self, session_id: u64, fullscreen: bool) {
        self.send(session_id, VideoEventPayload::Fullscreen { fullscreen });
    }

    pub(crate) fn report_control_error(&self, session_id: u64, message: String) {
        self.send(session_id, VideoEventPayload::ControlError { message });
    }

    pub(crate) fn fail(&self, session_id: u64, message: String) {
        self.send(session_id, VideoEventPayload::Error { message });
        let _ = self.close(session_id);
    }

    fn send(&self, session_id: u64, payload: VideoEventPayload) {
        // Never call a channel while holding the state lock.
        let events = self
            .state()
            .active
            .as_ref()
            .filter(|active| active.playback.session_id == session_id)
            .map(|active| active.events.clone());
        if let Some(events) = events {
            let _ = events.send(VideoEvent {
                session_id,
                payload,
            });
        }
    }

    fn publish_snapshot(&self, session_id: u64, mpv: &Mpv) {
        let Some(mut snapshot) = self
            .playback_snapshot()
            .filter(|s| s.session_id == session_id)
        else {
            return;
        };
        // Property reads may wait for the core; never hold the lock used by GTK while reading.
        snapshot.refresh(mpv);
        let changed = self.with_session(session_id, |current| {
            snapshot.fullscreen = current.fullscreen;
            let changed = *current != snapshot;
            *current = snapshot;
            Ok(changed)
        });
        if changed == Ok(true) {
            self.send(session_id, VideoEventPayload::Snapshot { state: snapshot });
        }
    }

    fn enqueue(&self, command: WorkerCommand) -> Result<(), String> {
        self.inner
            .worker
            .send(command)
            .map_err(|_| "libmpv worker is unavailable".into())
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

impl PlaybackWorker {
    fn run(&mut self, commands: mpsc::Receiver<WorkerCommand>) {
        loop {
            let command = if self.session.is_none() {
                commands
                    .recv()
                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
            } else {
                commands.recv_timeout(Duration::from_millis(10))
            };
            match command {
                #[cfg(test)]
                Ok(WorkerCommand::Shutdown(done)) => {
                    let _ = self.stop();
                    let _ = done.send(());
                    break;
                }
                Ok(WorkerCommand::Open {
                    request_id,
                    path,
                    events,
                    reply,
                }) => {
                    let result = self.open(request_id, &path, events);
                    let _ = reply.send(result);
                }
                Ok(WorkerCommand::Control {
                    session_id,
                    command,
                }) => {
                    if self.service.require_current(session_id).is_ok() {
                        if let Err(error) = self.control(command) {
                            self.service.report_control_error(session_id, error);
                        }
                        self.service.publish_snapshot(session_id, self.mpv);
                    }
                }
                Ok(WorkerCommand::Close { session_id }) => {
                    if self
                        .session
                        .as_ref()
                        .is_some_and(|(id, _)| *id == session_id)
                    {
                        if let Err(error) = self.stop() {
                            eprintln!("libmpv stop failed: {error}");
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            self.drain_events();
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        self.mpv
            .command("stop", &[])
            .map_err(|error| error.to_string())?;
        // Stop completion is a boundary: old decoder events cannot enter the next client's queue.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self
            .mpv
            .get_property::<bool>("idle-active")
            .unwrap_or(false)
        {
            if Instant::now() >= deadline {
                return Err("libmpv stop timed out".into());
            }
            if let Some((_, client)) = &self.session {
                let _ = client.wait_event(0.01);
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        if let Some((session_id, client)) = self.session.take() {
            // Retire before attempting the next client: even client creation failure
            // must not leave controls attached to the decoder we just stopped.
            self.service.retire(session_id);
            while client.wait_event(0.0).is_some() {}
        }
        Ok(())
    }

    fn open(
        &mut self,
        request_id: u64,
        path: &str,
        events: Channel<VideoEvent>,
    ) -> Result<u64, String> {
        self.service.require_pending(request_id)?;
        self.stop()?;
        self.service.require_pending(request_id)?;
        let client = VideoEventClient::new(self.mpv)?;
        let session_id = {
            let mut state = self.service.state();
            if state.pending != Some(request_id) {
                return Err("video open was cancelled or superseded".into());
            }
            state.next_session_id += 1;
            let session_id = state.next_session_id;
            state.active = Some(ActiveSession {
                request_id,
                events,
                playback: PlaybackSnapshot::new(session_id),
            });
            session_id
        };
        self.session = Some((session_id, client));
        self.service.send(session_id, VideoEventPayload::Loading);
        self.load_committed(request_id, session_id, path)?;
        Ok(session_id)
    }

    fn load_committed(
        &mut self,
        request_id: u64,
        session_id: u64,
        path: &str,
    ) -> Result<(), String> {
        let result = {
            // Cancellation cannot acknowledge success between this check and loadfile.
            let state = self.service.state();
            if state.pending != Some(request_id)
                || state
                    .active
                    .as_ref()
                    .is_none_or(|active| active.playback.session_id != session_id)
            {
                Err("video open was cancelled or superseded".to_string())
            } else {
                let (_, client) = self
                    .session
                    .as_ref()
                    .ok_or("video event client is unavailable")?;
                client
                    .command_async(1, &["set", "pause", "no"])
                    .and_then(|()| client.command_async(2, &["loadfile", path, "replace"]))
                    .map_err(|error| format!("libmpv could not open the video: {error}"))
            }
        };
        if let Err(error) = result {
            let _ = self.service.close(session_id);
            self.stop()?;
            return Err(error);
        }
        Ok(())
    }

    fn control(&self, command: PlayerCommand) -> Result<(), String> {
        match command {
            PlayerCommand::Play => self.mpv.set_property("pause", false),
            PlayerCommand::Pause => self.mpv.set_property("pause", true),
            PlayerCommand::Seek(time) => {
                let duration = self.mpv.get_property::<f64>("duration").unwrap_or(0.0);
                if duration <= 0.0 {
                    return Err("video duration is unavailable".into());
                }
                self.mpv.set_property("time-pos", time.clamp(0.0, duration))
            }
            PlayerCommand::SetVolume(volume) => self.mpv.set_property("volume", volume * 100.0),
            PlayerCommand::SetMuted(muted) => self.mpv.set_property("mute", muted),
            PlayerCommand::SetRate(rate) => self.mpv.set_property("speed", rate),
            PlayerCommand::SelectAudioTrack(track) => self.mpv.set_property("aid", track),
            PlayerCommand::SelectSubtitleTrack(track) => self.mpv.set_property("sid", track),
        }
        .map_err(|error| format!("libmpv control failed: {error}"))
    }

    fn drain_events(&self) {
        let Some((session_id, client)) = &self.session else {
            return;
        };
        let mut snapshot_changed = false;
        // Bound each turn so command processing remains responsive under frequent time updates.
        for _ in 0..64 {
            let Some(event) = client.wait_event(0.0) else {
                break;
            };
            if self.service.require_current(*session_id).is_err() {
                continue;
            }
            match event {
                NativeEvent::Metadata => {
                    self.service.send(
                        *session_id,
                        VideoEventPayload::Metadata {
                            duration: self.mpv.get_property("duration").unwrap_or(0.0),
                            width: self.mpv.get_property("width").unwrap_or(0),
                            height: self.mpv.get_property("height").unwrap_or(0),
                        },
                    );
                    snapshot_changed = true;
                }
                NativeEvent::StateChanged => snapshot_changed = true,
                NativeEvent::Error(error) => self.service.fail(*session_id, error),
                NativeEvent::Other => {}
            }
        }
        if snapshot_changed {
            self.service.publish_snapshot(*session_id, self.mpv);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> VideoPlayerService {
        VideoPlayerService::with_mpv(Err("test player".into()))
    }
    fn activate(service: &VideoPlayerService, request_id: u64, session_id: u64) {
        service.state().active = Some(ActiveSession {
            request_id,
            events: Channel::new(|_| Ok(())),
            playback: PlaybackSnapshot::new(session_id),
        });
    }

    #[test]
    fn cancellation_covers_pending_and_committed_opens_without_touching_a_replacement() {
        let service = service();
        let first = service.begin_open();
        assert_eq!(service.cancel_open(first), None);
        assert!(service.require_pending(first).is_err());
        let second = service.begin_open();
        activate(&service, second, 7);
        assert_eq!(service.cancel_open(first), None);
        assert!(service.require_pending(second).is_ok());
        assert_eq!(service.cancel_open(second), Some(7));
        assert!(service.require_pending(second).is_err());
    }

    #[test]
    fn stale_closes_and_surface_operations_do_not_change_the_current_session() {
        let service = service();
        activate(&service, 2, 7);
        assert!(service.close(6).is_ok());
        assert!(service
            .with_session(6, |state| {
                state.fullscreen = true;
                Ok(())
            })
            .is_err());
        assert!(!service.playback_snapshot().unwrap().fullscreen);
    }

    #[test]
    fn snapshot_serialization_keeps_pause_seek_and_buffering_independent() {
        let event = VideoEvent {
            session_id: 7,
            payload: VideoEventPayload::Snapshot {
                state: PlaybackSnapshot {
                    seeking: true,
                    buffering: true,
                    ..PlaybackSnapshot::new(7)
                },
            },
        };
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["type"], "snapshot");
        assert_eq!(value["session_id"], 7);
        assert_eq!(value["state"]["paused"], true);
        assert_eq!(value["state"]["seeking"], true);
        assert_eq!(value["state"]["buffering"], true);
    }
    #[test]
    fn supersession_after_commit_retires_the_session_without_cancelling_the_new_request() {
        let mpv = Box::leak(Box::new(
            Mpv::with_initializer(|init| {
                init.set_option("vo", "null")?;
                init.set_option("ao", "null")?;
                Ok(())
            })
            .unwrap(),
        ));
        let service = service();
        let request = service.begin_open();
        activate(&service, request, 7);
        let client = VideoEventClient::new(mpv).unwrap();
        let mut worker = PlaybackWorker {
            service: service.clone(),
            mpv,
            session: Some((7, client)),
        };
        let replacement = service.begin_open();
        assert!(worker
            .load_committed(request, 7, "must-not-open.mp4")
            .is_err());
        assert!(service.playback_snapshot().is_none());
        assert!(worker.session.is_none());
        assert!(service.require_pending(replacement).is_ok());
        assert!(mpv.get_property::<bool>("idle-active").unwrap());

        activate(&service, replacement, 8);
        worker.session = Some((8, VideoEventClient::new(mpv).unwrap()));
        let next_request = service.begin_open();
        worker.stop().unwrap();
        // This invariant holds before the next fallible event-client allocation.
        assert!(service.playback_snapshot().is_none());
        assert!(service.require_current(8).is_err());
        assert!(service.require_pending(next_request).is_ok());
    }

    #[test]
    fn real_mpv_preserves_paused_seeks_and_preferences_across_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("clip.mp4");
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x90:rate=10",
                "-t",
                "3",
                "-c:v",
                "mpeg4",
            ])
            .arg(&path)
            .output()
            .expect("ffmpeg is required for native playback tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mpv = Mpv::with_initializer(|init| {
            init.set_option("vo", "null")?;
            init.set_option("ao", "null")?;
            init.set_option("loop-file", "inf")?;
            Ok(())
        })
        .unwrap();
        let service = VideoPlayerService::with_mpv(Ok(Box::leak(Box::new(mpv))));
        struct StopWorker(VideoPlayerService);
        impl Drop for StopWorker {
            fn drop(&mut self) {
                let (done, stopped) = mpsc::sync_channel(1);
                if self.0.enqueue(WorkerCommand::Shutdown(done)).is_ok() {
                    stopped
                        .recv_timeout(Duration::from_secs(6))
                        .expect("playback worker cleanup");
                }
            }
        }
        let _stop = StopWorker(service.clone());
        let wait = |predicate: &dyn Fn(PlaybackSnapshot) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(snapshot) = service.playback_snapshot() {
                    if predicate(snapshot) {
                        return snapshot;
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "playback did not reach expected state: {:?}",
                    service.playback_snapshot()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let first_request = service.begin_open();
        let first = service
            .open(
                first_request,
                path.to_str().unwrap().into(),
                Channel::new(|_| Ok(())),
            )
            .unwrap();
        wait(&|s| s.duration > 2.0 && !s.paused && s.current_time > 0.0);
        service.control(first, PlayerCommand::Pause).unwrap();
        service
            .control(first, PlayerCommand::SetMuted(true))
            .unwrap();
        service
            .control(first, PlayerCommand::SetVolume(0.4))
            .unwrap();
        service.control(first, PlayerCommand::SetRate(1.5)).unwrap();
        wait(&|s| s.paused && s.muted && s.volume == 0.4 && s.rate == 1.5);
        service.control(first, PlayerCommand::Seek(1.0)).unwrap();
        wait(&|s| s.paused && !s.seeking && s.current_time >= 0.9);
        std::thread::sleep(Duration::from_millis(100));
        assert!(service.playback_snapshot().unwrap().paused);

        let second_request = service.begin_open();
        let second = service
            .open(
                second_request,
                path.to_str().unwrap().into(),
                Channel::new(|_| Ok(())),
            )
            .unwrap();
        assert_ne!(first, second);
        let snapshot = wait(&|s| {
            s.session_id == second && s.duration > 2.0 && !s.paused && s.current_time > 0.0
        });
        assert!(snapshot.current_time < 1.0);
        assert!(snapshot.muted);
        assert_eq!(snapshot.volume, 0.4);
        assert_eq!(snapshot.rate, 1.5);
        assert!(service.control(first, PlayerCommand::Pause).is_err());
        service.close(first).unwrap();
        assert_eq!(service.playback_snapshot().unwrap().session_id, second);
        service.control(second, PlayerCommand::Seek(2.8)).unwrap();
        wait(&|s| s.current_time > 2.5);
        wait(&|s| s.current_time < 1.0 && !s.paused);
        service.close(second).unwrap();
        assert!(service.playback_snapshot().is_none());
        let corrupt = temp.path().join("broken.mp4");
        std::fs::write(&corrupt, b"invalid media bytes").unwrap();
        let (error_tx, error_rx) = mpsc::channel();
        let errors = Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = body {
                let value: serde_json::Value = serde_json::from_str(&json).unwrap();
                if value["type"] == "error" {
                    let _ = error_tx.send(value);
                }
            }
            Ok(())
        });
        let broken_request = service.begin_open();
        let broken_session = service
            .open(broken_request, corrupt.to_str().unwrap().into(), errors)
            .unwrap();
        let error = error_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("decoder failure event");
        assert_eq!(error["session_id"], broken_session);
        let deadline = Instant::now() + Duration::from_secs(5);
        while service.playback_snapshot().is_some() {
            assert!(Instant::now() < deadline, "failed session was not retired");
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::copy(&path, &corrupt).unwrap();
        let retry_request = service.begin_open();
        let retry = service
            .open(
                retry_request,
                corrupt.to_str().unwrap().into(),
                Channel::new(|_| Ok(())),
            )
            .unwrap();
        wait(&|s| s.session_id == retry && s.duration > 2.0 && !s.paused);
        service.fail(broken_session, "late retired error".into());
        assert_eq!(service.playback_snapshot().unwrap().session_id, retry);
        service.close(retry).unwrap();
        let cancelled = service.begin_open();
        service.cancel_open(cancelled);
        assert!(service
            .open(
                cancelled,
                path.to_str().unwrap().into(),
                Channel::new(|_| Ok(()))
            )
            .is_err());
    }
}
