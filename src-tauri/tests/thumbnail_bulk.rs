use media_tagger::{
    app::state::AppState,
    db,
    models::NewAsset,
    services::{db_pool::DatabaseRuntime, thumb_scheduler::ThumbnailScheduler, thumb_service},
};
use std::{
    fs,
    path::Path,
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Mutex, RwLock,
    },
};

fn index_fixture(conn: &rusqlite::Connection, path: &Path, kind: &str) -> i64 {
    db::upsert_asset(
        conn,
        &NewAsset {
            path: path.to_string_lossy().into_owned(),
            kind: kind.into(),
            size_bytes: fs::metadata(path).unwrap().len() as i64,
            modified_at: 1,
            width: Some(32),
            height: Some(24),
            duration_ms: (kind == "video").then_some(500),
            thumb_path: None,
        },
    )
    .unwrap();
    conn.query_row(
        "SELECT id FROM assets WHERE path = ?1",
        [path.to_string_lossy().as_ref()],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn bulk_render_and_retry_publish_decodable_jpegs_for_temporary_media() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("media.db");
    let thumbs_dir = dir.path().join("thumbs");
    fs::create_dir(&thumbs_dir).unwrap();
    let conn = db::open_connection(&db_path).unwrap();
    db::init_schema(&conn).unwrap();
    let image = image::RgbImage::from_pixel(32, 24, image::Rgb([20, 160, 230]));
    let mut prefailed = 0;
    for id in 0..36 {
        let source = dir.path().join(format!("image-{id}.png"));
        image.save(&source).unwrap();
        prefailed = index_fixture(&conn, &source, "image");
    }
    db::record_thumbnail_failure(&conn, prefailed, 1, Some("retry fixture")).unwrap();
    let gif = dir.path().join("animation.gif");
    image.save(&gif).unwrap();
    index_fixture(&conn, &gif, "gif");
    let video = dir.path().join("clip.mp4");
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=32x24:d=0.5",
            "-c:v",
            "mpeg4",
            "-threads",
            "1",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&video)
        .output()
        .expect("ffmpeg must be installed for the temporary-video smoke test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    index_fixture(&conn, &video, "video");
    let corrupt = dir.path().join("repair.png");
    fs::write(&corrupt, b"invalid image").unwrap();
    index_fixture(&conn, &corrupt, "image");
    let state = AppState {
        database: DatabaseRuntime::new(db_path.clone()),
        db_path,
        thumbs_dir,
        ffmpeg_path: "ffmpeg".into(),
        scan_lock: Mutex::new(()),
        thumb_lock: RwLock::new(()),
        thumb_scheduler: ThumbnailScheduler::from_available_parallelism("ffmpeg".into()),
        thumbnail_render_all_running: AtomicBool::new(false),
        thumbnail_render_all_cancel_requested: AtomicBool::new(false),
        thumbnail_generation: AtomicU64::new(0),
    };
    assert!(state.thumb_scheduler.is_healthy());
    let permit = state.database.admit().unwrap();
    let all = thumb_service::render_all_thumbnails(&state, &permit, &|_| Ok(())).unwrap();
    assert_eq!(
        (
            all.total,
            all.processed,
            all.generated,
            all.failed,
            all.skipped_failed
        ),
        (39, 39, 37, 1, 1)
    );
    assert!(!all.cancelled);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM assets WHERE thumb_path IS NOT NULL",
            [],
            |row| row.get::<_, usize>(0)
        )
        .unwrap(),
        37
    );
    // Repair the decoder input without re-indexing, so Retry failed exercises
    // the same source-version failure markers as the failed first pass.
    image.save(&corrupt).unwrap();
    let retry = thumb_service::render_failed_thumbnails(&state, &permit, &|_| Ok(())).unwrap();
    assert_eq!(
        (retry.total, retry.processed, retry.generated, retry.failed),
        (2, 2, 2, 0)
    );
    let mut statement = conn
        .prepare("SELECT thumb_path FROM assets ORDER BY id")
        .unwrap();
    let paths: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(paths.len(), 39);
    for path in paths {
        assert!(Path::new(&path).starts_with(&state.thumbs_dir));
        let bytes = fs::read(path).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Jpeg
        );
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert!(decoded.width() > 0 && decoded.width() <= 390);
        assert!(decoded.height() > 0 && decoded.height() <= 390);
    }
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM thumbnail_failures", [], |row| row
            .get::<_, usize>(
            0
        ))
        .unwrap(),
        0
    );
    assert_eq!(fs::read_dir(&state.thumbs_dir).unwrap().count(), 39);
    eprintln!("Rendered 39 temporary image/GIF/video assets; decoded every published JPEG and verified SQLite paths and cleared failure markers.");
}
