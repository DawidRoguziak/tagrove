use super::*;

pub fn ensure_no_pending_file_operations(conn: &Connection) -> anyhow::Result<()> {
    let has_pending_table = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'pending_file_operations')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if has_pending_table {
        let pending =
            conn.query_row("SELECT COUNT(*) FROM pending_file_operations", [], |row| {
                row.get::<_, i64>(0)
            })?;
        anyhow::ensure!(
            pending == 0,
            "database contains pending local file operations"
        );
    }
    Ok(())
}

pub fn list_backup_asset_paths(conn: &Connection) -> anyhow::Result<Vec<BackupAssetPath>> {
    let mut stmt = conn.prepare("SELECT id, path, thumb_path FROM assets ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok(BackupAssetPath {
            id: row.get(0)?,
            path: row.get(1)?,
            thumb_path: row.get(2)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_backup_asset_root_mappings(conn: &Connection) -> anyhow::Result<Vec<(i64, String)>> {
    let mut stmt = conn
        .prepare("SELECT asset_id, root_path FROM asset_scan_roots ORDER BY asset_id, root_path")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) fn extract_file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub fn get_asset_path_and_thumb_by_id(
    conn: &Connection,
    asset_id: i64,
) -> anyhow::Result<Option<(String, Option<String>)>> {
    conn.query_row(
        "SELECT path, thumb_path FROM assets WHERE id = ?1",
        params![asset_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    )
    .optional()
    .map_err(Into::into)
}

pub fn asset_paths_for_name(conn: &Connection, key: &str) -> anyhow::Result<Vec<AssetPathRow>> {
    let mut stmt = conn.prepare("SELECT id, path FROM assets WHERE file_name_key = ?1")?;
    let rows = stmt.query_map([key], |row| {
        Ok(AssetPathRow {
            id: row.get(0)?,
            path: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn assigned_asset_roots(
    conn: &Connection,
    asset_id: i64,
) -> anyhow::Result<Vec<(i64, String)>> {
    let mut stmt =
        conn.prepare("SELECT asset_id, root_path FROM asset_scan_roots WHERE asset_id = ?1")?;
    let rows = stmt.query_map([asset_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_file_mutation_asset(
    conn: &Connection,
    asset_id: i64,
) -> anyhow::Result<Option<FileMutationAsset>> {
    conn.query_row(
        "SELECT id, path, thumb_path, record_version FROM assets WHERE id = ?1",
        params![asset_id],
        |row| {
            Ok(FileMutationAsset {
                id: row.get(0)?,
                path: row.get(1)?,
                thumb_path: row.get(2)?,
                record_version: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn list_pending_file_operations(
    conn: &Connection,
) -> anyhow::Result<Vec<PendingFileOperation>> {
    let mut statement = conn.prepare(
        "SELECT operation_id, asset_id, action, original_path, staging_path, final_path, committed
         FROM pending_file_operations ORDER BY created_at, operation_id, asset_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(PendingFileOperation {
            operation_id: row.get(0)?,
            asset_id: row.get(1)?,
            action: row.get(2)?,
            original_path: row.get(3)?,
            staging_path: row.get(4)?,
            final_path: row.get(5)?,
            committed: row.get(6)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_duplicate_file_name_counts(
    conn: &Connection,
) -> anyhow::Result<Vec<DuplicateFileNameCount>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "
        SELECT
          file_name_key,
          MIN(file_name) AS file_name_display,
          COUNT(*) AS asset_count
        FROM assets
        WHERE trim(file_name) <> ''
        GROUP BY file_name_key
        HAVING COUNT(*) > 1
        ORDER BY asset_count DESC, file_name_key ASC
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(DuplicateFileNameCount {
            file_name_key: row.get(0)?,
            file_name_display: row.get(1)?,
            asset_count: row.get(2)?,
        })
    })?;

    for row in rows {
        out.push(row?);
    }

    Ok(out)
}

pub fn list_duplicate_assets_by_file_name_key(
    conn: &Connection,
    file_name_key: &str,
) -> anyhow::Result<Vec<DuplicateAssetRow>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "
        SELECT id, path
        FROM assets
        WHERE file_name_key = ?1
        ORDER BY modified_at DESC, id DESC
        ",
    )?;
    let rows = stmt.query_map(params![file_name_key], |row| {
        Ok(DuplicateAssetRow {
            id: row.get(0)?,
            path: row.get(1)?,
        })
    })?;

    for row in rows {
        out.push(row?);
    }

    Ok(out)
}

pub fn rename_asset_file_by_id(
    conn: &Connection,
    asset_id: i64,
    new_path: &str,
    new_file_name: &str,
) -> anyhow::Result<Option<(String, Option<String>)>> {
    let asset = conn
        .query_row(
            "SELECT path, thumb_path FROM assets WHERE id = ?1",
            params![asset_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?;

    let Some((old_path, old_thumb_path)) = asset else {
        return Ok(None);
    };

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE assets SET path = ?1, file_name = ?2, file_name_key = ?3,
          thumb_path = NULL, record_version = record_version + 1 WHERE id = ?4",
        params![
            new_path,
            new_file_name,
            canonical_key(new_file_name),
            asset_id
        ],
    )?;
    tx.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id = ?1",
        params![asset_id],
    )?;
    tx.commit()?;

    Ok(Some((old_path, old_thumb_path)))
}

#[allow(dead_code)]
pub fn list_assets(
    conn: &Connection,
    offset: i64,
    limit: i64,
    tags_and: &[String],
    tags_not: &[String],
    kind: Option<&str>,
    favorites_only: bool,
) -> anyhow::Result<AssetPage> {
    list_assets_with_meta(
        conn,
        offset,
        limit,
        tags_and,
        tags_not,
        kind,
        favorites_only,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn list_assets_with_meta(
    conn: &Connection,
    offset: i64,
    limit: i64,
    tags_and: &[String],
    tags_not: &[String],
    kind: Option<&str>,
    favorites_only: bool,
    meta_filter: Option<&AssetMetaFilter>,
) -> anyhow::Result<AssetPage> {
    let kind_filter = kind.map(str::to_string);
    let positive_tag_count_filter = match meta_filter {
        Some(AssetMetaFilter::HasNoTags { tag_count }) if *tag_count > 0 => Some(*tag_count),
        _ => None,
    };
    let normalized_group_name = match meta_filter {
        Some(AssetMetaFilter::GroupName { group_name }) => Some(group_name.trim().to_lowercase()),
        _ => None,
    };
    let mut where_clauses = Vec::<String>::new();

    if kind_filter.is_some() {
        where_clauses.push("a.kind = ?".to_string());
    }

    if favorites_only {
        where_clauses.push("a.is_favorite = 1".to_string());
    }

    if !tags_and.is_empty() {
        let include_placeholders = vec!["?"; tags_and.len()].join(",");
        where_clauses.push(format!(
            "a.id IN (
              SELECT at.asset_id
              FROM asset_tags at
              JOIN tags t ON t.id = at.tag_id
              WHERE lower(t.name) IN ({})
              GROUP BY at.asset_id
              HAVING COUNT(DISTINCT lower(t.name)) = ?
            )",
            include_placeholders
        ));
    }

    if !tags_not.is_empty() {
        let exclude_placeholders = vec!["?"; tags_not.len()].join(",");
        where_clauses.push(format!(
            "NOT EXISTS (
              SELECT 1
              FROM asset_tags atn
              JOIN tags tn ON tn.id = atn.tag_id
              WHERE atn.asset_id = a.id
                AND lower(tn.name) IN ({})
            )",
            exclude_placeholders
        ));
    }

    match meta_filter {
        Some(AssetMetaFilter::HasNoTags { tag_count }) if *tag_count == 0 => {
            where_clauses.push(
                "NOT EXISTS (SELECT 1 FROM asset_tags atm WHERE atm.asset_id = a.id)".to_string(),
            );
        }
        Some(AssetMetaFilter::HasNoTags { .. }) => {
            where_clauses.push(
                "(SELECT COUNT(*) FROM asset_tags atm WHERE atm.asset_id = a.id) = ?".to_string(),
            );
        }
        Some(AssetMetaFilter::GroupName { .. }) => {
            where_clauses.push(
                "a.media_group_key IS NOT NULL AND trim(a.media_group_key) <> '' AND lower(trim(a.media_group_key)) = ?"
                    .to_string(),
            );
        }
        None => {}
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let total_sql = format!("SELECT COUNT(*) FROM assets a{}", where_sql);
    let include_count = tags_and.len() as i64;

    let mut total_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
    if let Some(kind_value) = kind_filter.as_ref() {
        total_params.push(kind_value);
    }
    for tag in tags_and {
        total_params.push(tag as &dyn rusqlite::ToSql);
    }
    if !tags_and.is_empty() {
        total_params.push(&include_count);
    }
    for tag in tags_not {
        total_params.push(tag as &dyn rusqlite::ToSql);
    }
    if let Some(tag_count) = positive_tag_count_filter.as_ref() {
        total_params.push(tag_count);
    }
    if let Some(group_name) = normalized_group_name.as_ref() {
        total_params.push(group_name);
    }

    let total = conn.query_row(&total_sql, total_params.as_slice(), |row| row.get(0))?;

    let list_sql = format!(
        "
        WITH filtered_assets AS (
          SELECT
            a.id,
            a.path,
            a.kind,
            a.size_bytes,
            a.modified_at,
            a.width,
            a.height,
            a.duration_ms,
            a.thumb_path,
            a.is_favorite,
            a.media_group_key,
            a.media_group_order,
            COALESCE((
              SELECT GROUP_CONCAT(t2.name, char(31))
              FROM tags t2
              JOIN asset_tags at2 ON at2.tag_id = t2.id
              WHERE at2.asset_id = a.id
            ), '') AS tags_joined,
            CASE
              WHEN a.media_group_key IS NOT NULL AND trim(a.media_group_key) <> '' THEN 'group:' || a.media_group_key
              ELSE 'asset:' || printf('%020lld', a.id)
            END AS sort_bucket
          FROM assets a
          {}
        ),
        bucket_stats AS (
          SELECT
            fa.sort_bucket,
            MAX(fa.modified_at) AS bucket_modified_at,
            MAX(fa.id) AS bucket_max_id
          FROM filtered_assets fa
          GROUP BY fa.sort_bucket
        )
        SELECT
          fa.id,
          fa.path,
          fa.kind,
          fa.size_bytes,
          fa.modified_at,
          fa.width,
          fa.height,
          fa.duration_ms,
          fa.thumb_path,
          fa.is_favorite,
          fa.media_group_key,
          fa.media_group_order,
          fa.tags_joined
        FROM filtered_assets fa
        JOIN bucket_stats bs ON bs.sort_bucket = fa.sort_bucket
        ORDER BY
          bs.bucket_modified_at DESC,
          bs.bucket_max_id DESC,
          CASE
            WHEN fa.media_group_key IS NOT NULL
              AND trim(fa.media_group_key) <> ''
              AND fa.media_group_order IS NULL
            THEN 1
            ELSE 0
          END ASC,
          fa.media_group_order ASC,
          fa.modified_at DESC,
          fa.id DESC
        LIMIT ? OFFSET ?
        ",
        where_sql
    );

    let mut list_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
    if let Some(kind_value) = kind_filter.as_ref() {
        list_params.push(kind_value);
    }
    for tag in tags_and {
        list_params.push(tag as &dyn rusqlite::ToSql);
    }
    if !tags_and.is_empty() {
        list_params.push(&include_count);
    }
    for tag in tags_not {
        list_params.push(tag as &dyn rusqlite::ToSql);
    }
    if let Some(tag_count) = positive_tag_count_filter.as_ref() {
        list_params.push(tag_count);
    }
    if let Some(group_name) = normalized_group_name.as_ref() {
        list_params.push(group_name);
    }
    list_params.push(&limit);
    list_params.push(&offset);

    let mut items = Vec::<Asset>::new();
    let mut stmt = conn.prepare(&list_sql)?;
    let mapped = stmt.query_map(list_params.as_slice(), parse_asset_row)?;
    for row in mapped {
        items.push(row?);
    }

    Ok(AssetPage { items, total })
}

pub(super) fn parse_asset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    let tags_joined: String = row.get(12)?;
    let tags = tags_joined
        .split('\u{1f}')
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    Ok(Asset {
        id: row.get(0)?,
        path: row.get(1)?,
        kind: row.get(2)?,
        size_bytes: row.get(3)?,
        modified_at: row.get(4)?,
        width: row.get(5)?,
        height: row.get(6)?,
        duration_ms: row.get(7)?,
        thumb_path: row.get(8)?,
        is_favorite: row.get::<_, i64>(9)? != 0,
        media_group_key: row.get(10)?,
        media_group_order: row.get(11)?,
        tags,
    })
}

pub fn list_duplicate_groups(conn: &Connection) -> anyhow::Result<Vec<DuplicateGroup>> {
    let mut groups = Vec::<DuplicateGroup>::new();
    let mut stmt = conn.prepare(
        "
        WITH duplicate_names AS (
          SELECT file_name_key, MIN(file_name) AS file_name_display, COUNT(*) AS asset_count
          FROM assets
          WHERE file_name_key <> ''
          GROUP BY file_name_key
          HAVING COUNT(*) > 1
        )
        SELECT d.file_name_key, d.file_name_display, a.id, a.path,
               a.record_version, a.size_bytes
        FROM duplicate_names d
        JOIN assets a ON a.file_name_key = d.file_name_key
        ORDER BY d.asset_count DESC, d.file_name_key ASC, a.modified_at DESC, a.id DESC
        ",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    let mut current_key = String::new();
    for row in rows {
        let (key, display, id, path, record_version, size_bytes) = row?;
        if current_key != key {
            current_key = key;
            groups.push(DuplicateGroup {
                file_name: display,
                assets: Vec::new(),
            });
        }
        if let Some(group) = groups.last_mut() {
            group.assets.push(DuplicateAsset {
                id,
                path,
                record_version,
                size_bytes,
            });
        }
    }
    Ok(groups)
}

/// Library-wide assignment counts, captured once by desktop startup.
pub fn list_popular_tags(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t
         JOIN asset_tags at ON at.tag_id = t.id
         GROUP BY t.id
         ORDER BY COUNT(*) DESC, t.name ASC
         LIMIT 10",
    )?;
    let tags = stmt.query_map([], |row| row.get(0))?;
    Ok(tags.collect::<rusqlite::Result<Vec<String>>>()?)
}

pub fn list_tags_page(
    conn: &Connection,
    query: &str,
    offset: i64,
    limit: i64,
) -> anyhow::Result<TagListPage> {
    let safe_offset = offset.max(0);
    let safe_limit = limit.clamp(1, 200);
    let trimmed_query = query.trim();

    let total = if trimmed_query.is_empty() {
        conn.query_row("SELECT COUNT(*) FROM tags", [], |row| row.get::<_, i64>(0))?
    } else {
        let q = format!("%{}%", trimmed_query.to_lowercase());
        conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE lower(name) LIKE ?1",
            params![q],
            |row| row.get::<_, i64>(0),
        )?
    };

    let mut items = Vec::new();
    if trimmed_query.is_empty() {
        let mut stmt =
            conn.prepare("SELECT name FROM tags ORDER BY name ASC LIMIT ?1 OFFSET ?2")?;
        let rows = stmt.query_map(params![safe_limit, safe_offset], |row| {
            row.get::<_, String>(0)
        })?;
        for row in rows {
            items.push(row?);
        }
    } else {
        let q = format!("%{}%", trimmed_query.to_lowercase());
        let mut stmt = conn.prepare(
            "SELECT name FROM tags WHERE lower(name) LIKE ?1 ORDER BY name ASC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![q, safe_limit, safe_offset], |row| {
            row.get::<_, String>(0)
        })?;
        for row in rows {
            items.push(row?);
        }
    }

    Ok(TagListPage { items, total })
}

pub fn list_assets_for_csv_export(conn: &Connection) -> anyhow::Result<Vec<CsvAssetRow>> {
    let mut out = Vec::new();
    for_each_asset_for_csv_export(conn, |row| {
        out.push(row);
        Ok(())
    })?;
    Ok(out)
}

pub fn for_each_asset_for_csv_export(
    conn: &Connection,
    mut visitor: impl FnMut(CsvAssetRow) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "
        SELECT
          a.path,
          a.is_favorite,
          a.media_group_key,
          a.media_group_order,
          COALESCE((
            SELECT GROUP_CONCAT(t.name, char(31))
            FROM tags t
            JOIN asset_tags at ON at.tag_id = t.id
            WHERE at.asset_id = a.id
          ), '') AS tags_joined
        FROM assets a
        ORDER BY a.id ASC
        ",
    )?;

    let rows = stmt.query_map([], |row| {
        let tags_joined: String = row.get(4)?;
        let tags = tags_joined
            .split('\u{1f}')
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();

        Ok(CsvAssetRow {
            path: row.get(0)?,
            is_favorite: row.get::<_, i64>(1)? != 0,
            media_group_key: row.get(2)?,
            media_group_order: row.get(3)?,
            tags,
        })
    })?;

    for row in rows {
        visitor(row?)?;
    }
    Ok(())
}

pub fn list_asset_paths(conn: &Connection) -> anyhow::Result<Vec<AssetPathRow>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare("SELECT id, path FROM assets ORDER BY id ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(AssetPathRow {
            id: row.get(0)?,
            path: row.get(1)?,
        })
    })?;

    for row in rows {
        out.push(row?);
    }

    Ok(out)
}

pub fn list_asset_tags(conn: &Connection, asset_id: i64) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "
        SELECT t.name
        FROM tags t
        JOIN asset_tags at ON at.tag_id = t.id
        WHERE at.asset_id = ?1
        ORDER BY t.name ASC
        ",
    )?;
    let rows = stmt.query_map(params![asset_id], |row| row.get::<_, String>(0))?;

    for row in rows {
        out.push(row?);
    }

    Ok(out)
}

pub fn get_asset_favorite(conn: &Connection, asset_id: i64) -> anyhow::Result<bool> {
    let is_favorite: i64 = conn.query_row(
        "SELECT is_favorite FROM assets WHERE id = ?1",
        params![asset_id],
        |row| row.get(0),
    )?;
    Ok(is_favorite != 0)
}

pub fn get_asset_media_group(
    conn: &Connection,
    asset_id: i64,
) -> anyhow::Result<(Option<String>, Option<f64>)> {
    let tuple = conn.query_row(
        "SELECT media_group_key, media_group_order FROM assets WHERE id = ?1",
        params![asset_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(tuple)
}

pub fn get_video_asset_source(
    conn: &Connection,
    asset_id: i64,
) -> anyhow::Result<Option<VideoAssetSource>> {
    let mut statement = conn.prepare(
        "
        SELECT a.path, a.kind, ar.root_path
        FROM assets a
        LEFT JOIN asset_scan_roots ar ON ar.asset_id = a.id
        WHERE a.id = ?1
        ORDER BY ar.root_path
        ",
    )?;
    let mut rows = statement.query(params![asset_id])?;
    let Some(first) = rows.next()? else {
        return Ok(None);
    };
    let path = first.get(0)?;
    let kind = first.get(1)?;
    let mut scan_roots: Vec<String> = first.get::<_, Option<String>>(2)?.into_iter().collect();
    while let Some(row) = rows.next()? {
        if let Some(root) = row.get::<_, Option<String>>(2)? {
            scan_roots.push(root);
        }
    }
    Ok(Some(VideoAssetSource {
        path,
        kind,
        scan_roots,
    }))
}

pub fn current_library_revision(conn: &Connection) -> anyhow::Result<i64> {
    conn.query_row(
        "SELECT value FROM library_metadata WHERE key = 'revision'",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub fn optimize(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch("PRAGMA optimize;")?;
    Ok(())
}

pub(super) fn grouped_bucket_stats_sql() -> &'static str {
    "SELECT
       'group:' || media_group_key AS sort_bucket,
       MAX(modified_at) AS bucket_modified_at,
       MAX(id) AS bucket_max_id
     FROM filtered_assets
     WHERE media_group_key IS NOT NULL AND trim(media_group_key) <> ''
     GROUP BY 'group:' || media_group_key"
}

pub fn list_ordered_asset_ids_with_meta(
    conn: &Connection,
    tags_and: &[String],
    tags_not: &[String],
    kind: Option<&str>,
    favorites_only: bool,
    meta_filter: Option<&AssetMetaFilter>,
) -> anyhow::Result<Vec<i64>> {
    Ok(try_list_ordered_asset_ids_with_meta(
        conn,
        tags_and,
        tags_not,
        kind,
        favorites_only,
        meta_filter,
        || false,
    )?
    .unwrap_or_default())
}

/// Builds the ordered matching-ID list, cooperatively aborting through
/// `cancel_check` (checked periodically while streaming rows). `Ok(None)`
/// means the build was cancelled; the partial list is discarded.
pub fn try_list_ordered_asset_ids_with_meta(
    conn: &Connection,
    tags_and: &[String],
    tags_not: &[String],
    kind: Option<&str>,
    favorites_only: bool,
    meta_filter: Option<&AssetMetaFilter>,
    cancel_check: impl Fn() -> bool,
) -> anyhow::Result<Option<Vec<i64>>> {
    use rusqlite::types::Value;

    let mut where_clauses = Vec::<String>::new();
    let mut values = Vec::<Value>::new();

    if let Some(kind) = kind {
        where_clauses.push("a.kind = ?".to_string());
        values.push(Value::Text(kind.to_string()));
    }
    if favorites_only {
        where_clauses.push("a.is_favorite = 1".to_string());
    }
    if !tags_and.is_empty() {
        let placeholders = vec!["?"; tags_and.len()].join(",");
        where_clauses.push(format!(
            "a.id IN (
              SELECT at.asset_id
              FROM asset_tags at
              JOIN tags t ON t.id = at.tag_id
              WHERE t.name IN ({placeholders})
              GROUP BY at.asset_id
              HAVING COUNT(DISTINCT t.id) = ?
            )"
        ));
        values.extend(tags_and.iter().cloned().map(Value::Text));
        values.push(Value::Integer(tags_and.len() as i64));
    }
    if !tags_not.is_empty() {
        let placeholders = vec!["?"; tags_not.len()].join(",");
        where_clauses.push(format!(
            "NOT EXISTS (
              SELECT 1 FROM asset_tags atn
              JOIN tags tn ON tn.id = atn.tag_id
              WHERE atn.asset_id = a.id AND tn.name IN ({placeholders})
            )"
        ));
        values.extend(tags_not.iter().cloned().map(Value::Text));
    }
    match meta_filter {
        Some(AssetMetaFilter::HasNoTags { tag_count }) => {
            where_clauses.push("a.tag_count = ?".to_string());
            values.push(Value::Integer(*tag_count));
        }
        Some(AssetMetaFilter::GroupName { group_name }) => {
            where_clauses.push("a.media_group_key_normalized = ?".to_string());
            values.push(Value::Text(group_name.trim().to_lowercase()));
        }
        None => {}
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };
    let grouped_bucket_stats_sql = grouped_bucket_stats_sql();
    let sql = format!(
        "
        WITH filtered_assets AS (
          SELECT
            a.id,
            a.modified_at,
            a.media_group_key,
            a.media_group_order
          FROM assets a
          {where_sql}
        ),
        grouped_bucket_stats AS (
          {grouped_bucket_stats_sql}
        )
        SELECT fa.id
        FROM filtered_assets fa
        LEFT JOIN grouped_bucket_stats bs
          ON fa.media_group_key IS NOT NULL
          AND trim(fa.media_group_key) <> ''
          AND bs.sort_bucket = 'group:' || fa.media_group_key
        ORDER BY
          CASE
            WHEN fa.media_group_key IS NOT NULL AND trim(fa.media_group_key) <> ''
              THEN bs.bucket_modified_at
            ELSE fa.modified_at
          END DESC,
          CASE
            WHEN fa.media_group_key IS NOT NULL AND trim(fa.media_group_key) <> ''
              THEN bs.bucket_max_id
            ELSE fa.id
          END DESC,
          CASE
            WHEN fa.media_group_key IS NOT NULL
              AND trim(fa.media_group_key) <> ''
              AND fa.media_group_order IS NULL
            THEN 1 ELSE 0
          END ASC,
          fa.media_group_order ASC,
          fa.modified_at DESC,
          fa.id DESC
        "
    );

    let mut ids = Vec::new();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| row.get(0))?;
    const CANCEL_CHECK_INTERVAL: usize = 256;
    for (index, row) in rows.enumerate() {
        if index % CANCEL_CHECK_INTERVAL == 0 && cancel_check() {
            return Ok(None);
        }
        ids.push(row?);
    }
    Ok(Some(ids))
}

pub fn list_asset_summaries_by_ids(
    conn: &Connection,
    asset_ids: &[i64],
) -> anyhow::Result<Vec<AssetSummary>> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut by_id = HashMap::<i64, AssetSummary>::with_capacity(asset_ids.len());
    for chunk in asset_ids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT id, file_name, CASE WHEN kind IN ('gif', 'video') THEN path ELSE NULL END, kind,
                    modified_at, width, height, duration_ms, thumb_path,
                    is_favorite, media_group_key, media_group_order
             FROM assets WHERE id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(chunk.iter()),
            parse_asset_summary_row,
        )?;
        for row in rows {
            let summary = row?;
            by_id.insert(summary.id, summary);
        }
    }

    Ok(asset_ids
        .iter()
        .filter_map(|asset_id| by_id.remove(asset_id))
        .collect())
}

pub fn get_asset_details(conn: &Connection, asset_id: i64) -> anyhow::Result<Option<AssetDetails>> {
    let row = conn
        .query_row(
            "SELECT id, file_name, CASE WHEN kind = 'gif' THEN path ELSE NULL END, kind,
                    modified_at, width, height, duration_ms, thumb_path,
                    is_favorite, media_group_key, media_group_order, path, size_bytes
             FROM assets WHERE id = ?1",
            params![asset_id],
            |row| {
                Ok((
                    parse_asset_summary_row(row)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, i64>(13)?,
                ))
            },
        )
        .optional()?;

    let Some((summary, path, size_bytes)) = row else {
        return Ok(None);
    };
    let tags = list_asset_tags(conn, asset_id)?;
    Ok(Some(AssetDetails {
        summary,
        path,
        size_bytes,
        tags,
    }))
}

pub(super) fn parse_asset_summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetSummary> {
    Ok(AssetSummary {
        id: row.get(0)?,
        file_name: row.get(1)?,
        preview_path: row.get(2)?,
        kind: row.get(3)?,
        modified_at: row.get(4)?,
        width: row.get(5)?,
        height: row.get(6)?,
        duration_ms: row.get(7)?,
        thumb_path: row.get(8)?,
        is_favorite: row.get::<_, i64>(9)? != 0,
        media_group_key: row.get(10)?,
        media_group_order: row.get(11)?,
    })
}
