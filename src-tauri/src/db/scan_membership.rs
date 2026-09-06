use super::*;

pub(super) fn root_descendant_like_pattern(root: &str) -> String {
    let separator = if root.contains('\\') {
        '\\'
    } else if root.contains('/') {
        '/'
    } else {
        std::path::MAIN_SEPARATOR
    };
    let root = root.trim_end_matches(['\\', '/']);
    let escaped = root
        .replace('^', "^^")
        .replace('%', "^%")
        .replace('_', "^_");
    format!("{escaped}{separator}%")
}

pub(super) fn upsert_asset_with_fingerprint(
    conn: &Connection,
    asset: &NewAsset,
    fingerprint_mtime_ns: i64,
) -> anyhow::Result<()> {
    let file_name = extract_file_name(&asset.path);
    let file_name_key = canonical_key(&file_name);
    conn.execute(
        "
        INSERT INTO assets(path, file_name, file_name_key, kind, size_bytes, modified_at, width, height, duration_ms, thumb_path, fingerprint_mtime_ns, indexed_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, unixepoch())
        ON CONFLICT(path) DO UPDATE SET
          file_name=excluded.file_name,
          file_name_key=excluded.file_name_key,
          kind=excluded.kind,
          size_bytes=excluded.size_bytes,
          modified_at=excluded.modified_at,
          width=excluded.width,
          height=excluded.height,
          duration_ms=excluded.duration_ms,
          thumb_path=CASE
            WHEN assets.modified_at = excluded.modified_at AND assets.size_bytes = excluded.size_bytes AND assets.kind = excluded.kind AND assets.fingerprint_mtime_ns = excluded.fingerprint_mtime_ns THEN assets.thumb_path
            ELSE excluded.thumb_path
          END,
          record_version=CASE WHEN
            assets.kind IS NOT excluded.kind OR
            assets.size_bytes IS NOT excluded.size_bytes OR
            assets.modified_at IS NOT excluded.modified_at OR
            assets.fingerprint_mtime_ns IS NOT excluded.fingerprint_mtime_ns
          THEN assets.record_version + 1 ELSE assets.record_version END,
          fingerprint_mtime_ns=excluded.fingerprint_mtime_ns,
          indexed_at=unixepoch();
        ",
        params![
            asset.path,
            file_name,
            file_name_key,
            asset.kind,
            asset.size_bytes,
            asset.modified_at,
            asset.width,
            asset.height,
            asset.duration_ms,
            asset.thumb_path,
            fingerprint_mtime_ns
        ],
    )?;

    Ok(())
}

pub fn delete_stale_assets_by_prefix(
    conn: &Connection,
    prefix: &str,
    scan_started_at: i64,
) -> anyhow::Result<usize> {
    let like_pattern = root_descendant_like_pattern(prefix);
    let affected = conn.execute(
        "DELETE FROM assets WHERE (path = ?1 OR path LIKE ?2 ESCAPE '^') AND indexed_at < ?3",
        params![prefix, like_pattern, scan_started_at],
    )?;
    Ok(affected)
}

pub fn list_scan_root_settings(conn: &Connection) -> anyhow::Result<Vec<crate::models::ScanRoot>> {
    let mut stmt =
        conn.prepare("SELECT path, auto_scan_on_startup FROM scan_roots ORDER BY path ASC")?;
    let roots = stmt
        .query_map([], |row| {
            Ok(crate::models::ScanRoot {
                path: row.get(0)?,
                auto_scan_on_startup: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(roots)
}

pub fn set_scan_root_auto_scan(conn: &Connection, path: &str, enabled: bool) -> anyhow::Result<()> {
    let changed = conn.execute(
        "UPDATE scan_roots SET auto_scan_on_startup = ?2 WHERE path = ?1",
        params![path, enabled],
    )?;
    anyhow::ensure!(changed == 1, "Scan folder is not registered");
    Ok(())
}

pub fn list_scan_roots(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut roots = Vec::new();
    let mut stmt = conn.prepare("SELECT path FROM scan_roots ORDER BY path ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        roots.push(row?);
    }
    Ok(roots)
}

pub fn add_scan_root(conn: &Connection, path: &str) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO scan_roots(path) VALUES (?1) ON CONFLICT(path) DO NOTHING",
        params![path],
    )?;
    Ok(())
}

pub fn remove_scan_root(conn: &Connection, path: &str) -> anyhow::Result<()> {
    conn.execute("DELETE FROM scan_roots WHERE path = ?1", params![path])?;
    Ok(())
}

pub fn remove_scan_root_and_orphan_assets(
    conn: &Connection,
    path: &str,
) -> anyhow::Result<(usize, Vec<String>)> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM scan_roots WHERE path = ?1", params![path])?;
    let mut thumbs = Vec::new();
    {
        let mut stmt = tx.prepare(
            "SELECT thumb_path FROM assets
             WHERE thumb_path IS NOT NULL
               AND NOT EXISTS (
                 SELECT 1 FROM asset_scan_roots ar WHERE ar.asset_id = assets.id
               )",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            thumbs.push(row?);
        }
    }
    let removed = tx.execute(
        "DELETE FROM assets WHERE NOT EXISTS (
           SELECT 1 FROM asset_scan_roots ar WHERE ar.asset_id = assets.id
         )",
        [],
    )?;
    tx.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id NOT IN (SELECT id FROM assets)",
        [],
    )?;
    cleanup_orphan_tags(&tx)?;
    if removed > 0 {
        bump_library_revision_in_tx(&tx)?;
    }
    tx.commit()?;
    Ok((removed, thumbs))
}

pub fn list_asset_fingerprints_for_root(
    conn: &Connection,
    root: &str,
) -> anyhow::Result<HashMap<String, ExistingAssetFingerprint>> {
    let like_pattern = root_descendant_like_pattern(root);
    let mut out = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT id, path, kind, size_bytes, fingerprint_mtime_ns
         FROM assets WHERE path = ?1 OR path LIKE ?2 ESCAPE '^'",
    )?;
    let rows = stmt.query_map(params![root, like_pattern], |row| {
        Ok(ExistingAssetFingerprint {
            id: row.get(0)?,
            path: row.get(1)?,
            kind: row.get(2)?,
            size_bytes: row.get(3)?,
            modified_at_ns: row.get(4)?,
        })
    })?;
    for row in rows {
        let fingerprint = row?;
        out.insert(fingerprint.path.clone(), fingerprint);
    }
    Ok(out)
}

pub fn list_asset_fingerprints_by_paths(
    conn: &Connection,
    paths: &[String],
) -> anyhow::Result<HashMap<String, ExistingAssetFingerprint>> {
    if paths.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = vec!["?"; paths.len()].join(",");
    let sql = format!(
        "SELECT id, path, kind, size_bytes, fingerprint_mtime_ns
         FROM assets WHERE path IN ({placeholders})"
    );
    let mut out = HashMap::with_capacity(paths.len());
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(paths.iter()), |row| {
        Ok(ExistingAssetFingerprint {
            id: row.get(0)?,
            path: row.get(1)?,
            kind: row.get(2)?,
            size_bytes: row.get(3)?,
            modified_at_ns: row.get(4)?,
        })
    })?;
    for row in rows {
        let fingerprint = row?;
        out.insert(fingerprint.path.clone(), fingerprint);
    }
    Ok(out)
}

pub fn upsert_scanned_asset(
    conn: &Connection,
    asset: &NewAsset,
    fingerprint_mtime_ns: i64,
    root: &str,
    generation: i64,
) -> anyhow::Result<()> {
    upsert_asset_with_fingerprint(conn, asset, fingerprint_mtime_ns)?;
    conn.execute(
        "INSERT INTO asset_scan_roots(asset_id, root_path, last_seen_generation)
         SELECT id, ?1, ?2 FROM assets WHERE path = ?3
         ON CONFLICT(asset_id, root_path) DO UPDATE SET
           last_seen_generation = excluded.last_seen_generation",
        params![root, generation, asset.path],
    )?;
    Ok(())
}

pub fn touch_scan_root_assets(
    conn: &Connection,
    asset_ids: &[i64],
    root: &str,
    generation: i64,
) -> anyhow::Result<()> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare(
        "INSERT INTO asset_scan_roots(asset_id, root_path, last_seen_generation)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(asset_id, root_path) DO UPDATE SET
           last_seen_generation = excluded.last_seen_generation",
    )?;
    for asset_id in asset_ids {
        stmt.execute(params![asset_id, root, generation])?;
    }
    drop(stmt);
    tx.commit()?;
    Ok(())
}

/// Prunes stale mappings only after the caller has confirmed that discovery for
/// this exact root and generation completed without warnings.
pub fn prune_completed_scan_root_generation(
    conn: &Connection,
    root: &str,
    generation: i64,
) -> anyhow::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM asset_scan_roots WHERE root_path = ?1 AND last_seen_generation != ?2",
        params![root, generation],
    )?;
    let removed = tx.execute(
        "DELETE FROM assets
         WHERE NOT EXISTS (
           SELECT 1 FROM asset_scan_roots ar WHERE ar.asset_id = assets.id
         )",
        [],
    )?;
    tx.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id NOT IN (SELECT id FROM assets)",
        [],
    )?;
    cleanup_orphan_tags(&tx)?;
    if removed > 0 {
        bump_library_revision_in_tx(&tx)?;
    }
    tx.commit()?;
    Ok(removed)
}
