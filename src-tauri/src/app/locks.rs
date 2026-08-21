use tauri::State;

use crate::{
    app::state::AppState,
    db,
    error::AppResult,
    services::{asset_query_service, db_pool},
};

pub fn with_scan_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _database_guard = db::database_access_guard()?;
    let _guard = state.scan_lock.lock().map_err(|e| e.to_string())?;
    action()
}

pub fn with_thumb_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _database_guard = db::database_access_guard()?;
    let _guard = state.thumb_lock.write().map_err(|e| e.to_string())?;
    action()
}

pub fn with_thumb_read_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _database_guard = db::database_access_guard()?;
    let _guard = state.thumb_lock.read().map_err(|e| e.to_string())?;
    action()
}

pub fn with_scan_and_thumb_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _database_guard = db::database_access_guard()?;
    let _scan_guard = state.scan_lock.lock().map_err(|e| e.to_string())?;
    let _thumb_guard = state.thumb_lock.write().map_err(|e| e.to_string())?;
    action()
}

pub fn with_database_maintenance<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let maintenance_guard = db::begin_database_maintenance()?;
    db_pool::invalidate(&state.db_path);
    asset_query_service::manager().clear();
    maintenance_guard.wait_for_connections()?;

    let _scan_guard = state.scan_lock.lock().map_err(|e| e.to_string())?;
    let _thumb_guard = state.thumb_lock.write().map_err(|e| e.to_string())?;
    action()
}
