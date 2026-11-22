//! The bulk dock (epic E14) in the main window on GPUI's test platform, with the
//! scripted backend and the deterministic executor: the chip and the expanded dock, the
//! counter, a tag added to several assets, the tabs, the group key with the inline order
//! board (keys and drag), the larger order view, and the start states of the three dock
//! scenarios.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use gpui::{
    Bounds, Entity, Focusable as _, Modifiers, MouseButton, Pixels, TestAppContext,
    VisualTestContext, WindowHandle,
};
use media_tagger_client::{
    backend::{fake::Call, AssetIdRange, FakeBackend, MediaGroupUpdate},
    bulk::{
        layout::{dock as dock_layout, group as group_layout},
        DockTab,
    },
    controls::ICON_BUTTON_SIZE,
    exec::{Executor, ManualExecutor},
    gallery::{Interaction, SelectionInput},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    text::Selection,
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
    scenario::start_state,
    ui::text_input::TextInputEvent,
    views::{
        bulk_dock::{self, BulkDock},
        gallery::{self, GalleryView},
        shell::ShellView,
        tag_editor,
    },
    window::{init_application, open_main_window, open_main_window_started, ShellStart},
};

const TOTAL: usize = 7;

fn asset(index: usize) -> AssetSummary {
    let id = index as i64 + 1;
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

/// One group write: the assets with their orders, in the order sent, and the key.
type GroupWrite = (Vec<(i64, Option<f64>)>, Option<String>);

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
}

impl Fixture {
    /// A library of seven synthetic images (ID = index + 1) whose bulk writes reach
    /// every asset they name.
    fn library() -> Self {
        let backend = Arc::new(FakeBackend::new());
        let items = || (0..TOTAL).map(asset).collect::<Vec<_>>();
        backend
            .on_start_asset_query(move |_| {
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total: TOTAL,
                    offset: 0,
                    items: items(),
                })
            })
            .on_get_asset_query_page(move |_, offset, _| {
                Ok(AssetQueryPageResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total: TOTAL,
                    offset,
                    items: items(),
                })
            })
            .on_get_asset_query_sections(|_| {
                Ok(AssetQuerySectionsResult::Ready {
                    session_id: 1,
                    total: TOTAL,
                    sections: Vec::new(),
                })
            })
            .on_get_asset_query_id_range(|_, start, end| {
                Ok(AssetIdRange::Ready(
                    (start..=end).map(|index| index as i64 + 1).collect(),
                ))
            })
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: Vec::new(),
                    total: 0,
                })
            })
            .on_get_asset_details(|asset_id| {
                Ok(Some(AssetDetails {
                    summary: asset(asset_id as usize - 1),
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
            .on_merge_asset_tags_bulk(|asset_ids, tags| {
                Ok(BulkTagMergeSummary {
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
                })
            })
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

    /// The group writes sent since the last call: the updates in order, and the key.
    fn take_group_writes(&self) -> Vec<GroupWrite> {
        self.backend
            .take_calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::SetAssetsMediaGroupBulk {
                    updates,
                    media_group_key,
                } => Some((
                    updates
                        .into_iter()
                        .map(
                            |MediaGroupUpdate {
                                 asset_id,
                                 media_group_order,
                             }| { (asset_id, media_group_order) },
                        )
                        .collect(),
                    media_group_key,
                )),
                _ => None,
            })
            .collect()
    }
}

/// Runs the backend calls and lets the replies reach the UI thread; a few frames, since
/// focus moves and measurements land a frame later.
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

fn click(selector: &'static str, fixture: &Fixture, cx: &mut VisualTestContext) {
    let at = shown(selector, cx).center();
    cx.simulate_click(at, Modifiers::none());
    settle(fixture, cx);
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Modifiers::none()
    }
}

/// Selection mode on and the tiles at `indexes` selected with Ctrl-clicks.
fn select(indexes: &[usize], fixture: &Fixture, cx: &mut VisualTestContext) {
    click(gallery::BULK_TOGGLE_ID, fixture, cx);
    for index in indexes {
        let tile = bounds(gallery::tile_selector(*index), cx).expect("the tile is drawn");
        cx.simulate_click(tile.center(), ctrl());
        settle(fixture, cx);
    }
}

fn selected(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Vec<i64> {
    runtime(window, cx).read_with(cx, |runtime, _| runtime.model().selection().ids().to_vec())
}

fn inline_tile(asset_id: i64, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    bounds(bulk_dock::tile_selector(false, asset_id), cx)
}

#[gpui::test]
fn the_dock_is_a_chip_until_something_is_selected_and_its_counter_clears(cx: &mut TestAppContext) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    assert!(cx.debug_bounds(bulk_dock::PANEL_SELECTOR).is_none());

    click(gallery::BULK_TOGGLE_ID, &fixture, &mut cx);
    let chip = shown(bulk_dock::PANEL_SELECTOR, &mut cx);
    let viewport = shown(gallery::VIEWPORT_SELECTOR, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::CHIP_SELECTOR).is_some());
    assert!(cx.debug_bounds(bulk_dock::HEADER_SELECTOR).is_none());
    assert_eq!(f32::from(chip.size.width), dock_layout::CHIP_WIDTH);
    // Bottom centre of the gallery's viewport.
    assert_eq!(
        viewport.bottom() - chip.bottom(),
        gpui::px(dock_layout::MARGIN)
    );
    assert!((f32::from(chip.center().x) - f32::from(viewport.center().x)).abs() <= 1.0);

    let tile = bounds(gallery::tile_selector(0), &mut cx).expect("a tile");
    cx.simulate_click(tile.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    let panel = shown(bulk_dock::PANEL_SELECTOR, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::CHIP_SELECTOR).is_none());
    assert!(cx.debug_bounds(bulk_dock::HEADER_SELECTOR).is_some());
    assert!(cx.debug_bounds(bulk_dock::TAGS_PANEL_SELECTOR).is_some());
    assert_eq!(
        f32::from(panel.size.width),
        dock_layout::expanded_width(f32::from(viewport.size.width))
    );
    assert_eq!(selected(&window, &cx), [1]);

    click(bulk_dock::CLEAR_ID, &fixture, &mut cx);
    assert!(selected(&window, &cx).is_empty());
    assert!(cx.debug_bounds(bulk_dock::CHIP_SELECTOR).is_some());
    // Selection mode stays on.
    assert!(
        runtime(&window, &cx).read_with(&cx, |runtime, _| runtime.model().selection().enabled())
    );

    // Off: the dock goes with the mode.
    click(gallery::BULK_TOGGLE_ID, &fixture, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::PANEL_SELECTOR).is_none());
}

#[gpui::test]
fn a_press_on_the_dock_is_none_on_the_grid_beneath_it(cx: &mut TestAppContext) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    select(&[0, 1], &fixture, &mut cx);
    let before = selected(&window, &cx);

    let header = shown(bulk_dock::HEADER_SELECTOR, &mut cx);
    let panel = shown(bulk_dock::PANEL_SELECTOR, &mut cx);
    // The dock's own padding, left of the header row.
    let blank = gpui::point(panel.left() + gpui::px(4.), header.center().y);
    cx.simulate_click(blank, Modifiers::none());
    settle(&fixture, &mut cx);

    assert_eq!(selected(&window, &cx), before);
}

#[gpui::test]
fn a_tag_for_several_assets_is_merged_shown_without_removal_and_the_field_gets_the_focus_back(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    select(&[0, 1, 2], &fixture, &mut cx);
    fixture.backend.take_calls();

    let field = shown(tag_editor::INPUT_SELECTOR, &mut cx);
    cx.simulate_click(field.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    cx.simulate_input(" Holiday ");
    cx.simulate_keystrokes("enter");
    settle(&fixture, &mut cx);

    let merges: Vec<_> = fixture
        .backend
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::MergeAssetTagsBulk { asset_ids, tags } => Some((asset_ids, tags)),
            _ => None,
        })
        .collect();
    assert_eq!(merges, [(vec![1, 2, 3], vec!["holiday".to_owned()])]);
    let dock = dock(&window, &cx);
    let editor = dock.read_with(&cx, |dock, _| dock.editor().clone());
    assert_eq!(
        editor.read_with(&cx, |editor, _| editor.props().tags.clone()),
        ["holiday"]
    );
    assert!(bounds(tag_editor::chip_selector(0), &mut cx).is_some());
    assert!(
        bounds(tag_editor::remove_selector(0), &mut cx).is_none(),
        "several assets: add-only"
    );
    assert_eq!(
        editor.read_with(&cx, |editor, cx| editor.draft(cx).to_owned()),
        ""
    );
    assert!(
        cx.update(|window, cx| editor.focus_handle(cx).is_focused(window)),
        "the field has the focus again"
    );

    // An invalid draft stays and is explained; nothing is sent.
    cx.simulate_input("two words");
    cx.simulate_keystrokes("enter");
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::TAG_INVALID_SELECTOR).is_some());
    assert_eq!(
        editor.read_with(&cx, |editor, cx| editor.draft(cx).to_owned()),
        "two words"
    );
    assert!(fixture
        .backend
        .take_calls()
        .iter()
        .all(|call| !matches!(call, Call::MergeAssetTagsBulk { .. })));
}

#[gpui::test]
fn one_selected_asset_shows_its_tags_with_removal_and_the_heart_toggles_favorites(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    select(&[1], &fixture, &mut cx);
    let dock = dock(&window, &cx);
    let editor = dock.read_with(&cx, |dock, _| dock.editor().clone());
    assert_eq!(
        editor.read_with(&cx, |editor, _| editor.props().tags.clone()),
        ["cat", "dog"],
        "the complete tag list of the one asset"
    );
    assert!(!editor.read_with(&cx, |editor, _| editor.editing_disabled()));

    // A chip's remove button writes the list without that tag.
    fixture.backend.take_calls();
    let remove = bounds(tag_editor::remove_selector(0), &mut cx).expect("a remove button");
    cx.simulate_click(remove.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    let writes: Vec<_> = fixture
        .backend
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::SetAssetTags { asset_id, tags } => Some((asset_id, tags)),
            _ => None,
        })
        .collect();
    assert_eq!(writes, [(2, vec!["dog".to_owned()])]);
    assert_eq!(
        editor.read_with(&cx, |editor, _| editor.props().tags.clone()),
        ["dog"],
        "the backend's canonical list"
    );

    // The heart toggles the favorite of the selection and is filled afterwards.
    click(bulk_dock::FAVORITE_ID, &fixture, &mut cx);
    let toggles: Vec<_> = fixture
        .backend
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::ToggleAssetsFavoriteBulk { asset_ids } => Some(asset_ids),
            _ => None,
        })
        .collect();
    assert_eq!(toggles, [vec![2]]);
    assert!(runtime(&window, &cx).read_with(&cx, |runtime, _| runtime
        .model()
        .bulk()
        .all_selected_favorites()));
}

#[gpui::test]
fn the_tabs_follow_clicks_and_keys_and_the_group_tab_is_tall(cx: &mut TestAppContext) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    select(&[0, 1], &fixture, &mut cx);
    let compact = shown(bulk_dock::PANEL_SELECTOR, &mut cx);
    let viewport = shown(gallery::VIEWPORT_SELECTOR, &mut cx);

    click(DockTab::Group.test_id(), &fixture, &mut cx);
    let tall = shown(bulk_dock::PANEL_SELECTOR, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::GROUP_PANEL_SELECTOR).is_some());
    assert!(cx.debug_bounds(bulk_dock::TAGS_PANEL_SELECTOR).is_none());
    // Laid out on whole pixels.
    let expected = dock_layout::tall_height(f32::from(viewport.size.height));
    assert!((f32::from(tall.size.height) - expected).abs() < 1.0);
    assert!(tall.size.height > compact.size.height);
    assert_eq!(tall.bottom(), compact.bottom());

    // Arrow left wraps to Tags and moves the focus with the selection.
    let grid_top = |cx: &VisualTestContext| {
        gallery_view(&window, cx).read_with(cx, |gallery, _| gallery.grid().offset())
    };
    let scrolled_to = grid_top(&cx);
    let dock = dock(&window, &cx);
    cx.simulate_keystrokes("left");
    settle(&fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().tab()),
        DockTab::Tags
    );
    assert!(cx.update(|window, cx| dock.read(cx).tab_focus(DockTab::Tags).is_focused(window)));
    cx.simulate_keystrokes("end");
    settle(&fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().tab()),
        DockTab::Group
    );
    assert!(cx.update(|window, cx| dock.read(cx).tab_focus(DockTab::Group).is_focused(window)));
    cx.simulate_keystrokes("home");
    settle(&fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().tab()),
        DockTab::Tags
    );

    // The keys of the tablist scrolled nothing beneath.
    assert_eq!(grid_top(&cx), scrolled_to);

    // Clearing the selection returns to the Tags tab.
    click(DockTab::Group.test_id(), &fixture, &mut cx);
    click(bulk_dock::CLEAR_ID, &fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().tab()),
        DockTab::Tags
    );
}

/// Three tiles selected, the group tab open and `trip` typed as the key.
fn group_tab_with_key(
    fixture: &Fixture,
    window: &WindowHandle<ShellView>,
    cx: &mut VisualTestContext,
) -> Entity<BulkDock> {
    select(&[0, 1, 2], fixture, cx);
    click(DockTab::Group.test_id(), fixture, cx);
    assert!(
        cx.debug_bounds(bulk_dock::ORDER_PANEL_SELECTOR).is_none(),
        "no ordering before a group key is set"
    );
    let key = shown(bulk_dock::GROUP_KEY_SELECTOR, cx);
    cx.simulate_click(key.center(), Modifiers::none());
    settle(fixture, cx);
    cx.simulate_input("trip");
    settle(fixture, cx);
    dock(window, cx)
}

#[gpui::test]
fn a_group_key_shows_the_inline_board_and_a_grip_moves_its_tile_with_the_arrow_keys(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&fixture, &window, &mut cx);
    assert!(cx.debug_bounds(bulk_dock::ORDER_PANEL_SELECTOR).is_some());
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [1, 2, 3]
    );
    let first = inline_tile(1, &mut cx).expect("the first tile");
    let second = inline_tile(2, &mut cx).expect("the second tile");
    assert!(first.right() < second.left() && first.top() == second.top());
    // As tall as the Solid tile: the preview (0.66 of the width, rounded), the 44px
    // caption with its line inside, and the 1px border above and below.
    assert_eq!(
        f32::from(first.size.height),
        (f32::from(first.size.width) * 0.66).round() + 46.0
    );

    // The grip of the first tile: arrow right moves it behind the second and the grip
    // keeps the focus.
    let grip = dock
        .read_with(&cx, |dock, _| dock.handle_focus(false, 1).cloned())
        .expect("the grip of the first tile");
    cx.update(|window, cx| window.focus(&grip, cx));
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [2, 1, 3]
    );
    assert!(cx.update(|window, _| grip.is_focused(window)));
    assert_eq!(
        inline_tile(1, &mut cx).expect("moved").origin,
        second.origin
    );
    // At the edge of the order a key moves nothing.
    cx.simulate_keystrokes("right right");
    settle(&fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [2, 3, 1]
    );
    assert!(cx.update(|window, _| grip.is_focused(window)));

    // Apply sends the inline order with orders 1..n.
    fixture.backend.take_calls();
    click(bulk_dock::APPLY_GROUP_ID, &fixture, &mut cx);
    assert_eq!(
        fixture.take_group_writes(),
        [(
            vec![(2, Some(1.0)), (3, Some(2.0)), (1, Some(3.0))],
            Some("trip".to_owned())
        )]
    );
}

#[gpui::test]
fn a_grip_is_kept_while_its_asset_is_ordered_and_goes_when_the_asset_leaves_the_order(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&fixture, &window, &mut cx);
    let grip = |asset_id: i64, cx: &VisualTestContext| {
        dock.read_with(cx, |dock, _| dock.handle_focus(false, asset_id).cloned())
    };
    let first = grip(1, &cx).expect("the grip of the first tile");
    assert!(grip(3, &cx).is_some());

    // Frames that change nothing, and a new order of the same assets, keep the grips.
    settle(&fixture, &mut cx);
    assert!(grip(1, &cx).as_ref() == Some(&first));
    cx.update(|window, cx| window.focus(&first, cx));
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [2, 1, 3]
    );
    assert!(grip(1, &cx).as_ref() == Some(&first));
    assert!(grip(3, &cx).is_some());

    // The third asset leaves the selection, and so the order the board shows once a
    // key is typed again: its grip goes, the others are the ones they were.
    let tile = bounds(gallery::tile_selector(2), &mut cx).expect("the tile is drawn");
    cx.simulate_click(tile.center(), ctrl());
    settle(&fixture, &mut cx);
    assert_eq!(selected(&window, &cx), [1, 2]);
    let key = shown(bulk_dock::GROUP_KEY_SELECTOR, &mut cx);
    cx.simulate_click(key.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    cx.simulate_input("trip");
    settle(&fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [1, 2]
    );
    assert!(inline_tile(1, &mut cx).is_some() && inline_tile(3, &mut cx).is_none());
    assert!(grip(1, &cx).as_ref() == Some(&first));
    assert!(grip(2, &cx).is_some());
    assert!(grip(3, &cx).is_none());
}

#[gpui::test]
fn a_tile_dragged_onto_another_takes_its_place(cx: &mut TestAppContext) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&fixture, &window, &mut cx);
    let first = inline_tile(1, &mut cx).expect("the first tile");
    let third = inline_tile(3, &mut cx).expect("the third tile");

    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(third.center(), MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(
        bounds(bulk_dock::insertion_line_selector(false), &mut cx).is_some(),
        "the insertion line shows where the tile lands"
    );
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [1, 2, 3],
        "nothing moves before the drop"
    );
    cx.simulate_mouse_up(third.center(), MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);

    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [2, 3, 1]
    );
    assert!(bounds(bulk_dock::insertion_line_selector(false), &mut cx).is_none());
    // The selection beneath the dock is what it was.
    assert_eq!(selected(&window, &cx), [1, 2, 3]);
}

#[gpui::test]
fn the_larger_view_starts_from_the_inline_draft_replaces_the_inline_board_and_saves_its_order(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&fixture, &window, &mut cx);
    let grip = dock
        .read_with(&cx, |dock, _| dock.handle_focus(false, 1).cloned())
        .expect("the grip of the first tile");
    cx.update(|window, cx| window.focus(&grip, cx));
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);

    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    let modal = dock.read_with(&cx, |dock, _| dock.order_modal().clone());
    assert!(modal.read_with(&cx, |modal, _| modal.is_open()));
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().modal_order().to_vec()),
        [2, 1, 3]
    );
    assert!(inline_tile(1, &mut cx).is_none(), "never both boards");
    assert!(bounds(bulk_dock::tile_selector(true, 1), &mut cx).is_some());

    // A change in the larger view stays there; Cancel drops it.
    let larger_grip = dock
        .read_with(&cx, |dock, _| dock.handle_focus(true, 3).cloned())
        .expect("a grip in the larger view");
    cx.update(|window, cx| window.focus(&larger_grip, cx));
    cx.simulate_keystrokes("left");
    settle(&fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().modal_order().to_vec()),
        [2, 3, 1]
    );
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [2, 1, 3]
    );
    click(bulk_dock::ORDER_MODAL_CANCEL_ID, &fixture, &mut cx);
    assert!(!modal.read_with(&cx, |modal, _| modal.is_open()));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(&fixture, &mut cx);
    assert!(
        inline_tile(1, &mut cx).is_some(),
        "the inline board is back"
    );

    // Opened again it starts from the inline draft, and Save writes exactly its order.
    click(bulk_dock::OPEN_ORDER_MODAL_ID, &fixture, &mut cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().modal_order().to_vec()),
        [2, 1, 3]
    );
    let larger_grip = dock
        .read_with(&cx, |dock, _| dock.handle_focus(true, 3).cloned())
        .expect("a grip in the larger view");
    cx.update(|window, cx| window.focus(&larger_grip, cx));
    cx.simulate_keystrokes("left left");
    settle(&fixture, &mut cx);
    fixture.backend.take_calls();
    click(bulk_dock::ORDER_MODAL_SAVE_ID, &fixture, &mut cx);
    assert_eq!(
        fixture.take_group_writes(),
        [(
            vec![(3, Some(1.0)), (2, Some(2.0)), (1, Some(3.0))],
            Some("trip".to_owned())
        )]
    );
    assert!(
        !modal.read_with(&cx, |modal, _| modal.is_open()),
        "saved: closed"
    );
    // The model's order is the saved one, and the inline draft restarts from it.
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().inline_order().to_vec()),
        [3, 2, 1]
    );
}

/// `BulkDock.tsx`: the key row is `grid-cols-[minmax(0,1fr)_40px_auto]` with `gap-2`. The
/// "Generate UUID" button (36 pixels) sits at the start of its 40-pixel track, so Apply
/// starts 48 pixels after the button's left edge and ends at the panel's right edge.
#[gpui::test]
fn the_group_key_row_keeps_a_fixed_track_for_the_generate_button(cx: &mut TestAppContext) {
    let fixture = Fixture::library();
    let (_window, mut cx) = open(&fixture, cx);
    select(&[0, 1], &fixture, &mut cx);
    click(DockTab::Group.test_id(), &fixture, &mut cx);

    let panel = shown(bulk_dock::GROUP_PANEL_SELECTOR, &mut cx);
    let key = shown(bulk_dock::GROUP_KEY_SELECTOR, &mut cx);
    let generate = shown(bulk_dock::GENERATE_KEY_ID, &mut cx);
    let apply = shown(bulk_dock::APPLY_GROUP_ID, &mut cx);
    let near = |left: Pixels, right: f32, what: &str| {
        assert!(
            (f32::from(left) - right).abs() < 0.01,
            "{what}: {left:?}, expected {right}"
        );
    };
    near(
        key.left(),
        f32::from(panel.left()),
        "the field starts the row",
    );
    near(
        generate.left() - key.right(),
        group_layout::ROW_GAP,
        "the gap after the field",
    );
    near(generate.size.width, ICON_BUTTON_SIZE, "the button's width");
    near(
        apply.left() - generate.left(),
        group_layout::KEY_BUTTON_TRACK + group_layout::ROW_GAP,
        "the button's track and the gap after it",
    );
    near(
        apply.right(),
        f32::from(panel.right()),
        "Apply ends the row",
    );
    near(
        key.size.height,
        group_layout::INPUT_HEIGHT,
        "the field's height",
    );
    // `items-center`.
    near(key.center().y, f32::from(generate.center().y), "centred");
    near(apply.center().y, f32::from(generate.center().y), "centred");
}

fn vocabulary(fixture: &Fixture, tags: &'static [&'static str]) {
    fixture.backend.on_list_tags(move |_, _, _| {
        Ok(TagListPage {
            items: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            total: tags.len() as i64,
        })
    });
}

fn selection_mode(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> bool {
    runtime(window, cx).read_with(cx, |runtime, _| runtime.model().selection().enabled())
}

/// Escape with the focus in the dock's tag field, as in the Solid interface: the
/// combobox takes the key only while its list shows items (`SearchTagator.tsx` closes
/// the list and stops the event); any other Escape reaches the layer stack, whose top
/// layer in selection mode is the gallery's (`createGridHandlers.ts`, `onEscape:
/// clearSelection`): the selection is cleared and selection mode stays on.
#[gpui::test]
fn escape_in_the_tag_field_closes_its_suggestions_first_and_then_clears_the_selection(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library();
    vocabulary(&fixture, &["travel", "train"]);
    let (window, mut cx) = open(&fixture, cx);
    select(&[0, 1], &fixture, &mut cx);
    let editor = dock(&window, &cx).read_with(&cx, |dock, _| dock.editor().clone());

    let field = shown(tag_editor::INPUT_SELECTOR, &mut cx);
    cx.simulate_click(field.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    cx.simulate_input("tra");
    settle(&fixture, &mut cx);
    assert!(editor.read_with(&cx, |editor, _| editor.combobox().popup()));
    assert!(cx.debug_bounds(tag_editor::LIST_SELECTOR).is_some());

    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(!editor.read_with(&cx, |editor, _| editor.combobox().popup()));
    assert!(cx.debug_bounds(tag_editor::LIST_SELECTOR).is_none());
    assert_eq!(selected(&window, &cx), [1, 2], "the list took the key");
    assert!(cx.update(|window, cx| editor.focus_handle(cx).is_focused(window)));
    assert_eq!(
        editor.read_with(&cx, |editor, cx| editor.draft(cx).to_owned()),
        "tra"
    );

    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(
        selected(&window, &cx).is_empty(),
        "the selection is cleared"
    );
    assert!(selection_mode(&window, &cx), "selection mode stays on");
    assert!(cx.debug_bounds(bulk_dock::CHIP_SELECTOR).is_some());
}

/// The group key field is a plain input in the Solid dock: its Escape goes straight to
/// the layer stack and clears the selection.
#[gpui::test]
fn escape_in_the_group_key_field_clears_the_selection_and_keeps_selection_mode(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&fixture, &window, &mut cx);
    let key = dock.read_with(&cx, |dock, _| dock.group_key_input().clone());
    assert!(cx.update(|window, cx| key.focus_handle(cx).is_focused(window)));
    assert_eq!(selected(&window, &cx), [1, 2, 3]);

    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(
        selected(&window, &cx).is_empty(),
        "the selection is cleared"
    );
    assert!(selection_mode(&window, &cx), "selection mode stays on");
    assert!(cx.debug_bounds(bulk_dock::CHIP_SELECTOR).is_some());
    assert!(fixture.take_group_writes().is_empty());
}

/// `BulkDock::apply` runs on every publish of the runtime and hands the group key
/// field its text and its disabled state again. Values that did not change leave the
/// field alone: its caret, its selection and the focus stay, and it reports nothing.
#[gpui::test]
fn a_publish_of_the_runtime_leaves_the_group_key_field_as_the_user_left_it(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library();
    let (window, mut cx) = open(&fixture, cx);
    let dock = group_tab_with_key(&fixture, &window, &mut cx);
    let key = dock.read_with(&cx, |dock, _| dock.group_key_input().clone());
    cx.simulate_keystrokes("left shift-left shift-left");
    settle(&fixture, &mut cx);
    let state = |cx: &mut VisualTestContext| {
        let (text, selection, disabled) = key.read_with(cx, |key, _| {
            (key.text().to_owned(), key.selection(), key.is_disabled())
        });
        let focused = cx.update(|window, cx| key.focus_handle(cx).is_focused(window));
        (text, selection, disabled, focused)
    };
    let before = state(&mut cx);
    assert_eq!(
        before,
        (
            "trip".to_owned(),
            Selection { anchor: 3, head: 1 },
            false,
            true
        )
    );

    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(&key, move |_, event: &TextInputEvent, _| {
            seen.borrow_mut().push(*event);
        })
    });
    let runtime = runtime(&window, &cx);
    for _ in 0..3 {
        // An input that changes nothing the dock shows, and a bare publish.
        runtime.update(&mut cx, |runtime, cx| {
            runtime.send(
                RuntimeInput::Selection(SelectionInput::Interact(Interaction::RectangleCancel)),
                cx,
            );
        });
        settle(&fixture, &mut cx);
        runtime.update(&mut cx, |_, cx| cx.notify());
        settle(&fixture, &mut cx);
    }
    drop(subscription);

    assert_eq!(state(&mut cx), before);
    assert_eq!(*events.borrow(), []);
    assert_eq!(selected(&window, &cx), [1, 2, 3]);
}

/// The main window in the start state of scenario `id`, over the scripted library.
fn open_scenario(
    id: &str,
    fixture: &Fixture,
    cx: &mut TestAppContext,
) -> (WindowHandle<ShellView>, VisualTestContext) {
    let start = start_state(profile::E2E, Some(id)).expect("a scenario of the E2E profile");
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window_started(
                profile::E2E,
                fixture.parts(),
                ShellStart {
                    tile_size: start.tile_size,
                    selection: start.selection,
                    search: start.search,
                    tag_list: start.tag_list,
                    dock: start.dock,
                },
                cx,
            )
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.update(|window, _| window.activate_window());
    settle(fixture, &mut cx);
    settle(fixture, &mut cx);
    (window, cx)
}

#[gpui::test]
fn the_tags_scenario_starts_with_its_tag_added_to_the_three_tiles(cx: &mut TestAppContext) {
    let fixture = Fixture::library();
    let (window, mut cx) = open_scenario("bulk-dock-tags", &fixture, cx);

    let mut ids = selected(&window, &cx);
    ids.sort_unstable();
    assert_eq!(ids, [1, 2, 3]);
    let dock = dock(&window, &cx);
    assert_eq!(
        dock.read_with(&cx, |dock, _| dock.dock().tab()),
        DockTab::Tags
    );
    let editor = dock.read_with(&cx, |dock, _| dock.editor().clone());
    assert_eq!(
        editor.read_with(&cx, |editor, _| editor.props().tags.clone()),
        ["holiday"]
    );
    assert_eq!(
        editor.read_with(&cx, |editor, cx| editor.draft(cx).to_owned()),
        ""
    );
    // As a capture of the Tauri window: no field has the focus.
    assert!(!cx.update(|window, cx| editor.focus_handle(cx).is_focused(window)));
    assert!(cx.debug_bounds(bulk_dock::TAGS_PANEL_SELECTOR).is_some());
}

#[gpui::test]
fn the_group_scenarios_start_with_the_key_typed_and_the_board_inline_or_in_the_larger_view(
    cx: &mut TestAppContext,
) {
    for (id, larger) in [("bulk-dock-group", false), ("group-order-modal", true)] {
        let fixture = Fixture::library();
        let (window, mut cx) = open_scenario(id, &fixture, cx);

        // The Shift click replaces the selection with the range of the first three
        // tiles, so the selection and the board follow the gallery's order (ID = index
        // + 1 here), not the order of the clicks (second, first, third).
        assert_eq!(selected(&window, &cx), [1, 2, 3], "{id}");
        let dock = dock(&window, &cx);
        assert_eq!(
            dock.read_with(&cx, |dock, _| if larger {
                dock.dock().modal_order().to_vec()
            } else {
                dock.dock().inline_order().to_vec()
            }),
            [1, 2, 3],
            "{id}"
        );
        assert_eq!(
            dock.read_with(&cx, |dock, _| dock.dock().tab()),
            DockTab::Group,
            "{id}"
        );
        let key = dock.read_with(&cx, |dock, _| dock.group_key_input().clone());
        assert_eq!(
            key.read_with(&cx, |key, _| key.text().to_owned()),
            "trip",
            "{id}"
        );
        let modal = dock.read_with(&cx, |dock, _| dock.order_modal().clone());
        assert_eq!(
            modal.read_with(&cx, |modal, _| modal.is_open()),
            larger,
            "{id}"
        );
        assert_eq!(inline_tile(1, &mut cx).is_some(), !larger, "{id}");
        assert_eq!(
            bounds(bulk_dock::tile_selector(true, 1), &mut cx).is_some(),
            larger,
            "{id}"
        );
        assert!(
            fixture.take_group_writes().is_empty(),
            "{id}: nothing is saved"
        );
    }
}
