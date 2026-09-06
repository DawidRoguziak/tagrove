use media_tagger::{
    app::state::AppState,
    db,
    models::NewAsset,
    services::{db_pool::DatabaseRuntime, scan_service, thumb_scheduler::ThumbnailScheduler},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Mutex, RwLock,
    },
};

fn state(path: PathBuf, thumbs: PathBuf) -> AppState {
    AppState {
        database: DatabaseRuntime::new(path.clone()),
        db_path: path,
        thumbs_dir: thumbs,
        ffmpeg_path: "ffmpeg".into(),
        scan_lock: Mutex::new(()),
        thumb_lock: RwLock::new(()),
        thumb_scheduler: ThumbnailScheduler::new(1, "ffmpeg".into()),
        thumbnail_render_all_running: AtomicBool::new(false),
        thumbnail_render_all_cancel_requested: AtomicBool::new(false),
        thumbnail_generation: AtomicU64::new(0),
    }
}
fn asset() -> NewAsset {
    NewAsset {
        path: "/media/image.png".into(),
        kind: "image".into(),
        size_bytes: 12,
        modified_at: 7,
        width: Some(2),
        height: Some(2),
        duration_ms: None,
        thumb_path: None,
    }
}

#[test]
fn precise_changes_invalidate_mutations_and_old_failure_workers() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    db::init_schema(&conn).unwrap();
    db::add_scan_root(&conn, "/media").unwrap();
    let image = asset();
    db::upsert_scanned_asset(&conn, &image, 7_000_000_000, "/media", 1).unwrap();
    let first = db::get_file_mutation_asset(&conn, 1).unwrap().unwrap();
    conn.execute("UPDATE assets SET thumb_path = 'old.jpg'", [])
        .unwrap();
    db::upsert_scanned_asset(&conn, &image, 7_000_000_001, "/media", 2).unwrap();
    let second = db::get_file_mutation_asset(&conn, 1).unwrap().unwrap();
    assert_eq!(second.record_version, first.record_version + 1);
    assert!(second.thumb_path.is_none());
    db::upsert_scanned_asset(&conn, &image, 7_000_000_001, "/media", 3).unwrap();
    assert_eq!(
        db::get_file_mutation_asset(&conn, 1)
            .unwrap()
            .unwrap()
            .record_version,
        second.record_version
    );
    db::record_thumbnail_failure_if_version_matches(
        &conn,
        1,
        second.record_version,
        Some("new failure"),
    )
    .unwrap();
    db::record_thumbnail_failure_if_version_matches(
        &conn,
        1,
        first.record_version,
        Some("old failure"),
    )
    .unwrap();
    db::clear_thumbnail_failure_if_version_matches(&conn, 1, first.record_version).unwrap();
    assert_eq!(
        conn.query_row("SELECT last_error FROM thumbnail_failures", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap(),
        "new failure"
    );
}

#[test]
fn version_two_failure_migration_preserves_metadata() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    db::init_schema(&conn).unwrap();
    db::upsert_asset(&conn, &asset()).unwrap();
    db::set_asset_tags(&conn, 1, &["retained".into()]).unwrap();
    db::record_thumbnail_failure(&conn, 1, 7, Some("legacy")).unwrap();
    conn.execute_batch("ALTER TABLE thumbnail_failures DROP COLUMN source_record_version; PRAGMA user_version = 2;").unwrap();
    assert_eq!(
        db::validate_backup_database(&conn, false).unwrap(),
        db::BackupSchemaCompatibility::Version2
    );
    db::init_schema(&conn).unwrap();
    assert!(db::list_failed_thumbnail_asset_ids(&conn)
        .unwrap()
        .is_empty());
    assert_eq!(db::list_asset_tags(&conn, 1).unwrap(), ["retained"]);
    db::validate_backup_database(&conn, false).unwrap();
}

#[test]
fn source_journal_connections_are_full_and_pool_connections_remain_normal() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = DatabaseRuntime::new(dir.path().join("media.db"));
    let operation = runtime.admit().unwrap();
    let synchronous = |conn: &rusqlite::Connection| {
        conn.query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
            .unwrap()
    };
    assert_eq!(synchronous(&operation.durable_connection().unwrap()), 2);
    assert_eq!(synchronous(&operation.connection().unwrap()), 1);
}

#[test]
fn real_scan_crosses_batches_preserves_overlapping_membership_and_reports_partial_roots() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("media");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    let seed = nested.join("seed.png");
    image::RgbImage::new(2, 2).save(&seed).unwrap();
    for id in 0..513 {
        std::fs::copy(&seed, root.join(format!("{id}.png"))).unwrap();
    }
    let state = state(dir.path().join("media.db"), dir.path().join("thumbs"));
    let permit = state.database.admit().unwrap();
    {
        let conn = permit.connection().unwrap();
        db::init_schema(&conn).unwrap();
        db::add_scan_root(&conn, root.to_str().unwrap()).unwrap();
        db::add_scan_root(&conn, nested.to_str().unwrap()).unwrap();
    }
    let report = scan_service::scan_roots(
        &[
            root.to_string_lossy().into(),
            nested.to_string_lossy().into(),
        ],
        &state,
        &permit,
        &|_| Ok(()),
    )
    .unwrap();
    assert_eq!(report.failed, 0);
    {
        let conn = permit.connection().unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            514
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM asset_scan_roots", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            515
        );
    }
    std::fs::remove_file(root.join("0.png")).unwrap();
    let report = scan_service::scan_roots(
        &[
            root.to_string_lossy().into(),
            dir.path().join("missing").to_string_lossy().into(),
        ],
        &state,
        &permit,
        &|_| Ok(()),
    )
    .unwrap();
    assert!(matches!(
        report.completion,
        media_tagger::models::ScanCompletion::Partial
    ));
    let conn = permit.connection().unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        513
    );
    assert_eq!(
        db::remove_scan_root_and_orphan_assets(&conn, root.to_str().unwrap())
            .unwrap()
            .0,
        512
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
#[ignore = "isolated memory and timing measurements; run with --ignored --nocapture --test-threads=1"]
fn metadata_scale_measurements() {
    for count in [1_000, 10_000] {
        let dir = tempfile::tempdir().unwrap();
        let conn = db::open_connection(&dir.path().join("media.db")).unwrap();
        db::init_schema(&conn).unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        for id in 0..count {
            let mut image = asset();
            image.path = format!("/media/{id}.png");
            image.modified_at = id;
            db::upsert_asset(&tx, &image).unwrap();
        }
        tx.commit().unwrap();
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id, path FROM assets WHERE file_name_key = '123.png'",
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(plan.contains("idx_assets_file_name_key"), "{plan}");
        let steps = |sql: &str| {
            let mut stmt = conn.prepare(sql).unwrap();
            {
                let mut rows = stmt.query([]).unwrap();
                while rows.next().unwrap().is_some() {}
            }
            stmt.get_status(rusqlite::StatementStatus::VmStep)
        };
        let full_path_steps = steps("SELECT id, path FROM assets");
        let indexed_path_steps =
            steps("SELECT id, path FROM assets WHERE file_name_key = '123.png'");
        assert!(indexed_path_steps < full_path_steps / 10);
        let mut current = 0;
        let mut highwater = 0;
        // SAFETY: pointers are valid; this ignored test runs in isolation to measure SQLite allocations.
        unsafe {
            rusqlite::ffi::sqlite3_status64(
                rusqlite::ffi::SQLITE_STATUS_MEMORY_USED,
                &mut current,
                &mut highwater,
                1,
            );
        }
        let before = current;
        let started = std::time::Instant::now();
        let ids = db::list_ordered_asset_ids_with_meta(&conn, &[], &[], None, false, None).unwrap();
        let query_us = started.elapsed().as_micros();
        unsafe {
            rusqlite::ffi::sqlite3_status64(
                rusqlite::ffi::SQLITE_STATUS_MEMORY_USED,
                &mut current,
                &mut highwater,
                0,
            );
        }
        assert_eq!(ids.len(), count as usize);
        let mut after = 0;
        let mut pages = 0;
        let mut seen = 0;
        loop {
            let page = db::thumbnail_candidates_page(&conn, after, i64::MAX, 7, false).unwrap();
            if page.is_empty() {
                break;
            }
            assert!(page.len() <= 7);
            after = page.last().unwrap().0.id;
            seen += page.len();
            pages += 1;
        }
        assert_eq!(seen, count as usize);
        println!("rows={count} query_us={query_us} retained_vector_bytes={} sqlite_peak_delta_bytes={} candidate_pages={pages} max_page=7 full_path_steps={full_path_steps} indexed_path_steps={indexed_path_steps} filename_plan={plan}", ids.capacity() * 8, highwater - before);
    }
}
