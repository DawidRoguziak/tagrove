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

        CREATE INDEX IF NOT EXISTS idx_assets_kind ON assets(kind);
        CREATE INDEX IF NOT EXISTS idx_assets_modified_at ON assets(modified_at DESC);
        CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
        CREATE INDEX IF NOT EXISTS idx_asset_tags_tag ON asset_tags(tag_id);
        CREATE INDEX IF NOT EXISTS idx_asset_tags_asset ON asset_tags(asset_id);
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
    if performance_schema_version < 3 {
        migrate_canonical_keys_and_tags(conn)?;
    }
    optimize(conn)?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    if conn.query_row("SELECT COUNT(*) FROM pragma_table_info('thumbnail_failures') WHERE name = 'source_record_version'", [], |row| row.get::<_, i64>(0))? == 0 {
        conn.execute_batch("ALTER TABLE thumbnail_failures ADD COLUMN source_record_version INTEGER NOT NULL DEFAULT 0; DELETE FROM thumbnail_failures;")?;
    }
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

    let metadata_rows = conn.query_row(
        "SELECT COUNT(*) FROM library_metadata
         WHERE (key = 'revision' AND value >= 1)
             OR (key = 'performance_schema_version' AND value BETWEEN 0 AND 3)",
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
    ensure_no_pending_file_operations(conn)?;
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

pub(super) fn migrate_canonical_keys_and_tags(conn: &Connection) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let mut changed = false;
    let filename_updates = {
        let mut stmt = tx.prepare("SELECT id, file_name, file_name_key FROM assets")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (asset_id, file_name, existing_key) in filename_updates {
        let key = canonical_key(&file_name);
        if key == existing_key {
            continue;
        }
        tx.execute(
            "UPDATE assets SET file_name_key = ?1 WHERE id = ?2",
            params![key, asset_id],
        )?;
        changed = true;
    }

    let asset_tags = {
        let mut stmt = tx.prepare("SELECT id FROM assets ORDER BY id")?;
        let asset_ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
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
        changed |= set_asset_tags_in_tx(&tx, asset_id, &tags)?.0;
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
