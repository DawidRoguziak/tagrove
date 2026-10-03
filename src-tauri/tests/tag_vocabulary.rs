use media_tagger::{db, models::TagQueryImpact};
use rusqlite::{params, Connection};

fn fixture() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open_connection(&dir.path().join("tags.db")).unwrap();
    db::init_schema(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO assets(id,path,kind,size_bytes,modified_at) VALUES
        (1,'/synthetic/one.png','image',1,1), (2,'/synthetic/two.png','image',1,1);",
    )
    .unwrap();
    (dir, conn)
}

fn names(conn: &Connection) -> Vec<String> {
    conn.prepare("SELECT name FROM tags ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn vocabulary_changes_only_when_stored_names_change() {
    let (_dir, mut conn) = fixture();
    conn.execute("INSERT INTO tags(name) VALUES ('unrelated-orphan')", [])
        .unwrap();
    let first = db::set_asset_tags_with_revision(&mut conn, 1, &["cat".into()]).unwrap();
    assert!(first.changed && first.vocabulary_changed);
    let shared = db::set_asset_tags_with_revision(&mut conn, 2, &["cat".into()]).unwrap();
    assert!(shared.changed && !shared.vocabulary_changed);
    assert_eq!(shared.revision, first.revision + 1);
    let noop = db::set_asset_tags_with_revision(&mut conn, 1, &[" CAT ".into()]).unwrap();
    assert!(!noop.changed && !noop.vocabulary_changed);
    assert_eq!(noop.revision, shared.revision);
    assert_eq!(noop.query_impact, TagQueryImpact::None);
    let removed_shared = db::set_asset_tags_with_revision(&mut conn, 1, &[]).unwrap();
    assert!(removed_shared.changed && !removed_shared.vocabulary_changed);
    let removed_last = db::set_asset_tags_with_revision(&mut conn, 2, &[]).unwrap();
    assert!(removed_last.changed && removed_last.vocabulary_changed);
    assert_eq!(removed_last.revision, shared.revision + 2);
    assert_eq!(names(&conn), vec!["unrelated-orphan"]);
    assert_eq!(
        serde_json::to_value(removed_last).unwrap()["vocabulary_changed"],
        true
    );
}

#[test]
fn retained_ids_and_count_repairs_do_not_change_vocabulary() {
    let (_dir, mut conn) = fixture();
    db::set_asset_tags_with_revision(&mut conn, 1, &["cat".into()]).unwrap();
    db::set_asset_tags_with_revision(&mut conn, 2, &["dog".into()]).unwrap();
    let added_existing =
        db::set_asset_tags_with_revision(&mut conn, 1, &["cat".into(), "dog".into()]).unwrap();
    assert!(!added_existing.vocabulary_changed);
    conn.execute("UPDATE assets SET tag_count=99 WHERE id=1", [])
        .unwrap();
    let repair =
        db::set_asset_tags_with_revision(&mut conn, 1, &["cat".into(), "dog".into()]).unwrap();
    assert!(repair.changed && !repair.vocabulary_changed);
    assert_eq!(repair.query_impact, TagQueryImpact::All);
    assert_eq!(repair.revision, added_existing.revision + 1);
    assert_eq!(names(&conn), vec!["cat", "dog"]);
}

#[test]
fn canonicalization_reports_renames_merges_and_cleans_surviving_candidate_ids() {
    for collision in [false, true] {
        let (_dir, mut conn) = fixture();
        conn.execute_batch(
            "INSERT INTO tags(id,name) VALUES (1,' Cat ');
            INSERT INTO asset_tags VALUES (1,1),(2,1);
            UPDATE assets SET tag_count=1;",
        )
        .unwrap();
        if collision {
            conn.execute_batch(
                "INSERT INTO tags(id,name) VALUES (2,'cat');
                INSERT INTO asset_tags VALUES (1,2);
                UPDATE assets SET tag_count=2 WHERE id=1;",
            )
            .unwrap();
        }
        let repaired = db::set_asset_tags_with_revision(&mut conn, 1, &["cat".into()]).unwrap();
        assert!(repaired.vocabulary_changed);
        assert_eq!(repaired.query_impact, TagQueryImpact::All);
        assert_eq!(names(&conn), vec!["cat"]);
        assert_eq!(db::list_asset_tags(&conn, 2).unwrap(), vec!["cat"]);
        assert_eq!(
            conn.query_row("SELECT tag_count FROM assets WHERE id=2", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    let (_dir, mut conn) = fixture();
    conn.execute_batch(
        "INSERT INTO tags(id,name) VALUES (1,' Cat '),(2,'cat');
        INSERT INTO asset_tags VALUES (1,1),(1,2); UPDATE assets SET tag_count=2 WHERE id=1;",
    )
    .unwrap();
    let removed = db::set_asset_tags_with_revision(&mut conn, 1, &[]).unwrap();
    assert!(removed.vocabulary_changed);
    assert!(names(&conn).is_empty());
}

#[test]
fn bulk_ors_vocabulary_flags_and_empty_or_missing_inputs_preserve_revision() {
    let (_dir, mut conn) = fixture();
    let (_, first_revision, _, added) =
        db::merge_asset_tags_bulk_with_revision(&mut conn, &[1, 999, 2], &["cat".into()]).unwrap();
    assert!(added); // The second asset's unchanged vocabulary must not overwrite true.
    let (results, revision, impact, changed) =
        db::merge_asset_tags_bulk_with_revision(&mut conn, &[1, 2], &["cat".into()]).unwrap();
    assert!(!changed && results.iter().all(|r| !r.changed));
    assert_eq!(revision, first_revision);
    assert_eq!(impact, TagQueryImpact::None);
    for (ids, tags) in [
        (vec![], vec!["cat".into()]),
        (vec![1], vec![]),
        (vec![999], vec!["new".into()]),
    ] {
        let (results, revision, impact, changed) =
            db::merge_asset_tags_bulk_with_revision(&mut conn, &ids, &tags).unwrap();
        assert!(results.is_empty() && !changed);
        assert_eq!(revision, first_revision);
        assert_eq!(impact, TagQueryImpact::None);
    }
    assert!(db::set_asset_tags_with_revision(&mut conn, 999, &["new".into()]).is_err());
    assert_eq!(names(&conn), vec!["cat"]);
}

#[test]
fn failed_transaction_rolls_back_vocabulary_cleanup_mappings_and_revision() {
    for bulk in [false, true] {
        let (_dir, mut conn) = fixture();
        let initial = db::set_asset_tags_with_revision(&mut conn, 1, &["old".into()]).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_revision BEFORE UPDATE ON library_metadata
            WHEN OLD.key='revision' BEGIN SELECT RAISE(ABORT, 'rollback proof'); END;",
        )
        .unwrap();
        if bulk {
            assert!(
                db::merge_asset_tags_bulk_with_revision(&mut conn, &[1, 2], &["new".into()])
                    .is_err()
            );
        } else {
            assert!(db::set_asset_tags_with_revision(&mut conn, 1, &["new".into()]).is_err());
        }
        assert_eq!(names(&conn), vec!["old"]);
        assert_eq!(db::list_asset_tags(&conn, 1).unwrap(), vec!["old"]);
        assert!(db::list_asset_tags(&conn, 2).unwrap().is_empty());
        assert_eq!(
            db::current_library_revision(&conn).unwrap(),
            initial.revision
        );
        assert!(conn.is_autocommit());
    }
}

#[test]
fn tag_indexes_are_idempotent_and_used_on_new_and_existing_databases() {
    let (_dir, conn) = fixture();
    for id in 0..1_000 {
        conn.execute(
            "INSERT INTO tags(name) VALUES (?1)",
            [format!("fixture-{id}")],
        )
        .unwrap();
    }
    for existing in [false, true] {
        if existing {
            conn.execute_batch(
                "DROP INDEX idx_tags_normalized_name;
                INSERT INTO tags(name) VALUES (' Cat '),('cat');",
            )
            .unwrap();
        }
        db::init_schema(&conn).unwrap();
        db::init_schema(&conn).unwrap();
        assert_eq!(conn.query_row("SELECT count(*) FROM pragma_index_list('tags') WHERE name='idx_tags_normalized_name' AND \"unique\"=0", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
        for (sql, required) in [
            ("SELECT id,name FROM tags WHERE lower(trim(name))=?1 ORDER BY CASE WHEN name=?1 COLLATE BINARY THEN 0 ELSE 1 END,id", "idx_tags_normalized_name"),
            ("DELETE FROM tags WHERE id=?1 AND NOT EXISTS (SELECT 1 FROM asset_tags WHERE tag_id=?1)", "idx_asset_tags_tag"),
        ] {
            let plan: Vec<String> = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap()
                .query_map(params![1], |row| row.get(3)).unwrap().collect::<Result<_, _>>().unwrap();
            assert!(plan.iter().any(|step| step.contains(required)), "{plan:?}");
            assert!(plan.iter().all(|step| !step.starts_with("SCAN ")), "{plan:?}");
            if sql.starts_with("DELETE") {
                assert!(plan.iter().any(|step| step.contains("SEARCH tags USING INTEGER PRIMARY KEY")), "{plan:?}");
            }
        }
        if existing {
            assert_eq!(names(&conn).len(), 1_002);
        }
    }
}
