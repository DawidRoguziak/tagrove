//! Test view of a query session for integration tests: the ordered session
//! vector (the gallery's own path), cut to one page and read through
//! `get_asset_details`. It replaced the removed `list_assets` OFFSET query
//! (grid-performance stage 10). Unit tests use `db::test_support`.

use media_tagger::db;
use rusqlite::Connection;

// Shared by several test binaries; each reads only some fields.
#[allow(dead_code)]
pub struct SessionAsset {
    pub id: i64,
    pub path: String,
    pub media_group_key: Option<String>,
    pub media_group_order: Option<f64>,
}

pub struct SessionPage {
    pub items: Vec<SessionAsset>,
    pub total: usize,
}

pub fn session_page(
    conn: &Connection,
    offset: usize,
    limit: usize,
    tags_and: &[String],
    tags_not: &[String],
    kind: Option<&str>,
    favorites_only: bool,
) -> anyhow::Result<SessionPage> {
    let ids =
        db::list_ordered_asset_ids_with_meta(conn, tags_and, tags_not, kind, favorites_only, None)?;
    let items = ids
        .iter()
        .skip(offset)
        .take(limit)
        .map(|id| {
            let details = db::get_asset_details(conn, *id)?
                .ok_or_else(|| anyhow::anyhow!("asset {id} has no details"))?;
            Ok(SessionAsset {
                id: details.summary.id,
                path: details.path,
                media_group_key: details.summary.media_group_key,
                media_group_order: details.summary.media_group_order,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(SessionPage {
        items,
        total: ids.len(),
    })
}
