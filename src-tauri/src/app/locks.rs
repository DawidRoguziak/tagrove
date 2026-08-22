use std::sync::{PoisonError, RwLockReadGuard, RwLockWriteGuard};

use tauri::State;

use crate::{
    app::state::AppState,
    db,
    error::AppResult,
    services::{asset_query_service, db_pool},
};

// Lock guards recover from poisoning instead of failing forever: once a
// panicked holder poisoned a mutex, every later command would otherwise be
// rejected for the remaining process lifetime. The protected data stays valid,
// so resuming after a panic is the controlled way to keep serving requests.

fn recover_scan_mutex<'a>(
    result: Result<
        std::sync::MutexGuard<'a, ()>,
        std::sync::PoisonError<std::sync::MutexGuard<'a, ()>>,
    >,
) -> AppResult<std::sync::MutexGuard<'a, ()>> {
    Ok(result.unwrap_or_else(PoisonError::into_inner))
}

fn recover_thumb_read<'a>(
    result: Result<RwLockReadGuard<'a, ()>, PoisonError<RwLockReadGuard<'a, ()>>>,
) -> AppResult<RwLockReadGuard<'a, ()>> {
    Ok(result.unwrap_or_else(PoisonError::into_inner))
}

fn recover_thumb_write<'a>(
    result: Result<RwLockWriteGuard<'a, ()>, PoisonError<RwLockWriteGuard<'a, ()>>>,
) -> AppResult<RwLockWriteGuard<'a, ()>> {
    Ok(result.unwrap_or_else(PoisonError::into_inner))
}

pub fn with_scan_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _database_guard = db::database_access_guard()?;
    let _guard = recover_scan_mutex(state.scan_lock.lock())?;
    action()
}

pub fn with_thumb_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _database_guard = db::database_access_guard()?;
    let _guard = recover_thumb_write(state.thumb_lock.write())?;
    action()
}

pub fn with_thumb_read_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _database_guard = db::database_access_guard()?;
    let _guard = recover_thumb_read(state.thumb_lock.read())?;
    action()
}

pub fn with_scan_and_thumb_lock<T>(
    state: &State<AppState>,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let _database_guard = db::database_access_guard()?;
    let _scan_guard = recover_scan_mutex(state.scan_lock.lock())?;
    let _thumb_guard = recover_thumb_write(state.thumb_lock.write())?;
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

    let _scan_guard = recover_scan_mutex(state.scan_lock.lock())?;
    let _thumb_guard = recover_thumb_write(state.thumb_lock.write())?;
    action()
}
