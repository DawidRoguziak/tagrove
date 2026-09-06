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
    let tx = conn.unchecked_transaction()?;
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
        tx.execute(
            "UPDATE assets SET is_favorite = ?1 WHERE id = ?2",
            params![if is_favorite { 1 } else { 0 }, asset_id],
        )?;
        bump_library_revision_in_tx(tx)
    })
}

pub fn toggle_assets_favorite_bulk(
    conn: &mut Connection,
    asset_ids: &[i64],
) -> anyhow::Result<crate::models::BulkFavoriteSummary> {
    retry_immediate_transaction(conn, |tx| {
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
            bump_library_revision_in_tx(tx)?
        };
        Ok(crate::models::BulkFavoriteSummary {
            processed_asset_ids,
            is_favorite,
            revision,
        })
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
        let mut processed_assets = Vec::new();
        let mut updated_assets = 0usize;

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
            updated_assets += 1;
        }

        drop(update_stmt);
        drop(get_current_stmt);

        if updated_assets > 0 {
            bump_library_revision_in_tx(tx)?;
        }
        Ok((processed_assets, updated_assets))
    })
}

pub(super) fn list_asset_tags_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_id: i64,
) -> anyhow::Result<Option<Vec<String>>> {
    let exists = tx
        .query_row(
            "SELECT 1 FROM assets WHERE id = ?1",
            params![asset_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(None);
    }
    let mut stmt = tx.prepare(
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
        let mut stmt = tx.prepare(
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
            let mut stmt = tx.prepare(
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

        let keeper_name: String = tx.query_row(
            "SELECT name FROM tags WHERE id = ?1",
            params![keeper_id],
            |row| row.get(0),
        )?;
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

pub(crate) fn set_asset_tags_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_id: i64,
    tags: &[String],
) -> anyhow::Result<(bool, Vec<String>)> {
    let normalized = normalize_and_validate_tags(tags.to_vec())?;
    let existing = list_asset_tags_in_tx(tx, asset_id)?
        .ok_or_else(|| anyhow::anyhow!("Asset {asset_id} not found"))?;
    let canonicalized = canonicalize_tag_rows_in_tx(tx, asset_id)?;
    let actual_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM asset_tags WHERE asset_id = ?1",
        params![asset_id],
        |row| row.get(0),
    )?;
    let stored_count: i64 = tx.query_row(
        "SELECT tag_count FROM assets WHERE id = ?1",
        params![asset_id],
        |row| row.get(0),
    )?;
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
        return Ok((canonicalized || count_repaired, comparable));
    }

    tx.execute(
        "DELETE FROM asset_tags WHERE asset_id = ?1",
        params![asset_id],
    )?;
    for tag in &normalized {
        tx.execute(
            "INSERT INTO tags(name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            params![tag],
        )?;
        let tag_id: i64 = tx.query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
            params![tag],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO asset_tags(asset_id, tag_id) VALUES (?1, ?2)",
            params![asset_id, tag_id],
        )?;
    }
    tx.execute(
        "UPDATE assets SET tag_count = (
           SELECT COUNT(*) FROM asset_tags WHERE asset_id = ?1
         ) WHERE id = ?1",
        params![asset_id],
    )?;
    let canonical = list_asset_tags_in_tx(tx, asset_id)?.unwrap_or_default();
    Ok((true, canonical))
}

pub(crate) fn bump_library_revision_in_tx(tx: &rusqlite::Transaction<'_>) -> anyhow::Result<i64> {
    tx.execute(
        "UPDATE library_metadata SET value = value + 1 WHERE key = 'revision'",
        [],
    )?;
    Ok(tx.query_row(
        "SELECT value FROM library_metadata WHERE key = 'revision'",
        [],
        |row| row.get(0),
    )?)
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
                            changed = true;
                        }
                    }
                    assets_updated += usize::from(changed);
                }
            }
        }
        drop(find_assets_stmt);
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
    cleanup_orphan_tags(&tx)?;
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
        let (changed, canonical_tags) = set_asset_tags_in_tx(tx, asset_id, &normalized)?;
        cleanup_orphan_tags(tx)?;
        let revision = if changed {
            bump_library_revision_in_tx(tx)?
        } else {
            tx.query_row(
                "SELECT value FROM library_metadata WHERE key = 'revision'",
                [],
                |row| row.get(0),
            )?
        };
        Ok(SetAssetTagsSummary {
            asset_id,
            changed,
            tags: canonical_tags,
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
        let (changed, _) = set_asset_tags_in_tx(&tx, *asset_id, &merged)?;
        updated += usize::from(changed);
    }
    cleanup_orphan_tags(&tx)?;
    tx.commit()?;
    Ok((processed, updated))
}

pub fn merge_asset_tags_bulk_with_revision(
    conn: &mut Connection,
    asset_ids: &[i64],
    incoming_tags: &[String],
) -> anyhow::Result<(Vec<AssetTagResult>, i64)> {
    let normalized_incoming = normalize_and_validate_tags(incoming_tags.to_vec())?;
    if asset_ids.is_empty() || normalized_incoming.is_empty() {
        return Ok((Vec::new(), current_library_revision(conn)?));
    }
    retry_immediate_transaction(conn, |tx| {
        let mut results = Vec::new();
        for asset_id in asset_ids {
            let Some(existing) = list_asset_tags_in_tx(tx, *asset_id)? else {
                continue;
            };
            let merged = merge_tags(&existing, &normalized_incoming);
            let (changed, tags) = set_asset_tags_in_tx(tx, *asset_id, &merged)?;
            results.push(AssetTagResult {
                asset_id: *asset_id,
                changed,
                tags,
            });
        }
        cleanup_orphan_tags(tx)?;
        let changed = results.iter().any(|result| result.changed);
        let revision = if changed {
            bump_library_revision_in_tx(tx)?
        } else {
            tx.query_row(
                "SELECT value FROM library_metadata WHERE key = 'revision'",
                [],
                |row| row.get(0),
            )?
        };
        Ok((results, revision))
    })
}

pub fn bump_library_revision(conn: &Connection) -> anyhow::Result<i64> {
    conn.execute(
        "UPDATE library_metadata SET value = value + 1 WHERE key = 'revision'",
        [],
    )?;
    current_library_revision(conn)
}
