//! The lightbox in the main window (epic E16) on GPUI's test platform with the scripted
//! backend and the deterministic executor: a tile opens it, the picture is decoded
//! from a file of a temporary directory, the viewer's keys zoom and navigate, Escape
//! closes the innermost thing, the delete confirmation sends one deletion, and a
//! narrow window has a drawer. The picture files are synthetic and live in a
//! temporary directory; deletions are answered by the fake and touch no file.

use std::{sync::Arc, time::Duration};

use gpui::{point, px, size, Entity, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use media_tagger_client::{
    backend::{fake::Call, FakeBackend, ThumbnailImage, ThumbnailReply},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    lightbox::{FocusTarget, LoadState},
    prefs::MemoryPrefsStore,
    runtime::RuntimeConfig,
    scenario::{ScenarioLightbox, LIGHTBOX_DELETE_CONFIRM, LIGHTBOX_IMAGE_ZOOMED},
    theme::{ThemeName, TYPOGRAPHY},
};
use media_tagger_core::models::{
    AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySectionsResult,
    AssetSummary, DeleteAssetSummary, DeleteSourceStatus, ScanRoot, StartAssetQueryResult,
    TagListPage,
};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui,
    views::{
        gallery,
        lightbox::{self, LightboxView},
        shell::ShellView,
    },
    window::{init_application, open_main_window, open_main_window_started, ShellStart},
};
use tempfile::TempDir;

/// An 8 x 8 JPEG: 256 bytes once decoded.
const JPEG: [u8; 285] = [
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x0d, 0x09, 0x0a, 0x0b, 0x0a, 0x08, 0x0d,
    0x0b, 0x0a, 0x0b, 0x0e, 0x0e, 0x0d, 0x0f, 0x13, 0x20, 0x15, 0x13, 0x12, 0x12, 0x13, 0x27, 0x1c,
    0x1e, 0x17, 0x20, 0x2e, 0x29, 0x31, 0x30, 0x2e, 0x29, 0x2d, 0x2c, 0x33, 0x3a, 0x4a, 0x3e, 0x33,
    0x36, 0x46, 0x37, 0x2c, 0x2d, 0x40, 0x57, 0x41, 0x46, 0x4c, 0x4e, 0x52, 0x53, 0x52, 0x32, 0x3e,
    0x5a, 0x61, 0x5a, 0x50, 0x60, 0x4a, 0x51, 0x52, 0x4f, 0xff, 0xdb, 0x00, 0x43, 0x01, 0x0e, 0x0e,
    0x0e, 0x13, 0x11, 0x13, 0x26, 0x15, 0x15, 0x26, 0x4f, 0x35, 0x2d, 0x35, 0x4f, 0x4f, 0x4f, 0x4f,
    0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f,
    0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f,
    0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0xff, 0xc0,
    0x00, 0x11, 0x08, 0x00, 0x08, 0x00, 0x08, 0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11,
    0x01, 0xff, 0xc4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0xff, 0xc4, 0x00, 0x14, 0x10, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xc4, 0x00,
    0x15, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x05, 0x06, 0xff, 0xc4, 0x00, 0x14, 0x11, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xda, 0x00, 0x0c, 0x03, 0x01,
    0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0x32, 0x80, 0x43, 0xff, 0xd9,
];

const TOTAL: usize = 6;

fn asset(index: usize) -> AssetSummary {
    let id = index as i64 + 1;
    AssetSummary {
        id,
        file_name: format!("asset-{id}.jpg"),
        preview_path: None,
        kind: "image".to_owned(),
        modified_at: 0,
        width: Some(8),
        height: Some(8),
        duration_ms: None,
        thumb_version: format!("{id:016x}"),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    /// Holds the synthetic picture files.
    _media: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let media = tempfile::tempdir().expect("a temporary media directory");
        for index in 0..TOTAL {
            std::fs::write(media.path().join(asset(index).file_name), JPEG)
                .expect("write a synthetic picture");
        }
        let root = media.path().to_path_buf();
        let rows = || (0..TOTAL).map(asset).collect::<Vec<_>>();
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(move |_| {
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total: TOTAL,
                    offset: 0,
                    items: rows(),
                })
            })
            .on_get_asset_query_page(move |_, offset, _| {
                Ok(AssetQueryPageResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total: TOTAL,
                    offset,
                    items: rows().split_off(offset.min(TOTAL)),
                })
            })
            .on_get_asset_query_sections(|_| {
                Ok(AssetQuerySectionsResult::Ready {
                    session_id: 1,
                    total: TOTAL,
                    sections: Vec::new(),
                })
            })
            .on_get_asset_query_position(|_, asset_id| {
                Ok(usize::try_from(asset_id - 1)
                    .ok()
                    .filter(|index| *index < TOTAL)
                    .map_or(AssetQueryPositionResult::Missing, |index| {
                        AssetQueryPositionResult::Resolved { index }
                    }))
            })
            .on_get_asset_details(move |asset_id| {
                let summary =
                    asset(usize::try_from(asset_id - 1).expect("an asset of the library"));
                let path = root.join(&summary.file_name);
                Ok(Some(AssetDetails {
                    summary,
                    path: path.to_string_lossy().into_owned(),
                    size_bytes: JPEG.len() as i64,
                    thumb_path: None,
                    tags: vec!["cat".to_owned()],
                }))
            })
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: vec!["cat".into()],
                    total: 1,
                })
            })
            .on_delete_asset(|_| {
                Ok(DeleteAssetSummary {
                    removed_assets: 1,
                    removed_thumbnails: 0,
                    source_status: DeleteSourceStatus::Deleted,
                    revision: 2,
                    recovery_path: None,
                })
            })
            .on_request_thumbnail(|request, responder| {
                responder.respond(ThumbnailReply::Ready(ThumbnailImage {
                    bytes: JPEG.to_vec(),
                    version: request.version,
                }));
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
            _media: media,
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

    fn deletions(&self) -> Vec<i64> {
        self.backend
            .take_calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::DeleteAsset { asset_id } => Some(asset_id),
                _ => None,
            })
            .collect()
    }
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
    settle(fixture, &mut cx);
    (window, cx)
}

/// Runs the backend calls and the background decodes, lets the answers reach the UI
/// thread and draws the next frame, in which the focus the lightbox asked for moves.
fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    for _ in 0..2 {
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

/// Lets a fade or a slide end.
fn rest(fixture: &Fixture, cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(300));
    settle(fixture, cx);
}

fn lightbox_view(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<LightboxView> {
    window
        .read_with(cx, |shell, _| shell.lightbox().clone())
        .expect("the window is open")
}

fn selected_id(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Option<i64> {
    window
        .read_with(cx, |shell, cx| {
            shell
                .runtime()
                .read(cx)
                .model()
                .lightbox()
                .selected()
                .map(|selected| selected.id())
        })
        .expect("the window is open")
}

fn click_selector(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
}

fn open_tile(fixture: &Fixture, index: usize, cx: &mut VisualTestContext) {
    let tile = cx
        .debug_bounds(Box::leak(gallery::tile_selector(index).into_boxed_str()))
        .expect("the tile is drawn");
    cx.simulate_click(tile.center(), Modifiers::none());
    settle(fixture, cx);
}

#[gpui::test]
fn a_tile_opens_the_lightbox_with_its_decoded_picture_and_the_keys_zoom_and_navigate(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    assert!(cx.debug_bounds(lightbox::DIALOG_SELECTOR).is_none());

    open_tile(&fixture, 1, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    assert!(view.read_with(&cx, |view, _| view.is_open()));
    let dialog = cx
        .debug_bounds(lightbox::DIALOG_SELECTOR)
        .expect("the dialog is drawn");
    let sidebar = cx
        .debug_bounds(lightbox::SIDEBAR_SELECTOR)
        .expect("the side panel is open in a wide window");
    assert_eq!(
        sidebar.right(),
        dialog.right() - px(1.0),
        "inside the border"
    );
    assert_eq!(sidebar.size.width, px(340.0));

    // The picture was read from its file and decoded in the background.
    let (state, size) = view.read_with(&cx, |view, cx| {
        let originals = view.originals().read(cx);
        (originals.state(), originals.size())
    });
    assert_eq!((state, size), (LoadState::Ready, Some((8, 8))));
    let stage = cx
        .debug_bounds(lightbox::STAGE_SELECTOR)
        .expect("the stage is drawn");
    let image = cx
        .debug_bounds(lightbox::IMAGE_SELECTOR)
        .expect("the picture is drawn");
    // A square picture is fitted to the stage's height and centred.
    assert_eq!(image.size.height, stage.size.height);
    assert_eq!(image.size.width, stage.size.height);
    assert_eq!(image.center().x, stage.center().x);

    // The viewer's keys.
    cx.simulate_keystrokes("+");
    cx.simulate_keystrokes("=");
    let zoom = view.read_with(&cx, |view, _| view.session().map(|s| s.controls().zoom()));
    assert_eq!(zoom, Some(1.5625));
    let zoomed = cx.debug_bounds(lightbox::IMAGE_SELECTOR).expect("zoomed");
    assert!(zoomed.size.width > image.size.width);
    cx.simulate_keystrokes("-");
    cx.simulate_keystrokes("0");
    let zoom = view.read_with(&cx, |view, _| view.session().map(|s| s.controls().zoom()));
    assert_eq!(zoom, Some(1.0));

    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(3));
    cx.simulate_keystrokes("left");
    cx.simulate_keystrokes("left");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));

    // A click on the picture zooms in at the pointer; the wheel zooms out again.
    let image = cx
        .debug_bounds(lightbox::IMAGE_SELECTOR)
        .expect("the picture");
    cx.simulate_click(image.center(), Modifiers::none());
    let zoom = view.read_with(&cx, |view, _| view.session().map(|s| s.controls().zoom()));
    assert_eq!(zoom, Some(1.25));

    // F makes the window fullscreen and the dialog fills it; Escape leaves
    // fullscreen first and closes the lightbox second.
    cx.simulate_keystrokes("f");
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.is_fullscreen()));
    let viewport = cx.update(|window, _| window.viewport_size());
    let dialog = cx.debug_bounds(lightbox::DIALOG_SELECTOR).expect("dialog");
    assert_eq!(dialog.size, viewport);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    assert_eq!(selected_id(&window, &cx), Some(1));
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), None);
    assert!(!view.read_with(&cx, |view, _| view.is_open()));
    // It fades out showing what it showed, then it is gone.
    assert!(view.read_with(&cx, |view, _| view.shown().is_some()));
    rest(&fixture, &mut cx);
    assert!(view.read_with(&cx, |view, _| view.shown().is_none()));
    assert!(cx.debug_bounds(lightbox::DIALOG_SELECTOR).is_none());
}

#[gpui::test]
fn the_delete_confirmation_takes_the_typed_word_and_sends_one_deletion(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    assert!(cx.debug_bounds(lightbox::CONFIRM_SELECTOR).is_none());

    click_selector(lightbox::DELETE_ID, &mut cx);
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(lightbox::CONFIRM_SELECTOR).is_some());
    // The confirmation's field has the focus.
    let focused = view.read_with(&cx, |view, cx| {
        view.focus_handle_of(FocusTarget::ConfirmInput, cx)
    });
    assert!(cx.update(|window, _| focused.is_focused(window)));

    // Escape closes the confirmation and keeps the lightbox.
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(lightbox::CONFIRM_SELECTOR).is_none());
    assert_eq!(selected_id(&window, &cx), Some(1));

    click_selector(lightbox::DELETE_ID, &mut cx);
    settle(&fixture, &mut cx);
    // Confirm is disabled until the word is typed, in any case.
    click_selector(lightbox::CONFIRM_SUBMIT_ID, &mut cx);
    settle(&fixture, &mut cx);
    assert!(fixture.deletions().is_empty());
    cx.simulate_input("yes");
    settle(&fixture, &mut cx);
    assert_eq!(
        view.read_with(&cx, |view, _| view
            .session()
            .map(|session| session.confirm().text().to_owned())),
        Some("yes".to_owned())
    );
    click_selector(lightbox::CONFIRM_SUBMIT_ID, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(fixture.deletions(), [1]);
    assert_eq!(selected_id(&window, &cx), None, "the deleted asset closes");
}

#[gpui::test]
fn a_narrow_window_has_a_drawer_that_escape_closes_before_the_lightbox(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    cx.simulate_resize(size(px(700.0), px(720.0)));
    settle(&fixture, &mut cx);
    open_tile(&fixture, 0, &mut cx);

    assert!(cx.debug_bounds(lightbox::SIDEBAR_SELECTOR).is_none());
    assert!(cx.debug_bounds(lightbox::TRIGGER_ID).is_some());
    click_selector(lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    let dialog = cx.debug_bounds(lightbox::DIALOG_SELECTOR).expect("dialog");
    let sidebar = cx.debug_bounds(lightbox::SIDEBAR_SELECTOR).expect("drawer");
    assert_eq!(sidebar.size.width, px(340.0));
    assert!(sidebar.right() <= dialog.right());
    assert!(cx.debug_bounds(lightbox::SIDEBAR_SCRIM_SELECTOR).is_some());
    // The drawer covers the media: the viewer's keys are off.
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));

    cx.simulate_keystrokes("escape");
    rest(&fixture, &mut cx);
    assert!(cx.debug_bounds(lightbox::SIDEBAR_SELECTOR).is_none());
    assert_eq!(selected_id(&window, &cx), Some(1));
    assert!(view.read_with(&cx, |view, _| view.is_open()));

    // A press on the scrim closes the drawer as well.
    click_selector(lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    let scrim = cx
        .debug_bounds(lightbox::SIDEBAR_SCRIM_SELECTOR)
        .expect("the scrim");
    cx.simulate_click(scrim.origin + point(px(8.0), px(8.0)), Modifiers::none());
    rest(&fixture, &mut cx);
    assert!(cx.debug_bounds(lightbox::SIDEBAR_SELECTOR).is_none());

    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), None);
}

/// The control with the keyboard focus, by its debug selector: the one the focus ring
/// is drawn around.
fn ringed(stops: &[&'static str], cx: &mut VisualTestContext) -> Option<&'static str> {
    let ring = cx.debug_bounds(ui::theme::FOCUS_RING_SELECTOR)?;
    stops.iter().copied().find(|stop| {
        cx.debug_bounds(stop).is_some_and(|control| {
            ring.left() <= control.left()
                && ring.top() <= control.top()
                && ring.right() >= control.right()
                && ring.bottom() >= control.bottom()
                && ring.size.width - control.size.width <= px(12.0)
                && ring.size.height - control.size.height <= px(12.0)
        })
    })
}

/// Tab goes through the dialog in the order of the DOM of `LightboxModal.tsx`. With the
/// panel closed the floating buttons (open the panel, close) come first, although they
/// are drawn above the stage and so after the action rail; then the rail. With the
/// panel open the rail comes before the panel. Shift+Tab goes the same way back.
#[gpui::test]
fn tab_goes_round_the_dialog_in_the_order_of_the_solid_dom(cx: &mut TestAppContext) {
    const CLOSED: [&str; 6] = [
        lightbox::TRIGGER_ID,
        lightbox::FLOATING_CLOSE_ID,
        lightbox::FAVORITE_ID,
        lightbox::RESET_ZOOM_ID,
        lightbox::FULLSCREEN_ID,
        lightbox::DELETE_ID,
    ];
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    // A narrow window opens the lightbox with the panel closed.
    cx.simulate_resize(size(px(700.0), px(720.0)));
    settle(&fixture, &mut cx);
    open_tile(&fixture, 0, &mut cx);
    assert!(cx.debug_bounds(lightbox::SIDEBAR_SELECTOR).is_none());
    // The lightbox opened by a click: the focus is on the reopen button, without a ring.
    assert_eq!(ringed(&CLOSED, &mut cx), None);

    let press = |keys: &str, cx: &mut VisualTestContext| {
        cx.simulate_keystrokes(keys);
        settle(&fixture, cx);
        ringed(&CLOSED, cx)
    };
    // A whole round from the reopen button, and one stop more.
    let forward: Vec<_> = (0..=CLOSED.len()).map(|_| press("tab", &mut cx)).collect();
    let expected: Vec<_> = (1..=CLOSED.len() + 1)
        .map(|index| Some(CLOSED[index % CLOSED.len()]))
        .collect();
    assert_eq!(forward, expected, "Tab");
    assert_eq!(forward.last(), Some(&Some(lightbox::FLOATING_CLOSE_ID)));
    // The same way back, past the first stop to the last.
    let backward: Vec<_> = (0..CLOSED.len())
        .map(|_| press("shift-tab", &mut cx))
        .collect();
    let expected: Vec<_> = [0, 5, 4, 3, 2, 1].map(|index| Some(CLOSED[index])).to_vec();
    assert_eq!(backward, expected, "Shift+Tab");
    assert!(view.read_with(&cx, |view, _| view.is_open()));
    assert_eq!(selected_id(&window, &cx), Some(1), "Tab moved no picture");

    // The panel open: the rail's last button is followed by the panel's first, and
    // Shift+Tab from the rail's first button wraps to the panel's last.
    const OPEN: [&str; 4] = [
        lightbox::FAVORITE_ID,
        lightbox::DELETE_ID,
        lightbox::SIDEBAR_CLOSE_ID,
        lightbox::INFO_TOGGLE_ID,
    ];
    cx.simulate_keystrokes("escape");
    rest(&fixture, &mut cx);
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    settle(&fixture, &mut cx);
    open_tile(&fixture, 0, &mut cx);
    rest(&fixture, &mut cx);
    assert!(cx.debug_bounds(lightbox::SIDEBAR_SELECTOR).is_some());
    assert!(cx.debug_bounds(lightbox::TRIGGER_ID).is_none());
    let press = |keys: &str, cx: &mut VisualTestContext| {
        cx.simulate_keystrokes(keys);
        settle(&fixture, cx);
        ringed(&OPEN, cx)
    };
    let until = |stop: &'static str, cx: &mut VisualTestContext| {
        for _ in 0..24 {
            if press("tab", cx) == Some(stop) {
                return;
            }
        }
        panic!("Tab never reaches {stop}");
    };
    until(lightbox::FAVORITE_ID, &mut cx);
    assert_eq!(
        press("shift-tab", &mut cx),
        Some(lightbox::INFO_TOGGLE_ID),
        "before the rail's first button: the panel's last stop"
    );
    assert_eq!(press("tab", &mut cx), Some(lightbox::FAVORITE_ID));
    until(lightbox::DELETE_ID, &mut cx);
    assert_eq!(
        press("tab", &mut cx),
        Some(lightbox::SIDEBAR_CLOSE_ID),
        "after the rail's last button: the panel's first stop"
    );
}

/// The main window in a start state that opens the lightbox, as
/// `media_tagger_gpui --scenario lightbox-*` starts it, over the scripted library.
fn open_scripted(
    fixture: &Fixture,
    lightbox: ScenarioLightbox,
    cx: &mut TestAppContext,
) -> (WindowHandle<ShellView>, VisualTestContext) {
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window_started(
                profile::E2E,
                fixture.parts(),
                ShellStart {
                    lightbox: Some(lightbox),
                    settings: None,
                    ..ShellStart::default()
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

/// The start of `lightbox-image-zoomed` on another tile (`Runtime::script_lightbox`,
/// `Runtime::take_scripted_lightbox`, the view's scripted steps): the tile is clicked
/// once its row is loaded, and the picture is zoomed and dragged once it is on the
/// stage. The controls of a scripted lightbox never fade and none of its fields has
/// the focus, as a capture of the Tauri window shows it.
#[gpui::test]
fn a_scripted_start_opens_the_lightbox_on_its_tile_zoomed_and_dragged(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let script = ScenarioLightbox {
        index: 2,
        ..LIGHTBOX_IMAGE_ZOOMED
    };
    let (window, mut cx) = open_scripted(&fixture, script, cx);
    let view = lightbox_view(&window, &cx);

    assert_eq!(selected_id(&window, &cx), Some(3), "the tile at index 2");
    assert!(view.read_with(&cx, |view, _| view.is_open()));
    let state = view.read_with(&cx, |view, cx| view.originals().read(cx).state());
    assert_eq!(state, LoadState::Ready);
    let (zoom, pan, confirm) = view.read_with(&cx, |view, _| {
        let session = view.session().expect("the session of the open lightbox");
        (
            session.controls().zoom(),
            session.controls().pan(),
            session.confirm().is_open(),
        )
    });
    assert_eq!(zoom, 1.25_f64.powi(i32::from(script.zoom_in)));
    assert_eq!(
        (pan.x, pan.y),
        (f64::from(script.drag.0), f64::from(script.drag.1)),
        "dragged from the centre, within the picture's bounds"
    );
    assert!(!confirm);
    // The picture is drawn where the zoom and the drag put it, to the pixel the
    // layout rounds to.
    let stage = cx.debug_bounds(lightbox::STAGE_SELECTOR).expect("stage");
    let image = cx.debug_bounds(lightbox::IMAGE_SELECTOR).expect("picture");
    assert!(image.size.width > stage.size.width);
    let offset = image.center() - stage.center();
    assert!(
        (f32::from(offset.x) - script.drag.0 as f32).abs() < 1.0
            && (f32::from(offset.y) - script.drag.1 as f32).abs() < 1.0,
        "{offset:?}"
    );

    // No field has the focus (a capture shows no caret), and the controls do not
    // fade.
    let fields = view.read_with(&cx, |view, cx| {
        [
            view.focus_handle_of(FocusTarget::TagInput, cx),
            view.focus_handle_of(FocusTarget::ConfirmInput, cx),
            gpui::Focusable::focus_handle(view.group_key_input().read(cx), cx),
            gpui::Focusable::focus_handle(view.group_order_input().read(cx), cx),
        ]
    });
    assert!(!cx.update(|window, _| fields.iter().any(|field| field.is_focused(window))));
    cx.executor().advance_clock(Duration::from_secs(10));
    settle(&fixture, &mut cx);
    assert!(view.read_with(&cx, |view, cx| {
        let now = view.now(cx);
        view.session().unwrap().controls_visible(now)
    }));
    assert_eq!(selected_id(&window, &cx), Some(3), "opened once");
}

/// The start of `lightbox-delete-confirm`: the confirmation is open in the side
/// panel with the scenario's word typed, Confirm is enabled, the field has no focus
/// and nothing is deleted.
#[gpui::test]
fn a_scripted_start_opens_the_delete_confirmation_with_its_word_typed(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let script = ScenarioLightbox {
        index: 4,
        ..LIGHTBOX_DELETE_CONFIRM
    };
    let word = script.delete_confirm.expect("the scenario types a word");
    let (window, mut cx) = open_scripted(&fixture, script, cx);
    let view = lightbox_view(&window, &cx);

    assert_eq!(selected_id(&window, &cx), Some(5), "the tile at index 4");
    assert!(cx.debug_bounds(lightbox::CONFIRM_SELECTOR).is_some());
    assert!(cx.debug_bounds(lightbox::SIDEBAR_SELECTOR).is_some());
    let (open, text, can_confirm, zoom) = view.read_with(&cx, |view, _| {
        let session = view.session().expect("the session of the open lightbox");
        (
            session.confirm().is_open(),
            session.confirm().text().to_owned(),
            session.confirm().can_confirm("Yes"),
            session.controls().zoom(),
        )
    });
    assert!(open);
    assert_eq!(text, word);
    assert!(can_confirm, "the typed word is the confirmation word");
    assert_eq!(zoom, 1.0);
    let field = view.read_with(&cx, |view, _| view.confirm_input().clone());
    assert_eq!(
        field.read_with(&cx, |field, _| field.text().to_owned()),
        word
    );
    let focus = view.read_with(&cx, |view, cx| {
        view.focus_handle_of(FocusTarget::ConfirmInput, cx)
    });
    assert!(!cx.update(|window, _| focus.is_focused(window)));
    assert!(fixture.deletions().is_empty(), "nothing is deleted");

    // The enabled Confirm deletes the asset the scenario opened.
    click_selector(lightbox::CONFIRM_SUBMIT_ID, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(fixture.deletions(), [5]);
}

/// GPUI paints a deferred element, which every dialog is, without the text styles of
/// the ancestors outside it. The field reads its line height while it is prepainted,
/// so its text sits in the middle of the field and not 3 px below it (the window's
/// default line height is 26 px, the application's 19.5 px).
#[gpui::test]
fn the_fields_of_the_dialog_draw_their_text_in_the_line_height_around_them(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    open_tile(&fixture, 1, &mut cx);
    rest(&fixture, &mut cx);
    let view = lightbox_view(&window, &cx);

    // `Window::line_height` rounds to whole pixels.
    let expected = px((TYPOGRAPHY.body_size * TYPOGRAPHY.root_line_height).round());
    for (name, selector, drawn) in [
        (
            "group key",
            lightbox::GROUP_KEY_SELECTOR,
            view.read_with(&cx, |view, cx| {
                let input = view.group_key_input().read(cx);
                (input.text_bounds(), input.painted_line_height())
            }),
        ),
        (
            "group order",
            lightbox::GROUP_ORDER_SELECTOR,
            view.read_with(&cx, |view, cx| {
                let input = view.group_order_input().read(cx);
                (input.text_bounds(), input.painted_line_height())
            }),
        ),
    ] {
        let field = cx.debug_bounds(selector).expect(name);
        let (text, line_height) = drawn;
        let text = text.expect("the line was drawn");
        assert_eq!(line_height, Some(expected), "{name}");
        // The line is in the middle of the field, to the pixel the layout rounds to.
        let above = text.top() - field.top();
        let below = field.bottom() - text.bottom();
        assert!(
            (above - below).abs() <= px(1.),
            "{name}: {above:?} {below:?}"
        );
    }
}

/// `listboxAriaLabel={t("lightbox.taggingSuggestions")}` of `LightboxTagPanel`: the tag
/// editor of the side panel names its suggestion list (epic E19).
#[gpui::test]
fn the_tag_editor_of_the_side_panel_names_its_suggestion_list(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    open_tile(&fixture, 0, &mut cx);
    let view = lightbox_view(&window, &cx);
    let editor = view.read_with(&cx, |view, _| view.editor().clone());
    let (list, field) = editor.read_with(&cx, |editor, _| {
        (
            editor.props().list_label.clone(),
            editor.props().input_label.clone(),
        )
    });
    assert_eq!(list.as_ref(), "Tagging suggestions");
    assert_eq!(field.as_ref(), "Add tag");
}

/// The field of the delete confirmation is drawn inside the deferred dialog as well:
/// its text sits in the middle of the field in the line height around it, not in the
/// window's default 26 px (the comparison scenario `lightbox-delete-confirm`, epic E19).
#[gpui::test]
fn the_confirmation_field_draws_its_text_in_the_line_height_around_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    open_tile(&fixture, 0, &mut cx);
    rest(&fixture, &mut cx);
    click_selector(lightbox::DELETE_ID, &mut cx);
    settle(&fixture, &mut cx);
    cx.simulate_input("yes");
    settle(&fixture, &mut cx);
    rest(&fixture, &mut cx);
    let view = lightbox_view(&window, &cx);

    let expected = px((TYPOGRAPHY.body_size * TYPOGRAPHY.root_line_height).round());
    let (text, line_height) = view.read_with(&cx, |view, cx| {
        let input = view.confirm_input().read(cx);
        (input.text_bounds(), input.painted_line_height())
    });
    let field = cx
        .debug_bounds(lightbox::CONFIRM_INPUT_SELECTOR)
        .expect("the confirmation field");
    let text = text.expect("the line was drawn");
    assert_eq!(line_height, Some(expected));
    assert_ne!(line_height, Some(px(26.)), "not the window's default");
    let above = text.top() - field.top();
    let below = field.bottom() - text.bottom();
    assert!((above - below).abs() <= px(1.), "{above:?} {below:?}");
}
