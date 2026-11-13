use super::*;

pub const MAX_THUMBNAIL_CANDIDATE_PAGE: usize = 2048;

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
        params![
            after,
            ceiling,
            limit.min(MAX_THUMBNAIL_CANDIDATE_PAGE) as i64,
            failed_only
        ],
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

/// Result of one thumbnail job as it is published to SQLite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThumbnailWriteOutcome {
    /// Store this path and clear the source version's failure marker.
    Ready(String),
    /// Clear the stored path and record a failure for the source version.
    Failed(&'static str),
}

/// One thumbnail result: the compare-and-set path write plus the failure
/// marker change, both guarded by the source version the job started from.
#[derive(Debug, Clone)]
pub struct ThumbnailWrite {
    pub asset_id: i64,
    pub version: crate::thumbs::SourceVersion,
    pub record_version: i64,
    pub outcome: ThumbnailWriteOutcome,
}

/// Publishes thumbnail results in one IMMEDIATE transaction. Each path write is
/// a compare-and-set on its source version; only an applied write changes the
/// failure marker of that `record_version`. IDs whose guard no longer matches
/// are returned (and nothing is written for them) so callers can remove their
/// produced files instead of leaving stale-version targets behind.
pub fn commit_thumbnail_results(
    conn: &Connection,
    writes: &[ThumbnailWrite],
) -> anyhow::Result<Vec<i64>> {
    if writes.is_empty() {
        return Ok(Vec::new());
    }
    let mut stale = Vec::new();
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    for write in writes {
        let path = match &write.outcome {
            ThumbnailWriteOutcome::Ready(path) => Some(path.as_str()),
            ThumbnailWriteOutcome::Failed(_) => None,
        };
        match thumbnail_cas_execute(&tx, write.asset_id, path, &write.version)? {
            ThumbnailCasOutcome::VersionMismatch => stale.push(write.asset_id),
            ThumbnailCasOutcome::Applied => match write.outcome {
                ThumbnailWriteOutcome::Ready(_) => {
                    clear_thumbnail_failure_if_version_matches(
                        &tx,
                        write.asset_id,
                        write.record_version,
                    )?;
                }
                ThumbnailWriteOutcome::Failed(error) => {
                    record_thumbnail_failure_if_version_matches(
                        &tx,
                        write.asset_id,
                        write.record_version,
                        Some(error),
                    )?;
                }
            },
        }
    }
    tx.commit()?;
    Ok(stale)
}

/// Thumbnail rows in requested order, each with whether a failure marker for
/// its current `record_version` exists. Both come from one statement, so a
/// stored path and its source version are read from the same snapshot.
pub fn get_assets_for_thumbnails_by_ids(
    conn: &Connection,
    asset_ids: &[i64],
) -> anyhow::Result<Vec<(ThumbnailAsset, bool)>> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_id = HashMap::<i64, (ThumbnailAsset, bool)>::with_capacity(asset_ids.len());
    for chunk in asset_ids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT a.id, a.path, a.kind, a.modified_at, a.duration_ms, a.thumb_path, a.size_bytes,
                    a.fingerprint_mtime_ns, a.record_version, tf.asset_id IS NOT NULL
             FROM assets a
             LEFT JOIN thumbnail_failures tf
               ON tf.asset_id = a.id AND tf.source_record_version = a.record_version
             WHERE a.id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
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
        })?;
        for row in rows {
            let entry = row?;
            by_id.insert(entry.0.id, entry);
        }
    }
    Ok(asset_ids
        .iter()
        .filter_map(|asset_id| by_id.remove(asset_id))
        .collect())
}

#[cfg(test)]
mod candidate_page_tests {
    use super::*;

    #[test]
    fn candidate_pages_can_fill_large_bulk_windows_but_remain_bounded() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn.execute_batch(
            "WITH RECURSIVE ids(id) AS (SELECT 1 UNION ALL SELECT id + 1 FROM ids WHERE id < 2055)
             INSERT INTO assets(path, kind, size_bytes, modified_at)
             SELECT 'fixture-' || id || '.png', 'image', 1, 1 FROM ids;",
        )
        .unwrap();
        assert_eq!(
            thumbnail_candidates_page(&conn, 0, 2055, 768, false)
                .unwrap()
                .len(),
            768
        );
        let page = thumbnail_candidates_page(&conn, 0, 2055, usize::MAX, false).unwrap();
        assert_eq!(page.len(), MAX_THUMBNAIL_CANDIDATE_PAGE);
        let after = page.last().unwrap().0.id;
        let tail = thumbnail_candidates_page(&conn, after, 2055, usize::MAX, false).unwrap();
        assert_eq!(tail.len(), 7);
        assert_eq!(tail.first().unwrap().0.id, after + 1);
    }

    fn scanned_asset(conn: &Connection, path: &str, mtime_ns: i64) -> crate::thumbs::SourceVersion {
        conn.execute(
            "INSERT INTO assets(path, kind, size_bytes, modified_at, fingerprint_mtime_ns)
             VALUES (?1, 'image', 10, 1, ?2)",
            params![path, mtime_ns],
        )
        .unwrap();
        crate::thumbs::SourceVersion::new(path, 10, mtime_ns)
    }

    #[test]
    fn thumbnail_results_commit_paths_and_failure_markers_in_one_transaction() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let ready = scanned_asset(&conn, "/media/ready.jpg", 1);
        let failed = scanned_asset(&conn, "/media/failed.jpg", 2);
        let reindexed = scanned_asset(&conn, "/media/reindexed.jpg", 3);
        record_thumbnail_failure_if_version_matches(&conn, 1, 1, Some("old failure")).unwrap();

        let commits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = commits.clone();
        conn.commit_hook(Some(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            false
        }))
        .unwrap();
        let stale = commit_thumbnail_results(
            &conn,
            &[
                ThumbnailWrite {
                    asset_id: 1,
                    version: ready,
                    record_version: 1,
                    outcome: ThumbnailWriteOutcome::Ready("/thumbs/ready.jpg".into()),
                },
                ThumbnailWrite {
                    asset_id: 2,
                    version: failed,
                    record_version: 1,
                    outcome: ThumbnailWriteOutcome::Failed("render failed"),
                },
                ThumbnailWrite {
                    asset_id: 3,
                    version: crate::thumbs::SourceVersion::new(
                        reindexed.path.clone(),
                        10,
                        reindexed.mtime_ns + 1,
                    ),
                    record_version: 1,
                    outcome: ThumbnailWriteOutcome::Ready("/thumbs/stale.jpg".into()),
                },
            ],
        )
        .unwrap();
        conn.commit_hook(None::<fn() -> bool>).unwrap();

        assert_eq!(commits.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(stale, vec![3]);
        let thumb = |id: i64| {
            conn.query_row("SELECT thumb_path FROM assets WHERE id = ?1", [id], |row| {
                row.get::<_, Option<String>>(0)
            })
            .unwrap()
        };
        assert_eq!(thumb(1).as_deref(), Some("/thumbs/ready.jpg"));
        assert_eq!(thumb(2), None);
        assert_eq!(thumb(3), None, "a stale result writes nothing");
        assert_eq!(list_failed_thumbnail_asset_ids(&conn).unwrap(), vec![2]);

        let flagged = get_assets_for_thumbnails_by_ids(&conn, &[3, 2, 1, 99]).unwrap();
        let summary: Vec<_> = flagged
            .iter()
            .map(|(asset, failed)| (asset.id, *failed))
            .collect();
        assert_eq!(summary, vec![(3, false), (2, true), (1, false)]);
        assert!(commit_thumbnail_results(&conn, &[]).unwrap().is_empty());
    }
}
