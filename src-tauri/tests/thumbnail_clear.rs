use media_tagger::{
    app::state::AppState,
    db,
    models::NewAsset,
    services::{db_pool::DatabaseRuntime, thumb_scheduler::ThumbnailScheduler, thumb_service},
};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Mutex, RwLock,
    },
};

#[test]
fn rescan_then_clear_all_removes_obsolete_and_current_files_and_regenerates() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("media.db");
    let thumbs = dir.path().join("thumbs");
    fs::create_dir(&thumbs).unwrap();
    let old_thumb = thumbs.join("old.jpg");
    fs::write(&old_thumb, b"old generated thumbnail").unwrap();
    let conn = db::open_connection(&db_path).unwrap();
    db::init_schema(&conn).unwrap();
    let mut asset = NewAsset {
        path: dir.path().join("photo.jpg").to_string_lossy().into(),
        kind: "image".into(),
        size_bytes: 100,
        modified_at: 1,
        width: Some(10),
        height: Some(10),
        duration_ms: None,
        thumb_path: Some(old_thumb.to_string_lossy().into()),
    };
    db::upsert_asset(&conn, &asset).unwrap();
    // The normal rescan write after an edited image invalidates its thumbnail.
    asset.modified_at = 2;
    asset.thumb_path = None;
    db::upsert_asset(&conn, &asset).unwrap();
    let source = dir.path().join("photo.jpg");
    image::RgbImage::new(20, 20).save(&source).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, thumbs.join("source-link.jpg")).unwrap();
        std::os::unix::fs::symlink(dir.path(), thumbs.join("outside-directory")).unwrap();
    }
    let nested = thumbs.join("nested/deeper");
    fs::create_dir_all(&nested).unwrap();
    let current_thumb = nested.join("current.jpg");
    fs::write(&current_thumb, b"current thumbnail").unwrap();
    asset.path = dir.path().join("other.jpg").to_string_lossy().into();
    asset.thumb_path = Some(current_thumb.to_string_lossy().into());
    db::upsert_asset(&conn, &asset).unwrap();
    db::record_thumbnail_failure(&conn, 1, 2, Some("failed")).unwrap();
    drop(conn);
    let state = AppState {
        database: DatabaseRuntime::new(db_path.clone()),
        db_path,
        thumbs_dir: thumbs,
        ffmpeg_path: PathBuf::from("ffmpeg"),
        scan_lock: Mutex::new(()),
        thumb_lock: RwLock::new(()),
        thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
        thumbnail_render_all_running: AtomicBool::new(false),
        thumbnail_render_all_cancel_requested: AtomicBool::new(false),
        thumbnail_generation: AtomicU64::new(0),
    };
    let state = std::sync::Arc::new(state);
    let reader = state.thumb_lock.read().unwrap();
    let worker_state = state.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let removed = media_tagger::app::locks::with_thumb_lock(&worker_state, |permit| {
            thumb_service::clear_all_thumbnails(&worker_state, permit, &|_| Ok(()))
        })
        .unwrap();
        finished_tx.send(removed).unwrap();
    });
    started_rx.recv().unwrap();
    assert!(finished_rx
        .recv_timeout(std::time::Duration::from_millis(50))
        .is_err());
    assert!(old_thumb.exists());
    drop(reader);
    assert_eq!(
        finished_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
        2
    );
    worker.join().unwrap();
    assert!(!old_thumb.exists());
    assert!(!current_thumb.exists());
    assert!(!nested.exists());
    assert!(state.thumbs_dir.is_dir());
    assert!(source.exists());
    assert_eq!(
        state
            .thumbnail_generation
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    let permit = state.database.admit().unwrap();
    let conn = permit.connection().unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM assets WHERE thumb_path IS NOT NULL",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM thumbnail_failures", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let regenerated = media_tagger::app::locks::with_thumb_read_lock(&state, |permit| {
        thumb_service::ensure_asset_thumbnail(1, &state, permit)
    })
    .unwrap()
    .unwrap();
    assert!(std::path::Path::new(&regenerated).is_file());
    assert!(source.exists());
}
