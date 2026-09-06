use super::*;

pub fn thumbnail_candidates_page(
    conn: &Connection,
    after: i64,
    ceiling: i64,
    limit: usize,
    failed_only: bool,
) -> anyhow::Result<Vec<(ThumbnailAsset, bool)>> {
    let mut stmt = conn.prepare("SELECT a.id, a.path, a.kind, a.modified_at, a.duration_ms, a.thumb_path, a.size_bytes, a.fingerprint_mtime_ns, a.record_version,
        tf.asset_id IS NOT NULL FROM assets a LEFT JOIN thumbnail_failures tf ON tf.asset_id = a.id AND tf.source_record_version = a.record_version
        WHERE a.id > ?1 AND a.id <= ?2 AND (?4 = 0 OR tf.asset_id IS NOT NULL) ORDER BY a.id LIMIT ?3")?;
    let rows = stmt.query_map(
        params![after, ceiling, limit.min(512) as i64, failed_only],
        |row| {
            Ok((
                ThumbnailAsset {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    kind: row.get(2)?,
                    modified_at: row.get(3)?,
                    duration_ms: row.get(4)?,
                    thumb_path: row.get(5)?,
                    size_bytes: row.get(6)?,
                    fingerprint_mtime_ns: row.get(7)?,
                    record_version: row.get(8)?,
                },
                row.get(9)?,
            ))
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_assets_for_thumbnail_render(conn: &Connection) -> anyhow::Result<Vec<ThumbnailAsset>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "
        SELECT id, path, kind, modified_at, duration_ms, thumb_path, size_bytes, fingerprint_mtime_ns, record_version
        FROM assets ORDER BY id ASC
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ThumbnailAsset {
            id: row.get(0)?,
            path: row.get(1)?,
            kind: row.get(2)?,
            modified_at: row.get(3)?,
            duration_ms: row.get(4)?,
            thumb_path: row.get(5)?,
            size_bytes: row.get(6)?,
            fingerprint_mtime_ns: row.get(7)?,
            record_version: row.get(8)?,
        })
    })?;

    for row in rows {
        out.push(row?);
    }

    Ok(out)
}

pub fn list_failed_assets_for_thumbnail_render(
    conn: &Connection,
) -> anyhow::Result<Vec<ThumbnailAsset>> {
    cleanup_stale_thumbnail_failures(conn)?;

    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "
        SELECT a.id, a.path, a.kind, a.modified_at, a.duration_ms, a.thumb_path, a.size_bytes, a.fingerprint_mtime_ns, a.record_version
        FROM assets a
        JOIN thumbnail_failures tf ON tf.asset_id = a.id AND tf.source_record_version = a.record_version
        ORDER BY tf.last_failed_at ASC, a.id ASC
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ThumbnailAsset {
            id: row.get(0)?,
            path: row.get(1)?,
            kind: row.get(2)?,
            modified_at: row.get(3)?,
            duration_ms: row.get(4)?,
            thumb_path: row.get(5)?,
            size_bytes: row.get(6)?,
            fingerprint_mtime_ns: row.get(7)?,
            record_version: row.get(8)?,
        })
    })?;

    for row in rows {
        out.push(row?);
    }

    Ok(out)
}

pub fn list_failed_thumbnail_asset_ids(conn: &Connection) -> anyhow::Result<Vec<i64>> {
    cleanup_stale_thumbnail_failures(conn)?;

    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "
        SELECT tf.asset_id
        FROM thumbnail_failures tf
        JOIN assets a ON a.id = tf.asset_id
        WHERE tf.source_record_version = a.record_version
        ORDER BY tf.asset_id ASC
        ",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn record_thumbnail_failure(
    conn: &Connection,
    asset_id: i64,
    asset_modified_at: i64,
    error_message: Option<&str>,
) -> anyhow::Result<()> {
    let version: Option<i64> = conn
        .query_row(
            "SELECT record_version FROM assets WHERE id = ?1 AND modified_at = ?2",
            params![asset_id, asset_modified_at],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(version) = version {
        record_thumbnail_failure_if_version_matches(conn, asset_id, version, error_message)?;
    }
    Ok(())
}

pub fn record_thumbnail_failure_if_version_matches(
    conn: &Connection,
    asset_id: i64,
    record_version: i64,
    error_message: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute("INSERT INTO thumbnail_failures(asset_id, failure_count, last_error, last_failed_at, asset_modified_at, source_record_version)
        SELECT id, 1, ?2, unixepoch(), modified_at, record_version FROM assets WHERE id = ?1 AND record_version = ?3
        ON CONFLICT(asset_id) DO UPDATE SET
          failure_count = CASE WHEN thumbnail_failures.source_record_version = excluded.source_record_version THEN thumbnail_failures.failure_count + 1 ELSE 1 END,
          last_error = excluded.last_error, last_failed_at = excluded.last_failed_at,
          asset_modified_at = excluded.asset_modified_at, source_record_version = excluded.source_record_version",
        params![asset_id, error_message, record_version])?;
    Ok(())
}

pub fn clear_thumbnail_failure_if_version_matches(
    conn: &Connection,
    asset_id: i64,
    record_version: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id = ?1 AND source_record_version = ?2",
        params![asset_id, record_version],
    )?;
    Ok(())
}

pub fn clear_thumbnail_failure(conn: &Connection, asset_id: i64) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id = ?1",
        params![asset_id],
    )?;
    Ok(())
}

pub(super) fn cleanup_stale_thumbnail_failures(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id NOT IN (SELECT id FROM assets)",
        [],
    )?;
    conn.execute(
        "
        DELETE FROM thumbnail_failures
        WHERE asset_id IN (
          SELECT tf.asset_id
          FROM thumbnail_failures tf
          JOIN assets a ON a.id = tf.asset_id
          WHERE tf.source_record_version != a.record_version
        )
        ",
        [],
    )?;
    Ok(())
}

pub fn clear_all_thumbnail_paths(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE assets SET thumb_path = NULL WHERE thumb_path IS NOT NULL",
        [],
    )?;
    conn.execute("DELETE FROM thumbnail_failures", [])?;
    Ok(())
}

pub fn get_asset_for_thumbnail(
    conn: &Connection,
    asset_id: i64,
) -> anyhow::Result<Option<ThumbnailAsset>> {
    let asset = conn
        .query_row(
            "
            SELECT id, path, kind, modified_at, duration_ms, thumb_path, size_bytes, fingerprint_mtime_ns, record_version
            FROM assets WHERE id = ?1
            ",
            params![asset_id],
            |row| {
                Ok(ThumbnailAsset {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    kind: row.get(2)?,
                    modified_at: row.get(3)?,
                    duration_ms: row.get(4)?,
                    thumb_path: row.get(5)?,
                    size_bytes: row.get(6)?,
                    fingerprint_mtime_ns: row.get(7)?,
                    record_version: row.get(8)?,
                })
            },
        )
        .optional()?;
    Ok(asset)
}

pub(super) fn thumbnail_cas_execute(
    conn: &Connection,
    asset_id: i64,
    thumb_path: Option<&str>,
    version: &crate::thumbs::SourceVersion,
) -> anyhow::Result<ThumbnailCasOutcome> {
    let changed = conn.execute(
        "
        UPDATE assets SET thumb_path = ?1
        WHERE id = ?2 AND path = ?3 AND size_bytes = ?4 AND fingerprint_mtime_ns = ?5
          AND COALESCE(thumb_path, '') != COALESCE(?1, '')
        ",
        params![
            thumb_path,
            asset_id,
            version.path,
            version.size_bytes,
            version.mtime_ns
        ],
    )?;
    if changed > 0 {
        return Ok(ThumbnailCasOutcome::Applied);
    }

    // Zero rows can mean either a version mismatch or an already-equal value.
    // Re-read to distinguish: an equal stored path with matching version is a
    // successful no-op; anything else is a lost race against a re-index.
    let current = conn
        .query_row(
            "SELECT COALESCE(thumb_path, '') FROM assets WHERE id = ?1",
            params![asset_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let expected = thumb_path.unwrap_or_default();
    match current {
        Some(stored) if stored == expected => {
            let version_matches = conn.query_row(
                "SELECT COUNT(*) FROM assets
                 WHERE id = ?1 AND path = ?2 AND size_bytes = ?3 AND fingerprint_mtime_ns = ?4",
                params![asset_id, version.path, version.size_bytes, version.mtime_ns],
                |row| row.get::<_, i64>(0),
            )? > 0;
            if version_matches {
                Ok(ThumbnailCasOutcome::Applied)
            } else {
                Ok(ThumbnailCasOutcome::VersionMismatch)
            }
        }
        _ => Ok(ThumbnailCasOutcome::VersionMismatch),
    }
}

/// Compare-and-set of `assets.thumb_path` guarded by the exact source version
/// the render was started for. A stale result never overwrites a re-indexed
/// record; the caller receives [`ThumbnailCasOutcome::VersionMismatch`] and is
/// responsible for removing the produced file.
pub fn update_asset_thumbnail_path_if_version_matches(
    conn: &Connection,
    asset_id: i64,
    thumb_path: Option<&str>,
    version: &crate::thumbs::SourceVersion,
) -> anyhow::Result<ThumbnailCasOutcome> {
    thumbnail_cas_execute(conn, asset_id, thumb_path, version)
}

/// Versioned batch write of thumbnail paths. Each row is a compare-and-set
/// guarded by the source version captured when its render started; IDs whose
/// guard no longer matches are returned so callers can remove their produced
/// files instead of leaving stale-version targets behind.
pub fn update_asset_thumbnail_paths_batch_versioned(
    conn: &Connection,
    updates: &[(i64, Option<String>, crate::thumbs::SourceVersion)],
) -> anyhow::Result<Vec<i64>> {
    if updates.is_empty() {
        return Ok(Vec::new());
    }

    let mut stale = Vec::new();
    let tx = conn.unchecked_transaction()?;
    for (asset_id, thumb_path, version) in updates {
        let outcome = thumbnail_cas_execute(&tx, *asset_id, thumb_path.as_deref(), version)?;
        if outcome == ThumbnailCasOutcome::VersionMismatch {
            stale.push(*asset_id);
        }
    }
    tx.commit()?;
    Ok(stale)
}

pub fn get_assets_for_thumbnails_by_ids(
    conn: &Connection,
    asset_ids: &[i64],
) -> anyhow::Result<Vec<ThumbnailAsset>> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_id = HashMap::<i64, ThumbnailAsset>::with_capacity(asset_ids.len());
    for chunk in asset_ids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT id, path, kind, modified_at, duration_ms, thumb_path, size_bytes, fingerprint_mtime_ns, record_version
             FROM assets WHERE id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            Ok(ThumbnailAsset {
                id: row.get(0)?,
                path: row.get(1)?,
                kind: row.get(2)?,
                modified_at: row.get(3)?,
                duration_ms: row.get(4)?,
                thumb_path: row.get(5)?,
                size_bytes: row.get(6)?,
                fingerprint_mtime_ns: row.get(7)?,
                record_version: row.get(8)?,
            })
        })?;
        for row in rows {
            let asset = row?;
            by_id.insert(asset.id, asset);
        }
    }
    Ok(asset_ids
        .iter()
        .filter_map(|asset_id| by_id.remove(asset_id))
        .collect())
}
