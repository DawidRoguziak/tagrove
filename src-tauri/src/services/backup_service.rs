use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

use tauri::State;

use crate::{
    app::state::AppState,
    db,
    error::AppResult,
    models::{
        DbBundleExportSummary, DbBundleImportSummary, DbBundleInspection, DbRootMapping,
    },
    services::{asset_query_service, db_pool},
};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BundleManifest {
    format_version: u32,
    source_platform: String,
    roots: Vec<String>,
    source_thumbs_dir: String,
}

pub fn export_db_bundle(path: String, state: &State<AppState>) -> AppResult<DbBundleExportSummary> {
    let target_file = PathBuf::from(path.trim());
    if target_file.as_os_str().is_empty() {
        return Err("DB export archive path is empty".into());
    }

    if let Some(parent) = target_file.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let file = fs::File::create(&target_file)?;
    let mut zip = zip::ZipWriter::new(file);
    let conn = db::open_connection(&state.db_path)?;
    let manifest = BundleManifest {
        format_version: 1,
        source_platform: std::env::consts::OS.to_string(),
        roots: db::list_scan_roots(&conn)?,
        source_thumbs_dir: state.thumbs_dir.to_string_lossy().to_string(),
    };
    zip.start_file(
        "manifest.json",
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644),
    )?;
    serde_json::to_writer(&mut zip, &manifest).map_err(|error| error.to_string())?;
    add_file_to_zip(&mut zip, &state.db_path, "media.db")?;
    let mut copied_files = 1usize;

    let wal_path = sqlite_sidecar_path(&state.db_path, "-wal");
    if wal_path.exists() {
        add_file_to_zip(&mut zip, &wal_path, "media.db-wal")?;
        copied_files += 1;
    }

    let shm_path = sqlite_sidecar_path(&state.db_path, "-shm");
    if shm_path.exists() {
        add_file_to_zip(&mut zip, &shm_path, "media.db-shm")?;
        copied_files += 1;
    }

    let mut copied_thumbnails = 0usize;
    if state.thumbs_dir.exists() {
        for entry in walkdir::WalkDir::new(&state.thumbs_dir) {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().is_file() {
                continue;
            }

            let source_path = entry.path();
            let relative = source_path
                .strip_prefix(&state.thumbs_dir)
                .map_err(|e| e.to_string())?;
            let relative_name = relative.to_string_lossy().replace('\\', "/");
            let zip_name = format!("thumbs/{relative_name}");
            add_file_to_zip(&mut zip, source_path, &zip_name)?;
            copied_thumbnails += 1;
        }
    }

    zip.finish()?;

    Ok(DbBundleExportSummary {
        copied_files,
        copied_thumbnails,
    })
}

pub fn inspect_db_bundle(path: String, state: &State<AppState>) -> AppResult<DbBundleInspection> {
    let source_file = validate_bundle_source(&path)?;
    let file = fs::File::open(&source_file)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if let Ok(mut entry) = archive.by_name("manifest.json") {
        let manifest: BundleManifest =
            serde_json::from_reader(&mut entry).map_err(|error| error.to_string())?;
        let requires_mapping = cfg!(not(windows)) && manifest.source_platform == "windows";
        return Ok(DbBundleInspection {
            format_version: Some(manifest.format_version),
            source_platform: Some(manifest.source_platform),
            source_thumbs_dir: Some(manifest.source_thumbs_dir),
            roots: manifest.roots,
            requires_mapping,
        });
    }

    let inspect_root = state.db_path.parent().ok_or("Cannot resolve app data directory")?.join(format!(
        "restore-inspect-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&inspect_root);
    fs::create_dir_all(&inspect_root)?;
    let inspect_db = inspect_root.join("media.db");
    let result = (|| -> AppResult<DbBundleInspection> {
        let mut db_entry = archive.by_name("media.db")?;
        write_archive_entry_to_file(&mut db_entry, &inspect_db)?;
        let conn = db::open_connection(&inspect_db)?;
        db::init_schema(&conn)?;
        let roots = db::list_scan_roots(&conn)?;
        let requires_mapping = cfg!(not(windows)) && roots.iter().any(|root| is_windows_absolute(root));
        Ok(DbBundleInspection {
            format_version: None,
            source_platform: None,
            source_thumbs_dir: None,
            roots,
            requires_mapping,
        })
    })();
    let _ = fs::remove_dir_all(inspect_root);
    result
}

pub fn import_db_bundle(
    path: String,
    root_mappings: Vec<DbRootMapping>,
    state: &State<AppState>,
) -> AppResult<DbBundleImportSummary> {
    let source_file = validate_bundle_source(&path)?;

    let file = fs::File::open(&source_file)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut has_db = false;
    for idx in 0..archive.len() {
        let entry = archive.by_index(idx)?;
        let safe_path = sanitize_zip_entry_path(entry.name())?;
        if safe_path == PathBuf::from("media.db") {
            has_db = true;
            break;
        }
    }
    if !has_db {
        return Err("Backup archive does not contain media.db".into());
    }

    let app_data_dir = state
        .db_path
        .parent()
        .ok_or_else(|| "Cannot resolve app data directory".to_string())?;
    let staging_root = app_data_dir.join("restore-staging");
    let staging_db = staging_root.join("media.db");
    let staging_thumbs = staging_root.join("thumbs");
    if staging_root.exists() {
        fs::remove_dir_all(&staging_root)?;
    }
    fs::create_dir_all(&staging_thumbs)?;

    let mut restored_files = 0usize;
    let mut restored_thumbnails = 0usize;

    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx)?;
        if entry.is_dir() {
            continue;
        }

        let safe_path = sanitize_zip_entry_path(entry.name())?;
        if safe_path == PathBuf::from("media.db") {
            write_archive_entry_to_file(&mut entry, &staging_db)?;
            restored_files += 1;
            continue;
        }

        if safe_path == PathBuf::from("media.db-wal") {
            let target_wal = sqlite_sidecar_path(&staging_db, "-wal");
            write_archive_entry_to_file(&mut entry, &target_wal)?;
            restored_files += 1;
            continue;
        }

        if safe_path == PathBuf::from("media.db-shm") {
            let target_shm = sqlite_sidecar_path(&staging_db, "-shm");
            write_archive_entry_to_file(&mut entry, &target_shm)?;
            restored_files += 1;
            continue;
        }

        if let Ok(relative_thumb) = safe_path.strip_prefix("thumbs") {
            if relative_thumb.as_os_str().is_empty() {
                continue;
            }
            let destination = staging_thumbs.join(relative_thumb);
            write_archive_entry_to_file(&mut entry, &destination)?;
            restored_thumbnails += 1;
        }
    }

    {
        let conn = db::open_connection(&staging_db)?;
        db::init_schema(&conn)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    rewrite_staged_paths(&staging_db, &staging_thumbs, &state.thumbs_dir, &root_mappings)?;

    let previous_db = app_data_dir.join("media.db.restore-previous");
    let previous_thumbs = app_data_dir.join("thumbs.restore-previous");
    let _ = fs::remove_file(&previous_db);
    if previous_thumbs.exists() {
        fs::remove_dir_all(&previous_thumbs)?;
    }
    let current_wal = sqlite_sidecar_path(&state.db_path, "-wal");
    let current_shm = sqlite_sidecar_path(&state.db_path, "-shm");
    db_pool::invalidate(&state.db_path);
    asset_query_service::manager().clear();
    if state.db_path.exists() {
        let current = db::open_connection(&state.db_path)?;
        current.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    let _ = fs::remove_file(&current_wal);
    let _ = fs::remove_file(&current_shm);

    if state.db_path.exists() {
        fs::rename(&state.db_path, &previous_db)?;
    }
    if state.thumbs_dir.exists() {
        fs::rename(&state.thumbs_dir, &previous_thumbs)?;
    }

    let install_result = (|| -> AppResult<()> {
        fs::rename(&staging_db, &state.db_path)?;
        fs::rename(&staging_thumbs, &state.thumbs_dir)?;
        Ok(())
    })();
    if let Err(error) = install_result {
        let _ = fs::remove_file(&state.db_path);
        let _ = fs::remove_dir_all(&state.thumbs_dir);
        if previous_db.exists() {
            let _ = fs::rename(&previous_db, &state.db_path);
        }
        if previous_thumbs.exists() {
            let _ = fs::rename(&previous_thumbs, &state.thumbs_dir);
        }
        return Err(error);
    }

    let _ = fs::remove_file(previous_db);
    let _ = fs::remove_dir_all(previous_thumbs);
    let _ = fs::remove_dir_all(staging_root);
    let conn = db::open_connection(&state.db_path)?;
    db::init_schema(&conn)?;
    db::bump_library_revision(&conn)?;
    db_pool::invalidate(&state.db_path);

    Ok(DbBundleImportSummary {
        restored_files,
        restored_thumbnails,
    })
}

fn validate_bundle_source(path: &str) -> AppResult<PathBuf> {
    let source_file = PathBuf::from(path.trim());
    if !source_file.exists() || !source_file.is_file() {
        return Err("DB import archive path does not exist or is not a file".into());
    }
    Ok(source_file)
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
        || path.starts_with("\\\\")
}

fn mapped_path(path: &str, mappings: &[DbRootMapping]) -> Option<String> {
    mappings.iter().find_map(|mapping| {
        if path.eq_ignore_ascii_case(&mapping.source_root) {
            return Some(mapping.target_root.clone());
        }
        let source = mapping.source_root.trim_end_matches(['\\', '/']);
        let suffix = path.get(source.len()..)?;
        if path[..source.len()].eq_ignore_ascii_case(source)
            && suffix.starts_with(['\\', '/'])
        {
            let suffix = suffix.trim_start_matches(['\\', '/']).replace('\\', "/");
            return Some(Path::new(&mapping.target_root).join(suffix).to_string_lossy().to_string());
        }
        None
    })
}

fn rewrite_staged_paths(
    staging_db: &Path,
    staging_thumbs: &Path,
    live_thumbs: &Path,
    root_mappings: &[DbRootMapping],
) -> AppResult<()> {
    let conn = db::open_connection(staging_db)?;
    let roots = db::list_scan_roots(&conn)?;
    if root_mappings.is_empty() {
        if cfg!(not(windows)) && roots.iter().any(|root| is_windows_absolute(root)) {
            return Err("Windows backup requires a target mapping for every scan root".into());
        }
        return Ok(());
    }
    let mapping_by_root = root_mappings
        .iter()
        .map(|mapping| (mapping.source_root.to_lowercase(), mapping))
        .collect::<HashMap<_, _>>();
    for root in &roots {
        if is_windows_absolute(root) && !mapping_by_root.contains_key(&root.to_lowercase()) {
            return Err(format!("Missing target mapping for scan root: {root}").into());
        }
    }
    for mapping in root_mappings {
        let target = Path::new(&mapping.target_root);
        if !target.is_absolute() || !target.is_dir() {
            return Err(format!("Mapped scan root is not an existing absolute directory: {}", mapping.target_root).into());
        }
    }

    let mut mappings = root_mappings.to_vec();
    mappings.sort_by_key(|mapping| std::cmp::Reverse(mapping.source_root.len()));
    let mut assets = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, path, thumb_path FROM assets ORDER BY id")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?)))?;
        for row in rows { assets.push(row?); }
    }
    let mut thumb_candidates: HashMap<String, Vec<PathBuf>> = HashMap::new();
    if staging_thumbs.exists() {
        for entry in walkdir::WalkDir::new(staging_thumbs) {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.file_type().is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    thumb_candidates.entry(name.to_string()).or_default().push(entry.path().to_path_buf());
                }
            }
        }
    }
    let mut targets = HashSet::new();
    let mut rewritten = Vec::new();
    for (id, old_path, old_thumb) in assets {
        let next_path = mapped_path(&old_path, &mappings).unwrap_or(old_path);
        if !targets.insert(next_path.clone()) {
            return Err(format!("Path mapping collision: {next_path}").into());
        }
        let file_name = next_path
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_string();
        let next_thumb = old_thumb.and_then(|value| {
            let name = value.rsplit(['\\', '/']).next()?;
            let matches = thumb_candidates.get(name)?;
            if matches.len() != 1 { return None; }
            let relative = matches[0].strip_prefix(staging_thumbs).ok()?;
            Some(live_thumbs.join(relative).to_string_lossy().to_string())
        });
        rewritten.push((id, next_path, file_name, next_thumb));
    }

    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM asset_scan_roots", [])?;
    tx.execute("DELETE FROM scan_roots", [])?;
    for mapping in &mappings {
        tx.execute("INSERT INTO scan_roots(path) VALUES (?1)", rusqlite::params![mapping.target_root])?;
    }
    for (id, path, file_name, thumb_path) in rewritten {
        tx.execute(
            "UPDATE assets SET path=?1, file_name=?2, file_name_key=lower(?2), thumb_path=?3 WHERE id=?4",
            rusqlite::params![path, file_name, thumb_path, id],
        )?;
    }
    for mapping in &mappings {
        let escaped = mapping.target_root.trim_end_matches(['/', '\\'])
            .replace('^', "^^").replace('%', "^%").replace('_', "^_");
        let pattern = format!("{escaped}/%");
        tx.execute(
            "INSERT INTO asset_scan_roots(asset_id, root_path, last_seen_generation) SELECT id, ?1, 0 FROM assets WHERE path=?1 OR path LIKE ?2 ESCAPE '^'",
            rusqlite::params![mapping.target_root, pattern],
        )?;
    }
    tx.execute("DELETE FROM thumbnail_failures", [])?;
    tx.commit()?;
    Ok(())
}

fn sqlite_sidecar_path(base_db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", base_db_path.to_string_lossy(), suffix))
}

fn add_file_to_zip(
    zip: &mut zip::ZipWriter<fs::File>,
    source_path: &Path,
    entry_name: &str,
) -> AppResult<()> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    zip.start_file(entry_name, options)?;
    let mut source = fs::File::open(source_path)?;
    std::io::copy(&mut source, zip)?;
    Ok(())
}

fn sanitize_zip_entry_path(entry_name: &str) -> AppResult<PathBuf> {
    let normalized = entry_name.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute() {
        return Err("Backup archive contains absolute path entry".into());
    }

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => out.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("Backup archive contains unsafe path entry".into())
            }
        }
    }

    Ok(out)
}

fn write_archive_entry_to_file(
    entry: &mut zip::read::ZipFile<'_>,
    destination: &Path,
) -> AppResult<()> {
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut file = fs::File::create(destination)?;
    std::io::copy(entry, &mut file)
        .map(|_| ())
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        sync::{atomic::AtomicBool, Mutex, RwLock},
    };

    use tempfile::tempdir;

    use crate::{
        app::state::AppState, db, models::NewAsset, services::thumb_scheduler::ThumbnailScheduler,
    };

    use super::{export_db_bundle, import_db_bundle, sanitize_zip_entry_path};

    fn create_test_state(db_path: &Path, thumbs_dir: &Path) -> AppState {
        AppState {
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
        }
    }

    fn as_state<'a>(state: &'a AppState) -> tauri::State<'a, AppState> {
        unsafe { std::mem::transmute::<&'a AppState, tauri::State<'a, AppState>>(state) }
    }

    fn new_asset(path: &Path, modified_at: i64) -> NewAsset {
        NewAsset {
            path: path.to_string_lossy().to_string(),
            kind: "image".to_string(),
            size_bytes: 10,
            modified_at,
            width: Some(100),
            height: Some(100),
            duration_ms: None,
            thumb_path: None,
        }
    }

    #[test]
    fn sanitize_rejects_parent_segments() {
        assert!(sanitize_zip_entry_path("../media.db").is_err());
        assert!(sanitize_zip_entry_path("thumbs/../../x").is_err());
    }

    #[test]
    fn sanitize_accepts_relative_file() {
        let safe = sanitize_zip_entry_path("thumbs/a/b.jpg").expect("must pass");
        assert_eq!(safe.to_string_lossy().replace('\\', "/"), "thumbs/a/b.jpg");
    }

    #[test]
    fn export_db_bundle_includes_database_sidecars_and_thumbnails() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        drop(conn);

        let wal_path = PathBuf::from(format!("{}-wal", db_path.to_string_lossy()));
        let shm_path = PathBuf::from(format!("{}-shm", db_path.to_string_lossy()));
        fs::write(&wal_path, b"wal").expect("write wal");
        fs::write(&shm_path, b"shm").expect("write shm");

        let nested_thumb_dir = thumbs_dir.join("nested");
        fs::create_dir_all(&nested_thumb_dir).expect("create nested thumbs");
        let thumb_file = nested_thumb_dir.join("thumb.jpg");
        fs::write(&thumb_file, b"thumb").expect("write thumb file");

        let state = create_test_state(&db_path, &thumbs_dir);
        let state_ref = as_state(&state);
        let archive_path = tmp.path().join("backup.zip");

        let summary = export_db_bundle(archive_path.to_string_lossy().to_string(), &state_ref)
            .expect("export bundle");

        assert_eq!(summary.copied_files, 3);
        assert_eq!(summary.copied_thumbnails, 1);

        let file = fs::File::open(&archive_path).expect("open archive");
        let mut archive = zip::ZipArchive::new(file).expect("read archive");
        let mut names = (0..archive.len())
            .map(|idx| {
                archive
                    .by_index(idx)
                    .expect("archive entry")
                    .name()
                    .to_string()
            })
            .collect::<Vec<_>>();
        names.sort();

        assert!(names.iter().any(|name| name == "media.db"));
        assert!(names.iter().any(|name| name == "media.db-wal"));
        assert!(names.iter().any(|name| name == "media.db-shm"));
        assert!(names.iter().any(|name| name == "thumbs/nested/thumb.jpg"));
    }

    #[test]
    fn import_db_bundle_rejects_archive_without_media_db() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        db::add_scan_root(&conn, "C:/existing").expect("seed root");

        let archive_path = tmp.path().join("broken.zip");
        {
            let file = fs::File::create(&archive_path).expect("create archive");
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("thumbs/file.jpg", zip::write::SimpleFileOptions::default())
                .expect("start file");
            zip.write_all(b"thumb").expect("write thumb");
            zip.finish().expect("finish archive");
        }

        let state = create_test_state(&db_path, &thumbs_dir);
        let state_ref = as_state(&state);
        let error = import_db_bundle(archive_path.to_string_lossy().to_string(), Vec::new(), &state_ref)
            .expect_err("must fail without media.db");

        assert!(
            error.to_string().contains("does not contain media.db"),
            "unexpected error: {error}"
        );

        let conn = db::open_connection(&db_path).expect("reopen db");
        let roots = db::list_scan_roots(&conn).expect("list roots");
        assert_eq!(roots, vec!["C:/existing".to_string()]);
    }

    #[test]
    fn import_db_bundle_restores_database_and_thumbnails() {
        let tmp = tempdir().expect("tempdir");
        let source_db_path = tmp.path().join("source.db");
        let source_thumb_bytes = b"thumb-bytes";

        {
            let conn = db::open_connection(&source_db_path).expect("open source db");
            db::init_schema(&conn).expect("init source schema");
            db::add_scan_root(&conn, "/restored-root").expect("add restored root");
            let asset_path = tmp.path().join("asset.jpg");
            fs::write(&asset_path, b"asset").expect("write source asset");
            db::upsert_asset(&conn, &new_asset(&asset_path, 10)).expect("upsert source asset");
        }

        let archive_path = tmp.path().join("restore.zip");
        {
            let file = fs::File::create(&archive_path).expect("create archive");
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("media.db", zip::write::SimpleFileOptions::default())
                .expect("start media db");
            let db_bytes = fs::read(&source_db_path).expect("read source db");
            zip.write_all(&db_bytes).expect("write media db");

            zip.start_file(
                "thumbs/restored/thumb.jpg",
                zip::write::SimpleFileOptions::default(),
            )
            .expect("start thumb");
            zip.write_all(source_thumb_bytes)
                .expect("write thumb bytes");
            zip.finish().expect("finish archive");
        }

        let target_db_path = tmp.path().join("target.db");
        let target_thumbs_dir = tmp.path().join("target-thumbs");
        fs::create_dir_all(&target_thumbs_dir).expect("create target thumbs dir");
        let conn = db::open_connection(&target_db_path).expect("open target db");
        db::init_schema(&conn).expect("init target schema");
        db::add_scan_root(&conn, "C:/old-root").expect("seed old root");
        drop(conn);

        let state = create_test_state(&target_db_path, &target_thumbs_dir);
        let state_ref = as_state(&state);
        let summary = import_db_bundle(archive_path.to_string_lossy().to_string(), Vec::new(), &state_ref)
            .expect("import bundle");

        assert_eq!(summary.restored_files, 1);
        assert_eq!(summary.restored_thumbnails, 1);

        let restored_thumb = target_thumbs_dir.join("restored").join("thumb.jpg");
        assert_eq!(
            fs::read(&restored_thumb).expect("read restored thumb"),
            source_thumb_bytes
        );

        let conn = db::open_connection(&target_db_path).expect("open restored db");
        let roots = db::list_scan_roots(&conn).expect("list restored roots");
        assert_eq!(roots, vec!["/restored-root".to_string()]);
        let page = db::list_assets(&conn, 0, 50, &[], &[], None, false).expect("list assets");
        assert_eq!(page.total, 1);
    }

    #[cfg(not(windows))]
    #[test]
    fn imports_windows_bundle_with_root_mapping_and_preserves_asset_metadata() {
        let tmp = tempdir().expect("tempdir");
        let source_db = tmp.path().join("windows.db");
        let source_root = r"C:\Users\Example\Pictures";
        {
            let conn = db::open_connection(&source_db).expect("open source db");
            db::init_schema(&conn).expect("init source schema");
            db::add_scan_root(&conn, source_root).expect("add source root");
            let asset = NewAsset {
                path: format!(r"{source_root}\album\photo.jpg"),
                kind: "image".to_string(),
                size_bytes: 10,
                modified_at: 12,
                width: Some(20),
                height: Some(30),
                duration_ms: None,
                thumb_path: Some(r"C:\old-profile\thumbs\legacy.jpg".to_string()),
            };
            db::upsert_asset(&conn, &asset).expect("insert asset");
            db::set_asset_tags(&conn, 1, &["travel".to_string()]).expect("set tags");
            db::set_asset_favorite(&conn, 1, true).expect("set favorite");
            db::set_asset_media_group(&conn, 1, Some("album"), Some(2.0))
                .expect("set group");
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .expect("checkpoint source");
        }

        let archive_path = tmp.path().join("windows-backup.zip");
        {
            let file = fs::File::create(&archive_path).expect("create archive");
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("media.db", zip::write::SimpleFileOptions::default())
                .expect("start db");
            zip.write_all(&fs::read(&source_db).expect("read source db"))
                .expect("write db");
            zip.start_file("thumbs/legacy.jpg", zip::write::SimpleFileOptions::default())
                .expect("start thumb");
            zip.write_all(b"legacy-thumbnail").expect("write thumb");
            zip.finish().expect("finish archive");
        }

        let target_root = tmp.path().join("linux-media");
        fs::create_dir_all(target_root.join("album")).expect("create mapped root");
        fs::write(target_root.join("album/photo.jpg"), b"media").expect("write mapped media");
        let target_db = tmp.path().join("target.db");
        let target_thumbs = tmp.path().join("target-thumbs");
        fs::create_dir_all(&target_thumbs).expect("create target thumbs");
        let conn = db::open_connection(&target_db).expect("open target");
        db::init_schema(&conn).expect("init target");
        drop(conn);
        let state = create_test_state(&target_db, &target_thumbs);
        let mapping = crate::models::DbRootMapping {
            source_root: source_root.to_string(),
            target_root: target_root.to_string_lossy().to_string(),
        };

        import_db_bundle(
            archive_path.to_string_lossy().to_string(),
            vec![mapping],
            &as_state(&state),
        )
        .expect("import mapped bundle");

        let conn = db::open_connection(&target_db).expect("open restored db");
        let page = db::list_assets(&conn, 0, 10, &[], &[], None, false).expect("list assets");
        assert_eq!(page.total, 1);
        let asset = &page.items[0];
        assert_eq!(asset.id, 1);
        assert_eq!(asset.path, target_root.join("album/photo.jpg").to_string_lossy());
        assert_eq!(asset.tags, vec!["travel".to_string()]);
        assert!(asset.is_favorite);
        assert_eq!(asset.media_group_key.as_deref(), Some("album"));
        assert_eq!(asset.media_group_order, Some(2.0));
        let expected_thumb = target_thumbs.join("legacy.jpg").to_string_lossy().to_string();
        assert_eq!(asset.thumb_path.as_deref(), Some(expected_thumb.as_str()));
        assert_eq!(
            db::list_scan_roots(&conn).expect("roots"),
            vec![target_root.to_string_lossy().to_string()]
        );
        assert_eq!(fs::read(target_thumbs.join("legacy.jpg")).expect("restored thumb"), b"legacy-thumbnail");
    }
}
