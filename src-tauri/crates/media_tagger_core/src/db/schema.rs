use super::*;

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
    // Before any write: a restored database could carry a trigger under one of
    // these names with a different body. They are recreated below.
    drop_sort_key_triggers(conn)?;
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
          path TEXT PRIMARY KEY,
          auto_scan_on_startup INTEGER NOT NULL DEFAULT 0 CHECK(auto_scan_on_startup IN (0, 1))
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

        CREATE INDEX IF NOT EXISTS idx_tags_normalized_name ON tags(lower(trim(name)));
        CREATE INDEX IF NOT EXISTS idx_thumbnail_failures_last_failed_at ON thumbnail_failures(last_failed_at DESC);
        ",
    )?;
    ensure_assets_is_favorite_column(conn)?;
    if !conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('scan_roots') WHERE name = 'auto_scan_on_startup')",
        [], |row| row.get::<_, bool>(0),
    )? {
        conn.execute(
            "ALTER TABLE scan_roots ADD COLUMN auto_scan_on_startup INTEGER NOT NULL DEFAULT 0 CHECK(auto_scan_on_startup IN (0, 1))",
            [],
        )?;
    }
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

    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_assets_gallery_kind_favorite
          ON assets(kind, is_favorite, modified_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_assets_tag_count
          ON assets(tag_count, modified_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_assets_file_name_key
          ON assets(file_name_key, modified_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_asset_tags_tag_asset
          ON asset_tags(tag_id, asset_id);
        CREATE INDEX IF NOT EXISTS idx_asset_scan_roots_generation
          ON asset_scan_roots(root_path, last_seen_generation, asset_id);
        ",
    )?;
    conn.execute_batch(GROUP_NORMALIZED_INDEX_SQL)?;
    if performance_schema_version < 2 {
        backfill_asset_scan_roots(conn)?;
        conn.execute(
            "UPDATE library_metadata SET value = 2 WHERE key = 'performance_schema_version'",
            [],
        )?;
    }
    if performance_schema_version < 3 {
        migrate_canonical_keys_and_tags(conn)?;
    }
    // A committed deferral marker would disable the bucket triggers; it can
    // only come from an interrupted writer bug or a crafted database.
    let stray_deferral = conn.execute(
        "DELETE FROM library_metadata WHERE key = 'sort_keys_deferred'",
        [],
    )? > 0;
    if performance_schema_version < 4 || stray_deferral {
        backfill_sort_keys(conn)?;
    }
    // Derived ordering keys are maintained by triggers. They are recreated on
    // every start so their SQL is always the canonical text below, including
    // in a restored backup (ADR 0004).
    install_sort_key_triggers(conn)?;
    conn.execute_batch(SORT_KEY_INDEX_SQL)?;
    conn.execute_batch(
        "
        DROP INDEX IF EXISTS idx_assets_kind;
        DROP INDEX IF EXISTS idx_assets_modified_at;
        DROP INDEX IF EXISTS idx_tags_name;
        DROP INDEX IF EXISTS idx_asset_tags_tag;
        DROP INDEX IF EXISTS idx_asset_tags_asset;
        ",
    )?;
    optimize(conn)?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    if conn.query_row("SELECT COUNT(*) FROM pragma_table_info('thumbnail_failures') WHERE name = 'source_record_version'", [], |row| row.get::<_, i64>(0))? == 0 {
        conn.execute_batch("ALTER TABLE thumbnail_failures ADD COLUMN source_record_version INTEGER NOT NULL DEFAULT 0; DELETE FROM thumbnail_failures;")?;
    }
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    rebuild_query_revisions(conn)?;

    Ok(())
}

pub fn validate_backup_database(
    conn: &Connection,
    allow_legacy: bool,
) -> anyhow::Result<BackupSchemaCompatibility> {
    let compatibility = validate_backup_database_except_journal(conn, allow_legacy)?;
    ensure_no_pending_file_operations(conn)?;
    Ok(compatibility)
}

/// Validation of the generation a restore is about to install or has installed (ADR 0030).
///
/// A bundle never carries a file-mutation journal, so `validate_backup_database` rejects any
/// row. The generation a restore builds additionally holds the rows the restore copied from
/// the live database: every other check is the same, and `pending_file_operations` must hold
/// exactly `carried`, in journal order.
pub fn validate_restored_generation(
    conn: &Connection,
    carried: &[PendingFileOperation],
) -> anyhow::Result<()> {
    validate_backup_database_except_journal(conn, false)?;
    anyhow::ensure!(
        list_pending_file_operations(conn)? == carried,
        "restored database does not hold exactly the carried local file operations"
    );
    Ok(())
}

fn validate_backup_database_except_journal(
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
        match schema_version {
            SCHEMA_VERSION => BackupSchemaCompatibility::Current,
            1 => BackupSchemaCompatibility::Version1,
            2 => BackupSchemaCompatibility::Version2,
            _ => anyhow::bail!("unsupported Tagrove schema version {schema_version}"),
        }
    } else if application_id == 0 && schema_version == 0 && allow_legacy {
        BackupSchemaCompatibility::Legacy
    } else if application_id == 0 {
        anyhow::bail!("database has no Tagrove application identity");
    } else {
        anyhow::bail!("database belongs to another application");
    };

    if matches!(
        compatibility,
        BackupSchemaCompatibility::Current | BackupSchemaCompatibility::Version2
    ) {
        validate_table_columns(conn, "scan_roots", &["auto_scan_on_startup"])?;
        validate_column_constraints(conn, "scan_roots", "auto_scan_on_startup", true, 0)?;
        let invalid: i64 = conn.query_row(
            "SELECT COUNT(*) FROM scan_roots WHERE typeof(auto_scan_on_startup) <> 'integer' OR auto_scan_on_startup NOT IN (0, 1)",
            [], |row| row.get(0),
        )?;
        anyhow::ensure!(
            invalid == 0,
            "backup contains invalid startup scan preferences"
        );
    }
    if compatibility == BackupSchemaCompatibility::Current {
        validate_table_columns(conn, "thumbnail_failures", &["source_record_version"])?;
        validate_column_constraints(conn, "thumbnail_failures", "source_record_version", true, 0)?;
        let invalid: i64 = conn.query_row("SELECT COUNT(*) FROM thumbnail_failures WHERE typeof(source_record_version) <> 'integer' OR source_record_version < 1", [], |row| row.get(0))?;
        anyhow::ensure!(
            invalid == 0,
            "backup contains invalid thumbnail source versions"
        );
    }
    validate_backup_schema(conn, compatibility)?;
    validate_backup_data(conn, compatibility)?;
    Ok(compatibility)
}

pub(super) fn validate_backup_schema(
    conn: &Connection,
    compatibility: BackupSchemaCompatibility,
) -> anyhow::Result<()> {
    let required_tables = [
        (
            "assets",
            &[
                "id",
                "path",
                "kind",
                "size_bytes",
                "modified_at",
                "thumb_path",
            ][..],
        ),
        ("tags", &["id", "name"][..]),
        ("asset_tags", &["asset_id", "tag_id"][..]),
        ("scan_roots", &["path"][..]),
        (
            "thumbnail_failures",
            &[
                "asset_id",
                "failure_count",
                "last_failed_at",
                "asset_modified_at",
            ][..],
        ),
        ("library_metadata", &["key", "value"][..]),
    ];
    for (table, columns) in required_tables {
        validate_table_columns(conn, table, columns)?;
    }
    validate_backup_constraints(conn, compatibility)?;

    if compatibility != BackupSchemaCompatibility::Legacy {
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

pub(super) fn validate_backup_constraints(
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
    if compatibility != BackupSchemaCompatibility::Legacy {
        // `file_name` is `NOT NULL` in a created schema, but earlier builds
        // added it to a legacy table as a nullable column and SQLite cannot
        // change that declaration afterwards. Both shapes are accepted;
        // `validate_backup_data` rejects a NULL or blank value either way
        // (ADR 0015).
        let (_, file_name_key_position) = column_constraints(conn, "assets", "file_name")?;
        anyhow::ensure!(
            file_name_key_position == 0,
            "backup column assets.file_name has incompatible constraints"
        );
        for (table, column, not_null, primary_key_position) in [
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
            has_unique_index(
                conn,
                "pending_file_operations",
                &["operation_id", "asset_id"]
            )?,
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

pub(super) fn validate_column_constraints(
    conn: &Connection,
    table: &str,
    expected_column: &str,
    expected_not_null: bool,
    expected_primary_key_position: i64,
) -> anyhow::Result<()> {
    let (not_null, primary_key_position) = column_constraints(conn, table, expected_column)?;
    anyhow::ensure!(
        not_null == expected_not_null && primary_key_position == expected_primary_key_position,
        "backup column {table}.{expected_column} has incompatible constraints"
    );
    Ok(())
}

/// The declared `NOT NULL` flag and primary-key position (0 when the column is
/// not part of the key) of one column.
fn column_constraints(
    conn: &Connection,
    table: &str,
    expected_column: &str,
) -> anyhow::Result<(bool, i64)> {
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
            return Ok((not_null, primary_key_position));
        }
    }
    anyhow::bail!("backup table {table} is missing required column {expected_column}")
}

pub(super) fn has_unique_index(
    conn: &Connection,
    table: &str,
    expected: &[&str],
) -> anyhow::Result<bool> {
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
                let mut xinfo_stmt =
                    conn.prepare(&format!("PRAGMA index_xinfo(\"{escaped_name}\")"))?;
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

pub(super) fn has_cascade_foreign_key(
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

pub(super) fn validate_table_columns(
    conn: &Connection,
    table: &str,
    required: &[&str],
) -> anyhow::Result<()> {
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
            "path"
            | "kind"
            | "thumb_path"
            | "name"
            | "key"
            | "file_name"
            | "media_group_key"
            | "file_name_key"
            | "media_group_key_normalized"
            | "root_path"
            | "operation_id"
            | "action"
            | "original_path"
            | "staging_path"
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

pub(super) fn validate_backup_data(
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

    // Only the trigger set this application installs may exist; init_schema
    // drops and recreates those by name before writing (ADR 0004).
    let mut stmt = conn.prepare(
        "SELECT type, name FROM sqlite_schema WHERE type IN ('trigger', 'view') ORDER BY name",
    )?;
    let objects = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (object_type, name) in objects {
        anyhow::ensure!(
            object_type == "trigger" && SORT_KEY_TRIGGERS.contains(&name.as_str()),
            "backup contains an unexpected database {object_type}"
        );
    }

    let metadata_rows = conn.query_row(
        "SELECT COUNT(*) FROM library_metadata
         WHERE (key = 'revision' AND value >= 1)
             OR (key = 'performance_schema_version' AND value BETWEEN 0 AND 4)",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    anyhow::ensure!(
        metadata_rows == 2,
        "backup has incompatible Tagrove metadata"
    );

    if compatibility != BackupSchemaCompatibility::Legacy {
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
        let performance_schema_version = conn.query_row(
            "SELECT value FROM library_metadata WHERE key = 'performance_schema_version'",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        let invalid_derived_assets = conn.query_row(
            "SELECT COUNT(*) FROM assets
              WHERE tag_count <> (SELECT COUNT(*) FROM asset_tags WHERE asset_id = assets.id)
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
        if performance_schema_version >= 4 {
            validate_table_columns(
                conn,
                "assets",
                &[
                    "sort_bucket_time",
                    "sort_bucket_id",
                    "sort_group_null_order",
                ],
            )?;
            anyhow::ensure!(
                count_inconsistent_sort_keys(conn)? == 0,
                "backup contains inconsistent asset ordering keys"
            );
        }
        if performance_schema_version >= 3 {
            let mut stmt = conn.prepare("SELECT file_name, file_name_key FROM assets")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (file_name, file_name_key) = row?;
                anyhow::ensure!(
                    file_name_key == canonical_key(&file_name),
                    "backup contains inconsistent filename keys"
                );
            }
            let mut stmt = conn.prepare("SELECT name FROM tags")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                let name = row?;
                // Versions before the U+FEFF rule could store such a tag (CSV import); a backup
                // that carries one stays restorable (the tag is kept as stored). Everything else
                // about the name must still be canonical.
                let name = name.replace('\u{FEFF}', "");
                if name.is_empty() {
                    // Only U+FEFF: the old rule stored and accepted it as a tag; keep it restorable.
                    continue;
                }
                let normalized = normalize_and_validate_tags(vec![name.clone()])?;
                anyhow::ensure!(
                    normalized.len() == 1 && normalized[0] == name,
                    "backup contains noncanonical tag names"
                );
            }
        } else {
            let invalid_filename_keys = conn.query_row(
                "SELECT COUNT(*) FROM assets WHERE file_name_key <> lower(file_name)",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            anyhow::ensure!(
                invalid_filename_keys == 0,
                "backup contains inconsistent filename keys"
            );
        }
        let (table, predicate) = (
            "asset_scan_roots",
            "typeof(asset_id) <> 'integer' OR typeof(root_path) <> 'text'
                 OR typeof(last_seen_generation) <> 'integer'",
        );
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {predicate}");
        let invalid = conn.query_row(&sql, [], |row| row.get::<_, i64>(0))?;
        anyhow::ensure!(invalid == 0, "backup table {table} contains invalid values");
    }
    Ok(())
}

pub(super) fn backfill_asset_scan_roots(conn: &Connection) -> anyhow::Result<()> {
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

pub(super) fn ensure_performance_columns(conn: &Connection) -> anyhow::Result<()> {
    for (name, definition) in [
        ("fingerprint_mtime_ns", "INTEGER NOT NULL DEFAULT 0"),
        ("tag_count", "INTEGER NOT NULL DEFAULT 0"),
        ("file_name_key", "TEXT NOT NULL DEFAULT ''"),
        ("media_group_key_normalized", "TEXT"),
        ("record_version", "INTEGER NOT NULL DEFAULT 1"),
        ("sort_bucket_time", "INTEGER"),
        ("sort_bucket_id", "INTEGER"),
        ("sort_group_null_order", "INTEGER"),
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

pub(super) fn ensure_assets_is_favorite_column(conn: &Connection) -> anyhow::Result<()> {
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

pub(super) fn ensure_assets_media_group_key_column(conn: &Connection) -> anyhow::Result<()> {
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

pub(super) fn ensure_assets_media_group_order_column(conn: &Connection) -> anyhow::Result<()> {
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

pub(super) fn ensure_assets_file_name_column(conn: &Connection) -> anyhow::Result<()> {
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
        // Same declaration as the fresh schema. Earlier builds added a plain
        // nullable `TEXT` here; backup validation still accepts that shape.
        conn.execute(
            "ALTER TABLE assets ADD COLUMN file_name TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }

    // The gate below needs this index to stay a bounded search on a library
    // where every name is filled. Created here rather than later in
    // `init_schema` so the first start after the column exists already uses it.
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_assets_file_name ON assets(file_name COLLATE NOCASE)",
        [],
    )?;
    let needs_fill = conn
        .prepare_cached(BLANK_FILE_NAME_EXISTS_SQL)?
        .query_row([], |row| row.get::<_, bool>(0))?;
    if !needs_fill {
        return Ok(());
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

    {
        let mut update = tx.prepare_cached("UPDATE assets SET file_name = ?1 WHERE id = ?2")?;
        for (asset_id, file_name) in updates {
            update.execute(params![file_name, asset_id])?;
        }
    }
    tx.commit()?;

    Ok(())
}

/// True when some row matches the fill predicate
/// `file_name IS NULL OR trim(file_name) = ''`, answered with index searches on
/// `idx_assets_file_name` instead of a scan of `assets`. SQLite `trim` strips
/// only spaces, so a blank text value is empty or starts with a byte below
/// `'!'`; the range `< '!'` therefore holds every blank text value and, on a
/// filled library, only the rare names that start with a space or a control
/// character. Values of TEXT affinity are text, NULL or (from a crafted
/// database) BLOB; BLOB sorts after all text, so the third search covers it.
/// The OR form is split into three EXISTS because SQLite would not use the
/// index for the combined predicate.
pub(super) const BLANK_FILE_NAME_EXISTS_SQL: &str = "
    SELECT EXISTS(SELECT 1 FROM assets WHERE file_name IS NULL)
        OR EXISTS(SELECT 1 FROM assets
                  WHERE file_name < '!' COLLATE NOCASE AND trim(file_name) = '')
        OR EXISTS(SELECT 1 FROM assets
                  WHERE file_name >= x'' COLLATE NOCASE AND trim(file_name) = '')";

pub(super) fn migrate_canonical_keys_and_tags(conn: &Connection) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let mut changed = false;
    // Only rows whose key differs are kept, so a large library does not hold
    // every name in memory.
    let filename_updates = {
        let mut stmt = tx.prepare("SELECT id, file_name, file_name_key FROM assets")?;
        let mut rows = stmt.query([])?;
        let mut updates = Vec::new();
        while let Some(row) = rows.next()? {
            let file_name = row.get_ref(1)?.as_str()?;
            let key = canonical_key(file_name);
            if key != row.get_ref(2)?.as_str()? {
                updates.push((row.get::<_, i64>(0)?, key));
            }
        }
        updates
    };
    {
        let mut update = tx.prepare_cached("UPDATE assets SET file_name_key = ?1 WHERE id = ?2")?;
        for (asset_id, key) in filename_updates {
            update.execute(params![key, asset_id])?;
            changed = true;
        }
    }

    let asset_tags = {
        let asset_ids = assets_needing_tag_migration(&tx)?;
        let mut values = Vec::with_capacity(asset_ids.len());
        for asset_id in asset_ids {
            let legacy_tags = list_asset_tags(&tx, asset_id)?;
            let normalized = normalize_tags(
                legacy_tags
                    .iter()
                    .flat_map(|tag| parse_legacy_tags(tag))
                    .collect(),
            );
            values.push((asset_id, normalized));
        }
        values
    };
    for (asset_id, tags) in asset_tags {
        changed |= set_asset_tags_in_tx(&tx, asset_id, &tags)?.changed;
    }
    cleanup_orphan_tags(&tx)?;
    if changed {
        bump_library_revision_in_tx(&tx)?;
    }
    tx.execute(
        "UPDATE library_metadata SET value = 3 WHERE key = 'performance_schema_version'",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

/// Ids, ascending, of the assets on which the version-3 rewrite
/// (`set_asset_tags_in_tx` with the legacy-split tags) can change anything.
/// For every other asset that call is a no-op, so skipping it leaves the
/// result unchanged. An asset is selected when
/// - it holds a tag of a dirty group: a tag whose name is not already one
///   canonical token (`normalize_tags(parse_legacy_tags(name)) != [name]`),
///   or any tag sharing `lower(trim(name))` with such a tag, which
///   `canonicalize_tag_rows_in_tx` would merge or rename; or
/// - its stored `tag_count` differs from its mapping count, which the rewrite
///   repairs.
///
/// A tag outside a dirty group is canonical and alone in its
/// `lower(trim(name))` group, so an asset holding only such tags with a
/// correct count keeps its rows, its names and its count.
pub(super) fn assets_needing_tag_migration(
    tx: &rusqlite::Transaction<'_>,
) -> anyhow::Result<std::collections::BTreeSet<i64>> {
    let mut dirty_tag_ids = std::collections::BTreeSet::new();
    let noncanonical_names = {
        let mut stmt = tx.prepare("SELECT id, name FROM tags")?;
        let mut rows = stmt.query([])?;
        let mut names = Vec::new();
        while let Some(row) = rows.next()? {
            let Ok(name) = row.get_ref(1)?.as_str() else {
                // Not UTF-8 text (crafted database): its holders go through
                // the rewrite, which fails on it exactly as before.
                dirty_tag_ids.insert(row.get::<_, i64>(0)?);
                continue;
            };
            let tokens = normalize_tags(parse_legacy_tags(name));
            if tokens.len() != 1 || tokens[0] != name {
                names.push(name.to_string());
            }
        }
        names
    };
    {
        let mut group =
            tx.prepare_cached("SELECT id FROM tags WHERE lower(trim(name)) = lower(trim(?1))")?;
        for name in &noncanonical_names {
            let rows = group.query_map(params![name], |row| row.get::<_, i64>(0))?;
            for row in rows {
                dirty_tag_ids.insert(row?);
            }
        }
    }

    let mut asset_ids = std::collections::BTreeSet::new();
    {
        // Joined to `assets` like the previous walk over every asset, so a
        // mapping without its asset (foreign keys off) is ignored as before.
        let mut holders = tx.prepare_cached(
            "SELECT at.asset_id FROM asset_tags at JOIN assets a ON a.id = at.asset_id
             WHERE at.tag_id = ?1",
        )?;
        for tag_id in dirty_tag_ids {
            let rows = holders.query_map(params![tag_id], |row| row.get::<_, i64>(0))?;
            for row in rows {
                asset_ids.insert(row?);
            }
        }
    }
    {
        let mut stmt = tx.prepare(
            "SELECT id FROM assets
             WHERE tag_count <> (SELECT COUNT(*) FROM asset_tags WHERE asset_id = assets.id)",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        for row in rows {
            asset_ids.insert(row?);
        }
    }
    Ok(asset_ids)
}

/// Predicate for an asset that belongs to a media-group bucket, written for the
/// row alias `x`. A NULL, empty, or whitespace-only key is a singleton bucket.
macro_rules! grouped {
    ($x:literal) => {
        concat!(
            $x,
            ".media_group_key IS NOT NULL AND trim(",
            $x,
            ".media_group_key) <> ''"
        )
    };
}

/// Bucket identity: the key coerced to TEXT, so a BLOB key and a TEXT key with
/// the same bytes share one bucket (the legacy `'group:' || key` coercion).
macro_rules! bucket {
    ($x:literal) => {
        concat!("CAST(", $x, ".media_group_key AS TEXT)")
    };
}

/// Recomputes the stored ordering keys of every member of the bucket `$key`
/// (an SQL expression evaluating to the TEXT bucket identity). Only rows whose
/// keys actually change are written. The subqueries are not correlated with
/// the updated row, so SQLite evaluates each once per statement.
macro_rules! recompute_bucket_sql {
    ($key:literal) => {
        concat!(
            "UPDATE assets SET
               sort_bucket_time = (SELECT MAX(g.modified_at) FROM assets g
                 WHERE ",
            grouped!("g"),
            " AND ",
            bucket!("g"),
            " = ",
            $key,
            "),
               sort_bucket_id = (SELECT MAX(g.id) FROM assets g
                 WHERE ",
            grouped!("g"),
            " AND ",
            bucket!("g"),
            " = ",
            $key,
            "),
               sort_group_null_order = (media_group_order IS NULL)
             WHERE ",
            grouped!("assets"),
            " AND ",
            bucket!("assets"),
            " = ",
            $key,
            "
               AND (sort_bucket_time IS NOT (SELECT MAX(g.modified_at) FROM assets g
                      WHERE ",
            grouped!("g"),
            " AND ",
            bucket!("g"),
            " = ",
            $key,
            ")
                 OR sort_bucket_id IS NOT (SELECT MAX(g.id) FROM assets g
                      WHERE ",
            grouped!("g"),
            " AND ",
            bucket!("g"),
            " = ",
            $key,
            ")
                 OR sort_group_null_order IS NOT (media_group_order IS NULL));"
        )
    };
}

const GROUP_NORMALIZED_INDEX_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_assets_group_normalized
          ON assets(media_group_key_normalized, modified_at DESC, id DESC)";
const GROUP_BUCKET_INDEX_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_assets_group_bucket
          ON assets(CAST(media_group_key AS TEXT), modified_at)
          WHERE media_group_key IS NOT NULL AND trim(media_group_key) <> ''";
const SORT_KEY_INDEX_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_assets_sort_key ON assets(
          sort_bucket_time DESC, sort_bucket_id DESC, sort_group_null_order,
          media_group_order, modified_at DESC, id DESC
        )";

/// The `assets` indexes a media-group write changes for every row it touches
/// (key, normalized key, order and the derived ordering keys), with the SQL
/// `init_schema` creates them with.
const GROUP_WRITE_INDEXES: [(&str, &str); 3] = [
    ("idx_assets_group_normalized", GROUP_NORMALIZED_INDEX_SQL),
    ("idx_assets_group_bucket", GROUP_BUCKET_INDEX_SQL),
    ("idx_assets_sort_key", SORT_KEY_INDEX_SQL),
];

/// Runs `write` with the group-write indexes dropped and recreates them
/// afterwards, all inside the caller's transaction (ADR 0026).
///
/// Maintaining these three indexes costs about 2 µs per changed row and index
/// even with a warm cache, and far more once the dirty pages outgrow the page
/// cache; rebuilding one by sorting costs about 0.5 s per 1,000,000 rows. A
/// write that changes a large part of the library is therefore cheaper with
/// the indexes rebuilt. `write` must not need them: it may not look up group
/// buckets or the gallery order. Their `sqlite_stat1` rows, which `DROP INDEX`
/// deletes, are put back and reloaded, so the query planner keeps the same
/// statistics. An error rolls the whole transaction back, indexes included.
pub(crate) fn with_group_write_indexes_rebuilt<T>(
    conn: &Connection,
    write: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let has_stats: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'sqlite_stat1')",
        [],
        |row| row.get(0),
    )?;
    let mut stats = Vec::new();
    if has_stats {
        let mut stmt = conn.prepare(
            "SELECT idx, stat FROM sqlite_stat1 WHERE tbl = 'assets' AND idx IN (?1, ?2, ?3)",
        )?;
        let rows = stmt.query_map(
            params![
                GROUP_WRITE_INDEXES[0].0,
                GROUP_WRITE_INDEXES[1].0,
                GROUP_WRITE_INDEXES[2].0
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        for row in rows {
            stats.push(row?);
        }
    }
    for (name, _) in GROUP_WRITE_INDEXES {
        conn.execute_batch(&format!("DROP INDEX IF EXISTS {name};"))?;
    }
    let result = write()?;
    for (_, sql) in GROUP_WRITE_INDEXES {
        conn.execute_batch(sql)?;
    }
    if !stats.is_empty() {
        let mut insert =
            conn.prepare("INSERT INTO sqlite_stat1(tbl, idx, stat) VALUES ('assets', ?1, ?2)")?;
        for (index, stat) in &stats {
            insert.execute(params![index, stat])?;
        }
        // Loads the restored rows into this connection; other connections
        // read them when they reload the changed schema.
        conn.execute_batch("ANALYZE sqlite_schema;")?;
    }
    Ok(result)
}

/// Names of the triggers that maintain `sort_bucket_time`, `sort_bucket_id`
/// and `sort_group_null_order`. Backup validation accepts no other trigger.
pub const SORT_KEY_TRIGGERS: [&str; 6] = [
    "assets_sort_key_insert_singleton",
    "assets_sort_key_insert_grouped",
    "assets_sort_key_update_singleton",
    "assets_sort_key_update_old_bucket",
    "assets_sort_key_update_new_bucket",
    "assets_sort_key_delete_grouped",
];

const SORT_KEY_CHANGED: &str = "OLD.id IS NOT NEW.id
      OR OLD.modified_at IS NOT NEW.modified_at
      OR OLD.media_group_key IS NOT NEW.media_group_key
      OR OLD.media_group_order IS NOT NEW.media_group_order";

fn drop_sort_key_triggers(conn: &Connection) -> anyhow::Result<()> {
    for name in SORT_KEY_TRIGGERS {
        conn.execute_batch(&format!("DROP TRIGGER IF EXISTS {name};"))?;
    }
    Ok(())
}

pub(super) fn install_sort_key_triggers(conn: &Connection) -> anyhow::Result<()> {
    drop_sort_key_triggers(conn)?;
    let update_of = "AFTER UPDATE OF id, modified_at, media_group_key, media_group_order ON assets";
    conn.execute_batch(&format!(
        "
        {bucket_index};

        CREATE TRIGGER assets_sort_key_insert_singleton AFTER INSERT ON assets
        WHEN NOT ({grouped_new})
        BEGIN
          UPDATE assets SET sort_bucket_time = NEW.modified_at, sort_bucket_id = NEW.id,
                            sort_group_null_order = 0
          WHERE id = NEW.id;
        END;

        CREATE TRIGGER assets_sort_key_insert_grouped AFTER INSERT ON assets
        WHEN {grouped_new} AND {not_deferred}
        BEGIN
          {recompute_new}
        END;

        CREATE TRIGGER assets_sort_key_update_singleton {update_of}
        WHEN ({changed}) AND NOT ({grouped_new})
        BEGIN
          UPDATE assets SET sort_bucket_time = NEW.modified_at, sort_bucket_id = NEW.id,
                            sort_group_null_order = 0
          WHERE id = NEW.id;
        END;

        CREATE TRIGGER assets_sort_key_update_old_bucket {update_of}
        WHEN ({changed}) AND {grouped_old} AND {not_deferred}
        BEGIN
          {recompute_old}
        END;

        CREATE TRIGGER assets_sort_key_update_new_bucket {update_of}
        WHEN ({changed}) AND {grouped_new} AND {not_deferred}
        BEGIN
          {recompute_new}
        END;

        CREATE TRIGGER assets_sort_key_delete_grouped AFTER DELETE ON assets
        WHEN {grouped_old} AND {not_deferred}
        BEGIN
          {recompute_old}
        END;
        ",
        bucket_index = GROUP_BUCKET_INDEX_SQL,
        grouped_new = grouped!("NEW"),
        grouped_old = grouped!("OLD"),
        changed = SORT_KEY_CHANGED,
        not_deferred =
            "NOT EXISTS (SELECT 1 FROM library_metadata WHERE key = 'sort_keys_deferred')",
        recompute_new = recompute_bucket_sql!("CAST(NEW.media_group_key AS TEXT)"),
        recompute_old = recompute_bucket_sql!("CAST(OLD.media_group_key AS TEXT)"),
    ))?;
    Ok(())
}

/// One-time backfill for `performance_schema_version` 4: computes every
/// asset's ordering keys from its whole bucket in one transaction.
pub(super) fn backfill_sort_keys(conn: &Connection) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(concat!(
        "UPDATE assets SET sort_bucket_time = modified_at, sort_bucket_id = id,
                           sort_group_null_order = 0
         WHERE NOT (",
        grouped!("assets"),
        ");
         WITH buckets AS (
           SELECT ",
        bucket!("a"),
        " AS bucket, MAX(a.modified_at) AS bucket_time,
                  MAX(a.id) AS bucket_id
           FROM assets a WHERE ",
        grouped!("a"),
        "
           GROUP BY 1
         )
         UPDATE assets SET sort_bucket_time = buckets.bucket_time,
                           sort_bucket_id = buckets.bucket_id,
                           sort_group_null_order = (assets.media_group_order IS NULL)
         FROM buckets
         WHERE ",
        grouped!("assets"),
        " AND ",
        bucket!("assets"),
        " = buckets.bucket;
         UPDATE library_metadata SET value = 4 WHERE key = 'performance_schema_version';"
    ))?;
    tx.commit()?;
    Ok(())
}

/// Counts assets whose stored ordering keys differ from a full recomputation.
pub fn count_inconsistent_sort_keys(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row(
        concat!(
            "WITH buckets AS (
               SELECT ",
            bucket!("a"),
            " AS bucket, MAX(a.modified_at) AS bucket_time,
                      MAX(a.id) AS bucket_id
               FROM assets a WHERE ",
            grouped!("a"),
            "
               GROUP BY 1
             )
             SELECT COUNT(*) FROM assets
             LEFT JOIN buckets ON ",
            grouped!("assets"),
            " AND ",
            bucket!("assets"),
            " = buckets.bucket
             WHERE typeof(assets.sort_bucket_time) <> 'integer'
                OR typeof(assets.sort_bucket_id) <> 'integer'
                OR typeof(assets.sort_group_null_order) <> 'integer'
                OR assets.sort_bucket_time IS NOT
                     CASE WHEN ",
            grouped!("assets"),
            " THEN buckets.bucket_time
                          ELSE assets.modified_at END
                OR assets.sort_bucket_id IS NOT
                     CASE WHEN ",
            grouped!("assets"),
            " THEN buckets.bucket_id
                          ELSE assets.id END
                OR assets.sort_group_null_order IS NOT
                     CASE WHEN ",
            grouped!("assets"),
            " THEN (assets.media_group_order IS NULL)
                          ELSE 0 END"
        ),
        [],
        |row| row.get(0),
    )?)
}

/// Defers the per-row group-bucket triggers for the rest of one transaction.
///
/// Every row that joins or leaves a group would otherwise rescan its whole
/// bucket, which makes grouping N assets O(N²). Bulk writers begin a deferral,
/// record the old and new key of every row they change, and call `finish`
/// before committing: it removes the marker row and recomputes each touched
/// bucket once. Singleton keys stay maintained by their triggers throughout.
/// The marker lives in `library_metadata` inside the writer's transaction, so
/// an error that rolls the transaction back also removes it.
pub struct DeferredSortKeys {
    buckets: HashSet<String>,
}

const SORT_KEYS_DEFERRED: &str = "sort_keys_deferred";

impl DeferredSortKeys {
    pub fn begin(conn: &Connection) -> anyhow::Result<Self> {
        conn.execute(
            "INSERT INTO library_metadata(key, value) VALUES (?1, 1)
             ON CONFLICT(key) DO UPDATE SET value = 1",
            params![SORT_KEYS_DEFERRED],
        )?;
        Ok(Self {
            buckets: HashSet::new(),
        })
    }

    /// Records a group key a changed row had before or has after the write.
    pub fn touch(&mut self, media_group_key: Option<&str>) {
        if let Some(key) = media_group_key {
            self.buckets.insert(key.to_string());
        }
    }

    pub fn finish(self, conn: &Connection) -> anyhow::Result<()> {
        conn.execute(
            "DELETE FROM library_metadata WHERE key = ?1",
            params![SORT_KEYS_DEFERRED],
        )?;
        let mut stmt = conn.prepare(recompute_bucket_sql!("?1"))?;
        for key in self.buckets {
            stmt.execute(params![key])?;
        }
        Ok(())
    }
}
