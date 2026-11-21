//! Metadata writes as data (`createMutations` of the Solid frontend).
//!
//! A [`Mutation`] is a command of the user: replace or change the tags of one asset,
//! add tags to several, set or toggle favorites, set media groups, delete an asset,
//! or one of the three writes on a selection spec. The library model
//! ([`Library`](super::Library)) admits it to the write queue of
//! [`AssetTagState`](super::AssetTagState), sends its [`WriteRequest`] when its turn
//! came, and publishes from the backend's [`WriteReply`] only: [`patches`] are the
//! canonical values of the assets the backend acknowledged, never the values that
//! were asked for on assets it did not name.
//!
//! This file holds the data and the pure rules between them; the sequencing lives in
//! `library_model.rs`.

use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter},
};

use media_tagger_core::models::{
    AssetDetails, BulkFavoriteSummary, BulkMediaGroupSummary, BulkTagMergeSummary,
    DeleteAssetSummary, SelectionMutationResult, SetAssetTagsSummary,
};

use super::{page_cache::SummaryPatch, tag_state::TagStateError};
use crate::backend::{
    Backend, BackendError, BackendResult, MediaGroupUpdate, SelectionMutation, SelectionSpec,
};

/// The canonical values a write changed on one asset. Unset fields stay.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetadataPatch {
    /// The complete tag list. Summary rows never carry it.
    pub tags: Option<Vec<String>>,
    /// The fields a query row shares with the details.
    pub summary: SummaryPatch,
}

impl MetadataPatch {
    /// `details` with this patch applied.
    pub fn apply(&self, details: &AssetDetails) -> AssetDetails {
        let mut patched = details.clone();
        if let Some(tags) = &self.tags {
            patched.tags = tags.clone();
        }
        self.summary.apply(&mut patched.summary);
        patched
    }
}

/// What a write published, for models that keep their own copies (`subscribe` of
/// `createMutations`).
#[derive(Debug, Clone, PartialEq)]
pub enum MetadataCommit {
    /// The acknowledged assets with their canonical values, in the write's order.
    Updated(Vec<(i64, MetadataPatch)>),
    Deleted(Vec<i64>),
}

/// How [`Mutation::SaveTags`] gets the complete list it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagEdit {
    /// This list, as the editor holds it.
    Replace(Vec<String>),
    /// The canonical tags as they are when the write reaches the head of the asset's
    /// queue, without `remove` and with the tags of `add` they do not hold yet
    /// appended. A queued edit of this form cannot undo the write before it.
    Change {
        add: Vec<String>,
        remove: Vec<String>,
    },
}

impl TagEdit {
    /// The complete list to write over `canonical`.
    pub fn resolve(&self, canonical: &[String]) -> Vec<String> {
        match self {
            Self::Replace(tags) => tags.clone(),
            Self::Change { add, remove } => {
                let mut next: Vec<String> = canonical
                    .iter()
                    .filter(|tag| !remove.contains(tag))
                    .cloned()
                    .collect();
                for tag in add {
                    if !next.contains(tag) {
                        next.push(tag.clone());
                    }
                }
                next
            }
        }
    }
}

/// A metadata write of the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    /// Replaces the complete tag list of one asset (`set_asset_tags`). Refused while
    /// the asset's complete tag list is not known.
    SaveTags {
        asset_id: i64,
        edit: TagEdit,
    },
    /// Adds tags to every asset; never removes any (`merge_asset_tags_bulk`).
    MergeTags {
        asset_ids: Vec<i64>,
        tags: Vec<String>,
    },
    SetFavorite {
        asset_id: i64,
        favorite: bool,
    },
    /// Favorites all assets unless all already are, then clears all.
    ToggleFavorites {
        asset_ids: Vec<i64>,
    },
    /// A blank key clears the group.
    SetMediaGroup {
        asset_id: i64,
        key: Option<String>,
        order: Option<f64>,
    },
    SetMediaGroups {
        updates: Vec<MediaGroupUpdate>,
        key: Option<String>,
    },
    DeleteAsset {
        asset_id: i64,
    },
    /// Adds tags to every asset of a selection spec.
    MergeTagsInSelection {
        selection: SelectionSpec,
        tags: Vec<String>,
    },
    /// Favorites a selection spec unless all of it already is, then clears it.
    ToggleFavoritesInSelection {
        selection: SelectionSpec,
    },
    /// Groups a selection spec in selection order; a blank key clears its groups.
    SetMediaGroupInSelection {
        selection: SelectionSpec,
        key: Option<String>,
    },
}

/// `key?.trim() || null`.
fn group_key(key: Option<String>) -> Option<String> {
    key.map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
}

/// Each ID once, in the order of its first appearance.
fn unique(ids: impl IntoIterator<Item = i64>) -> Vec<i64> {
    let mut seen = HashSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).collect()
}

impl Mutation {
    /// The command as it is queued: asset IDs once each in their first order, the
    /// group key of a single-asset or selection write trimmed, blank as no group.
    pub(super) fn normalized(self) -> Self {
        match self {
            Self::MergeTags { asset_ids, tags } => Self::MergeTags {
                asset_ids: unique(asset_ids),
                tags,
            },
            Self::ToggleFavorites { asset_ids } => Self::ToggleFavorites {
                asset_ids: unique(asset_ids),
            },
            Self::SetMediaGroup {
                asset_id,
                key,
                order,
            } => Self::SetMediaGroup {
                asset_id,
                key: group_key(key),
                order,
            },
            Self::SetMediaGroupInSelection { selection, key } => Self::SetMediaGroupInSelection {
                selection,
                key: group_key(key),
            },
            other => other,
        }
    }

    /// The assets whose queues the write waits in; `None` for a write on a selection
    /// spec, whose assets only the backend knows.
    pub(super) fn asset_ids(&self) -> Option<Vec<i64>> {
        match self {
            Self::SaveTags { asset_id, .. }
            | Self::SetFavorite { asset_id, .. }
            | Self::SetMediaGroup { asset_id, .. }
            | Self::DeleteAsset { asset_id } => Some(vec![*asset_id]),
            Self::MergeTags { asset_ids, .. } | Self::ToggleFavorites { asset_ids } => {
                Some(asset_ids.clone())
            }
            Self::SetMediaGroups { updates, .. } => {
                Some(unique(updates.iter().map(|update| update.asset_id)))
            }
            Self::MergeTagsInSelection { .. }
            | Self::ToggleFavoritesInSelection { .. }
            | Self::SetMediaGroupInSelection { .. } => None,
        }
    }
}

/// One backend call of a write.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteRequest {
    SetAssetTags {
        asset_id: i64,
        tags: Vec<String>,
    },
    MergeAssetTagsBulk {
        asset_ids: Vec<i64>,
        tags: Vec<String>,
    },
    SetAssetFavorite {
        asset_id: i64,
        favorite: bool,
    },
    ToggleAssetsFavoriteBulk {
        asset_ids: Vec<i64>,
    },
    SetAssetMediaGroup {
        asset_id: i64,
        key: Option<String>,
        order: Option<f64>,
    },
    SetAssetsMediaGroupBulk {
        updates: Vec<MediaGroupUpdate>,
        key: Option<String>,
    },
    DeleteAsset {
        asset_id: i64,
    },
    ApplySelectionMutation {
        selection: SelectionSpec,
        mutation: SelectionMutation,
    },
}

impl WriteRequest {
    /// The port method the request calls.
    pub fn operation(&self) -> &'static str {
        match self {
            Self::SetAssetTags { .. } => "set_asset_tags",
            Self::MergeAssetTagsBulk { .. } => "merge_asset_tags_bulk",
            Self::SetAssetFavorite { .. } => "set_asset_favorite",
            Self::ToggleAssetsFavoriteBulk { .. } => "toggle_assets_favorite_bulk",
            Self::SetAssetMediaGroup { .. } => "set_asset_media_group",
            Self::SetAssetsMediaGroupBulk { .. } => "set_assets_media_group_bulk",
            Self::DeleteAsset { .. } => "delete_asset",
            Self::ApplySelectionMutation { .. } => "apply_selection_mutation",
        }
    }

    /// Calls the backend. Blocking: the runner runs it on the executor.
    pub fn run(self, backend: &dyn Backend) -> BackendResult<WriteReply> {
        Ok(match self {
            Self::SetAssetTags { asset_id, tags } => {
                WriteReply::TagsSet(backend.set_asset_tags(asset_id, tags)?)
            }
            Self::MergeAssetTagsBulk { asset_ids, tags } => {
                WriteReply::TagsMerged(backend.merge_asset_tags_bulk(asset_ids, tags)?)
            }
            Self::SetAssetFavorite { asset_id, favorite } => {
                backend.set_asset_favorite(asset_id, favorite)?;
                WriteReply::Done
            }
            Self::ToggleAssetsFavoriteBulk { asset_ids } => {
                WriteReply::FavoritesToggled(backend.toggle_assets_favorite_bulk(asset_ids)?)
            }
            Self::SetAssetMediaGroup {
                asset_id,
                key,
                order,
            } => {
                backend.set_asset_media_group(asset_id, key, order)?;
                WriteReply::Done
            }
            Self::SetAssetsMediaGroupBulk { updates, key } => {
                WriteReply::MediaGroupsSet(backend.set_assets_media_group_bulk(updates, key)?)
            }
            Self::DeleteAsset { asset_id } => WriteReply::Deleted(backend.delete_asset(asset_id)?),
            Self::ApplySelectionMutation {
                selection,
                mutation,
            } => WriteReply::Selection(backend.apply_selection_mutation(selection, mutation)?),
        })
    }
}

/// The backend's answer to a [`WriteRequest`].
#[derive(Debug)]
pub enum WriteReply {
    TagsSet(SetAssetTagsSummary),
    TagsMerged(BulkTagMergeSummary),
    /// `set_asset_favorite` and `set_asset_media_group` answer without a value.
    Done,
    FavoritesToggled(BulkFavoriteSummary),
    MediaGroupsSet(BulkMediaGroupSummary),
    Deleted(DeleteAssetSummary),
    Selection(SelectionMutationResult),
}

/// The write as it was sent, kept until the backend answered.
#[derive(Debug)]
pub(super) enum Sent {
    SaveTags {
        requested: Vec<String>,
    },
    MergeTags {
        requested: usize,
    },
    SetFavorite {
        asset_id: i64,
        favorite: bool,
    },
    ToggleFavorites {
        requested: usize,
    },
    SetMediaGroup {
        asset_id: i64,
        key: Option<String>,
        order: Option<f64>,
    },
    SetMediaGroups {
        updates: Vec<MediaGroupUpdate>,
    },
    DeleteAsset {
        asset_id: i64,
    },
}

/// The backend call of a write on assets and what to remember of it. `canonical` is
/// the complete tag list of a [`Mutation::SaveTags`] asset when the write starts.
pub(super) fn request_of(
    mutation: Mutation,
    canonical: Option<&[String]>,
) -> Result<(WriteRequest, Sent), MutationError> {
    Ok(match mutation {
        Mutation::SaveTags { asset_id, edit } => {
            let tags = edit.resolve(canonical.ok_or(MutationError::TagsNotLoaded)?);
            (
                WriteRequest::SetAssetTags {
                    asset_id,
                    tags: tags.clone(),
                },
                Sent::SaveTags { requested: tags },
            )
        }
        Mutation::MergeTags { asset_ids, tags } => {
            let requested = asset_ids.len();
            (
                WriteRequest::MergeAssetTagsBulk { asset_ids, tags },
                Sent::MergeTags { requested },
            )
        }
        Mutation::SetFavorite { asset_id, favorite } => (
            WriteRequest::SetAssetFavorite { asset_id, favorite },
            Sent::SetFavorite { asset_id, favorite },
        ),
        Mutation::ToggleFavorites { asset_ids } => {
            let requested = asset_ids.len();
            (
                WriteRequest::ToggleAssetsFavoriteBulk { asset_ids },
                Sent::ToggleFavorites { requested },
            )
        }
        Mutation::SetMediaGroup {
            asset_id,
            key,
            order,
        } => (
            WriteRequest::SetAssetMediaGroup {
                asset_id,
                key: key.clone(),
                order,
            },
            Sent::SetMediaGroup {
                asset_id,
                key,
                order,
            },
        ),
        Mutation::SetMediaGroups { updates, key } => (
            WriteRequest::SetAssetsMediaGroupBulk {
                updates: updates.clone(),
                key,
            },
            Sent::SetMediaGroups { updates },
        ),
        Mutation::DeleteAsset { asset_id } => (
            WriteRequest::DeleteAsset { asset_id },
            Sent::DeleteAsset { asset_id },
        ),
        Mutation::MergeTagsInSelection { .. }
        | Mutation::ToggleFavoritesInSelection { .. }
        | Mutation::SetMediaGroupInSelection { .. } => return Err(MutationError::unexpected()),
    })
}

/// The backend call of a write on a selection spec.
pub(super) fn selection_request_of(mutation: Mutation) -> Option<WriteRequest> {
    let (selection, mutation) = match mutation {
        Mutation::MergeTagsInSelection { selection, tags } => {
            (selection, SelectionMutation::MergeTags { tags })
        }
        Mutation::ToggleFavoritesInSelection { selection } => {
            (selection, SelectionMutation::ToggleFavorite)
        }
        Mutation::SetMediaGroupInSelection { selection, key } => (
            selection,
            SelectionMutation::SetMediaGroup {
                media_group_key: key,
            },
        ),
        _ => return None,
    };
    Some(WriteRequest::ApplySelectionMutation {
        selection,
        mutation,
    })
}

fn favorite(is_favorite: bool) -> MetadataPatch {
    MetadataPatch {
        tags: None,
        summary: SummaryPatch {
            is_favorite: Some(is_favorite),
            ..SummaryPatch::default()
        },
    }
}

fn group(key: Option<String>, order: Option<f64>) -> MetadataPatch {
    MetadataPatch {
        tags: None,
        summary: SummaryPatch {
            media_group_key: Some(key),
            media_group_order: Some(order),
            ..SummaryPatch::default()
        },
    }
}

fn tags(tags: &[String]) -> MetadataPatch {
    MetadataPatch {
        tags: Some(tags.to_vec()),
        summary: SummaryPatch::default(),
    }
}

/// The canonical values per asset the backend acknowledged. An asset the reply does
/// not name gets no patch, whatever was asked for. `None`: the reply does not belong
/// to the write.
pub(super) fn patches(sent: &Sent, reply: &WriteReply) -> Option<Vec<(i64, MetadataPatch)>> {
    Some(match (sent, reply) {
        (Sent::SaveTags { .. }, WriteReply::TagsSet(summary)) => {
            vec![(summary.asset_id, tags(&summary.tags))]
        }
        (Sent::MergeTags { .. }, WriteReply::TagsMerged(summary)) => summary
            .results
            .iter()
            .map(|item| (item.asset_id, tags(&item.tags)))
            .collect(),
        (
            Sent::SetFavorite {
                asset_id,
                favorite: is_favorite,
            },
            WriteReply::Done,
        ) => vec![(*asset_id, favorite(*is_favorite))],
        (Sent::ToggleFavorites { .. }, WriteReply::FavoritesToggled(summary)) => summary
            .processed_asset_ids
            .iter()
            .map(|id| (*id, favorite(summary.is_favorite)))
            .collect(),
        (
            Sent::SetMediaGroup {
                asset_id,
                key,
                order,
            },
            WriteReply::Done,
        ) => vec![(*asset_id, group(key.clone(), *order))],
        (Sent::SetMediaGroups { updates }, WriteReply::MediaGroupsSet(summary)) => {
            // The last order given for an asset counts.
            let orders: HashMap<i64, Option<f64>> = updates
                .iter()
                .map(|update| (update.asset_id, update.media_group_order))
                .collect();
            summary
                .processed_asset_ids
                .iter()
                .map(|id| {
                    let order = orders.get(id).copied().flatten();
                    (*id, group(summary.media_group_key.clone(), order))
                })
                .collect()
        }
        (Sent::DeleteAsset { .. }, WriteReply::Deleted(_)) => Vec::new(),
        _ => return None,
    })
}

/// How an accepted write ended: the backend's answer.
#[derive(Debug)]
pub enum MutationOutcome {
    TagsSaved {
        /// The complete list that was sent.
        requested: Vec<String>,
        summary: SetAssetTagsSummary,
    },
    TagsMerged(BulkTagMergeSummary),
    FavoriteSet {
        asset_id: i64,
        favorite: bool,
    },
    FavoritesToggled(BulkFavoriteSummary),
    MediaGroupSet {
        asset_id: i64,
        key: Option<String>,
        order: Option<f64>,
    },
    MediaGroupsSet(BulkMediaGroupSummary),
    Deleted(DeleteAssetSummary),
    /// A write on a selection spec. `Stale` wrote nothing.
    Selection(SelectionMutationResult),
}

pub(super) fn outcome_of(sent: Sent, reply: WriteReply) -> Result<MutationOutcome, MutationError> {
    Ok(match (sent, reply) {
        (Sent::SaveTags { requested }, WriteReply::TagsSet(summary)) => {
            MutationOutcome::TagsSaved { requested, summary }
        }
        (Sent::MergeTags { .. }, WriteReply::TagsMerged(summary)) => {
            MutationOutcome::TagsMerged(summary)
        }
        (Sent::SetFavorite { asset_id, favorite }, WriteReply::Done) => {
            MutationOutcome::FavoriteSet { asset_id, favorite }
        }
        (Sent::ToggleFavorites { .. }, WriteReply::FavoritesToggled(summary)) => {
            MutationOutcome::FavoritesToggled(summary)
        }
        (
            Sent::SetMediaGroup {
                asset_id,
                key,
                order,
            },
            WriteReply::Done,
        ) => MutationOutcome::MediaGroupSet {
            asset_id,
            key,
            order,
        },
        (Sent::SetMediaGroups { .. }, WriteReply::MediaGroupsSet(summary)) => {
            MutationOutcome::MediaGroupsSet(summary)
        }
        (Sent::DeleteAsset { .. }, WriteReply::Deleted(summary)) => {
            MutationOutcome::Deleted(summary)
        }
        _ => return Err(MutationError::unexpected()),
    })
}

/// Why a write did not happen, or is not known to have happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationError {
    /// The write queue refused the write: maintenance, a deleted asset, another
    /// identity, disposal. Nothing was sent.
    Refused(TagStateError),
    /// The complete tag list of the asset is not known, so there is nothing a tag
    /// edit could be applied to. Nothing was sent.
    TagsNotLoaded,
    /// The backend call failed. Nothing was published.
    Backend(BackendError),
}

impl MutationError {
    /// A reply of another write's kind: a defect of the runner, reported as a failed
    /// call.
    pub(super) fn unexpected() -> Self {
        Self::Backend(BackendError::worker_failed(
            "metadata write",
            "the reply does not belong to the write",
        ))
    }
}

impl Display for MutationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(error) => Display::fmt(error, f),
            Self::TagsNotLoaded => f.write_str("The complete tag list is not loaded."),
            Self::Backend(error) => Display::fmt(error, f),
        }
    }
}

impl std::error::Error for MutationError {}

#[cfg(test)]
mod tests {
    use media_tagger_core::models::{
        AssetTagResult, BulkFavoriteSummary, BulkMediaGroupSummary, BulkTagMergeSummary,
        TagQueryImpact,
    };

    use super::{patches, request_of, Mutation, MutationError, Sent, TagEdit, WriteReply};
    use crate::backend::MediaGroupUpdate;

    fn list(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| (*tag).to_owned()).collect()
    }

    #[test]
    fn a_change_is_applied_to_the_canonical_tags_of_the_moment() {
        let add_dog = TagEdit::Change {
            add: list(&["dog"]),
            remove: Vec::new(),
        };
        assert_eq!(add_dog.resolve(&list(&["cat"])), ["cat", "dog"]);
        // Another writer got there first: its tags stay, nothing is added twice.
        assert_eq!(
            add_dog.resolve(&list(&["cat", "bird", "dog"])),
            ["cat", "bird", "dog"]
        );

        let swap = TagEdit::Change {
            add: list(&["new"]),
            remove: list(&["cat"]),
        };
        assert_eq!(swap.resolve(&list(&["cat", "bird"])), ["bird", "new"]);
        assert_eq!(
            TagEdit::Replace(list(&["only"])).resolve(&list(&["cat"])),
            ["only"]
        );
    }

    #[test]
    fn a_tag_edit_needs_the_complete_tag_list() {
        for edit in [
            TagEdit::Replace(list(&["dog"])),
            TagEdit::Change {
                add: list(&["dog"]),
                remove: Vec::new(),
            },
        ] {
            let refused = request_of(Mutation::SaveTags { asset_id: 1, edit }, None);
            assert!(matches!(refused, Err(MutationError::TagsNotLoaded)));
        }
    }

    #[test]
    fn commands_are_queued_with_unique_ids_and_trimmed_group_keys() {
        assert_eq!(
            Mutation::MergeTags {
                asset_ids: vec![2, 1, 2, 1],
                tags: list(&["dog"]),
            }
            .normalized()
            .asset_ids(),
            Some(vec![2, 1])
        );
        assert_eq!(
            Mutation::SetMediaGroup {
                asset_id: 1,
                key: Some(" Trip ".into()),
                order: Some(2.0),
            }
            .normalized(),
            Mutation::SetMediaGroup {
                asset_id: 1,
                key: Some("Trip".into()),
                order: Some(2.0),
            }
        );
        assert_eq!(
            Mutation::SetMediaGroup {
                asset_id: 1,
                key: Some("   ".into()),
                order: None,
            }
            .normalized(),
            Mutation::SetMediaGroup {
                asset_id: 1,
                key: None,
                order: None,
            }
        );
    }

    #[test]
    fn a_partial_bulk_answer_patches_only_the_acknowledged_assets() {
        let merged = WriteReply::TagsMerged(BulkTagMergeSummary {
            vocabulary_changed: false,
            query_impact: TagQueryImpact::None,
            processed_assets: 1,
            updated_assets: 1,
            processed_asset_ids: vec![1],
            updated_asset_ids: vec![1],
            results: vec![AssetTagResult {
                asset_id: 1,
                changed: true,
                tags: list(&["cat", "dog"]),
            }],
            revision: 2,
        });
        let merged = patches(&Sent::MergeTags { requested: 2 }, &merged).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].0, 1);
        assert_eq!(merged[0].1.tags, Some(list(&["cat", "dog"])));

        let toggled = WriteReply::FavoritesToggled(BulkFavoriteSummary {
            processed_asset_ids: vec![3],
            is_favorite: true,
            revision: 3,
        });
        let toggled = patches(&Sent::ToggleFavorites { requested: 2 }, &toggled).unwrap();
        assert_eq!(toggled.len(), 1);
        assert_eq!(toggled[0].1.summary.is_favorite, Some(true));

        let grouped = WriteReply::MediaGroupsSet(BulkMediaGroupSummary {
            processed_asset_ids: vec![1],
            processed_assets: 1,
            updated_assets: 1,
            media_group_key: Some("Trip".into()),
        });
        let sent = Sent::SetMediaGroups {
            updates: vec![
                MediaGroupUpdate {
                    asset_id: 1,
                    media_group_order: Some(3.0),
                },
                MediaGroupUpdate {
                    asset_id: 2,
                    media_group_order: Some(4.0),
                },
            ],
        };
        let grouped = patches(&sent, &grouped).unwrap();
        assert_eq!(grouped.len(), 1);
        assert_eq!(
            grouped[0].1.summary.media_group_key,
            Some(Some("Trip".into()))
        );
        assert_eq!(grouped[0].1.summary.media_group_order, Some(Some(3.0)));
    }

    #[test]
    fn a_reply_of_another_kind_patches_nothing() {
        assert!(patches(&Sent::DeleteAsset { asset_id: 1 }, &WriteReply::Done).is_none());
    }
}
