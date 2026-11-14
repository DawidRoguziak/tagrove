use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::db;

pub fn resolve_video_path(conn: &rusqlite::Connection, asset_id: i64) -> anyhow::Result<PathBuf> {
    if asset_id <= 0 {
        anyhow::bail!("asset id must be positive");
    }

    let asset = db::get_video_asset_source(conn, asset_id)?
        .with_context(|| format!("asset {asset_id} not found"))?;
    if asset.kind != "video" {
        anyhow::bail!("asset {asset_id} is not a video");
    }

    let source = PathBuf::from(asset.path)
        .canonicalize()
        .context("video file is unavailable")?;
    if !source
        .metadata()
        .context("video file is unavailable")?
        .is_file()
    {
        anyhow::bail!("video path is not a regular file");
    }
    if asset.scan_roots.is_empty() {
        anyhow::bail!("asset {asset_id} has no assigned scan root");
    }

    let authorized = asset.scan_roots.iter().any(|root| {
        Path::new(root)
            .canonicalize()
            .is_ok_and(|canonical_root| source.starts_with(canonical_root))
    });
    if !authorized {
        anyhow::bail!("asset {asset_id} source is outside its assigned scan roots");
    }

    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn seed_asset(conn: &Connection, id: i64, path: &Path, kind: &str, root: Option<&Path>) {
        conn.execute(
            "INSERT INTO assets (id, path, file_name, kind, size_bytes, modified_at) VALUES (?1, ?2, ?3, ?4, 1, 1)",
            rusqlite::params![
                id,
                path.to_string_lossy(),
                path.file_name().expect("file name").to_string_lossy(),
                kind
            ],
        )
        .expect("asset row");
        if let Some(root) = root {
            let root = root.to_string_lossy();
            db::add_scan_root(conn, &root).expect("scan root");
            conn.execute(
                "INSERT INTO asset_scan_roots(asset_id, root_path, last_seen_generation) VALUES (?1, ?2, 1)",
                rusqlite::params![id, root],
            )
            .expect("asset root mapping");
        }
    }

    #[test]
    fn validates_id_kind_file_and_assigned_root() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("media.db");
        let video = temp.path().join("clip.mp4");
        let image = temp.path().join("image.jpg");
        let missing = temp.path().join("missing.mp4");
        let unassigned = temp.path().join("unassigned.mp4");
        std::fs::write(&video, b"video").expect("video");
        std::fs::write(&image, b"image").expect("image");
        std::fs::write(&unassigned, b"video").expect("unassigned video");
        let conn = db::open_connection(&db_path).expect("database");
        db::init_schema(&conn).expect("schema");
        seed_asset(&conn, 1, &video, "video", Some(temp.path()));
        seed_asset(&conn, 2, &image, "image", Some(temp.path()));
        seed_asset(&conn, 3, &missing, "video", Some(temp.path()));
        seed_asset(&conn, 4, &unassigned, "video", None);
        drop(conn);

        assert_eq!(
            resolve_video_path(&db::open_connection_read_only(&db_path).unwrap(), 1)
                .expect("authorized source"),
            video.canonicalize().expect("canonical source")
        );
        assert!(resolve_video_path(&db::open_connection_read_only(&db_path).unwrap(), 0).is_err());
        assert!(resolve_video_path(&db::open_connection_read_only(&db_path).unwrap(), 99).is_err());
        assert!(resolve_video_path(&db::open_connection_read_only(&db_path).unwrap(), 2).is_err());
        assert!(resolve_video_path(&db::open_connection_read_only(&db_path).unwrap(), 3).is_err());
        assert!(resolve_video_path(&db::open_connection_read_only(&db_path).unwrap(), 4).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temp dir");
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::create_dir_all(&outside).expect("outside");
        let target = outside.join("clip.mp4");
        let link = root.join("clip.mp4");
        std::fs::write(&target, b"outside").expect("outside video");
        symlink(&target, &link).expect("symlink");
        let db_path = temp.path().join("media.db");
        let conn = db::open_connection(&db_path).expect("database");
        db::init_schema(&conn).expect("schema");
        seed_asset(&conn, 1, &link, "video", Some(&root));
        drop(conn);

        assert!(resolve_video_path(&db::open_connection_read_only(&db_path).unwrap(), 1).is_err());
    }
}
