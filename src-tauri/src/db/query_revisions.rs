use super::*;

const PREFIX: &str = "query_revision:";

/// Called only during isolated startup/staging, when no sessions can survive.
pub(super) fn rebuild_query_revisions(conn: &Connection) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM library_metadata WHERE substr(key, 1, ?1) = ?2",
        params![PREFIX.len(), PREFIX],
    )?;
    let revision = current_library_revision(&tx)?;
    for name in ["general", "favorites", "tag_count"] {
        write_query_revision(&tx, name, revision)?;
    }
    tx.commit()?;
    Ok(())
}

fn write_query_revision(
    tx: &rusqlite::Transaction<'_>,
    name: &str,
    revision: i64,
) -> anyhow::Result<()> {
    tx.execute("INSERT INTO library_metadata(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![format!("{PREFIX}{name}"), revision])?;
    Ok(())
}

pub(super) fn bump_scoped_revision(
    tx: &rusqlite::Transaction<'_>,
    names: &[String],
) -> anyhow::Result<i64> {
    tx.execute(
        "UPDATE library_metadata SET value = value + 1 WHERE key = 'revision'",
        [],
    )?;
    let revision = current_library_revision(tx)?;
    for name in names {
        write_query_revision(tx, name, revision)?;
    }
    Ok(revision)
}

pub(super) fn bump_tag_revision(
    tx: &rusqlite::Transaction<'_>,
    impact: &TagQueryImpact,
) -> anyhow::Result<i64> {
    match impact {
        TagQueryImpact::None => current_library_revision(tx),
        TagQueryImpact::All => bump_library_revision_in_tx(tx),
        TagQueryImpact::Tags {
            changed_tags,
            tag_count_changed,
        } => {
            let mut names: Vec<_> = changed_tags
                .iter()
                .map(|tag| format!("tag:{tag}"))
                .collect();
            if *tag_count_changed {
                names.push("tag_count".into());
            }
            bump_scoped_revision(tx, &names)
        }
    }
}

/// Exact keys, bounded below SQLite's parameter limit. Missing tag markers are zero.
/// The caller holds the same read transaction used for rows and global revision.
pub fn query_dependency_revision(
    conn: &Connection,
    dependencies: &[String],
) -> anyhow::Result<i64> {
    let mut revision = 0;
    for batch in dependencies.chunks(500) {
        let placeholders = vec!["?"; batch.len()].join(",");
        let value: i64 = conn.query_row(&format!("SELECT COALESCE(MAX(value), 0) FROM library_metadata WHERE key IN ({placeholders})"), rusqlite::params_from_iter(batch.iter().map(|name| format!("{PREFIX}{name}"))), |row| row.get(0))?;
        revision = revision.max(value);
    }
    Ok(revision)
}
