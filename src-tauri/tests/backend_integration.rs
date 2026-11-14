use std::path::Path;
#[path = "support/assets.rs"]
mod support;

use media_tagger::{db, models::NewAsset, services::asset_mutation_service};
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

    let filtered = support::session_page(
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

    let left = support::session_page(&conn, 0, 100, &[], &[], None, false).expect("left assets");
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

    let page = support::session_page(&conn, 0, 100, &[], &[], None, false)
        .expect("list assets after clear");
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
        support::session_page(&conn, 0, 10, &[], &[], None, false)
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

    assert_eq!(processed, vec![2, 1]);
    assert_eq!(updated, 2);

    let page = support::session_page(&conn, 0, 100, &[], &[], None, false).expect("list assets");
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

// ---------------------------------------------------------------------------
// Golden ordering tests (grid-performance stage 03).
//
// `legacy_ordered_ids` is a verbatim copy of the session ordering query as it
// stood before the sort-key columns (commit 73989ec). It is the reference
// definition of the order: bucket statistics are computed over the filtered
// rows, groups are keyed by `'group:' || media_group_key` (so a BLOB key and a
// TEXT key with the same bytes share one bucket), blank keys are singletons.
// ---------------------------------------------------------------------------

mod golden_order {
    use media_tagger::db::{self, AssetMetaFilter};
    use rusqlite::{params, types::Value, Connection};

    #[derive(Clone, Debug)]
    pub struct Filters {
        pub tags_and: Vec<String>,
        pub tags_not: Vec<String>,
        pub kind: Option<&'static str>,
        pub favorites_only: bool,
        pub meta: Option<AssetMetaFilter>,
    }

    impl Filters {
        fn none() -> Self {
            Self {
                tags_and: Vec::new(),
                tags_not: Vec::new(),
                kind: None,
                favorites_only: false,
                meta: None,
            }
        }
    }

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    pub fn filter_matrix() -> Vec<(&'static str, Filters)> {
        let base = Filters::none;
        vec![
            ("none", base()),
            (
                "kind image",
                Filters {
                    kind: Some("image"),
                    ..base()
                },
            ),
            (
                "kind video",
                Filters {
                    kind: Some("video"),
                    ..base()
                },
            ),
            (
                "kind gif",
                Filters {
                    kind: Some("gif"),
                    ..base()
                },
            ),
            (
                "favorites",
                Filters {
                    favorites_only: true,
                    ..base()
                },
            ),
            (
                "and t1",
                Filters {
                    tags_and: tags(&["t1"]),
                    ..base()
                },
            ),
            (
                "and t1 t2",
                Filters {
                    tags_and: tags(&["t1", "t2"]),
                    ..base()
                },
            ),
            (
                "and missing",
                Filters {
                    tags_and: tags(&["missing-tag"]),
                    ..base()
                },
            ),
            (
                "and t1 missing",
                Filters {
                    tags_and: tags(&["t1", "missing-tag"]),
                    ..base()
                },
            ),
            (
                "not t3",
                Filters {
                    tags_not: tags(&["t3"]),
                    ..base()
                },
            ),
            (
                "not t3 t4",
                Filters {
                    tags_not: tags(&["t3", "t4"]),
                    ..base()
                },
            ),
            (
                "not missing",
                Filters {
                    tags_not: tags(&["missing-tag"]),
                    ..base()
                },
            ),
            (
                "and t1 not t2",
                Filters {
                    tags_and: tags(&["t1"]),
                    tags_not: tags(&["t2"]),
                    ..base()
                },
            ),
            (
                "no tags",
                Filters {
                    meta: Some(AssetMetaFilter::HasNoTags { tag_count: 0 }),
                    ..base()
                },
            ),
            (
                "two tags",
                Filters {
                    meta: Some(AssetMetaFilter::HasNoTags { tag_count: 2 }),
                    ..base()
                },
            ),
            (
                "group g1",
                Filters {
                    meta: Some(AssetMetaFilter::GroupName {
                        group_name: "g1".into(),
                    }),
                    ..base()
                },
            ),
            (
                "group g2 favorites",
                Filters {
                    favorites_only: true,
                    meta: Some(AssetMetaFilter::GroupName {
                        group_name: "g2".into(),
                    }),
                    ..base()
                },
            ),
            (
                "image favorites not t1",
                Filters {
                    kind: Some("image"),
                    favorites_only: true,
                    tags_not: tags(&["t1"]),
                    ..base()
                },
            ),
            (
                "video and t2 not t5",
                Filters {
                    kind: Some("video"),
                    tags_and: tags(&["t2"]),
                    tags_not: tags(&["t5"]),
                    ..base()
                },
            ),
        ]
    }

    pub fn current_ordered_ids(conn: &Connection, filters: &Filters) -> Vec<i64> {
        db::list_ordered_asset_ids_with_meta(
            conn,
            &filters.tags_and,
            &filters.tags_not,
            filters.kind,
            filters.favorites_only,
            filters.meta.as_ref(),
        )
        .expect("ordered ids")
    }

    /// Verbatim copy of the pre-stage-03 query (see module comment).
    pub fn legacy_ordered_ids(conn: &Connection, filters: &Filters) -> Vec<i64> {
        let mut where_clauses = Vec::<String>::new();
        let mut values = Vec::<Value>::new();
        if let Some(kind) = filters.kind {
            where_clauses.push("a.kind = ?".to_string());
            values.push(Value::Text(kind.to_string()));
        }
        if filters.favorites_only {
            where_clauses.push("a.is_favorite = 1".to_string());
        }
        if !filters.tags_and.is_empty() {
            let placeholders = vec!["?"; filters.tags_and.len()].join(",");
            where_clauses.push(format!(
                "a.id IN (
                  SELECT at.asset_id
                  FROM asset_tags at
                  JOIN tags t ON t.id = at.tag_id
                  WHERE t.name IN ({placeholders})
                  GROUP BY at.asset_id
                  HAVING COUNT(DISTINCT t.id) = ?
                )"
            ));
            values.extend(filters.tags_and.iter().cloned().map(Value::Text));
            values.push(Value::Integer(filters.tags_and.len() as i64));
        }
        if !filters.tags_not.is_empty() {
            let placeholders = vec!["?"; filters.tags_not.len()].join(",");
            where_clauses.push(format!(
                "NOT EXISTS (
                  SELECT 1 FROM asset_tags atn
                  JOIN tags tn ON tn.id = atn.tag_id
                  WHERE atn.asset_id = a.id AND tn.name IN ({placeholders})
                )"
            ));
            values.extend(filters.tags_not.iter().cloned().map(Value::Text));
        }
        match &filters.meta {
            Some(AssetMetaFilter::HasNoTags { tag_count }) => {
                where_clauses.push("a.tag_count = ?".to_string());
                values.push(Value::Integer(*tag_count));
            }
            Some(AssetMetaFilter::GroupName { group_name }) => {
                where_clauses.push("a.media_group_key_normalized = ?".to_string());
                values.push(Value::Text(group_name.trim().to_lowercase()));
            }
            None => {}
        }
        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };
        let sql = format!(
            "
            WITH filtered_assets AS (
              SELECT a.id, a.modified_at, a.media_group_key, a.media_group_order
              FROM assets a
              {where_sql}
            ),
            grouped_bucket_stats AS (
              SELECT
                'group:' || media_group_key AS sort_bucket,
                MAX(modified_at) AS bucket_modified_at,
                MAX(id) AS bucket_max_id
              FROM filtered_assets
              WHERE media_group_key IS NOT NULL AND trim(media_group_key) <> ''
              GROUP BY 'group:' || media_group_key
            )
            SELECT fa.id
            FROM filtered_assets fa
            LEFT JOIN grouped_bucket_stats bs
              ON fa.media_group_key IS NOT NULL
              AND trim(fa.media_group_key) <> ''
              AND bs.sort_bucket = 'group:' || fa.media_group_key
            ORDER BY
              CASE
                WHEN fa.media_group_key IS NOT NULL AND trim(fa.media_group_key) <> ''
                  THEN bs.bucket_modified_at
                ELSE fa.modified_at
              END DESC,
              CASE
                WHEN fa.media_group_key IS NOT NULL AND trim(fa.media_group_key) <> ''
                  THEN bs.bucket_max_id
                ELSE fa.id
              END DESC,
              CASE
                WHEN fa.media_group_key IS NOT NULL
                  AND trim(fa.media_group_key) <> ''
                  AND fa.media_group_order IS NULL
                THEN 1 ELSE 0
              END ASC,
              fa.media_group_order ASC,
              fa.modified_at DESC,
              fa.id DESC
            "
        );
        let mut stmt = conn.prepare(&sql).expect("prepare legacy order");
        let rows = stmt
            .query_map(rusqlite::params_from_iter(values.iter()), |row| row.get(0))
            .expect("legacy order rows");
        rows.collect::<Result<Vec<i64>, _>>().expect("legacy ids")
    }

    pub fn open() -> Connection {
        let conn = Connection::open_in_memory().expect("db");
        db::init_schema(&conn).expect("schema");
        conn
    }

    /// (id, kind, modified_at, favorite, group key, group order, tags)
    type RawRow<'a> = (i64, &'a str, i64, bool, Value, Value, Vec<&'a str>);

    fn insert_raw(conn: &Connection, row: RawRow<'_>) {
        let (id, kind, modified_at, favorite, group_key, group_order, tag_names) = row;
        conn.execute(
            "INSERT INTO assets(id, path, file_name, file_name_key, kind, size_bytes, modified_at,
                                is_favorite, media_group_key, media_group_order)
             VALUES (?1, '/synthetic/golden/' || ?1 || '.bin', ?1 || '.bin', ?1 || '.bin',
                     ?2, 1, ?3, ?4, ?5, ?6)",
            params![
                id,
                kind,
                modified_at,
                favorite as i64,
                group_key,
                group_order
            ],
        )
        .expect("insert golden asset");
        for name in tag_names {
            conn.execute(
                "INSERT OR IGNORE INTO tags(name) VALUES (?1)",
                params![name],
            )
            .expect("insert tag");
            conn.execute(
                "INSERT INTO asset_tags(asset_id, tag_id) SELECT ?1, id FROM tags WHERE name = ?2",
                params![id, name],
            )
            .expect("link tag");
        }
    }

    fn finish_derived(conn: &Connection) {
        conn.execute_batch(
            "UPDATE assets SET tag_count = (SELECT COUNT(*) FROM asset_tags WHERE asset_id = assets.id);
             UPDATE assets SET media_group_key_normalized =
               CASE WHEN media_group_key IS NULL OR trim(media_group_key) = '' THEN NULL
                    ELSE lower(trim(media_group_key)) END;",
        )
        .expect("derived columns");
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    /// Hand-written fixture covering every ordering edge case.
    pub fn seed_hand_fixture(conn: &Connection) {
        let null = || Value::Null;
        let rows: Vec<RawRow<'_>> = vec![
            // Ungrouped, including equal modified_at (ID tie-break).
            (1, "image", 100, false, null(), null(), vec!["t1"]),
            (2, "image", 100, true, null(), null(), vec!["t1", "t2"]),
            (3, "video", 300, false, null(), Value::Real(-4.0), vec![]),
            // Blank keys are singletons; their order value is ignored.
            (
                4,
                "image",
                250,
                false,
                text(""),
                Value::Real(9.0),
                vec!["t3"],
            ),
            (5, "gif", 250, true, text("   "), null(), vec![]),
            // g1: mixed orders and a NULL order; newest member is a non-favorite.
            (
                6,
                "image",
                50,
                true,
                text("g1"),
                Value::Real(2.0),
                vec!["t1"],
            ),
            (
                7,
                "image",
                400,
                false,
                text("g1"),
                Value::Real(1.0),
                vec!["t2"],
            ),
            (8, "video", 60, true, text("g1"), null(), vec!["t1", "t3"]),
            (9, "image", 60, false, text("g1"), Value::Real(1.0), vec![]),
            (
                10,
                "image",
                70,
                true,
                text("g1"),
                Value::Integer(-1),
                vec!["t4"],
            ),
            // Case-different key is a separate bucket; normalized group filter joins them.
            (11, "image", 200, true, text("G1"), null(), vec!["t1"]),
            (
                12,
                "image",
                10,
                false,
                text("G1"),
                Value::Real(0.5),
                vec!["t2"],
            ),
            // Untrimmed key forms its own bucket ('group: g2 ' != 'group:g2').
            (
                13,
                "image",
                150,
                true,
                text(" g2 "),
                Value::Real(3.0),
                vec!["t5"],
            ),
            (
                14,
                "video",
                150,
                true,
                text("g2"),
                Value::Real(2.0),
                vec!["t2"],
            ),
            (
                15,
                "image",
                140,
                false,
                text("g2"),
                null(),
                vec!["t2", "t5"],
            ),
            (16, "image", 150, true, text("g2"), Value::Real(2.0), vec![]),
            // BLOB key with the same bytes as TEXT 'same' shares its bucket.
            (
                17,
                "image",
                90,
                false,
                text("same"),
                Value::Real(2.0),
                vec!["t3"],
            ),
            (
                18,
                "gif",
                20,
                true,
                Value::Blob(b"same".to_vec()),
                Value::Real(1.0),
                vec!["t1"],
            ),
            // Group whose bucket time ties an ungrouped asset's time.
            (19, "image", 300, false, text("tie"), null(), vec![]),
            (20, "image", 5, true, text("tie"), null(), vec!["t4"]),
            (21, "image", 300, true, null(), null(), vec!["t2", "t4"]),
        ];
        for row in rows {
            insert_raw(conn, row);
        }
        finish_derived(conn);
    }

    /// Deterministic pseudo-random library with many ties and split groups.
    pub fn seed_random_fixture(conn: &Connection, rows: i64) {
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move |bound: u64| -> u64 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % bound
        };
        let tx = conn.unchecked_transaction().expect("tx");
        for id in 1..=rows {
            let kind = ["image", "image", "image", "video", "gif"][next(5) as usize];
            let modified_at = next(400) as i64 - 20;
            let favorite = next(5) == 0;
            let key = match next(10) {
                0..=4 => Value::Null,
                5 => [text(""), text("  "), text("\t")][next(3) as usize].clone(),
                6 => Value::Blob(format!("b{}", next(20)).into_bytes()),
                7 => text(&format!(" k{} ", next(30))),
                8 => text(&format!("K{}", next(30))),
                _ => text(&format!("k{}", next(30))),
            };
            let order = match next(6) {
                0 | 1 => Value::Null,
                2 => Value::Real(next(5) as f64 + 0.5),
                3 => Value::Integer(next(7) as i64 - 3),
                _ => Value::Real(next(6) as f64),
            };
            let mut tag_names = Vec::new();
            for tag in ["t1", "t2", "t3", "t4", "t5"] {
                if next(4) == 0 {
                    tag_names.push(tag);
                }
            }
            insert_raw(
                &tx,
                (id, kind, modified_at, favorite, key, order, tag_names),
            );
        }
        tx.commit().expect("commit");
        finish_derived(conn);
    }

    #[test]
    fn golden_order_hand_fixture_matches_recorded_ids() {
        let conn = open();
        seed_hand_fixture(&conn);
        // Recorded from the legacy implementation (commit 73989ec).
        let expected: &[(&str, &[i64])] = &[
            (
                "none",
                &[
                    10, 7, 9, 6, 8, 21, 19, 20, 3, 5, 4, 12, 11, 16, 14, 15, 13, 2, 1, 18, 17,
                ],
            ),
            (
                "kind image",
                &[10, 7, 9, 6, 21, 19, 20, 4, 12, 11, 16, 15, 13, 2, 1, 17],
            ),
            ("kind video", &[3, 14, 8]),
            ("kind gif", &[5, 18]),
            ("favorites", &[21, 5, 11, 16, 14, 13, 2, 10, 6, 8, 18, 20]),
            ("and t1", &[11, 2, 1, 6, 8, 18]),
            ("and t1 t2", &[2]),
            ("and missing", &[]),
            ("and t1 missing", &[]),
            (
                "not t3",
                &[
                    10, 7, 9, 6, 21, 19, 20, 3, 5, 12, 11, 16, 14, 15, 13, 2, 1, 18,
                ],
            ),
            (
                "not t3 t4",
                &[7, 9, 6, 19, 3, 5, 12, 11, 16, 14, 15, 13, 2, 1, 18],
            ),
            (
                "not missing",
                &[
                    10, 7, 9, 6, 8, 21, 19, 20, 3, 5, 4, 12, 11, 16, 14, 15, 13, 2, 1, 18, 17,
                ],
            ),
            ("and t1 not t2", &[11, 1, 6, 8, 18]),
            ("no tags", &[19, 3, 5, 16, 9]),
            ("two tags", &[21, 15, 2, 8]),
            ("group g1", &[10, 7, 9, 6, 8, 12, 11]),
            ("group g2 favorites", &[16, 14, 13]),
            ("image favorites not t1", &[21, 16, 13, 10, 20]),
            ("video and t2 not t5", &[14]),
        ];
        for (name, filters) in filter_matrix() {
            let ids = current_ordered_ids(&conn, &filters);
            assert_eq!(
                ids,
                legacy_ordered_ids(&conn, &filters),
                "filter `{name}` differs from the legacy reference"
            );
            let (_, want) = expected
                .iter()
                .find(|(case, _)| *case == name)
                .expect("every filter case has a recorded order");
            assert_eq!(
                &ids, want,
                "filter `{name}` differs from the recorded order"
            );
        }
    }

    #[test]
    fn golden_order_random_fixture_matches_legacy_reference() {
        let conn = open();
        seed_random_fixture(&conn, 5_000);
        for (name, filters) in filter_matrix() {
            assert_eq!(
                current_ordered_ids(&conn, &filters),
                legacy_ordered_ids(&conn, &filters),
                "filter `{name}` differs from the legacy reference"
            );
        }
    }
}

// Sort-key maintenance: every write path keeps the trigger-maintained keys
// equal to a full recomputation, and the order equal to the legacy reference.
mod sort_key_maintenance {
    use super::golden_order::{
        current_ordered_ids, filter_matrix, legacy_ordered_ids, open, seed_random_fixture,
    };
    use media_tagger::{db, models::NewAsset};
    use rusqlite::{params, Connection};

    fn assert_consistent(conn: &Connection, context: &str) {
        assert_eq!(
            db::count_inconsistent_sort_keys(conn).expect("sort key consistency"),
            0,
            "stored sort keys drifted after {context}"
        );
        for (name, filters) in filter_matrix() {
            assert_eq!(
                current_ordered_ids(conn, &filters),
                legacy_ordered_ids(conn, &filters),
                "filter `{name}` differs from the legacy reference after {context}"
            );
        }
    }

    fn scanned(path: &str, modified_at: i64) -> NewAsset {
        NewAsset {
            path: path.to_string(),
            kind: "image".to_string(),
            size_bytes: 1,
            modified_at,
            width: None,
            height: None,
            duration_ms: None,
            thumb_path: None,
        }
    }

    #[test]
    fn random_writes_keep_sort_keys_and_order_exact() {
        let mut conn = open();
        seed_random_fixture(&conn, 1_500);
        db::add_scan_root(&conn, "/synthetic/golden").expect("root");
        assert_consistent(&conn, "seeding");

        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move |bound: u64| -> u64 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % bound
        };
        let keys = [
            None,
            Some(""),
            Some("k1"),
            Some("K1"),
            Some("k2"),
            Some(" k3 "),
            Some("new"),
        ];
        let mut max_id = 1_500u64;
        for step in 1..=600 {
            let id = (next(max_id) + 1) as i64;
            match next(7) {
                0 => {
                    let key = keys[next(keys.len() as u64) as usize];
                    let order = [None, Some(1.0), Some(-2.0), Some(3.5)][next(4) as usize];
                    db::set_asset_media_group(&conn, id, key, order).expect("single group");
                }
                1 => {
                    let key = keys[next(keys.len() as u64) as usize];
                    let updates = (0..next(12) + 1)
                        .map(|index| ((next(max_id) + 1) as i64, Some(index as f64 + 1.0)))
                        // The bulk path reads keys as TEXT; BLOB keys exist only
                        // through ad hoc SQL and are not its concern here.
                        .filter(|(id, _)| {
                            conn.query_row(
                                "SELECT COUNT(*) FROM assets WHERE id = ?1 AND typeof(media_group_key) = 'blob'",
                                [id],
                                |row| row.get::<_, i64>(0),
                            )
                            .unwrap()
                                == 0
                        })
                        .collect::<Vec<_>>();
                    db::set_assets_media_group_bulk(&mut conn, &updates, key).expect("bulk group");
                }
                2 => {
                    // Rescan of an existing file with a new modification time.
                    let path = format!("/synthetic/golden/{id}.bin");
                    let modified_at = next(500) as i64 - 50;
                    db::upsert_scanned_asset(
                        &conn,
                        &scanned(&path, modified_at),
                        modified_at * 1_000_000_000,
                        "/synthetic/golden",
                        step,
                    )
                    .expect("rescan");
                }
                3 => {
                    db::delete_asset_by_id_with_thumb(&conn, id).expect("delete");
                }
                4 => {
                    // A newly discovered file, then grouped like an existing one.
                    max_id += 1;
                    let path = format!("/synthetic/golden/new-{max_id}.bin");
                    db::upsert_scanned_asset(
                        &conn,
                        &scanned(&path, next(500) as i64),
                        1,
                        "/synthetic/golden",
                        step,
                    )
                    .expect("new scan");
                    let new_id = conn.last_insert_rowid();
                    db::set_asset_media_group(&conn, new_id, Some("k2"), None).expect("group new");
                }
                5 => {
                    // Ad hoc SQL, e.g. an older binary writing the same database.
                    conn.execute(
                        "UPDATE assets SET modified_at = ?1, media_group_order = NULL WHERE id = ?2",
                        params![next(500) as i64, id],
                    )
                    .expect("raw update");
                }
                _ => {
                    db::set_asset_favorite(&conn, id, next(2) == 0).expect("favorite");
                }
            }
            if step % 100 == 0 {
                assert_consistent(&conn, &format!("step {step}"));
            }
        }
    }

    #[test]
    fn prefix_delete_and_clear_recompute_buckets() {
        let conn = open();
        for (path, modified_at) in [
            ("/synthetic/a/1.jpg", 10),
            ("/synthetic/a/2.jpg", 20),
            ("/synthetic/b/3.jpg", 90),
            ("/synthetic/b/4.jpg", 5),
        ] {
            db::upsert_asset(&conn, &scanned(path, modified_at)).expect("insert");
        }
        for id in 1..=4 {
            db::set_asset_media_group(&conn, id, Some("trip"), Some(id as f64)).expect("group");
        }
        let bucket = |conn: &Connection| -> Vec<(i64, i64, i64)> {
            conn.prepare("SELECT id, sort_bucket_time, sort_bucket_id FROM assets ORDER BY id")
                .unwrap()
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(
            bucket(&conn),
            vec![(1, 90, 4), (2, 90, 4), (3, 90, 4), (4, 90, 4)]
        );

        db::delete_assets_by_prefix_with_thumbs(&conn, "/synthetic/b").expect("delete prefix");
        assert_eq!(bucket(&conn), vec![(1, 20, 2), (2, 20, 2)]);
        assert_consistent(&conn, "prefix delete");

        db::clear_library_data(&conn).expect("clear");
        assert_eq!(bucket(&conn), vec![]);
    }

    fn sort_key_columns(conn: &Connection) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('assets') WHERE name LIKE 'sort\\_%' ESCAPE '\\'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn schema_objects(conn: &Connection, kind: &str) -> Vec<String> {
        conn.prepare("SELECT name FROM sqlite_schema WHERE type = ?1 ORDER BY name")
            .unwrap()
            .query_map([kind], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn init_schema_backfills_a_version_three_database_and_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("media.db");
        {
            let conn = db::open_connection(&path).expect("open");
            db::init_schema(&conn).expect("schema");
            seed_random_fixture(&conn, 800);
            // Reconstruct a version 3 database: no sort-key columns, triggers,
            // or index, and the former duplicate indexes still present.
            conn.execute_batch(
                "DROP INDEX idx_assets_sort_key;
                 DROP INDEX idx_assets_group_bucket;
                 DROP TRIGGER assets_sort_key_insert_singleton;
                 DROP TRIGGER assets_sort_key_insert_grouped;
                 DROP TRIGGER assets_sort_key_update_singleton;
                 DROP TRIGGER assets_sort_key_update_old_bucket;
                 DROP TRIGGER assets_sort_key_update_new_bucket;
                 DROP TRIGGER assets_sort_key_delete_grouped;
                 ALTER TABLE assets DROP COLUMN sort_bucket_time;
                 ALTER TABLE assets DROP COLUMN sort_bucket_id;
                 ALTER TABLE assets DROP COLUMN sort_group_null_order;
                 CREATE INDEX idx_assets_kind ON assets(kind);
                 CREATE INDEX idx_assets_modified_at ON assets(modified_at DESC);
                 CREATE INDEX idx_tags_name ON tags(name);
                 CREATE INDEX idx_asset_tags_tag ON asset_tags(tag_id);
                 CREATE INDEX idx_asset_tags_asset ON asset_tags(asset_id);
                 UPDATE library_metadata SET value = 3 WHERE key = 'performance_schema_version';",
            )
            .expect("downgrade to version 3");
            assert_eq!(sort_key_columns(&conn), 0);
        }

        let conn = db::open_connection(&path).expect("reopen");
        let legacy = filter_matrix()
            .into_iter()
            .map(|(_, filters)| legacy_ordered_ids(&conn, &filters))
            .collect::<Vec<_>>();
        for round in 0..2 {
            db::init_schema(&conn).expect("migrate");
            assert_eq!(sort_key_columns(&conn), 3, "round {round}");
            assert_eq!(
                conn.query_row(
                    "SELECT value FROM library_metadata WHERE key = 'performance_schema_version'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                4
            );
            assert_eq!(
                schema_objects(&conn, "trigger"),
                db::SORT_KEY_TRIGGERS
                    .iter()
                    .map(|name| name.to_string())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
            );
            let indexes = schema_objects(&conn, "index");
            assert!(indexes.contains(&"idx_assets_sort_key".to_string()));
            for duplicate in [
                "idx_assets_kind",
                "idx_assets_modified_at",
                "idx_tags_name",
                "idx_asset_tags_tag",
                "idx_asset_tags_asset",
            ] {
                assert!(
                    !indexes.contains(&duplicate.to_string()),
                    "{duplicate} kept"
                );
            }
            assert_eq!(db::count_inconsistent_sort_keys(&conn).unwrap(), 0);
            for ((name, filters), expected) in filter_matrix().into_iter().zip(&legacy) {
                assert_eq!(&current_ordered_ids(&conn, &filters), expected, "{name}");
            }
        }
    }
}

mod selection_spec {
    use std::{path::Path, sync::Arc, time::Instant};

    use media_tagger::{
        db,
        models::{NewAsset, SelectionMutationResult, StartAssetQueryResult},
        services::{
            asset_query_service::{AssetQueryFilters, SelectionSpec},
            db_pool::DatabaseRuntime,
            selection_service::{apply_selection_mutation, SelectionMutation},
        },
    };
    use tempfile::tempdir;

    fn no_filters() -> AssetQueryFilters {
        AssetQueryFilters {
            tags_and: vec![],
            tags_not: vec![],
            kind: None,
            favorites_only: false,
            meta_filter: None,
        }
    }

    /// `count` assets; asset `id` is at session index `id - 1` (newest first).
    fn library(dir: &Path, count: i64) -> Arc<DatabaseRuntime> {
        let path = dir.join("media.db");
        let conn = db::open_connection(&path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        for id in 1..=count {
            let asset = NewAsset {
                path: format!("/library/item-{id:03}.jpg"),
                kind: "image".into(),
                size_bytes: 10,
                modified_at: 1_000 - id,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            };
            db::upsert_asset(&conn, &asset).expect("insert");
        }
        DatabaseRuntime::new(path)
    }

    fn session(runtime: &Arc<DatabaseRuntime>, generation: u64) -> u64 {
        let request = runtime.queries.begin_request("selection-test", generation);
        match runtime
            .queries
            .start(
                &runtime.admit().unwrap(),
                no_filters(),
                16,
                request,
                generation,
            )
            .unwrap()
        {
            StartAssetQueryResult::Ready { session_id, .. } => session_id,
            StartAssetQueryResult::Superseded => panic!("superseded"),
        }
    }

    fn tagged(runtime: &Arc<DatabaseRuntime>, tag: &str) -> Vec<i64> {
        let conn = runtime.admit().unwrap().connection().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT at.asset_id FROM asset_tags at JOIN tags t ON t.id = at.tag_id
                 WHERE t.name = ?1 ORDER BY at.asset_id",
            )
            .unwrap();
        stmt.query_map([tag], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn merge(tag: &str) -> SelectionMutation {
        SelectionMutation::MergeTags {
            tags: vec![tag.into()],
        }
    }

    fn processed(result: SelectionMutationResult) -> usize {
        match result {
            SelectionMutationResult::Ready {
                processed_assets, ..
            } => processed_assets,
            SelectionMutationResult::Stale => panic!("unexpected stale"),
        }
    }

    #[test]
    fn spec_resolves_ranges_exclusions_and_inclusions_for_every_mutation() {
        let dir = tempdir().unwrap();
        let runtime = library(dir.path(), 10);
        let session_id = session(&runtime, 1);
        // Indexes 0..=3 and 6..=7 minus asset 3, plus asset 10 (index 9) and a
        // covered asset 1 listed again as included.
        let spec = SelectionSpec {
            session_id,
            ranges: vec![[0, 3], [6, 7]],
            excluded: vec![3],
            included: vec![10, 1],
        };
        {
            // Asset 2 already has the tag: processed, but not updated.
            let conn = runtime.admit().unwrap().connection().unwrap();
            db::set_asset_tags(&conn, 2, &["picked".into(), "other".into()]).unwrap();
        }
        let result = apply_selection_mutation(&runtime, &spec, merge("picked")).unwrap();
        let SelectionMutationResult::Ready {
            processed_assets,
            updated_assets,
            vocabulary_changed,
            is_favorite,
            query_impact,
            ..
        } = result
        else {
            panic!("expected ready");
        };
        assert_eq!((processed_assets, updated_assets), (6, 5));
        assert!(!vocabulary_changed, "the tag already existed");
        assert_eq!(is_favorite, None);
        assert_eq!(
            serde_json::to_value(&query_impact).unwrap(),
            serde_json::json!({"type": "tags", "changed_tags": ["picked"], "tag_count_changed": true})
        );
        assert_eq!(tagged(&runtime, "picked"), vec![1, 2, 4, 7, 8, 10]);
        {
            let conn = runtime.admit().unwrap().connection().unwrap();
            let counts: Vec<(i64, i64)> = conn
                .prepare("SELECT id, tag_count FROM assets WHERE id IN (1, 2, 3) ORDER BY id")
                .unwrap()
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(counts, vec![(1, 1), (2, 2), (3, 0)]);
        }
        // Adding the same tag again changes nothing and bumps no tag revision.
        let again = apply_selection_mutation(&runtime, &spec, merge("picked")).unwrap();
        assert!(matches!(
            again,
            SelectionMutationResult::Ready {
                processed_assets: 6,
                updated_assets: 0,
                ..
            }
        ));

        // A tag write does not change an unfiltered session: the spec stays valid.
        let favorite =
            apply_selection_mutation(&runtime, &spec, SelectionMutation::ToggleFavorite).unwrap();
        assert!(matches!(
            favorite,
            SelectionMutationResult::Ready {
                processed_assets: 6,
                is_favorite: Some(true),
                ..
            }
        ));
        let conn = runtime.admit().unwrap().connection().unwrap();
        let favorites: Vec<i64> = conn
            .prepare("SELECT id FROM assets WHERE is_favorite = 1 ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(favorites, vec![1, 2, 4, 7, 8, 10]);

        let group = apply_selection_mutation(
            &runtime,
            &spec,
            SelectionMutation::SetMediaGroup {
                media_group_key: Some("  Trip ".into()),
            },
        )
        .unwrap();
        assert!(matches!(
            &group,
            SelectionMutationResult::Ready { processed_assets: 6, updated_assets: 6, media_group_key: Some(key), .. } if key == "Trip"
        ));
        // Orders follow the selection: session order, then included IDs.
        let groups: Vec<(i64, f64)> = conn
            .prepare("SELECT id, media_group_order FROM assets WHERE media_group_key = 'Trip' ORDER BY media_group_order")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            groups,
            vec![(1, 1.0), (2, 2.0), (4, 3.0), (7, 4.0), (8, 5.0), (10, 6.0)]
        );
        // Grouping reorders the gallery, so the same spec is now stale.
        assert_eq!(
            apply_selection_mutation(&runtime, &spec, merge("late")).unwrap(),
            SelectionMutationResult::Stale
        );
        assert!(tagged(&runtime, "late").is_empty());
    }

    #[test]
    fn a_changed_or_missing_session_writes_nothing_and_bad_ranges_are_rejected() {
        let dir = tempdir().unwrap();
        let runtime = library(dir.path(), 5);
        let session_id = session(&runtime, 1);
        let all = SelectionSpec {
            session_id,
            ranges: vec![[0, 4]],
            ..SelectionSpec::default()
        };
        {
            let conn = runtime.admit().unwrap().connection().unwrap();
            db::bump_library_revision(&conn).unwrap();
        }
        for mutation in [
            merge("never"),
            SelectionMutation::ToggleFavorite,
            SelectionMutation::SetMediaGroup {
                media_group_key: Some("never".into()),
            },
        ] {
            assert_eq!(
                apply_selection_mutation(&runtime, &all, mutation).unwrap(),
                SelectionMutationResult::Stale
            );
        }
        let conn = runtime.admit().unwrap().connection().unwrap();
        let untouched: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE is_favorite = 1 OR media_group_key IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(untouched, 0);
        assert!(tagged(&runtime, "never").is_empty());
        let missing = SelectionSpec {
            session_id: u64::MAX,
            ..all.clone()
        };
        assert_eq!(
            apply_selection_mutation(&runtime, &missing, merge("never")).unwrap(),
            SelectionMutationResult::Stale
        );

        let fresh = session(&runtime, 2);
        for ranges in [
            vec![[0, 5]],
            vec![[3, 2]],
            vec![[2, 3], [3, 4]],
            vec![[3, 4], [0, 1]],
        ] {
            let spec = SelectionSpec {
                session_id: fresh,
                ranges,
                ..SelectionSpec::default()
            };
            assert!(apply_selection_mutation(&runtime, &spec, merge("never")).is_err());
        }
        let empty_tags = SelectionSpec {
            session_id: fresh,
            ranges: vec![[0, 4]],
            ..SelectionSpec::default()
        };
        assert!(apply_selection_mutation(
            &runtime,
            &empty_tags,
            SelectionMutation::MergeTags { tags: vec![] }
        )
        .is_err());
        assert!(tagged(&runtime, "never").is_empty());
        assert_eq!(
            processed(apply_selection_mutation(&runtime, &empty_tags, merge("now")).unwrap()),
            5
        );
    }

    /// Select-all on 1M synthetic rows: tag merge and favorite toggle through a
    /// spec, each in one transaction. Run in release mode with `--nocapture`.
    #[test]
    #[ignore = "timing diagnostic: run explicitly in release mode"]
    fn one_million_select_all_tag_and_favorite() {
        const ROWS: i64 = 1_000_000;
        let dir = tempdir().unwrap();
        let path = dir.path().join("media.db");
        let conn = db::open_connection(&path).unwrap();
        db::init_schema(&conn).unwrap();
        conn.execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
             INSERT INTO assets(path, file_name, file_name_key, kind, size_bytes, modified_at,
               fingerprint_mtime_ns)
             SELECT '/synthetic/' || i || '.jpg', i || '.jpg', i || '.jpg', 'image', 10,
               1420070400 + i, (1420070400 + i) * 1000000000 FROM n",
            [ROWS],
        )
        .unwrap();
        db::bump_library_revision(&conn).unwrap();
        drop(conn);
        let runtime = DatabaseRuntime::new(path);
        let session_id = session(&runtime, 1);
        let all = SelectionSpec {
            session_id,
            ranges: vec![[0, ROWS as usize - 1]],
            excluded: vec![5, 6, 7],
            ..SelectionSpec::default()
        };
        let started = Instant::now();
        let resolved = runtime.queries.resolve_selection(&all).unwrap().unwrap();
        let resolve_ms = started.elapsed().as_secs_f64() * 1_000.0;
        assert_eq!(resolved.asset_ids.len(), ROWS as usize - 3);
        let started = Instant::now();
        let tagged_count =
            processed(apply_selection_mutation(&runtime, &all, merge("all")).unwrap());
        let tag_ms = started.elapsed().as_secs_f64() * 1_000.0;
        assert_eq!(tagged_count, ROWS as usize - 3);
        let started = Instant::now();
        let favorite_count = processed(
            apply_selection_mutation(&runtime, &all, SelectionMutation::ToggleFavorite).unwrap(),
        );
        let favorite_ms = started.elapsed().as_secs_f64() * 1_000.0;
        assert_eq!(favorite_count, ROWS as usize - 3);
        println!(
            "[selection] rows={ROWS} resolve_ms={resolve_ms:.1} merge_tag_ms={tag_ms:.0} toggle_favorite_ms={favorite_ms:.0}"
        );
    }

    /// `rows` assets with tied and zero modification times, plus existing
    /// groups: `Trip` (members spread over the gallery), `Beach`, a differently
    /// cased `trip`, and one ungrouped asset that kept an order. Statistics
    /// exist, as after the app's `PRAGMA optimize`.
    fn group_library(path: &Path, rows: i64) {
        let mut conn = db::open_connection(path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        conn.execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
             INSERT INTO assets(id, path, file_name, file_name_key, kind, size_bytes,
               modified_at, fingerprint_mtime_ns)
             SELECT i, '/synthetic/' || i || '.jpg', i || '.jpg', i || '.jpg', 'image', 10,
               CASE WHEN i % 97 = 5 THEN 0 ELSE 1000 + (i * 7919) % (?1 / 3 + 1) END, 0
             FROM n",
            [rows],
        )
        .expect("assets");
        for (key, updates) in [
            (
                "Trip",
                (1..=8)
                    .map(|k| (k * rows / 9, Some(k as f64)))
                    .collect::<Vec<_>>(),
            ),
            (
                "Beach",
                vec![(2, Some(1.0)), (3, Some(2.0)), (4, Some(3.0))],
            ),
            ("trip", vec![(rows - 1, Some(1.0)), (rows, Some(2.0))]),
        ] {
            db::set_assets_media_group_bulk(&mut conn, &updates, Some(key)).expect("seed group");
        }
        db::set_asset_media_group(&conn, 7, None, Some(3.0)).expect("order without key");
        conn.execute_batch("ANALYZE;").expect("statistics");
        assert_eq!(db::count_inconsistent_sort_keys(&conn).unwrap(), 0);
    }

    /// Every group and ordering field with its SQLite type, plus the index
    /// definitions and statistics.
    fn group_state(conn: &rusqlite::Connection) -> (Vec<String>, Vec<String>, Vec<String>) {
        let collect = |sql: &str| -> Vec<String> {
            conn.prepare(sql)
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        (
            collect(
                "SELECT id || '|' || quote(media_group_key) || '|' || quote(media_group_order)
                   || '|' || quote(media_group_key_normalized) || '|' || quote(sort_bucket_time)
                   || '|' || quote(sort_bucket_id) || '|' || quote(sort_group_null_order)
                 FROM assets ORDER BY id",
            ),
            collect(
                "SELECT name || ': ' || sql FROM sqlite_schema
                 WHERE type = 'index' AND sql IS NOT NULL ORDER BY name",
            ),
            collect("SELECT tbl || '/' || idx || '=' || stat FROM sqlite_stat1 ORDER BY tbl, idx"),
        )
    }

    fn schema_version(conn: &rusqlite::Connection) -> i64 {
        conn.query_row("PRAGMA schema_version", [], |row| row.get(0))
            .unwrap()
    }

    /// Applies `key` to `spec` through `apply_selection_mutation` on `runtime`
    /// and, as the reference, through the per-asset `set_assets_media_group_bulk`
    /// (orders 1..n by position, as the spec path used to) on an identical
    /// copy; both must leave the same rows and counts, and the spec write must
    /// bump the revision exactly once, only when a row changed. Returns whether
    /// the spec write changed the schema (it rebuilt the group indexes).
    fn assert_group_write_matches_reference(
        runtime: &Arc<DatabaseRuntime>,
        reference: &mut rusqlite::Connection,
        spec: &SelectionSpec,
        key: Option<&str>,
    ) -> bool {
        let ids = runtime
            .queries
            .resolve_selection(spec)
            .unwrap()
            .unwrap()
            .asset_ids;
        let trimmed = key.map(str::trim).filter(|key| !key.is_empty());
        let updates: Vec<(i64, Option<f64>)> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, trimmed.map(|_| (index + 1) as f64)))
            .collect();
        let (expected_processed, expected_updated) =
            db::set_assets_media_group_bulk(reference, &updates, trimmed).unwrap();

        let conn = runtime.admit().unwrap().connection().unwrap();
        let (revision_before, schema_before) = (
            db::current_library_revision(&conn).unwrap(),
            schema_version(&conn),
        );
        drop(conn);
        let result = apply_selection_mutation(
            runtime,
            spec,
            SelectionMutation::SetMediaGroup {
                media_group_key: key.map(str::to_string),
            },
        )
        .unwrap();
        let SelectionMutationResult::Ready {
            processed_assets,
            updated_assets,
            revision,
            media_group_key,
            ..
        } = result
        else {
            panic!("unexpected stale");
        };
        assert_eq!(
            (processed_assets, updated_assets),
            (expected_processed.len(), expected_updated)
        );
        assert_eq!(media_group_key.as_deref(), trimmed);
        let conn = runtime.admit().unwrap().connection().unwrap();
        assert_eq!(group_state(&conn), group_state(reference));
        assert_eq!(db::count_inconsistent_sort_keys(&conn).unwrap(), 0);
        let revision_after = db::current_library_revision(&conn).unwrap();
        assert_eq!(revision, revision_after);
        assert_eq!(
            revision_after - revision_before,
            i64::from(updated_assets > 0)
        );
        schema_version(&conn) != schema_before
    }

    #[test]
    fn ranged_group_writes_match_the_per_asset_path_on_a_mixed_library() {
        let dir = tempdir().unwrap();
        let (path, reference_path) = (dir.path().join("media.db"), dir.path().join("ref.db"));
        group_library(&path, 60);
        group_library(&reference_path, 60);
        let mut reference = db::open_connection(&reference_path).unwrap();
        let runtime = DatabaseRuntime::new(path);
        // Gallery ranges with exclusions, an included asset outside them and a
        // missing included ID (its position is skipped), then a cleared and a
        // trimmed key, a repeat that changes nothing, and a merge of `Beach`.
        for (generation, (ranges, excluded, included, key)) in (1..).zip([
            (
                vec![[0, 9], [20, 44]],
                vec![30, 31],
                vec![999, 1],
                Some("  Trip "),
            ),
            (vec![[0, 59]], vec![], vec![], Some("Trip")),
            (vec![[5, 25]], vec![2], vec![], Some("")),
            (vec![[5, 25]], vec![2], vec![], None),
            (vec![[10, 19]], vec![], vec![998, 3, 997], Some("Beach")),
        ]) {
            let session_id = session(&runtime, generation);
            let spec = SelectionSpec {
                session_id,
                ranges,
                excluded,
                included,
            };
            assert!(
                !assert_group_write_matches_reference(&runtime, &mut reference, &spec, key),
                "a small write maintains the indexes"
            );
        }
    }

    #[test]
    fn a_ranged_group_write_over_most_of_the_library_rebuilds_the_group_indexes() {
        const ROWS: i64 = 24_000;
        let dir = tempdir().unwrap();
        let (path, reference_path) = (dir.path().join("media.db"), dir.path().join("ref.db"));
        group_library(&path, ROWS);
        group_library(&reference_path, ROWS);
        let mut reference = db::open_connection(&reference_path).unwrap();
        let runtime = DatabaseRuntime::new(path);
        for (generation, key) in [(1, Some("Everything")), (2, None)] {
            let spec = SelectionSpec {
                session_id: session(&runtime, generation),
                ranges: vec![[0, ROWS as usize - 1]],
                excluded: vec![7, 8],
                included: vec![],
            };
            assert!(
                assert_group_write_matches_reference(&runtime, &mut reference, &spec, key),
                "more than 20,000 changed rows and an eighth of the library rebuild the indexes"
            );
        }
        // The rebuilt indexes serve the next query; the gallery order matches.
        let session_id = session(&runtime, 3);
        let first: Vec<i64> = runtime
            .queries
            .resolve_selection(&SelectionSpec {
                session_id,
                ranges: vec![[0, 15]],
                ..SelectionSpec::default()
            })
            .unwrap()
            .unwrap()
            .asset_ids;
        let expected: Vec<i64> = reference
            .prepare(
                "SELECT id FROM assets ORDER BY sort_bucket_time DESC, sort_bucket_id DESC,
                   sort_group_null_order, media_group_order, modified_at DESC, id DESC LIMIT 16",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(first, expected);
    }
}

/// Opens the profile database the way startup does and reconciles the file-mutation journal
/// (`app::bootstrap::open_storage`): a fresh connection, `init_schema`, FULL sync, then recovery.
fn restart_and_recover(db_path: &Path) -> (rusqlite::Connection, anyhow::Result<()>) {
    let conn = db::open_connection(db_path).expect("reopen db");
    db::init_schema(&conn).expect("init schema on restart");
    conn.pragma_update(None, "synchronous", "FULL")
        .expect("full sync");
    let recovery = asset_mutation_service::recover_pending_file_operations(&conn);
    (conn, recovery)
}

/// A library with one scanned asset whose delete committed but left its staged file behind
/// (`cleanup_pending`): the asset row is gone, the journal row is committed, and the hidden
/// staging sibling still holds the bytes.
fn library_with_a_committed_delete_awaiting_cleanup(
    db_path: &Path,
    root: &Path,
) -> std::path::PathBuf {
    let folder = root.join("album");
    std::fs::create_dir_all(&folder).expect("create album");
    let original = folder.join("photo.jpg");
    let staging = folder.join(".mediatagger-0123456789abcdefghijklmn.pending");
    std::fs::write(&original, b"source").expect("write source");

    let conn = db::open_connection(db_path).expect("open db");
    db::init_schema(&conn).expect("init schema");
    conn.pragma_update(None, "synchronous", "FULL")
        .expect("full sync");
    let root_text = root.to_string_lossy();
    db::add_scan_root(&conn, &root_text).expect("add root");
    db::upsert_scanned_asset(
        &conn,
        &new_asset(&original, "image", 1, None),
        1,
        &root_text,
        1,
    )
    .expect("index asset");
    let asset = db::get_file_mutation_asset(&conn, 1)
        .expect("asset query")
        .expect("indexed asset");

    db::insert_pending_file_operations(
        &conn,
        &[db::PendingFileOperation {
            operation_id: "interrupted-delete".to_string(),
            asset_id: asset.id,
            action: "delete".to_string(),
            original_path: asset.path.clone(),
            staging_path: staging.to_string_lossy().into_owned(),
            final_path: None,
            committed: false,
        }],
    )
    .expect("journal the delete");
    std::fs::rename(&original, &staging).expect("stage the source");
    db::apply_file_mutations(
        &conn,
        "interrupted-delete",
        &[db::DbFileMutation::Delete {
            asset_id: asset.id,
            expected_path: &asset.path,
            expected_record_version: asset.record_version,
        }],
    )
    .expect("commit the delete");

    let pending = db::list_pending_file_operations(&conn).expect("list journal");
    assert_eq!(pending.len(), 1);
    assert!(pending[0].committed);
    staging
}

#[test]
fn startup_survives_a_committed_delete_whose_scan_root_was_removed() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let root = tmp.path().join("library");
    let staging = library_with_a_committed_delete_awaiting_cleanup(&db_path, &root);
    let root_text = root.to_string_lossy().into_owned();
    {
        let conn = db::open_connection(&db_path).expect("open db");
        db::remove_scan_root_and_orphan_assets(&conn, &root_text).expect("remove root");
        assert!(db::list_scan_roots(&conn).expect("roots").is_empty());
    }

    let (conn, recovery) = restart_and_recover(&db_path);

    recovery.expect("startup must not fail on a cleanup-only journal row");
    assert_eq!(std::fs::read(&staging).expect("staged bytes"), b"source");
    assert_eq!(
        db::list_pending_file_operations(&conn)
            .expect("list journal")
            .len(),
        1
    );
    assert!(db::ensure_no_pending_file_operations(&conn).is_err());
    db::add_scan_root(&conn, &root_text).expect("add the root again");
    drop(conn);

    let (conn, recovery) = restart_and_recover(&db_path);

    recovery.expect("startup with the root registered again");
    assert!(!staging.exists());
    assert!(db::list_pending_file_operations(&conn)
        .expect("list journal")
        .is_empty());
    db::ensure_no_pending_file_operations(&conn).expect("journal is empty");
}

#[test]
fn startup_survives_a_committed_delete_after_clear_library() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let root = tmp.path().join("library");
    let staging = library_with_a_committed_delete_awaiting_cleanup(&db_path, &root);
    {
        let conn = db::open_connection(&db_path).expect("open db");
        let (_, removed_roots, _) = db::clear_library_data(&conn).expect("clear library");
        assert_eq!(removed_roots, 1);
    }

    let (conn, recovery) = restart_and_recover(&db_path);

    recovery.expect("startup must not fail on a cleanup-only journal row");
    assert_eq!(std::fs::read(&staging).expect("staged bytes"), b"source");
    assert_eq!(
        db::list_pending_file_operations(&conn)
            .expect("list journal")
            .len(),
        1
    );
    drop(conn);

    // A later start without the staged file (removed by hand) drops the row.
    std::fs::remove_file(&staging).expect("remove the staged file by hand");
    let (conn, recovery) = restart_and_recover(&db_path);

    recovery.expect("startup after manual cleanup");
    assert!(db::list_pending_file_operations(&conn)
        .expect("list journal")
        .is_empty());
}

#[test]
fn startup_keeps_a_committed_delete_while_its_scan_root_is_unreachable() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let root = tmp.path().join("library");
    let unmounted = tmp.path().join("library-unmounted");
    let staging = library_with_a_committed_delete_awaiting_cleanup(&db_path, &root);
    std::fs::rename(&root, &unmounted).expect("make the root unreachable");

    let (conn, recovery) = restart_and_recover(&db_path);

    recovery.expect("startup must not fail while the root is unreachable");
    assert_eq!(
        db::list_pending_file_operations(&conn)
            .expect("list journal")
            .len(),
        1,
        "the journal row must survive until the staged file can be inspected"
    );
    assert_eq!(db::list_scan_roots(&conn).expect("roots").len(), 1);
    drop(conn);

    std::fs::rename(&unmounted, &root).expect("make the root reachable again");
    assert_eq!(std::fs::read(&staging).expect("staged bytes"), b"source");
    let (conn, recovery) = restart_and_recover(&db_path);

    recovery.expect("startup with the root back");
    assert!(!staging.exists());
    assert!(db::list_pending_file_operations(&conn)
        .expect("list journal")
        .is_empty());
}

#[test]
fn startup_restores_an_interrupted_delete_and_keeps_the_asset_indexed() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("media.db");
    let root = tmp.path().join("library");
    std::fs::create_dir_all(&root).expect("create root");
    let original = root.join("photo.jpg");
    let staging = root.join(".mediatagger-abcdefghijklmn0123456789.pending");
    std::fs::write(&original, b"source").expect("write source");
    let baseline_revision;
    {
        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        let root_text = root.to_string_lossy();
        db::add_scan_root(&conn, &root_text).expect("add root");
        db::upsert_scanned_asset(
            &conn,
            &new_asset(&original, "image", 1, None),
            1,
            &root_text,
            1,
        )
        .expect("index asset");
        db::set_asset_tags(&conn, 1, &["keep".to_string()]).expect("tag asset");
        baseline_revision = db::current_library_revision(&conn).expect("revision");
        db::insert_pending_file_operations(
            &conn,
            &[db::PendingFileOperation {
                operation_id: "interrupted-delete".to_string(),
                asset_id: 1,
                action: "delete".to_string(),
                original_path: original.to_string_lossy().into_owned(),
                staging_path: staging.to_string_lossy().into_owned(),
                final_path: None,
                committed: false,
            }],
        )
        .expect("journal the delete");
        // The process dies after staging and before the database commit.
        std::fs::rename(&original, &staging).expect("stage the source");
    }

    let (conn, recovery) = restart_and_recover(&db_path);

    recovery.expect("startup recovery");
    assert_eq!(
        std::fs::read(&original).expect("restored source"),
        b"source"
    );
    assert!(!staging.exists());
    assert!(db::list_pending_file_operations(&conn)
        .expect("list journal")
        .is_empty());
    let page = support::session_page(&conn, 0, 10, &["keep".to_string()], &[], None, false)
        .expect("query the restored asset");
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].path, original.to_string_lossy());
    assert_eq!(
        db::current_library_revision(&conn).expect("revision"),
        baseline_revision
    );
}

/// Bundle restore over a library whose file-mutation journal is not empty (ADR 0030).
mod restore_with_a_live_file_journal {
    use std::{
        path::Path,
        sync::{
            atomic::{AtomicBool, AtomicU64},
            Mutex, RwLock,
        },
    };

    use media_tagger::{
        app::state::AppState,
        db,
        services::{backup_service, db_pool::DatabaseRuntime, thumb_scheduler::ThumbnailScheduler},
    };
    use tempfile::tempdir;

    fn state(profile: &Path) -> AppState {
        let db_path = profile.join("media.db");
        let thumbs_dir = profile.join("thumbs");
        std::fs::create_dir_all(&thumbs_dir).expect("create thumbs");
        AppState {
            database: DatabaseRuntime::new(db_path.clone()),
            db_path,
            thumbs_dir,
            ffmpeg_path: "ffmpeg".into(),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, "ffmpeg".into()),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: AtomicU64::new(0),
        }
    }

    fn export(state: &AppState, target: &Path) -> Result<(), String> {
        backup_service::export_db_bundle(
            target.to_string_lossy().into_owned(),
            state,
            &state.database.maintenance().expect("maintenance"),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    /// The profile phases of startup (`app::bootstrap::open_storage`) for the profile.
    fn restart(profile: &Path) -> (rusqlite::Connection, anyhow::Result<()>) {
        backup_service::recover_interrupted_restore(profile).expect("restore journal recovery");
        backup_service::remove_abandoned_backup_temporaries(profile);
        super::restart_and_recover(&profile.join("media.db"))
    }

    #[test]
    fn restore_keeps_a_committed_delete_until_a_later_start_can_finish_it() {
        let tmp = tempdir().expect("tempdir");
        // The bundle: another library with its own root and one tagged asset.
        let bundled_root = tmp.path().join("bundled-library");
        std::fs::create_dir_all(&bundled_root).expect("create bundled root");
        let bundled_asset = bundled_root.join("kept.jpg");
        std::fs::write(&bundled_asset, b"kept").expect("write bundled asset");
        let source_profile = tmp.path().join("source-profile");
        let source = state(&source_profile);
        {
            let conn = db::open_connection(&source.db_path).expect("open source db");
            db::init_schema(&conn).expect("init source schema");
            let root_text = bundled_root.to_string_lossy();
            db::add_scan_root(&conn, &root_text).expect("add bundled root");
            db::upsert_scanned_asset(
                &conn,
                &super::new_asset(&bundled_asset, "image", 1, None),
                1,
                &root_text,
                1,
            )
            .expect("index bundled asset");
            db::set_asset_tags(&conn, 1, &["bundled".to_string()]).expect("tag bundled asset");
        }
        let archive = tmp.path().join("bundle.zip");
        export(&source, &archive).expect("export the bundle");

        // The live library: a committed delete whose staged file is still on disk.
        let profile = tmp.path().join("profile");
        let live = state(&profile);
        let root = tmp.path().join("library");
        let staging = super::library_with_a_committed_delete_awaiting_cleanup(&live.db_path, &root);
        let journal_before = {
            let conn = db::open_connection(&live.db_path).expect("open live db");
            db::list_pending_file_operations(&conn).expect("list journal")
        };
        let blocked_export = tmp.path().join("blocked.zip");
        assert!(export(&live, &blocked_export).is_err());

        backup_service::import_db_bundle(
            archive.to_string_lossy().into_owned(),
            Vec::new(),
            &live,
            &live.database.maintenance().expect("maintenance"),
        )
        .expect("a retained journal row must not block restore");

        // The restored library does not know the journaled root or asset.
        let (conn, recovery) = restart(&profile);
        recovery.expect("startup after restore must not fail");
        assert_eq!(
            db::list_scan_roots(&conn).expect("roots"),
            vec![bundled_root.to_string_lossy().into_owned()]
        );
        let page =
            super::support::session_page(&conn, 0, 10, &["bundled".to_string()], &[], None, false)
                .expect("query the restored asset");
        assert_eq!(page.total, 1);
        assert_eq!(
            db::list_pending_file_operations(&conn).expect("list journal"),
            journal_before,
            "the row stays while no registered root covers the staged file"
        );
        assert_eq!(std::fs::read(&staging).expect("staged bytes"), b"source");
        drop(conn);
        let error = export(&live, &blocked_export).expect_err("export stays rejected");
        assert!(error.contains("pending local file operations"), "{error}");

        // Registering the root in the restored library lets a later start finish.
        {
            let conn = db::open_connection(&live.db_path).expect("open restored db");
            db::add_scan_root(&conn, &root.to_string_lossy()).expect("add the root again");
        }
        let (conn, recovery) = restart(&profile);
        recovery.expect("startup with the root registered");
        assert!(!staging.exists());
        db::ensure_no_pending_file_operations(&conn).expect("journal is empty");
        drop(conn);
        export(&live, &blocked_export).expect("export works again");
    }
}

/// Full scan -> prune workflow with a supported file whose name is not valid
/// UTF-8: discovery skips it with a warning, the root stays complete, and the
/// row of a file deleted outside the app is pruned on the next scan.
#[cfg(unix)]
mod scan_prune_with_non_utf8_names {
    use std::{
        ffi::OsString,
        os::unix::ffi::OsStringExt,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, AtomicU64},
            Mutex, RwLock,
        },
    };

    use media_tagger::{
        app::state::AppState,
        db,
        models::{ScanCompletion, ScanSummary},
        services::{db_pool::DatabaseRuntime, scan_service, thumb_scheduler::ThumbnailScheduler},
    };
    use tempfile::tempdir;

    fn state(dir: &Path) -> AppState {
        let db_path = dir.join("media.db");
        AppState {
            database: DatabaseRuntime::new(db_path.clone()),
            db_path,
            thumbs_dir: dir.join("thumbs"),
            ffmpeg_path: "ffmpeg".into(),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, "ffmpeg".into()),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: AtomicU64::new(0),
        }
    }

    fn scan(state: &AppState, root: &Path) -> ScanSummary {
        let permit = state.database.admit().expect("admit");
        scan_service::scan_roots(
            &[root.to_str().expect("UTF-8 root").to_string()],
            state,
            &permit,
            &|_| Ok(()),
        )
        .expect("scan")
    }

    fn asset_paths(state: &AppState) -> Vec<String> {
        let conn = state
            .database
            .admit()
            .expect("admit")
            .connection()
            .expect("connection");
        let mut stmt = conn
            .prepare("SELECT path FROM assets ORDER BY path")
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query");
        rows.map(|row| row.expect("row")).collect()
    }

    fn tag_names(state: &AppState) -> Vec<String> {
        let conn = state
            .database
            .admit()
            .expect("admit")
            .connection()
            .expect("connection");
        let mut stmt = conn
            .prepare("SELECT name FROM tags ORDER BY name")
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query");
        rows.map(|row| row.expect("row")).collect()
    }

    fn non_utf8(root: &Path) -> PathBuf {
        // "caf" + Latin-1 0xE9 + ".jpg": a byte sequence that is not UTF-8.
        root.join(OsString::from_vec(b"caf\xE9.jpg".to_vec()))
    }

    #[test]
    fn complete_scan_prunes_rows_of_deleted_files_next_to_a_non_utf8_name() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join("media");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join("kept.jpg"), b"kept").expect("kept");
        std::fs::write(root.join("deleted.jpg"), b"deleted").expect("deleted");
        std::fs::write(non_utf8(&root), b"latin-1 name").expect("non-UTF-8 file");
        let state = state(dir.path());
        {
            let conn = state
                .database
                .admit()
                .expect("admit")
                .connection()
                .expect("connection");
            db::init_schema(&conn).expect("schema");
            db::add_scan_root(&conn, root.to_str().expect("UTF-8 root")).expect("add root");
        }
        let root_text = root.to_string_lossy().to_string();
        let kept = format!("{root_text}/kept.jpg");
        let deleted = format!("{root_text}/deleted.jpg");

        let first = scan(&state, &root);
        assert!(matches!(first.completion, ScanCompletion::Complete));
        assert_eq!((first.indexed, first.removed, first.failed), (2, 0, 0));
        assert_eq!(asset_paths(&state), vec![deleted.clone(), kept.clone()]);

        {
            let conn = state
                .database
                .admit()
                .expect("admit")
                .connection()
                .expect("connection");
            for (path, tag) in [(&kept, "keep"), (&deleted, "gone")] {
                let id: i64 = conn
                    .query_row("SELECT id FROM assets WHERE path = ?1", [path], |row| {
                        row.get(0)
                    })
                    .expect("asset id");
                db::set_asset_tags(&conn, id, &[tag.to_string()]).expect("tag");
            }
        }
        std::fs::remove_file(root.join("deleted.jpg")).expect("delete outside the app");

        // The non-UTF-8 file is still there, and the root is still complete,
        // so the stale row and its now-orphaned tag are pruned.
        let second = scan(&state, &root);
        assert!(matches!(second.completion, ScanCompletion::Complete));
        assert_eq!((second.indexed, second.removed, second.failed), (1, 1, 0));
        assert_eq!(asset_paths(&state), vec![kept.clone()]);
        assert_eq!(tag_names(&state), vec!["keep".to_string()]);

        // An unchanged rescan stays complete and removes nothing. With no
        // stale mapping the prune skips the library-wide orphan sweeps, so an
        // unrelated row without any mapping is left alone.
        {
            let conn = state
                .database
                .admit()
                .expect("admit")
                .connection()
                .expect("connection");
            db::upsert_asset(
                &conn,
                &media_tagger::models::NewAsset {
                    path: "/elsewhere/unmapped.jpg".to_string(),
                    kind: "image".to_string(),
                    size_bytes: 1,
                    modified_at: 1,
                    width: None,
                    height: None,
                    duration_ms: None,
                    thumb_path: None,
                },
            )
            .expect("unmapped asset");
        }
        let third = scan(&state, &root);
        assert!(matches!(third.completion, ScanCompletion::Complete));
        assert_eq!((third.indexed, third.removed, third.failed), (1, 0, 0));
        let mut expected = vec!["/elsewhere/unmapped.jpg".to_string(), kept];
        expected.sort();
        assert_eq!(asset_paths(&state), expected);
        assert_eq!(tag_names(&state), vec!["keep".to_string()]);
    }
}

/// Unfiltered timeline sections while a scan commits batches between its
/// throttled revision bumps (ADR 0002, ADR 0027).
mod sections_during_a_scan {
    use std::{
        path::Path,
        sync::{
            atomic::{AtomicBool, AtomicU64},
            Mutex, RwLock,
        },
    };

    use media_tagger::{
        app::state::AppState,
        db,
        models::{
            AssetQueryPageResult, AssetQuerySectionsResult, NewAsset, ScanCompletion,
            StartAssetQueryResult,
        },
        services::{
            asset_query_service::AssetQueryFilters, db_pool::DatabaseRuntime, scan_service,
            thumb_scheduler::ThumbnailScheduler,
        },
    };
    use tempfile::tempdir;

    fn state(dir: &Path) -> AppState {
        let db_path = dir.join("media.db");
        AppState {
            database: DatabaseRuntime::new(db_path.clone()),
            db_path,
            thumbs_dir: dir.join("thumbs"),
            ffmpeg_path: "ffmpeg".into(),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, "ffmpeg".into()),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: AtomicU64::new(0),
        }
    }

    fn scan(state: &AppState, root: &str) {
        let permit = state.database.admit().expect("admit");
        let summary = scan_service::scan_roots(&[root.to_string()], state, &permit, &|_| Ok(()))
            .expect("scan");
        assert!(matches!(summary.completion, ScanCompletion::Complete));
    }

    fn start(state: &AppState, generation: u64) -> (u64, usize) {
        let queries = &state.database.queries;
        let request = queries.begin_request("sections-scan-test", generation);
        let filters = AssetQueryFilters {
            tags_and: vec![],
            tags_not: vec![],
            kind: None,
            favorites_only: false,
            meta_filter: None,
        };
        match queries
            .start(
                &state.database.admit().expect("admit"),
                filters,
                16,
                request,
                generation,
            )
            .expect("start")
        {
            StartAssetQueryResult::Ready {
                session_id, total, ..
            } => (session_id, total),
            StartAssetQueryResult::Superseded => panic!("superseded"),
        }
    }

    fn sections(state: &AppState, session_id: u64) -> AssetQuerySectionsResult {
        state
            .database
            .queries
            .sections(&state.database.admit().expect("admit"), session_id)
            .expect("sections")
    }

    fn page_total(state: &AppState, session_id: u64) -> Option<usize> {
        match state
            .database
            .queries
            .page(&state.database.admit().expect("admit"), session_id, 0, 16)
            .expect("page")
        {
            AssetQueryPageResult::Ready { total, .. } => Some(total),
            AssetQueryPageResult::Stale => None,
        }
    }

    #[test]
    fn a_session_survives_an_unbumped_scan_batch_and_gets_sections_after_the_scan_ends() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join("media");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join("first.jpg"), b"first").expect("first");
        std::fs::write(root.join("second.jpg"), b"second").expect("second");
        let state = state(dir.path());
        let root_text = root.to_str().expect("UTF-8 root").to_string();
        {
            let conn = state
                .database
                .admit()
                .expect("admit")
                .connection()
                .expect("connection");
            db::init_schema(&conn).expect("schema");
            db::add_scan_root(&conn, &root_text).expect("add root");
        }
        scan(&state, &root_text);
        let (session_id, total) = start(&state, 1);
        assert_eq!(total, 2);

        // A rescan finds a new file and commits its batch without a bump,
        // as `flush_scan_batch` does between two throttled bumps.
        std::fs::write(root.join("third.jpg"), b"third").expect("third");
        {
            let conn = state
                .database
                .admit()
                .expect("admit")
                .connection()
                .expect("connection");
            let revision = db::current_library_revision(&conn).expect("revision");
            let tx = rusqlite::Transaction::new_unchecked(
                &conn,
                rusqlite::TransactionBehavior::Immediate,
            )
            .expect("begin");
            db::upsert_scanned_asset(
                &tx,
                &NewAsset {
                    path: format!("{root_text}/third.jpg"),
                    kind: "image".into(),
                    size_bytes: 5,
                    modified_at: 1_700_000_000,
                    width: None,
                    height: None,
                    duration_ms: None,
                    thumb_path: None,
                },
                1,
                &root_text,
                1,
            )
            .expect("batch row");
            tx.commit().expect("commit");
            assert_eq!(
                db::current_library_revision(&conn).expect("revision"),
                revision
            );
        }

        // The gallery's session keeps serving its snapshot; only the
        // timeline is unavailable for it.
        assert_eq!(
            sections(&state, session_id),
            AssetQuerySectionsResult::Ready {
                session_id,
                total: 2,
                sections: vec![],
            }
        );
        assert_eq!(page_total(&state, session_id), Some(2));
        assert_eq!(start(&state, 2), (session_id, 2));

        // The real scan bumps when it ends: the old session goes stale and the
        // new one has sections covering every row.
        scan(&state, &root_text);
        assert_eq!(page_total(&state, session_id), None);
        assert_eq!(
            sections(&state, session_id),
            AssetQuerySectionsResult::Stale
        );
        let (fresh, total) = start(&state, 3);
        assert_ne!(fresh, session_id);
        assert_eq!(total, 3);
        let AssetQuerySectionsResult::Ready {
            session_id: returned,
            total,
            sections,
        } = sections(&state, fresh)
        else {
            panic!("ready sections after the scan");
        };
        assert_eq!((returned, total), (fresh, 3));
        assert!(!sections.is_empty());
        assert_eq!(sections[0].start, 0);
        assert_eq!(
            sections.iter().map(|section| section.count).sum::<usize>(),
            3
        );
    }
}
