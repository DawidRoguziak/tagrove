//! Repeatable diagnostic, deliberately without timing assertions.
use media_tagger_core::db;
use rusqlite::params;
use std::time::Instant;

#[test]
#[ignore = "diagnostic benchmark: run with --ignored --nocapture"]
fn tag_save_benchmark() {
    println!("vocabulary,tags_per_asset,assignment_copies,assignments,single_noop_us,single_replace_us,bulk_noop_us,bulk_add_us");
    for vocabulary in [1_000, 10_000, 50_000] {
        for tag_count in [1, 15, 30] {
            for copies in [1, 4, 16] {
                let dir = tempfile::tempdir().unwrap();
                let mut conn = db::open_connection(&dir.path().join("benchmark.db")).unwrap();
                db::init_schema(&conn).unwrap();
                let tx = conn.transaction().unwrap();
                for id in 1..=copies + 10 {
                    tx.execute("INSERT INTO assets(id,path,kind,size_bytes,modified_at) VALUES (?1,?2,'image',1,1)", params![id, format!("/synthetic/{id}.png")]).unwrap();
                }
                for id in 1..=vocabulary {
                    tx.execute(
                        "INSERT INTO tags(id,name) VALUES (?1,?2)",
                        params![id, format!("tag-{id:05}")],
                    )
                    .unwrap();
                }
                for id in 1..=copies {
                    tx.execute("INSERT INTO asset_tags SELECT ?1,id FROM tags", [id])
                        .unwrap();
                }
                let targets: Vec<i64> = (copies + 1..=copies + 10).collect();
                for id in &targets {
                    tx.execute(
                        "INSERT INTO asset_tags SELECT ?1,id FROM tags WHERE id <= ?2",
                        params![id, tag_count],
                    )
                    .unwrap();
                }
                tx.execute("UPDATE assets SET tag_count=(SELECT COUNT(*) FROM asset_tags WHERE asset_id=assets.id)", []).unwrap();
                tx.commit().unwrap();
                let original: Vec<String> =
                    (1..=tag_count).map(|id| format!("tag-{id:05}")).collect();
                let mut replacement = original.clone();
                replacement[0] = format!("tag-{:05}", tag_count + 1);
                let measure = |conn: &mut rusqlite::Connection, mode: usize| {
                    let start = Instant::now();
                    for iteration in 0..12 {
                        match mode {
                            0 => {
                                db::set_asset_tags_with_revision(conn, targets[0], &original)
                                    .unwrap();
                            }
                            1 => {
                                db::set_asset_tags_with_revision(
                                    conn,
                                    targets[0],
                                    if iteration % 2 == 0 {
                                        &replacement
                                    } else {
                                        &original
                                    },
                                )
                                .unwrap();
                            }
                            2 => {
                                db::merge_asset_tags_bulk_with_revision(conn, &targets, &original)
                                    .unwrap();
                            }
                            _ => {
                                db::merge_asset_tags_bulk_with_revision(
                                    conn,
                                    &targets,
                                    &[format!("tag-{:05}", tag_count + iteration + 2)],
                                )
                                .unwrap();
                            }
                        }
                    }
                    start.elapsed().as_micros() / 12
                };
                let timings: Vec<_> = (0..4).map(|mode| measure(&mut conn, mode)).collect();
                println!(
                    "{vocabulary},{tag_count},{copies},{},{},{},{},{}",
                    vocabulary * copies + 10 * tag_count,
                    timings[0],
                    timings[1],
                    timings[2],
                    timings[3]
                );
            }
        }
    }
}
