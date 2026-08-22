use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Mutex, RwLock,
    },
};

use crate::services::thumb_scheduler::ThumbnailScheduler;

pub struct AppState {
    pub db_path: PathBuf,
    pub thumbs_dir: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub scan_lock: Mutex<()>,
    pub thumb_lock: RwLock<()>,
    pub thumb_scheduler: ThumbnailScheduler,
    pub thumbnail_render_all_running: AtomicBool,
    pub thumbnail_render_all_cancel_requested: AtomicBool,
    /// Monotonic epoch guarding thumbnail publication. Cleared-thumbnail
    /// workflows bump it so results rendered before the reset are dropped
    /// instead of being written back to SQLite.
    pub thumbnail_generation: AtomicU64,
    /// Highest frontend request id seen by the streaming endpoint. Older
    /// request ids belong to abandoned generations and are rejected.
    pub thumbnail_latest_request_id: AtomicU64,
}
