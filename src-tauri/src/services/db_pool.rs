use std::{
    collections::HashMap,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};

use rusqlite::Connection;

use crate::{db, error::AppResult};

const MAX_CONNECTIONS: usize = 4;
/// Upper bound for waiting on a pooled connection so callers get a
/// reportable busy failure instead of blocking forever.
const POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

struct PoolState {
    idle: Vec<db::ManagedConnection>,
    total: usize,
    invalidated: bool,
}

struct DbPool {
    path: PathBuf,
    state: Mutex<PoolState>,
    available: Condvar,
}

pub struct PooledConnection {
    pool: Arc<DbPool>,
    connection: Option<db::ManagedConnection>,
}

impl Deref for PooledConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection.as_ref().expect("pooled connection")
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        let Some(connection) = self.connection.take() else {
            return;
        };
        if let Ok(mut state) = self.pool.state.lock() {
            if state.invalidated {
                state.total = state.total.saturating_sub(1);
            } else {
                state.idle.push(connection);
            }
            self.pool.available.notify_all();
        }
    }
}

pub fn connection(path: &Path) -> AppResult<PooledConnection> {
    let pool = pool_for(path)?;
    let deadline = Instant::now() + POOL_ACQUIRE_TIMEOUT;
    let mut state = pool
        .state
        .lock()
        .map_err(|error| format!("database pool lock error: {error}"))?;
    loop {
        if state.invalidated {
            return Err("database pool was invalidated for maintenance".into());
        }
        if let Some(connection) = state.idle.pop() {
            drop(state);
            return Ok(PooledConnection {
                pool: Arc::clone(&pool),
                connection: Some(connection),
            });
        }
        if state.total < MAX_CONNECTIONS {
            state.total += 1;
            drop(state);
            return match db::open_connection(&pool.path) {
                Ok(connection) => {
                    let mut state = pool
                        .state
                        .lock()
                        .map_err(|error| format!("database pool lock error: {error}"))?;
                    if state.invalidated {
                        state.total = state.total.saturating_sub(1);
                        pool.available.notify_all();
                        drop(state);
                        drop(connection);
                        Err("database pool was invalidated for maintenance".into())
                    } else {
                        drop(state);
                        Ok(PooledConnection {
                            pool: Arc::clone(&pool),
                            connection: Some(connection),
                        })
                    }
                }
                Err(error) => {
                    if let Ok(mut state) = pool.state.lock() {
                        state.total = state.total.saturating_sub(1);
                        pool.available.notify_one();
                    }
                    Err(error.into())
                }
            };
        }
        let Some(timeout) = deadline.checked_duration_since(Instant::now()) else {
            return Err(
                "database pool is busy: timed out waiting for a free connection"
                    .to_string()
                    .into(),
            );
        };
        let (guard, wait_result) = pool
            .available
            .wait_timeout(state, timeout)
            .map_err(|error| format!("database pool wait error: {error}"))?;
        state = guard;
        if wait_result.timed_out() && deadline <= Instant::now() {
            if state.invalidated {
                return Err("database pool was invalidated for maintenance".into());
            }
            return Err(
                "database pool is busy: timed out waiting for a free connection"
                    .to_string()
                    .into(),
            );
        }
    }
}

pub fn invalidate(path: &Path) {
    if let Some(registry) = REGISTRY.get() {
        if let Ok(mut registry) = registry.lock() {
            if let Some(pool) = registry.remove(path) {
                if let Ok(mut state) = pool.state.lock() {
                    state.invalidated = true;
                    let idle_count = state.idle.len();
                    state.total = state.total.saturating_sub(idle_count);
                    state.idle.clear();
                    pool.available.notify_all();
                }
            }
        }
    }
}

fn pool_for(path: &Path) -> AppResult<Arc<DbPool>> {
    let registry = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry
        .lock()
        .map_err(|error| format!("database pool registry error: {error}"))?;
    Ok(registry
        .entry(path.to_path_buf())
        .or_insert_with(|| {
            Arc::new(DbPool {
                path: path.to_path_buf(),
                state: Mutex::new(PoolState {
                    idle: Vec::new(),
                    total: 0,
                    invalidated: false,
                }),
                available: Condvar::new(),
            })
        })
        .clone())
}

static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Arc<DbPool>>>> = OnceLock::new();
