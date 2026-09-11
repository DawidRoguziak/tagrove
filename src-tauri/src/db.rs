mod connections;
pub use connections::*;
mod schema;
pub use schema::*;
mod queries;
pub use queries::*;
mod query_revisions;
pub use query_revisions::*;
mod mutations;
pub use mutations::*;
mod scan_membership;
pub use scan_membership::*;
mod thumbnails;
pub use thumbnails::*;

use std::{collections::HashMap, path::Path, thread, time::Duration};

use anyhow::Context;
use rusqlite::{params, Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior};

use crate::models::{
    Asset, AssetDetails, AssetPage, AssetSummary, AssetTagResult, DuplicateAsset, DuplicateGroup,
    NewAsset, SetAssetTagsSummary, TagListPage, TagQueryImpact, ThumbnailAsset,
};
use crate::utils::{
    tags::{merge_tags, normalize_and_validate_tags, normalize_tags, parse_legacy_tags},
    text::canonical_key,
};

pub struct CsvAssetRow {
    pub path: String,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub media_group_key: Option<String>,
    pub media_group_order: Option<f64>,
}

pub struct AssetPathRow {
    pub id: i64,
    pub path: String,
}

pub struct VideoAssetSource {
    pub path: String,
    pub kind: String,
    pub scan_roots: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CsvImportRecord {
    pub file_name_key: String,
    pub tags: Vec<String>,
    pub favorite: Option<bool>,
    #[serde(with = "csv_optional_update")]
    pub media_group_key: Option<Option<String>>,
    #[serde(with = "csv_optional_update")]
    pub media_group_order: Option<Option<f64>>,
}

mod csv_optional_update {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<T: Serialize, S: Serializer>(
        value: &Option<Option<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        (value.is_some(), value.as_ref().and_then(Option::as_ref)).serialize(serializer)
    }
    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        let (present, value) = <(bool, Option<T>)>::deserialize(deserializer)?;
        Ok(present.then_some(value))
    }
}

pub const APPLICATION_ID: i64 = 0x4d54_4147;
pub const SCHEMA_VERSION: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupSchemaCompatibility {
    Current,
    Version1,
    Version2,
    Legacy,
}

#[derive(Debug, Clone)]
pub struct BackupAssetPath {
    pub id: i64,
    pub path: String,
    pub thumb_path: Option<String>,
}

pub struct DuplicateFileNameCount {
    pub file_name_key: String,
    pub file_name_display: String,
    pub asset_count: usize,
}

pub struct DuplicateAssetRow {
    pub id: i64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct FileMutationAsset {
    pub id: i64,
    pub path: String,
    pub thumb_path: Option<String>,
    pub record_version: i64,
}

#[derive(Debug, Clone)]
pub struct PendingFileOperation {
    pub operation_id: String,
    pub asset_id: i64,
    pub action: String,
    pub original_path: String,
    pub staging_path: String,
    pub final_path: Option<String>,
    pub committed: bool,
}

pub enum DbFileMutation<'a> {
    Rename {
        asset_id: i64,
        expected_path: &'a str,
        expected_record_version: i64,
        new_path: &'a str,
        new_file_name: &'a str,
    },
    Delete {
        asset_id: i64,
        expected_path: &'a str,
        expected_record_version: i64,
    },
}

#[derive(Debug, Clone)]
pub enum AssetMetaFilter {
    HasNoTags { tag_count: i64 },
    GroupName { group_name: String },
}

#[derive(Debug, Clone)]
pub struct ExistingAssetFingerprint {
    pub id: i64,
    pub path: String,
    pub kind: String,
    pub size_bytes: i64,
    pub modified_at_ns: i64,
}

/// Outcome of a compare-and-set thumbnail write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailCasOutcome {
    /// The asset still matches the expected source version; the path was
    /// stored (or already equaled the requested value).
    Applied,
    /// The record was re-indexed after the task snapshot was taken; nothing
    /// was written and any produced file must be discarded by the caller.
    VersionMismatch,
}

#[cfg(test)]
mod tests {
    use rusqlite::{params, Connection};

    use crate::models::NewAsset;

    use super::{
        add_scan_root, clear_thumbnail_failure, current_library_revision,
        delete_asset_by_id_with_thumb, delete_assets_by_prefix_with_thumbs, get_asset_media_group,
        get_asset_path_and_thumb_by_id, grouped_bucket_stats_sql, init_schema,
        list_asset_summaries_by_ids, list_assets, list_assets_for_csv_export,
        list_assets_for_thumbnail_render, list_assets_with_meta,
        list_duplicate_assets_by_file_name_key, list_duplicate_file_name_counts,
        list_failed_assets_for_thumbnail_render, list_failed_thumbnail_asset_ids,
        list_ordered_asset_ids_with_meta, list_tags_page, merge_asset_tags_bulk,
        merge_asset_tags_bulk_with_revision, prune_completed_scan_root_generation,
        record_thumbnail_failure, rename_asset_file_by_id, root_descendant_like_pattern,
        set_asset_favorite, set_asset_favorite_with_revision, set_asset_media_group,
        set_asset_media_group_with_revision, set_asset_tags, set_asset_tags_with_revision,
        set_assets_media_group_bulk, try_list_ordered_asset_ids_with_meta,
        update_asset_thumbnail_path_if_version_matches,
        update_asset_thumbnail_paths_batch_versioned, upsert_asset, upsert_scanned_asset,
        validate_backup_database, AssetMetaFilter, ThumbnailCasOutcome,
    };

    #[test]
    fn descendant_pattern_preserves_the_root_separator_style() {
        assert_eq!(root_descendant_like_pattern("C:\\media\\"), "C:\\media\\%");
        assert_eq!(root_descendant_like_pattern("/srv/media/"), "/srv/media/%");
        assert_eq!(root_descendant_like_pattern("/"), "/%");
        assert_eq!(
            root_descendant_like_pattern("/srv/100%_media"),
            "/srv/100^%^_media/%"
        );
    }

    #[test]
    fn completed_scan_prune_bumps_revision_only_when_it_removes_an_asset() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        add_scan_root(&conn, "/media").expect("root");
        upsert_scanned_asset(
            &conn,
            &NewAsset {
                path: "/media/stale.jpg".to_string(),
                kind: "image".to_string(),
                size_bytes: 1,
                modified_at: 1,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            },
            1,
            "/media",
            1,
        )
        .expect("asset");
        let baseline = current_library_revision(&conn).expect("baseline");

        assert_eq!(
            prune_completed_scan_root_generation(&conn, "/media", 1).expect("no-op prune"),
            0
        );
        assert_eq!(
            current_library_revision(&conn).expect("no-op revision"),
            baseline
        );

        assert_eq!(
            prune_completed_scan_root_generation(&conn, "/media", 2).expect("prune"),
            1
        );
        assert_eq!(
            current_library_revision(&conn).expect("pruned revision"),
            baseline + 1
        );
    }

    #[test]
    fn completed_scan_prune_rolls_back_asset_removal_when_revision_bump_fails() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        add_scan_root(&conn, "/media").expect("root");
        upsert_scanned_asset(
            &conn,
            &NewAsset {
                path: "/media/stale.jpg".to_string(),
                kind: "image".to_string(),
                size_bytes: 1,
                modified_at: 1,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            },
            1,
            "/media",
            1,
        )
        .expect("asset");
        let baseline = current_library_revision(&conn).expect("baseline");
        conn.execute_batch(
            "CREATE TEMP TRIGGER fail_prune_revision
             BEFORE UPDATE ON library_metadata
             WHEN OLD.key = 'revision'
             BEGIN SELECT RAISE(ABORT, 'injected revision failure'); END;",
        )
        .expect("trigger");

        assert!(prune_completed_scan_root_generation(&conn, "/media", 2).is_err());
        assert_eq!(
            list_assets(&conn, 0, 10, &[], &[], None, false)
                .expect("assets")
                .total,
            1
        );
        assert_eq!(current_library_revision(&conn).expect("revision"), baseline);
    }

    fn insert_asset(conn: &Connection, path: &str, kind: &str, modified_at: i64) {
        upsert_asset(
            conn,
            &NewAsset {
                path: path.to_string(),
                kind: kind.to_string(),
                size_bytes: 1,
                modified_at,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            },
        )
        .expect("upsert asset");
    }

    #[test]
    fn summary_preview_paths_include_video_and_gif_sources_only() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "/media/still.jpg", "image", 30);
        insert_asset(&conn, "/media/clip.mp4", "video", 20);
        insert_asset(&conn, "/media/animation.gif", "gif", 10);

        let summaries = list_asset_summaries_by_ids(&conn, &[2, 999, 3, 1]).expect("summaries");

        assert_eq!(
            summaries.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
        assert_eq!(
            summaries[0].preview_path.as_deref(),
            Some("/media/clip.mp4")
        );
        assert_eq!(
            summaries[1].preview_path.as_deref(),
            Some("/media/animation.gif")
        );
        assert_eq!(summaries[2].preview_path, None);
    }

    #[test]
    fn delete_by_prefix_does_not_match_similar_root() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        conn.execute(
            "INSERT INTO assets(path, kind, size_bytes, modified_at, width, height, duration_ms, thumb_path, indexed_at) VALUES (?1, 'image', 1, 1, NULL, NULL, NULL, NULL, unixepoch())",
            ["C:\\media\\a.jpg"],
        )
        .expect("insert 1");
        conn.execute(
            "INSERT INTO assets(path, kind, size_bytes, modified_at, width, height, duration_ms, thumb_path, indexed_at) VALUES (?1, 'image', 1, 1, NULL, NULL, NULL, NULL, unixepoch())",
            ["C:\\media2\\b.jpg"],
        )
        .expect("insert 2");

        let (removed, _) = delete_assets_by_prefix_with_thumbs(&conn, "C:\\media").expect("delete");
        assert_eq!(removed, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn list_assets_respects_kind_and_tag_filters_with_and_not_logic() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\b.mp4", "video", 20);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 10);

        set_asset_tags(&conn, 1, &["cat".to_string(), "vacation".to_string()]).expect("tags 1");
        set_asset_tags(&conn, 2, &["cat".to_string()]).expect("tags 2");
        set_asset_tags(
            &conn,
            3,
            &["cat".to_string(), "vacation".to_string(), "dog".to_string()],
        )
        .expect("tags 3");

        let page = list_assets(
            &conn,
            0,
            50,
            &["cat".to_string(), "vacation".to_string()],
            &["dog".to_string()],
            Some("image"),
            false,
        )
        .expect("list");

        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].path, "C:\\media\\a.jpg");
    }

    #[test]
    fn list_assets_paginates_in_modified_desc_order() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 10);

        let page = list_assets(&conn, 1, 1, &[], &[], None, false).expect("list");
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].path, "C:\\media\\b.jpg");
    }

    #[test]
    fn list_assets_keeps_media_group_adjacent_and_sorts_by_group_order() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\top.jpg", "image", 100);
        insert_asset(&conn, "C:\\media\\group-second.jpg", "image", 95);
        insert_asset(&conn, "C:\\media\\group-null.jpg", "image", 96);
        insert_asset(&conn, "C:\\media\\group-first.jpg", "image", 97);
        insert_asset(&conn, "C:\\media\\bottom.jpg", "image", 90);

        set_asset_media_group(&conn, 2, Some("group-alpha"), Some(2.0)).expect("set order 2");
        set_asset_media_group(&conn, 3, Some("group-alpha"), None).expect("set null order");
        set_asset_media_group(&conn, 4, Some("group-alpha"), Some(1.0)).expect("set order 1");

        let page = list_assets(&conn, 0, 50, &[], &[], None, false).expect("list");
        let ordered_paths = page
            .items
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ordered_paths,
            vec![
                "C:\\media\\top.jpg",
                "C:\\media\\group-first.jpg",
                "C:\\media\\group-second.jpg",
                "C:\\media\\group-null.jpg",
                "C:\\media\\bottom.jpg",
            ]
        );
    }

    #[test]
    fn list_assets_positions_group_by_newest_asset_in_group() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\group-old-a.jpg", "image", 70);
        insert_asset(&conn, "C:\\media\\group-old-b.jpg", "image", 60);
        insert_asset(&conn, "C:\\media\\single.jpg", "image", 75);
        insert_asset(&conn, "C:\\media\\group-new-a.jpg", "image", 80);
        insert_asset(&conn, "C:\\media\\group-new-b.jpg", "image", 10);

        set_asset_media_group(&conn, 1, Some("group-old"), Some(1.0)).expect("old 1");
        set_asset_media_group(&conn, 2, Some("group-old"), Some(2.0)).expect("old 2");
        set_asset_media_group(&conn, 4, Some("group-new"), Some(1.0)).expect("new 1");
        set_asset_media_group(&conn, 5, Some("group-new"), Some(2.0)).expect("new 2");

        let page = list_assets(&conn, 0, 50, &[], &[], None, false).expect("list");
        let ordered_paths = page
            .items
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ordered_paths,
            vec![
                "C:\\media\\group-new-a.jpg",
                "C:\\media\\group-new-b.jpg",
                "C:\\media\\single.jpg",
                "C:\\media\\group-old-a.jpg",
                "C:\\media\\group-old-b.jpg",
            ]
        );
    }

    #[test]
    fn list_assets_filters_favorites_only() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 20);

        set_asset_favorite(&conn, 2, true).expect("favorite b");

        let page = list_assets(&conn, 0, 50, &[], &[], None, true).expect("list favorites");
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].path, "C:\\media\\b.jpg");
        assert!(page.items[0].is_favorite);
    }

    #[test]
    fn list_assets_filters_exact_tag_count_metatags() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\zero.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\two.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\three.jpg", "image", 10);

        set_asset_tags(&conn, 2, &["cat".to_string(), "travel".to_string()]).expect("tags 2");
        set_asset_tags(
            &conn,
            3,
            &["cat".to_string(), "travel".to_string(), "dog".to_string()],
        )
        .expect("tags 3");

        let zero_tags = list_assets_with_meta(
            &conn,
            0,
            50,
            &[],
            &[],
            None,
            false,
            Some(&AssetMetaFilter::HasNoTags { tag_count: 0 }),
        )
        .expect("zero tags");
        assert_eq!(zero_tags.total, 1);
        assert_eq!(zero_tags.items[0].path, "C:\\media\\zero.jpg");

        let two_tags = list_assets_with_meta(
            &conn,
            0,
            50,
            &[],
            &[],
            None,
            false,
            Some(&AssetMetaFilter::HasNoTags { tag_count: 2 }),
        )
        .expect("two tags");
        assert_eq!(two_tags.total, 1);
        assert_eq!(two_tags.items[0].path, "C:\\media\\two.jpg");
    }

    #[test]
    fn list_assets_filters_group_name_as_exact_case_insensitive_match() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\match-a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\match-b.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\other.jpg", "image", 10);

        set_asset_media_group(&conn, 1, Some("Trip-2026"), Some(1.0)).expect("group a");
        set_asset_media_group(&conn, 2, Some("trip-2026"), Some(2.0)).expect("group b");
        set_asset_media_group(&conn, 3, Some("Trip-2027"), Some(1.0)).expect("group other");

        let page = list_assets_with_meta(
            &conn,
            0,
            50,
            &[],
            &[],
            None,
            false,
            Some(&AssetMetaFilter::GroupName {
                group_name: "  TRIP-2026  ".to_string(),
            }),
        )
        .expect("group match");

        let paths = page
            .items
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(page.total, 2);
        assert_eq!(
            paths,
            vec!["C:\\media\\match-a.jpg", "C:\\media\\match-b.jpg"]
        );
    }

    #[test]
    fn ordered_asset_ids_preserve_group_and_singleton_sort_tuple() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        for (path, modified_at) in [
            ("C:\\media\\group-a-second.jpg", 100),
            ("C:\\media\\group-a-null.jpg", 90),
            ("C:\\media\\group-a-first.jpg", 80),
            ("C:\\media\\null-key.jpg", 95),
            ("C:\\media\\blank-key.jpg", 110),
            ("C:\\media\\whitespace-key.jpg", 100),
            ("C:\\media\\case-second.jpg", 70),
            ("C:\\media\\case-first.jpg", 70),
            ("C:\\media\\lowercase-case.jpg", 70),
            ("C:\\media\\tie-singleton.jpg", 70),
            ("C:\\media\\group-b-null.jpg", 60),
            ("C:\\media\\group-b-ordered.jpg", 61),
        ] {
            insert_asset(&conn, path, "image", modified_at);
        }

        for (id, group_key, group_order) in [
            (1, Some("group-a"), Some(2.0)),
            (2, Some("group-a"), None),
            (3, Some("group-a"), Some(1.0)),
            (4, None, Some(-10.0)),
            (5, Some(""), Some(-20.0)),
            (6, Some("   "), None),
            (7, Some("Case"), Some(2.0)),
            (8, Some("Case"), Some(1.0)),
            (9, Some("case"), None),
            (10, None, None),
            (11, Some("group-b"), None),
            (12, Some("group-b"), Some(2.0)),
        ] {
            conn.execute(
                "UPDATE assets SET media_group_key = ?1, media_group_order = ?2 WHERE id = ?3",
                params![group_key, group_order, id],
            )
            .expect("seed raw group values");
        }

        let ids = list_ordered_asset_ids_with_meta(&conn, &[], &[], None, false, None)
            .expect("ordered ids");

        assert_eq!(ids, vec![5, 6, 3, 1, 2, 4, 10, 9, 8, 7, 12, 11]);
    }

    #[test]
    fn ordered_asset_ids_coerce_text_and_blob_group_keys_like_legacy_buckets() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\text-group.jpg", "image", 50);
        insert_asset(&conn, "C:\\media\\blob-group.jpg", "image", 200);
        insert_asset(&conn, "C:\\media\\singleton.jpg", "image", 100);

        conn.execute(
            "UPDATE assets SET media_group_key = 'same', media_group_order = 2 WHERE id = 1",
            [],
        )
        .expect("seed text group key");
        conn.execute(
            "UPDATE assets SET media_group_key = x'73616d65', media_group_order = 1 WHERE id = 2",
            [],
        )
        .expect("seed blob group key");

        let storage_classes = conn
            .prepare("SELECT typeof(media_group_key) FROM assets WHERE id IN (1, 2) ORDER BY id")
            .expect("storage class query")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("storage class rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("storage classes");
        assert_eq!(storage_classes, vec!["text", "blob"]);

        let ids = list_ordered_asset_ids_with_meta(&conn, &[], &[], None, false, None)
            .expect("ordered ids");

        assert_eq!(ids, vec![2, 1, 3]);
    }

    #[test]
    fn grouped_bucket_stats_exclude_singleton_buckets() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        for (path, modified_at) in [
            ("C:\\media\\null.jpg", 50),
            ("C:\\media\\empty.jpg", 40),
            ("C:\\media\\blank.jpg", 30),
            ("C:\\media\\group-a.jpg", 20),
            ("C:\\media\\group-b.jpg", 10),
        ] {
            insert_asset(&conn, path, "image", modified_at);
        }
        conn.execute("UPDATE assets SET media_group_key = '' WHERE id = 2", [])
            .expect("seed empty key");
        conn.execute("UPDATE assets SET media_group_key = '   ' WHERE id = 3", [])
            .expect("seed blank key");
        conn.execute(
            "UPDATE assets SET media_group_key = 'real-group' WHERE id IN (4, 5)",
            [],
        )
        .expect("seed real group");

        let sql = format!(
            "WITH filtered_assets AS (
               SELECT id, modified_at, media_group_key FROM assets
             ),
             grouped_bucket_stats AS (
               {}
             )
             SELECT COUNT(*), MIN(sort_bucket) FROM grouped_bucket_stats",
            grouped_bucket_stats_sql()
        );
        let (bucket_count, bucket): (i64, String) = conn
            .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("grouped bucket stats");

        assert_eq!(bucket_count, 1);
        assert_eq!(bucket, "group:real-group");
    }

    #[test]
    fn ordered_asset_ids_filter_before_computing_group_stats() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        for (path, kind, modified_at) in [
            ("C:\\media\\excluded-new-group-member.mp4", "video", 200),
            ("C:\\media\\included-group-second.jpg", "image", 50),
            ("C:\\media\\included-singleton.jpg", "image", 100),
            ("C:\\media\\included-group-first.jpg", "image", 40),
            ("C:\\media\\excluded-dog.jpg", "image", 110),
            ("C:\\media\\excluded-not-favorite.jpg", "image", 120),
        ] {
            insert_asset(&conn, path, kind, modified_at);
        }

        set_asset_media_group(&conn, 1, Some("trip"), Some(0.0)).expect("excluded group");
        set_asset_media_group(&conn, 2, Some("trip"), Some(2.0)).expect("group second");
        set_asset_media_group(&conn, 4, Some("trip"), Some(1.0)).expect("group first");

        for id in 1..=6 {
            set_asset_tags(&conn, id, &["cat".to_string()]).expect("cat tag");
        }
        set_asset_tags(&conn, 5, &["cat".to_string(), "dog".to_string()])
            .expect("excluded dog tags");
        for id in [2, 3, 4, 5] {
            set_asset_favorite(&conn, id, true).expect("favorite");
        }

        let ids = list_ordered_asset_ids_with_meta(
            &conn,
            &["cat".to_string()],
            &["dog".to_string()],
            Some("image"),
            true,
            Some(&AssetMetaFilter::HasNoTags { tag_count: 1 }),
        )
        .expect("filtered ordered ids");

        assert_eq!(ids, vec![3, 4, 2]);
    }

    #[test]
    fn init_schema_adds_new_columns_to_legacy_assets_table() {
        let conn = Connection::open_in_memory().expect("db");
        conn.execute_batch(
            "
            CREATE TABLE assets (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              path TEXT NOT NULL UNIQUE,
              kind TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              modified_at INTEGER NOT NULL,
              width INTEGER,
              height INTEGER,
              duration_ms INTEGER,
              thumb_path TEXT,
              indexed_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            ",
        )
        .expect("legacy assets schema");

        init_schema(&conn).expect("schema");

        let mut columns = Vec::new();
        let mut stmt = conn
            .prepare("PRAGMA table_info(assets)")
            .expect("table info");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("table info rows");
        for row in rows {
            columns.push(row.expect("column name"));
        }

        assert!(columns.iter().any(|name| name == "is_favorite"));
        assert!(columns.iter().any(|name| name == "media_group_key"));
        assert!(columns.iter().any(|name| name == "media_group_order"));
        assert!(columns.iter().any(|name| name == "file_name"));
    }

    #[test]
    fn version_three_migration_splits_legacy_control_character_tags() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        conn.execute(
            "INSERT INTO tags(name) VALUES (?1)",
            params!["cat\0dog\u{7}bird"],
        )
        .expect("legacy tag");
        conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) SELECT 1, id FROM tags",
            [],
        )
        .expect("legacy mapping");
        conn.execute("UPDATE assets SET tag_count = 1 WHERE id = 1", [])
            .expect("legacy count");
        conn.execute(
            "UPDATE library_metadata SET value = 2 WHERE key = 'performance_schema_version'",
            [],
        )
        .expect("legacy performance version");

        init_schema(&conn).expect("migrate schema");

        assert_eq!(
            super::list_asset_tags(&conn, 1).expect("migrated tags"),
            vec!["bird".to_string(), "cat".to_string(), "dog".to_string()]
        );
        assert_eq!(
            conn.query_row(
                "SELECT value FROM library_metadata WHERE key = 'performance_schema_version'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .expect("performance version"),
            3
        );
    }

    #[test]
    fn startup_preference_migrates_and_survives_reopen_and_duplicate_add() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("media.db");
        {
            let conn = super::open_connection(&path).unwrap();
            super::init_schema(&conn).unwrap();
            super::add_scan_root(&conn, "/media").unwrap();
            conn.execute_batch(
                "ALTER TABLE scan_roots DROP COLUMN auto_scan_on_startup; PRAGMA user_version = 1;",
            )
            .unwrap();
            assert_eq!(
                super::validate_backup_database(&conn, false).unwrap(),
                super::BackupSchemaCompatibility::Version1
            );
            super::init_schema(&conn).unwrap();
            assert!(!super::list_scan_root_settings(&conn).unwrap()[0].auto_scan_on_startup);
            let revision = super::current_library_revision(&conn).unwrap();
            super::set_scan_root_auto_scan(&conn, "/media", true).unwrap();
            super::add_scan_root(&conn, "/media").unwrap();
            assert_eq!(revision, super::current_library_revision(&conn).unwrap());
            assert!(super::set_scan_root_auto_scan(&conn, "/missing", true).is_err());
        }
        let conn = super::open_connection(&path).unwrap();
        super::init_schema(&conn).unwrap();
        assert!(super::list_scan_root_settings(&conn).unwrap()[0].auto_scan_on_startup);
        assert_eq!(
            super::validate_backup_database(&conn, false).unwrap(),
            super::BackupSchemaCompatibility::Current
        );
        super::remove_scan_root(&conn, "/media").unwrap();
        super::add_scan_root(&conn, "/media").unwrap();
        assert!(!super::list_scan_root_settings(&conn).unwrap()[0].auto_scan_on_startup);
    }

    #[test]
    fn backup_validation_rejects_invalid_startup_preferences() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::init_schema(&conn).unwrap();
        super::add_scan_root(&conn, "/media").unwrap();
        conn.execute_batch(
            "PRAGMA ignore_check_constraints = ON; UPDATE scan_roots SET auto_scan_on_startup = 2;",
        )
        .unwrap();
        assert!(super::validate_backup_database(&conn, false)
            .unwrap_err()
            .to_string()
            .contains("startup scan preferences"));
    }

    #[test]
    fn backup_validation_rejects_noncanonical_unicode_tag_names() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        conn.execute("INSERT INTO tags(name) VALUES ('ŻÓŁW')", [])
            .expect("noncanonical tag");

        let error = validate_backup_database(&conn, false).expect_err("invalid backup tag");
        assert!(error.to_string().contains("noncanonical tag names"));
    }

    #[test]
    fn upsert_replaces_thumbnail_when_size_changes_within_same_second() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        upsert_asset(
            &conn,
            &NewAsset {
                path: "C:\\media\\a.jpg".to_string(),
                kind: "image".to_string(),
                size_bytes: 1,
                modified_at: 123,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: Some("thumb-old.jpg".to_string()),
            },
        )
        .expect("first upsert");

        upsert_asset(
            &conn,
            &NewAsset {
                path: "C:\\media\\a.jpg".to_string(),
                kind: "image".to_string(),
                size_bytes: 99,
                modified_at: 123,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: Some("thumb-new.jpg".to_string()),
            },
        )
        .expect("second upsert");

        let thumb_path: Option<String> = conn
            .query_row(
                "SELECT thumb_path FROM assets WHERE path = ?1",
                params!["C:\\media\\a.jpg"],
                |row| row.get(0),
            )
            .expect("thumb path");
        assert_eq!(thumb_path.as_deref(), Some("thumb-new.jpg"));
    }

    #[test]
    fn list_tags_page_filters_case_insensitively() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        set_asset_tags(&conn, 1, &["cat".to_string(), "Vacation".to_string()]).expect("tags");

        let filtered = list_tags_page(&conn, "vaC", 0, 100).expect("filtered tags");
        assert_eq!(filtered.items, vec!["vacation".to_string()]);
    }

    #[test]
    fn set_asset_tags_normalizes_defensively_and_rejects_missing_assets() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);

        set_asset_tags(
            &conn,
            1,
            &[
                " Cat ".to_string(),
                "cat".to_string(),
                "".to_string(),
                "travel".to_string(),
            ],
        )
        .expect("normalized tags");
        let count: i64 = conn
            .query_row("SELECT tag_count FROM assets WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let mapping_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM asset_tags WHERE asset_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(mapping_count, 2);
        assert!(set_asset_tags(&conn, 1, &["new york".to_string()]).is_err());
        assert!(set_asset_tags(&conn, 999, &[]).is_err());
        assert!(set_asset_tags(&conn, 999, &["cat".to_string()]).is_err());
    }

    #[test]
    fn semantic_noop_repairs_tag_count_and_canonicalizes_legacy_tag_spelling() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        conn.execute("INSERT INTO tags(name) VALUES ('Cat')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) SELECT 1, id FROM tags WHERE name = 'Cat'",
            [],
        )
        .unwrap();
        conn.execute("UPDATE assets SET tag_count = 99 WHERE id = 1", [])
            .unwrap();
        let initial = current_library_revision(&conn).unwrap();

        let repaired = set_asset_tags_with_revision(&mut conn, 1, &[" CAT ".to_string()]).unwrap();
        assert!(repaired.changed);
        assert_eq!(repaired.query_impact, crate::models::TagQueryImpact::All);
        assert_eq!(repaired.tags, vec!["cat".to_string()]);
        assert_eq!(repaired.revision, initial + 1);
        let stored: (String, i64) = conn
            .query_row(
                "SELECT t.name, a.tag_count FROM assets a
                 JOIN asset_tags at ON at.asset_id = a.id
                 JOIN tags t ON t.id = at.tag_id WHERE a.id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, ("cat".to_string(), 1));

        let noop = set_asset_tags_with_revision(&mut conn, 1, &["cat".to_string()]).unwrap();
        assert!(!noop.changed);
        assert_eq!(noop.revision, repaired.revision);
    }

    #[test]
    fn canonicalization_consolidates_colliding_rows_and_preserves_shared_mappings() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 2);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 1);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 1);
        conn.execute("INSERT INTO tags(name) VALUES ('cat')", [])
            .unwrap();
        conn.execute("INSERT INTO tags(name) VALUES (' Cat ')", [])
            .unwrap();
        let canonical_id: i64 = conn
            .query_row(
                "SELECT id FROM tags WHERE name = 'cat' COLLATE BINARY",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let legacy_id: i64 = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ' Cat ' COLLATE BINARY",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) VALUES (1, ?1), (1, ?2), (2, ?2), (3, ?1)",
            params![canonical_id, legacy_id],
        )
        .unwrap();
        conn.execute("UPDATE assets SET tag_count = 2 WHERE id = 1", [])
            .unwrap();
        conn.execute("UPDATE assets SET tag_count = 1 WHERE id IN (2, 3)", [])
            .unwrap();

        let result = set_asset_tags_with_revision(&mut conn, 1, &["cat".to_string()]).unwrap();
        assert!(result.changed);
        assert_eq!(result.tags, vec!["cat".to_string()]);
        assert_eq!(result.query_impact, crate::models::TagQueryImpact::All);
        let tag_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE lower(trim(name)) = 'cat'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag_rows, 1);
        let mappings = conn
            .prepare(
                "SELECT at.asset_id, t.name FROM asset_tags at JOIN tags t ON t.id = at.tag_id
                 ORDER BY at.asset_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            mappings,
            vec![
                (1, "cat".to_string()),
                (2, "cat".to_string()),
                (3, "cat".to_string()),
            ]
        );
        let counts = conn
            .prepare("SELECT id, tag_count FROM assets ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(counts, vec![(1, 1), (2, 1), (3, 1)]);
    }

    #[test]
    fn tag_revision_mutations_are_noop_aware_and_return_partial_bulk_results() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);
        let initial = current_library_revision(&conn).unwrap();
        let first = set_asset_tags_with_revision(&mut conn, 1, &["cat".to_string()]).unwrap();
        assert!(first.changed);
        assert_eq!(first.revision, initial + 1);
        let noop = set_asset_tags_with_revision(&mut conn, 1, &[" CAT ".to_string()]).unwrap();
        assert!(!noop.changed);
        assert_eq!(noop.revision, first.revision);

        let (results, revision, _) =
            merge_asset_tags_bulk_with_revision(&mut conn, &[1, 999, 2], &["travel".to_string()])
                .unwrap();
        assert_eq!(
            results
                .iter()
                .map(|result| result.asset_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(results.iter().all(|result| result.changed));
        assert_eq!(revision, first.revision + 1);
    }

    #[test]
    fn bulk_tag_merge_with_revision_short_circuits_empty_inputs_without_repairs() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        conn.execute("INSERT INTO tags(name) VALUES (' Legacy ')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) SELECT 1, id FROM tags WHERE name = ' Legacy '",
            [],
        )
        .unwrap();
        conn.execute("UPDATE assets SET tag_count = 99 WHERE id = 1", [])
            .unwrap();
        let initial_revision = current_library_revision(&conn).unwrap();

        let (empty_id_results, empty_id_revision, _) =
            merge_asset_tags_bulk_with_revision(&mut conn, &[], &["travel".to_string()]).unwrap();
        let (blank_tag_results, blank_tag_revision, _) = merge_asset_tags_bulk_with_revision(
            &mut conn,
            &[1],
            &["  ".to_string(), "".to_string()],
        )
        .unwrap();

        assert!(empty_id_results.is_empty());
        assert!(blank_tag_results.is_empty());
        assert_eq!(empty_id_revision, initial_revision);
        assert_eq!(blank_tag_revision, initial_revision);
        let stored: (String, i64) = conn
            .query_row(
                "SELECT t.name, a.tag_count FROM assets a
                 JOIN asset_tags at ON at.asset_id = a.id
                 JOIN tags t ON t.id = at.tag_id WHERE a.id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (" Legacy ".to_string(), 99));
    }

    #[test]
    fn merge_asset_tags_bulk_merges_tags_and_is_idempotent_for_duplicates() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 3);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);

        set_asset_tags(&conn, 1, &["cat".to_string()]).expect("set tags 1");
        set_asset_tags(&conn, 2, &["dog".to_string(), "cat".to_string()]).expect("set tags 2");

        let (processed, updated) = merge_asset_tags_bulk(
            &conn,
            &[1, 2, 999],
            &[
                "cat".to_string(),
                "travel".to_string(),
                "  TRAVEL ".to_string(),
            ],
        )
        .expect("merge bulk");

        assert_eq!(processed, 2);
        assert_eq!(updated, 2);

        let a_tags = list_assets(&conn, 0, 10, &[], &[], None, false)
            .expect("list")
            .items
            .into_iter()
            .find(|asset| asset.id == 1)
            .expect("asset 1")
            .tags;
        let b_tags = list_assets(&conn, 0, 10, &[], &[], None, false)
            .expect("list")
            .items
            .into_iter()
            .find(|asset| asset.id == 2)
            .expect("asset 2")
            .tags;

        assert!(a_tags.iter().any(|tag| tag == "cat"));
        assert!(a_tags.iter().any(|tag| tag == "travel"));
        assert!(b_tags.iter().any(|tag| tag == "dog"));
        assert!(b_tags.iter().any(|tag| tag == "travel"));

        let (processed_second, updated_second) =
            merge_asset_tags_bulk(&conn, &[1, 2], &["travel".to_string(), "cat".to_string()])
                .expect("merge second");

        assert_eq!(processed_second, 2);
        assert_eq!(updated_second, 0);
    }

    #[test]
    fn set_asset_media_group_updates_values() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);

        set_asset_media_group(&conn, 1, Some("group-alpha"), Some(10.5)).expect("set media group");

        let (group_key, group_order) = get_asset_media_group(&conn, 1).expect("get media group");
        assert_eq!(group_key.as_deref(), Some("group-alpha"));
        assert_eq!(group_order, Some(10.5));

        set_asset_media_group(&conn, 1, None, None).expect("clear media group");
        let (group_key_cleared, group_order_cleared) =
            get_asset_media_group(&conn, 1).expect("get cleared media group");
        assert_eq!(group_key_cleared, None);
        assert_eq!(group_order_cleared, None);
    }

    #[test]
    fn set_assets_media_group_bulk_updates_multiple_assets_in_order() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 3);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 1);
        set_asset_media_group(&conn, 1, Some("legacy"), Some(7.0)).expect("seed group");

        let (processed, updated) = set_assets_media_group_bulk(
            &mut conn,
            &[(2, Some(1.0)), (1, Some(2.0)), (999, Some(3.0))],
            Some("trip-2026"),
        )
        .expect("bulk set");

        assert_eq!(processed, vec![2, 1]);
        assert_eq!(updated, 2);

        let page = list_assets(&conn, 0, 10, &[], &[], None, false).expect("list");
        let first = page
            .items
            .iter()
            .find(|asset| asset.id == 1)
            .expect("first asset");
        let second = page
            .items
            .iter()
            .find(|asset| asset.id == 2)
            .expect("second asset");
        let third = page
            .items
            .iter()
            .find(|asset| asset.id == 3)
            .expect("third asset");

        assert_eq!(first.media_group_key.as_deref(), Some("trip-2026"));
        assert_eq!(first.media_group_order, Some(2.0));
        assert_eq!(second.media_group_key.as_deref(), Some("trip-2026"));
        assert_eq!(second.media_group_order, Some(1.0));
        assert_eq!(third.media_group_key, None);
        assert_eq!(third.media_group_order, None);
    }

    #[test]
    fn set_assets_media_group_bulk_clears_keys_and_orders() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 2);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 1);
        set_asset_media_group(&conn, 1, Some("legacy-a"), Some(4.0)).expect("seed first group");
        set_asset_media_group(&conn, 2, Some("legacy-b"), Some(7.0)).expect("seed second group");

        let (processed, updated) =
            set_assets_media_group_bulk(&mut conn, &[(1, None), (2, None)], None)
                .expect("clear bulk groups");

        assert_eq!(processed, vec![1, 2]);
        assert_eq!(updated, 2);
        assert_eq!(
            get_asset_media_group(&conn, 1).expect("first group"),
            (None, None)
        );
        assert_eq!(
            get_asset_media_group(&conn, 2).expect("second group"),
            (None, None)
        );
    }

    #[test]
    fn list_assets_for_csv_export_includes_media_group_fields() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        set_asset_tags(&conn, 1, &["cat".to_string()]).expect("set tags");
        set_asset_favorite(&conn, 1, true).expect("set favorite");
        set_asset_media_group(&conn, 1, Some("group-alpha"), Some(7.25)).expect("set media group");

        let rows = list_assets_for_csv_export(&conn).expect("csv rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].media_group_key.as_deref(), Some("group-alpha"));
        assert_eq!(rows[0].media_group_order, Some(7.25));
    }

    #[test]
    fn list_assets_for_thumbnail_render_includes_gifs() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.gif", "gif", 1);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);

        let assets = list_assets_for_thumbnail_render(&conn).expect("thumbnail assets");
        let kinds = assets
            .into_iter()
            .map(|asset| asset.kind)
            .collect::<Vec<_>>();

        assert_eq!(kinds, vec!["gif".to_string(), "image".to_string()]);
    }

    #[test]
    fn thumbnail_failures_track_and_filter_by_asset_version() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 2);

        record_thumbnail_failure(&conn, 1, 1, Some("decode error")).expect("record fail a");
        record_thumbnail_failure(&conn, 1, 1, Some("decode error again"))
            .expect("record fail a again");
        record_thumbnail_failure(&conn, 2, 2, Some("missing source")).expect("record fail b");

        let failed_ids = list_failed_thumbnail_asset_ids(&conn).expect("failed ids");
        assert_eq!(failed_ids, vec![1, 2]);

        let failed_assets = list_failed_assets_for_thumbnail_render(&conn).expect("failed assets");
        assert_eq!(failed_assets.len(), 2);
        assert_eq!(failed_assets[0].id, 1);
        assert_eq!(failed_assets[1].id, 2);

        upsert_asset(
            &conn,
            &NewAsset {
                path: "C:\\media\\a.jpg".to_string(),
                kind: "image".to_string(),
                size_bytes: 1,
                modified_at: 99,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            },
        )
        .expect("upsert changed a");

        let failed_ids_after_change =
            list_failed_thumbnail_asset_ids(&conn).expect("failed ids after change");
        assert_eq!(failed_ids_after_change, vec![2]);

        clear_thumbnail_failure(&conn, 2).expect("clear fail b");
        let final_failed = list_failed_thumbnail_asset_ids(&conn).expect("final failed ids");
        assert!(final_failed.is_empty());
    }

    #[test]
    fn delete_by_prefix_removes_thumbnail_failures() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 10);
        insert_asset(&conn, "C:\\other\\b.jpg", "image", 11);

        record_thumbnail_failure(&conn, 1, 10, Some("broken a")).expect("record a");
        record_thumbnail_failure(&conn, 2, 11, Some("broken b")).expect("record b");

        let (_removed, _thumbs) =
            delete_assets_by_prefix_with_thumbs(&conn, "C:\\media").expect("delete media");

        let failed_ids = list_failed_thumbnail_asset_ids(&conn).expect("failed ids after delete");
        assert_eq!(failed_ids, vec![2]);
    }

    fn insert_scanned_asset(
        conn: &Connection,
        path: &str,
        size_bytes: i64,
        modified_at: i64,
        mtime_ns: i64,
        thumb_path: Option<&str>,
    ) {
        add_scan_root(conn, "C:\\media").expect("add scan root");
        upsert_scanned_asset(
            conn,
            &NewAsset {
                path: path.to_string(),
                kind: "image".to_string(),
                size_bytes,
                modified_at,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: thumb_path.map(str::to_string),
            },
            mtime_ns,
            "C:\\media",
            1,
        )
        .expect("insert scanned asset");
    }

    #[test]
    fn thumbnail_cas_applies_when_source_version_matches() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_scanned_asset(&conn, "C:\\media\\a.jpg", 100, 5, 5_000_000_000, None);

        let version = crate::thumbs::SourceVersion::new("C:\\media\\a.jpg", 100, 5_000_000_000);
        let outcome = update_asset_thumbnail_path_if_version_matches(
            &conn,
            1,
            Some("C:\\thumbs\\a.jpg"),
            &version,
        )
        .expect("cas apply");
        assert_eq!(outcome, ThumbnailCasOutcome::Applied);

        // Applying the same value again is still a successful no-op.
        let repeat = update_asset_thumbnail_path_if_version_matches(
            &conn,
            1,
            Some("C:\\thumbs\\a.jpg"),
            &version,
        )
        .expect("cas repeat");
        assert_eq!(repeat, ThumbnailCasOutcome::Applied);
    }

    #[test]
    fn thumbnail_cas_rejects_result_of_reindexed_version() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_scanned_asset(&conn, "C:\\media\\a.jpg", 100, 5, 5_000_000_000, None);

        // Render started for this snapshot...
        let stale_version =
            crate::thumbs::SourceVersion::new("C:\\media\\a.jpg", 100, 5_000_000_000);
        // ...but a scan re-indexed the file (same second, new nanosecond
        // fingerprint and size) before the render completed.
        insert_scanned_asset(&conn, "C:\\media\\a.jpg", 200, 5, 5_999_999_999, None);

        let outcome = update_asset_thumbnail_path_if_version_matches(
            &conn,
            1,
            Some("C:\\thumbs\\stale.jpg"),
            &stale_version,
        )
        .expect("cas stale");
        assert_eq!(outcome, ThumbnailCasOutcome::VersionMismatch);

        let stored: Option<String> = conn
            .query_row("SELECT thumb_path FROM assets WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("read thumb path");
        assert_eq!(
            stored, None,
            "a stale-version result must never be published"
        );
    }

    #[test]
    fn thumbnail_batch_cas_reports_stale_ids_for_cleanup() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_scanned_asset(&conn, "C:\\media\\a.jpg", 10, 1, 1_000_000_000, None);
        insert_scanned_asset(&conn, "C:\\media\\b.jpg", 20, 2, 2_000_000_000, None);

        let fresh = crate::thumbs::SourceVersion::new("C:\\media\\a.jpg", 10, 1_000_000_000);
        let stale = crate::thumbs::SourceVersion::new("C:\\media\\b.jpg", 20, 9_999_999_999);
        let stale_ids = update_asset_thumbnail_paths_batch_versioned(
            &conn,
            &[
                (1, Some("C:\\thumbs\\a.jpg".to_string()), fresh),
                (2, Some("C:\\thumbs\\b-stale.jpg".to_string()), stale),
            ],
        )
        .expect("batch cas");

        assert_eq!(stale_ids, vec![2]);
    }

    #[test]
    fn rescan_clears_thumb_reference_when_precise_fingerprint_changes_within_same_second() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_scanned_asset(&conn, "C:\\media\\a.jpg", 10, 7, 7_000_000_001, None);
        update_asset_thumbnail_path_if_version_matches(
            &conn,
            1,
            Some("C:\\thumbs\\old.jpg"),
            &crate::thumbs::SourceVersion::new("C:\\media\\a.jpg", 10, 7_000_000_001),
        )
        .expect("store old thumb");

        // Same seconds-resolution modified_at, changed file contents.
        insert_scanned_asset(&conn, "C:\\media\\a.jpg", 30, 7, 7_500_000_000, None);

        let stored: Option<String> = conn
            .query_row("SELECT thumb_path FROM assets WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("read thumb path");
        assert_eq!(
            stored, None,
            "same-second modification must release the stale thumbnail reference"
        );
    }

    #[test]
    fn delete_asset_by_id_returns_paths_and_removes_row() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a.jpg", "image", 1);
        conn.execute(
            "UPDATE assets SET thumb_path = ?1 WHERE id = ?2",
            params!["thumb-a.jpg", 1],
        )
        .expect("set thumb");

        let result = delete_asset_by_id_with_thumb(&conn, 1).expect("delete by id");
        assert_eq!(
            result,
            Some((
                "C:\\media\\a.jpg".to_string(),
                Some("thumb-a.jpg".to_string())
            ))
        );

        let page = list_assets(&conn, 0, 10, &[], &[], None, false).expect("list after delete");
        assert!(page.items.is_empty());
    }

    #[test]
    fn delete_asset_by_id_returns_none_for_missing_asset() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        let result = delete_asset_by_id_with_thumb(&conn, 999).expect("delete missing");
        assert!(result.is_none());
    }

    #[test]
    fn duplicate_file_name_queries_return_expected_groups() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\a\\same.jpg", "image", 10);
        insert_asset(&conn, "C:\\media\\b\\same.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\c\\other.jpg", "image", 30);

        let groups = list_duplicate_file_name_counts(&conn).expect("duplicate groups");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].file_name_key, "same.jpg");
        assert_eq!(groups[0].asset_count, 2);

        let assets =
            list_duplicate_assets_by_file_name_key(&conn, "same.jpg").expect("duplicate assets");
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].path, "C:\\media\\b\\same.jpg");
        assert_eq!(assets[1].path, "C:\\media\\a\\same.jpg");
    }

    #[test]
    fn rename_asset_file_updates_path_and_clears_thumbnail_reference() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");

        insert_asset(&conn, "C:\\media\\same.jpg", "image", 1);
        conn.execute(
            "UPDATE assets SET thumb_path = ?1 WHERE id = ?2",
            params!["thumb-same.jpg", 1],
        )
        .expect("set thumb");

        let result = rename_asset_file_by_id(&conn, 1, "C:\\media\\renamed.jpg", "renamed.jpg")
            .expect("rename in db");
        assert_eq!(
            result,
            Some((
                "C:\\media\\same.jpg".to_string(),
                Some("thumb-same.jpg".to_string())
            ))
        );

        let updated = get_asset_path_and_thumb_by_id(&conn, 1)
            .expect("query asset")
            .expect("asset exists");
        assert_eq!(updated.0, "C:\\media\\renamed.jpg");
        assert_eq!(updated.1, None);

        let file_name: String = conn
            .query_row("SELECT file_name FROM assets WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("file name");
        assert_eq!(file_name, "renamed.jpg");
    }

    #[test]
    fn try_list_ordered_asset_ids_supports_cooperative_cancellation() {
        let conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("init schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 30);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 20);
        insert_asset(&conn, "C:\\media\\c.jpg", "image", 10);

        let cancelled =
            try_list_ordered_asset_ids_with_meta(&conn, &[], &[], None, false, None, || true)
                .expect("cancelled build");
        assert_eq!(cancelled, None);

        let completed =
            try_list_ordered_asset_ids_with_meta(&conn, &[], &[], None, false, None, || false)
                .expect("completed build");
        assert_eq!(completed.as_deref(), Some(&[1_i64, 2_i64, 3_i64][..]));
    }

    #[test]
    fn bulk_favorite_toggle_handles_mixed_selection_missing_ids_and_revision() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "/tmp/a.png", "image", 1);
        insert_asset(&conn, "/tmp/b.png", "image", 2);
        insert_asset(&conn, "/tmp/c.png", "image", 3);
        set_asset_favorite(&conn, 2, true).expect("favorite");
        let revision = current_library_revision(&conn).expect("revision");
        let result = super::toggle_assets_favorite_bulk(&mut conn, &[1, 2, 2, 999, 0, -1])
            .expect("toggle mixed");
        assert_eq!(result.processed_asset_ids, vec![1, 2]);
        assert!(result.is_favorite);
        assert_eq!(result.revision, revision + 1);
        assert!(
            super::get_asset_details(&conn, 1)
                .unwrap()
                .unwrap()
                .summary
                .is_favorite
        );
        assert!(
            super::get_asset_details(&conn, 2)
                .unwrap()
                .unwrap()
                .summary
                .is_favorite
        );
        assert!(
            !super::get_asset_details(&conn, 3)
                .unwrap()
                .unwrap()
                .summary
                .is_favorite
        );
        let serialized = serde_json::to_value(&result).expect("serialize IPC result");
        assert_eq!(
            serialized,
            serde_json::json!({
                "processed_asset_ids": [1, 2], "is_favorite": true, "revision": revision + 1
            })
        );
        let result =
            super::toggle_assets_favorite_bulk(&mut conn, &[1, 2, 999]).expect("toggle all");
        assert!(!result.is_favorite);
        assert_eq!(result.revision, revision + 2);
        assert!(
            !super::get_asset_details(&conn, 1)
                .unwrap()
                .unwrap()
                .summary
                .is_favorite
        );
        assert!(
            !super::get_asset_details(&conn, 2)
                .unwrap()
                .unwrap()
                .summary
                .is_favorite
        );
        let result = super::toggle_assets_favorite_bulk(&mut conn, &[1]).expect("toggle single");
        assert!(result.is_favorite);
        assert_eq!(result.revision, revision + 3);
        for ids in [&[][..], &[999][..]] {
            let result = super::toggle_assets_favorite_bulk(&mut conn, ids).expect("empty");
            assert!(result.processed_asset_ids.is_empty());
            assert_eq!(result.revision, revision + 3);
        }
    }

    #[test]
    fn bulk_favorite_toggle_rolls_back_rows_and_revision_on_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut conn = super::open_connection(&dir.path().join("media.db")).expect("db");
        init_schema(&conn).expect("schema");
        insert_asset(&conn, "/tmp/a.png", "image", 1);
        insert_asset(&conn, "/tmp/b.png", "image", 2);
        let revision = current_library_revision(&conn).expect("revision");
        conn.execute_batch(
            "CREATE TRIGGER reject_second_favorite BEFORE UPDATE OF is_favorite ON assets
            WHEN NEW.id = 2 BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
        )
        .expect("trigger");
        assert!(super::toggle_assets_favorite_bulk(&mut conn, &[1, 2]).is_err());
        assert!(
            !super::get_asset_details(&conn, 1)
                .unwrap()
                .unwrap()
                .summary
                .is_favorite
        );
        assert!(
            !super::get_asset_details(&conn, 2)
                .unwrap()
                .unwrap()
                .summary
                .is_favorite
        );
        assert_eq!(current_library_revision(&conn).unwrap(), revision);
    }

    #[test]
    fn favorite_and_group_mutations_bump_revision_in_the_same_transaction() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("init schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 10);

        let baseline = current_library_revision(&conn).expect("baseline revision");

        set_asset_favorite_with_revision(&mut conn, 1, true).expect("favorite");
        let after_favorite = current_library_revision(&conn).expect("revision after favorite");
        assert_eq!(after_favorite, baseline + 1);
        assert_eq!(
            list_assets_with_meta(&conn, 0, 50, &[], &[], None, true, None)
                .expect("favorites page")
                .total,
            1
        );

        set_asset_media_group_with_revision(&mut conn, 1, Some("trip"), Some(1.0)).expect("group");
        let after_group = current_library_revision(&conn).expect("revision after group");
        assert_eq!(after_group, baseline + 2);
    }

    #[test]
    fn bulk_media_group_bumps_revision_only_when_rows_change_in_one_transaction() {
        let mut conn = Connection::open_in_memory().expect("db");
        init_schema(&conn).expect("init schema");
        insert_asset(&conn, "C:\\media\\a.jpg", "image", 10);
        insert_asset(&conn, "C:\\media\\b.jpg", "image", 9);

        let baseline = current_library_revision(&conn).expect("baseline revision");

        let (processed, updated) =
            set_assets_media_group_bulk(&mut conn, &[(999, Some(1.0))], Some("trip"))
                .expect("bulk with only missing ids");
        assert_eq!((processed, updated), (vec![], 0));
        assert_eq!(
            current_library_revision(&conn).expect("noop revision"),
            baseline
        );

        let (processed, updated) =
            set_assets_media_group_bulk(&mut conn, &[(1, Some(1.0)), (2, Some(2.0))], Some("trip"))
                .expect("bulk change");
        assert_eq!((processed, updated), (vec![1, 2], 2));
        assert_eq!(
            current_library_revision(&conn).expect("bumped revision"),
            baseline + 1
        );
    }

    #[test]
    fn tag_impacts_use_actual_sets_and_bulk_unions() {
        use crate::models::TagQueryImpact;
        let mut conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        for id in 1..=3 {
            insert_asset(&conn, &format!("/library/{id}.png"), "image", id);
        }
        let first = set_asset_tags_with_revision(
            &mut conn,
            1,
            &[" CAT ".into(), "dog".into(), "cat".into()],
        )
        .unwrap();
        assert_eq!(
            first.query_impact,
            TagQueryImpact::Tags {
                changed_tags: vec!["cat".into(), "dog".into()],
                tag_count_changed: true
            }
        );
        let noop =
            set_asset_tags_with_revision(&mut conn, 1, &["DOG".into(), "cat".into()]).unwrap();
        assert_eq!(noop.query_impact, TagQueryImpact::None);
        assert_eq!(noop.revision, first.revision);
        let replacement =
            set_asset_tags_with_revision(&mut conn, 1, &["cat".into(), "żółć|%_:'新".into()])
                .unwrap();
        assert_eq!(
            replacement.query_impact,
            TagQueryImpact::Tags {
                changed_tags: vec!["dog".into(), "żółć|%_:'新".into()],
                tag_count_changed: false
            }
        );
        set_asset_tags_with_revision(&mut conn, 2, &["cat".into()]).unwrap();
        let (results, _, impact) =
            merge_asset_tags_bulk_with_revision(&mut conn, &[1, 2, 999], &["żółć|%_:'新".into()])
                .unwrap();
        assert!(!results[0].changed);
        assert!(results[1].changed);
        assert_eq!(
            impact,
            TagQueryImpact::Tags {
                changed_tags: vec!["żółć|%_:'新".into()],
                tag_count_changed: true
            }
        );
        conn.execute("UPDATE assets SET tag_count = 99 WHERE id = 2", [])
            .unwrap();
        let (_, _, impact) =
            merge_asset_tags_bulk_with_revision(&mut conn, &[1, 2], &["cat".into()]).unwrap();
        assert_eq!(impact, TagQueryImpact::All);
        for (impact, value) in [
            (TagQueryImpact::None, serde_json::json!({"type":"none"})),
            (TagQueryImpact::All, serde_json::json!({"type":"all"})),
            (
                TagQueryImpact::Tags {
                    changed_tags: vec!["cat".into()],
                    tag_count_changed: false,
                },
                serde_json::json!({"type":"tags","changed_tags":["cat"],"tag_count_changed":false}),
            ),
        ] {
            assert_eq!(serde_json::to_value(impact).unwrap(), value);
        }
    }

    #[test]
    fn marker_write_failure_rolls_back_metadata_and_global_revision() {
        for operation in [
            "single_tag",
            "bulk_tag",
            "favorite",
            "bulk_favorite",
            "general",
        ] {
            let mut conn = Connection::open_in_memory().unwrap();
            init_schema(&conn).unwrap();
            insert_asset(&conn, "/library/1.png", "image", 1);
            let revision = current_library_revision(&conn).unwrap();
            conn.execute_batch(
                "CREATE TRIGGER reject_marker BEFORE INSERT ON library_metadata
                WHEN substr(NEW.key, 1, 15) = 'query_revision:'
                BEGIN SELECT RAISE(ABORT, 'marker failure'); END;",
            )
            .unwrap();
            let failed = match operation {
                "single_tag" => {
                    set_asset_tags_with_revision(&mut conn, 1, &["cat".into()]).is_err()
                }
                "bulk_tag" => {
                    merge_asset_tags_bulk_with_revision(&mut conn, &[1], &["cat".into()]).is_err()
                }
                "favorite" => set_asset_favorite_with_revision(&mut conn, 1, true).is_err(),
                "bulk_favorite" => super::toggle_assets_favorite_bulk(&mut conn, &[1]).is_err(),
                _ => super::bump_library_revision(&conn).is_err(),
            };
            assert!(failed, "{operation}");
            assert_eq!(current_library_revision(&conn).unwrap(), revision);
            let detail = super::get_asset_details(&conn, 1).unwrap().unwrap();
            assert!(!detail.summary.is_favorite);
            assert!(detail.tags.is_empty());
            assert_eq!(
                super::query_dependency_revision(
                    &conn,
                    &[
                        "general".into(),
                        "favorites".into(),
                        "tag_count".into(),
                        "tag:cat".into()
                    ]
                )
                .unwrap(),
                revision
            );
        }
    }

    #[test]
    fn initialization_rebuilds_only_dependency_namespace_and_batches_exact_keys() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        insert_asset(&conn, "/library/1.png", "image", 1);
        set_asset_tags_with_revision(&mut conn, 1, &["cat".into()]).unwrap();
        conn.execute(
            "INSERT INTO library_metadata(key, value) VALUES ('extension', 77)",
            [],
        )
        .unwrap();
        for absent in [true, false] {
            conn.execute(
                "DELETE FROM library_metadata WHERE substr(key,1,15) = 'query_revision:'",
                [],
            )
            .unwrap();
            if !absent {
                conn.execute_batch("INSERT INTO library_metadata VALUES ('query_revision:general', 9999), ('query_revision:obsolete', 9999), ('query_revision:tag:cat', 9999)").unwrap();
            }
            for _ in 0..2 {
                init_schema(&conn).unwrap();
                let revision = current_library_revision(&conn).unwrap();
                let mut stmt = conn.prepare("SELECT key,value FROM library_metadata WHERE substr(key,1,15) = 'query_revision:' ORDER BY key").unwrap();
                let markers = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                assert_eq!(
                    markers,
                    vec![
                        ("query_revision:favorites".into(), revision),
                        ("query_revision:general".into(), revision),
                        ("query_revision:tag_count".into(), revision)
                    ]
                );
                assert_eq!(
                    conn.query_row(
                        "SELECT value FROM library_metadata WHERE key='extension'",
                        [],
                        |row| row.get::<_, i64>(0)
                    )
                    .unwrap(),
                    77
                );
                assert_eq!(
                    super::get_asset_details(&conn, 1).unwrap().unwrap().tags,
                    vec!["cat"]
                );
            }
        }
        let names = (0..1200)
            .map(|n| format!("tag:tag-{n}"))
            .collect::<Vec<_>>();
        conn.execute(
            "INSERT INTO library_metadata VALUES ('query_revision:tag:tag-1199', 123)",
            [],
        )
        .unwrap();
        assert_eq!(
            super::query_dependency_revision(&conn, &names).unwrap(),
            123
        );
        conn.execute("INSERT INTO library_metadata VALUES ('query_revision:tag:a_b', 222), ('query_revision:tag:axb', 333)", []).unwrap();
        assert_eq!(
            super::query_dependency_revision(&conn, &["tag:a_b".into()]).unwrap(),
            222
        );
        assert_eq!(
            super::query_dependency_revision(&conn, &["tag:%".into()]).unwrap(),
            0
        );
    }
}
