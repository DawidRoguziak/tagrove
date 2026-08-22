use std::{collections::HashMap, path::Path};

use media_tagger::{
    db,
    models::NewAsset,
    utils::tags::{merge_tags, normalize_tags, parse_csv_tags},
};
use tempfile::tempdir;

fn asset(path: &Path, modified_at: i64) -> NewAsset {
    NewAsset {
        path: path.to_string_lossy().to_string(),
        kind: "image".to_string(),
        size_bytes: 5,
        modified_at,
        width: Some(100),
        height: Some(100),
        duration_ms: None,
        thumb_path: None,
    }
}

#[test]
fn backend_end_to_end_csv_merge_and_library_clear_workflow() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let mut conn = db::open_connection(&db_path).expect("open db");
    db::init_schema(&conn).expect("init schema");

    let root = tmp.path().join("library");
    std::fs::create_dir_all(&root).expect("create root");
    db::add_scan_root(&conn, &root.to_string_lossy()).expect("add scan root");

    let a = root.join("beach.jpg");
    let b = root.join("city.jpg");
    std::fs::write(&a, b"a").expect("write a");
    std::fs::write(&b, b"b").expect("write b");

    db::upsert_asset(&conn, &asset(&a, 30)).expect("upsert a");
    db::upsert_asset(&conn, &asset(&b, 20)).expect("upsert b");
    db::set_asset_tags(&conn, 1, &["cat".to_string()]).expect("set tags a");
    db::set_asset_tags(&conn, 2, &["dog".to_string()]).expect("set tags b");
    db::set_asset_media_group(&conn, 1, Some("group-alpha"), Some(2.5)).expect("set media group");

    let exported_rows = db::list_assets_for_csv_export(&conn).expect("export rows");
    assert_eq!(exported_rows.len(), 2);
    let beach_export = exported_rows
        .iter()
        .find(|row| row.path.ends_with("beach.jpg"))
        .expect("beach export row");
    assert_eq!(beach_export.media_group_key.as_deref(), Some("group-alpha"));
    assert_eq!(beach_export.media_group_order, Some(2.5));

    let mut incoming_by_file = HashMap::<String, Vec<String>>::new();
    incoming_by_file.insert(
        "beach.jpg".to_string(),
        normalize_tags(parse_csv_tags("vacation, summer; cat")),
    );
    incoming_by_file.insert(
        "city.jpg".to_string(),
        normalize_tags(parse_csv_tags("night travel")),
    );

    let mut by_name = HashMap::<String, Vec<i64>>::new();
    for row in db::list_asset_paths(&conn).expect("asset paths") {
        let file_name = Path::new(&row.path)
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .to_lowercase();
        by_name.entry(file_name).or_default().push(row.id);
    }

    for (file_name, imported_tags) in incoming_by_file {
        if let Some(asset_ids) = by_name.get(&file_name.to_lowercase()) {
            for asset_id in asset_ids {
                let existing = db::list_asset_tags(&conn, *asset_id).expect("existing tags");
                let merged = merge_tags(&existing, &imported_tags);
                db::set_asset_tags(&conn, *asset_id, &merged).expect("set merged tags");
            }
        }
    }

    let page = db::list_assets(
        &conn,
        0,
        50,
        &["vacation".to_string(), "cat".to_string()],
        &[],
        Some("image"),
        false,
    )
    .expect("filtered by merged tags");
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].path, a.to_string_lossy().to_string());

    let all_tags = db::list_tags_page(&conn, "", 0, 500).expect("list tags");
    assert!(all_tags.items.iter().any(|tag| tag == "vacation"));
    assert!(all_tags.items.iter().any(|tag| tag == "summer"));
    assert!(all_tags.items.iter().any(|tag| tag == "travel"));

    let (processed_groups, updated_groups) = db::set_assets_media_group_bulk(
        &mut conn,
        &[(2, Some(1.0)), (1, Some(2.0))],
        Some("trip-2026"),
    )
    .expect("set groups in bulk");
    assert_eq!(processed_groups, 2);
    assert_eq!(updated_groups, 2);

    let grouped =
        db::list_assets(&conn, 0, 50, &[], &[], Some("image"), false).expect("grouped page");
    let beach = grouped
        .items
        .iter()
        .find(|item| item.path == a.to_string_lossy())
        .expect("beach grouped item");
    let city = grouped
        .items
        .iter()
        .find(|item| item.path == b.to_string_lossy())
        .expect("city grouped item");
    assert_eq!(beach.media_group_key.as_deref(), Some("trip-2026"));
    assert_eq!(beach.media_group_order, Some(2.0));
    assert_eq!(city.media_group_key.as_deref(), Some("trip-2026"));
    assert_eq!(city.media_group_order, Some(1.0));

    let (removed_assets, removed_roots, _) = db::clear_library_data(&conn).expect("clear data");
    assert_eq!(removed_assets, 2);
    assert_eq!(removed_roots, 1);

    let remaining = db::list_assets(&conn, 0, 50, &[], &[], None, false).expect("remaining assets");
    assert_eq!(remaining.total, 0);
    let roots = db::list_scan_roots(&conn).expect("remaining roots");
    assert!(roots.is_empty());
}
