//! Video use case: the core part of `open_video`. Called by `commands/video.rs`.
//!
//! The native player, its surface and its event channel stay in the adapter
//! (ADR 0034, D1). The core only resolves and authorizes the source of an
//! asset ID; the frontend never sends a path.

use std::{path::PathBuf, sync::Arc};

use crate::{
    error::{AppError, AppResult},
    services::{db_pool::DatabaseRuntime, video_source_service},
};

/// The canonical path of the video `asset_id`, authorized by
/// [`video_source_service::resolve_video_path`]. Runs on the command's
/// blocking task.
pub fn open_video_service(database: &Arc<DatabaseRuntime>, asset_id: i64) -> AppResult<PathBuf> {
    let conn = database.admit()?.connection()?;
    video_source_service::resolve_video_path(&conn, asset_id).map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn open_video_service_resolves_an_authorized_source_and_keeps_the_error_text() {
        let temp = tempfile::tempdir().expect("temp dir");
        let db_path = temp.path().join("media.db");
        let video = temp.path().join("clip.mp4");
        std::fs::write(&video, b"video").expect("video");
        let conn = db::open_connection(&db_path).expect("database");
        db::init_schema(&conn).expect("schema");
        let root = temp.path().to_string_lossy();
        conn.execute(
            "INSERT INTO assets (id, path, file_name, kind, size_bytes, modified_at) \
             VALUES (1, ?1, 'clip.mp4', 'video', 1, 1)",
            [video.to_string_lossy()],
        )
        .expect("asset row");
        db::add_scan_root(&conn, &root).expect("scan root");
        conn.execute(
            "INSERT INTO asset_scan_roots(asset_id, root_path, last_seen_generation) VALUES (1, ?1, 1)",
            [&root],
        )
        .expect("asset root mapping");
        drop(conn);
        let database = DatabaseRuntime::new(db_path.clone());

        assert_eq!(
            open_video_service(&database, 1).expect("authorized source"),
            video.canonicalize().expect("canonical source")
        );
        let conn = db::open_connection_read_only(&db_path).expect("read-only connection");
        for asset_id in [0, 99] {
            let expected = video_source_service::resolve_video_path(&conn, asset_id)
                .map_err(AppError::from)
                .expect_err("rejected source")
                .to_string();
            let error = open_video_service(&database, asset_id)
                .expect_err("rejected source")
                .to_string();
            assert_eq!(error, expected);
        }
        assert_eq!(
            open_video_service(&database, 99).unwrap_err().to_string(),
            "asset 99 not found"
        );
    }
}
