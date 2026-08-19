use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Mutex, RwLock},
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
}
