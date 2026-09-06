use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::Context;

use crate::{
    db::{self, CsvImportRecord},
    error::AppResult,
    models::{CsvExportSummary, CsvImportSummary},
    utils::{
        tags::{normalize_and_validate_tags, parse_csv_tags},
        text::canonical_key,
    },
};

const CSV_HEADERS: [&str; 5] = [
    "file_name",
    "tags",
    "favorite",
    "media_group_key",
    "media_group_order",
];
const MAX_CSV_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_CSV_RECORDS: usize = 2_000_000;
const MAX_CSV_FIELD_BYTES: usize = 1024 * 1024;

pub fn export_tags_csv(
    path: &str,
    db_path: &Path,
    permit: &crate::services::db_pool::OperationPermit,
) -> AppResult<CsvExportSummary> {
    export_tags_csv_with_limits(path, db_path, permit, MAX_CSV_FILE_BYTES, MAX_CSV_RECORDS)
}

fn export_tags_csv_with_limits(
    path: &str,
    db_path: &Path,
    permit: &crate::services::db_pool::OperationPermit,
    max_bytes: u64,
    max_records: usize,
) -> AppResult<CsvExportSummary> {
    let conn = permit.connection()?;
    let target = validate_export_target(path, db_path, &conn)?;
    let target_name = target
        .file_name()
        .ok_or("CSV export path has no file name")?
        .to_string_lossy();
    let temporary = target
        .parent()
        .ok_or("CSV export path has no parent")?
        .join(format!(
            ".{target_name}.{}-{}.tmp",
            std::process::id(),
            rand::random::<u64>()
        ));

    let result = (|| -> AppResult<CsvExportSummary> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut writer = csv::WriterBuilder::new()
            .has_headers(true)
            .from_writer(file);
        writer.write_record(CSV_HEADERS)?;
        let mut rows = 0usize;
        db::for_each_asset_for_csv_export(&conn, |row| {
            let tags = normalize_and_validate_tags(row.tags)?;
            let file_name = Path::new(&row.path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            writer.write_record([
                file_name,
                tags.join(" "),
                if row.is_favorite { "1" } else { "0" }.to_string(),
                row.media_group_key.unwrap_or_default(),
                row.media_group_order
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ])?;
            rows += 1;
            anyhow::ensure!(rows <= max_records, "CSV record limit exceeded");
            Ok(())
        })?;
        writer.flush()?;
        let file = writer.into_inner().map_err(|error| error.into_error())?;
        file.sync_all()?;
        if file.metadata()?.len() > max_bytes {
            return Err("CSV byte limit exceeded".into());
        }
        parse_csv_to_sink(
            LimitedReader::new(fs::File::open(&temporary)?, max_bytes),
            max_records,
            MAX_CSV_FIELD_BYTES,
            |_| Ok(()),
        )?;
        publish_file(&temporary, &target)?;
        sync_parent_directory(&target)?;
        Ok(CsvExportSummary { rows })
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn import_tags_csv(
    path: &str,
    permit: &crate::services::db_pool::OperationPermit,
) -> AppResult<CsvImportSummary> {
    let source = PathBuf::from(path.trim());
    let file = fs::File::open(&source)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err("CSV import path does not exist or is not a file".into());
    }
    validate_csv_file_size(metadata.len())?;
    // SQLite's empty filename creates an automatically deleted disk database.
    let mut staging = rusqlite::Connection::open("")?;
    staging.pragma_update(None, "cache_size", -2048)?;
    staging
        .execute_batch("CREATE TABLE records (id INTEGER PRIMARY KEY, payload TEXT NOT NULL)")?;
    let tx = staging.transaction()?;
    parse_csv_to_sink(
        LimitedReader::new(file, MAX_CSV_FILE_BYTES),
        MAX_CSV_RECORDS,
        MAX_CSV_FIELD_BYTES,
        |record| {
            tx.execute(
                "INSERT INTO records(payload) VALUES (?1)",
                [serde_json::to_string(&record).map_err(|e| e.to_string())?],
            )?;
            Ok(())
        },
    )?;
    tx.commit()?;
    let mut conn = permit.connection()?;
    db::import_csv_record_batches(&mut conn, |offset| {
        let mut stmt =
            staging.prepare("SELECT payload FROM records WHERE id > ?1 ORDER BY id LIMIT 512")?;
        let rows = stmt.query_map([offset as i64], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    })
    .map_err(Into::into)
}

struct LimitedReader<R> {
    reader: R,
    remaining: u64,
}
impl<R> LimitedReader<R> {
    fn new(reader: R, remaining: u64) -> Self {
        Self { reader, remaining }
    }
}
impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut probe = [0];
            return if self.reader.read(&mut probe)? == 0 {
                Ok(0)
            } else {
                Err(std::io::Error::other("CSV byte limit exceeded"))
            };
        }
        let length = buf
            .len()
            .min(self.remaining.min(usize::MAX as u64) as usize);
        let count = self.reader.read(&mut buf[..length])?;
        self.remaining -= count as u64;
        Ok(count)
    }
}

#[cfg(test)]
fn read_with_byte_limit(reader: impl Read, max_bytes: u64) -> AppResult<Vec<u8>> {
    let mut document = Vec::new();
    reader.take(max_bytes + 1).read_to_end(&mut document)?;
    if document.len() as u64 > max_bytes {
        return Err(format!("CSV exceeds the {max_bytes}-byte file size limit").into());
    }
    Ok(document)
}

#[cfg(test)]
fn parse_csv_document(reader: impl Read) -> AppResult<Vec<CsvImportRecord>> {
    parse_csv_document_with_limits(reader, MAX_CSV_RECORDS, MAX_CSV_FIELD_BYTES)
}

#[cfg(test)]
fn parse_csv_document_with_limits(
    reader: impl Read,
    max_records: usize,
    max_field_bytes: usize,
) -> AppResult<Vec<CsvImportRecord>> {
    let mut parsed = Vec::new();
    parse_csv_to_sink(reader, max_records, max_field_bytes, |record| {
        parsed.push(record);
        Ok(())
    })?;
    Ok(parsed)
}

fn parse_csv_to_sink(
    reader: impl Read,
    max_records: usize,
    max_field_bytes: usize,
    mut consume: impl FnMut(CsvImportRecord) -> AppResult<()>,
) -> AppResult<()> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader);
    let headers = reader.headers()?.clone();
    validate_csv_fields(&headers, 1, max_field_bytes)?;
    let mut indices = [None; CSV_HEADERS.len()];
    for (index, raw_header) in headers.iter().enumerate() {
        let header = raw_header.trim();
        if let Some(position) = CSV_HEADERS
            .iter()
            .position(|expected| header.eq_ignore_ascii_case(expected))
        {
            if indices[position].replace(index).is_some() {
                return Err(format!("CSV header '{}' is duplicated", CSV_HEADERS[position]).into());
            }
        }
    }
    for (position, header) in CSV_HEADERS.iter().enumerate() {
        if indices[position].is_none() {
            return Err(format!("CSV is missing required header '{header}'").into());
        }
    }
    let indices = indices.map(Option::unwrap);

    for (row_index, record) in reader.records().enumerate() {
        if row_index >= max_records {
            return Err(format!("CSV exceeds the limit of {max_records} records").into());
        }
        let record =
            record.with_context(|| format!("Invalid CSV record at row {}", row_index + 2))?;
        validate_csv_fields(&record, row_index + 2, max_field_bytes)?;
        let value = |header_index: usize| record.get(indices[header_index]).unwrap_or("").trim();
        let favorite = parse_favorite(value(2), row_index + 2)?;
        let media_group_order = parse_group_order(value(4), row_index + 2)?;
        let media_group_key = value(3);
        consume(CsvImportRecord {
            file_name_key: canonical_key(value(0)),
            tags: normalize_and_validate_tags(parse_csv_tags(value(1)))?,
            favorite,
            media_group_key: Some(if media_group_key.is_empty() {
                None
            } else {
                Some(media_group_key.to_string())
            }),
            media_group_order: Some(media_group_order),
        })?;
    }
    Ok(())
}

fn validate_csv_file_size(size: u64) -> AppResult<()> {
    if size > MAX_CSV_FILE_BYTES {
        return Err(format!("CSV exceeds the {MAX_CSV_FILE_BYTES}-byte file size limit").into());
    }
    Ok(())
}

fn validate_csv_fields(
    record: &csv::StringRecord,
    row: usize,
    max_field_bytes: usize,
) -> AppResult<()> {
    if record.iter().any(|field| field.len() > max_field_bytes) {
        return Err(
            format!("CSV field at row {row} exceeds the {max_field_bytes}-byte limit").into(),
        );
    }
    Ok(())
}

fn parse_favorite(raw: &str, row: usize) -> AppResult<Option<bool>> {
    if raw.is_empty() {
        return Ok(None);
    }
    match canonical_key(raw).as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(Some(true)),
        "0" | "false" | "no" | "n" | "off" => Ok(Some(false)),
        _ => Err(format!("Invalid favorite value at CSV row {row}").into()),
    }
}

fn parse_group_order(raw: &str, row: usize) -> AppResult<Option<f64>> {
    if raw.is_empty() {
        return Ok(None);
    }
    let value = raw
        .parse::<f64>()
        .with_context(|| format!("Invalid media_group_order at CSV row {row}"))?;
    if !value.is_finite() {
        return Err(format!("Invalid media_group_order at CSV row {row}").into());
    }
    Ok(Some(value))
}

fn validate_export_target(
    raw_path: &str,
    db_path: &Path,
    conn: &rusqlite::Connection,
) -> AppResult<PathBuf> {
    let requested = PathBuf::from(raw_path.trim());
    if requested.as_os_str().is_empty() {
        return Err("CSV export path is empty".into());
    }
    let parent = requested
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let target = parent.canonicalize()?.join(
        requested
            .file_name()
            .ok_or("CSV export path has no file name")?,
    );
    let profile = db_path
        .parent()
        .ok_or("Cannot resolve app data directory")?
        .canonicalize()?;
    if target.starts_with(profile) {
        return Err("CSV export cannot be stored inside the active profile".into());
    }
    for root in db::list_scan_roots(conn)? {
        if let Ok(root) = Path::new(&root).canonicalize() {
            if target.starts_with(root) {
                return Err("CSV export cannot be stored inside an indexed source root".into());
            }
        }
    }
    for asset in db::list_asset_paths(conn)? {
        if let Ok(source) = Path::new(&asset.path).canonicalize() {
            if target == source {
                return Err("CSV export cannot overwrite an indexed source file".into());
            }
        }
    }
    Ok(target)
}

fn publish_file(source: &Path, target: &Path) -> AppResult<()> {
    fs::rename(source, target)?;
    Ok(())
}

fn sync_parent_directory(path: &Path) -> AppResult<()> {
    fs::File::open(path.parent().ok_or("CSV export path has no parent")?)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Cursor};

    use tempfile::tempdir;

    use super::{
        export_tags_csv, parse_csv_document, parse_csv_document_with_limits, read_with_byte_limit,
        validate_csv_file_size, MAX_CSV_FILE_BYTES,
    };
    use crate::{
        db::{self, CsvImportRecord},
        models::NewAsset,
        utils::text::canonical_key,
    };

    const HEADERS: &str = "file_name,tags,favorite,media_group_key,media_group_order\n";

    fn asset(path: &str) -> NewAsset {
        NewAsset {
            path: path.to_string(),
            kind: "image".to_string(),
            size_bytes: 1,
            modified_at: 1,
            width: None,
            height: None,
            duration_ms: None,
            thumb_path: None,
        }
    }

    #[test]
    fn parser_requires_each_known_header_exactly_once_and_allows_reordering() {
        let missing = parse_csv_document(Cursor::new("file_name,tags\na.jpg,cat\n"));
        assert!(missing.unwrap_err().to_string().contains("favorite"));

        let duplicate = parse_csv_document(Cursor::new(
            "file_name,tags,favorite,FAVORITE,media_group_key,media_group_order\na.jpg,cat,1,0,,\n",
        ));
        assert!(duplicate.unwrap_err().to_string().contains("duplicated"));

        let reordered = parse_csv_document(Cursor::new(
            "tags,media_group_order,file_name,media_group_key,favorite\ncat,2,A.JPG,trip,yes\n",
        ))
        .unwrap();
        assert_eq!(reordered.len(), 1);
        assert_eq!(reordered[0].file_name_key, "a.jpg");
        assert_eq!(reordered[0].tags, vec!["cat"]);
        assert_eq!(reordered[0].favorite, Some(true));
        assert_eq!(reordered[0].media_group_key, Some(Some("trip".to_string())));
        assert_eq!(reordered[0].media_group_order, Some(Some(2.0)));
    }

    #[test]
    fn parser_rejects_a_malformed_late_row_before_any_database_work() {
        let csv = format!("{HEADERS}a.jpg,cat,1,trip,1\nb.jpg,\"unterminated,0,,\n");
        assert!(parse_csv_document(Cursor::new(csv)).is_err());
    }

    #[test]
    fn import_fans_out_by_unicode_canonical_basename_and_bumps_once() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &asset("/one/ŻÓŁW.JPG")).unwrap();
        db::upsert_asset(&conn, &asset("/two/żółw.jpg")).unwrap();
        let baseline = db::current_library_revision(&conn).unwrap();
        let records =
            parse_csv_document(Cursor::new(format!("{HEADERS}ŻÓŁW.JPG,animal,1,trip,2\n")))
                .unwrap();

        let summary = db::import_csv_records(&mut conn, &records).unwrap();

        assert_eq!(summary.rows_applied, 1);
        assert_eq!(summary.assets_matched, 2);
        assert_eq!(summary.assets_updated, 2);
        assert_eq!(db::current_library_revision(&conn).unwrap(), baseline + 1);
        assert_eq!(db::list_asset_tags(&conn, 1).unwrap(), vec!["animal"]);
        assert_eq!(db::list_asset_tags(&conn, 2).unwrap(), vec!["animal"]);
    }

    #[test]
    fn database_failure_rolls_back_all_metadata_and_revision() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &asset("/one/a.jpg")).unwrap();
        db::upsert_asset(&conn, &asset("/two/b.jpg")).unwrap();
        let baseline = db::current_library_revision(&conn).unwrap();
        conn.execute_batch(
            "CREATE TEMP TRIGGER fail_second_import
             BEFORE UPDATE OF tag_count ON assets
             WHEN OLD.id = 2
             BEGIN SELECT RAISE(ABORT, 'injected import failure'); END;",
        )
        .unwrap();
        let records = [
            CsvImportRecord {
                file_name_key: canonical_key("a.jpg"),
                tags: vec!["first".to_string()],
                favorite: Some(true),
                media_group_key: Some(Some("trip".to_string())),
                media_group_order: Some(Some(1.0)),
            },
            CsvImportRecord {
                file_name_key: canonical_key("b.jpg"),
                tags: vec!["second".to_string()],
                favorite: Some(true),
                media_group_key: Some(Some("trip".to_string())),
                media_group_order: Some(Some(2.0)),
            },
        ];

        assert!(db::import_csv_records(&mut conn, &records).is_err());
        assert_eq!(db::current_library_revision(&conn).unwrap(), baseline);
        assert!(db::list_asset_tags(&conn, 1).unwrap().is_empty());
        assert!(db::list_asset_tags(&conn, 2).unwrap().is_empty());
        assert!(!db::get_asset_favorite(&conn, 1).unwrap());
        assert_eq!(db::get_asset_media_group(&conn, 1).unwrap(), (None, None));
    }

    #[test]
    fn failed_export_preserves_an_existing_target() {
        let root = tempdir().unwrap();
        let profile = root.path().join("profile");
        let exports = root.path().join("exports");
        fs::create_dir_all(&profile).unwrap();
        fs::create_dir_all(&exports).unwrap();
        let db_path = profile.join("media.db");
        let conn = db::open_connection(&db_path).unwrap();
        db::init_schema(&conn).unwrap();
        db::upsert_asset(&conn, &asset("/source/a.jpg")).unwrap();
        conn.execute("INSERT INTO tags(name) VALUES ('new york')", [])
            .unwrap();
        conn.execute("INSERT INTO asset_tags(asset_id, tag_id) VALUES (1, 1)", [])
            .unwrap();
        let target = exports.join("tags.csv");
        fs::write(&target, "keep me").unwrap();

        assert!(export_tags_csv(
            target.to_str().unwrap(),
            &db_path,
            &crate::services::db_pool::DatabaseRuntime::new(db_path.clone())
                .admit()
                .unwrap()
        )
        .is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "keep me");
        let temporary_count = fs::read_dir(exports)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(temporary_count, 0);
    }

    #[test]
    fn parser_rejects_invalid_optional_scalars() {
        for row in ["a.jpg,cat,maybe,,", "a.jpg,cat,1,,NaN"] {
            let csv = format!("{HEADERS}{row}\n");
            assert!(parse_csv_document(Cursor::new(csv)).is_err());
        }
    }

    #[test]
    fn csv_file_size_limit_accepts_boundary_and_rejects_next_byte() {
        assert!(validate_csv_file_size(MAX_CSV_FILE_BYTES).is_ok());
        assert!(validate_csv_file_size(MAX_CSV_FILE_BYTES + 1).is_err());
    }

    #[test]
    fn bounded_csv_read_accepts_boundary_and_rejects_next_byte() {
        assert_eq!(
            read_with_byte_limit(Cursor::new(b"1234"), 4).unwrap(),
            b"1234"
        );
        assert!(read_with_byte_limit(Cursor::new(b"12345"), 4).is_err());
    }

    #[test]
    fn parser_enforces_record_limit_at_boundary() {
        let at_limit = format!("{HEADERS}a.jpg,cat,1,,\nb.jpg,dog,0,,\n");
        assert_eq!(
            parse_csv_document_with_limits(Cursor::new(at_limit), 2, 64)
                .expect("records at limit")
                .len(),
            2
        );
        let over_limit = format!("{HEADERS}a.jpg,cat,1,,\nb.jpg,dog,0,,\nc.jpg,bird,1,,\n");
        assert!(parse_csv_document_with_limits(Cursor::new(over_limit), 2, 64).is_err());
    }

    #[test]
    fn parser_enforces_field_limit_at_boundary() {
        let at_limit = format!("{HEADERS}a.jpg,cat,1,{},\n", "x".repeat(32));
        assert!(parse_csv_document_with_limits(Cursor::new(at_limit), 1, 32).is_ok());
        let over_limit = format!("{HEADERS}a.jpg,cat,1,{},\n", "x".repeat(33));
        assert!(parse_csv_document_with_limits(Cursor::new(over_limit), 1, 32).is_err());
    }
    #[test]
    fn staged_import_keeps_record_order_and_explicit_group_clears_across_batches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("media.db");
        let input = dir.path().join("input.csv");
        let runtime = crate::services::db_pool::DatabaseRuntime::new(path);
        let permit = runtime.admit().unwrap();
        {
            let conn = permit.connection().unwrap();
            db::init_schema(&conn).unwrap();
            db::upsert_asset(&conn, &asset("/one/a.jpg")).unwrap();
        }
        let mut document =
            String::from("file_name,tags,favorite,media_group_key,media_group_order\n");
        for _ in 0..513 {
            document.push_str("a.jpg,retained,true,group,2\n");
        }
        document.push_str("a.jpg,,false,,\n");
        std::fs::write(&input, document).unwrap();
        let before = db::current_library_revision(&permit.connection().unwrap()).unwrap();
        let summary = super::import_tags_csv(input.to_str().unwrap(), &permit).unwrap();
        assert_eq!(summary.rows_read, 514);
        let conn = permit.connection().unwrap();
        assert_eq!(db::get_asset_media_group(&conn, 1).unwrap(), (None, None));
        assert!(!db::get_asset_favorite(&conn, 1).unwrap());
        assert_eq!(db::list_asset_tags(&conn, 1).unwrap(), ["retained"]);
        assert_eq!(db::current_library_revision(&conn).unwrap(), before + 1);
        std::fs::write(&input, "file_name,tags,favorite,media_group_key,media_group_order\na.jpg,new,true,group,2\na.jpg,,invalid,,\n").unwrap();
        assert!(super::import_tags_csv(input.to_str().unwrap(), &permit).is_err());
        assert_eq!(db::list_asset_tags(&conn, 1).unwrap(), ["retained"]);
    }

    #[test]
    fn export_limit_failure_preserves_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("profile");
        std::fs::create_dir(&data).unwrap();
        let path = data.join("media.db");
        let output = dir.path().join("export.csv");
        let runtime = crate::services::db_pool::DatabaseRuntime::new(path.clone());
        let permit = runtime.admit().unwrap();
        {
            let conn = permit.connection().unwrap();
            db::init_schema(&conn).unwrap();
            db::upsert_asset(&conn, &asset("/media/a.jpg")).unwrap();
        }
        std::fs::write(&output, b"previous export").unwrap();
        for (bytes, records) in [(1, 10), (1024, 0)] {
            assert!(super::export_tags_csv_with_limits(
                output.to_str().unwrap(),
                &path,
                &permit,
                bytes,
                records
            )
            .is_err());
            assert_eq!(std::fs::read(&output).unwrap(), b"previous export");
        }
    }
}
