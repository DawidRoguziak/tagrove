//! Grid-performance baseline diagnostics.
//!
//! Step 0 of `docs/plans/grid-performance/07-verification-and-docs.md`: numbers
//! recorded before the grid/mutation stages change any code, and repeated
//! after them. Every test is ignored and uses only synthetic rows and files in
//! temporary directories. There are no thresholds; compare runs made with the
//! same build profile on the same idle host. Run alone, in release mode, with
//! `TMPDIR` on a disk-backed filesystem (a tmpfs `/tmp` hides write costs):
//!
//! ```sh
//! TMPDIR=/var/tmp cargo test --manifest-path src-tauri/Cargo.toml --locked --release \
//!   --test grid_baseline -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Output lines start with `[baseline]` and contain only counts and timings.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
    thread,
    time::{Duration, Instant},
};

use media_tagger::{
    app::{locks::with_scan_lock, state::AppState},
    db,
    error::AppResult,
    models::{AssetQueryPageResult, ScanProgress, StartAssetQueryResult},
    services::{
        asset_query_service::AssetQueryFilters, db_pool::DatabaseRuntime, scan_service,
        thumb_scheduler::ThumbnailScheduler,
    },
    thumbs,
};

const PAGE_SIZE: usize = 128;
const SCAN_FILES: usize = 20_000;
const SCAN_LIBRARY_ROWS: i64 = 100_000;
const SCAN_ROUNDS: usize = 10;
/// A user operation slower than this counts as delayed.
const DELAYED_MS: f64 = 1_000.0;

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn summary(samples: &[f64]) -> String {
    if samples.is_empty() {
        return "n=0".into();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let pick = |q: f64| sorted[((sorted.len() - 1) as f64 * q).round() as usize];
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    format!(
        "n={} min_ms={:.2} p50_ms={:.2} p95_ms={:.2} max_ms={:.2} mean_ms={:.2}",
        sorted.len(),
        sorted[0],
        pick(0.5),
        pick(0.95),
        sorted[sorted.len() - 1],
        mean
    )
}

/// Synthetic library shaped like a real collection: 90% images, 2% GIFs, 5%
/// videos (with a few overlaps resolved toward video), 5% favorites, 10% of
/// assets in four-item media groups, 0-3 vocabulary tags per asset from 2,000
/// names, a `common` tag on 20% and a `rare` tag on 0.5%. Modification times
/// are a fixed pseudo-random permutation over ten years, so the gallery order
/// differs from insertion order; 0.1% have a zero (unknown) time. Derived columns and root mappings match what
/// the app writes; the synthetic root is never scanned.
fn seed_library(path: &Path, rows: i64) -> Duration {
    let started = Instant::now();
    let conn = db::open_connection(path).unwrap();
    db::init_schema(&conn).unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    tx.execute(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1),
         shaped AS (
           SELECT i,
             CASE WHEN i % 20 = 0 THEN 'video' WHEN i % 50 = 1 THEN 'gif' ELSE 'image' END AS kind,
             'asset-' || printf('%07d', i) ||
               CASE WHEN i % 20 = 0 THEN '.mp4' WHEN i % 50 = 1 THEN '.gif' ELSE '.jpg' END AS file_name,
             CASE WHEN i % 1000 = 7 THEN 0
               ELSE 1420070400 + ((i * 2654435761) % 315360000) END AS modified_at,
             (i / 4) % 10 = 0 AS grouped
           FROM n
         )
         INSERT INTO assets(
           path, file_name, file_name_key, kind, size_bytes, modified_at, width, height,
           duration_ms, fingerprint_mtime_ns, is_favorite, media_group_key,
           media_group_key_normalized, media_group_order
         )
         SELECT
           '/synthetic/library/' || printf('%03d', i % 1000) || '/' || file_name,
           file_name, lower(file_name), kind,
           50000 + (i * 7919) % 5000000,
           modified_at,
           CASE WHEN kind = 'video' THEN 1920 ELSE 4000 END,
           CASE WHEN kind = 'video' THEN 1080 ELSE 3000 END,
           CASE WHEN kind = 'video' THEN 15000 + (i % 600) * 1000 ELSE NULL END,
           modified_at * 1000000000,
           CASE WHEN (i * 31) % 20 = 3 THEN 1 ELSE 0 END,
           CASE WHEN grouped THEN 'Group ' || (i / 4) ELSE NULL END,
           CASE WHEN grouped THEN 'group ' || (i / 4) ELSE NULL END,
           CASE WHEN grouped THEN (i % 4) + 1 ELSE NULL END
         FROM shaped",
        [rows],
    )
    .unwrap();
    tx.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 1999)
         INSERT INTO tags(id, name) SELECT i + 1, 'tag-' || printf('%04d', i) FROM n;
         INSERT INTO tags(id, name) VALUES (2001, 'common'), (2002, 'rare');
         INSERT INTO asset_tags(asset_id, tag_id)
           SELECT a.id, ((a.id * 13 + j.j * 101) % 2000) + 1
           FROM assets a
           JOIN (SELECT 0 AS j UNION ALL SELECT 1 UNION ALL SELECT 2) j ON j.j < a.id % 4;
         INSERT INTO asset_tags(asset_id, tag_id) SELECT id, 2001 FROM assets WHERE id % 5 = 2;
         INSERT INTO asset_tags(asset_id, tag_id) SELECT id, 2002 FROM assets WHERE id % 200 = 3;
         UPDATE assets SET tag_count = (SELECT COUNT(*) FROM asset_tags WHERE asset_id = assets.id);
         -- Every indexed asset belongs to a scan root; unmapped rows are
         -- pruned as orphans by the next completed scan of any root.
         INSERT INTO scan_roots(path) VALUES ('/synthetic/library');
         INSERT INTO asset_scan_roots(asset_id, root_path, last_seen_generation)
           SELECT id, '/synthetic/library', 1 FROM assets;",
    )
    .unwrap();
    tx.commit().unwrap();
    db::bump_library_revision(&conn).unwrap();
    // Sort-key columns are maintained by triggers (ADR 0004), so raw-SQL
    // seeding produces the same keys as the app's write paths.
    assert_eq!(db::count_inconsistent_sort_keys(&conn).unwrap(), 0);
    db::optimize(&conn).unwrap();
    let stored: i64 = conn
        .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
        .unwrap();
    assert_eq!(stored, rows);
    started.elapsed()
}

fn filters() -> AssetQueryFilters {
    AssetQueryFilters {
        tags_and: vec![],
        tags_not: vec![],
        kind: None,
        favorites_only: false,
        meta_filter: None,
    }
}

fn named_filters() -> Vec<(&'static str, AssetQueryFilters)> {
    vec![
        ("none", filters()),
        (
            "favorites",
            AssetQueryFilters {
                favorites_only: true,
                ..filters()
            },
        ),
        (
            "tag_common",
            AssetQueryFilters {
                tags_and: vec!["common".into()],
                ..filters()
            },
        ),
        (
            "tag_rare",
            AssetQueryFilters {
                tags_and: vec!["rare".into()],
                ..filters()
            },
        ),
        (
            "exclude_common",
            AssetQueryFilters {
                tags_not: vec!["common".into()],
                ..filters()
            },
        ),
        (
            "kind_video",
            AssetQueryFilters {
                kind: Some("video".into()),
                ..filters()
            },
        ),
    ]
}

static GENERATION: AtomicU64 = AtomicU64::new(1);

/// The same service call `start_asset_query` makes, minus IPC.
fn start_query(runtime: &Arc<DatabaseRuntime>, filters: AssetQueryFilters) -> (f64, u64, usize) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst);
    let request = runtime.queries.begin_request("grid-baseline", generation);
    let started = Instant::now();
    let permit = runtime.admit().unwrap();
    let result = runtime
        .queries
        .start(&permit, filters, PAGE_SIZE, request, generation)
        .unwrap();
    let elapsed = ms(started);
    match result {
        StartAssetQueryResult::Ready {
            session_id, total, ..
        } => (elapsed, session_id, total),
        StartAssetQueryResult::Superseded => panic!("baseline query was superseded"),
    }
}

fn page(runtime: &Arc<DatabaseRuntime>, session_id: u64, offset: usize) -> (f64, bool) {
    let started = Instant::now();
    let permit = runtime.admit().unwrap();
    let result = runtime
        .queries
        .page(&permit, session_id, offset, PAGE_SIZE)
        .unwrap();
    (ms(started), matches!(result, AssetQueryPageResult::Stale))
}

#[test]
#[ignore = "baseline diagnostic: run explicitly, alone, in release mode"]
fn query_rebuild_and_page_baseline() {
    for rows in [100_000_i64, 1_000_000] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("media.db");
        let seeded = seed_library(&path, rows);
        println!(
            "[baseline] fixture rows={rows} seed_ms={}",
            seeded.as_millis()
        );
        let runtime = DatabaseRuntime::new(path.clone());
        let writer = db::open_connection(&path).unwrap();
        let iterations = if rows >= 1_000_000 { 6 } else { 12 };

        for (name, filter) in named_filters() {
            let mut samples = Vec::new();
            let mut matches = 0;
            for _ in 0..iterations {
                // Every scan batch and query-affecting write bumps the
                // revision; the next start rebuilds the ID vector.
                db::bump_library_revision(&writer).unwrap();
                let (elapsed, _, total) = start_query(&runtime, filter.clone());
                samples.push(elapsed);
                matches = total;
            }
            println!(
                "[baseline] metric=start_asset_query_rebuild rows={rows} filter={name} matches={matches} {}",
                summary(&samples)
            );
        }

        // The later filters bumped the revision; rebuild once, then hit the cache.
        let (_, all_session, _) = start_query(&runtime, filters());
        let cached: Vec<f64> = (0..5).map(|_| start_query(&runtime, filters()).0).collect();
        println!(
            "[baseline] metric=start_asset_query_cached rows={rows} filter=none {}",
            summary(&cached)
        );

        let mut page_samples = Vec::new();
        let total = rows as usize;
        for step in 0..60usize {
            let offset = match step {
                0 => 0,
                1 => total - PAGE_SIZE,
                2 => total / 2,
                _ => (step * 7_919 * PAGE_SIZE) % (total - PAGE_SIZE),
            };
            let (elapsed, stale) = page(&runtime, all_session, offset);
            assert!(!stale, "page session must stay current without writes");
            page_samples.push(elapsed);
        }
        println!(
            "[baseline] metric=get_asset_query_page rows={rows} limit={PAGE_SIZE} {}",
            summary(&page_samples)
        );
    }
}

/// Stage 03 (ADR 0004): one-time sort-key backfill on upgrade, the plan of the
/// unfiltered order query, and bulk grouping cost with trigger maintenance.
#[test]
#[ignore = "baseline diagnostic: run explicitly, alone, in release mode"]
fn sort_key_backfill_plan_and_bulk_group_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("media.db");
    seed_library(&path, 1_000_000);
    {
        let conn = db::open_connection(&path).unwrap();
        // Reconstruct the version 3 state: no triggers, no sort index, no keys.
        for name in db::SORT_KEY_TRIGGERS {
            conn.execute_batch(&format!("DROP TRIGGER {name};"))
                .unwrap();
        }
        conn.execute_batch(
            "DROP INDEX idx_assets_sort_key;
             DROP INDEX idx_assets_group_bucket;
             UPDATE assets SET sort_bucket_time = NULL, sort_bucket_id = NULL,
                               sort_group_null_order = NULL;
             UPDATE library_metadata SET value = 3 WHERE key = 'performance_schema_version';
             PRAGMA wal_checkpoint(TRUNCATE);",
        )
        .unwrap();
    }
    let conn = db::open_connection(&path).unwrap();
    let started = Instant::now();
    db::init_schema(&conn).unwrap();
    println!(
        "[baseline] metric=sort_key_upgrade_init_schema rows=1000000 ms={:.0}",
        ms(started)
    );
    assert_eq!(db::count_inconsistent_sort_keys(&conn).unwrap(), 0);

    let plan = conn
        .prepare(
            "EXPLAIN QUERY PLAN SELECT id FROM assets ORDER BY sort_bucket_time DESC,
               sort_bucket_id DESC, sort_group_null_order ASC, media_group_order ASC,
               modified_at DESC, id DESC",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    println!(
        "[baseline] metric=order_plan filter=none plan={:?}",
        plan.join(" | ")
    );
    drop(conn);

    let mut conn = db::open_connection(&path).unwrap();
    for (name, descending) in [("newest_first", true), ("oldest_first", false)] {
        // 5,000 ungrouped assets into one new group, processed in time order:
        // oldest first is the worst case (every member raises the bucket).
        let order = if descending { "DESC" } else { "ASC" };
        let ids = conn
            .prepare(&format!(
                "SELECT id FROM assets WHERE media_group_key IS NULL
                 ORDER BY modified_at {order}, id {order} LIMIT 5000"
            ))
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let updates = ids
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, Some(index as f64 + 1.0)))
            .collect::<Vec<_>>();
        let started = Instant::now();
        db::set_assets_media_group_bulk(&mut conn, &updates, Some(&format!("bulk-{name}")))
            .unwrap();
        println!(
            "[baseline] metric=bulk_group_5000 order={name} rows=1000000 ms={:.0}",
            ms(started)
        );
    }
    assert_eq!(db::count_inconsistent_sort_keys(&conn).unwrap(), 0);
}

fn app_state(db_path: PathBuf, thumbs: PathBuf) -> AppState {
    AppState {
        database: DatabaseRuntime::new(db_path.clone()),
        db_path,
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

#[derive(Default)]
struct UserOps {
    favorite: Vec<f64>,
    tags: Vec<f64>,
    pool_wait: Vec<f64>,
    failed: usize,
    errors: Vec<String>,
}

impl UserOps {
    fn delayed(&self) -> usize {
        self.favorite
            .iter()
            .chain(&self.tags)
            .filter(|value| **value > DELAYED_MS)
            .count()
    }
}

/// The same work as `set_asset_favorite` and `set_asset_tags` after IPC:
/// admit, take a user-write connection, run the revision-aware write.
fn user_operation(state: &AppState, ops: &mut UserOps, step: usize, rows: i64) {
    let asset_id = 1 + (step as i64 * 2_477) % rows;
    let started = Instant::now();
    let favorite = step.is_multiple_of(2);
    let result = (|| -> AppResult<f64> {
        let mut conn = state.database.admit()?.user_write_connection()?;
        let waited = ms(started);
        if favorite {
            db::set_asset_favorite_with_revision(&mut conn, asset_id, step.is_multiple_of(4))?;
        } else {
            let tags = vec!["common".to_string(), format!("baseline-{}", step % 7)];
            db::set_asset_tags_with_revision(&mut conn, asset_id, &tags)?;
        }
        Ok(waited)
    })();
    let elapsed = ms(started);
    match result {
        Ok(waited) => {
            ops.pool_wait.push(waited);
            if favorite {
                ops.favorite.push(elapsed);
            } else {
                ops.tags.push(elapsed);
            }
        }
        Err(error) => {
            ops.failed += 1;
            // Error text from SQLite/pool only; no paths are involved.
            ops.errors.push(error.to_string());
        }
    }
}

fn write_scan_media(root: &Path, count: usize) {
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 5) as u8, 90])
    })
    .write_to(&mut encoded, image::ImageFormat::Jpeg)
    .unwrap();
    let bytes = encoded.into_inner();
    for index in 0..count {
        let folder = root.join(format!("batch-{:02}", index / 1_000));
        if index % 1_000 == 0 {
            fs::create_dir_all(&folder).unwrap();
        }
        fs::write(folder.join(format!("scan-{index:05}.jpg")), &bytes).unwrap();
    }
}

#[test]
#[ignore = "baseline diagnostic: run explicitly, alone, in release mode"]
fn mutation_latency_and_rebuilds_during_scan_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("media.db");
    seed_library(&db_path, SCAN_LIBRARY_ROWS);
    let state = app_state(db_path.clone(), dir.path().join("thumbs"));

    let mut idle = UserOps::default();
    for step in 0..40 {
        user_operation(&state, &mut idle, step, SCAN_LIBRARY_ROWS);
    }
    println!(
        "[baseline] metric=favorite_write_idle rows={SCAN_LIBRARY_ROWS} {}",
        summary(&idle.favorite)
    );
    println!(
        "[baseline] metric=tag_write_idle rows={SCAN_LIBRARY_ROWS} {}",
        summary(&idle.tags)
    );
    println!(
        "[baseline] metric=user_ops_idle failed={} delayed={}",
        idle.failed,
        idle.delayed()
    );

    let revision_reader = db::open_connection_read_only(&db_path).unwrap();
    let revision = || db::current_library_revision(&revision_reader).unwrap();
    let mut busy = UserOps::default();
    let mut rebuilds = Vec::new();
    let mut page_calls = 0usize;
    let mut user_step = 0usize;

    // Each round indexes a new folder of SCAN_FILES files, so every file is
    // new work. Rounds repeat the short scan to collect enough user samples.
    for round in 0..SCAN_ROUNDS {
        let media = dir.path().join(format!("media-{round}"));
        write_scan_media(&media, SCAN_FILES);
        let media_root = media.to_str().unwrap().to_owned();
        let revision_before = revision();
        let (_, mut session, _) = start_query(&state.database, filters());
        let scan_done = AtomicBool::new(false);
        let progress_events = AtomicUsize::new(0);
        let round_rebuilds_before = rebuilds.len();
        let round_ops_before = busy.favorite.len() + busy.tags.len() + busy.failed;

        let (scan_ms, indexed) = thread::scope(|scope| {
            let state = &state;
            let scan_done = &scan_done;
            let progress_events = &progress_events;
            let scan = scope.spawn(move || {
                let started = Instant::now();
                // Mirrors `scan_folder_service`: the registration connection is
                // returned before the scan takes its own.
                let scanned = with_scan_lock(state, |permit| {
                    {
                        let conn = permit.connection()?;
                        db::add_scan_root(&conn, &media_root)?;
                    }
                    let sink = |_progress: ScanProgress| -> AppResult<()> {
                        progress_events.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    };
                    scan_service::scan_roots(
                        std::slice::from_ref(&media_root),
                        state,
                        permit,
                        &sink,
                    )
                })
                .unwrap();
                scan_done.store(true, Ordering::SeqCst);
                (ms(started), scanned.indexed)
            });

            // A user scrolling the gallery: one page read every 100 ms. A
            // stale page makes the frontend refresh, which rebuilds the session.
            let rebuilds = &mut rebuilds;
            let page_calls = &mut page_calls;
            let browsing = scope.spawn(move || {
                let mut step = 0usize;
                while !scan_done.load(Ordering::SeqCst) {
                    let offset =
                        (step * 7_919 * PAGE_SIZE) % (SCAN_LIBRARY_ROWS as usize - PAGE_SIZE);
                    let (_, stale) = page(&state.database, session, offset);
                    *page_calls += 1;
                    if stale {
                        let (elapsed, next, _) = start_query(&state.database, filters());
                        rebuilds.push(elapsed);
                        session = next;
                    }
                    step += 1;
                    thread::sleep(Duration::from_millis(100));
                }
            });

            // A like or tag click every 50 ms while the scan runs.
            let busy = &mut busy;
            let user_step = &mut user_step;
            let user = scope.spawn(move || {
                while !scan_done.load(Ordering::SeqCst) {
                    user_operation(state, busy, *user_step, SCAN_LIBRARY_ROWS);
                    *user_step += 1;
                    thread::sleep(Duration::from_millis(50));
                }
            });

            let result = scan.join().unwrap();
            browsing.join().unwrap();
            user.join().unwrap();
            result
        });
        assert_eq!(indexed, SCAN_FILES);
        println!(
            "[baseline] metric=scan round={round} files={SCAN_FILES} scan_ms={scan_ms:.0} revision_bumps={} progress_events={} rebuilds={} user_ops={}",
            revision() - revision_before,
            progress_events.load(Ordering::Relaxed),
            rebuilds.len() - round_rebuilds_before,
            busy.favorite.len() + busy.tags.len() + busy.failed - round_ops_before
        );
    }

    println!(
        "[baseline] metric=rebuild_during_scan rounds={SCAN_ROUNDS} page_reads={page_calls} rebuilds={} {}",
        rebuilds.len(),
        summary(&rebuilds)
    );
    println!(
        "[baseline] metric=favorite_write_during_scan {}",
        summary(&busy.favorite)
    );
    println!(
        "[baseline] metric=tag_write_during_scan {}",
        summary(&busy.tags)
    );
    println!(
        "[baseline] metric=pool_wait_during_scan {}",
        summary(&busy.pool_wait)
    );
    let mut errors = busy.errors.clone();
    errors.sort();
    errors.dedup();
    println!(
        "[baseline] metric=user_ops_during_scan attempted={} failed={} delayed_over_{DELAYED_MS}ms={} distinct_errors={errors:?}",
        busy.favorite.len() + busy.tags.len() + busy.failed,
        busy.failed,
        busy.delayed()
    );
}

#[test]
#[ignore = "baseline diagnostic: run explicitly, alone, in release mode"]
fn jpeg_24mp_thumbnail_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("synthetic-24mp.jpg");
    // Smooth gradients plus deterministic high-frequency detail, so the JPEG
    // carries a realistic amount of entropy for a 6000x4000 photo.
    let mut seed = 0x2545_f491_u32;
    image::RgbImage::from_fn(6_000, 4_000, |x, y| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let noise = (seed & 0x1f) as u8;
        image::Rgb([
            ((x / 24) as u8).wrapping_add(noise),
            ((y / 16) as u8).wrapping_add(noise / 2),
            (((x + y) / 40) as u8) ^ noise,
        ])
    })
    .save_with_format(&source, image::ImageFormat::Jpeg)
    .unwrap();
    let source_bytes = fs::metadata(&source).unwrap().len();

    let mut samples = Vec::new();
    for attempt in 0..4 {
        let target = dir.path().join(format!("thumb-{attempt}.jpg"));
        let started = Instant::now();
        thumbs::create_image_thumb(&source, &target).unwrap();
        samples.push(ms(started));
        assert!(target.exists());
    }
    println!(
        "[baseline] metric=thumbnail_jpeg_24mp source_bytes={source_bytes} {}",
        summary(&samples)
    );
}
