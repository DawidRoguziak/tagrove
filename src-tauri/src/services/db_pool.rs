use crate::{db, error::AppResult, services::asset_query_service::AssetQueryManager};
use rusqlite::Connection;
use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

const MAX_CONNECTIONS: usize = 4;

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

pub struct DatabaseRuntime {
    path: PathBuf,
    admission: Mutex<Admission>,
    changed: Condvar,
    pool: Mutex<Pool>,
    available: Condvar,
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
        let runtime = &self.0.runtime;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut pool = runtime.pool.lock().unwrap_or_else(|e| e.into_inner());
        let connection = loop {
            if let Some(connection) = pool.idle.pop() {
                break connection;
            }
            if pool.total < MAX_CONNECTIONS {
                pool.total += 1;
                drop(pool);
                match db::open_connection_untracked(&runtime.path) {
                    Ok(connection) => break connection,
                    Err(error) => {
                        runtime.pool.lock().unwrap_or_else(|e| e.into_inner()).total -= 1;
                        runtime.available.notify_one();
                        return Err(error.into());
                    }
                }
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("database pool is busy: timed out waiting for a free connection")?;
            pool = runtime
                .available
                .wait_timeout(pool, remaining)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        };
        Ok(PooledConnection {
            connection: Some(connection),
            permit: self.clone(),
            reusable: true,
        })
    }

    /// Source-file journals need power-loss durability and must not alter pooled settings.
    pub fn durable_connection(&self) -> AppResult<PooledConnection> {
        let connection = db::open_connection_untracked(&self.0.runtime.path)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(PooledConnection {
            connection: Some(connection),
            permit: self.clone(),
            reusable: false,
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
        if let Some(connection) = self.connection.take() {
            connection.progress_handler(0, None::<fn() -> bool>);
            let runtime = &self.permit.0.runtime;
            if self.reusable {
                let mut pool = runtime.pool.lock().unwrap_or_else(|e| e.into_inner());
                if connection.is_autocommit() {
                    pool.idle.push(connection);
                } else {
                    pool.total -= 1;
                }
                runtime.available.notify_one();
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
        let conn = runtime.admit().unwrap().connection().unwrap();
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
        );
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
    fn recycled_connection_has_no_progress_callback_or_admission() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = DatabaseRuntime::new(dir.path().join("media.db"));
        let permit = runtime.admit().unwrap();
        let conn = permit.connection().unwrap();
        conn.progress_handler(1, Some(|| true));
        drop(conn);
        drop(permit);
        assert_eq!(runtime.admission.lock().unwrap().active, 0);
        let conn = runtime.admit().unwrap().connection().unwrap();
        assert_eq!(conn.query_row("WITH RECURSIVE n(x) AS (VALUES (1) UNION ALL SELECT x + 1 FROM n WHERE x < 1000) SELECT SUM(x) FROM n", [], |row| row.get::<_, i64>(0)).unwrap(), 500500);
    }
}
