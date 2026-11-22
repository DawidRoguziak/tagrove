//! `bulkGroupOrderService.ts` and `parseMediaGroupOrder.ts`: the group key of a draft,
//! the group a selection shares with its saved order, a drop in the order board and an
//! order typed as text.

use std::collections::HashSet;

use media_tagger_core::models::AssetSummary;

use crate::backend::MediaGroupUpdate;

/// What a selection says about its media group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BulkGroupSelectionState {
    /// The group every selected asset is in, trimmed; empty without one.
    pub group_key: String,
    /// The assets are in different groups, or some are in one and some in none.
    pub has_conflicting_groups: bool,
    /// The assets in their saved order when they share a group, else as selected.
    pub ordered_asset_ids: Vec<i64>,
}

/// `normalizeGroupKey`: the key without the whitespace around it.
pub fn normalize_group_key(value: &str) -> &str {
    value.trim()
}

/// `uniqueOrderedAssetIds`: the positive IDs, each once, in their first order.
pub fn unique_ordered_asset_ids(asset_ids: &[i64]) -> Vec<i64> {
    let mut seen = HashSet::new();
    asset_ids
        .iter()
        .copied()
        .filter(|id| *id > 0 && seen.insert(*id))
        .collect()
}

/// `buildBulkMediaGroupAssignments`: orders 1..n in the given order, or no order when
/// the group is being cleared.
pub fn build_bulk_media_group_assignments(
    asset_ids: &[i64],
    has_group_key: bool,
) -> Vec<MediaGroupUpdate> {
    unique_ordered_asset_ids(asset_ids)
        .into_iter()
        .enumerate()
        .map(|(index, asset_id)| MediaGroupUpdate {
            asset_id,
            media_group_order: has_group_key.then_some((index + 1) as f64),
        })
        .collect()
}

/// The group of an asset as groups are compared: trimmed and lowercased.
fn group_identity(asset: &AssetSummary) -> Option<String> {
    let key = normalize_group_key(asset.media_group_key.as_deref().unwrap_or(""));
    (!key.is_empty()).then(|| key.to_lowercase())
}

fn saved_order(asset: &AssetSummary) -> Option<f64> {
    asset.media_group_order.filter(|order| order.is_finite())
}

/// `deriveBulkGroupSelectionState`: the group draft and the order a selection starts
/// with. Assets of one group come in their saved order, the ones without an order
/// after them as selected.
pub fn derive_bulk_group_selection_state(selected: &[&AssetSummary]) -> BulkGroupSelectionState {
    let ids = || selected.iter().map(|asset| asset.id).collect::<Vec<_>>();
    let Some(first) = selected.first() else {
        return BulkGroupSelectionState::default();
    };
    let first_key = normalize_group_key(first.media_group_key.as_deref().unwrap_or(""));
    if selected.len() == 1 {
        return BulkGroupSelectionState {
            group_key: first_key.to_owned(),
            has_conflicting_groups: false,
            ordered_asset_ids: ids(),
        };
    }
    let first_identity = group_identity(first);
    if selected
        .iter()
        .any(|asset| group_identity(asset) != first_identity)
    {
        return BulkGroupSelectionState {
            group_key: String::new(),
            has_conflicting_groups: true,
            ordered_asset_ids: ids(),
        };
    }
    if first_identity.is_none() {
        return BulkGroupSelectionState {
            group_key: String::new(),
            has_conflicting_groups: false,
            ordered_asset_ids: ids(),
        };
    }
    let mut ordered: Vec<(usize, &AssetSummary)> = selected.iter().copied().enumerate().collect();
    // Stable: assets with the same order, and the ones without one, keep the order
    // they were selected in.
    ordered.sort_by(|(left_index, left), (right_index, right)| {
        match (saved_order(left), saved_order(right)) {
            (Some(left), Some(right)) if left != right => left.total_cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => left_index.cmp(right_index),
        }
    });
    BulkGroupSelectionState {
        group_key: first_key.to_owned(),
        has_conflicting_groups: false,
        ordered_asset_ids: ordered.into_iter().map(|(_, asset)| asset.id).collect(),
    }
}

/// `reorderAssetIdsByDrop`: `dragged` taken out and put where `target` is, which
/// lands it after a later target and before an earlier one. An unknown ID, or a drop
/// on itself, changes nothing.
pub fn reorder_asset_ids_by_drop(ordered: &[i64], dragged: i64, target: i64) -> Vec<i64> {
    let from = ordered.iter().position(|id| *id == dragged);
    let to = ordered.iter().position(|id| *id == target);
    let (Some(from), Some(to)) = (from, to) else {
        return ordered.to_vec();
    };
    if from == to {
        return ordered.to_vec();
    }
    let mut next = ordered.to_vec();
    let moved = next.remove(from);
    next.insert(to, moved);
    next
}

/// What a typed group order means (`parseMediaGroupOrder`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupOrderDraft {
    /// The empty draft clears the order.
    Clear,
    /// A positive integer that a JavaScript number holds exactly.
    Order(u64),
    /// Anything else: the draft is refused and the order stays.
    Invalid,
}

/// `Number.MAX_SAFE_INTEGER`.
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// `parseMediaGroupOrder`: ASCII digits only, greater than zero, no larger than
/// `Number.MAX_SAFE_INTEGER`. The draft is not trimmed.
pub fn parse_media_group_order(draft: &str) -> GroupOrderDraft {
    if draft.is_empty() {
        return GroupOrderDraft::Clear;
    }
    if !draft.bytes().all(|byte| byte.is_ascii_digit()) {
        return GroupOrderDraft::Invalid;
    }
    match draft.parse::<u64>() {
        Ok(order) if order > 0 && order <= MAX_SAFE_INTEGER => GroupOrderDraft::Order(order),
        _ => GroupOrderDraft::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use media_tagger_core::models::AssetSummary;

    use super::{
        build_bulk_media_group_assignments, derive_bulk_group_selection_state, normalize_group_key,
        parse_media_group_order, reorder_asset_ids_by_drop, unique_ordered_asset_ids,
        GroupOrderDraft,
    };

    fn asset(id: i64, key: Option<&str>, order: Option<f64>) -> AssetSummary {
        AssetSummary {
            id,
            file_name: format!("{id}.png"),
            preview_path: None,
            kind: "image".into(),
            modified_at: 0,
            width: None,
            height: None,
            duration_ms: None,
            thumb_version: format!("{id:016x}"),
            is_favorite: false,
            media_group_key: key.map(str::to_owned),
            media_group_order: order,
        }
    }

    fn derive(assets: &[AssetSummary]) -> super::BulkGroupSelectionState {
        derive_bulk_group_selection_state(&assets.iter().collect::<Vec<_>>())
    }

    #[test]
    fn keys_are_trimmed_and_ids_kept_once_in_order() {
        assert_eq!(normalize_group_key("  trip-1  "), "trip-1");
        assert_eq!(unique_ordered_asset_ids(&[3, 2, 3, -1, 0, 1]), [3, 2, 1]);
    }

    #[test]
    fn assignments_are_one_based_or_cleared() {
        let orders = |has_key| {
            build_bulk_media_group_assignments(&[11, 7, 11, 5], has_key)
                .into_iter()
                .map(|update| (update.asset_id, update.media_group_order))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            orders(true),
            [(11, Some(1.0)), (7, Some(2.0)), (5, Some(3.0))]
        );
        assert_eq!(orders(false), [(11, None), (7, None), (5, None)]);
    }

    #[test]
    fn a_drop_moves_the_dragged_id_to_the_place_of_its_target() {
        assert_eq!(reorder_asset_ids_by_drop(&[1, 2, 3, 4], 4, 2), [1, 4, 2, 3]);
        assert_eq!(reorder_asset_ids_by_drop(&[1, 2, 3, 4], 1, 3), [2, 3, 1, 4]);
        assert_eq!(reorder_asset_ids_by_drop(&[1, 2, 3], 2, 2), [1, 2, 3]);
        assert_eq!(reorder_asset_ids_by_drop(&[1, 2, 3], 9, 2), [1, 2, 3]);
    }

    #[test]
    fn a_shared_group_is_sorted_by_its_saved_order() {
        let state = derive(&[
            asset(1, Some(" Trip "), Some(3.0)),
            asset(2, Some("trip"), None),
            asset(3, Some("TRIP"), Some(1.0)),
            asset(4, Some("trip"), None),
        ]);
        assert_eq!(state.group_key, "Trip");
        assert!(!state.has_conflicting_groups);
        assert_eq!(state.ordered_asset_ids, [3, 1, 2, 4]);
    }

    #[test]
    fn mixed_groups_conflict_and_keep_the_selected_order() {
        let state = derive(&[asset(1, Some("a"), Some(1.0)), asset(2, None, None)]);
        assert!(state.has_conflicting_groups);
        assert_eq!(state.group_key, "");
        assert_eq!(state.ordered_asset_ids, [1, 2]);

        let ungrouped = derive(&[asset(5, None, None), asset(4, Some("  "), None)]);
        assert!(!ungrouped.has_conflicting_groups);
        assert_eq!(ungrouped.ordered_asset_ids, [5, 4]);

        assert_eq!(derive(&[]), super::BulkGroupSelectionState::default());
        let single = derive(&[asset(9, Some(" solo "), Some(4.0))]);
        assert_eq!(single.group_key, "solo");
        assert_eq!(single.ordered_asset_ids, [9]);
    }

    #[test]
    fn a_typed_order_is_a_positive_safe_integer_or_empty() {
        assert_eq!(parse_media_group_order(""), GroupOrderDraft::Clear);
        assert_eq!(parse_media_group_order("12"), GroupOrderDraft::Order(12));
        assert_eq!(parse_media_group_order("007"), GroupOrderDraft::Order(7));
        assert_eq!(
            parse_media_group_order("9007199254740991"),
            GroupOrderDraft::Order(9_007_199_254_740_991)
        );
        for invalid in [
            "0",
            "-1",
            "1.5",
            " 1",
            "1e3",
            "9007199254740992",
            "99999999999999999999",
        ] {
            assert_eq!(
                parse_media_group_order(invalid),
                GroupOrderDraft::Invalid,
                "{invalid}"
            );
        }
    }
}
