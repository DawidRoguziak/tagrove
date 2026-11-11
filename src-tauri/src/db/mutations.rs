use super::*;

pub fn upsert_asset(conn: &Connection, asset: &NewAsset) -> anyhow::Result<()> {
    upsert_asset_with_fingerprint(conn, asset, asset.modified_at.saturating_mul(1_000_000_000))
}

pub fn delete_assets_by_prefix_with_thumbs(
    conn: &Connection,
    prefix: &str,
) -> anyhow::Result<(usize, Vec<String>)> {
    let like_pattern = root_descendant_like_pattern(prefix);
    let mut thumbs = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT thumb_path FROM assets WHERE (path = ?1 OR path LIKE ?2 ESCAPE '^') AND thumb_path IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![prefix, like_pattern.as_str()], |row| {
        row.get::<_, String>(0)
    })?;
    for row in rows {
        thumbs.push(row?);
    }

    conn.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id IN (SELECT id FROM assets WHERE path = ?1 OR path LIKE ?2 ESCAPE '^')",
        params![prefix, like_pattern.as_str()],
    )?;

    let removed_assets = conn.execute(
        "DELETE FROM assets WHERE path = ?1 OR path LIKE ?2 ESCAPE '^'",
        params![prefix, like_pattern.as_str()],
    )?;

    cleanup_orphan_tags(conn)?;

    Ok((removed_assets, thumbs))
}

pub fn delete_asset_by_id_with_thumb(
    conn: &Connection,
    asset_id: i64,
) -> anyhow::Result<Option<(String, Option<String>)>> {
    let asset = conn
        .query_row(
            "SELECT path, thumb_path FROM assets WHERE id = ?1",
            params![asset_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?;

    let Some((path, thumb_path)) = asset else {
        return Ok(None);
    };

    conn.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id = ?1",
        params![asset_id],
    )?;
    conn.execute("DELETE FROM assets WHERE id = ?1", params![asset_id])?;
    cleanup_orphan_tags(conn)?;

    Ok(Some((path, thumb_path)))
}

pub fn apply_file_mutations(
    conn: &Connection,
    operation_id: &str,
    mutations: &[DbFileMutation<'_>],
) -> anyhow::Result<i64> {
    // IMMEDIATE takes the write lock up front. A rename reads before its first write, and in WAL
    // mode a deferred read snapshot cannot be upgraded once another connection has committed
    // (SQLITE_BUSY_SNAPSHOT, which the busy timeout does not retry). That would roll back a
    // batch whose compare-and-set checks all hold.
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let mut pending_renames = Vec::new();
    for mutation in mutations {
        let affected = match mutation {
            DbFileMutation::Rename {
                asset_id,
                expected_path,
                expected_record_version,
                new_path,
                new_file_name,
            } => {
                let temporary_path = format!("mediatagger-pending://{operation_id}/{asset_id}");
                let occupied = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM assets WHERE path = ?1)",
                    params![temporary_path],
                    |row| row.get::<_, bool>(0),
                )?;
                if occupied {
                    anyhow::bail!("internal rename path collision for asset {asset_id}");
                }
                let affected = tx.execute(
                    "UPDATE assets SET path = ?1
                     WHERE id = ?2 AND path = ?3 AND record_version = ?4",
                    params![
                        temporary_path,
                        asset_id,
                        expected_path,
                        expected_record_version
                    ],
                )?;
                if affected == 1 {
                    tx.execute(
                        "DELETE FROM thumbnail_failures WHERE asset_id = ?1",
                        params![asset_id],
                    )?;
                    pending_renames.push((*asset_id, temporary_path, *new_path, *new_file_name));
                }
                affected
            }
            DbFileMutation::Delete {
                asset_id,
                expected_path,
                expected_record_version,
            } => {
                tx.execute(
                    "DELETE FROM thumbnail_failures WHERE asset_id = ?1",
                    params![asset_id],
                )?;
                tx.execute(
                    "DELETE FROM assets WHERE id = ?1 AND path = ?2 AND record_version = ?3",
                    params![asset_id, expected_path, expected_record_version],
                )?
            }
        };
        if affected != 1 {
            anyhow::bail!(
                "asset {} changed since the operation was prepared",
                match mutation {
                    DbFileMutation::Rename { asset_id, .. }
                    | DbFileMutation::Delete { asset_id, .. } => asset_id,
                }
            );
        }
    }
    for (asset_id, temporary_path, new_path, new_file_name) in pending_renames {
        let affected = tx.execute(
            "UPDATE assets SET path = ?1, file_name = ?2, file_name_key = ?3,
               thumb_path = NULL, record_version = record_version + 1
             WHERE id = ?4 AND path = ?5",
            params![
                new_path,
                new_file_name,
                canonical_key(new_file_name),
                asset_id,
                temporary_path
            ],
        )?;
        if affected != 1 {
            anyhow::bail!("asset {asset_id} changed while finalizing its database rename");
        }
    }
    cleanup_orphan_tags(&tx)?;
    let revision = bump_library_revision_in_tx(&tx)?;
    tx.execute(
        "UPDATE pending_file_operations SET committed = 1 WHERE operation_id = ?1",
        params![operation_id],
    )?;
    tx.commit()?;
    Ok(revision)
}

pub fn insert_pending_file_operations(
    conn: &Connection,
    operations: &[PendingFileOperation],
) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    for operation in operations {
        tx.execute(
            "INSERT INTO pending_file_operations(
               operation_id, asset_id, action, original_path, staging_path, final_path, committed
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                operation.operation_id,
                operation.asset_id,
                operation.action,
                operation.original_path,
                operation.staging_path,
                operation.final_path,
                operation.committed,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Copies every `pending_file_operations` row of `source` into `target` in one transaction,
/// keeping `created_at` so recovery later walks the rows in their original order, and returns
/// the copied rows in that order. A source without the table has nothing to copy. `target`
/// must not already hold a row with the same key.
pub fn copy_pending_file_operations(
    source: &Connection,
    target: &Connection,
) -> anyhow::Result<Vec<PendingFileOperation>> {
    let has_journal = source.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'pending_file_operations')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !has_journal {
        return Ok(Vec::new());
    }
    let mut statement = source.prepare(
        "SELECT operation_id, asset_id, action, original_path, staging_path, final_path, committed,
                created_at
         FROM pending_file_operations ORDER BY created_at, operation_id, asset_id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                PendingFileOperation {
                    operation_id: row.get(0)?,
                    asset_id: row.get(1)?,
                    action: row.get(2)?,
                    original_path: row.get(3)?,
                    staging_path: row.get(4)?,
                    final_path: row.get(5)?,
                    committed: row.get(6)?,
                },
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let tx = target.unchecked_transaction()?;
    for (operation, created_at) in &rows {
        tx.execute(
            "INSERT INTO pending_file_operations(
               operation_id, asset_id, action, original_path, staging_path, final_path, committed,
               created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                operation.operation_id,
                operation.asset_id,
                operation.action,
                operation.original_path,
                operation.staging_path,
                operation.final_path,
                operation.committed,
                created_at,
            ],
        )?;
    }
    tx.commit()?;
    Ok(rows.into_iter().map(|(operation, _)| operation).collect())
}

pub fn remove_pending_file_operation(
    conn: &Connection,
    operation_id: &str,
    asset_id: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM pending_file_operations WHERE operation_id = ?1 AND asset_id = ?2",
        params![operation_id, asset_id],
    )?;
    Ok(())
}

pub fn clear_library_data(conn: &Connection) -> anyhow::Result<(usize, usize, Vec<String>)> {
    let tx = conn.unchecked_transaction()?;
    let mut thumbs = Vec::new();
    {
        let mut stmt =
            tx.prepare("SELECT DISTINCT thumb_path FROM assets WHERE thumb_path IS NOT NULL")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            thumbs.push(row?);
        }
    }

    let removed_assets = tx.execute("DELETE FROM assets", [])?;
    tx.execute("DELETE FROM thumbnail_failures", [])?;
    tx.execute("DELETE FROM tags", [])?;
    let removed_roots = tx.execute("DELETE FROM scan_roots", [])?;
    bump_library_revision_in_tx(&tx)?;
    tx.commit()?;

    Ok((removed_assets, removed_roots, thumbs))
}

pub(crate) fn cleanup_orphan_tags(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM asset_tags)",
        [],
    )?;
    Ok(())
}

pub fn set_asset_favorite(
    conn: &Connection,
    asset_id: i64,
    is_favorite: bool,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE assets SET is_favorite = ?1 WHERE id = ?2",
        params![if is_favorite { 1 } else { 0 }, asset_id],
    )?;
    Ok(())
}

pub fn set_asset_favorite_with_revision(
    conn: &mut Connection,
    asset_id: i64,
    is_favorite: bool,
) -> anyhow::Result<i64> {
    retry_immediate_transaction(conn, |tx| {
        let changed = tx.execute(
            "UPDATE assets SET is_favorite = ?1 WHERE id = ?2 AND is_favorite != ?1",
            params![if is_favorite { 1 } else { 0 }, asset_id],
        )? > 0;
        bump_scoped_revision(
            tx,
            &if changed {
                vec!["favorites".into()]
            } else {
                vec![]
            },
        )
    })
}

pub fn toggle_assets_favorite_bulk(
    conn: &mut Connection,
    asset_ids: &[i64],
) -> anyhow::Result<crate::models::BulkFavoriteSummary> {
    retry_immediate_transaction(conn, |tx| toggle_assets_favorite_bulk_in_tx(tx, asset_ids))
}

/// Sets every existing asset of `asset_ids` to favorite unless all already
/// are, in which case all are cleared. Bumps the `favorites` revision.
pub(crate) fn toggle_assets_favorite_bulk_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_ids: &[i64],
) -> anyhow::Result<crate::models::BulkFavoriteSummary> {
    let mut processed_asset_ids = Vec::new();
    let mut all_favorites = true;
    let mut seen = std::collections::HashSet::new();
    let mut query = tx.prepare_cached("SELECT is_favorite FROM assets WHERE id = ?1")?;
    for &asset_id in asset_ids {
        if asset_id <= 0 || !seen.insert(asset_id) {
            continue;
        }
        if let Some(favorite) = query
            .query_row([asset_id], |row| row.get::<_, bool>(0))
            .optional()?
        {
            processed_asset_ids.push(asset_id);
            all_favorites &= favorite;
        }
    }
    let is_favorite = !all_favorites;
    let revision = if processed_asset_ids.is_empty() {
        current_library_revision(tx)?
    } else {
        let mut update = tx.prepare_cached(
            "UPDATE assets SET is_favorite = ?1 WHERE id = ?2 AND is_favorite != ?1",
        )?;
        for &asset_id in &processed_asset_ids {
            update.execute(params![is_favorite, asset_id])?;
        }
        bump_scoped_revision(tx, &["favorites".into()])?
    };
    Ok(crate::models::BulkFavoriteSummary {
        processed_asset_ids,
        is_favorite,
        revision,
    })
}

pub fn set_asset_media_group(
    conn: &Connection,
    asset_id: i64,
    media_group_key: Option<&str>,
    media_group_order: Option<f64>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE assets SET media_group_key = ?1, media_group_order = ?2,
          media_group_key_normalized = CASE
            WHEN ?1 IS NULL OR trim(?1) = '' THEN NULL
            ELSE lower(trim(?1))
          END
         WHERE id = ?3",
        params![media_group_key, media_group_order, asset_id],
    )?;
    Ok(())
}

pub fn set_asset_media_group_with_revision(
    conn: &mut Connection,
    asset_id: i64,
    media_group_key: Option<&str>,
    media_group_order: Option<f64>,
) -> anyhow::Result<i64> {
    retry_immediate_transaction(conn, |tx| {
        tx.execute(
            "UPDATE assets SET media_group_key = ?1, media_group_order = ?2,
              media_group_key_normalized = CASE
                WHEN ?1 IS NULL OR trim(?1) = '' THEN NULL
                ELSE lower(trim(?1))
              END
             WHERE id = ?3",
            params![media_group_key, media_group_order, asset_id],
        )?;
        bump_library_revision_in_tx(tx)
    })
}

pub fn set_assets_media_group_bulk(
    conn: &mut Connection,
    updates: &[(i64, Option<f64>)],
    media_group_key: Option<&str>,
) -> anyhow::Result<(Vec<i64>, usize)> {
    if updates.is_empty() {
        return Ok((Vec::new(), 0));
    }

    retry_immediate_transaction(conn, |tx| {
        set_assets_media_group_bulk_in_tx(tx, updates, media_group_key)
    })
}

/// Assigns `media_group_key` and each update's order to the existing assets,
/// keeping sort keys consistent; bumps the library revision when a row changed.
pub(crate) fn set_assets_media_group_bulk_in_tx(
    tx: &rusqlite::Transaction<'_>,
    updates: &[(i64, Option<f64>)],
    media_group_key: Option<&str>,
) -> anyhow::Result<(Vec<i64>, usize)> {
    let mut processed_assets = Vec::new();
    let mut updated_assets = 0usize;
    let mut sort_keys = DeferredSortKeys::begin(tx)?;

    let mut get_current_stmt =
        tx.prepare("SELECT media_group_key, media_group_order FROM assets WHERE id = ?1")?;
    let mut update_stmt = tx.prepare(
        "UPDATE assets SET media_group_key = ?1, media_group_order = ?2,
           media_group_key_normalized = CASE
             WHEN ?1 IS NULL OR trim(?1) = '' THEN NULL
             ELSE lower(trim(?1))
           END
         WHERE id = ?3",
    )?;

    for (asset_id, media_group_order) in updates {
        let current = get_current_stmt
            .query_row(params![asset_id], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                ))
            })
            .optional()?;

        let Some((current_key, current_order)) = current else {
            continue;
        };

        processed_assets.push(*asset_id);
        if current_key.as_deref() == media_group_key && current_order == *media_group_order {
            continue;
        }

        update_stmt.execute(params![media_group_key, media_group_order, asset_id])?;
        sort_keys.touch(current_key.as_deref());
        sort_keys.touch(media_group_key);
        updated_assets += 1;
    }

    drop(update_stmt);
    drop(get_current_stmt);
    sort_keys.finish(tx)?;

    if updated_assets > 0 {
        bump_library_revision_in_tx(tx)?;
    }
    Ok((processed_assets, updated_assets))
}

/// A ranged group write that changes at least this many rows, and at least an
/// eighth of the library, rebuilds the group-write indexes instead of
/// maintaining them row by row (`with_group_write_indexes_rebuilt`).
const GROUP_INDEX_REBUILD_MIN_ROWS: i64 = 20_000;

/// Set-based media-group write for a resolved selection (record S-034d0ea6).
///
/// Same result as `set_assets_media_group_bulk_in_tx` with the order of each
/// ID set to its one-based position in `asset_ids` (no order when clearing):
/// a missing ID is skipped but keeps its position, so later orders have a gap;
/// a row whose key and order already match is processed but not rewritten;
/// the derived ordering keys of every touched bucket end up as a full
/// recomputation leaves them; and the library revision is bumped once, in
/// `tx`, only when a row changed. `asset_ids` are expected to be distinct, as
/// a resolved selection is; a repeated ID counts once and takes its last
/// position. Returns the processed (existing) and updated asset counts.
///
/// The per-asset path costs about 16 µs per row (17 s for 1,000,000 rows in
/// one transaction). Here the IDs are staged in a temporary table and a few
/// statements over it do the work: the new bucket's ordering keys are
/// computed once and written with the group fields, only the buckets the rows
/// leave are recomputed, and a write that changes a large part of the library
/// rebuilds the three indexes it would otherwise update per row (ADR 0026).
pub(crate) fn set_selection_media_group_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_ids: &[i64],
    media_group_key: Option<&str>,
) -> anyhow::Result<(usize, usize)> {
    set_selection_media_group_with(tx, asset_ids, media_group_key, None)
}

/// `rebuild_indexes` overrides the size rule that picks between maintaining
/// and rebuilding the group-write indexes (tests cover both paths).
pub(crate) fn set_selection_media_group_with(
    tx: &rusqlite::Transaction<'_>,
    asset_ids: &[i64],
    media_group_key: Option<&str>,
    rebuild_indexes: Option<bool>,
) -> anyhow::Result<(usize, usize)> {
    tx.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS selection_group_ids(
           id INTEGER PRIMARY KEY, group_order REAL
         );
         DELETE FROM temp.selection_group_ids;",
    )?;
    {
        // Staged in ID order, so every insert appends to the temporary B-tree
        // and the existence checks walk the assets table forward. Missing IDs
        // are not staged; their positions stay unused.
        let mut staged: Vec<(i64, usize)> = asset_ids.iter().copied().zip(1..).collect();
        staged.sort_unstable();
        let mut stage = tx.prepare_cached(
            "INSERT INTO temp.selection_group_ids(id, group_order)
             SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM main.assets WHERE id = ?1)
             ON CONFLICT(id) DO UPDATE SET group_order = excluded.group_order",
        )?;
        for (asset_id, position) in staged {
            stage.execute(params![asset_id, media_group_key.map(|_| position as f64)])?;
        }
    }
    let count_staged = |tx: &rusqlite::Transaction<'_>| -> anyhow::Result<i64> {
        Ok(
            tx.query_row("SELECT COUNT(*) FROM temp.selection_group_ids", [], |row| {
                row.get(0)
            })?,
        )
    };
    let processed = count_staged(tx)?;
    // Keep only the rows the write changes: the same exact comparison of key
    // and order as the per-asset path (BINARY text, numeric order).
    tx.execute(
        "DELETE FROM temp.selection_group_ids
         WHERE EXISTS (
           SELECT 1 FROM main.assets a
           WHERE a.id = selection_group_ids.id
             AND a.media_group_key IS ?1
             AND a.media_group_order IS selection_group_ids.group_order
         )",
        params![media_group_key],
    )?;
    let changed = count_staged(tx)?;
    if changed == 0 {
        return Ok((processed as usize, 0));
    }

    let grouped_new: bool = tx.query_row(
        "SELECT ?1 IS NOT NULL AND trim(?1) <> ''",
        params![media_group_key],
        |row| row.get(0),
    )?;
    let mut sort_keys = DeferredSortKeys::begin(tx)?;
    {
        // The buckets the changed rows leave; the new one is written below.
        let mut old_buckets = tx.prepare(
            "SELECT DISTINCT CAST(media_group_key AS TEXT) FROM main.assets
             WHERE id IN (SELECT id FROM temp.selection_group_ids)
               AND media_group_key IS NOT NULL AND trim(media_group_key) <> ''
               AND CAST(media_group_key AS TEXT) IS NOT ?1",
        )?;
        let keys =
            old_buckets.query_map(params![media_group_key], |row| row.get::<_, String>(0))?;
        for key in keys {
            sort_keys.touch(Some(&key?));
        }
    }
    // The new bucket's ordering keys over its final members: the members it
    // already has plus the changed rows. Members outside the selection get
    // them now, through the bucket index; the changed rows get them with the
    // group fields. A singleton (cleared or blank key) gets its own keys.
    let (bucket_time, bucket_id) = if grouped_new {
        let (time, id): (rusqlite::types::Value, rusqlite::types::Value) = tx.query_row(
            "SELECT MAX(modified_at), MAX(id) FROM (
               SELECT modified_at, id FROM main.assets
               WHERE media_group_key IS NOT NULL AND trim(media_group_key) <> ''
                 AND CAST(media_group_key AS TEXT) = ?1
               UNION ALL
               SELECT modified_at, id FROM main.assets
               WHERE id IN (SELECT id FROM temp.selection_group_ids)
             )",
            params![media_group_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        tx.execute(
            "UPDATE main.assets SET sort_bucket_time = ?2, sort_bucket_id = ?3,
               sort_group_null_order = (media_group_order IS NULL)
             WHERE media_group_key IS NOT NULL AND trim(media_group_key) <> ''
               AND CAST(media_group_key AS TEXT) = ?1
               AND id NOT IN (SELECT id FROM temp.selection_group_ids)
               AND (sort_bucket_time IS NOT ?2 OR sort_bucket_id IS NOT ?3
                 OR sort_group_null_order IS NOT (media_group_order IS NULL))",
            params![media_group_key, time, id],
        )?;
        (time, id)
    } else {
        (rusqlite::types::Value::Null, rusqlite::types::Value::Null)
    };
    let rebuild = match rebuild_indexes {
        Some(rebuild) => rebuild,
        None => {
            changed >= GROUP_INDEX_REBUILD_MIN_ROWS
                && changed.saturating_mul(8)
                    >= tx.query_row("SELECT COUNT(*) FROM main.assets", [], |row| {
                        row.get::<_, i64>(0)
                    })?
        }
    };
    // Every changed row gets a non-null order exactly when the key is set, so
    // `sort_group_null_order` is 0 for grouped and singleton rows alike.
    let write = || -> anyhow::Result<usize> {
        Ok(tx.execute(
            "UPDATE main.assets SET media_group_key = ?1,
               media_group_order = (
                 SELECT group_order FROM temp.selection_group_ids WHERE id = assets.id
               ),
               media_group_key_normalized = CASE
                 WHEN ?1 IS NULL OR trim(?1) = '' THEN NULL
                 ELSE lower(trim(?1))
               END,
               sort_bucket_time = CASE WHEN ?4 THEN ?2 ELSE modified_at END,
               sort_bucket_id = CASE WHEN ?4 THEN ?3 ELSE id END,
               sort_group_null_order = 0
             WHERE id IN (SELECT id FROM temp.selection_group_ids)",
            params![media_group_key, bucket_time, bucket_id, grouped_new],
        )?)
    };
    let updated = if rebuild {
        with_group_write_indexes_rebuilt(tx, write)?
    } else {
        write()?
    };
    sort_keys.finish(tx)?;
    tx.execute("DELETE FROM temp.selection_group_ids", [])?;
    bump_library_revision_in_tx(tx)?;
    Ok((processed as usize, updated))
}

pub(super) fn list_asset_tags_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_id: i64,
) -> anyhow::Result<Option<Vec<String>>> {
    let exists = tx
        .prepare_cached("SELECT 1 FROM assets WHERE id = ?1")?
        .query_row(params![asset_id], |_| Ok(()))
        .optional()?
        .is_some();
    if !exists {
        return Ok(None);
    }
    let mut stmt = tx.prepare_cached(
        "SELECT lower(trim(t.name)) FROM tags t JOIN asset_tags at ON at.tag_id = t.id
         WHERE at.asset_id = ?1 ORDER BY lower(trim(t.name)) ASC",
    )?;
    let rows = stmt.query_map(params![asset_id], |row| row.get::<_, String>(0))?;
    Ok(Some(rows.collect::<Result<Vec<_>, _>>()?))
}

pub(super) fn canonicalize_tag_rows_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_id: i64,
) -> anyhow::Result<bool> {
    let normalized_names = {
        let mut stmt = tx.prepare_cached(
            "SELECT DISTINCT lower(trim(t.name))
             FROM tags t
             JOIN asset_tags at ON at.tag_id = t.id
             WHERE at.asset_id = ?1 AND trim(t.name) <> ''",
        )?;
        let rows = stmt.query_map(params![asset_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let mut changed = false;
    for normalized_name in normalized_names {
        let tag_rows = {
            let mut stmt = tx.prepare_cached(
                "SELECT id, name FROM tags
                 WHERE lower(trim(name)) = ?1
                 ORDER BY CASE WHEN name = ?1 COLLATE BINARY THEN 0 ELSE 1 END, id ASC",
            )?;
            let rows = stmt.query_map(params![normalized_name], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let Some(&(keeper_id, _)) = tag_rows.first() else {
            continue;
        };

        let redundant_ids = tag_rows
            .iter()
            .skip(1)
            .map(|(tag_id, _)| *tag_id)
            .collect::<Vec<_>>();
        let mut affected_asset_ids = Vec::new();
        if !redundant_ids.is_empty() {
            let placeholders = vec!["?"; redundant_ids.len()].join(",");
            let select_sql = format!(
                "SELECT DISTINCT asset_id FROM asset_tags WHERE tag_id IN ({placeholders})"
            );
            let mut stmt = tx.prepare(&select_sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(redundant_ids.iter()), |row| {
                row.get::<_, i64>(0)
            })?;
            affected_asset_ids = rows.collect::<Result<Vec<_>, _>>()?;
            drop(stmt);

            for redundant_id in &redundant_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO asset_tags(asset_id, tag_id)
                     SELECT asset_id, ?1 FROM asset_tags WHERE tag_id = ?2",
                    params![keeper_id, redundant_id],
                )?;
                tx.execute(
                    "DELETE FROM asset_tags WHERE tag_id = ?1",
                    params![redundant_id],
                )?;
                tx.execute("DELETE FROM tags WHERE id = ?1", params![redundant_id])?;
            }
            changed = true;
        }

        let keeper_name: String = tx
            .prepare_cached("SELECT name FROM tags WHERE id = ?1")?
            .query_row(params![keeper_id], |row| row.get(0))?;
        if keeper_name != normalized_name {
            tx.execute(
                "UPDATE tags SET name = ?1 WHERE id = ?2",
                params![normalized_name, keeper_id],
            )?;
            changed = true;
        }

        for affected_asset_id in affected_asset_ids {
            tx.execute(
                "UPDATE assets SET tag_count = (
                   SELECT COUNT(*) FROM asset_tags WHERE asset_id = ?1
                 ) WHERE id = ?1",
                params![affected_asset_id],
            )?;
        }
    }

    Ok(changed)
}

pub(crate) struct TagMutationOutcome {
    pub changed: bool,
    pub vocabulary_changed: bool,
    pub tags: Vec<String>,
    pub query_impact: TagQueryImpact,
}

pub(crate) fn set_asset_tags_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_id: i64,
    tags: &[String],
) -> anyhow::Result<TagMutationOutcome> {
    let normalized = normalize_and_validate_tags(tags.to_vec())?;
    let existing = list_asset_tags_in_tx(tx, asset_id)?
        .ok_or_else(|| anyhow::anyhow!("Asset {asset_id} not found"))?;
    let noncanonical = existing != normalize_tags(existing.clone());
    let mut vocabulary_changed = canonicalize_tag_rows_in_tx(tx, asset_id)?;
    let canonicalized = vocabulary_changed || noncanonical;
    let actual_count: i64 = tx
        .prepare_cached("SELECT COUNT(*) FROM asset_tags WHERE asset_id = ?1")?
        .query_row(params![asset_id], |row| row.get(0))?;
    let stored_count: i64 = tx
        .prepare_cached("SELECT tag_count FROM assets WHERE id = ?1")?
        .query_row(params![asset_id], |row| row.get(0))?;
    let count_repaired = stored_count != actual_count;
    if count_repaired {
        tx.execute(
            "UPDATE assets SET tag_count = ?1 WHERE id = ?2",
            params![actual_count, asset_id],
        )?;
    }

    let mut comparable = normalized.clone();
    comparable.sort();
    if existing == comparable {
        return Ok(TagMutationOutcome {
            changed: canonicalized || count_repaired,
            vocabulary_changed,
            tags: comparable,
            query_impact: if canonicalized || count_repaired {
                TagQueryImpact::All
            } else {
                TagQueryImpact::None
            },
        });
    }

    // Canonicalization may have merged IDs shared with other assets. Capture the
    // surviving assignments before replacing them, then exclude retained IDs.
    let mut removed_candidates = {
        let mut stmt = tx.prepare("SELECT tag_id FROM asset_tags WHERE asset_id = ?1")?;
        let rows = stmt.query_map(params![asset_id], |row| row.get::<_, i64>(0))?;
        rows.collect::<Result<std::collections::BTreeSet<_>, _>>()?
    };
    tx.execute(
        "DELETE FROM asset_tags WHERE asset_id = ?1",
        params![asset_id],
    )?;
    for tag in &normalized {
        vocabulary_changed |= tx
            .prepare_cached("INSERT INTO tags(name) VALUES (?1) ON CONFLICT(name) DO NOTHING")?
            .execute(params![tag])?
            > 0;
        let tag_id: i64 = tx
            .prepare_cached("SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE")?
            .query_row(params![tag], |row| row.get(0))?;
        removed_candidates.remove(&tag_id);
        tx.prepare_cached("INSERT OR IGNORE INTO asset_tags(asset_id, tag_id) VALUES (?1, ?2)")?
            .execute(params![asset_id, tag_id])?;
    }
    tx.execute(
        "UPDATE assets SET tag_count = (
           SELECT COUNT(*) FROM asset_tags WHERE asset_id = ?1
         ) WHERE id = ?1",
        params![asset_id],
    )?;
    for tag_id in removed_candidates {
        vocabulary_changed |= tx.execute(
            "DELETE FROM tags WHERE id = ?1
             AND NOT EXISTS (SELECT 1 FROM asset_tags WHERE tag_id = ?1)",
            params![tag_id],
        )? > 0;
    }
    let canonical = list_asset_tags_in_tx(tx, asset_id)?.unwrap_or_default();
    let before: std::collections::BTreeSet<_> = normalize_tags(existing).into_iter().collect();
    let after: std::collections::BTreeSet<_> =
        normalize_tags(canonical.clone()).into_iter().collect();
    let query_impact = if canonicalized || count_repaired {
        TagQueryImpact::All
    } else {
        TagQueryImpact::Tags {
            changed_tags: before.symmetric_difference(&after).cloned().collect(),
            tag_count_changed: before.len() != after.len(),
        }
    };
    Ok(TagMutationOutcome {
        changed: true,
        vocabulary_changed,
        tags: canonical,
        query_impact,
    })
}

pub(crate) fn bump_library_revision_in_tx(tx: &rusqlite::Transaction<'_>) -> anyhow::Result<i64> {
    bump_scoped_revision(tx, &["general".into()])
}

pub(super) fn is_retryable_sqlite_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<rusqlite::Error>()
            .is_some_and(|error| {
                matches!(
                    error,
                    rusqlite::Error::SqliteFailure(inner, _)
                        if matches!(inner.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
                )
            })
    })
}

pub(crate) fn retry_immediate_transaction<T>(
    conn: &mut Connection,
    mut operation: impl FnMut(&rusqlite::Transaction<'_>) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    const ATTEMPTS: usize = 4;
    for attempt in 0..ATTEMPTS {
        let result = (|| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let output = operation(&tx)?;
            tx.commit()?;
            Ok(output)
        })();
        match result {
            Ok(output) => return Ok(output),
            Err(error) if attempt + 1 < ATTEMPTS && is_retryable_sqlite_error(&error) => {
                thread::sleep(Duration::from_millis(20 * (attempt as u64 + 1)));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

pub fn import_csv_records(
    conn: &mut Connection,
    records: &[CsvImportRecord],
) -> anyhow::Result<crate::models::CsvImportSummary> {
    import_csv_record_batches(conn, |offset| {
        Ok(records.iter().skip(offset).take(512).cloned().collect())
    })
}

pub fn import_csv_record_batches(
    conn: &mut Connection,
    mut read_batch: impl FnMut(usize) -> anyhow::Result<Vec<CsvImportRecord>>,
) -> anyhow::Result<crate::models::CsvImportSummary> {
    retry_immediate_transaction(conn, |tx| {
        let mut rows_applied = 0usize;
        let mut assets_matched = 0usize;
        let mut assets_updated = 0usize;
        let mut find_assets_stmt =
            tx.prepare("SELECT id FROM assets WHERE file_name_key = ?1 ORDER BY id ASC")?;
        let mut sort_keys = DeferredSortKeys::begin(tx)?;

        let mut rows_read = 0;
        loop {
            let records = read_batch(rows_read)?;
            if records.is_empty() {
                break;
            }
            rows_read += records.len();
            for record in records {
                if record.file_name_key.is_empty()
                    || (record.tags.is_empty()
                        && record.favorite.is_none()
                        && record.media_group_key.is_none()
                        && record.media_group_order.is_none())
                {
                    continue;
                }
                let asset_ids = find_assets_stmt
                    .query_map(params![record.file_name_key], |row| row.get::<_, i64>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                if asset_ids.is_empty() {
                    continue;
                }

                rows_applied += 1;
                for asset_id in asset_ids {
                    assets_matched += 1;
                    let existing = list_asset_tags(tx, asset_id)?;
                    let merged = merge_tags(&existing, &record.tags);
                    let normalized_existing = normalize_and_validate_tags(existing)?;
                    let mut changed = false;

                    if merged != normalized_existing {
                        set_asset_tags_in_tx(tx, asset_id, &merged)?;
                        changed = true;
                    }
                    if let Some(next_favorite) = record.favorite {
                        let current_favorite = get_asset_favorite(tx, asset_id)?;
                        if current_favorite != next_favorite {
                            set_asset_favorite(tx, asset_id, next_favorite)?;
                            changed = true;
                        }
                    }
                    if record.media_group_key.is_some() || record.media_group_order.is_some() {
                        let (current_group_key, current_group_order) =
                            get_asset_media_group(tx, asset_id)?;
                        let next_group_key = record
                            .media_group_key
                            .clone()
                            .unwrap_or(current_group_key.clone());
                        let next_group_order =
                            record.media_group_order.unwrap_or(current_group_order);
                        if current_group_key != next_group_key
                            || current_group_order != next_group_order
                        {
                            set_asset_media_group(
                                tx,
                                asset_id,
                                next_group_key.as_deref(),
                                next_group_order,
                            )?;
                            sort_keys.touch(current_group_key.as_deref());
                            sort_keys.touch(next_group_key.as_deref());
                            changed = true;
                        }
                    }
                    assets_updated += usize::from(changed);
                }
            }
        }
        drop(find_assets_stmt);
        sort_keys.finish(tx)?;
        cleanup_orphan_tags(tx)?;
        if assets_updated > 0 {
            bump_library_revision_in_tx(tx)?;
        }
        Ok(crate::models::CsvImportSummary {
            rows_read,
            rows_applied,
            assets_matched,
            assets_updated,
        })
    })
}

pub fn set_asset_tags(conn: &Connection, asset_id: i64, tags: &[String]) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    set_asset_tags_in_tx(&tx, asset_id, tags)?;
    tx.commit()?;
    Ok(())
}

pub fn set_asset_tags_with_revision(
    conn: &mut Connection,
    asset_id: i64,
    tags: &[String],
) -> anyhow::Result<SetAssetTagsSummary> {
    let normalized = normalize_and_validate_tags(tags.to_vec())?;
    retry_immediate_transaction(conn, |tx| {
        let outcome = set_asset_tags_in_tx(tx, asset_id, &normalized)?;
        let revision = bump_tag_revision(tx, &outcome.query_impact)?;
        Ok(SetAssetTagsSummary {
            asset_id,
            changed: outcome.changed,
            vocabulary_changed: outcome.vocabulary_changed,
            tags: outcome.tags,
            query_impact: outcome.query_impact,
            revision,
        })
    })
}

pub fn merge_asset_tags_bulk(
    conn: &Connection,
    asset_ids: &[i64],
    incoming_tags: &[String],
) -> anyhow::Result<(usize, usize)> {
    let normalized_incoming = normalize_and_validate_tags(incoming_tags.to_vec())?;
    if asset_ids.is_empty() || normalized_incoming.is_empty() {
        return Ok((0, 0));
    }
    let tx = conn.unchecked_transaction()?;
    let mut processed = 0;
    let mut updated = 0;
    for asset_id in asset_ids {
        let Some(existing) = list_asset_tags_in_tx(&tx, *asset_id)? else {
            continue;
        };
        processed += 1;
        let merged = merge_tags(&existing, &normalized_incoming);
        let outcome = set_asset_tags_in_tx(&tx, *asset_id, &merged)?;
        updated += usize::from(outcome.changed);
    }
    tx.commit()?;
    Ok((processed, updated))
}

pub fn merge_asset_tags_bulk_with_revision(
    conn: &mut Connection,
    asset_ids: &[i64],
    incoming_tags: &[String],
) -> anyhow::Result<(Vec<AssetTagResult>, i64, TagQueryImpact, bool)> {
    let normalized_incoming = normalize_and_validate_tags(incoming_tags.to_vec())?;
    if asset_ids.is_empty() || normalized_incoming.is_empty() {
        return Ok((
            Vec::new(),
            current_library_revision(conn)?,
            TagQueryImpact::None,
            false,
        ));
    }
    retry_immediate_transaction(conn, |tx| {
        let merged = merge_asset_tags_bulk_in_tx(tx, asset_ids, &normalized_incoming, true)?;
        Ok((
            merged.results,
            merged.revision,
            merged.query_impact,
            merged.vocabulary_changed,
        ))
    })
}

/// Outcome of `merge_asset_tags_bulk_in_tx`.
pub(crate) struct TagMergeOutcome {
    /// Per-asset canonical tags; empty unless requested (a selection-wide
    /// merge would otherwise hold every asset's tag list in memory).
    pub results: Vec<AssetTagResult>,
    pub processed: usize,
    pub updated: usize,
    pub revision: i64,
    pub query_impact: TagQueryImpact,
    pub vocabulary_changed: bool,
}

/// Adds already normalized tags to every existing asset of `asset_ids` and
/// bumps the tag revisions the change affects.
pub(crate) fn merge_asset_tags_bulk_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_ids: &[i64],
    normalized_incoming: &[String],
    keep_results: bool,
) -> anyhow::Result<TagMergeOutcome> {
    let mut results = Vec::new();
    let mut processed = 0;
    let mut updated = 0;
    let mut query_impact = TagQueryImpact::None;
    let mut vocabulary_changed = false;
    for asset_id in asset_ids {
        let Some(existing) = list_asset_tags_in_tx(tx, *asset_id)? else {
            continue;
        };
        let merged = merge_tags(&existing, normalized_incoming);
        let outcome = set_asset_tags_in_tx(tx, *asset_id, &merged)?;
        query_impact.union(outcome.query_impact);
        vocabulary_changed |= outcome.vocabulary_changed;
        processed += 1;
        updated += usize::from(outcome.changed);
        if keep_results {
            results.push(AssetTagResult {
                asset_id: *asset_id,
                changed: outcome.changed,
                tags: outcome.tags,
            });
        }
    }
    let revision = bump_tag_revision(tx, &query_impact)?;
    Ok(TagMergeOutcome {
        results,
        processed,
        updated,
        revision,
        query_impact,
        vocabulary_changed,
    })
}

/// Set-based add-only tag merge for a resolved selection (stage 09). The
/// per-asset merge costs about ten statements per asset (35 s for 1M rows in
/// one transaction); this one stages the IDs in a temporary table and runs one
/// insert per tag, so the write lock is held for seconds instead of a minute.
/// Unlike `merge_asset_tags_bulk_in_tx` it does not canonicalize legacy tag
/// rows of each asset (that repair stays on the per-asset paths), but
/// `tag_count` is recounted exactly for every asset that gained a tag.
/// `normalized_incoming` must already be normalized and validated.
pub(crate) fn merge_tags_into_selection_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_ids: &[i64],
    normalized_incoming: &[String],
) -> anyhow::Result<TagMergeOutcome> {
    tx.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS selection_merge_ids(
           id INTEGER PRIMARY KEY, added INTEGER NOT NULL DEFAULT 0
         );
         DELETE FROM temp.selection_merge_ids;",
    )?;
    {
        let mut insert = tx.prepare_cached(
            "INSERT OR IGNORE INTO temp.selection_merge_ids(id)
             SELECT id FROM assets WHERE id = ?1",
        )?;
        for id in asset_ids {
            insert.execute([id])?;
        }
    }
    let processed: i64 =
        tx.query_row("SELECT COUNT(*) FROM temp.selection_merge_ids", [], |row| {
            row.get(0)
        })?;
    let mut vocabulary_changed = false;
    let mut changed_tags = Vec::new();
    for tag in normalized_incoming {
        vocabulary_changed |= tx.execute(
            "INSERT INTO tags(name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            params![tag],
        )? > 0;
        let tag_id: i64 = tx.query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
            params![tag],
            |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE temp.selection_merge_ids SET added = 1
             WHERE NOT EXISTS (
               SELECT 1 FROM asset_tags WHERE asset_id = selection_merge_ids.id AND tag_id = ?1
             )",
            params![tag_id],
        )?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO asset_tags(asset_id, tag_id)
             SELECT id, ?1 FROM temp.selection_merge_ids",
            params![tag_id],
        )?;
        if inserted > 0 {
            changed_tags.push(tag.clone());
        }
    }
    let updated = tx.execute(
        "UPDATE assets SET tag_count = (
           SELECT COUNT(*) FROM asset_tags WHERE asset_id = assets.id
         )
         WHERE id IN (SELECT id FROM temp.selection_merge_ids WHERE added = 1)",
        [],
    )?;
    tx.execute("DELETE FROM temp.selection_merge_ids", [])?;
    let query_impact = if changed_tags.is_empty() {
        TagQueryImpact::None
    } else {
        changed_tags.sort();
        changed_tags.dedup();
        TagQueryImpact::Tags {
            changed_tags,
            tag_count_changed: true,
        }
    };
    let revision = bump_tag_revision(tx, &query_impact)?;
    Ok(TagMergeOutcome {
        results: Vec::new(),
        processed: processed as usize,
        updated,
        revision,
        query_impact,
        vocabulary_changed,
    })
}

/// Runs `operation` in one IMMEDIATE transaction on a user-write connection,
/// but only when `is_current` still holds inside that transaction. The write
/// lock is held from the check to the commit, so nothing can change the
/// checked state in between; `Ok(None)` means nothing was written.
pub fn guarded_user_write<T>(
    conn: &mut Connection,
    is_current: impl Fn(&rusqlite::Transaction<'_>) -> anyhow::Result<bool>,
    mut operation: impl FnMut(&rusqlite::Transaction<'_>) -> anyhow::Result<T>,
) -> anyhow::Result<Option<T>> {
    retry_immediate_transaction(conn, |tx| {
        if !is_current(tx)? {
            return Ok(None);
        }
        operation(tx).map(Some)
    })
}

pub fn bump_library_revision(conn: &Connection) -> anyhow::Result<i64> {
    let tx = conn.unchecked_transaction()?;
    let revision = bump_library_revision_in_tx(&tx)?;
    tx.commit()?;
    Ok(revision)
}
