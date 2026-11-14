use super::*;

/// Total time an ordinary connection waits for a busy SQLite lock.
pub(crate) const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Busy handler for short user metadata writes. SQLite's default handler
/// backs off to 100 ms sleeps, so a waiting writer can keep missing the short
/// gaps between back-to-back scan batches. This one retries every millisecond
/// within the same `BUSY_TIMEOUT` budget.
pub(crate) fn poll_busy_lock_quickly(attempt: i32) -> bool {
    thread_local! {
        static STARTED: std::cell::Cell<Option<std::time::Instant>> =
            const { std::cell::Cell::new(None) };
    }
    let started = STARTED.with(|started| {
        if attempt == 0 || started.get().is_none() {
            started.set(Some(std::time::Instant::now()));
        }
        started.get().unwrap_or_else(std::time::Instant::now)
    });
    if started.elapsed() >= BUSY_TIMEOUT {
        return false;
    }
    thread::sleep(Duration::from_millis(1));
    true
}

/// Open an isolated database during startup or in service fixtures.
pub fn open_connection(db_path: &Path) -> anyhow::Result<Connection> {
    open_connection_untracked(db_path)
}

pub(crate) fn open_connection_untracked(db_path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("cannot open sqlite db at {}", db_path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -65_536i64)?;
    conn.pragma_update(None, "mmap_size", 268_435_456i64)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    Ok(conn)
}

pub fn open_connection_read_only(db_path: &Path) -> anyhow::Result<Connection> {
    open_connection_read_only_untracked(db_path)
}

pub(crate) fn open_connection_read_only_untracked(db_path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("cannot open sqlite db read-only at {}", db_path.display()))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    Ok(conn)
}
