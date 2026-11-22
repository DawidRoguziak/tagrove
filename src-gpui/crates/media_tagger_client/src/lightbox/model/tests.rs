//! The lightbox model in the application runtime, against the scripted backend on
//! the deterministic executor. A pending promise of the Solid suite
//! (`createLightbox.test.ts`) is a backend call that waits on the executor.

use std::sync::Arc;

use media_tagger_core::models::{
    AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetSummary, DeleteAssetSummary,
    DeleteSourceStatus, SelectionMutationResult, SetAssetTagsSummary, StartAssetQueryResult,
    TagListPage, TagQueryImpact,
};

use super::{DeletedSource, LightboxCommand, LightboxEvent, NavigationStatus};
use crate::{
    backend::{fake::Call, BackendError, FakeBackend, SelectionSpec},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    library::{
        AssetPatch, LibraryInput, LibraryReset, Mutation, MutationError, QueryInput, SummaryPatch,
        TagEdit, TagStateError, Ticket,
    },
    lightbox::{DeleteConfirm, LightboxView},
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    theme::ThemeName,
};

const PAGE: usize = 128;

fn summary(id: i64) -> AssetSummary {
    AssetSummary {
        id,
        file_name: format!("{id}.png"),
        preview_path: None,
        kind: "image".into(),
        modified_at: 1,
        width: Some(100),
        height: Some(80),
        duration_ms: None,
        thumb_version: "0123456789abcdef".into(),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

fn list(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_owned()).collect()
}

fn details(id: i64, tags: &[&str]) -> AssetDetails {
    AssetDetails {
        summary: summary(id),
        path: format!("/synthetic/{id}.png"),
        size_bytes: 10,
        thumb_path: None,
        tags: list(tags),
    }
}

fn saved(asset_id: i64, tags: Vec<String>) -> SetAssetTagsSummary {
    SetAssetTagsSummary {
        vocabulary_changed: false,
        query_impact: TagQueryImpact::None,
        asset_id,
        changed: true,
        tags,
        revision: 2,
    }
}

fn started(ids: &[i64], total: usize) -> StartAssetQueryResult {
    StartAssetQueryResult::Ready {
        session_id: 1,
        revision: 1,
        total,
        offset: 0,
        items: ids.iter().copied().map(summary).collect(),
    }
}

struct Fx {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl Fx {
    /// A library of `total` assets (ID = index + 1, each tagged `cat`), loaded.
    fn new(total: usize) -> Self {
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(move |_| {
                let ids: Vec<i64> = (1..=total.min(PAGE) as i64).collect();
                Ok(started(&ids, total))
            })
            .on_get_asset_query_page(move |session_id, offset, limit| {
                Ok(AssetQueryPageResult::Ready {
                    session_id,
                    revision: 1,
                    total,
                    offset,
                    items: (offset..(offset + limit).min(total))
                        .map(|index| summary(index as i64 + 1))
                        .collect(),
                })
            })
            .on_get_asset_query_position(|_, asset_id| {
                Ok(AssetQueryPositionResult::Resolved {
                    index: (asset_id - 1) as usize,
                })
            })
            .on_get_asset_details(|id| Ok(Some(details(id, &["cat"]))))
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: list(&["cat", "dog"]),
                    total: 2,
                })
            })
            .on_set_asset_tags(|id, tags| Ok(saved(id, tags)))
            .on_set_asset_favorite(|_, _| Ok(()))
            .on_set_asset_media_group(|_, _, _| Ok(()))
            .on_delete_asset(|_| {
                Ok(DeleteAssetSummary {
                    removed_assets: 1,
                    removed_thumbnails: 0,
                    source_status: DeleteSourceStatus::Deleted,
                    revision: 2,
                    recovery_path: None,
                })
            });
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            }),
            RuntimeRunner::new(backend.clone(), Arc::new(MemoryPrefsStore::default())),
            executor.clone() as Arc<dyn Executor>,
        );
        let mut fx = Self {
            backend,
            executor,
            driver,
        };
        fx.send(RuntimeInput::RefreshLibrary);
        fx.settle();
        fx
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn lightbox(&mut self, command: LightboxCommand) {
        self.send(RuntimeInput::lightbox(command));
    }

    fn select(&mut self, id: i64) {
        self.lightbox(LightboxCommand::Select {
            asset: Box::new(summary(id)),
            index: None,
        });
    }

    fn select_at(&mut self, id: i64, index: usize) {
        self.lightbox(LightboxCommand::Select {
            asset: Box::new(summary(id)),
            index: Some(index),
        });
    }

    /// Selects `id` and lets every read end.
    fn open(&mut self, id: i64) {
        self.select(id);
        self.settle();
    }

    fn save(&mut self, tags: &[&str]) {
        self.lightbox(LightboxCommand::SaveTags(list(tags)));
    }

    /// Runs every waiting call and every reply, including the ones they start.
    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    /// Runs the waiting call at `index` and hands its reply to the runtime; whatever
    /// that starts waits.
    fn answer(&mut self, index: usize) {
        assert!(self.executor.run_nth(index), "a call waits at {index}");
        while self.driver.pump() > 0 {}
    }

    fn view(&self) -> LightboxView<'_> {
        self.driver.model().lightbox()
    }

    fn selected_id(&self) -> Option<i64> {
        self.view().selected().map(|selected| selected.id())
    }

    fn path(&self) -> Option<String> {
        self.view()
            .selected()
            .and_then(|selected| selected.path.clone())
    }

    fn detail_reads(&self, id: i64) -> usize {
        self.backend
            .calls()
            .iter()
            .filter(|call| matches!(call, Call::GetAssetDetails { asset_id } if *asset_id == id))
            .count()
    }

    fn tag_saves(&self) -> Vec<(i64, Vec<String>)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::SetAssetTags { asset_id, tags } => Some((asset_id, tags)),
                _ => None,
            })
            .collect()
    }

    fn favorite_saves(&self) -> Vec<(i64, bool)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::SetAssetFavorite {
                    asset_id,
                    is_favorite,
                } => Some((asset_id, is_favorite)),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn a_summary_opens_at_once_and_tags_cannot_be_replaced_before_the_details_arrive() {
    let mut fx = Fx::new(3);
    fx.select(1);
    fx.save(&["unsafe"]);

    assert_eq!(fx.selected_id(), Some(1));
    assert_eq!(fx.path(), None);
    assert!(fx.view().tag_details_loading());
    assert!(fx.view().tag_editing_disabled());
    assert!(fx.tag_saves().is_empty());

    fx.settle();
    assert!(fx.tag_saves().is_empty());
    assert_eq!(fx.path().as_deref(), Some("/synthetic/1.png"));
    assert_eq!(fx.view().tag_editor(), ["cat"]);
    assert!(!fx.view().tag_editing_disabled());
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
    assert_eq!(fx.view().selected_index(), Some(0));
}

#[test]
fn late_details_replace_neither_another_selection_nor_a_closed_lightbox() {
    let mut fx = Fx::new(3);
    fx.select(1);
    fx.select(2);
    assert_eq!(fx.executor.pending(), 2);

    // The details of the first asset arrive late.
    fx.answer(0);
    assert_eq!(fx.selected_id(), Some(2));
    assert_eq!(fx.path(), None);

    fx.lightbox(LightboxCommand::Close);
    fx.settle();
    assert_eq!(fx.selected_id(), None);
    assert!(!fx.view().is_open());
    assert!(!fx.view().asset_details_failed());
}

#[test]
fn neighbours_are_prefetched_and_cached_details_reselect_without_a_read() {
    let mut fx = Fx::new(5);
    fx.open(1);
    assert_eq!(
        (fx.detail_reads(1), fx.detail_reads(2), fx.detail_reads(5)),
        (1, 1, 1)
    );
    assert_eq!(fx.detail_reads(3) + fx.detail_reads(4), 0);

    fx.lightbox(LightboxCommand::Close);
    fx.select(2);
    assert_eq!(fx.path().as_deref(), Some("/synthetic/2.png"));
    fx.settle();
    assert_eq!(fx.detail_reads(2), 1);
    // Selecting cached details still prefetches the next neighbour.
    assert_eq!(fx.detail_reads(3), 1);
}

#[test]
fn a_result_of_one_row_prefetches_nothing() {
    let mut fx = Fx::new(1);
    fx.open(1);
    assert_eq!(
        fx.backend
            .calls()
            .iter()
            .filter(|call| call.method() == "get_asset_details")
            .count(),
        1
    );
}

#[test]
fn saves_made_while_one_runs_are_sent_as_one_following_write() {
    let mut fx = Fx::new(3);
    fx.open(1);

    fx.save(&["cat", "a"]);
    fx.save(&["cat", "a", "b"]);
    fx.save(&["cat", "final"]);
    assert!(fx.view().tag_saving());
    fx.lightbox(LightboxCommand::Close);
    fx.select(1);
    assert!(fx.view().tag_saving(), "the reopened asset still saves");
    assert_eq!(fx.view().tag_editor(), ["cat", "final"]);

    fx.settle();
    assert_eq!(
        fx.tag_saves(),
        [(1, list(&["cat", "a"])), (1, list(&["cat", "final"]))]
    );
    assert_eq!(fx.view().tag_editor(), ["cat", "final"]);
    assert!(!fx.view().tag_saving());
    assert!(!fx.view().tag_failed());
}

#[test]
fn a_failed_save_keeps_the_newest_tags_for_retry() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.backend
        .on_set_asset_tags(|_, _| Err(BackendError::rejected("failed")));
    fx.save(&["a"]);
    fx.save(&["b"]);
    fx.settle();
    assert!(fx.view().tag_failed());
    assert_eq!(fx.view().tag_editor(), ["b"]);
    assert_eq!(fx.tag_saves().len(), 1);

    fx.backend.on_set_asset_tags(|id, tags| Ok(saved(id, tags)));
    fx.lightbox(LightboxCommand::Close);
    fx.select(1);
    fx.lightbox(LightboxCommand::RetryTags);
    fx.settle();
    assert_eq!(fx.tag_saves().last(), Some(&(1, list(&["b"]))));
    assert!(!fx.view().tag_failed());
}

#[test]
fn a_save_behind_another_writer_is_merged_onto_its_result() {
    let mut fx = Fx::new(3);
    fx.open(1);
    // Another owner of the library writes first; its call waits.
    fx.send(RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(7),
        mutation: Mutation::SaveTags {
            asset_id: 1,
            edit: TagEdit::Replace(list(&["cat", "external"])),
        },
    }));
    fx.save(&["mine"]);
    assert!(fx.view().tag_saving());
    assert_eq!(fx.executor.pending(), 1, "only the other writer was sent");

    fx.answer(0);
    assert!(fx.view().tag_saving());
    assert_eq!(fx.view().tag_editor(), ["external", "mine"]);

    fx.settle();
    assert_eq!(
        fx.tag_saves().last(),
        Some(&(1, list(&["external", "mine"])))
    );
    assert_eq!(fx.tag_saves().len(), 2);
    assert!(!fx.view().tag_failed());
    assert_eq!(fx.view().tag_editor(), ["external", "mine"]);
}

#[test]
fn a_save_does_not_follow_the_selection_to_the_next_asset() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.save(&["cat", "a"]);
    fx.lightbox(LightboxCommand::Next);
    assert_eq!(fx.selected_id(), Some(2));
    assert!(!fx.view().tag_saving());

    fx.settle();
    assert_eq!(fx.selected_id(), Some(2));
    assert_eq!(fx.view().tag_editor(), ["cat"]);
    assert_eq!(fx.driver.model().lightbox().lightbox.retained_intents(), 0);

    fx.lightbox(LightboxCommand::Previous);
    fx.settle();
    assert_eq!(fx.selected_id(), Some(1));
    assert_eq!(fx.view().tag_editor(), ["cat", "a"]);
}

#[test]
fn favorite_toggles_queue_and_the_second_reverses_the_first() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.lightbox(LightboxCommand::ToggleFavorite);
    assert!(fx.view().favorite_pending());
    assert_eq!(fx.executor.pending(), 1);

    fx.answer(0);
    assert!(fx.view().favorite_pending(), "until the last one settles");
    assert!(fx.view().selected().unwrap().summary.is_favorite);

    fx.settle();
    assert_eq!(fx.favorite_saves(), [(1, true), (1, false)]);
    assert!(!fx.view().favorite_pending());
    assert!(!fx.view().favorite_failed());
    assert!(!fx.view().selected().unwrap().summary.is_favorite);
}

#[test]
fn a_refused_favorite_write_is_reported_and_changes_nothing() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.backend
        .on_set_asset_favorite(|_, _| Err(BackendError::rejected("write failed")));
    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.settle();
    assert!(fx.view().favorite_failed());
    assert!(!fx.view().favorite_pending());
    assert!(!fx.view().selected().unwrap().summary.is_favorite);
}

#[test]
fn navigation_wraps_and_rapid_presses_add_up() {
    let mut fx = Fx::new(260);
    fx.select_at(128, 127);
    assert_eq!(fx.view().selected_index(), Some(127));

    // Both rows are on a page that is not cached; the presses add up before it
    // arrives.
    fx.lightbox(LightboxCommand::Next);
    fx.lightbox(LightboxCommand::Next);
    assert_eq!(fx.selected_id(), Some(128));
    fx.settle();
    assert_eq!(fx.selected_id(), Some(130));
    assert_eq!(fx.view().selected_index(), Some(129));

    fx.select_at(1, 0);
    fx.lightbox(LightboxCommand::Previous);
    fx.settle();
    assert_eq!(fx.selected_id(), Some(260));
    fx.lightbox(LightboxCommand::Next);
    fx.settle();
    assert_eq!(fx.selected_id(), Some(1));
}

#[test]
fn a_page_that_fails_rolls_navigation_back_and_retry_reads_it_again() {
    let mut fx = Fx::new(260);
    fx.backend
        .on_get_asset_query_page(|_, _, _| Err(BackendError::rejected("page unavailable")));
    fx.select_at(128, 127);
    fx.settle();

    fx.lightbox(LightboxCommand::Next);
    fx.lightbox(LightboxCommand::Next);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);
    assert_eq!(fx.selected_id(), Some(128));

    // Cached Previous stays usable, and Next steps from the shown asset again.
    fx.lightbox(LightboxCommand::Previous);
    fx.settle();
    assert_eq!(fx.selected_id(), Some(127));
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
    fx.lightbox(LightboxCommand::Next);
    fx.settle();
    assert_eq!(fx.selected_id(), Some(128));
    fx.lightbox(LightboxCommand::Next);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);

    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);

    fx.backend
        .on_get_asset_query_page(|session_id, offset, limit| {
            Ok(AssetQueryPageResult::Ready {
                session_id,
                revision: 1,
                total: 260,
                offset,
                items: (offset..(offset + limit).min(260))
                    .map(|index| summary(index as i64 + 1))
                    .collect(),
            })
        });
    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.selected_id(), Some(129));
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
}

#[test]
fn a_refresh_resolves_the_position_of_the_selected_asset_in_the_new_result() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.backend
        .on_start_asset_query(|_| Ok(started(&[9], 500)))
        .on_get_asset_query_page(|session_id, offset, limit| {
            Ok(AssetQueryPageResult::Ready {
                session_id,
                revision: 1,
                total: 500,
                offset,
                items: (offset..(offset + limit).min(500))
                    .map(|index| summary(index as i64 + 1))
                    .collect(),
            })
        })
        .on_get_asset_query_position(|_, _| Ok(AssetQueryPositionResult::Resolved { index: 300 }));

    fx.send(RuntimeInput::RefreshLibrary);
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Resolving);
    // Keys do nothing while the position is unknown.
    fx.lightbox(LightboxCommand::Next);
    assert_eq!(fx.selected_id(), Some(1));

    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
    assert_eq!(fx.view().selected_index(), Some(300));
    assert_eq!(fx.selected_id(), Some(1));

    fx.lightbox(LightboxCommand::Next);
    fx.settle();
    assert_eq!(fx.view().selected_index(), Some(301));
    assert_eq!(fx.selected_id(), Some(302));
}

#[test]
fn a_missing_asset_stays_open_and_a_failed_lookup_is_retried() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.backend
        .on_start_asset_query(|_| Ok(started(&[], 0)))
        .on_get_asset_query_position(|_, _| Ok(AssetQueryPositionResult::Missing));
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Missing);
    assert_eq!(fx.selected_id(), Some(1));

    fx.backend
        .on_get_asset_query_position(|_, _| Err(BackendError::rejected("position")));
    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);

    fx.backend
        .on_start_asset_query(|_| Ok(started(&[1, 2, 3], 3)));
    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
    assert_eq!(fx.view().selected_index(), Some(0));
}

#[test]
fn a_refresh_that_fails_leaves_navigation_failed_with_retry() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("query failed")));
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);
    assert_eq!(fx.selected_id(), Some(1));

    fx.backend
        .on_start_asset_query(|_| Ok(started(&[1, 2, 3], 3)));
    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
}

#[test]
fn a_deletion_closes_the_selection_and_answers_the_command() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.lightbox(LightboxCommand::DeleteSelected { request: 9 });
    assert_eq!(
        fx.selected_id(),
        Some(1),
        "nothing before the backend answered"
    );
    fx.settle();
    assert_eq!(fx.selected_id(), None);
    let events = fx.driver.model_mut().take_lightbox_events();
    let [LightboxEvent::DeleteSettled {
        request: 9,
        result: Ok(Some(deleted)),
    }] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(deleted.source, DeletedSource::Deleted);
}

#[test]
fn a_refused_deletion_keeps_the_asset_selected() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.backend
        .on_delete_asset(|_| Err(BackendError::rejected("i18n:common.libraryBusy")));
    fx.lightbox(LightboxCommand::DeleteSelected { request: 4 });
    fx.settle();
    assert_eq!(fx.selected_id(), Some(1));
    assert_eq!(
        fx.driver.model_mut().take_lightbox_events(),
        [LightboxEvent::DeleteSettled {
            request: 4,
            result: Err(MutationError::Backend(BackendError::rejected(
                "i18n:common.libraryBusy"
            ))),
        }]
    );

    fx.lightbox(LightboxCommand::Close);
    fx.lightbox(LightboxCommand::DeleteSelected { request: 5 });
    assert_eq!(
        fx.driver.model_mut().take_lightbox_events(),
        [LightboxEvent::DeleteSettled {
            request: 5,
            result: Ok(None),
        }]
    );
    assert_eq!(
        fx.backend
            .calls()
            .iter()
            .filter(|call| call.method() == "delete_asset")
            .count(),
        1
    );
}

#[test]
fn an_identity_reset_closes_the_lightbox_and_an_obsolete_save_publishes_nothing() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.save(&["old"]);
    fx.send(RuntimeInput::library(LibraryInput::Reset(
        LibraryReset::Identity,
    )));
    assert_eq!(fx.selected_id(), None);

    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["new"]))));
    fx.select(1);
    fx.settle();
    assert_eq!(fx.view().tag_editor(), ["new"]);
    assert!(!fx.view().tag_saving());
    assert!(!fx.view().tag_failed());
}

#[test]
fn a_range_write_drops_the_tags_and_the_details_are_read_again_after_it() {
    let mut fx = Fx::new(3);
    fx.open(1);
    let before = fx.detail_reads(1);
    fx.backend.on_apply_selection_mutation(|_, _| {
        Ok(SelectionMutationResult::Ready {
            processed_assets: 3,
            updated_assets: 3,
            revision: 2,
            query_impact: TagQueryImpact::None,
            vocabulary_changed: false,
            is_favorite: Some(true),
            media_group_key: None,
        })
    });
    fx.send(RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(3),
        mutation: Mutation::ToggleFavoritesInSelection {
            selection: SelectionSpec {
                session_id: 1,
                ranges: vec![[0, 2]],
                excluded: Vec::new(),
                included: Vec::new(),
            },
        },
    }));
    assert!(fx.view().tag_editing_disabled());
    assert!(fx.view().tag_details_loading());
    assert_eq!(fx.detail_reads(1), before);

    fx.backend.on_get_asset_details(|id| {
        let mut details = details(id, &["cat", "dog"]);
        details.summary.is_favorite = true;
        Ok(Some(details))
    });
    fx.settle();
    assert_eq!(fx.detail_reads(1), before + 1);
    assert!(!fx.view().tag_editing_disabled());
    assert!(!fx.view().tag_details_loading());
    assert_eq!(fx.view().tag_editor(), ["cat", "dog"]);
    assert!(fx.view().selected().unwrap().summary.is_favorite);
}

#[test]
fn an_edit_overtaken_by_a_range_write_fails_and_retry_sends_it_merged() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.backend.on_apply_selection_mutation(|_, _| {
        Ok(SelectionMutationResult::Ready {
            processed_assets: 3,
            updated_assets: 3,
            revision: 2,
            query_impact: TagQueryImpact::None,
            vocabulary_changed: false,
            is_favorite: None,
            media_group_key: None,
        })
    });
    fx.save(&["cat", "a"]);
    fx.save(&["cat", "a", "b"]);
    assert_eq!(fx.executor.pending(), 1);
    // A range write is admitted behind the running save.
    fx.send(RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(3),
        mutation: Mutation::MergeTagsInSelection {
            selection: SelectionSpec {
                session_id: 1,
                ranges: vec![[0, 2]],
                excluded: Vec::new(),
                included: Vec::new(),
            },
            tags: list(&["dog"]),
        },
    }));
    assert_eq!(
        fx.executor.pending(),
        1,
        "the range write waits for the save"
    );

    // The save ends; the range write starts before the queued edit could be sent.
    fx.answer(0);
    assert_eq!(fx.tag_saves().len(), 1);
    assert!(fx.view().tag_failed());
    assert!(!fx.view().tag_saving());
    assert_eq!(fx.view().tag_editor(), ["cat", "a", "b"]);

    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["a", "cat", "dog"]))));
    fx.settle();
    assert_eq!(fx.tag_saves().len(), 1);
    assert!(fx.view().tag_failed());
    assert!(!fx.view().tag_editing_disabled());
    assert_eq!(fx.view().tag_editor(), ["a", "cat", "dog", "b"]);

    fx.lightbox(LightboxCommand::RetryTags);
    assert!(fx.view().tag_saving());
    assert!(!fx.view().tag_failed());
    fx.settle();
    assert_eq!(
        fx.tag_saves().last(),
        Some(&(1, list(&["a", "cat", "dog", "b"])))
    );
    assert!(!fx.view().tag_saving());
    assert!(!fx.view().tag_failed());
    assert_eq!(fx.view().tag_editor(), ["a", "cat", "dog", "b"]);
}

#[test]
fn dirty_group_drafts_survive_the_details_and_a_tag_save() {
    let mut fx = Fx::new(3);
    fx.backend.on_get_asset_details(|id| {
        let mut details = details(id, &["cat"]);
        details.summary.media_group_key = Some("canonical".into());
        details.summary.media_group_order = Some(4.0);
        Ok(Some(details))
    });
    fx.select(1);
    fx.lightbox(LightboxCommand::SetMediaGroupKeyDraft("draft".into()));
    fx.settle();
    assert_eq!(fx.view().media_group_key_editor(), "draft");
    assert_eq!(fx.view().media_group_order_editor(), "4");

    fx.lightbox(LightboxCommand::SetMediaGroupOrderDraft("x7".into()));
    assert_eq!(fx.view().media_group_order_editor(), "4", "refused whole");
    fx.lightbox(LightboxCommand::SetMediaGroupOrderDraft("07".into()));
    fx.save(&["new"]);
    fx.settle();
    assert_eq!(fx.view().media_group_key_editor(), "draft");
    assert_eq!(fx.view().media_group_order_editor(), "07");

    fx.lightbox(LightboxCommand::SaveMediaGroup);
    assert!(fx.view().group_pending());
    fx.settle();
    assert!(!fx.view().group_pending());
    assert!(!fx.view().group_failed());
    assert!(fx.backend.calls().contains(&Call::SetAssetMediaGroup {
        asset_id: 1,
        media_group_key: Some("draft".into()),
        media_group_order: Some(7.0),
    }));
}

#[test]
fn a_changed_cached_row_updates_the_selection_and_keeps_path_and_tags() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.send(RuntimeInput::Query(QueryInput::PatchAssets(
        AssetPatch::Update(vec![(
            1,
            SummaryPatch {
                media_group_key: Some(Some("new-group".into())),
                ..SummaryPatch::default()
            },
        )]),
    )));
    let selected = fx.view().selected().unwrap().clone();
    assert_eq!(
        selected.summary.media_group_key.as_deref(),
        Some("new-group")
    );
    assert_eq!(selected.path.as_deref(), Some("/synthetic/1.png"));
    assert_eq!(selected.size_bytes, Some(10));
    assert_eq!(fx.view().tag_editor(), ["cat"]);
    assert_eq!(fx.view().media_group_key_editor(), "new-group");
}

#[test]
fn details_that_fail_show_retry_and_a_retry_reads_them_again() {
    let mut fx = Fx::new(3);
    fx.backend
        .on_get_asset_details(|_| Err(BackendError::rejected("read failed")));
    fx.open(1);
    assert!(fx.view().asset_details_failed());
    assert!(fx.view().tag_details_failed());
    assert!(fx.view().tag_editing_disabled());

    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["cat"]))));
    fx.lightbox(LightboxCommand::RetryDetails);
    fx.settle();
    assert!(!fx.view().asset_details_failed());
    assert!(!fx.view().tag_editing_disabled());
}

#[test]
fn details_read_before_a_favorite_write_are_read_again() {
    let mut fx = Fx::new(1);
    fx.select(1);
    assert_eq!(fx.executor.pending(), 1);
    // Another owner favorites the asset while its details are being read.
    fx.send(RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(2),
        mutation: Mutation::SetFavorite {
            asset_id: 1,
            favorite: true,
        },
    }));
    // The write answers first, then the stale details.
    fx.answer(1);
    fx.backend.on_get_asset_details(|id| {
        let mut details = details(id, &["cat"]);
        details.summary.is_favorite = true;
        Ok(Some(details))
    });
    fx.settle();
    let selected = fx.view().selected().unwrap();
    assert!(selected.summary.is_favorite);
    assert_eq!(selected.path.as_deref(), Some("/synthetic/1.png"));
    assert_eq!(fx.detail_reads(1), 2);
    assert!(!fx.view().asset_details_failed());
}

#[test]
fn details_win_over_the_row_they_complete_until_a_refresh_reads_the_row_again() {
    let mut fx = Fx::new(3);
    fx.backend.on_get_asset_details(|id| {
        let mut details = details(id, &["cat"]);
        details.summary.width = Some(999);
        Ok(Some(details))
    });
    fx.open(1);
    let width = |fx: &Fx| fx.view().selected().unwrap().summary.width;
    assert_eq!(width(&fx), Some(999));

    // Another page of rows changes nothing for the shown asset.
    fx.send(RuntimeInput::Query(QueryInput::PatchAssets(
        AssetPatch::Update(vec![(2, SummaryPatch::default())]),
    )));
    assert_eq!(width(&fx), Some(999));

    // A refresh that has not answered still shows the old rows.
    fx.send(RuntimeInput::RefreshLibrary);
    assert_eq!(width(&fx), Some(999));
    fx.settle();
    assert_eq!(width(&fx), Some(100), "the row was read again");
    assert_eq!(fx.path().as_deref(), Some("/synthetic/1.png"));
}

#[test]
fn a_disposed_library_ends_the_lightbox_model() {
    let mut fx = Fx::new(3);
    fx.select(1);
    fx.send(RuntimeInput::library(LibraryInput::Dispose));
    fx.settle();
    assert!(fx.driver.model().lightbox().lightbox.is_disposed());
    assert_eq!(fx.path(), None, "the late details publish nothing");
    fx.lightbox(LightboxCommand::Close);
    assert_eq!(
        fx.selected_id(),
        Some(1),
        "a disposed model takes no command"
    );
}

/// The answer of a disposed model to a deletion.
fn unavailable(request: u64) -> LightboxEvent {
    LightboxEvent::DeleteSettled {
        request,
        result: Err(MutationError::Refused(TagStateError::Unavailable)),
    }
}

/// Every delete command is answered exactly once: a deletion that waits for the
/// backend when the model is disposed is answered at the disposal, so the
/// confirmation that sent it does not stay locked, and the backend's late answer
/// adds nothing.
#[test]
fn a_disposed_model_answers_the_deletion_that_was_on_its_way_exactly_once() {
    let mut fx = Fx::new(3);
    fx.open(1);
    let word = "delete";
    let mut confirm = DeleteConfirm::new();
    confirm.sync(fx.selected_id());
    confirm.open();
    confirm.set_text(word);
    let request = confirm.submit(word).expect("the word was typed");
    fx.lightbox(LightboxCommand::DeleteSelected { request });
    assert_eq!(
        fx.executor.pending(),
        1,
        "the deletion waits for the backend"
    );
    assert!(fx.driver.model_mut().take_lightbox_events().is_empty());
    assert!(confirm.deleting());

    fx.send(RuntimeInput::library(LibraryInput::Dispose));
    let events = fx.driver.model_mut().take_lightbox_events();
    assert_eq!(events, [unavailable(request)]);
    let [LightboxEvent::DeleteSettled { request, result }] = events.as_slice() else {
        unreachable!();
    };
    confirm.settled(*request, result);
    assert!(!confirm.deleting(), "the exits are usable again");
    assert_eq!(fx.driver.model().lightbox().lightbox.pending_writes(), 0);

    // The backend answers late: the model has answered already.
    fx.settle();
    assert_eq!(fx.backend.called_methods().last(), Some(&"delete_asset"));
    assert!(fx.driver.model_mut().take_lightbox_events().is_empty());
}

/// A deletion queued behind another deletion and one asked for after the disposal
/// are answered as well, in the order they were sent, and nothing more is sent.
#[test]
fn a_disposed_model_answers_queued_and_later_deletions_and_sends_nothing() {
    let mut fx = Fx::new(3);
    fx.open(1);
    fx.lightbox(LightboxCommand::DeleteSelected { request: 21 });
    fx.lightbox(LightboxCommand::DeleteSelected { request: 22 });
    assert_eq!(fx.driver.model().lightbox().lightbox.pending_writes(), 2);
    assert_eq!(
        fx.executor.pending(),
        1,
        "the second waits behind the first"
    );

    fx.send(RuntimeInput::library(LibraryInput::Dispose));
    assert_eq!(
        fx.driver.model_mut().take_lightbox_events(),
        [unavailable(21), unavailable(22)]
    );

    fx.lightbox(LightboxCommand::DeleteSelected { request: 23 });
    assert_eq!(
        fx.driver.model_mut().take_lightbox_events(),
        [unavailable(23)]
    );

    fx.settle();
    assert!(fx.driver.model_mut().take_lightbox_events().is_empty());
    assert_eq!(
        fx.backend
            .calls()
            .iter()
            .filter(|call| call.method() == "delete_asset")
            .count(),
        1,
        "only the deletion that was sent before the disposal"
    );
}
