//! Bulk metadata writes on a selection spec (stage 09, ADR 0008).
//!
//! The frontend sends a `SelectionSpec` (ranges of its query session plus
//! small ID sets) instead of materialized IDs. The spec is resolved on the
//! session vector in memory, and the write runs in one IMMEDIATE user-write
//! transaction that first checks the session's dependency revision. A changed
//! revision means the user no longer sees the snapshot the ranges refer to:
//! the whole operation is refused as `stale` and nothing is written.

use std::{sync::Arc, time::Instant};

use serde::Deserialize;

use crate::{
    db,
    error::AppResult,
    models::{SelectionMutationResult, TagQueryImpact},
    services::{asset_query_service::SelectionSpec, db_pool::DatabaseRuntime},
    utils::tags::normalize_and_validate_tags,
};

/// The metadata change applied to every selected asset.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SelectionMutation {
    /// Adds tags (never removes any), like `merge_asset_tags_bulk`.
    MergeTags { tags: Vec<String> },
    /// Favorites all selected assets unless all already are, then clears all.
    ToggleFavorite,
    /// Assigns the group key with orders 1..n in selection order, or clears it.
    #[serde(rename_all = "camelCase")]
    SetMediaGroup { media_group_key: Option<String> },
}

enum Normalized {
    MergeTags(Vec<String>),
    ToggleFavorite,
    SetMediaGroup(Option<String>),
}

pub fn apply_selection_mutation(
    database: &Arc<DatabaseRuntime>,
    spec: &SelectionSpec,
    mutation: SelectionMutation,
) -> AppResult<SelectionMutationResult> {
    let mutation = match mutation {
        SelectionMutation::MergeTags { tags } => {
            let tags = normalize_and_validate_tags(tags)?;
            if tags.is_empty() {
                return Err("Enter at least one tag.".into());
            }
            Normalized::MergeTags(tags)
        }
        SelectionMutation::ToggleFavorite => Normalized::ToggleFavorite,
        SelectionMutation::SetMediaGroup { media_group_key } => Normalized::SetMediaGroup(
            media_group_key
                .as_deref()
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(str::to_string),
        ),
    };
    let started = Instant::now();
    let Some(selection) = database.queries.resolve_selection(spec)? else {
        return Ok(SelectionMutationResult::Stale);
    };
    let resolved = started.elapsed();
    let mut conn = database.admit()?.user_write_connection()?;
    let result = db::guarded_user_write(
        &mut conn,
        |tx| selection.is_current(tx),
        |tx| {
            let ids = &selection.asset_ids;
            Ok(match &mutation {
                Normalized::MergeTags(tags) => {
                    let merged = db::merge_tags_into_selection_in_tx(tx, ids, tags)?;
                    SelectionMutationResult::Ready {
                        processed_assets: merged.processed,
                        updated_assets: merged.updated,
                        revision: merged.revision,
                        query_impact: merged.query_impact,
                        vocabulary_changed: merged.vocabulary_changed,
                        is_favorite: None,
                        media_group_key: None,
                    }
                }
                Normalized::ToggleFavorite => {
                    let summary = db::toggle_assets_favorite_bulk_in_tx(tx, ids)?;
                    let processed = summary.processed_asset_ids.len();
                    SelectionMutationResult::Ready {
                        processed_assets: processed,
                        updated_assets: processed,
                        revision: summary.revision,
                        query_impact: TagQueryImpact::None,
                        vocabulary_changed: false,
                        is_favorite: Some(summary.is_favorite),
                        media_group_key: None,
                    }
                }
                Normalized::SetMediaGroup(key) => {
                    let updates: Vec<(i64, Option<f64>)> = ids
                        .iter()
                        .enumerate()
                        .map(|(index, id)| (*id, key.as_ref().map(|_| (index + 1) as f64)))
                        .collect();
                    let (processed, updated) =
                        db::set_assets_media_group_bulk_in_tx(tx, &updates, key.as_deref())?;
                    SelectionMutationResult::Ready {
                        processed_assets: processed.len(),
                        updated_assets: updated,
                        revision: db::current_library_revision(tx)?,
                        query_impact: TagQueryImpact::None,
                        vocabulary_changed: false,
                        is_favorite: None,
                        media_group_key: key.clone(),
                    }
                }
            })
        },
    )?;
    if std::env::var_os("MEDIATAGGER_PERF").is_some() {
        eprintln!(
            "[perf] name=apply_selection_mutation assets={} resolve_ms={:.2} total_ms={:.2} stale={}",
            selection.asset_ids.len(),
            resolved.as_secs_f64() * 1_000.0,
            started.elapsed().as_secs_f64() * 1_000.0,
            result.is_none()
        );
    }
    Ok(result.unwrap_or(SelectionMutationResult::Stale))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn wire_payloads_use_camel_case_arguments_and_snake_case_results() {
        let spec: SelectionSpec = serde_json::from_value(json!({
            "sessionId": 7, "ranges": [[0, 9], [20, 29]], "excluded": [3], "included": [99]
        }))
        .unwrap();
        assert_eq!(spec.session_id, 7);
        assert_eq!(spec.ranges, vec![[0, 9], [20, 29]]);
        assert_eq!((spec.excluded, spec.included), (vec![3], vec![99]));
        let minimal: SelectionSpec =
            serde_json::from_value(json!({"sessionId": 1, "ranges": []})).unwrap();
        assert!(minimal.excluded.is_empty() && minimal.included.is_empty());

        assert!(matches!(
            serde_json::from_value(json!({"type": "mergeTags", "tags": ["a"]})).unwrap(),
            SelectionMutation::MergeTags { tags } if tags == ["a"]
        ));
        assert!(matches!(
            serde_json::from_value(json!({"type": "toggleFavorite"})).unwrap(),
            SelectionMutation::ToggleFavorite
        ));
        assert!(matches!(
            serde_json::from_value(json!({"type": "setMediaGroup", "mediaGroupKey": null}))
                .unwrap(),
            SelectionMutation::SetMediaGroup {
                media_group_key: None
            }
        ));
        assert!(serde_json::from_value::<SelectionMutation>(json!({"type": "delete"})).is_err());

        assert_eq!(
            serde_json::to_value(SelectionMutationResult::Ready {
                processed_assets: 2,
                updated_assets: 1,
                revision: 9,
                query_impact: TagQueryImpact::None,
                vocabulary_changed: false,
                is_favorite: Some(true),
                media_group_key: None,
            })
            .unwrap(),
            json!({
                "status": "ready", "processed_assets": 2, "updated_assets": 1, "revision": 9,
                "query_impact": {"type": "none"}, "vocabulary_changed": false,
                "is_favorite": true, "media_group_key": null
            })
        );
        assert_eq!(
            serde_json::to_value(SelectionMutationResult::Stale).unwrap(),
            json!({"status": "stale"})
        );
    }
}
