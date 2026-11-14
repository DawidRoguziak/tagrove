use std::sync::{PoisonError, RwLockReadGuard, RwLockWriteGuard, TryLockError};

use crate::services::db_pool::{MaintenancePermit, OperationPermit};

use crate::{app::state::AppState, error::AppResult};

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

fn recover_thumb_write<'a>(
    result: Result<RwLockWriteGuard<'a, ()>, PoisonError<RwLockWriteGuard<'a, ()>>>,
) -> AppResult<RwLockWriteGuard<'a, ()>> {
    Ok(result.unwrap_or_else(PoisonError::into_inner))
}

pub fn with_scan_lock<T>(
    state: &AppState,
    action: impl FnOnce(&OperationPermit) -> AppResult<T>,
) -> AppResult<T> {
    let permit = state.database.admit()?;
    let _guard = recover_scan_mutex(state.scan_lock.lock())?;
    action(&permit)
}

pub fn with_thumb_lock<T>(
    state: &AppState,
    action: impl FnOnce(&OperationPermit) -> AppResult<T>,
) -> AppResult<T> {
    let permit = state.database.admit()?;
    let _guard = recover_thumb_write(state.thumb_lock.write())?;
    action(&permit)
}

/// Admits a thumbnail render/ensure operation without holding `thumb_lock`.
///
/// The service takes [`thumb_read_section`] around each database read,
/// publication commit and file removal, and never while it waits for the
/// scheduler. A waiting exclusive writer (clear, delete, rename, root
/// removal) therefore waits for at most one short section, and new
/// thumbnail work waits only for that writer, not for a long render.
pub fn with_thumb_operation<T>(
    state: &AppState,
    action: impl FnOnce(&OperationPermit) -> AppResult<T>,
) -> AppResult<T> {
    let permit = state.database.admit()?;
    action(&permit)
}

/// Shared `thumb_lock` guard for one short thumbnail section. Callers already
/// hold an operation permit and no other workflow lock; never nest sections.
pub fn thumb_read_section(state: &AppState) -> RwLockReadGuard<'_, ()> {
    state
        .thumb_lock
        .read()
        .unwrap_or_else(PoisonError::into_inner)
}

pub fn with_scan_and_thumb_lock<T>(
    state: &AppState,
    action: impl FnOnce(&OperationPermit) -> AppResult<T>,
) -> AppResult<T> {
    let permit = state.database.admit()?;
    let _scan_guard = recover_scan_mutex(state.scan_lock.lock())?;
    let _thumb_guard = recover_thumb_write(state.thumb_lock.write())?;
    action(&permit)
}

/// [`with_scan_and_thumb_lock`] for interactive commands that must not wait
/// for a scan: returns `Ok(None)` without running `action` when `scan_lock` is
/// already held. Scans and the other combined workflows hold it for their whole
/// run, while the thumbnail write lock that follows waits only for short read
/// sections, so only the scan mutex is tried. The order is unchanged: the
/// permit, then `scan_lock`, then the exclusive `thumb_lock`.
pub fn try_with_scan_and_thumb_lock<T>(
    state: &AppState,
    action: impl FnOnce(&OperationPermit) -> AppResult<T>,
) -> AppResult<Option<T>> {
    let permit = state.database.admit()?;
    let _scan_guard = match state.scan_lock.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => return Ok(None),
    };
    let _thumb_guard = recover_thumb_write(state.thumb_lock.write())?;
    action(&permit).map(Some)
}

pub fn with_database_maintenance<T>(
    state: &AppState,
    action: impl FnOnce(&MaintenancePermit) -> AppResult<T>,
) -> AppResult<T> {
    let permit = state.database.maintenance()?;
    let _scan_guard = recover_scan_mutex(state.scan_lock.lock())?;
    let _thumb_guard = recover_thumb_write(state.thumb_lock.write())?;
    action(&permit)
}
