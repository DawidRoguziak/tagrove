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

    assert_eq!(processed, vec![2, 1]);
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
