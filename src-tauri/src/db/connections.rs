use super::*;

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
    conn.busy_timeout(Duration::from_secs(5))?;
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
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(conn)
}
