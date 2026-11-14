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
        Arc, Condvar, Mutex, RwLock,
    },
    thread,
    time::{Duration, Instant},
};

use media_tagger_core::{
    app::{locks::with_scan_lock, state::AppState},
    db,
    error::AppResult,
    models::{AssetQueryPageResult, ScanProgress, SelectionMutationResult, StartAssetQueryResult},
    services::{
        asset_query_service::{AssetQueryFilters, SelectionSpec},
        db_pool::DatabaseRuntime,
        scan_service,
        selection_service::{apply_selection_mutation, SelectionMutation},
        thumb_scheduler::{ThumbnailPriority, ThumbnailScheduler, ThumbnailTask},
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

        // Stage 08 (ADR 0007): unfiltered sections are read on the first
        // sections request, off the rebuild path. Each sample rebuilds a fresh
        // session, then times its first (scan) and second (cached) read.
        let mut first_reads = Vec::new();
        let mut cached_reads = Vec::new();
        let mut section_count = 0;
        for _ in 0..iterations {
            db::bump_library_revision(&writer).unwrap();
            let (_, session, _) = start_query(&runtime, filters());
            for samples in [&mut first_reads, &mut cached_reads] {
                let started = Instant::now();
                let permit = runtime.admit().unwrap();
                let result = runtime.queries.sections(&permit, session).unwrap();
                samples.push(ms(started));
                let media_tagger_core::models::AssetQuerySectionsResult::Ready {
                    sections,
                    total,
                    ..
                } = result
                else {
                    panic!("sections of a current session are ready");
                };
                assert_eq!(
                    sections.iter().map(|section| section.count).sum::<usize>(),
                    total
                );
                section_count = sections.len();
            }
        }
        println!(
            "[baseline] metric=get_asset_query_sections_first rows={rows} sections={section_count} {}",
            summary(&first_reads)
        );
        println!(
            "[baseline] metric=get_asset_query_sections_cached rows={rows} {}",
            summary(&cached_reads)
        );
        // `all_session` was replaced by the rebuilds above; read ranges from a current one.
        let (_, all_session, _) = start_query(&runtime, filters());

        // Stage 04 (ADR 0005): the binary ID range behind Shift-click and
        // rectangle selection, for 100k IDs and the whole snapshot.
        let spans = if total > 100_000 {
            vec![100_000, total]
        } else {
            vec![total]
        };
        for span in spans {
            let samples: Vec<f64> = (0..10)
                .map(|_| {
                    let started = Instant::now();
                    let permit = runtime.admit().unwrap();
                    let bytes = runtime
                        .queries
                        .id_range(&permit, all_session, 0, span - 1)
                        .unwrap()
                        .expect("range session must stay current without writes");
                    assert_eq!(bytes.len(), (span + 1) * 8);
                    ms(started)
                })
                .collect();
            println!(
                "[baseline] metric=get_asset_query_id_range rows={rows} span={span} bytes={} {}",
                (span + 1) * 8,
                summary(&samples)
            );
        }
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

/// Runs `work` while another connection polls the write lock every 200 µs
/// (busy timeout zero, `BEGIN IMMEDIATE` then `ROLLBACK`, so it never writes)
/// and returns the longest stretch that connection was locked out: what a
/// background writer waits before its transaction can start.
fn longest_write_lock_out<T>(path: &Path, work: impl FnOnce() -> T) -> (T, f64) {
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        let probe = scope.spawn(|| {
            let conn = db::open_connection(path).unwrap();
            conn.busy_timeout(Duration::ZERO).unwrap();
            let mut longest = 0.0_f64;
            let mut locked_since: Option<Instant> = None;
            while !stop.load(Ordering::SeqCst) {
                if conn.execute_batch("BEGIN IMMEDIATE; ROLLBACK;").is_ok() {
                    if let Some(since) = locked_since.take() {
                        longest = longest.max(ms(since));
                    }
                } else {
                    locked_since.get_or_insert_with(Instant::now);
                }
                thread::sleep(Duration::from_micros(200));
            }
            if let Some(since) = locked_since {
                longest = longest.max(ms(since));
            }
            longest
        });
        let result = work();
        stop.store(true, Ordering::SeqCst);
        (result, probe.join().unwrap())
    })
}

/// Package K27 (record S-034d0ea6): a ranged `setMediaGroup` through
/// `apply_selection_mutation`, the path a range selection in bulk mode takes
/// (Ctrl+A, Apply group / Remove groups). The whole write is one IMMEDIATE
/// transaction; `lock_out_ms` is how long a concurrent writer could not start
/// one (background writers give up after their 5 s busy budget), `ms` the call
/// including the commit's automatic checkpoint. The fixture has 10% of its
/// rows in four-item groups. Steps: the first tenth of the gallery into a new
/// group (existing groups are split), then every asset but three into one
/// group, the same again (no row changes), and every group cleared.
#[test]
#[ignore = "baseline diagnostic: run explicitly, alone, in release mode"]
fn ranged_set_media_group_baseline() {
    for rows in [100_000_i64, 1_000_000] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("media.db");
        seed_library(&path, rows);
        let runtime = DatabaseRuntime::new(path.clone());
        let tenth = rows as usize / 10;
        for (name, key, tenth_only) in [
            ("assign_tenth", Some("Tenth group"), true),
            ("assign_all", Some("Bulk group"), false),
            ("reassign_all_same", Some("Bulk group"), false),
            ("clear_all", None, false),
        ] {
            // Each write changes the gallery order, so every step needs a
            // fresh session: an old spec would be refused as stale.
            let (_, session_id, total) = start_query(&runtime, filters());
            let spec = SelectionSpec {
                session_id,
                ranges: vec![[0, if tenth_only { tenth - 1 } else { total - 1 }]],
                excluded: if tenth_only { vec![] } else { vec![5, 6, 7] },
                included: vec![],
            };
            let started = Instant::now();
            let (result, lock_out_ms) = longest_write_lock_out(&path, || {
                apply_selection_mutation(
                    &runtime,
                    &spec,
                    SelectionMutation::SetMediaGroup {
                        media_group_key: key.map(str::to_string),
                    },
                )
                .unwrap()
            });
            let elapsed = ms(started);
            let SelectionMutationResult::Ready {
                processed_assets,
                updated_assets,
                ..
            } = result
            else {
                panic!("baseline group write was stale");
            };
            let selected = if tenth_only { tenth } else { rows as usize - 3 };
            assert_eq!(processed_assets, selected);
            println!(
                "[baseline] metric=ranged_set_media_group step={name} rows={rows} processed={processed_assets} updated={updated_assets} lock_out_ms={lock_out_ms:.0} ms={elapsed:.0}"
            );
        }
        let conn = db::open_connection(&path).unwrap();
        assert_eq!(db::count_inconsistent_sort_keys(&conn).unwrap(), 0);
    }
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

/// Writes a synthetic 6000x4000 JPEG: smooth gradients plus deterministic
/// high-frequency detail, so it carries a realistic amount of entropy.
fn write_24mp_jpeg(source: &Path) {
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
    .save_with_format(source, image::ImageFormat::Jpeg)
    .unwrap();
}

#[test]
#[ignore = "baseline diagnostic: run explicitly, alone, in release mode"]
fn jpeg_24mp_thumbnail_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("synthetic-24mp.jpg");
    write_24mp_jpeg(&source);
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

/// How the modelled gallery requests a tile first shown in the overscan.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScrollRequests {
    /// Before ADR 0021: the tile keeps `p=1` (High, FIFO) when it scrolls into view.
    StickyFifo,
    /// The rejected alternative, a LIFO High queue. Modelled by sending the
    /// prefetches at Interactive (LIFO): during a steady scroll nothing else is.
    LifoHigh,
    /// ADR 0021: a tile that scrolls into view before its result asks again
    /// without `p=1`, which moves the queued job to the front of Interactive.
    Reveal,
}

/// Completion time per tile index, with a condition variable to wait on.
type TileDone = Arc<(Mutex<Vec<Option<Instant>>>, Condvar)>;

fn wait_for_tiles(done: &TileDone, tiles: std::ops::Range<usize>) {
    let (lock, ready) = &**done;
    let mut times = lock.lock().unwrap();
    while times[tiles.clone()].iter().any(Option::is_none) {
        times = ready
            .wait_timeout(times, Duration::from_secs(120))
            .unwrap()
            .0;
    }
}

/// Steady scrolling over never-rendered images, then a stop. The gallery is
/// modelled with 8 columns, 5 visible rows and the 4 overscan rows ahead; one
/// row scrolls into view every 50 ms (20 rows/s, below the fast-scroll pause).
/// The real scheduler renders real JPEGs with the browsing worker count of
/// this host and with 2 workers (the browsing minimum, as on a 4-CPU host).
/// Every asset is a hard link to one synthetic 24 MP JPEG, so each has its own
/// cache key. Reports the backlog of queued renders at the stop and how long
/// each tile visible at the stop still waits for its result. Each run renders
/// into its own empty thumbnail directory and drains before the next starts.
#[test]
#[ignore = "baseline diagnostic: run explicitly, alone, in release mode"]
fn steady_scroll_visible_thumbnail_wait_baseline() {
    const COLUMNS: usize = 8;
    const VISIBLE_ROWS: usize = 5;
    const OVERSCAN_ROWS: usize = 4;
    const SCROLL_ROWS: usize = 160;
    const ROW_INTERVAL: Duration = Duration::from_millis(50);
    let rows = VISIBLE_ROWS + OVERSCAN_ROWS + SCROLL_ROWS;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("synthetic-24mp.jpg");
    write_24mp_jpeg(&source);
    let size = fs::metadata(&source).unwrap().len() as i64;
    let links: Vec<PathBuf> = (0..rows * COLUMNS)
        .map(|index| {
            let link = dir.path().join(format!("asset-{index}.jpg"));
            fs::hard_link(&source, &link).unwrap();
            link
        })
        .collect();
    let render_ms: Vec<f64> = (0..3)
        .map(|attempt| {
            let started = Instant::now();
            thumbs::create_image_thumb(&source, &dir.path().join(format!("probe-{attempt}.jpg")))
                .unwrap();
            ms(started)
        })
        .collect();
    let browsing = thread::available_parallelism()
        .map(|count| count.get().saturating_sub(2).clamp(2, 8))
        .unwrap_or(4);
    println!(
        "[baseline] metric=steady_scroll_setup columns={COLUMNS} visible_rows={VISIBLE_ROWS} overscan_rows={OVERSCAN_ROWS} rows_per_s={} scrolled_rows={SCROLL_ROWS} render {}",
        1_000 / ROW_INTERVAL.as_millis(),
        summary(&render_ms)
    );
    let mut worker_counts = vec![browsing, 2];
    worker_counts.dedup();
    let runs = worker_counts.into_iter().flat_map(|workers| {
        [
            ("sticky_fifo", ScrollRequests::StickyFifo),
            ("lifo_high", ScrollRequests::LifoHigh),
            ("reveal", ScrollRequests::Reveal),
        ]
        .map(|(name, mode)| (workers, name, mode))
    });
    for (workers, name, mode) in runs {
        let scheduler = ThumbnailScheduler::new(workers, "ffmpeg".into());
        let thumbs_dir = dir.path().join(format!("thumbs-{workers}-{name}"));
        fs::create_dir_all(&thumbs_dir).unwrap();
        let done: TileDone = Arc::new((Mutex::new(vec![None; links.len()]), Condvar::new()));
        let requested = AtomicUsize::new(0);
        let request = |index: usize, priority: ThumbnailPriority| {
            let source_path = links[index].clone();
            let version = thumbs::SourceVersion::new(source_path.to_string_lossy(), size, 0);
            let done = Arc::clone(&done);
            scheduler
                .enqueue_with_callback(
                    ThumbnailTask {
                        operation: None,
                        asset_id: index as i64,
                        source_path,
                        target_path: thumbs::thumb_target(&thumbs_dir, &version),
                        kind: "image".into(),
                        duration_ms: None,
                        source_version: version,
                        legacy_thumb: None,
                    },
                    priority,
                    Box::new(move |result| {
                        assert!(result.thumb_path.is_some(), "render failed");
                        let (lock, ready) = &*done;
                        lock.lock().unwrap()[index].get_or_insert_with(Instant::now);
                        ready.notify_all();
                    }),
                )
                .unwrap();
            requested.fetch_max(index + 1, Ordering::Relaxed);
        };
        let prefetch = if mode == ScrollRequests::LifoHigh {
            ThumbnailPriority::Interactive
        } else {
            ThumbnailPriority::High
        };
        let row = |row: usize| row * COLUMNS..(row + 1) * COLUMNS;
        // Mount: visible rows first, then the overscan ahead.
        for index in 0..VISIBLE_ROWS * COLUMNS {
            request(index, ThumbnailPriority::Interactive);
        }
        for index in row(VISIBLE_ROWS).start..row(VISIBLE_ROWS + OVERSCAN_ROWS - 1).end {
            request(index, prefetch);
        }
        let mut next = Instant::now();
        for step in 1..=SCROLL_ROWS {
            next += ROW_INTERVAL;
            thread::sleep(next.saturating_duration_since(Instant::now()));
            if mode == ScrollRequests::Reveal {
                let shown: Vec<usize> = {
                    let times = done.0.lock().unwrap();
                    row(step + VISIBLE_ROWS - 1)
                        .filter(|&index| times[index].is_none())
                        .collect()
                };
                for index in shown {
                    request(index, ThumbnailPriority::Interactive);
                }
            }
            for index in row(step + VISIBLE_ROWS + OVERSCAN_ROWS - 1) {
                request(index, prefetch);
            }
        }
        let stop = Instant::now();
        let visible = row(SCROLL_ROWS).start..row(SCROLL_ROWS + VISIBLE_ROWS - 1).end;
        let backlog = {
            let times = done.0.lock().unwrap();
            times[..requested.load(Ordering::Relaxed)]
                .iter()
                .filter(|time| time.is_none())
                .count()
        };
        wait_for_tiles(&done, visible.clone());
        let visible_wait: Vec<f64> = {
            let times = done.0.lock().unwrap();
            times[visible]
                .iter()
                .map(|time| time.unwrap().saturating_duration_since(stop).as_secs_f64() * 1_000.0)
                .collect()
        };
        let ready_at_stop = visible_wait.iter().filter(|wait| **wait == 0.0).count();
        wait_for_tiles(&done, 0..links.len());
        println!(
            "[baseline] metric=steady_scroll_visible_wait workers={workers} mode={name} backlog_at_stop={backlog} visible_ready_at_stop={ready_at_stop}/{} drain_ms={:.0} wait {}",
            visible_wait.len(),
            ms(stop),
            summary(&visible_wait)
        );
    }
}
