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
               a.record_version, a.size_bytes, a.fingerprint_mtime_ns
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
            row.get::<_, i64>(6)?,
        ))
    })?;
    let mut current_key = String::new();
    for row in rows {
        let (key, display, id, path, record_version, size_bytes, mtime_ns) = row?;
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
                thumb_version: crate::thumbs::thumb_version_token(&path, size_bytes, mtime_ns),
                path,
                record_version,
                size_bytes,
            });
        }
    }
    Ok(groups)
}

/// Builds a literal-substring LIKE pattern (`ESCAPE '\\'`) from user text.
fn tag_filter_like_pattern(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 2);
    out.push('%');
    for ch in query.to_lowercase().chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
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
        let q = tag_filter_like_pattern(trimmed_query);
        conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE lower(name) LIKE ?1 ESCAPE '\\'",
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
        let q = tag_filter_like_pattern(trimmed_query);
        let mut stmt = conn.prepare(
            "SELECT name FROM tags WHERE lower(name) LIKE ?1 ESCAPE '\\' ORDER BY name ASC LIMIT ?2 OFFSET ?3",
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

/// Whether an asset row stores exactly `path`; uses the unique index on `assets.path`.
pub fn asset_path_exists(conn: &Connection, path: &str) -> anyhow::Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM assets WHERE path = ?1)",
        [path],
        |row| row.get(0),
    )?)
}

/// Streams stored asset paths without materializing them and stops at the first
/// one for which `predicate` returns true.
pub fn any_asset_path(
    conn: &Connection,
    mut predicate: impl FnMut(&str) -> bool,
) -> anyhow::Result<bool> {
    let mut stmt = conn.prepare("SELECT path FROM assets")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if predicate(row.get_ref(0)?.as_str()?) {
            return Ok(true);
        }
    }
    Ok(false)
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
    let mut stmt = conn.prepare_cached(
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
    .map(|ordered| ordered.ids)
    .unwrap_or_default())
}

/// The ordered session vector and, for filtered queries, its timeline
/// sections built in the same pass. Unfiltered sections are `None`: reading the
/// bucket columns for every row would double the 1M rebuild (29 → 58 ms), so
/// they are read on demand by `list_unfiltered_query_sections`.
#[derive(Debug, Default, PartialEq)]
pub struct OrderedAssetIds {
    pub ids: Vec<i64>,
    pub sections: Option<Vec<crate::models::AssetQuerySection>>,
}

/// Timeline sections of the unfiltered order, read from the sort-key index in
/// the same order as the session vector. Only `sort_bucket_time` is read:
/// members of a bucket share it, so splitting only where it changes never
/// splits a media group. The caller checks the session's revision in the same
/// read transaction and that the sections cover exactly its `total`.
pub fn list_unfiltered_query_sections(
    conn: &Connection,
) -> anyhow::Result<Vec<crate::models::AssetQuerySection>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT sort_bucket_time FROM assets ORDER BY {SORT_KEY_ORDER_SQL}"
    ))?;
    let mut rows = stmt.query([])?;
    let mut sections = super::query_sections::SectionBuilder::default();
    while let Some(row) = rows.next()? {
        let time = row.get::<_, Option<i64>>(0)?.unwrap_or(0);
        sections.push(time, time);
    }
    Ok(sections.finish())
}

/// Column order of `idx_assets_sort_key`; an unfiltered query streams it.
pub(super) const SORT_KEY_ORDER_SQL: &str = "sort_bucket_time DESC, sort_bucket_id DESC,
    sort_group_null_order ASC, media_group_order ASC, modified_at DESC, id DESC";

const CANCEL_CHECK_INTERVAL: usize = 256;

/// Resolves tag names to IDs. `tags.name` is `COLLATE NOCASE`, so the match is
/// the same as the former name join.
fn resolve_tag_ids(conn: &Connection, names: &[String]) -> anyhow::Result<Vec<i64>> {
    let placeholders = vec!["?"; names.len()].join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT id FROM tags WHERE name IN ({placeholders})"
    ))?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(names.iter()), |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(ids)
}

/// One filtered row with the stored keys needed to rebuild its bucket order.
struct FilteredOrderRow {
    id: i64,
    modified_at: i64,
    bucket: i64,
    null_order: i64,
    group_order: Option<f64>,
}

/// Builds the ordered matching-ID list (and, with filters, its timeline
/// sections, see `query_sections`), cooperatively aborting through `cancel_check` (checked
/// periodically while streaming rows). `Ok(None)` means the build was
/// cancelled; the partial list is discarded.
///
/// Ordering (ADR 0004): every nonblank media-group key forms one bucket, every
/// other asset its own bucket. Buckets sort by their newest *matching* member's
/// `modified_at` and then their highest matching ID, both descending; within a
/// bucket rows with a group order come first (ascending), then
/// `modified_at DESC, id DESC`. Without filters every member matches, so the
/// trigger-maintained whole-bucket keys in `idx_assets_sort_key` give the order
/// directly. With filters, `sort_bucket_id` identifies the bucket and the
/// matching-member statistics are recomputed here.
pub fn try_list_ordered_asset_ids_with_meta(
    conn: &Connection,
    tags_and: &[String],
    tags_not: &[String],
    kind: Option<&str>,
    favorites_only: bool,
    meta_filter: Option<&AssetMetaFilter>,
    cancel_check: impl Fn() -> bool,
) -> anyhow::Result<Option<OrderedAssetIds>> {
    use rusqlite::types::Value;

    use super::query_sections::SectionBuilder;

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
        let tag_ids = resolve_tag_ids(conn, tags_and)?;
        // The former `HAVING COUNT(DISTINCT t.id) = <names>` matched nothing
        // when a name was missing or two names denoted the same tag.
        if tag_ids.len() != tags_and.len() {
            return Ok(Some(OrderedAssetIds {
                ids: Vec::new(),
                sections: Some(Vec::new()),
            }));
        }
        let placeholders = vec!["?"; tag_ids.len()].join(",");
        where_clauses.push(format!(
            "a.id IN (
              SELECT asset_id FROM asset_tags
              WHERE tag_id IN ({placeholders})
              GROUP BY asset_id
              HAVING COUNT(*) = ?
            )"
        ));
        values.extend(tag_ids.iter().copied().map(Value::Integer));
        values.push(Value::Integer(tag_ids.len() as i64));
    }
    if !tags_not.is_empty() {
        let tag_ids = resolve_tag_ids(conn, tags_not)?;
        if !tag_ids.is_empty() {
            let placeholders = vec!["?"; tag_ids.len()].join(",");
            // Uncorrelated: SQLite builds the excluded-ID set once instead of
            // probing asset_tags per row. asset_id is NOT NULL, so NOT IN is
            // exactly NOT EXISTS here.
            where_clauses.push(format!(
                "a.id NOT IN (
                  SELECT asset_id FROM asset_tags WHERE tag_id IN ({placeholders})
                )"
            ));
            values.extend(tag_ids.iter().copied().map(Value::Integer));
        }
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

    // Excluding only unknown tags filters nothing, so it takes the index path.
    if where_clauses.is_empty() {
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT id FROM assets ORDER BY {SORT_KEY_ORDER_SQL}"
        ))?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut ids = Vec::new();
        for (index, row) in rows.enumerate() {
            if index % CANCEL_CHECK_INTERVAL == 0 && cancel_check() {
                return Ok(None);
            }
            ids.push(row?);
        }
        return Ok(Some(OrderedAssetIds {
            ids,
            sections: None,
        }));
    }

    let sql = format!(
        "SELECT a.id, a.modified_at, a.sort_bucket_id, a.sort_group_null_order,
                a.media_group_order
         FROM assets a
         WHERE {}",
        where_clauses.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), |row| {
        Ok(FilteredOrderRow {
            id: row.get(0)?,
            modified_at: row.get(1)?,
            bucket: row.get(2)?,
            null_order: row.get(3)?,
            group_order: row.get(4)?,
        })
    })?;
    let mut matched = Vec::new();
    // Matching-member statistics per bucket: (newest modified_at, highest ID).
    let mut buckets = HashMap::<i64, (i64, i64)>::new();
    for (index, row) in rows.enumerate() {
        if index % CANCEL_CHECK_INTERVAL == 0 && cancel_check() {
            return Ok(None);
        }
        let row = row?;
        buckets
            .entry(row.bucket)
            .and_modify(|(time, id)| {
                *time = (*time).max(row.modified_at);
                *id = (*id).max(row.id);
            })
            .or_insert((row.modified_at, row.id));
        matched.push(row);
    }
    if cancel_check() {
        return Ok(None);
    }

    let mut keyed = matched
        .into_iter()
        .map(|row| (buckets[&row.bucket], row))
        .collect::<Vec<_>>();
    keyed.sort_unstable_by(|(left_bucket, left), (right_bucket, right)| {
        right_bucket
            .0
            .cmp(&left_bucket.0)
            .then(right_bucket.1.cmp(&left_bucket.1))
            .then(left.null_order.cmp(&right.null_order))
            // SQLite sorts NULL first and compares numbers by value (-0.0 = 0.0);
            // stored orders are never NaN.
            .then_with(|| match (left.group_order, right.group_order) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(l), Some(r)) => l.partial_cmp(&r).unwrap_or(std::cmp::Ordering::Equal),
            })
            .then(right.modified_at.cmp(&left.modified_at))
            .then(right.id.cmp(&left.id))
    });
    if cancel_check() {
        return Ok(None);
    }
    // A fresh allocation: collecting in place would keep the wider row buffer
    // as the retained session vector's capacity.
    let mut ids = Vec::with_capacity(keyed.len());
    let mut sections = SectionBuilder::default();
    for ((bucket_time, _), row) in &keyed {
        sections.push(*bucket_time, row.bucket);
        ids.push(row.id);
    }
    Ok(Some(OrderedAssetIds {
        ids,
        sections: Some(sections.finish()),
    }))
}

/// Placeholder counts for summary lookups. A page is at most 256 IDs.
const SUMMARY_CHUNK_SIZES: [usize; 3] = [16, 64, 256];

pub fn list_asset_summaries_by_ids(
    conn: &Connection,
    asset_ids: &[i64],
) -> anyhow::Result<Vec<AssetSummary>> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut by_id = HashMap::<i64, AssetSummary>::with_capacity(asset_ids.len());
    for chunk in asset_ids.chunks(SUMMARY_CHUNK_SIZES[SUMMARY_CHUNK_SIZES.len() - 1]) {
        // Pad to a fixed placeholder count so the cached statement is reused;
        // NULL never matches `id IN (...)`.
        let size = SUMMARY_CHUNK_SIZES
            .iter()
            .copied()
            .find(|size| *size >= chunk.len())
            .expect("chunk fits the largest size");
        let placeholders = vec!["?"; size].join(",");
        let sql = format!(
            "SELECT id, file_name, CASE WHEN kind IN ('gif', 'video') THEN path ELSE NULL END, kind,
                    modified_at, width, height, duration_ms, path, size_bytes, fingerprint_mtime_ns,
                    is_favorite, media_group_key, media_group_order
             FROM assets WHERE id IN ({placeholders})"
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let params = chunk
            .iter()
            .map(|id| Some(*id))
            .chain(std::iter::repeat_n(None, size - chunk.len()));
        let rows = stmt.query_map(rusqlite::params_from_iter(params), parse_asset_summary_row)?;
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
                    modified_at, width, height, duration_ms, path, size_bytes, fingerprint_mtime_ns,
                    is_favorite, media_group_key, media_group_order, thumb_path
             FROM assets WHERE id = ?1",
            params![asset_id],
            |row| {
                Ok((
                    parse_asset_summary_row(row)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<String>>(14)?,
                ))
            },
        )
        .optional()?;

    let Some((summary, path, size_bytes, thumb_path)) = row else {
        return Ok(None);
    };
    let tags = list_asset_tags(conn, asset_id)?;
    Ok(Some(AssetDetails {
        summary,
        path,
        size_bytes,
        thumb_path,
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
        thumb_version: crate::thumbs::thumb_version_token(
            &row.get::<_, String>(8)?,
            row.get(9)?,
            row.get(10)?,
        ),
        is_favorite: row.get::<_, i64>(11)? != 0,
        media_group_key: row.get(12)?,
        media_group_order: row.get(13)?,
    })
}
