use tauri::State;

use crate::{app::state::AppState, error::AppResult};

pub fn with_scan_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _guard = state.scan_lock.lock().map_err(|e| e.to_string())?;
    action()
}

pub fn with_thumb_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _guard = state.thumb_lock.write().map_err(|e| e.to_string())?;
    action()
}

pub fn with_thumb_read_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _guard = state.thumb_lock.read().map_err(|e| e.to_string())?;
    action()
}

pub fn with_scan_and_thumb_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _scan_guard = state.scan_lock.lock().map_err(|e| e.to_string())?;
    let _thumb_guard = state.thumb_lock.write().map_err(|e| e.to_string())?;
    action()
}
