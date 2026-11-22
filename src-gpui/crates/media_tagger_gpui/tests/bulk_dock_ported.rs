//! The behaviour tests of the bulk dock's view (epic E14), ported from the Solid suite
//! `src/features/bulk/__tests__/BulkDock.test.tsx` and `GroupOrderModal.test.tsx`: the
//! focus that returns to the tag field, the feedback under the header row, the header's
//! order, the larger order view's focus, dismissal and closing frame, and the rows the
//! boards draw. The main window runs on GPUI's test platform over the scripted backend
//! and the deterministic executor; a "pending promise" is a backend call the test has
//! not run yet. The model and dock-state cases of the same suites are in the client
//! crate's `tests/bulk_ported.rs`.

use std::sync::Arc;

use gpui::{
    Bounds, Entity, Focusable as _, Modifiers, Pixels, TestAppContext, VisualTestContext,
    WindowHandle,
};
use media_tagger_client::{
    backend::{fake::Call, AssetIdRange, BackendError, FakeBackend},
    bulk::DockTab,
    exec::{Executor, ManualExecutor},
    gallery::{Interaction, SelectionInput},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    theme::ThemeName,
};
use media_tagger_core::models::{
    AssetDetails, AssetQueryPageResult, AssetQuerySectionsResult, AssetSummary, AssetTagResult,
    BulkFavoriteSummary, BulkMediaGroupSummary, BulkTagMergeSummary, ScanRoot, SetAssetTagsSummary,
    StartAssetQueryResult, TagListPage, TagQueryImpact,
};
use media_tagger_gpui::{
    profile,
    runtime::{Runtime, RuntimeParts},
    views::{
        bulk_dock::{self, BulkDock},
        gallery::{self, GalleryView},
        shell::ShellView,
        tag_editor,
    },
    window::{init_application, open_main_window},
};

fn asset(id: i64) -> AssetSummary {
    AssetSummary {
        id,
        file_name: format!("asset-{id}.png"),
        preview_path: None,
        kind: "image".to_owned(),
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

fn merged(asset_ids: Vec<i64>, tags: Vec<String>) -> BulkTagMergeSummary {
    BulkTagMergeSummary {
        vocabulary_changed: true,
        query_impact: TagQueryImpact::None,
        processed_assets: asset_ids.len(),
        updated_assets: asset_ids.len(),
        processed_asset_ids: asset_ids.clone(),
        updated_asset_ids: asset_ids.clone(),
        results: asset_ids
            .iter()
            .map(|asset_id| AssetTagResult {
                asset_id: *asset_id,
                changed: true,
                tags: tags.clone(),
            })
            .collect(),
        revision: 2,
    }
}

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
}

impl Fixture {
    /// A library of `total` synthetic images (ID = index + 1) whose writes reach every
    /// asset they name.
    fn library(total: usize) -> Self {
        Self::with_rows(total, asset)
    }

    /// As [`Fixture::library`], with the rows `row` makes.
    fn with_rows(total: usize, row: fn(i64) -> AssetSummary) -> Self {
        let backend = Arc::new(FakeBackend::new());
        let rows = move |offset: usize, limit: usize| {
            (offset..(offset + limit).min(total))
                .map(|index| row(index as i64 + 1))
                .collect::<Vec<_>>()
        };
        backend
            .on_start_asset_query(move |_| {
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total,
                    offset: 0,
                    items: rows(0, 200),
                })
            })
            .on_get_asset_query_page(move |_, offset, limit| {
                Ok(AssetQueryPageResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total,
                    offset,
                    items: rows(offset, limit),
                })
            })
            .on_get_asset_query_sections(move |_| {
                Ok(AssetQuerySectionsResult::Ready {
                    session_id: 1,
                    total,
                    sections: Vec::new(),
                })
            })
            .on_get_asset_query_id_range(|_, start, end| {
                Ok(AssetIdRange::Ready(
                    (start..=end).map(|index| index as i64 + 1).collect(),
                ))
            })
            .on_get_asset_summaries_by_ids(move |ids| Ok(ids.into_iter().map(row).collect()))
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: Vec::new(),
                    total: 0,
                })
            })
            .on_get_asset_details(move |asset_id| {
                Ok(Some(AssetDetails {
                    summary: row(asset_id),
                    path: format!("/media/asset-{asset_id}.png"),
                    size_bytes: 1,
                    thumb_path: None,
                    tags: vec!["cat".to_owned(), "dog".to_owned()],
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
            })
            .on_toggle_assets_favorite_bulk(|asset_ids| {
                Ok(BulkFavoriteSummary {
                    processed_asset_ids: asset_ids,
                    is_favorite: true,
                    revision: 2,
                })
            })
            .on_merge_asset_tags_bulk(|asset_ids, tags| Ok(merged(asset_ids, tags)))
            .on_set_assets_media_group_bulk(|updates, media_group_key| {
                Ok(BulkMediaGroupSummary {
                    processed_asset_ids: updates.iter().map(|update| update.asset_id).collect(),
                    processed_assets: updates.len(),
                    updated_assets: updates.len(),
                    media_group_key,
                })
            })
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| {
                Ok(vec![ScanRoot {
                    path: "/media".into(),
                    auto_scan_on_startup: false,
                }])
            });
        Self {
            backend,
            executor: Arc::new(ManualExecutor::new()),
        }
    }

    fn parts(&self) -> RuntimeParts {
        RuntimeParts {
            config: RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            },
            backend: self.backend.clone(),
            prefs: Arc::new(MemoryPrefsStore::default()),
            executor: self.executor.clone() as Arc<dyn Executor>,
        }
    }

    fn count(&self, method: &str) -> usize {
        self.backend
            .calls()
            .iter()
            .filter(|call| call.method() == method)
            .count()
    }

    /// The asset IDs of every thumbnail request so far.
    fn thumbnail_ids(&self) -> Vec<i64> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::RequestThumbnail { request } => Some(request.asset_id),
                _ => None,
            })
            .collect()
    }
}

/// Lets the UI thread catch up without running a backend call: a few frames, since
/// focus moves and measurements land a frame later.
fn park(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
    }
    cx.run_until_parked();
}

/// Runs the backend calls and lets the replies reach the UI thread.
fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    for _ in 0..4 {
        loop {
            cx.run_until_parked();
            if fixture.executor.run_all() == 0 {
                break;
            }
        }
        cx.update(|window, cx| window.simulate_next_frame(cx));
    }
    cx.run_until_parked();
}

fn open(
    fixture: &Fixture,
    cx: &mut TestAppContext,
) -> (WindowHandle<ShellView>, VisualTestContext) {
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window(profile::E2E, fixture.parts(), cx)
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.update(|window, _| window.activate_window());
    settle(fixture, &mut cx);
    (window, cx)
}

fn runtime(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<Runtime> {
    window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open")
}

fn gallery_view(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<GalleryView> {
    window
        .read_with(cx, |shell, _| shell.gallery().cloned())
        .expect("the window is open")
        .expect("the gallery is shown")
}

fn dock(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<BulkDock> {
    gallery_view(window, cx)
        .read_with(cx, |gallery, _| gallery.dock().cloned())
        .expect("the dock is shown in selection mode")
}

fn bounds(selector: String, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
}

fn shown(selector: &'static str, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

/// A click, after which the UI thread catches up but no backend call runs.
fn press(selector: &'static str, cx: &mut VisualTestContext) {
    let at = shown(selector, cx).center();
    cx.simulate_click(at, Modifiers::none());
    park(cx);
}

fn click(selector: &'static str, fixture: &Fixture, cx: &mut VisualTestContext) {
    press(selector, cx);
    settle(fixture, cx);
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Modifiers::none()
    }
}

fn ctrl_click_tile(index: usize, cx: &mut VisualTestContext) {
    let tile = bounds(gallery::tile_selector(index), cx).expect("the tile is drawn");
    cx.simulate_click(tile.center(), ctrl());
}

/// Selection mode on and the tiles at `indexes` selected with Ctrl-clicks.
fn select(indexes: &[usize], fixture: &Fixture, cx: &mut VisualTestContext) {
    click(gallery::BULK_TOGGLE_ID, fixture, cx);
    for index in indexes {
        ctrl_click_tile(*index, cx);
        settle(fixture, cx);
    }
}

fn send(window: &WindowHandle<ShellView>, input: RuntimeInput, cx: &mut VisualTestContext) {
    runtime(window, cx).update(cx, |runtime, cx| runtime.send(input, cx));
}

fn editor_focused(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> bool {
    let editor = dock(window, cx).read_with(cx, |dock, _| dock.editor().clone());
    cx.update(|window, cx| editor.focus_handle(cx).is_focused(window))
}

fn draft(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> String {
    let editor = dock(window, cx).read_with(cx, |dock, _| dock.editor().clone());
    editor.read_with(cx, |editor, cx| editor.draft(cx).to_owned())
}

fn tab_focused(window: &WindowHandle<ShellView>, tab: DockTab, cx: &mut VisualTestContext) -> bool {
    let dock = dock(window, cx);
    cx.update(|window, cx| dock.read(cx).tab_focus(tab).is_focused(window))
}

/// Types `text` into the tag field and presses Enter; no backend call runs.
fn type_tag(text: &str, cx: &mut VisualTestContext) {
    let field = shown(tag_editor::INPUT_SELECTOR, cx);
    cx.simulate_click(field.center(), Modifiers::none());
    park(cx);
    cx.simulate_input(text);
    cx.simulate_keystrokes("enter");
    park(cx);
}

/// "restores focus after a completed save's disabled state is committed away" and
/// "restores tag input focus after an Enter submission so another tag can be added"
#[gpui::test]
fn the_tag_field_gets_the_focus_back_once_its_save_ended_even_after_the_focus_moved(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(7);
    let (window, mut cx) = open(&fixture, cx);
    select(&[1], &fixture, &mut cx);
    type_tag("travel", &mut cx);
    let editor = dock(&window, &cx).read_with(&cx, |dock, _| dock.editor().clone());
    assert!(
        editor.read_with(&cx, |editor, _| editor.editing_disabled()),
        "disabled while the save runs"
    );
    // The focus moves to a tab while the save is on its way.
    press(DockTab::Tags.test_id(), &mut cx);
    assert!(!editor_focused(&window, &mut cx));
    assert_eq!(fixture.count("set_asset_tags"), 0);

    settle(&fixture, &mut cx);
    assert_eq!(fixture.count("set_asset_tags"), 1);
    assert!(
        editor_focused(&window, &mut cx),
        "the field has the focus again"
    );
    assert_eq!(draft(&window, &mut cx), "");

    cx.simulate_input("summer");
    cx.simulate_keystrokes("enter");
    settle(&fixture, &mut cx);
    let last = fixture
        .backend
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::SetAssetTags { tags, .. } => Some(tags),
            _ => None,
        })
        .next_back();
    assert_eq!(
        last,
        Some(vec![
            "cat".to_owned(),
            "dog".to_owned(),
            "travel".to_owned(),
            "summer".to_owned()
        ])
    );
    assert!(editor_focused(&window, &mut cx));
}

/// The tag lists of every `set_asset_tags` call so far.
fn tag_writes(fixture: &Fixture) -> Vec<Vec<String>> {
    fixture
        .backend
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::SetAssetTags { tags, .. } => Some(tags),
            _ => None,
        })
        .collect()
}

/// "removes and automatically adds tags for a single selected asset". The Solid case
/// also asserts where the suggestion listbox is: `fixed`, no descendant of the dock, and
/// not hung on the field (the `viewport` strategy). Here the list is a deferred element
/// anchored to the window (GPUI's `deferred` hands `Window::defer_draw` no content
/// mask): it lies above the field, reaches outside the dock's box, and an option there
/// takes a press, which a list clipped to the scrolling dock would not.
#[gpui::test]
fn one_selected_asset_loses_a_tag_by_its_chip_and_gains_tags_by_enter_and_from_the_list_over_the_dock(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(7);
    fixture.backend.on_list_tags(|_, _, _| {
        Ok(TagListPage {
            items: vec!["travel".to_owned(), "train".to_owned()],
            total: 2,
        })
    });
    let (window, mut cx) = open(&fixture, cx);
    select(&[0], &fixture, &mut cx);
    let editor = dock(&window, &cx).read_with(&cx, |dock, _| dock.editor().clone());
    let tags =
        |cx: &VisualTestContext| editor.read_with(cx, |editor, _| editor.props().tags.clone());
    let suggestions = |cx: &VisualTestContext| {
        editor.read_with(cx, |editor, _| {
            let combobox = editor.combobox();
            let mut items: Vec<String> = if combobox.popup() {
                combobox
                    .items()
                    .iter()
                    .map(|item| item.value.clone())
                    .collect()
            } else {
                Vec::new()
            };
            items.sort_unstable();
            items
        })
    };
    assert_eq!(tags(&cx), ["cat", "dog"]);
    // One asset has nothing to order.
    assert!(cx.debug_bounds(bulk_dock::ORDER_PANEL_SELECTOR).is_none());

    // The chip's remove control, by its accessible name.
    let (remove_name, field_name) = editor.read_with(&cx, |editor, _| {
        let props = editor.props();
        (
            props.remove_label.as_ref().map(|name| name("cat")),
            props.input_label.clone(),
        )
    });
    assert_eq!(remove_name.as_deref(), Some("Remove tag cat"));
    assert_eq!(field_name.as_ref(), "Add tag");
    let remove = bounds(tag_editor::remove_selector(0), &mut cx).expect("a remove control");
    cx.simulate_click(remove.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(tag_writes(&fixture), [vec!["dog".to_owned()]]);
    assert_eq!(tags(&cx), ["dog"]);

    // Typing opens the suggestions over the window, above the field and not inside
    // the dock's box.
    let field = shown(tag_editor::INPUT_SELECTOR, &mut cx);
    cx.simulate_click(field.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    cx.simulate_input("tra");
    settle(&fixture, &mut cx);
    assert_eq!(suggestions(&cx), ["train", "travel"]);
    let panel = shown(bulk_dock::PANEL_SELECTOR, &mut cx);
    let field = shown(tag_editor::INPUT_SELECTOR, &mut cx);
    let list = shown(tag_editor::LIST_SELECTOR, &mut cx);
    assert!(list.bottom() <= field.top(), "{list:?} above {field:?}");
    assert!(list.top() < panel.top(), "{list:?} leaves {panel:?}");
    assert!(list.left() >= panel.left() && list.right() <= panel.right());

    // The field is cleared, and Enter adds what was typed, not a suggestion.
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("travel");
    cx.simulate_keystrokes("enter");
    settle(&fixture, &mut cx);
    assert_eq!(
        tag_writes(&fixture).last(),
        Some(&vec!["dog".to_owned(), "travel".to_owned()])
    );
    assert_eq!(tags(&cx), ["dog", "travel"]);
    assert_eq!(draft(&window, &mut cx), "");
    assert!(editor_focused(&window, &mut cx));

    // A suggestion drawn outside the dock's box takes a press: the list is not
    // clipped to the dock. The assigned tag is no longer suggested.
    cx.simulate_input("tra");
    settle(&fixture, &mut cx);
    assert_eq!(suggestions(&cx), ["train"]);
    let panel = shown(bulk_dock::PANEL_SELECTOR, &mut cx);
    let option = bounds(tag_editor::option_selector(0), &mut cx).expect("the suggestion");
    assert!(
        option.center().y < panel.top(),
        "{option:?} outside {panel:?}"
    );
    cx.simulate_click(option.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(
        tag_writes(&fixture).last(),
        Some(&vec![
            "dog".to_owned(),
            "travel".to_owned(),
            "train".to_owned()
        ])
    );
    assert_eq!(tags(&cx), ["dog", "travel", "train"]);
    // The press was the list's, not the grid's beneath it.
    assert_eq!(
        runtime(&window, &cx).read_with(&cx, |runtime, _| runtime
            .model()
            .selection()
            .ids()
            .to_vec()),
        [1]
    );
}

/// "ignores a retired selection's saved / failed / rejected tag save without clearing
/// the new draft or stealing focus" and "cancels deferred tag focus when selection
/// changes before re-enabling"
#[gpui::test]
fn a_retired_selections_save_neither_clears_the_new_draft_nor_takes_the_focus(
    cx: &mut TestAppContext,
) {
    for succeeds in [true, false] {
        let fixture = Fixture::library(7);
        if !succeeds {
            fixture
                .backend
                .on_set_asset_tags(|_, _| Err(BackendError::rejected("retired save")));
        }
        let (window, mut cx) = open(&fixture, cx);
        select(&[1], &fixture, &mut cx);
        type_tag("old-tag", &mut cx);
        // The save is the last call started; the suggestion searches of the typing
        // before it may run.
        for _ in 1..fixture.executor.pending() {
            fixture.executor.run_nth(0);
        }
        park(&mut cx);
        assert_eq!(fixture.count("set_asset_tags"), 0, "the save waits");
        assert!(fixture.executor.pending() >= 1);

        // Another asset is selected; its own tag read runs, the old save still waits.
        press(bulk_dock::CLEAR_ID, &mut cx);
        ctrl_click_tile(2, &mut cx);
        park(&mut cx);
        assert_eq!(draft(&window, &mut cx), "", "a new selection starts empty");
        while fixture.executor.pending() > 1 {
            fixture.executor.run_nth(1);
            park(&mut cx);
        }
        // Any tag write keeps the field disabled, the retired one too (as in the Solid
        // model): no new draft can be typed meanwhile. The dock-state port in the
        // client crate keeps a new draft across the retired answer.
        let editor = dock(&window, &cx).read_with(&cx, |dock, _| dock.editor().clone());
        assert!(editor.read_with(&cx, |editor, _| editor.editing_disabled()));
        press(DockTab::Tags.test_id(), &mut cx);
        assert!(tab_focused(&window, DockTab::Tags, &mut cx));

        settle(&fixture, &mut cx);
        assert_eq!(draft(&window, &mut cx), "", "saved: {succeeds}");
        assert!(
            tab_focused(&window, DockTab::Tags, &mut cx),
            "saved: {succeeds}"
        );
        assert!(!editor_focused(&window, &mut cx), "saved: {succeeds}");
        assert_eq!(fixture.count("set_asset_tags"), 1);
    }
}

/// "shows header tag feedback and partial counts on the Group & order tab"
#[gpui::test]
fn the_header_feedback_and_the_partial_count_show_on_the_group_tab(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    fixture.backend.on_merge_asset_tags_bulk(|_, _| {
        Ok(BulkTagMergeSummary {
            processed_assets: 0,
            updated_assets: 0,
            ..merged(Vec::new(), Vec::new())
        })
    });
    let (_window, mut cx) = open(&fixture, cx);
    select(&[0, 1], &fixture, &mut cx);
    type_tag("dog", &mut cx);
    settle(&fixture, &mut cx);
    click(DockTab::Group.test_id(), &fixture, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::GROUP_PANEL_SELECTOR).is_some());
    assert!(cx.debug_bounds(bulk_dock::PARTIAL_SELECTOR).is_some());
    assert!(cx
        .debug_bounds(bulk_dock::TAG_SAVE_FAILED_SELECTOR)
        .is_some());

    let merges = fixture.count("merge_asset_tags_bulk");
    type_tag("two;tags", &mut cx);
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::TAG_INVALID_SELECTOR).is_some());
    assert_eq!(fixture.count("merge_asset_tags_bulk"), merges);
}

/// "shows a partial tag write on the default Tags tab"
#[gpui::test]
fn a_partial_tag_write_shows_its_count_on_the_tags_tab(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    fixture.backend.on_merge_asset_tags_bulk(|asset_ids, tags| {
        Ok(merged(asset_ids.into_iter().take(1).collect(), tags))
    });
    let (window, mut cx) = open(&fixture, cx);
    select(&[0, 1], &fixture, &mut cx);
    type_tag("dog", &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(
        dock(&window, &cx).read_with(&cx, |dock, _| dock.dock().tab()),
        DockTab::Tags
    );
    assert!(cx.debug_bounds(bulk_dock::TAGS_PANEL_SELECTOR).is_some());
    assert!(cx.debug_bounds(bulk_dock::PARTIAL_SELECTOR).is_some());
    assert!(cx
        .debug_bounds(bulk_dock::TAG_SAVE_FAILED_SELECTOR)
        .is_none());
}

/// "collapses to a chip ... The tag input comes first, then the heart and the clearing
/// counter on the same row."
#[gpui::test]
fn the_header_row_holds_the_tag_field_the_heart_and_the_counter_in_that_order(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(7);
    let (_window, mut cx) = open(&fixture, cx);
    select(&[0, 1], &fixture, &mut cx);
    let header = shown(bulk_dock::HEADER_SELECTOR, &mut cx);
    let field = shown(tag_editor::INPUT_SELECTOR, &mut cx);
    let heart = shown(bulk_dock::FAVORITE_ID, &mut cx);
    let counter = shown(bulk_dock::CLEAR_ID, &mut cx);
    assert!(field.right() <= heart.left(), "{field:?} {heart:?}");
    assert!(heart.right() <= counter.left(), "{heart:?} {counter:?}");
    for part in [field, heart, counter] {
        assert!(part.top() >= header.top() && part.bottom() <= header.bottom());
    }
}

/// "keeps tag editing disabled after details failure and retries loading separately"
#[gpui::test]
fn a_failed_tag_read_keeps_editing_disabled_and_retry_reads_again(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    fixture
        .backend
        .on_get_asset_details(|_| Err(BackendError::rejected("read")));
    let (window, mut cx) = open(&fixture, cx);
    select(&[0], &fixture, &mut cx);
    let editor = dock(&window, &cx).read_with(&cx, |dock, _| dock.editor().clone());
    assert!(editor.read_with(&cx, |editor, _| editor.editing_disabled()));
    assert!(cx
        .debug_bounds(bulk_dock::TAG_LOAD_FAILED_SELECTOR)
        .is_some());

    fixture.backend.on_get_asset_details(|asset_id| {
        Ok(Some(AssetDetails {
            summary: asset(asset_id),
            path: String::new(),
            size_bytes: 1,
            thumb_path: None,
            tags: vec!["cat".to_owned()],
        }))
    });
    let reads = fixture.count("get_asset_details");
    click(bulk_dock::RETRY_TAGS_ID, &fixture, &mut cx);
    assert_eq!(fixture.count("get_asset_details"), reads + 1);
    assert!(!editor.read_with(&cx, |editor, _| editor.editing_disabled()));
    assert_eq!(
        editor.read_with(&cx, |editor, _| editor.props().tags.clone()),
        ["cat"]
    );
    assert!(cx
        .debug_bounds(bulk_dock::TAG_LOAD_FAILED_SELECTOR)
        .is_none());
}

/// "shows the assigned-tag loading state"
#[gpui::test]
fn the_tags_show_a_loading_state_while_their_read_is_on_its_way(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    let (window, mut cx) = open(&fixture, cx);
    click(gallery::BULK_TOGGLE_ID, &fixture, &mut cx);
    ctrl_click_tile(0, &mut cx);
    park(&mut cx);
    let editor = dock(&window, &cx).read_with(&cx, |dock, _| dock.editor().clone());
    assert!(editor.read_with(&cx, |editor, _| editor.props().loading_text.is_some()));
    assert!(editor.read_with(&cx, |editor, _| editor.editing_disabled()));
    assert!(bounds(tag_editor::chip_selector(0), &mut cx).is_none());

    settle(&fixture, &mut cx);
    assert!(editor.read_with(&cx, |editor, _| editor.props().loading_text.is_none()));
    assert!(bounds(tag_editor::chip_selector(0), &mut cx).is_some());
}

fn conflicting(id: i64) -> AssetSummary {
    AssetSummary {
        media_group_key: match id {
            1 => Some("a".into()),
            2 => Some("b".into()),
            _ => None,
        },
        media_group_order: (id <= 2).then_some(1.0),
        ..asset(id)
    }
}

/// Types `key` into the group key field of the open group tab.
fn type_group_key(key: &str, fixture: &Fixture, cx: &mut VisualTestContext) {
    let field = shown(bulk_dock::GROUP_KEY_SELECTOR, cx);
    cx.simulate_click(field.center(), Modifiers::none());
    settle(fixture, cx);
    cx.simulate_input(key);
    settle(fixture, cx);
}

/// "shows conflicts in the group tab and opens the larger sorting view"
#[gpui::test]
fn a_group_conflict_is_shown_on_the_group_tab_only(cx: &mut TestAppContext) {
    let fixture = Fixture::with_rows(7, conflicting);
    let (window, mut cx) = open(&fixture, cx);
    select(&[0, 1, 2], &fixture, &mut cx);
    assert!(cx
        .debug_bounds(bulk_dock::GROUP_CONFLICT_SELECTOR)
        .is_none());
    click(DockTab::Group.test_id(), &fixture, &mut cx);
    assert!(cx
        .debug_bounds(bulk_dock::GROUP_CONFLICT_SELECTOR)
        .is_some());

    type_group_key("trip", &fixture, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::ORDER_PANEL_SELECTOR).is_some());
    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    let dock = dock(&window, &cx);
    let modal = dock.read_with(&cx, |dock, _| dock.order_modal().clone());
    assert!(modal.read_with(&cx, |modal, _| modal.is_open()));
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().modal_order().to_vec()),
        [1, 2, 3]
    );
}

/// The tiles at `indexes` selected, the group tab open and `trip` typed as the key.
fn group_tab_with_key(
    indexes: &[usize],
    fixture: &Fixture,
    window: &WindowHandle<ShellView>,
    cx: &mut VisualTestContext,
) -> Entity<BulkDock> {
    select(indexes, fixture, cx);
    click(DockTab::Group.test_id(), fixture, cx);
    type_group_key("trip", fixture, cx);
    dock(window, cx)
}

fn trigger_focused(dock: &Entity<BulkDock>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| dock.read(cx).open_order_modal_focus().is_focused(window))
}

/// "keeps keyboard changes local, discards on Cancel and restores trigger focus" (the
/// focus and Escape half)
#[gpui::test]
fn cancel_and_escape_close_the_larger_view_and_give_the_focus_back_to_its_trigger(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(7);
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&[0, 1, 2], &fixture, &window, &mut cx);
    let modal = dock.read_with(&cx, |dock, _| dock.order_modal().clone());

    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    assert!(modal.read_with(&cx, |modal, _| modal.is_open()));
    click(bulk_dock::ORDER_MODAL_CANCEL_ID, &fixture, &mut cx);
    assert!(!modal.read_with(&cx, |modal, _| modal.is_open()));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(&fixture, &mut cx);
    assert!(trigger_focused(&dock, &mut cx), "Cancel: the trigger");

    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    assert!(modal.read_with(&cx, |modal, _| modal.is_open()));
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(!modal.read_with(&cx, |modal, _| modal.is_open()));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(&fixture, &mut cx);
    assert!(trigger_focused(&dock, &mut cx), "Escape: the trigger");
    assert_eq!(fixture.count("set_assets_media_group_bulk"), 0);
}

/// "saves the exact modal draft, blocks duplicate submissions and dismissal while
/// saving"
#[gpui::test]
fn while_the_larger_view_saves_escape_cancel_and_a_second_save_do_nothing(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&[0, 1, 2], &fixture, &window, &mut cx);
    let modal = dock.read_with(&cx, |dock, _| dock.order_modal().clone());
    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    let grip = dock
        .read_with(&cx, |dock, _| dock.handle_focus(true, 1).cloned())
        .expect("the grip of the first tile");
    cx.update(|window, cx| window.focus(&grip, cx));
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);

    press(bulk_dock::ORDER_MODAL_SAVE_ID, &mut cx);
    assert_eq!(fixture.executor.pending(), 1, "the save waits");
    cx.simulate_keystrokes("escape");
    park(&mut cx);
    press(bulk_dock::ORDER_MODAL_CANCEL_ID, &mut cx);
    press(bulk_dock::ORDER_MODAL_SAVE_ID, &mut cx);
    assert!(
        modal.read_with(&cx, |modal, _| modal.is_open()),
        "no dismissal"
    );
    assert_eq!(fixture.executor.pending(), 1, "no second save");

    settle(&fixture, &mut cx);
    assert!(
        !modal.read_with(&cx, |modal, _| modal.is_open()),
        "saved: closed"
    );
    let writes: Vec<Vec<i64>> = fixture
        .backend
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::SetAssetsMediaGroupBulk { updates, .. } => {
                Some(updates.into_iter().map(|update| update.asset_id).collect())
            }
            _ => None,
        })
        .collect();
    assert_eq!(writes, [vec![2, 1, 3]]);
}

/// "retains the sorted closing frame, removes it after exit and initializes a fresh
/// draft on reopening"
#[gpui::test]
fn the_closing_larger_view_keeps_its_last_order_until_it_is_gone(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&[0, 1, 2], &fixture, &window, &mut cx);
    let modal = dock.read_with(&cx, |dock, _| dock.order_modal().clone());
    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    let grip = dock
        .read_with(&cx, |dock, _| dock.handle_focus(true, 1).cloned())
        .expect("the grip of the first tile");
    cx.update(|window, cx| window.focus(&grip, cx));
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);

    press(bulk_dock::ORDER_MODAL_CANCEL_ID, &mut cx);
    assert!(!modal.read_with(&cx, |modal, _| modal.is_open()));
    assert!(
        modal.read_with(&cx, |modal, _| modal.is_present()),
        "closing"
    );
    let first = bounds(bulk_dock::tile_selector(true, 2), &mut cx).expect("the closing frame");
    let second = bounds(bulk_dock::tile_selector(true, 1), &mut cx).expect("the closing frame");
    assert!(
        first.left() < second.left(),
        "the sorted order stays while closing"
    );

    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(&fixture, &mut cx);
    assert!(!modal.read_with(&cx, |modal, _| modal.is_present()));
    assert!(bounds(bulk_dock::tile_selector(true, 1), &mut cx).is_none());

    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().modal_order().to_vec()),
        [1, 2, 3]
    );
}

/// Selection mode on and every result selected.
fn select_all(window: &WindowHandle<ShellView>, fixture: &Fixture, cx: &mut VisualTestContext) {
    click(gallery::BULK_TOGGLE_ID, fixture, cx);
    send(
        window,
        RuntimeInput::Selection(SelectionInput::Interact(Interaction::SelectAll)),
        cx,
    );
    settle(fixture, cx);
}

/// "bounds mounted tiles and thumbnail requests for a thousand selected items" and
/// "windows fixed-height rows and requests thumbnails only for them"
#[gpui::test]
fn a_thousand_selected_items_draw_and_request_only_the_tiles_near_the_view(
    cx: &mut TestAppContext,
) {
    const TOTAL: i64 = 1000;
    let fixture = Fixture::library(TOTAL as usize);
    let (window, mut cx) = open(&fixture, cx);
    select_all(&window, &fixture, &mut cx);
    assert_eq!(
        runtime(&window, &cx).read_with(&cx, |runtime, _| runtime.model().selection().count()),
        TOTAL as usize
    );
    click(DockTab::Group.test_id(), &fixture, &mut cx);
    type_group_key("trip", &fixture, &mut cx);
    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    let modal = dock(&window, &cx).read_with(&cx, |dock, _| dock.order_modal().clone());
    assert!(modal.read_with(&cx, |modal, _| modal.is_open()));

    let drawn: Vec<i64> = (1..=TOTAL)
        .filter(|id| bounds(bulk_dock::tile_selector(true, *id), &mut cx).is_some())
        .collect();
    assert!(!drawn.is_empty() && drawn.len() < 40, "{drawn:?}");
    assert_eq!(drawn[0], 1);
    let requested = fixture.thumbnail_ids();
    assert!(!requested.is_empty());
    assert!(
        requested.iter().all(|id| *id < 200),
        "only tiles near a view ask for their picture: {requested:?}"
    );
}

/// "keeps a keyboard-moved tile in view"
#[gpui::test]
fn a_tile_moved_by_a_key_in_the_larger_view_stays_drawn_and_in_view(cx: &mut TestAppContext) {
    let fixture = Fixture::library(96);
    let (window, mut cx) = open(&fixture, cx);
    select_all(&window, &fixture, &mut cx);
    click(DockTab::Group.test_id(), &fixture, &mut cx);
    type_group_key("trip", &fixture, &mut cx);
    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    let dock = dock(&window, &cx);
    let grid = shown_string(bulk_dock::grid_selector(true), &mut cx);
    let first = bounds(bulk_dock::tile_selector(true, 1), &mut cx).expect("the first tile");
    let columns = (1..=96)
        .filter_map(|id| bounds(bulk_dock::tile_selector(true, id), &mut cx))
        .filter(|tile| tile.top() == first.top())
        .count();
    assert!(columns >= 1);

    let grip = dock
        .read_with(&cx, |dock, _| dock.handle_focus(true, 7).cloned())
        .expect("the grip of the seventh tile");
    cx.update(|window, cx| window.focus(&grip, cx));
    for _ in 0..6 {
        cx.simulate_keystrokes("down");
        settle(&fixture, &mut cx);
    }
    let index = dock
        .read_with(&cx, |dock, _| dock.dock().modal_order().to_vec())
        .iter()
        .position(|id| *id == 7)
        .expect("the tile is in the order");
    assert_eq!(index, (6 + 6 * columns).min(95 - (95 - 6) % columns));
    assert!(
        cx.update(|window, _| grip.is_focused(window)),
        "the grip keeps the focus"
    );
    let tile = bounds(bulk_dock::tile_selector(true, 7), &mut cx).expect("the moved tile is drawn");
    assert!(
        tile.top() >= grid.top() && tile.bottom() <= grid.bottom() + gpui::px(1.),
        "{tile:?} in {grid:?}"
    );
}

fn shown_string(selector: String, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    bounds(selector.clone(), cx).unwrap_or_else(|| panic!("{selector} is drawn"))
}
