//! The bulk model (epic E14) in the application runtime: against the scripted backend
//! for what a partial answer publishes and what a stale selection drops, and against
//! the real core on a fresh temporary profile for the writes themselves. The backend
//! runs on the deterministic executor.

use std::sync::{Arc, Mutex};

use media_tagger_client::{
    backend::{
        core_backend::test_support::TempCore, fake::Call, AssetIdRange, Backend, BackendError,
        FakeBackend, MediaGroupUpdate,
    },
    bulk::{BulkCommand, BulkEvent, GroupSaveResult, PartialResult, TagMode},
    exec::{Executor, ManualExecutor},
    gallery::{Interaction, SelectionInput},
    i18n::Language,
    library::{LibraryInput, Mutation, Ticket},
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    theme::ThemeName,
};
use media_tagger_core::models::{
    AssetDetails, AssetSummary, AssetTagResult, BulkFavoriteSummary, BulkMediaGroupSummary,
    BulkTagMergeSummary, SetAssetTagsSummary, StartAssetQueryResult, TagListPage, TagQueryImpact,
};

const TOTAL: i64 = 6;

fn asset(id: i64) -> AssetSummary {
    AssetSummary {
        id,
        file_name: format!("asset-{id}.png"),
        preview_path: None,
        kind: "image".into(),
        modified_at: 0,
        width: None,
        height: None,
        duration_ms: None,
        thumb_version: format!("{id:016x}"),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

fn list(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_owned()).collect()
}

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl Fixture {
    /// A runtime over a scripted library of six images (IDs 1 to 6, each tagged
    /// `cat`), loaded, with selection mode on.
    fn library() -> Self {
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(|_| {
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total: TOTAL as usize,
                    offset: 0,
                    items: (1..=TOTAL).map(asset).collect(),
                })
            })
            .on_get_asset_query_id_range(|_, start, end| {
                Ok(AssetIdRange::Ready(
                    (start..=end).map(|index| index as i64 + 1).collect(),
                ))
            })
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: list(&["cat"]),
                    total: 1,
                })
            })
            .on_get_asset_details(|asset_id| {
                Ok(Some(AssetDetails {
                    summary: asset(asset_id),
                    path: format!("/media/asset-{asset_id}.png"),
                    size_bytes: 1,
                    thumb_path: None,
                    tags: list(&["cat"]),
                }))
            })
            .on_set_asset_tags(|asset_id, tags| {
                Ok(SetAssetTagsSummary {
                    vocabulary_changed: true,
                    query_impact: TagQueryImpact::None,
                    asset_id,
                    changed: true,
                    tags,
                    revision: 2,
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
        let mut fixture = Self {
            backend,
            executor,
            driver,
        };
        fixture.send(RuntimeInput::RefreshLibrary);
        fixture.settle();
        fixture.send(RuntimeInput::Selection(SelectionInput::ToggleMode));
        fixture
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn bulk(&mut self, command: BulkCommand) {
        self.send(RuntimeInput::bulk(command));
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    /// Ctrl-clicks the tiles of `ids` (ID = index + 1) and lets their reads end.
    fn select(&mut self, ids: &[i64]) {
        for id in ids {
            self.send(RuntimeInput::Selection(SelectionInput::Interact(
                Interaction::Click {
                    asset_id: *id,
                    asset_index: (*id - 1) as usize,
                    ctrl_like: true,
                    shift: false,
                },
            )));
        }
        self.settle();
    }

    fn events(&mut self) -> Vec<BulkEvent> {
        self.driver.model_mut().take_bulk_events()
    }

    fn tags_of(&self, asset_id: i64) -> Option<Vec<String>> {
        self.runtime()
            .library()
            .asset_tags(asset_id)
            .map(|known| known.tags.to_vec())
    }
}

#[test]
fn one_selected_asset_loads_its_complete_tags_and_edits_replace_them() {
    let mut fx = Fixture::library();
    fx.select(&[2]);

    let view = fx.runtime().bulk();
    assert_eq!(view.tag_mode(), TagMode::Single);
    assert_eq!(view.single_asset_tags(), ["cat"]);
    assert!(!view.tag_details_loading() && !view.tag_details_failed());

    // An addition is normalized; one the asset already has needs no write.
    fx.backend.take_calls();
    fx.bulk(BulkCommand::AddTag {
        request: 1,
        tag: "  Dog ".into(),
    });
    assert!(fx.runtime().bulk().tag_applying());
    fx.bulk(BulkCommand::AddTag {
        request: 2,
        tag: "CAT".into(),
    });
    fx.settle();
    assert_eq!(
        fx.events(),
        [
            BulkEvent::TagSettled {
                request: 2,
                saved: true
            },
            BulkEvent::TagSettled {
                request: 1,
                saved: true
            }
        ]
    );
    assert_eq!(fx.tags_of(2), Some(list(&["cat", "dog"])));
    fx.bulk(BulkCommand::RemoveTag {
        request: 3,
        tag: "cat".into(),
    });
    fx.settle();
    assert_eq!(fx.tags_of(2), Some(list(&["dog"])));
    let writes: Vec<_> = fx
        .backend
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::SetAssetTags { asset_id, tags } => Some((asset_id, tags)),
            _ => None,
        })
        .collect();
    assert_eq!(
        writes,
        [(2, list(&["cat", "dog"])), (2, list(&["dog"]))],
        "complete lists, built on the canonical tags"
    );
    assert!(!fx.runtime().bulk().tag_applying() && !fx.runtime().bulk().tag_save_failed());
}

#[test]
fn unknown_tags_are_never_replaced_and_a_failed_read_is_retried_on_request() {
    let mut fx = Fixture::library();
    fx.backend
        .on_get_asset_details(|_| Err(BackendError::rejected("i18n:common.libraryBusy")));
    fx.select(&[3]);
    assert!(fx.runtime().bulk().tag_details_failed());

    fx.backend.take_calls();
    fx.bulk(BulkCommand::AddTag {
        request: 1,
        tag: "dog".into(),
    });
    fx.settle();
    assert_eq!(
        fx.events(),
        [BulkEvent::TagSettled {
            request: 1,
            saved: false
        }]
    );
    assert!(fx.backend.take_calls().is_empty(), "nothing was sent");

    fx.backend.on_get_asset_details(|asset_id| {
        Ok(Some(AssetDetails {
            summary: asset(asset_id),
            path: String::new(),
            size_bytes: 1,
            thumb_path: None,
            tags: list(&["bird"]),
        }))
    });
    fx.bulk(BulkCommand::RetryTagDetails);
    assert!(fx.runtime().bulk().tag_details_loading());
    fx.settle();
    let view = fx.runtime().bulk();
    assert!(!view.tag_details_failed() && !view.tag_details_loading());
    assert_eq!(view.single_asset_tags(), ["bird"]);
}

/// The library says that a read is discarded the moment the query epoch changes, and
/// a refresh changes it twice. A re-read made before the refresh has ended would be
/// discarded by its end and use up one of the three attempts.
#[test]
fn a_discarded_tag_read_is_repeated_only_after_the_query_refresh_on_its_way() {
    let mut fx = Fixture::library();
    fx.backend
        .on_merge_asset_tags_bulk(|_, _| Err(BackendError::rejected("disk")));
    fx.backend.take_calls();
    let reads = |fx: &Fixture| {
        fx.backend
            .calls()
            .iter()
            .filter(|call| call.method() == "get_asset_details")
            .count()
    };
    let answer_first = |fx: &mut Fixture| {
        assert!(fx.executor.run_nth(0), "a call waits");
        while fx.driver.pump() > 0 {}
    };

    fx.send(RuntimeInput::Selection(SelectionInput::Interact(
        Interaction::Click {
            asset_id: 1,
            asset_index: 0,
            ctrl_like: true,
            shift: false,
        },
    )));
    assert_eq!(fx.executor.pending(), 1, "the tag read waits");
    // Another owner's write takes the asset, so the read's answer is discarded and
    // the next attempt waits for the writes to end.
    fx.send(RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(1),
        mutation: Mutation::MergeTags {
            asset_ids: vec![1],
            tags: list(&["dog"]),
        },
    }));
    answer_first(&mut fx);
    assert_eq!(reads(&fx), 1);
    assert!(fx.runtime().bulk().tag_details_loading());

    // A refresh starts before the write ends: the writes ending is not enough.
    fx.send(RuntimeInput::RefreshLibrary);
    answer_first(&mut fx);
    assert!(
        fx.runtime().library().query().query_pending(),
        "the write was answered, the refresh was not"
    );
    assert_eq!(reads(&fx), 1, "no read the refresh would discard");
    assert!(fx.runtime().bulk().tag_details_loading());

    fx.settle();
    let view = fx.runtime().bulk();
    assert_eq!(view.single_asset_tags(), ["cat"]);
    assert!(!view.tag_details_loading() && !view.tag_details_failed());
    assert_eq!(reads(&fx), 2);
}

#[test]
fn a_partial_merge_publishes_only_the_acknowledged_assets() {
    let mut fx = Fixture::library();
    // The backend acknowledges every asset but the second.
    fx.backend.on_merge_asset_tags_bulk(|asset_ids, tags| {
        let processed: Vec<i64> = asset_ids.into_iter().filter(|id| *id != 2).collect();
        Ok(BulkTagMergeSummary {
            vocabulary_changed: true,
            query_impact: TagQueryImpact::None,
            processed_assets: processed.len(),
            updated_assets: processed.len(),
            processed_asset_ids: processed.clone(),
            updated_asset_ids: processed.clone(),
            results: processed
                .into_iter()
                .map(|asset_id| AssetTagResult {
                    asset_id,
                    changed: true,
                    tags: [list(&["cat"]), tags.clone()].concat(),
                })
                .collect(),
            revision: 2,
        })
    });
    fx.select(&[1, 2, 3]);
    assert_eq!(fx.runtime().bulk().tag_mode(), TagMode::Multiple);

    fx.backend.take_calls();
    fx.bulk(BulkCommand::AddTag {
        request: 7,
        tag: "Trip".into(),
    });
    fx.settle();

    let merges: Vec<_> = fx
        .backend
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::MergeAssetTagsBulk { asset_ids, tags } => Some((asset_ids, tags)),
            _ => None,
        })
        .collect();
    assert_eq!(merges, [(vec![1, 2, 3], list(&["trip"]))]);
    assert_eq!(
        fx.events(),
        [BulkEvent::TagSettled {
            request: 7,
            saved: true
        }]
    );
    let view = fx.runtime().bulk();
    assert_eq!(
        view.partial_result(),
        Some(PartialResult {
            processed: 2,
            requested: 3
        })
    );
    assert_eq!(view.applied_bulk_tags(), ["trip"]);
    assert!(!view.tag_save_failed());
    // Canonical tags only where the backend said so.
    assert_eq!(fx.tags_of(1), Some(list(&["cat", "trip"])));
    assert_eq!(fx.tags_of(3), Some(list(&["cat", "trip"])));
    assert_eq!(fx.tags_of(2), None);
}

#[test]
fn a_partial_favorite_toggle_marks_the_acknowledged_assets_and_unselects_the_others() {
    let mut fx = Fixture::library();
    fx.backend.on_toggle_assets_favorite_bulk(|asset_ids| {
        Ok(BulkFavoriteSummary {
            processed_asset_ids: asset_ids.into_iter().filter(|id| *id != 4).collect(),
            is_favorite: true,
            revision: 2,
        })
    });
    fx.select(&[4, 5]);
    assert!(!fx.runtime().bulk().all_selected_favorites());

    fx.bulk(BulkCommand::ToggleFavorite);
    assert!(fx.runtime().bulk().favorite_applying());
    fx.settle();

    let view = fx.runtime().bulk();
    assert!(!view.favorite_applying() && !view.favorite_failed());
    // The asset the backend did not acknowledge is gone from the selection; the
    // other is a favorite by the backend's answer.
    assert_eq!(view.selected_asset_ids().to_vec(), [5]);
    assert!(view.all_selected_favorites());
    // The pruned selection is a new one: the count of the old one is not its feedback.
    assert_eq!(view.partial_result(), None);
}

#[test]
fn a_group_write_uses_the_given_order_reports_a_partial_answer_and_refuses_incomplete_orders() {
    let mut fx = Fixture::library();
    let acknowledged = Arc::new(Mutex::new(None::<Vec<i64>>));
    let limit = acknowledged.clone();
    fx.backend
        .on_set_assets_media_group_bulk(move |updates, media_group_key| {
            let limit = limit.lock().unwrap().clone();
            let processed: Vec<i64> = updates
                .iter()
                .map(|update| update.asset_id)
                .filter(|id| limit.as_ref().is_none_or(|only| only.contains(id)))
                .collect();
            Ok(BulkMediaGroupSummary {
                processed_assets: processed.len(),
                updated_assets: processed.len(),
                processed_asset_ids: processed,
                media_group_key,
            })
        });
    fx.select(&[1, 2, 3]);
    assert_eq!(fx.runtime().bulk().ordered_asset_ids(), [1, 2, 3]);
    fx.bulk(BulkCommand::SetGroupKeyDraft("  Trip  ".into()));

    // An order that is not exactly the selection is refused without a write.
    fx.backend.take_calls();
    for (request, order) in [(1, vec![1, 2]), (2, vec![1, 2, 2]), (3, vec![1, 2, 9])] {
        fx.bulk(BulkCommand::ApplyGroup {
            request,
            order: Some(order),
        });
    }
    fx.settle();
    assert!(fx.events().iter().all(|event| matches!(
        event,
        BulkEvent::GroupSettled {
            result: GroupSaveResult::Ignored,
            ..
        }
    )));
    assert!(fx.backend.take_calls().is_empty());

    fx.bulk(BulkCommand::ApplyGroup {
        request: 4,
        order: Some(vec![3, 1, 2]),
    });
    assert!(fx.runtime().bulk().group_applying());
    fx.settle();
    let writes: Vec<_> = fx
        .backend
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::SetAssetsMediaGroupBulk {
                updates,
                media_group_key,
            } => Some((updates, media_group_key)),
            _ => None,
        })
        .collect();
    let update = |asset_id, order: f64| MediaGroupUpdate {
        asset_id,
        media_group_order: Some(order),
    };
    assert_eq!(
        writes,
        [(
            vec![update(3, 1.0), update(1, 2.0), update(2, 3.0)],
            Some("Trip".to_owned())
        )]
    );
    assert_eq!(
        fx.events(),
        [BulkEvent::GroupSettled {
            request: 4,
            result: GroupSaveResult::Saved
        }]
    );
    let view = fx.runtime().bulk();
    assert_eq!(view.ordered_asset_ids(), [3, 1, 2]);
    assert_eq!(view.group_key_draft(), "Trip");
    assert!(!view.group_applying() && !view.group_failed());

    // A partial answer: the draft and the order stay, the result says how many.
    *acknowledged.lock().unwrap() = Some(vec![1, 3]);
    fx.bulk(BulkCommand::ApplyGroup {
        request: 5,
        order: None,
    });
    fx.settle();
    let partial = PartialResult {
        processed: 2,
        requested: 3,
    };
    assert_eq!(
        fx.events(),
        [BulkEvent::GroupSettled {
            request: 5,
            result: GroupSaveResult::Partial(partial)
        }]
    );
    assert_eq!(fx.runtime().bulk().partial_result(), Some(partial));
}

#[test]
fn feedback_of_a_write_whose_selection_changed_is_dropped() {
    let mut fx = Fixture::library();
    fx.backend
        .on_merge_asset_tags_bulk(|_, _| Err(BackendError::rejected("i18n:common.libraryBusy")));
    fx.select(&[1, 2]);
    fx.bulk(BulkCommand::AddTag {
        request: 1,
        tag: "trip".into(),
    });
    // The selection changes while the write is on its way.
    fx.select(&[3]);
    fx.settle();

    assert_eq!(
        fx.events(),
        [BulkEvent::TagSettled {
            request: 1,
            saved: false
        }]
    );
    let view = fx.runtime().bulk();
    assert!(
        !view.tag_save_failed(),
        "the failure belongs to another selection"
    );
    assert_eq!(view.partial_result(), None);
    assert!(!view.tag_applying());

    // The same failure on the current selection is shown.
    fx.bulk(BulkCommand::AddTag {
        request: 2,
        tag: "trip".into(),
    });
    fx.settle();
    assert!(fx.runtime().bulk().tag_save_failed());
}

#[test]
fn the_dock_writes_tags_favorites_and_groups_on_the_real_core() {
    let core = TempCore::start();
    for index in 0..4 {
        core.write_image(&format!("image-{index}.png"));
    }
    core.backend()
        .scan_folder(TempCore::text(&core.media_dir()))
        .expect("scan the media directory");
    let executor = Arc::new(ManualExecutor::new());
    let mut driver = Driver::new(
        AppRuntime::new(RuntimeConfig {
            theme: ThemeName::Dark,
            language: Language::En,
        }),
        RuntimeRunner::new(core.shared(), Arc::new(MemoryPrefsStore::default())),
        executor.clone() as Arc<dyn Executor>,
    );
    let settle = |driver: &mut Driver<AppRuntime, RuntimeRunner>| {
        while executor.run_all() > 0 || driver.pump() > 0 {}
    };
    driver.handle(RuntimeInput::RefreshLibrary);
    settle(&mut driver);
    driver.handle(RuntimeInput::Selection(SelectionInput::ToggleMode));
    let ids: Vec<i64> = (0..4)
        .map(|index| driver.model().query().asset_at(index).expect("a row").id)
        .collect();
    let click = |driver: &mut Driver<AppRuntime, RuntimeRunner>, index: usize| {
        driver.handle(RuntimeInput::Selection(SelectionInput::Interact(
            Interaction::Click {
                asset_id: ids[index],
                asset_index: index,
                ctrl_like: true,
                shift: false,
            },
        )));
    };
    let details = |id: i64| {
        core.backend()
            .get_asset_details(id)
            .expect("read the details")
            .expect("the asset exists")
    };

    // One asset: its tags are loaded and replaced.
    click(&mut driver, 0);
    settle(&mut driver);
    assert_eq!(driver.model().bulk().tag_mode(), TagMode::Single);
    driver.handle(RuntimeInput::bulk(BulkCommand::AddTag {
        request: 1,
        tag: "Cat".into(),
    }));
    settle(&mut driver);
    assert_eq!(details(ids[0]).tags, ["cat"]);
    assert_eq!(driver.model().bulk().single_asset_tags(), ["cat"]);

    // Three assets: tags are added only, favorites toggle, and a group gets the
    // order it was given.
    click(&mut driver, 1);
    click(&mut driver, 2);
    settle(&mut driver);
    driver.handle(RuntimeInput::bulk(BulkCommand::AddTag {
        request: 2,
        tag: "trip".into(),
    }));
    driver.handle(RuntimeInput::bulk(BulkCommand::ToggleFavorite));
    settle(&mut driver);
    assert_eq!(details(ids[0]).tags, ["cat", "trip"]);
    assert_eq!(details(ids[1]).tags, ["trip"]);
    assert!(details(ids[3]).tags.is_empty());
    assert!((0..3).all(|index| details(ids[index]).summary.is_favorite));
    assert!(driver.model().bulk().all_selected_favorites());

    driver.handle(RuntimeInput::bulk(BulkCommand::SetGroupKeyDraft(
        "holiday".into(),
    )));
    driver.handle(RuntimeInput::bulk(BulkCommand::ApplyGroup {
        request: 3,
        order: Some(vec![ids[2], ids[0], ids[1]]),
    }));
    settle(&mut driver);
    let events = driver.model_mut().take_bulk_events();
    assert!(events.contains(&BulkEvent::GroupSettled {
        request: 3,
        result: GroupSaveResult::Saved
    }));
    for (id, order) in [(ids[2], 1.0), (ids[0], 2.0), (ids[1], 3.0)] {
        let summary = details(id).summary;
        assert_eq!(summary.media_group_key.as_deref(), Some("holiday"));
        assert_eq!(summary.media_group_order, Some(order));
    }
    assert_eq!(details(ids[3]).summary.media_group_key, None);

    // An empty key removes the group again.
    driver.handle(RuntimeInput::bulk(BulkCommand::SetGroupKeyDraft(
        String::new(),
    )));
    driver.handle(RuntimeInput::bulk(BulkCommand::ApplyGroup {
        request: 4,
        order: None,
    }));
    settle(&mut driver);
    assert!((0..3).all(|index| details(ids[index]).summary.media_group_key.is_none()));
}
