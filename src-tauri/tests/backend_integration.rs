use std::path::Path;

use media_tagger::{db, models::NewAsset};
use tempfile::tempdir;

fn new_asset(path: &Path, kind: &str, modified_at: i64, thumb_path: Option<&str>) -> NewAsset {
    NewAsset {
        path: path.to_string_lossy().to_string(),
        kind: kind.to_string(),
        size_bytes: 10,
        modified_at,
        width: None,
        height: None,
        duration_ms: None,
        thumb_path: thumb_path.map(str::to_string),
    }
}

#[test]
fn sqlite_file_db_supports_asset_tag_query_flow() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let conn = db::open_connection(&db_path).expect("open db");
    db::init_schema(&conn).expect("init schema");

    let root = tmp.path().join("library");
    let nested = root.join("sub");
    std::fs::create_dir_all(&nested).expect("create dirs");

    let a = root.join("a.jpg");
    let b = nested.join("b.jpg");
    let c = tmp.path().join("other").join("c.jpg");
    std::fs::create_dir_all(c.parent().expect("parent")).expect("create other");
    std::fs::write(&a, b"a").expect("write a");
    std::fs::write(&b, b"b").expect("write b");
    std::fs::write(&c, b"c").expect("write c");

    db::upsert_asset(&conn, &new_asset(&a, "image", 30, Some("thumb-a.jpg"))).expect("upsert a");
    db::upsert_asset(&conn, &new_asset(&b, "image", 20, Some("thumb-b.jpg"))).expect("upsert b");
    db::upsert_asset(&conn, &new_asset(&c, "image", 10, Some("thumb-c.jpg"))).expect("upsert c");

    db::set_asset_tags(&conn, 1, &["cat".to_string(), "vacation".to_string()]).expect("tags a");
    db::set_asset_tags(&conn, 2, &["cat".to_string(), "dog".to_string()]).expect("tags b");
    db::set_asset_tags(&conn, 3, &["cat".to_string(), "vacation".to_string()]).expect("tags c");

    let filtered = db::list_assets(
        &conn,
        0,
        100,
        &["cat".to_string(), "vacation".to_string()],
        &["dog".to_string()],
        Some("image"),
        false,
    )
    .expect("filtered assets");

    assert_eq!(filtered.total, 2);
    assert_eq!(filtered.items.len(), 2);

    let root_norm = root.to_string_lossy().to_string();
    let (removed, thumbs) =
        db::delete_assets_by_prefix_with_thumbs(&conn, &root_norm).expect("remove by prefix");
    assert_eq!(removed, 2);
    assert_eq!(thumbs.len(), 2);

    let left = db::list_assets(&conn, 0, 100, &[], &[], None, false).expect("left assets");
    assert_eq!(left.total, 1);
    assert_eq!(left.items[0].path, c.to_string_lossy().to_string());
}

#[test]
fn delete_assets_by_prefix_returns_distinct_thumbnails() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let conn = db::open_connection(&db_path).expect("open db");
    db::init_schema(&conn).expect("init schema");

    let root = tmp.path().join("library");
    std::fs::create_dir_all(&root).expect("create root");
    let one = root.join("1.jpg");
    let two = root.join("2.jpg");
    std::fs::write(&one, b"1").expect("write one");
    std::fs::write(&two, b"2").expect("write two");

    db::upsert_asset(&conn, &new_asset(&one, "image", 1, Some("same-thumb.jpg")))
        .expect("upsert one");
    db::upsert_asset(&conn, &new_asset(&two, "image", 2, Some("same-thumb.jpg")))
        .expect("upsert two");

    let (removed, thumbs) = db::delete_assets_by_prefix_with_thumbs(&conn, &root.to_string_lossy())
        .expect("delete by root");
    assert_eq!(removed, 2);
    assert_eq!(thumbs, vec!["same-thumb.jpg".to_string()]);
}

#[test]
fn clear_library_data_removes_assets_roots_tags_and_failures() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let conn = db::open_connection(&db_path).expect("open db");
    db::init_schema(&conn).expect("init schema");

    let first_root = tmp.path().join("library-a");
    let second_root = tmp.path().join("library-b");
    std::fs::create_dir_all(&first_root).expect("create first root");
    std::fs::create_dir_all(&second_root).expect("create second root");

    db::add_scan_root(&conn, &first_root.to_string_lossy()).expect("add first root");
    db::add_scan_root(&conn, &second_root.to_string_lossy()).expect("add second root");

    let a = first_root.join("a.jpg");
    let b = second_root.join("b.jpg");
    std::fs::write(&a, b"a").expect("write a");
    std::fs::write(&b, b"b").expect("write b");

    db::upsert_asset(&conn, &new_asset(&a, "image", 10, Some("shared-thumb.jpg")))
        .expect("upsert a");
    db::upsert_asset(&conn, &new_asset(&b, "image", 20, Some("shared-thumb.jpg")))
        .expect("upsert b");
    db::set_asset_tags(&conn, 1, &["cat".to_string(), "vacation".to_string()]).expect("tags a");
    db::set_asset_tags(&conn, 2, &["dog".to_string()]).expect("tags b");
    db::record_thumbnail_failure(&conn, 1, 10, Some("thumb error")).expect("record fail");
    let baseline_revision = db::current_library_revision(&conn).expect("baseline revision");

    let (removed_assets, removed_roots, thumbs) =
        db::clear_library_data(&conn).expect("clear data");
    assert_eq!(removed_assets, 2);
    assert_eq!(removed_roots, 2);
    assert_eq!(thumbs, vec!["shared-thumb.jpg".to_string()]);
    assert_eq!(
        db::current_library_revision(&conn).expect("revision after clear"),
        baseline_revision + 1
    );

    let page =
        db::list_assets(&conn, 0, 100, &[], &[], None, false).expect("list assets after clear");
    assert_eq!(page.total, 0);

    let roots = db::list_scan_roots(&conn).expect("list roots after clear");
    assert!(roots.is_empty());

    let tags = db::list_tags_page(&conn, "", 0, 500).expect("list tags after clear");
    assert!(tags.items.is_empty());

    let failed = db::list_failed_thumbnail_asset_ids(&conn).expect("failed ids after clear");
    assert!(failed.is_empty());
}

#[test]
fn clear_library_data_rolls_back_deletes_when_revision_bump_fails() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let conn = db::open_connection(&db_path).expect("open db");
    db::init_schema(&conn).expect("init schema");
    let root = tmp.path().join("library");
    std::fs::create_dir_all(&root).expect("create root");
    db::add_scan_root(&conn, &root.to_string_lossy()).expect("add root");
    db::upsert_asset(&conn, &new_asset(&root.join("a.jpg"), "image", 1, None))
        .expect("upsert asset");
    let baseline = db::current_library_revision(&conn).expect("baseline revision");
    conn.execute_batch(
        "CREATE TEMP TRIGGER fail_clear_revision
         BEFORE UPDATE ON library_metadata
         WHEN OLD.key = 'revision'
         BEGIN SELECT RAISE(ABORT, 'injected revision failure'); END;",
    )
    .expect("create trigger");

    assert!(db::clear_library_data(&conn).is_err());
    assert_eq!(
        db::list_assets(&conn, 0, 10, &[], &[], None, false)
            .expect("assets")
            .total,
        1
    );
    assert_eq!(db::list_scan_roots(&conn).expect("roots").len(), 1);
    assert_eq!(
        db::current_library_revision(&conn).expect("revision"),
        baseline
    );
}

#[test]
fn merge_asset_tags_bulk_updates_multiple_assets_atomically() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let conn = db::open_connection(&db_path).expect("open db");
    db::init_schema(&conn).expect("init schema");

    let root = tmp.path().join("library");
    std::fs::create_dir_all(&root).expect("create root");
    let first = root.join("first.jpg");
    let second = root.join("second.jpg");
    std::fs::write(&first, b"1").expect("write first");
    std::fs::write(&second, b"2").expect("write second");

    db::upsert_asset(&conn, &new_asset(&first, "image", 2, None)).expect("upsert first");
    db::upsert_asset(&conn, &new_asset(&second, "image", 1, None)).expect("upsert second");
    db::set_asset_tags(&conn, 1, &["cat".to_string()]).expect("set first tags");
    db::set_asset_tags(&conn, 2, &["dog".to_string()]).expect("set second tags");

    let (processed, updated) =
        db::merge_asset_tags_bulk(&conn, &[1, 2], &["travel".to_string(), "CAT".to_string()])
            .expect("merge tags");

    assert_eq!(processed, 2);
    assert_eq!(updated, 2);

    let first_tags = db::list_asset_tags(&conn, 1).expect("first tags");
    let second_tags = db::list_asset_tags(&conn, 2).expect("second tags");
    assert!(first_tags.iter().any(|tag| tag == "travel"));
    assert!(second_tags.iter().any(|tag| tag == "travel"));
    assert!(db::list_tags_page(&conn, "", 0, 500)
        .expect("tags")
        .items
        .iter()
        .any(|tag| tag == "travel"));
}

#[test]
fn set_assets_media_group_bulk_overwrites_existing_values() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let mut conn = db::open_connection(&db_path).expect("open db");
    db::init_schema(&conn).expect("init schema");

    let root = tmp.path().join("library");
    std::fs::create_dir_all(&root).expect("create root");
    let first = root.join("first.jpg");
    let second = root.join("second.jpg");
    std::fs::write(&first, b"1").expect("write first");
    std::fs::write(&second, b"2").expect("write second");

    db::upsert_asset(&conn, &new_asset(&first, "image", 2, None)).expect("upsert first");
    db::upsert_asset(&conn, &new_asset(&second, "image", 1, None)).expect("upsert second");
    db::set_asset_media_group(&conn, 1, Some("legacy"), Some(50.0)).expect("seed first");

    let (processed, updated) = db::set_assets_media_group_bulk(
        &mut conn,
        &[(2, Some(1.0)), (1, Some(2.0))],
        Some("trip-2026"),
    )
    .expect("set bulk group");

    assert_eq!(processed, 2);
    assert_eq!(updated, 2);

    let page = db::list_assets(&conn, 0, 100, &[], &[], None, false).expect("list assets");
    let first_asset = page
        .items
        .iter()
        .find(|asset| asset.id == 1)
        .expect("first asset");
    let second_asset = page
        .items
        .iter()
        .find(|asset| asset.id == 2)
        .expect("second asset");

    assert_eq!(first_asset.media_group_key.as_deref(), Some("trip-2026"));
    assert_eq!(first_asset.media_group_order, Some(2.0));
    assert_eq!(second_asset.media_group_key.as_deref(), Some("trip-2026"));
    assert_eq!(second_asset.media_group_order, Some(1.0));
}
