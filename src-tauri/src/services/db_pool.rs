use crate::{db, error::AppResult, services::asset_query_service::AssetQueryManager};
use rusqlite::Connection;
use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

/// Upper bound on open pooled connections.
const MAX_CONNECTIONS: usize = 8;
/// Checkouts kept free for short user metadata writes (favorite, tags,
/// groups), so readers and background work cannot take every connection.
const USER_WRITE_RESERVE: usize = 2;
const ORDINARY_CHECKOUTS: usize = MAX_CONNECTIONS - USER_WRITE_RESERVE;
const POOL_WAIT: Duration = Duration::from_secs(5);
/// Longest time one background write transaction defers to user writes.
const USER_WRITE_YIELD_LIMIT: Duration = Duration::from_millis(250);

#[derive(Default)]
struct Admission {
    active: usize,
    maintenance: bool,
}

#[derive(Default)]
struct Pool {
    idle: Vec<Connection>,
    total: usize,
}

impl Pool {
    fn checked_out(&self) -> usize {
        self.total - self.idle.len()
    }
}

pub struct DatabaseRuntime {
    path: PathBuf,
    admission: Mutex<Admission>,
    changed: Condvar,
    pool: Mutex<Pool>,
    available: Condvar,
    user_writes: Mutex<usize>,
    user_writes_done: Condvar,
    pub queries: AssetQueryManager,
}

impl DatabaseRuntime {
    pub fn new(path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            path,
            admission: Mutex::new(Admission::default()),
            changed: Condvar::new(),
            pool: Mutex::new(Pool::default()),
            available: Condvar::new(),
            user_writes: Mutex::new(0),
            user_writes_done: Condvar::new(),
            queries: AssetQueryManager::new(),
        })
    }

    pub fn admit(self: &Arc<Self>) -> AppResult<OperationPermit> {
        let mut state = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        while state.maintenance {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.active += 1;
        Ok(OperationPermit(Arc::new(AdmittedOperation {
            runtime: self.clone(),
        })))
    }

    pub fn maintenance(self: &Arc<Self>) -> AppResult<MaintenancePermit> {
        let mut state = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        while state.maintenance {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.maintenance = true;
        while state.active != 0 {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        *self.pool.lock().unwrap_or_else(|e| e.into_inner()) = Pool::default();
        self.queries.clear();
        Ok(MaintenancePermit {
            runtime: self.clone(),
        })
    }

    /// Pooled connections currently checked out (diagnostics and tests).
    pub fn checked_out_connections(&self) -> usize {
        self.pool
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .checked_out()
    }

    fn checkout(&self, limit: usize) -> AppResult<Connection> {
        let deadline = Instant::now() + POOL_WAIT;
        let mut pool = self.pool.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if pool.checked_out() < limit {
                if let Some(connection) = pool.idle.pop() {
                    return Ok(connection);
                }
                if pool.total < MAX_CONNECTIONS {
                    pool.total += 1;
                    drop(pool);
                    return db::open_connection_untracked(&self.path).map_err(|error| {
                        self.pool.lock().unwrap_or_else(|e| e.into_inner()).total -= 1;
                        self.available.notify_all();
                        error.into()
                    });
                }
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("database pool is busy: timed out waiting for a free connection")?;
            pool = self
                .available
                .wait_timeout(pool, remaining)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

struct AdmittedOperation {
    runtime: Arc<DatabaseRuntime>,
}
impl Drop for AdmittedOperation {
    fn drop(&mut self) {
        let mut state = self
            .runtime
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        self.runtime.changed.notify_all();
    }
}

/// Marks one user write as waiting for, or holding, the SQLite write lock.
/// Background writers defer new transactions while any ticket exists.
struct UserWriteTicket(Arc<DatabaseRuntime>);
impl UserWriteTicket {
    fn new(runtime: &Arc<DatabaseRuntime>) -> Self {
        *runtime
            .user_writes
            .lock()
            .unwrap_or_else(|e| e.into_inner()) += 1;
        Self(runtime.clone())
    }
}
impl Drop for UserWriteTicket {
    fn drop(&mut self) {
        let mut waiting = self.0.user_writes.lock().unwrap_or_else(|e| e.into_inner());
        *waiting -= 1;
        if *waiting == 0 {
            self.0.user_writes_done.notify_all();
        }
    }
}

/// Clones share one admission, including connections and child workers.
#[derive(Clone)]
pub struct OperationPermit(Arc<AdmittedOperation>);
impl std::fmt::Debug for OperationPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OperationPermit")
    }
}
impl OperationPermit {
    pub fn connection(&self) -> AppResult<PooledConnection> {
        let connection = self.0.runtime.checkout(ORDINARY_CHECKOUTS)?;
        Ok(PooledConnection {
            connection: Some(connection),
            permit: self.clone(),
            reusable: true,
            user_write: None,
        })
    }

    /// Connection for one short user metadata write. It may use the reserved
    /// checkouts, polls a busy write lock every millisecond, and makes
    /// background writers yield until it is dropped.
    pub fn user_write_connection(&self) -> AppResult<PooledConnection> {
        let runtime = &self.0.runtime;
        let ticket = UserWriteTicket::new(runtime);
        let connection = runtime.checkout(MAX_CONNECTIONS)?;
        let mut pooled = PooledConnection {
            connection: Some(connection),
            permit: self.clone(),
            reusable: true,
            user_write: Some(ticket),
        };
        pooled.busy_handler(Some(db::poll_busy_lock_quickly))?;
        Ok(pooled)
    }

    /// Background writers call this before each write transaction. It waits
    /// while user writes are pending, at most `USER_WRITE_YIELD_LIMIT`, and
    /// returns the time spent waiting.
    pub fn yield_to_user_writes(&self) -> Duration {
        let runtime = &self.0.runtime;
        let waiting = runtime
            .user_writes
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *waiting == 0 {
            return Duration::ZERO;
        }
        let started = Instant::now();
        drop(
            runtime
                .user_writes_done
                .wait_timeout_while(waiting, USER_WRITE_YIELD_LIMIT, |waiting| *waiting > 0)
                .unwrap_or_else(|e| e.into_inner()),
        );
        started.elapsed()
    }

    /// Source-file journals need power-loss durability and must not alter pooled settings.
    pub fn durable_connection(&self) -> AppResult<PooledConnection> {
        let connection = db::open_connection_untracked(&self.0.runtime.path)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(PooledConnection {
            connection: Some(connection),
            permit: self.clone(),
            reusable: false,
            user_write: None,
        })
    }
}

pub struct MaintenancePermit {
    runtime: Arc<DatabaseRuntime>,
}
impl MaintenancePermit {
    pub fn connection(&self) -> AppResult<MaintenanceConnection<'_>> {
        Ok(MaintenanceConnection {
            connection: db::open_connection_untracked(&self.runtime.path)?,
            _permit: self,
        })
    }
}
/// A maintenance connection cannot survive reopening admission.
pub struct MaintenanceConnection<'a> {
    connection: Connection,
    _permit: &'a MaintenancePermit,
}
impl Deref for MaintenanceConnection<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.connection
    }
}
impl DerefMut for MaintenanceConnection<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}

impl Drop for MaintenancePermit {
    fn drop(&mut self) {
        let mut state = self
            .runtime
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        state.maintenance = false;
        self.runtime.changed.notify_all();
    }
}

pub struct PooledConnection {
    connection: Option<Connection>,
    permit: OperationPermit,
    reusable: bool,
    // Dropped after the connection is back in the pool.
    user_write: Option<UserWriteTicket>,
}
impl Deref for PooledConnection {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.connection.as_ref().expect("live connection")
    }
}
impl DerefMut for PooledConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        self.connection.as_mut().expect("live connection")
    }
}
impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(mut connection) = self.connection.take() {
            // A connection that still carries a cancellation callback must
            // not return to the pool.
            let cleared = connection.progress_handler(0, None::<fn() -> bool>).is_ok();
            let runtime = &self.permit.0.runtime;
            if self.reusable {
                // A user-write checkout replaced the busy handler; an idle
                // connection must carry the ordinary timeout again.
                let restored = cleared
                    && (self.user_write.is_none()
                        || connection.busy_timeout(db::BUSY_TIMEOUT).is_ok());
                let mut pool = runtime.pool.lock().unwrap_or_else(|e| e.into_inner());
                if restored && connection.is_autocommit() {
                    pool.idle.push(connection);
                } else {
                    pool.total -= 1;
                }
                runtime.available.notify_all();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    #[test]
    fn admitted_children_finish_while_maintenance_blocks_new_work_and_drains_pool() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = DatabaseRuntime::new(dir.path().join("test.db"));
        let permit = runtime.admit().unwrap();
        permit
            .connection()
            .unwrap()
            .execute_batch("CREATE TABLE test (id INTEGER)")
            .unwrap();
        let child = permit.clone();
        drop(permit);
        let (tx, rx) = mpsc::channel();
        let worker_runtime = runtime.clone();
        let worker = std::thread::spawn(move || {
            let maintenance = worker_runtime.maintenance().unwrap();
            tx.send(()).unwrap();
            assert!(worker_runtime.pool.lock().unwrap().idle.is_empty());
            drop(maintenance);
        });
        while !runtime.admission.lock().unwrap().maintenance {
            std::thread::yield_now();
        }
        child
            .connection()
            .unwrap()
            .execute("INSERT INTO test VALUES (1)", [])
            .unwrap();
        assert!(rx.try_recv().is_err());
        let connection = child.connection().unwrap();
        drop(child);
        assert!(rx.try_recv().is_err());
        drop(connection);
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
        assert_eq!(
            runtime
                .admit()
                .unwrap()
                .connection()
                .unwrap()
                .query_row("SELECT count(*) FROM test", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn sqlite_cancellation_interrupts_sorting_before_delivering_rows() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let runtime = DatabaseRuntime::new(dir.path().join("media.db"));
        let mut conn = runtime.admit().unwrap().connection().unwrap();
        let request = Arc::new(AtomicU64::new(1));
        let latest = request.clone();
        let mut callbacks = 0;
        conn.progress_handler(
            100,
            Some(move || {
                callbacks += 1;
                if callbacks == 3 {
                    latest.store(2, Ordering::SeqCst);
                }
                latest.load(Ordering::SeqCst) != 1
            }),
        )
        .unwrap();
        let result = conn.query_row(
            "WITH RECURSIVE n(x) AS (VALUES (1) UNION ALL SELECT x + 1 FROM n WHERE x < 10000) SELECT x FROM n ORDER BY x DESC",
            [], |row| row.get::<_, i64>(0),
        );
        assert!(
            matches!(result, Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::OperationInterrupted)
        );
        assert_eq!(request.load(Ordering::SeqCst), 2);
        drop(conn);
        assert_eq!(
            runtime
                .admit()
                .unwrap()
                .connection()
                .unwrap()
                .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn ordinary_checkouts_leave_the_user_write_reserve_free() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = DatabaseRuntime::new(dir.path().join("media.db"));
        let permit = runtime.admit().unwrap();
        let mut held: Vec<_> = (0..ORDINARY_CHECKOUTS)
            .map(|_| permit.connection().unwrap())
            .collect();
        assert_eq!(runtime.checked_out_connections(), ORDINARY_CHECKOUTS);

        let (tx, rx) = mpsc::channel();
        let waiter = permit.clone();
        let worker = std::thread::spawn(move || {
            let connection = waiter.connection().unwrap();
            tx.send(()).unwrap();
            drop(connection);
        });
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());

        let started = Instant::now();
        let user = permit.user_write_connection().unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(runtime.checked_out_connections(), ORDINARY_CHECKOUTS + 1);
        drop(user);
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());

        held.pop();
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
        drop(held);
        assert_eq!(runtime.checked_out_connections(), 0);
    }

    #[test]
    fn user_write_checkout_restores_the_default_busy_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = DatabaseRuntime::new(dir.path().join("media.db"));
        let permit = runtime.admit().unwrap();
        let busy_timeout = |conn: &Connection| {
            conn.query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i64>(0))
                .unwrap()
        };
        let user = permit.user_write_connection().unwrap();
        // A custom busy handler replaces the timeout handler.
        assert_eq!(busy_timeout(&user), 0);
        drop(user);
        let reused = permit.connection().unwrap();
        assert_eq!(runtime.pool.lock().unwrap().total, 1);
        assert_eq!(busy_timeout(&reused), 5_000);
    }

    #[test]
    fn background_writers_yield_until_user_writes_finish_within_a_bound() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = DatabaseRuntime::new(dir.path().join("media.db"));
        let permit = runtime.admit().unwrap();
        assert_eq!(permit.yield_to_user_writes(), Duration::ZERO);

        let user = permit.user_write_connection().unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            drop(user);
        });
        let waited = permit.yield_to_user_writes();
        releaser.join().unwrap();
        assert!(waited >= Duration::from_millis(40), "{waited:?}");
        assert!(waited < USER_WRITE_YIELD_LIMIT, "{waited:?}");

        let stuck = permit.user_write_connection().unwrap();
        let waited = permit.yield_to_user_writes();
        assert!(waited >= USER_WRITE_YIELD_LIMIT, "{waited:?}");
        assert!(waited < USER_WRITE_YIELD_LIMIT * 4, "{waited:?}");
        drop(stuck);
    }

    #[test]
    fn recycled_connection_has_no_progress_callback_or_admission() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = DatabaseRuntime::new(dir.path().join("media.db"));
        let permit = runtime.admit().unwrap();
        let mut conn = permit.connection().unwrap();
        conn.progress_handler(1, Some(|| true)).unwrap();
        drop(conn);
        drop(permit);
        assert_eq!(runtime.admission.lock().unwrap().active, 0);
        let conn = runtime.admit().unwrap().connection().unwrap();
        assert_eq!(conn.query_row("WITH RECURSIVE n(x) AS (VALUES (1) UNION ALL SELECT x + 1 FROM n WHERE x < 1000) SELECT SUM(x) FROM n", [], |row| row.get::<_, i64>(0)).unwrap(), 500500);
    }
}
