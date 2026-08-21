use std::{collections::HashMap, path::Path, thread, time::Duration};

use anyhow::Context;
use rusqlite::{params, Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior};

use crate::models::{
    Asset, AssetDetails, AssetPage, AssetSummary, AssetTagResult, DuplicateAsset, DuplicateGroup,
    NewAsset, SetAssetTagsSummary, TagListPage, ThumbnailAsset,
};
use crate::utils::tags::{merge_tags, normalize_tags};

pub struct CsvAssetRow {
    pub path: String,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub media_group_key: Option<String>,
    pub media_group_order: Option<f64>,
}

pub struct AssetPathRow {
    pub id: i64,
    pub path: String,
}

pub const APPLICATION_ID: i64 = 0x4d54_4147;
pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupSchemaCompatibility {
    Current,
    Legacy,
}

#[derive(Debug, Clone)]
pub struct BackupAssetPath {
    pub id: i64,
    pub path: String,
    pub thumb_path: Option<String>,
}

pub struct DuplicateFileNameCount {
    pub file_name_key: String,
    pub file_name_display: String,
    pub asset_count: usize,
}

pub struct DuplicateAssetRow {
    pub id: i64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct FileMutationAsset {
    pub id: i64,
    pub path: String,
    pub thumb_path: Option<String>,
    pub record_version: i64,
}

#[derive(Debug, Clone)]
pub struct PendingFileOperation {
    pub operation_id: String,
    pub asset_id: i64,
    pub action: String,
    pub original_path: String,
    pub staging_path: String,
    pub final_path: Option<String>,
    pub committed: bool,
}

pub enum DbFileMutation<'a> {
    Rename {
        asset_id: i64,
        expected_path: &'a str,
        expected_record_version: i64,
        new_path: &'a str,
        new_file_name: &'a str,
    },
    Delete {
        asset_id: i64,
        expected_path: &'a str,
        expected_record_version: i64,
    },
}

fn root_descendant_like_pattern(root: &str) -> String {
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

#[derive(Debug, Clone)]
pub enum AssetMetaFilter {
    HasNoTags { tag_count: i64 },
    GroupName { group_name: String },
}

pub fn open_connection(db_path: &Path) -> anyhow::Result<Connection> {
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
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("cannot open sqlite db read-only at {}", db_path.display()))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(conn)
}

#[derive(Debug, Clone)]
pub struct ExistingAssetFingerprint {
    pub id: i64,
    pub path: String,
    pub kind: String,
    pub size_bytes: i64,
    pub modified_at_ns: i64,
}

pub fn init_schema(conn: &Connection) -> anyhow::Result<()> {
    let application_id = conn.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))?;
    let schema_version = conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
    anyhow::ensure!(
        application_id == 0 || application_id == APPLICATION_ID,
        "database belongs to another application"
    );
    anyhow::ensure!(
        schema_version <= SCHEMA_VERSION,
        "database schema version {schema_version} is newer than supported version {SCHEMA_VERSION}"
    );
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS assets (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          path TEXT NOT NULL UNIQUE,
          file_name TEXT NOT NULL DEFAULT '',
          kind TEXT NOT NULL,
          size_bytes INTEGER NOT NULL,
          modified_at INTEGER NOT NULL,
          width INTEGER,
          height INTEGER,
          duration_ms INTEGER,
          thumb_path TEXT,
          is_favorite INTEGER NOT NULL DEFAULT 0,
          media_group_key TEXT,
          media_group_order REAL,
          fingerprint_mtime_ns INTEGER NOT NULL DEFAULT 0,
          tag_count INTEGER NOT NULL DEFAULT 0,
          file_name_key TEXT NOT NULL DEFAULT '',
          media_group_key_normalized TEXT,
          indexed_at INTEGER NOT NULL DEFAULT (unixepoch()),
          record_version INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS tags (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          name TEXT NOT NULL UNIQUE COLLATE NOCASE
        );

        CREATE TABLE IF NOT EXISTS asset_tags (
          asset_id INTEGER NOT NULL,
          tag_id INTEGER NOT NULL,
          PRIMARY KEY(asset_id, tag_id),
          FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE,
          FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS scan_roots (
          path TEXT PRIMARY KEY
        );

        CREATE TABLE IF NOT EXISTS thumbnail_failures (
          asset_id INTEGER PRIMARY KEY,
          failure_count INTEGER NOT NULL,
          last_error TEXT,
          last_failed_at INTEGER NOT NULL,
          asset_modified_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS library_metadata (
          key TEXT PRIMARY KEY,
          value INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS pending_file_operations (
          operation_id TEXT NOT NULL,
          asset_id INTEGER NOT NULL,
          action TEXT NOT NULL,
          original_path TEXT NOT NULL,
          staging_path TEXT NOT NULL,
          final_path TEXT,
          committed INTEGER NOT NULL DEFAULT 0,
          created_at INTEGER NOT NULL DEFAULT (unixepoch()),
          PRIMARY KEY(operation_id, asset_id)
        );

        INSERT INTO library_metadata(key, value)
        VALUES ('revision', 1)
        ON CONFLICT(key) DO NOTHING;

        INSERT INTO library_metadata(key, value)
        VALUES ('performance_schema_version', 0)
        ON CONFLICT(key) DO NOTHING;

        CREATE TABLE IF NOT EXISTS asset_scan_roots (
          asset_id INTEGER NOT NULL,
          root_path TEXT NOT NULL,
          last_seen_generation INTEGER NOT NULL,
          PRIMARY KEY(asset_id, root_path),
          FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE,
          FOREIGN KEY(root_path) REFERENCES scan_roots(path) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_assets_kind ON assets(kind);
        CREATE INDEX IF NOT EXISTS idx_assets_modified_at ON assets(modified_at DESC);
        CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
        CREATE INDEX IF NOT EXISTS idx_asset_tags_tag ON asset_tags(tag_id);
        CREATE INDEX IF NOT EXISTS idx_asset_tags_asset ON asset_tags(asset_id);
        CREATE INDEX IF NOT EXISTS idx_thumbnail_failures_last_failed_at ON thumbnail_failures(last_failed_at DESC);
        ",
    )?;
    ensure_assets_is_favorite_column(conn)?;
    ensure_assets_media_group_key_column(conn)?;
    ensure_assets_media_group_order_column(conn)?;
    ensure_assets_file_name_column(conn)?;
    ensure_performance_columns(conn)?;

    let performance_schema_version = conn.query_row(
        "SELECT value FROM library_metadata WHERE key = 'performance_schema_version'",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if performance_schema_version < 1 {
        conn.execute(
            "UPDATE assets SET file_name_key = lower(file_name) WHERE file_name_key = ''",
            [],
        )?;
        conn.execute(
            "UPDATE assets SET media_group_key_normalized = lower(trim(media_group_key))
             WHERE media_group_key IS NOT NULL AND trim(media_group_key) <> ''",
            [],
        )?;
        conn.execute(
            "UPDATE assets SET tag_count = (
               SELECT COUNT(*) FROM asset_tags at WHERE at.asset_id = assets.id
             )",
            [],
        )?;
        conn.execute(
            "UPDATE library_metadata SET value = 1 WHERE key = 'performance_schema_version'",
            [],
        )?;
    }

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_assets_file_name ON assets(file_name COLLATE NOCASE)",
        [],
    )?;
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_assets_gallery_kind_favorite
          ON assets(kind, is_favorite, modified_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_assets_tag_count
          ON assets(tag_count, modified_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_assets_group_normalized
          ON assets(media_group_key_normalized, modified_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_assets_file_name_key
          ON assets(file_name_key, modified_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_asset_tags_tag_asset
          ON asset_tags(tag_id, asset_id);
        CREATE INDEX IF NOT EXISTS idx_asset_scan_roots_generation
          ON asset_scan_roots(root_path, last_seen_generation, asset_id);
        ",
    )?;
    if performance_schema_version < 2 {
        backfill_asset_scan_roots(conn)?;
        conn.execute(
            "UPDATE library_metadata SET value = 2 WHERE key = 'performance_schema_version'",
            [],
        )?;
    }
    optimize(conn)?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;

    Ok(())
}

pub fn validate_backup_database(
    conn: &Connection,
    allow_legacy: bool,
) -> anyhow::Result<BackupSchemaCompatibility> {
    let integrity = conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?;
    anyhow::ensure!(
        integrity.eq_ignore_ascii_case("ok"),
        "SQLite integrity check failed: {integrity}"
    );

    let foreign_key_violation = conn
        .prepare("PRAGMA foreign_key_check")?
        .query_row([], |_| Ok(()))
        .optional()?;
    anyhow::ensure!(
        foreign_key_violation.is_none(),
        "SQLite foreign key check failed"
    );

    let application_id = conn.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))?;
    let schema_version = conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
    let compatibility = if application_id == APPLICATION_ID {
        anyhow::ensure!(
            schema_version <= SCHEMA_VERSION,
            "database schema version {schema_version} is newer than supported version {SCHEMA_VERSION}"
        );
        anyhow::ensure!(
            schema_version == SCHEMA_VERSION,
            "unsupported MediaTagger schema version {schema_version}"
        );
        BackupSchemaCompatibility::Current
    } else if application_id == 0 && schema_version == 0 && allow_legacy {
        BackupSchemaCompatibility::Legacy
    } else if application_id == 0 {
        anyhow::bail!("database has no MediaTagger application identity");
    } else {
        anyhow::bail!("database belongs to another application");
    };

    validate_backup_schema(conn, compatibility)?;
    validate_backup_data(conn, compatibility)?;
    Ok(compatibility)
}

fn validate_backup_schema(
    conn: &Connection,
    compatibility: BackupSchemaCompatibility,
) -> anyhow::Result<()> {
    let required_tables = [
        ("assets", &["id", "path", "kind", "size_bytes", "modified_at", "thumb_path"][..]),
        ("tags", &["id", "name"][..]),
        ("asset_tags", &["asset_id", "tag_id"][..]),
        ("scan_roots", &["path"][..]),
        (
            "thumbnail_failures",
            &["asset_id", "failure_count", "last_failed_at", "asset_modified_at"][..],
        ),
        ("library_metadata", &["key", "value"][..]),
    ];
    for (table, columns) in required_tables {
        validate_table_columns(conn, table, columns)?;
    }
    validate_backup_constraints(conn, compatibility)?;

    if compatibility == BackupSchemaCompatibility::Current {
        validate_table_columns(
            conn,
            "assets",
            &[
                "file_name",
                "width",
                "height",
                "duration_ms",
                "is_favorite",
                "media_group_key",
                "media_group_order",
                "fingerprint_mtime_ns",
                "tag_count",
                "file_name_key",
                "media_group_key_normalized",
                "indexed_at",
                "record_version",
            ],
        )?;
        validate_table_columns(
            conn,
            "asset_scan_roots",
            &["asset_id", "root_path", "last_seen_generation"],
        )?;
        validate_table_columns(
            conn,
            "pending_file_operations",
            &[
                "operation_id",
                "asset_id",
                "action",
                "original_path",
                "staging_path",
                "final_path",
                "committed",
                "created_at",
            ],
        )?;
    }
    Ok(())
}

fn validate_backup_constraints(
    conn: &Connection,
    compatibility: BackupSchemaCompatibility,
) -> anyhow::Result<()> {
    for (table, column, not_null, primary_key_position) in [
        ("assets", "id", false, 1),
        ("assets", "path", true, 0),
        ("assets", "kind", true, 0),
        ("assets", "size_bytes", true, 0),
        ("assets", "modified_at", true, 0),
        ("tags", "id", false, 1),
        ("tags", "name", true, 0),
        ("asset_tags", "asset_id", true, 1),
        ("asset_tags", "tag_id", true, 2),
        ("scan_roots", "path", false, 1),
        ("thumbnail_failures", "asset_id", false, 1),
        ("library_metadata", "key", false, 1),
        ("library_metadata", "value", true, 0),
    ] {
        validate_column_constraints(conn, table, column, not_null, primary_key_position)?;
    }
    for (table, columns) in [
        ("assets", &["path"][..]),
        ("tags", &["name"][..]),
        ("asset_tags", &["asset_id", "tag_id"][..]),
    ] {
        anyhow::ensure!(
            has_unique_index(conn, table, columns)?,
            "backup table {table} is missing a required unique key"
        );
    }
    for (table, from, target, to) in [
        ("asset_tags", "asset_id", "assets", "id"),
        ("asset_tags", "tag_id", "tags", "id"),
    ] {
        anyhow::ensure!(
            has_cascade_foreign_key(conn, table, from, target, to)?,
            "backup table {table} is missing a required cascading foreign key"
        );
    }
    if compatibility == BackupSchemaCompatibility::Current {
        for (table, column, not_null, primary_key_position) in [
            ("assets", "file_name", true, 0),
            ("assets", "is_favorite", true, 0),
            ("assets", "fingerprint_mtime_ns", true, 0),
            ("assets", "tag_count", true, 0),
            ("assets", "file_name_key", true, 0),
            ("assets", "indexed_at", true, 0),
            ("assets", "record_version", true, 0),
            ("asset_scan_roots", "asset_id", true, 1),
            ("asset_scan_roots", "root_path", true, 2),
            ("asset_scan_roots", "last_seen_generation", true, 0),
            ("pending_file_operations", "operation_id", true, 1),
            ("pending_file_operations", "asset_id", true, 2),
            ("pending_file_operations", "action", true, 0),
            ("pending_file_operations", "original_path", true, 0),
            ("pending_file_operations", "staging_path", true, 0),
            ("pending_file_operations", "committed", true, 0),
            ("pending_file_operations", "created_at", true, 0),
        ] {
            validate_column_constraints(conn, table, column, not_null, primary_key_position)?;
        }
        anyhow::ensure!(
            has_unique_index(conn, "asset_scan_roots", &["asset_id", "root_path"])?,
            "backup table asset_scan_roots is missing its primary key"
        );
        anyhow::ensure!(
            has_unique_index(conn, "pending_file_operations", &["operation_id", "asset_id"])?,
            "backup table pending_file_operations is missing its primary key"
        );
        for (from, target, to) in [
            ("asset_id", "assets", "id"),
            ("root_path", "scan_roots", "path"),
        ] {
            anyhow::ensure!(
                has_cascade_foreign_key(conn, "asset_scan_roots", from, target, to)?,
                "backup table asset_scan_roots is missing a required cascading foreign key"
            );
        }
    }
    Ok(())
}

fn validate_column_constraints(
    conn: &Connection,
    table: &str,
    expected_column: &str,
    expected_not_null: bool,
    expected_primary_key_position: i64,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(1)?,
            row.get::<_, bool>(3)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    for row in rows {
        let (column, not_null, primary_key_position) = row?;
        if column.eq_ignore_ascii_case(expected_column) {
            anyhow::ensure!(
                not_null == expected_not_null
                    && primary_key_position == expected_primary_key_position,
                "backup column {table}.{expected_column} has incompatible constraints"
            );
            return Ok(());
        }
    }
    anyhow::bail!("backup table {table} is missing required column {expected_column}")
}

fn has_unique_index(conn: &Connection, table: &str, expected: &[&str]) -> anyhow::Result<bool> {
    let mut index_stmt = conn.prepare(&format!("PRAGMA index_list({table})"))?;
    let index_rows = index_stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(1)?,
            row.get::<_, bool>(2)?,
            row.get::<_, bool>(4)?,
        ))
    })?;
    for index_row in index_rows {
        let (index_name, unique, partial) = index_row?;
        if !unique || partial {
            continue;
        }
        let escaped_name = index_name.replace('"', "\"\"");
        let mut columns_stmt = conn.prepare(&format!("PRAGMA index_info(\"{escaped_name}\")"))?;
        let column_rows = columns_stmt.query_map([], |row| row.get::<_, String>(2))?;
        let columns = column_rows.collect::<Result<Vec<_>, _>>()?;
        if columns
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
        {
            if table == "tags" && expected.len() == 1 && expected[0] == "name" {
                let mut xinfo_stmt = conn.prepare(&format!(
                    "PRAGMA index_xinfo(\"{escaped_name}\")"
                ))?;
                let xinfo_rows = xinfo_stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, bool>(5)?,
                    ))
                })?;
                let mut has_nocase_name = false;
                for xinfo_row in xinfo_rows {
                    let (column, collation, key) = xinfo_row?;
                    if key
                        && column.as_deref() == Some("name")
                        && collation.eq_ignore_ascii_case("NOCASE")
                    {
                        has_nocase_name = true;
                    }
                }
                if !has_nocase_name {
                    continue;
                }
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn has_cascade_foreign_key(
    conn: &Connection,
    table: &str,
    expected_from: &str,
    expected_table: &str,
    expected_to: &str,
) -> anyhow::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA foreign_key_list({table})"))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut foreign_keys = HashMap::<i64, Vec<(i64, String, String, String, String)>>::new();
    for row in rows {
        let (id, sequence, target_table, from, to, on_delete) = row?;
        foreign_keys
            .entry(id)
            .or_default()
            .push((sequence, target_table, from, to, on_delete));
    }
    for parts in foreign_keys.values() {
        if parts.len() == 1
            && parts[0].0 == 0
            && parts[0].2.eq_ignore_ascii_case(expected_from)
            && parts[0].1.eq_ignore_ascii_case(expected_table)
            && parts[0].3.eq_ignore_ascii_case(expected_to)
            && parts[0].4.eq_ignore_ascii_case("CASCADE")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_table_columns(conn: &Connection, table: &str, required: &[&str]) -> anyhow::Result<()> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(1)?.to_lowercase(),
            row.get::<_, String>(2)?.to_uppercase(),
        ))
    })?;
    let mut columns = HashMap::new();
    for row in rows {
        let (name, declared_type) = row?;
        columns.insert(name, declared_type);
    }
    anyhow::ensure!(
        !columns.is_empty(),
        "backup database is missing required table {table}"
    );
    for column in required {
        let declared_type = columns
            .get(&column.to_lowercase())
            .with_context(|| format!("backup table {table} is missing required column {column}"))?;
        let expected_type = match *column {
            "path" | "kind" | "thumb_path" | "name" | "key" | "file_name"
            | "media_group_key" | "file_name_key" | "media_group_key_normalized"
            | "root_path" | "operation_id" | "action" | "original_path" | "staging_path"
            | "final_path" => "TEXT",
            "media_group_order" => "REAL",
            _ => "INTEGER",
        };
        anyhow::ensure!(
            declared_type == expected_type,
            "backup column {table}.{column} has incompatible declared type {declared_type}"
        );
    }
    Ok(())
}

fn validate_backup_data(
    conn: &Connection,
    compatibility: BackupSchemaCompatibility,
) -> anyhow::Result<()> {
    let invalid_assets = conn.query_row(
        "SELECT COUNT(*) FROM assets
         WHERE typeof(id) <> 'integer' OR typeof(path) <> 'text' OR trim(path) = ''
            OR typeof(kind) <> 'text' OR kind NOT IN ('image', 'gif', 'video')
            OR typeof(size_bytes) <> 'integer' OR size_bytes < 0
            OR typeof(modified_at) <> 'integer'
            OR (thumb_path IS NOT NULL AND typeof(thumb_path) <> 'text')",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    anyhow::ensure!(invalid_assets == 0, "backup contains invalid asset values");

    for (table, predicate) in [
        (
            "tags",
            "typeof(id) <> 'integer' OR typeof(name) <> 'text' OR trim(name) = ''",
        ),
        (
            "asset_tags",
            "typeof(asset_id) <> 'integer' OR typeof(tag_id) <> 'integer'",
        ),
        (
            "scan_roots",
            "typeof(path) <> 'text' OR trim(path) = ''",
        ),
        (
            "library_metadata",
            "typeof(key) <> 'text' OR typeof(value) <> 'integer'",
        ),
        (
            "thumbnail_failures",
            "typeof(asset_id) <> 'integer' OR typeof(failure_count) <> 'integer' OR failure_count < 1
             OR (last_error IS NOT NULL AND typeof(last_error) <> 'text')
             OR typeof(last_failed_at) <> 'integer' OR typeof(asset_modified_at) <> 'integer'",
        ),
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {predicate}");
        let invalid = conn.query_row(&sql, [], |row| row.get::<_, i64>(0))?;
        anyhow::ensure!(invalid == 0, "backup table {table} contains invalid values");
    }

    let metadata_rows = conn.query_row(
        "SELECT COUNT(*) FROM library_metadata
         WHERE (key = 'revision' AND value >= 1)
            OR (key = 'performance_schema_version' AND value BETWEEN 0 AND 2)",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    anyhow::ensure!(
        metadata_rows == 2,
        "backup has incompatible MediaTagger metadata"
    );

    if compatibility == BackupSchemaCompatibility::Current {
        let invalid_current_assets = conn.query_row(
            "SELECT COUNT(*) FROM assets
             WHERE typeof(file_name) <> 'text' OR trim(file_name) = ''
                OR typeof(is_favorite) <> 'integer' OR is_favorite NOT IN (0, 1)
                OR typeof(fingerprint_mtime_ns) <> 'integer'
                OR typeof(tag_count) <> 'integer' OR tag_count < 0
                OR typeof(file_name_key) <> 'text'
                OR typeof(indexed_at) <> 'integer'
                OR typeof(record_version) <> 'integer' OR record_version < 1
                OR (width IS NOT NULL AND typeof(width) <> 'integer')
                OR (height IS NOT NULL AND typeof(height) <> 'integer')
                OR (duration_ms IS NOT NULL AND typeof(duration_ms) <> 'integer')
                OR (media_group_key IS NOT NULL AND typeof(media_group_key) <> 'text')
                OR (media_group_key_normalized IS NOT NULL AND typeof(media_group_key_normalized) <> 'text')
                OR (media_group_order IS NOT NULL AND typeof(media_group_order) NOT IN ('integer', 'real'))",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        anyhow::ensure!(
            invalid_current_assets == 0,
            "backup contains invalid current-schema asset values"
        );
        let invalid_derived_assets = conn.query_row(
            "SELECT COUNT(*) FROM assets
             WHERE file_name_key <> lower(file_name)
                OR tag_count <> (SELECT COUNT(*) FROM asset_tags WHERE asset_id = assets.id)
                OR COALESCE(media_group_key_normalized, '') <>
                   CASE WHEN media_group_key IS NULL OR trim(media_group_key) = ''
                        THEN '' ELSE lower(trim(media_group_key)) END",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        anyhow::ensure!(
            invalid_derived_assets == 0,
            "backup contains inconsistent derived asset values"
        );
        for (table, predicate) in [
            (
                "asset_scan_roots",
                "typeof(asset_id) <> 'integer' OR typeof(root_path) <> 'text'
                 OR typeof(last_seen_generation) <> 'integer'",
            ),
        ] {
            let sql = format!("SELECT COUNT(*) FROM {table} WHERE {predicate}");
            let invalid = conn.query_row(&sql, [], |row| row.get::<_, i64>(0))?;
            anyhow::ensure!(invalid == 0, "backup table {table} contains invalid values");
        }
    }
    ensure_no_pending_file_operations(conn)?;
    Ok(())
}

pub fn ensure_no_pending_file_operations(conn: &Connection) -> anyhow::Result<()> {
    let has_pending_table = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'pending_file_operations')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if has_pending_table {
        let pending = conn.query_row(
            "SELECT COUNT(*) FROM pending_file_operations",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        anyhow::ensure!(pending == 0, "database contains pending local file operations");
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
    let mut stmt = conn.prepare(
        "SELECT asset_id, root_path FROM asset_scan_roots ORDER BY asset_id, root_path",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn backfill_asset_scan_roots(conn: &Connection) -> anyhow::Result<()> {
    for root in list_scan_roots(conn)? {
        let like_pattern = root_descendant_like_pattern(&root);
        conn.execute(
            "INSERT OR IGNORE INTO asset_scan_roots(asset_id, root_path, last_seen_generation)
             SELECT id, ?1, 0 FROM assets WHERE path = ?1 OR path LIKE ?2 ESCAPE '^'",
            params![root, like_pattern],
        )?;
    }
    Ok(())
}

fn ensure_performance_columns(conn: &Connection) -> anyhow::Result<()> {
    for (name, definition) in [
        ("fingerprint_mtime_ns", "INTEGER NOT NULL DEFAULT 0"),
        ("tag_count", "INTEGER NOT NULL DEFAULT 0"),
        ("file_name_key", "TEXT NOT NULL DEFAULT ''"),
        ("media_group_key_normalized", "TEXT"),
        ("record_version", "INTEGER NOT NULL DEFAULT 1"),
    ] {
        let exists = {
            let mut stmt = conn.prepare("PRAGMA table_info(assets)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            let mut found = false;
            for row in rows {
                if row?.eq_ignore_ascii_case(name) {
                    found = true;
                    break;
                }
            }
            found
        };

        if !exists {
            conn.execute(
                &format!("ALTER TABLE assets ADD COLUMN {name} {definition}"),
                [],
            )?;
        }
    }
    Ok(())
}

fn ensure_assets_is_favorite_column(conn: &Connection) -> anyhow::Result<()> {
    let mut has_column = false;
    let mut stmt = conn.prepare("PRAGMA table_info(assets)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row?.eq_ignore_ascii_case("is_favorite") {
            has_column = true;
            break;
        }
    }

    if !has_column {
        conn.execute(
            "ALTER TABLE assets ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }

    Ok(())
}

fn ensure_assets_media_group_key_column(conn: &Connection) -> anyhow::Result<()> {
    let mut has_column = false;
    let mut stmt = conn.prepare("PRAGMA table_info(assets)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row?.eq_ignore_ascii_case("media_group_key") {
            has_column = true;
            break;
        }
    }

    if !has_column {
        conn.execute("ALTER TABLE assets ADD COLUMN media_group_key TEXT", [])?;
    }

    Ok(())
}

fn ensure_assets_media_group_order_column(conn: &Connection) -> anyhow::Result<()> {
    let mut has_column = false;
    let mut stmt = conn.prepare("PRAGMA table_info(assets)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row?.eq_ignore_ascii_case("media_group_order") {
            has_column = true;
            break;
        }
    }

    if !has_column {
        conn.execute("ALTER TABLE assets ADD COLUMN media_group_order REAL", [])?;
    }

    Ok(())
}

fn ensure_assets_file_name_column(conn: &Connection) -> anyhow::Result<()> {
    let mut has_column = false;
    let mut stmt = conn.prepare("PRAGMA table_info(assets)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row?.eq_ignore_ascii_case("file_name") {
            has_column = true;
            break;
        }
    }

    if !has_column {
        conn.execute("ALTER TABLE assets ADD COLUMN file_name TEXT", [])?;
    }

    let tx = conn.unchecked_transaction()?;
    let mut updates = Vec::<(i64, String)>::new();
    {
        let mut stmt = tx.prepare(
            "SELECT id, path FROM assets WHERE file_name IS NULL OR trim(file_name) = ''",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (asset_id, path) = row?;
            updates.push((asset_id, extract_file_name(&path)));
        }
    }

    for (asset_id, file_name) in updates {
        tx.execute(
            "UPDATE assets SET file_name = ?1 WHERE id = ?2",
            params![file_name, asset_id],
        )?;
    }
    tx.commit()?;

    Ok(())
}

fn extract_file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub fn upsert_asset(conn: &Connection, asset: &NewAsset) -> anyhow::Result<()> {
    let file_name = extract_file_name(&asset.path);
    conn.execute(
        "
        INSERT INTO assets(path, file_name, file_name_key, kind, size_bytes, modified_at, width, height, duration_ms, thumb_path, fingerprint_mtime_ns, indexed_at)
        VALUES (?1, ?2, lower(?2), ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, unixepoch())
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
            WHEN assets.modified_at = excluded.modified_at THEN assets.thumb_path
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
            asset.kind,
            asset.size_bytes,
            asset.modified_at,
            asset.width,
            asset.height,
            asset.duration_ms,
            asset.thumb_path,
            asset.modified_at.saturating_mul(1_000_000_000)
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

pub fn list_assets_for_thumbnail_render(conn: &Connection) -> anyhow::Result<Vec<ThumbnailAsset>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, path, kind, modified_at, duration_ms, thumb_path FROM assets ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ThumbnailAsset {
            id: row.get(0)?,
            path: row.get(1)?,
            kind: row.get(2)?,
            modified_at: row.get(3)?,
            duration_ms: row.get(4)?,
            thumb_path: row.get(5)?,
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
        SELECT a.id, a.path, a.kind, a.modified_at, a.duration_ms, a.thumb_path
        FROM assets a
        JOIN thumbnail_failures tf ON tf.asset_id = a.id AND tf.asset_modified_at = a.modified_at
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
        WHERE tf.asset_modified_at = a.modified_at
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
    conn.execute(
        "
        INSERT INTO thumbnail_failures(asset_id, failure_count, last_error, last_failed_at, asset_modified_at)
        VALUES (?1, 1, ?2, unixepoch(), ?3)
        ON CONFLICT(asset_id) DO UPDATE SET
          failure_count = CASE
            WHEN thumbnail_failures.asset_modified_at = excluded.asset_modified_at THEN thumbnail_failures.failure_count + 1
            ELSE 1
          END,
          last_error = excluded.last_error,
          last_failed_at = unixepoch(),
          asset_modified_at = excluded.asset_modified_at
        ",
        params![asset_id, error_message, asset_modified_at],
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

fn cleanup_stale_thumbnail_failures(conn: &Connection) -> anyhow::Result<()> {
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
          WHERE tf.asset_modified_at != a.modified_at
        )
        ",
        [],
    )?;
    Ok(())
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
                    params![temporary_path, asset_id, expected_path, expected_record_version],
                )?;
                if affected == 1 {
                    tx.execute(
                        "DELETE FROM thumbnail_failures WHERE asset_id = ?1",
                        params![asset_id],
                    )?;
                    pending_renames.push((
                        *asset_id,
                        temporary_path,
                        *new_path,
                        *new_file_name,
                    ));
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
            anyhow::bail!("asset {} changed since the operation was prepared", match mutation {
                DbFileMutation::Rename { asset_id, .. } | DbFileMutation::Delete { asset_id, .. } => asset_id,
            });
        }
    }
    for (asset_id, temporary_path, new_path, new_file_name) in pending_renames {
        let affected = tx.execute(
            "UPDATE assets SET path = ?1, file_name = ?2, file_name_key = lower(?2),
               thumb_path = NULL, record_version = record_version + 1
             WHERE id = ?3 AND path = ?4",
            params![new_path, new_file_name, asset_id, temporary_path],
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
          lower(file_name) AS file_name_key,
          MIN(file_name) AS file_name_display,
          COUNT(*) AS asset_count
        FROM assets
        WHERE trim(file_name) <> ''
        GROUP BY lower(file_name)
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
        WHERE lower(file_name) = ?1
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
        "UPDATE assets SET path = ?1, file_name = ?2, file_name_key = lower(?2),
          thumb_path = NULL, record_version = record_version + 1 WHERE id = ?3",
        params![new_path, new_file_name, asset_id],
    )?;
    tx.execute(
        "DELETE FROM thumbnail_failures WHERE asset_id = ?1",
        params![asset_id],
    )?;
    tx.commit()?;

    Ok(Some((old_path, old_thumb_path)))
}

pub fn clear_all_thumbnail_paths(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut thumbs = Vec::new();
    let mut stmt =
        conn.prepare("SELECT DISTINCT thumb_path FROM assets WHERE thumb_path IS NOT NULL")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        thumbs.push(row?);
    }

    conn.execute(
        "UPDATE assets SET thumb_path = NULL WHERE thumb_path IS NOT NULL",
        [],
    )?;
    conn.execute("DELETE FROM thumbnail_failures", [])?;
    Ok(thumbs)
}

pub fn clear_library_data(conn: &Connection) -> anyhow::Result<(usize, usize, Vec<String>)> {
    let mut thumbs = Vec::new();
    let mut stmt =
        conn.prepare("SELECT DISTINCT thumb_path FROM assets WHERE thumb_path IS NOT NULL")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        thumbs.push(row?);
    }

    let removed_assets = conn.execute("DELETE FROM assets", [])?;
    conn.execute("DELETE FROM thumbnail_failures", [])?;
    conn.execute("DELETE FROM tags", [])?;
    let removed_roots = conn.execute("DELETE FROM scan_roots", [])?;

    Ok((removed_assets, removed_roots, thumbs))
}

fn cleanup_orphan_tags(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM asset_tags)",
        [],
    )?;
    Ok(())
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
              SELECT GROUP_CONCAT(t2.name, ' ')
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

fn parse_asset_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    let tags_joined: String = row.get(12)?;
    let tags = tags_joined
        .split_whitespace()
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

pub fn set_assets_media_group_bulk(
    conn: &Connection,
    updates: &[(i64, Option<f64>)],
    media_group_key: Option<&str>,
) -> anyhow::Result<(usize, usize)> {
    if updates.is_empty() {
        return Ok((0, 0));
    }

    let tx = conn.unchecked_transaction()?;
    let mut processed_assets = 0usize;
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

        processed_assets += 1;
        if current_key.as_deref() == media_group_key && current_order == *media_group_order {
            continue;
        }

        update_stmt.execute(params![media_group_key, media_group_order, asset_id])?;
        updated_assets += 1;
    }

    drop(update_stmt);
    drop(get_current_stmt);

    tx.commit()?;
    Ok((processed_assets, updated_assets))
}

fn list_asset_tags_in_tx(
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

fn canonicalize_tag_rows_in_tx(
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

fn set_asset_tags_in_tx(
    tx: &rusqlite::Transaction<'_>,
    asset_id: i64,
    tags: &[String],
) -> anyhow::Result<(bool, Vec<String>)> {
    let normalized = normalize_tags(tags.to_vec());
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

fn bump_library_revision_in_tx(tx: &rusqlite::Transaction<'_>) -> anyhow::Result<i64> {
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

fn is_retryable_sqlite_error(error: &anyhow::Error) -> bool {
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

fn retry_immediate_transaction<T>(
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
    tx.commit()?;
    Ok((removed, thumbs))
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
        let (key, display, id, path, record_version, size_bytes, fingerprint_mtime_ns) = row?;
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
                fingerprint_mtime_ns,
            });
        }
    }
    Ok(groups)
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
    upsert_asset(conn, asset)?;
    conn.execute(
        "UPDATE assets SET fingerprint_mtime_ns = ?1 WHERE path = ?2",
        params![fingerprint_mtime_ns, asset.path],
    )?;
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
    tx.commit()?;
    Ok(removed)
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
    let normalized = normalize_tags(tags.to_vec());
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
    let normalized_incoming = normalize_tags(incoming_tags.to_vec());
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
    let normalized_incoming = normalize_tags(incoming_tags.to_vec());
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
            SELECT GROUP_CONCAT(t.name, ' ')
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
            .split_whitespace()
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

pub fn get_asset_for_thumbnail(
    conn: &Connection,
    asset_id: i64,
) -> anyhow::Result<Option<ThumbnailAsset>> {
    let asset = conn
        .query_row(
            "SELECT id, path, kind, modified_at, duration_ms, thumb_path FROM assets WHERE id = ?1",
            params![asset_id],
            |row| {
                Ok(ThumbnailAsset {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    kind: row.get(2)?,
                    modified_at: row.get(3)?,
                    duration_ms: row.get(4)?,
                    thumb_path: row.get(5)?,
                })
            },
        )
        .optional()?;
    Ok(asset)
}

pub fn update_asset_thumbnail_path_if_changed(
    conn: &Connection,
    asset_id: i64,
    thumb_path: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE assets SET thumb_path = ?1 WHERE id = ?2 AND COALESCE(thumb_path, '') != COALESCE(?1, '')",
        params![thumb_path, asset_id],
    )?;
    Ok(())
}

pub fn update_asset_thumbnail_paths_batch(
    conn: &Connection,
    updates: &[(i64, Option<String>)],
) -> anyhow::Result<()> {
    if updates.is_empty() {
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;
    for (asset_id, thumb_path) in updates {
        tx.execute(
            "UPDATE assets SET thumb_path = ?1 WHERE id = ?2 AND COALESCE(thumb_path, '') != COALESCE(?1, '')",
            params![thumb_path.as_deref(), asset_id],
        )?;
    }
    tx.commit()?;
    Ok(())
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
            "SELECT id, path, kind, modified_at, duration_ms, thumb_path
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

pub fn current_library_revision(conn: &Connection) -> anyhow::Result<i64> {
    conn.query_row(
        "SELECT value FROM library_metadata WHERE key = 'revision'",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub fn bump_library_revision(conn: &Connection) -> anyhow::Result<i64> {
    conn.execute(
        "UPDATE library_metadata SET value = value + 1 WHERE key = 'revision'",
        [],
    )?;
    current_library_revision(conn)
}

pub fn optimize(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch("PRAGMA optimize;")?;
    Ok(())
}

fn grouped_bucket_stats_sql() -> &'static str {
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
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
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

fn parse_asset_summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetSummary> {
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

#[cfg(test)]
mod tests {
    use rusqlite::{params, Connection};

    use crate::models::NewAsset;

    use super::{
        clear_thumbnail_failure, current_library_revision, delete_asset_by_id_with_thumb,
        delete_assets_by_prefix_with_thumbs, get_asset_media_group, get_asset_path_and_thumb_by_id,
        grouped_bucket_stats_sql, init_schema, list_assets, list_assets_for_csv_export,
        list_assets_for_thumbnail_render, list_assets_with_meta,
        list_duplicate_assets_by_file_name_key, list_duplicate_file_name_counts,
        list_asset_summaries_by_ids, list_failed_assets_for_thumbnail_render,
        list_failed_thumbnail_asset_ids,
        list_ordered_asset_ids_with_meta, list_tags_page, merge_asset_tags_bulk,
        merge_asset_tags_bulk_with_revision, record_thumbnail_failure, rename_asset_file_by_id,
        root_descendant_like_pattern,
        set_asset_favorite, set_asset_media_group, set_asset_tags, set_asset_tags_with_revision,
        set_assets_media_group_bulk, upsert_asset, AssetMetaFilter,
    };

    #[test]
    fn descendant_pattern_preserves_the_root_separator_style() {
        assert_eq!(root_descendant_like_pattern("C:\\media\\"), "C:\\media\\%");
        assert_eq!(root_descendant_like_pattern("/srv/media/"), "/srv/media/%");
        assert_eq!(root_descendant_like_pattern("/"), "/%");
        assert_eq!(root_descendant_like_pattern("/srv/100%_media"), "/srv/100^%^_media/%");
    }

    fn insert_asset(conn: &Connection, path: &str, kind: &str, modified_at: i64) {
        upsert_asset(
            conn,
            &NewAsset {
                path: path.to_string(),
                kind: kind.to_string(),
                size_bytes: 1,
                modified_at,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            },
        )
        .expect("upsert asset");
    }

    #[test]
    fn summary_preview_paths_include_video_and_gif_sources_only() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "/media/still.jpg", "image", 30);
        insert_asset(&conn, "/media/clip.mp4", "video", 20);
        insert_asset(&conn, "/media/animation.gif", "gif", 10);

        let summaries = list_asset_summaries_by_ids(&conn, &[2, 3, 1]).expect("summaries");

        assert_eq!(summaries.iter().map(|item| item.id).collect::<Vec<_>>(), vec![2, 3, 1]);
        assert_eq!(summaries[0].preview_path.as_deref(), Some("/media/clip.mp4"));
        assert_eq!(summaries[1].preview_path.as_deref(), Some("/media/animation.gif"));
        assert_eq!(summaries[2].preview_path, None);
    }

    #[test]
    fn delete_by_prefix_does_not_match_similar_root() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        conn.execute(
            "INSERT INTO assets(path, kind, size_bytes, modified_at, width, height, duration_ms, thumb_path, indexed_at) VALUES (?1, 'image', 1, 1, NULL, NULL, NULL, NULL, unixepoch())",
            ["C:\\media\\a.jpg"],
        )
        .expect("insert 1");
        conn.execute(
            "INSERT INTO assets(path, kind, size_bytes, modified_at, width, height, duration_ms, thumb_path, indexed_at) VALUES (?1, 'image', 1, 1, NULL, NULL, NULL, NULL, unixepoch())",
            ["C:\\media2\\b.jpg"],
        )
        .expect("insert 2");

        let (removed, _) = delete_assets_by_prefix_with_thumbs(&conn, "C:\\media").expect("delete");
        assert_eq!(removed, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn list_assets_respects_kind_and_tag_filters_with_and_not_logic() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\b.mp4", "video", 20);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 10);

        set_asset_tags(&conn, 1, &["cat".to_string(), "vacation".to_string()]).expect("tags 1");
        set_asset_tags(&conn, 2, &["cat".to_string()]).expect("tags 2");
        set_asset_tags(
            &conn,
            3,
            &["cat".to_string(), "vacation".to_string(), "dog".to_string()],
        )
        .expect("tags 3");

        let page = list_assets(
            &conn,
            0,
            50,
            &["cat".to_string(), "vacation".to_string()],
            &["dog".to_string()],
            Some("image"),
            false,
        )
        .expect("list");

        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].path, "C:\\media\\a.jpg");
    }

    #[test]
    fn list_assets_paginates_in_modified_desc_order() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 10);

        let page = list_assets(&conn, 1, 1, &[], &[], None, false).expect("list");
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].path, "C:\\media\\b.jpg");
    }

    #[test]
    fn list_assets_keeps_media_group_adjacent_and_sorts_by_group_order() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\top.jpg", "image", 100);
        insert_asset(&conn, "C:\\media\\group-second.jpg", "image", 95);
        insert_asset(&conn, "C:\\media\\group-null.jpg", "image", 96);
        insert_asset(&conn, "C:\\media\\group-first.jpg", "image", 97);
        insert_asset(&conn, "C:\\media\\bottom.jpg", "image", 90);

        set_asset_media_group(&conn, 2, Some("group-alpha"), Some(2.0)).expect("set order 2");
        set_asset_media_group(&conn, 3, Some("group-alpha"), None).expect("set null order");
        set_asset_media_group(&conn, 4, Some("group-alpha"), Some(1.0)).expect("set order 1");

        let page = list_assets(&conn, 0, 50, &[], &[], None, false).expect("list");
        let ordered_paths = page
            .items
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ordered_paths,
            vec![
                "C:\\media\\top.jpg",
                "C:\\media\\group-first.jpg",
                "C:\\media\\group-second.jpg",
                "C:\\media\\group-null.jpg",
                "C:\\media\\bottom.jpg",
            ]
        );
    }

    #[test]
    fn list_assets_positions_group_by_newest_asset_in_group() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\group-old-a.jpg", "image", 70);
        insert_asset(&conn, "C:\\media\\group-old-b.jpg", "image", 60);
        insert_asset(&conn, "C:\\media\\single.jpg", "image", 75);
        insert_asset(&conn, "C:\\media\\group-new-a.jpg", "image", 80);
        insert_asset(&conn, "C:\\media\\group-new-b.jpg", "image", 10);

        set_asset_media_group(&conn, 1, Some("group-old"), Some(1.0)).expect("old 1");
        set_asset_media_group(&conn, 2, Some("group-old"), Some(2.0)).expect("old 2");
        set_asset_media_group(&conn, 4, Some("group-new"), Some(1.0)).expect("new 1");
        set_asset_media_group(&conn, 5, Some("group-new"), Some(2.0)).expect("new 2");

        let page = list_assets(&conn, 0, 50, &[], &[], None, false).expect("list");
        let ordered_paths = page
            .items
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ordered_paths,
            vec![
                "C:\\media\\group-new-a.jpg",
                "C:\\media\\group-new-b.jpg",
                "C:\\media\\single.jpg",
                "C:\\media\\group-old-a.jpg",
                "C:\\media\\group-old-b.jpg",
            ]
        );
    }

    #[test]
    fn list_assets_filters_favorites_only() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 20);

        set_asset_favorite(&conn, 2, true).expect("favorite b");

        let page = list_assets(&conn, 0, 50, &[], &[], None, true).expect("list favorites");
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].path, "C:\\media\\b.jpg");
        assert!(page.items[0].is_favorite);
    }

    #[test]
    fn list_assets_filters_exact_tag_count_metatags() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\zero.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\two.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\three.jpg", "image", 10);

        set_asset_tags(&conn, 2, &["cat".to_string(), "travel".to_string()]).expect("tags 2");
        set_asset_tags(
            &conn,
            3,
            &["cat".to_string(), "travel".to_string(), "dog".to_string()],
        )
        .expect("tags 3");

        let zero_tags = list_assets_with_meta(
            &conn,
            0,
            50,
            &[],
            &[],
            None,
            false,
            Some(&AssetMetaFilter::HasNoTags { tag_count: 0 }),
        )
        .expect("zero tags");
        assert_eq!(zero_tags.total, 1);
        assert_eq!(zero_tags.items[0].path, "C:\\media\\zero.jpg");

        let two_tags = list_assets_with_meta(
            &conn,
            0,
            50,
            &[],
            &[],
            None,
            false,
            Some(&AssetMetaFilter::HasNoTags { tag_count: 2 }),
        )
        .expect("two tags");
        assert_eq!(two_tags.total, 1);
        assert_eq!(two_tags.items[0].path, "C:\\media\\two.jpg");
    }

    #[test]
    fn list_assets_filters_group_name_as_exact_case_insensitive_match() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\match-a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\match-b.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\other.jpg", "image", 10);

        set_asset_media_group(&conn, 1, Some("Trip-2026"), Some(1.0)).expect("group a");
        set_asset_media_group(&conn, 2, Some("trip-2026"), Some(2.0)).expect("group b");
        set_asset_media_group(&conn, 3, Some("Trip-2027"), Some(1.0)).expect("group other");

        let page = list_assets_with_meta(
            &conn,
            0,
            50,
            &[],
            &[],
            None,
            false,
            Some(&AssetMetaFilter::GroupName {
                group_name: "  TRIP-2026  ".to_string(),
            }),
        )
        .expect("group match");

        let paths = page
            .items
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(page.total, 2);
        assert_eq!(
            paths,
            vec!["C:\\media\\match-a.jpg", "C:\\media\\match-b.jpg"]
        );
    }

    #[test]
    fn ordered_asset_ids_preserve_group_and_singleton_sort_tuple() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        for (path, modified_at) in [
            ("C:\\media\\group-a-second.jpg", 100),
            ("C:\\media\\group-a-null.jpg", 90),
            ("C:\\media\\group-a-first.jpg", 80),
            ("C:\\media\\null-key.jpg", 95),
            ("C:\\media\\blank-key.jpg", 110),
            ("C:\\media\\whitespace-key.jpg", 100),
            ("C:\\media\\case-second.jpg", 70),
            ("C:\\media\\case-first.jpg", 70),
            ("C:\\media\\lowercase-case.jpg", 70),
            ("C:\\media\\tie-singleton.jpg", 70),
            ("C:\\media\\group-b-null.jpg", 60),
            ("C:\\media\\group-b-ordered.jpg", 61),
        ] {
            insert_asset(&conn, path, "image", modified_at);
        }

        for (id, group_key, group_order) in [
            (1, Some("group-a"), Some(2.0)),
            (2, Some("group-a"), None),
            (3, Some("group-a"), Some(1.0)),
            (4, None, Some(-10.0)),
            (5, Some(""), Some(-20.0)),
            (6, Some("   "), None),
            (7, Some("Case"), Some(2.0)),
            (8, Some("Case"), Some(1.0)),
            (9, Some("case"), None),
            (10, None, None),
            (11, Some("group-b"), None),
            (12, Some("group-b"), Some(2.0)),
        ] {
            conn.execute(
                "UPDATE assets SET media_group_key = ?1, media_group_order = ?2 WHERE id = ?3",
                params![group_key, group_order, id],
            )
            .expect("seed raw group values");
        }

        let ids = list_ordered_asset_ids_with_meta(&conn, &[], &[], None, false, None)
            .expect("ordered ids");

        assert_eq!(ids, vec![5, 6, 3, 1, 2, 4, 10, 9, 8, 7, 12, 11]);
    }

    #[test]
    fn ordered_asset_ids_coerce_text_and_blob_group_keys_like_legacy_buckets() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\text-group.jpg", "image", 50);
        insert_asset(&conn, "C:\\media\\blob-group.jpg", "image", 200);
        insert_asset(&conn, "C:\\media\\singleton.jpg", "image", 100);

        conn.execute(
            "UPDATE assets SET media_group_key = 'same', media_group_order = 2 WHERE id = 1",
            [],
        )
        .expect("seed text group key");
        conn.execute(
            "UPDATE assets SET media_group_key = x'73616d65', media_group_order = 1 WHERE id = 2",
            [],
        )
        .expect("seed blob group key");

        let storage_classes = conn
            .prepare("SELECT typeof(media_group_key) FROM assets WHERE id IN (1, 2) ORDER BY id")
            .expect("storage class query")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("storage class rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("storage classes");
        assert_eq!(storage_classes, vec!["text", "blob"]);

        let ids = list_ordered_asset_ids_with_meta(&conn, &[], &[], None, false, None)
            .expect("ordered ids");

        assert_eq!(ids, vec![2, 1, 3]);
    }

    #[test]
    fn grouped_bucket_stats_exclude_singleton_buckets() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        for (path, modified_at) in [
            ("C:\\media\\null.jpg", 50),
            ("C:\\media\\empty.jpg", 40),
            ("C:\\media\\blank.jpg", 30),
            ("C:\\media\\group-a.jpg", 20),
            ("C:\\media\\group-b.jpg", 10),
        ] {
            insert_asset(&conn, path, "image", modified_at);
        }
        conn.execute("UPDATE assets SET media_group_key = '' WHERE id = 2", [])
            .expect("seed empty key");
        conn.execute("UPDATE assets SET media_group_key = '   ' WHERE id = 3", [])
            .expect("seed blank key");
        conn.execute(
            "UPDATE assets SET media_group_key = 'real-group' WHERE id IN (4, 5)",
            [],
        )
        .expect("seed real group");

        let sql = format!(
            "WITH filtered_assets AS (
               SELECT id, modified_at, media_group_key FROM assets
             ),
             grouped_bucket_stats AS (
               {}
             )
             SELECT COUNT(*), MIN(sort_bucket) FROM grouped_bucket_stats",
            grouped_bucket_stats_sql()
        );
        let (bucket_count, bucket): (i64, String) = conn
            .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("grouped bucket stats");

        assert_eq!(bucket_count, 1);
        assert_eq!(bucket, "group:real-group");
    }

    #[test]
    fn ordered_asset_ids_filter_before_computing_group_stats() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        for (path, kind, modified_at) in [
            ("C:\\media\\excluded-new-group-member.mp4", "video", 200),
            ("C:\\media\\included-group-second.jpg", "image", 50),
            ("C:\\media\\included-singleton.jpg", "image", 100),
            ("C:\\media\\included-group-first.jpg", "image", 40),
            ("C:\\media\\excluded-dog.jpg", "image", 110),
            ("C:\\media\\excluded-not-favorite.jpg", "image", 120),
        ] {
            insert_asset(&conn, path, kind, modified_at);
        }

        set_asset_media_group(&conn, 1, Some("trip"), Some(0.0)).expect("excluded group");
        set_asset_media_group(&conn, 2, Some("trip"), Some(2.0)).expect("group second");
        set_asset_media_group(&conn, 4, Some("trip"), Some(1.0)).expect("group first");

        for id in 1..=6 {
            set_asset_tags(&conn, id, &["cat".to_string()]).expect("cat tag");
        }
        set_asset_tags(&conn, 5, &["cat".to_string(), "dog".to_string()])
            .expect("excluded dog tags");
        for id in [2, 3, 4, 5] {
            set_asset_favorite(&conn, id, true).expect("favorite");
        }

        let ids = list_ordered_asset_ids_with_meta(
            &conn,
            &["cat".to_string()],
            &["dog".to_string()],
            Some("image"),
            true,
            Some(&AssetMetaFilter::HasNoTags { tag_count: 1 }),
        )
        .expect("filtered ordered ids");

        assert_eq!(ids, vec![3, 4, 2]);
    }

    #[test]
    fn init_schema_adds_new_columns_to_legacy_assets_table() {
        let conn = Connection::open_in_memory().expect("db");
        conn.execute_batch(
            "
            CREATE TABLE assets (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              path TEXT NOT NULL UNIQUE,
              kind TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              modified_at INTEGER NOT NULL,
              width INTEGER,
              height INTEGER,
              duration_ms INTEGER,
              thumb_path TEXT,
              indexed_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            ",
        )
        .expect("legacy assets schema");

        init_schema(&conn).expect("schema");

        let mut columns = Vec::new();
        let mut stmt = conn
            .prepare("PRAGMA table_info(assets)")
            .expect("table info");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("table info rows");
        for row in rows {
            columns.push(row.expect("column name"));
        }

        assert!(columns.iter().any(|name| name == "is_favorite"));
        assert!(columns.iter().any(|name| name == "media_group_key"));
        assert!(columns.iter().any(|name| name == "media_group_order"));
        assert!(columns.iter().any(|name| name == "file_name"));
    }

    #[test]
    fn upsert_preserves_existing_thumbnail_when_file_timestamp_is_unchanged() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        upsert_asset(
            &conn,
            &NewAsset {
                path: "C:\\media\\a.jpg".to_string(),
                kind: "image".to_string(),
                size_bytes: 1,
                modified_at: 123,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: Some("thumb-old.jpg".to_string()),
            },
        )
        .expect("first upsert");

        upsert_asset(
            &conn,
            &NewAsset {
                path: "C:\\media\\a.jpg".to_string(),
                kind: "image".to_string(),
                size_bytes: 99,
                modified_at: 123,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: Some("thumb-new.jpg".to_string()),
            },
        )
        .expect("second upsert");

        let thumb_path: Option<String> = conn
            .query_row(
                "SELECT thumb_path FROM assets WHERE path = ?1",
                params!["C:\\media\\a.jpg"],
                |row| row.get(0),
            )
            .expect("thumb path");
        assert_eq!(thumb_path.as_deref(), Some("thumb-old.jpg"));
    }

    #[test]
    fn list_tags_page_filters_case_insensitively() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        set_asset_tags(&conn, 1, &["cat".to_string(), "Vacation".to_string()]).expect("tags");

        let filtered = list_tags_page(&conn, "vaC", 0, 100).expect("filtered tags");
        assert_eq!(filtered.items, vec!["vacation".to_string()]);
    }

    #[test]
    fn set_asset_tags_normalizes_defensively_and_rejects_missing_assets() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);

        set_asset_tags(
            &conn,
            1,
            &[
                " Cat ".to_string(),
                "cat".to_string(),
                "".to_string(),
                "new york".to_string(),
            ],
        )
        .expect("normalized tags");
        let count: i64 = conn
            .query_row("SELECT tag_count FROM assets WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let mapping_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM asset_tags WHERE asset_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(mapping_count, 2);
        assert!(set_asset_tags(&conn, 999, &[]).is_err());
        assert!(set_asset_tags(&conn, 999, &["cat".to_string()]).is_err());
    }

    #[test]
    fn semantic_noop_repairs_tag_count_and_canonicalizes_legacy_tag_spelling() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        conn.execute("INSERT INTO tags(name) VALUES ('Cat')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) SELECT 1, id FROM tags WHERE name = 'Cat'",
            [],
        )
        .unwrap();
        conn.execute("UPDATE assets SET tag_count = 99 WHERE id = 1", [])
            .unwrap();
        let initial = current_library_revision(&conn).unwrap();

        let repaired = set_asset_tags_with_revision(&mut conn, 1, &[" CAT ".to_string()]).unwrap();
        assert!(repaired.changed);
        assert_eq!(repaired.tags, vec!["cat".to_string()]);
        assert_eq!(repaired.revision, initial + 1);
        let stored: (String, i64) = conn
            .query_row(
                "SELECT t.name, a.tag_count FROM assets a
                 JOIN asset_tags at ON at.asset_id = a.id
                 JOIN tags t ON t.id = at.tag_id WHERE a.id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, ("cat".to_string(), 1));

        let noop = set_asset_tags_with_revision(&mut conn, 1, &["cat".to_string()]).unwrap();
        assert!(!noop.changed);
        assert_eq!(noop.revision, repaired.revision);
    }

    #[test]
    fn canonicalization_consolidates_colliding_rows_and_preserves_shared_mappings() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 2);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 1);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 1);
        conn.execute("INSERT INTO tags(name) VALUES ('cat')", [])
            .unwrap();
        conn.execute("INSERT INTO tags(name) VALUES (' Cat ')", [])
            .unwrap();
        let canonical_id: i64 = conn
            .query_row(
                "SELECT id FROM tags WHERE name = 'cat' COLLATE BINARY",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let legacy_id: i64 = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ' Cat ' COLLATE BINARY",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) VALUES (1, ?1), (1, ?2), (2, ?2), (3, ?1)",
            params![canonical_id, legacy_id],
        )
        .unwrap();
        conn.execute("UPDATE assets SET tag_count = 2 WHERE id = 1", [])
            .unwrap();
        conn.execute("UPDATE assets SET tag_count = 1 WHERE id IN (2, 3)", [])
            .unwrap();

        let result = set_asset_tags_with_revision(&mut conn, 1, &["cat".to_string()]).unwrap();
        assert!(result.changed);
        assert_eq!(result.tags, vec!["cat".to_string()]);
        let tag_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE lower(trim(name)) = 'cat'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag_rows, 1);
        let mappings = conn
            .prepare(
                "SELECT at.asset_id, t.name FROM asset_tags at JOIN tags t ON t.id = at.tag_id
                 ORDER BY at.asset_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            mappings,
            vec![
                (1, "cat".to_string()),
                (2, "cat".to_string()),
                (3, "cat".to_string()),
            ]
        );
        let counts = conn
            .prepare("SELECT id, tag_count FROM assets ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(counts, vec![(1, 1), (2, 1), (3, 1)]);
    }

    #[test]
    fn tag_revision_mutations_are_noop_aware_and_return_partial_bulk_results() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);
        let initial = current_library_revision(&conn).unwrap();
        let first = set_asset_tags_with_revision(&mut conn, 1, &["cat".to_string()]).unwrap();
        assert!(first.changed);
        assert_eq!(first.revision, initial + 1);
        let noop = set_asset_tags_with_revision(&mut conn, 1, &[" CAT ".to_string()]).unwrap();
        assert!(!noop.changed);
        assert_eq!(noop.revision, first.revision);

        let (results, revision) =
            merge_asset_tags_bulk_with_revision(&mut conn, &[1, 999, 2], &["travel".to_string()])
                .unwrap();
        assert_eq!(
            results
                .iter()
                .map(|result| result.asset_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(results.iter().all(|result| result.changed));
        assert_eq!(revision, first.revision + 1);
    }

    #[test]
    fn bulk_tag_merge_with_revision_short_circuits_empty_inputs_without_repairs() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        conn.execute("INSERT INTO tags(name) VALUES (' Legacy ')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) SELECT 1, id FROM tags WHERE name = ' Legacy '",
            [],
        )
        .unwrap();
        conn.execute("UPDATE assets SET tag_count = 99 WHERE id = 1", [])
            .unwrap();
        let initial_revision = current_library_revision(&conn).unwrap();

        let (empty_id_results, empty_id_revision) =
            merge_asset_tags_bulk_with_revision(&mut conn, &[], &["travel".to_string()]).unwrap();
        let (blank_tag_results, blank_tag_revision) = merge_asset_tags_bulk_with_revision(
            &mut conn,
            &[1],
            &["  ".to_string(), "".to_string()],
        )
        .unwrap();

        assert!(empty_id_results.is_empty());
        assert!(blank_tag_results.is_empty());
        assert_eq!(empty_id_revision, initial_revision);
        assert_eq!(blank_tag_revision, initial_revision);
        let stored: (String, i64) = conn
            .query_row(
                "SELECT t.name, a.tag_count FROM assets a
                 JOIN asset_tags at ON at.asset_id = a.id
                 JOIN tags t ON t.id = at.tag_id WHERE a.id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (" Legacy ".to_string(), 99));
    }

    #[test]
    fn merge_asset_tags_bulk_merges_tags_and_is_idempotent_for_duplicates() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 3);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);

        set_asset_tags(&conn, 1, &["cat".to_string()]).expect("set tags 1");
        set_asset_tags(&conn, 2, &["dog".to_string(), "cat".to_string()]).expect("set tags 2");

        let (processed, updated) = merge_asset_tags_bulk(
            &conn,
            &[1, 2, 999],
            &[
                "cat".to_string(),
                "travel".to_string(),
                "  TRAVEL ".to_string(),
            ],
        )
        .expect("merge bulk");

        assert_eq!(processed, 2);
        assert_eq!(updated, 2);

        let a_tags = list_assets(&conn, 0, 10, &[], &[], None, false)
            .expect("list")
            .items
            .into_iter()
            .find(|asset| asset.id == 1)
            .expect("asset 1")
            .tags;
        let b_tags = list_assets(&conn, 0, 10, &[], &[], None, false)
            .expect("list")
            .items
            .into_iter()
            .find(|asset| asset.id == 2)
            .expect("asset 2")
            .tags;

        assert!(a_tags.iter().any(|tag| tag == "cat"));
        assert!(a_tags.iter().any(|tag| tag == "travel"));
        assert!(b_tags.iter().any(|tag| tag == "dog"));
        assert!(b_tags.iter().any(|tag| tag == "travel"));

        let (processed_second, updated_second) =
            merge_asset_tags_bulk(&conn, &[1, 2], &["travel".to_string(), "cat".to_string()])
                .expect("merge second");

        assert_eq!(processed_second, 2);
        assert_eq!(updated_second, 0);
    }

    #[test]
    fn set_asset_media_group_updates_values() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);

        set_asset_media_group(&conn, 1, Some("group-alpha"), Some(10.5)).expect("set media group");

        let (group_key, group_order) = get_asset_media_group(&conn, 1).expect("get media group");
        assert_eq!(group_key.as_deref(), Some("group-alpha"));
        assert_eq!(group_order, Some(10.5));

        set_asset_media_group(&conn, 1, None, None).expect("clear media group");
        let (group_key_cleared, group_order_cleared) =
            get_asset_media_group(&conn, 1).expect("get cleared media group");
        assert_eq!(group_key_cleared, None);
        assert_eq!(group_order_cleared, None);
    }

    #[test]
    fn set_assets_media_group_bulk_updates_multiple_assets_in_order() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 3);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 1);
        set_asset_media_group(&conn, 1, Some("legacy"), Some(7.0)).expect("seed group");

        let (processed, updated) = set_assets_media_group_bulk(
            &conn,
            &[(2, Some(1.0)), (1, Some(2.0)), (999, Some(3.0))],
            Some("trip-2026"),
        )
        .expect("bulk set");

        assert_eq!(processed, 2);
        assert_eq!(updated, 2);

        let page = list_assets(&conn, 0, 10, &[], &[], None, false).expect("list");
        let first = page
            .items
            .iter()
            .find(|asset| asset.id == 1)
            .expect("first asset");
        let second = page
            .items
            .iter()
            .find(|asset| asset.id == 2)
            .expect("second asset");
        let third = page
            .items
            .iter()
            .find(|asset| asset.id == 3)
            .expect("third asset");

        assert_eq!(first.media_group_key.as_deref(), Some("trip-2026"));
        assert_eq!(first.media_group_order, Some(2.0));
        assert_eq!(second.media_group_key.as_deref(), Some("trip-2026"));
        assert_eq!(second.media_group_order, Some(1.0));
        assert_eq!(third.media_group_key, None);
        assert_eq!(third.media_group_order, None);
    }

    #[test]
    fn set_assets_media_group_bulk_clears_keys_and_orders() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 2);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 1);
        set_asset_media_group(&conn, 1, Some("legacy-a"), Some(4.0)).expect("seed first group");
        set_asset_media_group(&conn, 2, Some("legacy-b"), Some(7.0)).expect("seed second group");

        let (processed, updated) =
            set_assets_media_group_bulk(&conn, &[(1, None), (2, None)], None)
                .expect("clear bulk groups");

        assert_eq!(processed, 2);
        assert_eq!(updated, 2);
        assert_eq!(
            get_asset_media_group(&conn, 1).expect("first group"),
            (None, None)
        );
        assert_eq!(
            get_asset_media_group(&conn, 2).expect("second group"),
            (None, None)
        );
    }

    #[test]
    fn list_assets_for_csv_export_includes_media_group_fields() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        set_asset_tags(&conn, 1, &["cat".to_string()]).expect("set tags");
        set_asset_favorite(&conn, 1, true).expect("set favorite");
        set_asset_media_group(&conn, 1, Some("group-alpha"), Some(7.25)).expect("set media group");

        let rows = list_assets_for_csv_export(&conn).expect("csv rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].media_group_key.as_deref(), Some("group-alpha"));
        assert_eq!(rows[0].media_group_order, Some(7.25));
    }

    #[test]
    fn list_assets_for_thumbnail_render_includes_gifs() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.gif", "gif", 1);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);

        let assets = list_assets_for_thumbnail_render(&conn).expect("thumbnail assets");
        let kinds = assets
            .into_iter()
            .map(|asset| asset.kind)
            .collect::<Vec<_>>();

        assert_eq!(kinds, vec!["gif".to_string(), "image".to_string()]);
    }

    #[test]
    fn thumbnail_failures_track_and_filter_by_asset_version() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);

        record_thumbnail_failure(&conn, 1, 1, Some("decode error")).expect("record fail a");
        record_thumbnail_failure(&conn, 1, 1, Some("decode error again"))
            .expect("record fail a again");
        record_thumbnail_failure(&conn, 2, 2, Some("missing source")).expect("record fail b");

        let failed_ids = list_failed_thumbnail_asset_ids(&conn).expect("failed ids");
        assert_eq!(failed_ids, vec![1, 2]);

        let failed_assets = list_failed_assets_for_thumbnail_render(&conn).expect("failed assets");
        assert_eq!(failed_assets.len(), 2);
        assert_eq!(failed_assets[0].id, 1);
        assert_eq!(failed_assets[1].id, 2);

        upsert_asset(
            &conn,
            &NewAsset {
                path: "C:\\media\\a.jpg".to_string(),
                kind: "image".to_string(),
                size_bytes: 1,
                modified_at: 99,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            },
        )
        .expect("upsert changed a");

        let failed_ids_after_change =
            list_failed_thumbnail_asset_ids(&conn).expect("failed ids after change");
        assert_eq!(failed_ids_after_change, vec![2]);

        clear_thumbnail_failure(&conn, 2).expect("clear fail b");
        let final_failed = list_failed_thumbnail_asset_ids(&conn).expect("final failed ids");
        assert!(final_failed.is_empty());
    }

    #[test]
    fn delete_by_prefix_removes_thumbnail_failures() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 10);
        insert_asset(&conn, "C:\\other\\b.jpg", "image", 11);

        record_thumbnail_failure(&conn, 1, 10, Some("broken a")).expect("record a");
        record_thumbnail_failure(&conn, 2, 11, Some("broken b")).expect("record b");

        let (_removed, _thumbs) =
            delete_assets_by_prefix_with_thumbs(&conn, "C:\\media").expect("delete media");

        let failed_ids = list_failed_thumbnail_asset_ids(&conn).expect("failed ids after delete");
        assert_eq!(failed_ids, vec![2]);
    }

    #[test]
    fn delete_asset_by_id_returns_paths_and_removes_row() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        conn.execute(
            "UPDATE assets SET thumb_path = ?1 WHERE id = ?2",
            params!["thumb-a.jpg", 1],
        )
        .expect("set thumb");

        let result = delete_asset_by_id_with_thumb(&conn, 1).expect("delete by id");
        assert_eq!(
            result,
            Some((
                "C:\\media\\a.jpg".to_string(),
                Some("thumb-a.jpg".to_string())
            ))
        );

        let page = list_assets(&conn, 0, 10, &[], &[], None, false).expect("list after delete");
        assert!(page.items.is_empty());
    }

    #[test]
    fn delete_asset_by_id_returns_none_for_missing_asset() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        let result = delete_asset_by_id_with_thumb(&conn, 999).expect("delete missing");
        assert!(result.is_none());
    }

    #[test]
    fn duplicate_file_name_queries_return_expected_groups() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a\\same.jpg", "image", 10);
        insert_asset(&conn, "C:\\media\\b\\same.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\c\\other.jpg", "image", 30);

        let groups = list_duplicate_file_name_counts(&conn).expect("duplicate groups");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].file_name_key, "same.jpg");
        assert_eq!(groups[0].asset_count, 2);

        let assets =
            list_duplicate_assets_by_file_name_key(&conn, "same.jpg").expect("duplicate assets");
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].path, "C:\\media\\b\\same.jpg");
        assert_eq!(assets[1].path, "C:\\media\\a\\same.jpg");
    }

    #[test]
    fn rename_asset_file_updates_path_and_clears_thumbnail_reference() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\same.jpg", "image", 1);
        conn.execute(
            "UPDATE assets SET thumb_path = ?1 WHERE id = ?2",
            params!["thumb-same.jpg", 1],
        )
        .expect("set thumb");

        let result = rename_asset_file_by_id(&conn, 1, "C:\\media\\renamed.jpg", "renamed.jpg")
            .expect("rename in db");
        assert_eq!(
            result,
            Some((
                "C:\\media\\same.jpg".to_string(),
                Some("thumb-same.jpg".to_string())
            ))
        );

        let updated = get_asset_path_and_thumb_by_id(&conn, 1)
            .expect("query asset")
            .expect("asset exists");
        assert_eq!(updated.0, "C:\\media\\renamed.jpg");
        assert_eq!(updated.1, None);

        let file_name: String = conn
            .query_row("SELECT file_name FROM assets WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("file name");
        assert_eq!(file_name, "renamed.jpg");
    }
}
