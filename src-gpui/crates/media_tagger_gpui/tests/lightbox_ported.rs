//! The view halves of the Solid lightbox suite (epic E16), on GPUI's test platform with
//! the scripted backend on the deterministic executor: the cases of
//! `src/features/lightbox/__tests__/LightboxModal.test.tsx` that test the mounted
//! modal (hierarchy, opening and exit, modality, the narrow drawer, a nested overlay,
//! the tag field, details Retry, media failure, the GIF node, and the focus and exit
//! halves of the delete confirmation cases), the window halves of
//! `keyboard.test.tsx` and `imageControls.test.tsx`, the `defaultPrevented` half of
//! `presentation.test.ts` ("reveals without consuming") and the two view cases of
//! `MediaGroupSetter.test.tsx`. Each test names the TS case it ports in its doc
//! comment. The model halves are `media_tagger_client/tests/lightbox_ported.rs`, the
//! viewer's `media_tagger_client/tests/lightbox_viewer_ported.rs`.
//!
//! A pending promise of the Solid suite is a backend call the test has not run yet:
//! `frame` draws without running the `ManualExecutor`, `settle` runs everything. The
//! picture files are synthetic and live in a temporary directory; deletions are
//! answered by the fake and touch no file.

use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use gpui::{
    point, px, size, AppContext as _, BackgroundExecutor, Entity, Modifiers, MouseButton,
    ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext, WindowHandle,
};
use media_tagger_client::{
    backend::{fake::Call, BackendError, FakeBackend, ThumbnailImage, ThumbnailReply},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    lightbox::{info_rows, FocusTarget, LightboxCommand, LoadState, MediaSource, StageKind},
    overlay::Phase,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    theme::ThemeName,
};
use media_tagger_core::models::{
    AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySectionsResult,
    AssetSummary, DeleteAssetSummary, DeleteSourceStatus, ScanRoot, SetAssetTagsSummary,
    StartAssetQueryResult, TagListPage, TagQueryImpact,
};
use media_tagger_gpui::{
    media::{self, Originals, READ_DEADLINE},
    profile,
    runtime::RuntimeParts,
    ui::layers::{self, LayerOptions},
    views::{
        gallery,
        lightbox::{self, LightboxView},
        shell::ShellView,
        tag_editor::{self, TagEditorEvent},
    },
    window::{init_application, open_main_window},
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

/// A GIF of two 1 x 1 frames.
fn gif() -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
    bytes.extend_from_slice(&[0x21, 0xff, 11]);
    bytes.extend_from_slice(b"NETSCAPE2.0");
    bytes.extend_from_slice(&[3, 1, 0, 0, 0]);
    for _ in 0..2 {
        bytes.extend_from_slice(&[0x21, 0xf9, 4, 0, 10, 0, 0, 0]);
        bytes.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
        bytes.extend_from_slice(&[2, 2, 0x44, 0x01, 0]);
    }
    bytes.push(0x3b);
    bytes
}

const TOTAL: usize = 6;
const IMAGES: [&str; TOTAL] = ["image"; TOTAL];

fn file_name(id: i64, kind: &str) -> String {
    match kind {
        "gif" => format!("asset-{id}.gif"),
        "video" => format!("asset-{id}.mp4"),
        _ => format!("asset-{id}.jpg"),
    }
}

fn summary_of(index: usize, kinds: &[&'static str; TOTAL], root: &std::path::Path) -> AssetSummary {
    let id = index as i64 + 1;
    let kind = kinds[index];
    let name = file_name(id, kind);
    // A GIF or video row carries its verified preview path; an image waits for the
    // details.
    let preview_path = (kind != "image").then(|| root.join(&name).to_string_lossy().into_owned());
    AssetSummary {
        id,
        file_name: name,
        preview_path,
        kind: kind.to_owned(),
        modified_at: 0,
        width: Some(8),
        height: Some(8),
        duration_ms: (kind == "video").then_some(5_000),
        thumb_version: format!("{id:016x}"),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    kinds: [&'static str; TOTAL],
    /// The details fail while set.
    details_fail: Arc<AtomicBool>,
    /// Holds the synthetic media files.
    media: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self::with_kinds(IMAGES)
    }

    fn with_kinds(kinds: [&'static str; TOTAL]) -> Self {
        let media = tempfile::tempdir().expect("a temporary media directory");
        for (index, kind) in kinds.iter().enumerate() {
            let bytes = match *kind {
                "gif" => gif(),
                "video" => b"not decoded by the lightbox".to_vec(),
                _ => JPEG.to_vec(),
            };
            std::fs::write(media.path().join(file_name(index as i64 + 1, kind)), bytes)
                .expect("write a synthetic media file");
        }
        let root = media.path().to_path_buf();
        let rows = {
            let root = root.clone();
            move || {
                (0..TOTAL)
                    .map(|index| summary_of(index, &kinds, &root))
                    .collect::<Vec<_>>()
            }
        };
        let page_rows = rows.clone();
        let tags: Arc<Mutex<HashMap<i64, Vec<String>>>> = Arc::default();
        let details_fail = Arc::new(AtomicBool::new(false));
        let backend = Arc::new(FakeBackend::new());
        let details_tags = tags.clone();
        let fail = details_fail.clone();
        let saved_tags = tags.clone();
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
                    items: page_rows().split_off(offset.min(TOTAL)),
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
                if fail.load(Ordering::SeqCst) {
                    return Err(BackendError::rejected("read"));
                }
                let index = usize::try_from(asset_id - 1).expect("an asset of the library");
                let summary = summary_of(index, &kinds, &root);
                let path = root.join(&summary.file_name);
                let tags = details_tags
                    .lock()
                    .unwrap()
                    .get(&asset_id)
                    .cloned()
                    .unwrap_or_else(|| vec!["cat".to_owned()]);
                Ok(Some(AssetDetails {
                    summary,
                    path: path.to_string_lossy().into_owned(),
                    size_bytes: JPEG.len() as i64,
                    thumb_path: None,
                    tags,
                }))
            })
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: vec!["cat".into(), "dog".into(), "travel".into()],
                    total: 3,
                })
            })
            .on_set_asset_tags(move |asset_id, tags| {
                saved_tags.lock().unwrap().insert(asset_id, tags.clone());
                Ok(SetAssetTagsSummary {
                    vocabulary_changed: false,
                    query_impact: TagQueryImpact::None,
                    asset_id,
                    changed: true,
                    tags,
                    revision: 2,
                })
            })
            .on_set_asset_favorite(|_, _| Ok(()))
            .on_set_asset_media_group(|_, _, _| Ok(()))
            .on_delete_asset(|_| Ok(deleted()))
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
            kinds,
            details_fail,
            media,
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

    fn summary(&self, index: usize) -> AssetSummary {
        summary_of(index, &self.kinds, self.media.path())
    }

    fn path(&self, index: usize) -> std::path::PathBuf {
        self.media
            .path()
            .join(file_name(index as i64 + 1, self.kinds[index]))
    }

    /// The calls of one port method so far.
    fn calls(&self, method: &str) -> Vec<Call> {
        self.backend
            .calls()
            .into_iter()
            .filter(|call| call.method() == method)
            .collect()
    }
}

fn deleted() -> DeleteAssetSummary {
    DeleteAssetSummary {
        removed_assets: 1,
        removed_thumbnails: 0,
        source_status: DeleteSourceStatus::Deleted,
        revision: 2,
        recovery_path: None,
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
    cx.update(|window, _| window.activate_window());
    settle(fixture, &mut cx);
    (window, cx)
}

/// Runs the backend calls and the background decodes, lets the answers reach the UI
/// thread and draws the next frames, in which the focus the lightbox asked for moves.
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

/// Draws the next frame without running a backend call: what was asked of the
/// backend stays pending.
fn frame(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
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

fn command(window: &WindowHandle<ShellView>, command: LightboxCommand, cx: &mut VisualTestContext) {
    window
        .update(cx, |shell, _, cx| {
            shell.runtime().clone().update(cx, |runtime, cx| {
                runtime.send(RuntimeInput::lightbox(command), cx)
            })
        })
        .expect("the window is open");
}

fn select(
    fixture: &Fixture,
    window: &WindowHandle<ShellView>,
    index: usize,
    cx: &mut VisualTestContext,
) {
    command(
        window,
        LightboxCommand::Select {
            asset: Box::new(fixture.summary(index)),
            index: Some(index),
        },
        cx,
    );
}

fn drawn(selector: &'static str, cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn click_selector(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
}

fn click(fixture: &Fixture, selector: &'static str, cx: &mut VisualTestContext) {
    click_selector(selector, cx);
    settle(fixture, cx);
}

fn tile_selector(index: usize) -> &'static str {
    Box::leak(gallery::tile_selector(index).into_boxed_str())
}

fn open_tile(fixture: &Fixture, index: usize, cx: &mut VisualTestContext) {
    click(fixture, tile_selector(index), cx);
}

fn focused(view: &Entity<LightboxView>, target: FocusTarget, cx: &mut VisualTestContext) -> bool {
    let handle = view.read_with(cx, |view, cx| view.focus_handle_of(target, cx));
    cx.update(|window, _| handle.is_focused(window))
}

fn focus(view: &Entity<LightboxView>, target: FocusTarget, cx: &mut VisualTestContext) {
    let handle = view.read_with(cx, |view, cx| view.focus_handle_of(target, cx));
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
}

fn draft(view: &Entity<LightboxView>, cx: &VisualTestContext) -> String {
    view.read_with(cx, |view, cx| view.editor().read(cx).draft(cx).to_owned())
}

fn shown_tags(view: &Entity<LightboxView>, cx: &VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        view.shown()
            .map(|shown| shown.tags.clone())
            .unwrap_or_default()
    })
}

fn confirm_open(view: &Entity<LightboxView>, cx: &VisualTestContext) -> bool {
    view.read_with(cx, |view, _| {
        view.session()
            .is_some_and(|session| session.confirm().is_open())
    })
}

fn controls_visible(view: &Entity<LightboxView>, cx: &VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| {
        let now = view.now(cx);
        view.session()
            .is_some_and(|session| session.controls_visible(now))
    })
}

fn zoom(view: &Entity<LightboxView>, cx: &VisualTestContext) -> f64 {
    view.read_with(cx, |view, _| {
        view.session()
            .map_or(0.0, |session| session.controls().zoom())
    })
}

/// Opens the delete confirmation and types `word` into its focused field.
fn open_confirm(
    fixture: &Fixture,
    view: &Entity<LightboxView>,
    word: &str,
    cx: &mut VisualTestContext,
) {
    click(fixture, lightbox::DELETE_ID, cx);
    assert!(
        focused(view, FocusTarget::ConfirmInput, cx),
        "the field has the focus"
    );
    cx.simulate_input(word);
    settle(fixture, cx);
}

fn idle(fixture: &Fixture, cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(3001));
    settle(fixture, cx);
}

// ----- LightboxModal.test.tsx -----

/// LightboxModal.test.tsx, "renders the reference hierarchy and keeps info collapsed
/// initially": the dialog with the stage, the picture and the side panel; the
/// Information panel is closed until its toggle opens it with the asset's path.
#[gpui::test]
fn renders_the_reference_hierarchy_and_keeps_info_collapsed_initially(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    for selector in [
        lightbox::BACKDROP_SELECTOR,
        lightbox::DIALOG_SELECTOR,
        lightbox::MEDIA_COLUMN_SELECTOR,
        lightbox::STAGE_SELECTOR,
        lightbox::IMAGE_SELECTOR,
        lightbox::RAIL_SELECTOR,
        lightbox::SIDEBAR_SELECTOR,
        lightbox::TAG_PANEL_SELECTOR,
        lightbox::GROUP_PANEL_SELECTOR,
        lightbox::INFO_TOGGLE_ID,
    ] {
        assert!(drawn(selector, &mut cx), "{selector}");
    }
    let dialog = cx.debug_bounds(lightbox::DIALOG_SELECTOR).unwrap();
    let stage = cx.debug_bounds(lightbox::STAGE_SELECTOR).unwrap();
    let sidebar = cx.debug_bounds(lightbox::SIDEBAR_SELECTOR).unwrap();
    assert!(dialog.contains(&stage.center()) && dialog.contains(&sidebar.center()));
    assert!(
        stage.right() <= sidebar.left(),
        "the panel is beside the stage"
    );
    assert!(!drawn(lightbox::INFO_PANEL_SELECTOR, &mut cx));
    assert!(!view.read_with(&cx, |view, _| view.session().unwrap().info_open()));

    click(&fixture, lightbox::INFO_TOGGLE_ID, &mut cx);
    assert!(drawn(lightbox::INFO_PANEL_SELECTOR, &mut cx));
    let path = fixture.path(0).to_string_lossy().into_owned();
    let rows = window
        .read_with(&cx, |shell, cx| {
            let runtime = shell.runtime().read(cx);
            let shown = shell.lightbox().read(cx).shown().cloned().unwrap();
            info_rows(&shown.selected, &runtime.i18n())
        })
        .unwrap();
    assert!(
        rows.iter().any(|row| row.path && row.value == path),
        "{rows:?}"
    );
}

/// LightboxModal.test.tsx, "navigates without restarting the opening animation or
/// remounting the shell": the dialog stays open and settled, the session goes on with
/// the next asset.
#[gpui::test]
fn navigates_without_restarting_the_opening_animation(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    rest(&fixture, &mut cx);
    let modal = view.read_with(&cx, |view, _| view.modal().clone());
    assert_eq!(modal.read_with(&cx, |modal, _| modal.phase()), Phase::Open);
    let dialog = cx.debug_bounds(lightbox::DIALOG_SELECTOR).unwrap();
    let generation = view.read_with(&cx, |view, _| view.session().unwrap().generation());

    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    assert_eq!(modal.read_with(&cx, |modal, _| modal.phase()), Phase::Open);
    assert!(!modal.read_with(&cx, |modal, cx| modal.is_fading(cx)));
    assert_eq!(modal.read_with(&cx, |modal, cx| modal.opacity(cx)), 1.0);
    assert_eq!(cx.debug_bounds(lightbox::DIALOG_SELECTOR), Some(dialog));
    assert_eq!(
        view.read_with(&cx, |view, _| view.session().unwrap().generation()),
        generation + 1,
        "the same opening goes on"
    );
}

/// LightboxModal.test.tsx, "keeps closing content but releases modality
/// immediately".
#[gpui::test]
fn keeps_closing_content_but_releases_modality_immediately(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    let input = view.read_with(&cx, |view, _| view.group_key_input().clone());
    let handle = input.read_with(&cx, gpui::Focusable::focus_handle);
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.simulate_input("closing-group");
    settle(&fixture, &mut cx);
    assert!(cx.update(|_, cx| layers::has_modal(cx)));

    command(&window, LightboxCommand::Close, &mut cx);
    frame(&mut cx);
    assert_eq!(selected_id(&window, &cx), None);
    assert!(
        !cx.update(|_, cx| layers::has_modal(cx)),
        "modality is released at once"
    );
    let modal = view.read_with(&cx, |view, _| view.modal().clone());
    assert_eq!(
        modal.read_with(&cx, |modal, _| modal.phase()),
        Phase::Closing
    );
    assert!(drawn(lightbox::DIALOG_SELECTOR, &mut cx));
    assert!(drawn(lightbox::IMAGE_SELECTOR, &mut cx));
    assert_eq!(
        view.read_with(&cx, |view, _| view
            .shown()
            .unwrap()
            .selected
            .summary
            .file_name
            .clone()),
        "asset-1.jpg"
    );
    assert_eq!(
        input.read_with(&cx, |input, _| input.text().to_owned()),
        "closing-group"
    );
    rest(&fixture, &mut cx);
    assert!(!drawn(lightbox::IMAGE_SELECTOR, &mut cx));
}

/// LightboxModal.test.tsx, "retains the collapsed panel and save failure throughout
/// exit".
#[gpui::test]
fn retains_the_collapsed_panel_and_save_failure_throughout_exit(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    fixture
        .backend
        .on_set_asset_tags(|_, _| Err(BackendError::rejected("save")));
    command(
        &window,
        LightboxCommand::SaveTags(vec!["draft".into()]),
        &mut cx,
    );
    settle(&fixture, &mut cx);
    assert!(drawn(lightbox::TAG_FAILED_SELECTOR, &mut cx));
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    rest(&fixture, &mut cx);
    assert!(!drawn(lightbox::SIDEBAR_SELECTOR, &mut cx));

    command(&window, LightboxCommand::Close, &mut cx);
    frame(&mut cx);
    assert!(drawn(lightbox::DIALOG_SELECTOR, &mut cx), "fading out");
    assert!(
        !drawn(lightbox::SIDEBAR_SELECTOR, &mut cx),
        "still collapsed"
    );
    let (failed, tags) = view.read_with(&cx, |view, _| {
        let shown = view.shown().unwrap();
        (shown.tag_failed, shown.tags.clone())
    });
    assert!(failed);
    assert!(tags.contains(&"draft".to_owned()), "{tags:?}");
}

/// LightboxModal.test.tsx, "survives close/reopen before the old exit deadline".
#[gpui::test]
fn survives_close_and_reopen_before_the_old_exit_deadline(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    command(&window, LightboxCommand::Close, &mut cx);
    frame(&mut cx);
    select(&fixture, &window, 1, &mut cx);
    settle(&fixture, &mut cx);
    rest(&fixture, &mut cx);
    let modal = view.read_with(&cx, |view, _| view.modal().clone());
    assert_eq!(modal.read_with(&cx, |modal, _| modal.phase()), Phase::Open);
    assert!(view.read_with(&cx, |view, _| view.is_open()));
    assert_eq!(selected_id(&window, &cx), Some(2));
    assert_eq!(
        view.read_with(&cx, |view, _| view.shown().unwrap().selected.id()),
        2
    );
    assert!(drawn(lightbox::IMAGE_SELECTOR, &mut cx));
}

/// LightboxModal.test.tsx, "closes immediately under reduced motion".
#[gpui::test]
fn closes_immediately_under_reduced_motion(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    open_tile(&fixture, 0, &mut cx);
    let modal = view.read_with(&cx, |view, _| view.modal().clone());
    assert_eq!(modal.read_with(&cx, |modal, _| modal.phase()), Phase::Open);
    cx.simulate_keystrokes("escape");
    frame(&mut cx);
    assert_eq!(selected_id(&window, &cx), None);
    assert!(!modal.read_with(&cx, |modal, _| modal.is_present()));
    assert!(!drawn(lightbox::DIALOG_SELECTOR, &mut cx));
}

/// LightboxModal.test.tsx, "keeps collapse and editor drafts across navigation and
/// toggling".
#[gpui::test]
fn keeps_collapse_and_editor_drafts_across_navigation_and_toggling(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    rest(&fixture, &mut cx);
    command(&window, LightboxCommand::Next, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    assert!(drawn(lightbox::TRIGGER_ID, &mut cx));
    assert!(
        !drawn(lightbox::SIDEBAR_SELECTOR, &mut cx),
        "still collapsed"
    );

    click(&fixture, lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    focus(&view, FocusTarget::TagInput, &mut cx);
    cx.simulate_input("draft");
    settle(&fixture, &mut cx);
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    rest(&fixture, &mut cx);
    click(&fixture, lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    assert_eq!(draft(&view, &cx), "draft");
}

/// LightboxModal.test.tsx, "uses a closed narrow drawer, consumes Escape and restores
/// keyboard focus".
#[gpui::test]
fn uses_a_closed_narrow_drawer_consumes_escape_and_restores_keyboard_focus(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    cx.simulate_resize(size(px(700.0), px(720.0)));
    settle(&fixture, &mut cx);
    open_tile(&fixture, 0, &mut cx);
    assert!(!drawn(lightbox::SIDEBAR_SELECTOR, &mut cx));
    click(&fixture, lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    assert!(focused(&view, FocusTarget::SidebarClose, &mut cx));
    cx.simulate_keystrokes("escape");
    rest(&fixture, &mut cx);
    assert!(focused(&view, FocusTarget::SidebarTrigger, &mut cx));
    assert!(view.read_with(&cx, |view, _| view.is_open()));
    assert!(drawn(lightbox::DIALOG_SELECTOR, &mut cx));
}

/// LightboxModal.test.tsx, "blocks navigation under a nested overlay": with another
/// modal layer above the lightbox, the arrows navigate nothing and Escape closes only
/// that layer (acceptance criterion: Escape closes only the top layer).
#[gpui::test]
fn blocks_navigation_under_a_nested_overlay(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    let dismissed = Rc::new(Cell::new(0));
    let nested = {
        let dismissed = dismissed.clone();
        cx.update(|window, cx| {
            let container = cx.focus_handle();
            layers::register(
                LayerOptions::modal(container, move |_, _| dismissed.set(dismissed.get() + 1)),
                window,
                cx,
            )
        })
    };
    frame(&mut cx);
    // The key goes through the lightbox's dialog, as a window listener sees it.
    focus(&view, FocusTarget::Shell, &mut cx);
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(dismissed.get(), 1, "the nested layer takes Escape");
    assert_eq!(selected_id(&window, &cx), Some(1));
    assert!(view.read_with(&cx, |view, _| view.is_open()));

    cx.update(|window, cx| layers::release(nested, None, window, cx));
    settle(&fixture, &mut cx);
    focus(&view, FocusTarget::Shell, &mut cx);
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(
        selected_id(&window, &cx),
        Some(2),
        "on top again, the arrows navigate"
    );
}

/// LightboxModal.test.tsx, "explains an invalid tag, keeps the draft and sends no
/// save".
#[gpui::test]
fn explains_an_invalid_tag_keeps_the_draft_and_sends_no_save(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    focus(&view, FocusTarget::TagInput, &mut cx);
    cx.simulate_input("two;tags");
    cx.simulate_keystrokes("enter");
    settle(&fixture, &mut cx);
    assert!(drawn(lightbox::TAG_INVALID_SELECTOR, &mut cx));
    assert_eq!(draft(&view, &cx), "two;tags");
    assert_eq!(shown_tags(&view, &cx), ["cat"]);
    assert!(fixture.calls("set_asset_tags").is_empty());
    cx.simulate_keystrokes("backspace");
    settle(&fixture, &mut cx);
    assert!(!drawn(lightbox::TAG_INVALID_SELECTOR, &mut cx));
}

/// LightboxModal.test.tsx, "keeps input arrows and suggestion Escape inside the
/// editor".
#[gpui::test]
fn keeps_input_arrows_and_suggestion_escape_inside_the_editor(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    click(&fixture, tag_editor::INPUT_SELECTOR, &mut cx);
    cx.simulate_input("d");
    settle(&fixture, &mut cx);
    let suggestions = view.read_with(&cx, |view, cx| {
        let editor = view.editor().read(cx);
        editor
            .combobox()
            .items()
            .iter()
            .map(|item| item.value.clone())
            .collect::<Vec<_>>()
    });
    assert!(suggestions.contains(&"dog".to_owned()), "{suggestions:?}");
    cx.simulate_keystrokes("right");
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));
    assert_eq!(draft(&view, &cx), "d");
    assert!(view.read_with(&cx, |view, _| view.is_open()));
}

/// LightboxModal.test.tsx, "adds suggestions, excludes selected tags and removes chips
/// through canonical saves".
#[gpui::test]
fn adds_suggestions_excludes_selected_tags_and_removes_chips(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    let suggestions = |cx: &VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let combobox = view.editor().read(cx).combobox();
            if !combobox.popup() {
                return Vec::new();
            }
            combobox
                .items()
                .iter()
                .map(|item| item.value.clone())
                .collect::<Vec<_>>()
        })
    };
    focus(&view, FocusTarget::TagInput, &mut cx);
    cx.simulate_input("a");
    settle(&fixture, &mut cx);
    let listed = suggestions(&cx);
    assert!(
        !listed.contains(&"cat".to_owned()),
        "an assigned tag: {listed:?}"
    );
    assert!(listed.contains(&"travel".to_owned()), "{listed:?}");
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("do");
    settle(&fixture, &mut cx);
    assert_eq!(suggestions(&cx), ["dog"]);
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("enter");
    settle(&fixture, &mut cx);
    assert_eq!(shown_tags(&view, &cx), ["cat", "dog"]);
    assert_eq!(draft(&view, &cx), "");

    click(
        &fixture,
        Box::leak(tag_editor::remove_selector(0).into_boxed_str()),
        &mut cx,
    );
    assert_eq!(shown_tags(&view, &cx), ["dog"]);
    let saves = fixture.calls("set_asset_tags");
    assert_eq!(
        saves.last(),
        Some(&Call::SetAssetTags {
            asset_id: 1,
            tags: vec!["dog".into()]
        })
    );
}

/// LightboxModal.test.tsx, "rejects CSV separators and whitespace without issuing
/// saves".
#[gpui::test]
fn rejects_csv_separators_and_whitespace_without_issuing_saves(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    let editor = view.read_with(&cx, |view, _| view.editor().clone());
    focus(&view, FocusTarget::TagInput, &mut cx);
    // The field's text is set as `fireEvent.input` sets it.
    for value in ["a b", "a,b", "a;b", "cat"] {
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.set_draft(value, window, cx)));
        settle(&fixture, &mut cx);
        assert_eq!(draft(&view, &cx), value);
        cx.simulate_keystrokes("enter");
        settle(&fixture, &mut cx);
    }
    // The field never holds a control character (it drops one on input), so the NUL
    // reaches the lightbox as the editor's submission.
    cx.update(|_, cx| {
        editor.update(cx, |_, cx| {
            cx.emit(TagEditorEvent::AddTag("a\u{0}b".into()))
        })
    });
    settle(&fixture, &mut cx);
    assert!(fixture.calls("set_asset_tags").is_empty());
    assert_eq!(shown_tags(&view, &cx), ["cat"]);
}

/// LightboxModal.test.tsx, "disables editing during detail loading and offers a
/// details retry".
#[gpui::test]
fn disables_editing_during_detail_loading_and_offers_a_details_retry(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    click_selector(tile_selector(0), &mut cx);
    frame(&mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));
    assert!(view.read_with(&cx, |view, cx| view.editor().read(cx).editing_disabled()));
    assert!(drawn(lightbox::DETAILS_LOADING_SELECTOR, &mut cx));
    assert!(drawn(lightbox::TAG_DETAILS_LOADING_SELECTOR, &mut cx));

    fixture.details_fail.store(true, Ordering::SeqCst);
    settle(&fixture, &mut cx);
    assert!(drawn(lightbox::DETAILS_ERROR_SELECTOR, &mut cx));
    assert!(view.read_with(&cx, |view, _| view.shown().unwrap().tag_details_failed));
    assert!(view.read_with(&cx, |view, cx| view.editor().read(cx).editing_disabled()));

    fixture.details_fail.store(false, Ordering::SeqCst);
    click(&fixture, lightbox::RETRY_DETAILS_ID, &mut cx);
    assert!(drawn(lightbox::IMAGE_SELECTOR, &mut cx));
    assert!(!view.read_with(&cx, |view, cx| view.editor().read(cx).editing_disabled()));
}

/// LightboxModal.test.tsx, "resets media failure on A-B-A and ignores retired image
/// errors": the A-B-A half. A picture that could not be displayed is read again when
/// its asset is selected again.
#[gpui::test]
fn resets_media_failure_on_a_b_a(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    std::fs::write(fixture.path(0), b"not a picture").unwrap();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    assert!(drawn(lightbox::MEDIA_ERROR_SELECTOR, &mut cx));
    assert!(!drawn(lightbox::IMAGE_SELECTOR, &mut cx));
    std::fs::write(fixture.path(0), JPEG).unwrap();
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    cx.simulate_keystrokes("left");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));
    assert_eq!(
        view.read_with(&cx, |view, cx| view.originals().read(cx).state()),
        LoadState::Ready
    );
    assert!(drawn(lightbox::IMAGE_SELECTOR, &mut cx));
    assert!(!drawn(lightbox::MEDIA_ERROR_SELECTOR, &mut cx));
}

fn source(asset_id: i64, kind: StageKind, path: &std::path::Path) -> MediaSource {
    MediaSource {
        asset_id,
        kind,
        path: path.to_string_lossy().into_owned(),
    }
}

/// LightboxModal.test.tsx, "resets media failure on A-B-A and ignores retired image
/// errors": the retired half. The failed decode of a source that is no longer shown
/// does not fail the current one.
#[gpui::test]
fn ignores_the_failure_of_a_retired_source(cx: &mut TestAppContext) {
    let media = tempfile::tempdir().unwrap();
    let broken = media.path().join("broken.jpg");
    let good = media.path().join("good.jpg");
    std::fs::write(&broken, b"not a picture").unwrap();
    std::fs::write(&good, JPEG).unwrap();
    let originals = cx.new(|_| Originals::new());
    originals.update(cx, |originals, cx| {
        originals.show(Some(source(1, StageKind::Image, &broken)), None, cx);
        originals.show(Some(source(2, StageKind::Image, &good)), None, cx);
    });
    cx.run_until_parked();
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.state(), LoadState::Ready);
        assert_eq!(originals.size(), Some((8, 8)));
    });
}

/// imageControls.test.tsx, "ignores a detached image load, same asset=%s": the
/// adapter half, both rows. A decode started for a source that was replaced (another
/// asset, or the same asset at another path) never reports its size.
#[gpui::test]
fn ignores_the_decode_of_a_detached_source(cx: &mut TestAppContext) {
    let media = tempfile::tempdir().unwrap();
    let previous = media.path().join("previous.gif");
    let current = media.path().join("current.jpg");
    std::fs::write(&previous, gif()).unwrap();
    std::fs::write(&current, JPEG).unwrap();
    for previous_id in [10, 11] {
        let originals = cx.new(|_| Originals::new());
        originals.update(cx, |originals, cx| {
            originals.show(
                Some(source(previous_id, StageKind::Gif, &previous)),
                None,
                cx,
            );
            originals.show(Some(source(10, StageKind::Image, &current)), None, cx);
        });
        cx.run_until_parked();
        originals.read_with(cx, |originals, _| {
            assert_eq!(originals.state(), LoadState::Ready);
            assert_eq!(originals.size(), Some((8, 8)), "previous id {previous_id}");
        });
    }
}

/// No TS counterpart: the webview loads its `<img>` itself. A held arrow key shows one
/// source after another while the first is still being loaded. A decode that has begun
/// cannot be stopped, so one load runs at a time: the sources passed in between are
/// never handed to the background executor, and the last one is shown.
#[gpui::test]
fn loads_one_picture_at_a_time_and_skips_the_sources_passed_by(cx: &mut TestAppContext) {
    let media = tempfile::tempdir().unwrap();
    let passed = media.path().join("passed.gif");
    let last = media.path().join("last.jpg");
    std::fs::write(&passed, gif()).unwrap();
    std::fs::write(&last, JPEG).unwrap();
    let originals = cx.new(|_| Originals::new());
    originals.update(cx, |originals, cx| {
        for id in 1..=5 {
            originals.show(Some(source(id, StageKind::Gif, &passed)), None, cx);
        }
        originals.show(Some(source(6, StageKind::Image, &last)), None, cx);
        assert_eq!(
            originals.loads_started(),
            1,
            "the others wait for the first"
        );
    });
    cx.run_until_parked();
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 2, "the first and the last");
        assert_eq!(originals.state(), LoadState::Ready);
        assert_eq!(originals.size(), Some((8, 8)));
    });

    // A source that is left for nothing (the lightbox closed) is not followed by
    // another load, and the next source is loaded as usual.
    originals.update(cx, |originals, cx| {
        originals.show(Some(source(7, StageKind::Gif, &passed)), None, cx);
        originals.show(None, None, cx);
    });
    cx.run_until_parked();
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 3);
        assert_eq!(originals.state(), LoadState::Loading);
        assert!(originals.picture().is_none());
    });
    originals.update(cx, |originals, cx| {
        originals.show(Some(source(7, StageKind::Gif, &passed)), None, cx);
    });
    cx.run_until_parked();
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 4);
        assert_eq!(originals.state(), LoadState::Ready);
        assert_eq!(originals.size(), Some((1, 1)));
    });
}

/// How long the read of a stalled file takes: far longer than any test waits.
const STALL: Duration = Duration::from_secs(3600);

/// A reader whose read of a file named `stalled` returns nothing after [`STALL`], as a
/// read on a mount that stopped answering does when the mount is back; every other
/// file is read at once. Nothing blocks: the stalled read is a timer on the test clock.
fn stalling_reader(executor: BackgroundExecutor, stalled: &'static str) -> media::Reader {
    Box::new(move |source, _| {
        if source.path.ends_with(stalled) {
            let answered = executor.timer(STALL);
            Box::pin(async move {
                answered.await;
                None
            })
        } else {
            Box::pin(async move { std::fs::read(&source.path).ok() })
        }
    })
}

/// No TS counterpart: the webview loads each `<img>` on its own. A read cannot be
/// stopped and, on a mount that stopped answering, does not return. The read of the
/// shown source is waited for; once its source is no longer shown it keeps the next
/// one waiting for [`READ_DEADLINE`] at most and is then given up. The reads given up
/// are bounded, and a source that finds no room is loaded when one of them returns.
#[gpui::test]
fn gives_up_a_read_that_does_not_return_once_another_picture_waits(cx: &mut TestAppContext) {
    cx.executor().set_num_cpus(4);
    assert_eq!(media::reads_allowed(4), 3);
    let media = tempfile::tempdir().unwrap();
    let stalled = media.path().join("stalled.jpg");
    let good = media.path().join("good.jpg");
    std::fs::write(&good, JPEG).unwrap();
    let originals = cx.new(|_| Originals::new());
    let reader = stalling_reader(cx.executor(), "stalled.jpg");
    originals.update(cx, |originals, _| originals.set_reader(reader));
    let wait = |cx: &mut TestAppContext| {
        cx.executor().advance_clock(READ_DEADLINE);
        cx.run_until_parked();
    };

    // The shown source's own read is waited for, however long it takes.
    originals.update(cx, |originals, cx| {
        originals.show(Some(source(1, StageKind::Image, &stalled)), None, cx);
    });
    for _ in 0..3 {
        wait(cx);
    }
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 1);
        assert_eq!(originals.abandoned_reads(), 0);
        assert_eq!(originals.state(), LoadState::Loading);
    });

    // Another source waits for it, but not for longer than the deadline.
    originals.update(cx, |originals, cx| {
        originals.show(Some(source(2, StageKind::Image, &good)), None, cx);
    });
    cx.run_until_parked();
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 1, "one load at a time");
        assert_eq!(originals.state(), LoadState::Loading);
    });
    wait(cx);
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 2);
        assert_eq!(originals.abandoned_reads(), 1);
        assert_eq!(originals.state(), LoadState::Ready);
        assert_eq!(originals.size(), Some((8, 8)));
    });

    // Closing and opening the lightbox again changes nothing about that.
    originals.update(cx, |originals, cx| {
        originals.show(None, None, cx);
        originals.show(Some(source(2, StageKind::Image, &good)), None, cx);
    });
    cx.run_until_parked();
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 3);
        assert_eq!(originals.state(), LoadState::Ready);
    });

    // Two more reads stall and are given up: three threads are held, so the next
    // source finds no room and waits.
    for id in [3, 4] {
        originals.update(cx, |originals, cx| {
            originals.show(Some(source(id, StageKind::Image, &stalled)), None, cx);
        });
        cx.run_until_parked();
    }
    wait(cx);
    originals.update(cx, |originals, cx| {
        originals.show(Some(source(5, StageKind::Image, &good)), None, cx);
    });
    wait(cx);
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 5, "the stalled 3 and 4");
        assert_eq!(originals.abandoned_reads(), 3);
        assert_eq!(originals.state(), LoadState::Loading);
    });
    wait(cx);
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.loads_started(), 5);
        assert_eq!(originals.state(), LoadState::Loading);
    });

    // The mount answers again: the reads return and the waiting picture is loaded.
    cx.executor().advance_clock(STALL);
    cx.run_until_parked();
    originals.read_with(cx, |originals, _| {
        assert_eq!(originals.abandoned_reads(), 0);
        assert_eq!(originals.loads_started(), 6);
        assert_eq!(originals.state(), LoadState::Ready);
        assert_eq!(originals.size(), Some((8, 8)));
    });
}

/// No TS counterpart. The case above in the window: the picture of the opened asset
/// is on a mount that does not answer, so the stage shows the spinner; the next
/// asset's picture is shown once the deadline of that read has passed.
#[gpui::test]
fn shows_the_next_picture_although_the_read_of_the_one_before_never_returns(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    let reader = stalling_reader(cx.executor(), "asset-1.jpg");
    let originals = view.read_with(&cx, |view, _| view.originals().clone());
    originals.update(&mut cx, |originals, _| originals.set_reader(reader));

    open_tile(&fixture, 0, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));
    assert!(drawn(lightbox::MEDIA_LOADING_SELECTOR, &mut cx));
    assert!(!drawn(lightbox::IMAGE_SELECTOR, &mut cx));
    assert!(!drawn(lightbox::MEDIA_ERROR_SELECTOR, &mut cx));

    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    assert!(
        !drawn(lightbox::IMAGE_SELECTOR, &mut cx),
        "it waits for the read that runs"
    );
    assert!(drawn(lightbox::MEDIA_LOADING_SELECTOR, &mut cx));

    cx.executor().advance_clock(READ_DEADLINE);
    settle(&fixture, &mut cx);
    assert_eq!(
        originals.read_with(&cx, |originals, _| (
            originals.state(),
            originals.abandoned_reads()
        )),
        (LoadState::Ready, 1)
    );
    assert!(drawn(lightbox::IMAGE_SELECTOR, &mut cx));
    assert!(!drawn(lightbox::MEDIA_LOADING_SELECTOR, &mut cx));
}

/// LightboxModal.test.tsx, "retains the GIF node through details and metadata
/// changes": the first frame is decoded once; details and a favorite write keep the
/// same picture. The stage shows it with the GIF chip (plan decision D8).
#[gpui::test]
fn retains_the_gif_picture_through_details_and_metadata_changes(cx: &mut TestAppContext) {
    let mut kinds = IMAGES;
    kinds[0] = "gif";
    let fixture = Fixture::with_kinds(kinds);
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    click_selector(tile_selector(0), &mut cx);
    frame(&mut cx);
    frame(&mut cx);
    let picture = |cx: &VisualTestContext| {
        view.read_with(cx, |view, cx| view.originals().read(cx).picture().cloned())
    };
    let first = picture(&cx).expect("the GIF's first frame is decoded from its preview path");
    assert_eq!(
        view.read_with(&cx, |view, cx| view.originals().read(cx).size()),
        Some((1, 1))
    );
    assert!(drawn(lightbox::GIF_CHIP_SELECTOR, &mut cx));
    assert!(drawn(lightbox::IMAGE_SELECTOR, &mut cx));

    settle(&fixture, &mut cx);
    assert!(view.read_with(&cx, |view, _| !view.shown().unwrap().tag_details_loading));
    assert!(
        Arc::ptr_eq(&first, &picture(&cx).unwrap()),
        "details keep the picture"
    );

    click(&fixture, lightbox::FAVORITE_ID, &mut cx);
    assert_eq!(
        fixture.calls("set_asset_favorite"),
        [Call::SetAssetFavorite {
            asset_id: 1,
            is_favorite: true
        }]
    );
    assert!(
        Arc::ptr_eq(&first, &picture(&cx).unwrap()),
        "a favorite keeps the picture"
    );
    assert!(drawn(lightbox::GIF_CHIP_SELECTOR, &mut cx));
}

/// No TS counterpart: the webview frees an image with its node. Here `Originals`
/// holds the decoded picture and the window's atlas its texture, and GPUI frees
/// neither by itself. Another asset drops the picture of the one before, a closed
/// lightbox keeps its picture while it fades out and none after that, and a dropped
/// picture's texture leaves the atlas with the next frame: nothing but the test
/// holds the pixels then.
#[gpui::test]
fn frees_the_picture_after_navigation_and_once_the_closed_lightbox_has_faded_out(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    let originals = view.read_with(&cx, |view, _| view.originals().clone());
    let held = |cx: &VisualTestContext| {
        originals.read_with(cx, |originals, _| {
            (originals.picture().cloned(), originals.retired_pictures())
        })
    };

    open_tile(&fixture, 0, &mut cx);
    let (first, retired) = held(&cx);
    let first = first.expect("the first picture is decoded");
    assert_eq!(retired, 0);
    assert!(drawn(lightbox::IMAGE_SELECTOR, &mut cx));

    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    let (second, retired) = held(&cx);
    let second = second.expect("the second picture is decoded");
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(retired, 0, "the first picture's texture left the atlas");
    assert_eq!(Arc::strong_count(&first), 1, "only the test holds it");

    command(&window, LightboxCommand::Close, &mut cx);
    frame(&mut cx);
    assert!(drawn(lightbox::IMAGE_SELECTOR, &mut cx), "fading out");
    assert!(held(&cx).0.is_some(), "kept while it fades out");
    rest(&fixture, &mut cx);
    assert!(!drawn(lightbox::DIALOG_SELECTOR, &mut cx));
    let (picture, retired) = held(&cx);
    assert!(picture.is_none(), "a closed lightbox holds no picture");
    assert_eq!(retired, 0, "and no texture waits in the atlas");
    assert_eq!(Arc::strong_count(&second), 1, "only the test holds it");
}

/// LightboxModal.test.tsx, "confirms deletion case-insensitively and prevents
/// duplicate submission": the view half. Confirm with the word typed in another case
/// and padded sends one deletion however often it is pressed; while it runs the close
/// button, Escape and the backdrop close nothing.
#[gpui::test]
fn confirms_deletion_once_and_locks_the_exits_while_it_runs(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    open_confirm(&fixture, &view, " yEs ", &mut cx);
    click_selector(lightbox::CONFIRM_SUBMIT_ID, &mut cx);
    frame(&mut cx);
    click_selector(lightbox::CONFIRM_SUBMIT_ID, &mut cx);
    frame(&mut cx);
    assert!(view.read_with(&cx, |view, _| view.session().unwrap().confirm().deleting()));
    click_selector(lightbox::CLOSE_ID, &mut cx);
    frame(&mut cx);
    cx.simulate_keystrokes("escape");
    frame(&mut cx);
    let backdrop = cx.debug_bounds(lightbox::BACKDROP_SELECTOR).unwrap();
    cx.simulate_click(backdrop.origin + point(px(2.0), px(2.0)), Modifiers::none());
    frame(&mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));
    assert!(drawn(lightbox::CONFIRM_SELECTOR, &mut cx));

    settle(&fixture, &mut cx);
    assert_eq!(
        fixture.calls("delete_asset"),
        [Call::DeleteAsset { asset_id: 1 }]
    );
    assert_eq!(selected_id(&window, &cx), None);
}

/// LightboxModal.test.tsx, "dismisses the confirmation first on Escape with focus
/// outside it".
#[gpui::test]
fn dismisses_the_confirmation_first_on_escape_with_focus_outside_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    open_confirm(&fixture, &view, "Yes", &mut cx);
    focus(&view, FocusTarget::TagInput, &mut cx);
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(!drawn(lightbox::CONFIRM_SELECTOR, &mut cx));
    assert!(view.read_with(&cx, |view, _| view.is_open()));
    assert_eq!(selected_id(&window, &cx), Some(1));

    click(&fixture, lightbox::DELETE_ID, &mut cx);
    focus(&view, FocusTarget::Shell, &mut cx);
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(!drawn(lightbox::CONFIRM_SELECTOR, &mut cx));
    assert_eq!(selected_id(&window, &cx), Some(1));
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), None);
}

/// LightboxModal.test.tsx, "keeps failed deletion in its confirmation and restores
/// focus after Escape".
#[gpui::test]
fn keeps_a_failed_deletion_in_its_confirmation_and_restores_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_delete_asset(|_| Err(BackendError::rejected("source retained")));
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    open_confirm(&fixture, &view, "Yes", &mut cx);
    click(&fixture, lightbox::CONFIRM_SUBMIT_ID, &mut cx);
    assert!(drawn(lightbox::CONFIRM_ERROR_SELECTOR, &mut cx));
    let error = window
        .read_with(&cx, |shell, cx| {
            let i18n = shell.runtime().read(cx).i18n();
            shell
                .lightbox()
                .read(cx)
                .session()
                .and_then(|session| session.confirm().error_text(&i18n))
        })
        .unwrap();
    assert!(error.is_some_and(|text| text.contains("source retained")));
    assert!(focused(&view, FocusTarget::ConfirmInput, &mut cx));
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(!drawn(lightbox::CONFIRM_SELECTOR, &mut cx));
    assert!(view.read_with(&cx, |view, _| view.is_open()));
    assert!(focused(&view, FocusTarget::DeleteButton, &mut cx));
}

/// LightboxModal.test.tsx, "shows a busy-library delete rejection localized and keeps
/// every exit usable": the view half.
#[gpui::test]
fn shows_a_busy_library_refusal_and_keeps_every_exit_usable(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_delete_asset(|_| Err(BackendError::rejected("i18n:common.libraryBusy")));
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    open_confirm(&fixture, &view, "Yes", &mut cx);
    click(&fixture, lightbox::CONFIRM_SUBMIT_ID, &mut cx);
    assert!(drawn(lightbox::CONFIRM_ERROR_SELECTOR, &mut cx));
    let error = window
        .read_with(&cx, |shell, cx| {
            let i18n = shell.runtime().read(cx).i18n();
            shell
                .lightbox()
                .read(cx)
                .session()
                .and_then(|session| session.confirm().error_text(&i18n))
        })
        .unwrap()
        .expect("the refusal is shown");
    assert!(
        error.contains("A scan or another library operation is running."),
        "{error}"
    );
    assert!(!error.contains("i18n:"));
    assert_eq!(selected_id(&window, &cx), Some(1));
    let input = view.read_with(&cx, |view, _| view.confirm_input().clone());
    assert!(!input.read_with(&cx, |input, _| input.is_disabled()));

    click(&fixture, lightbox::CONFIRM_CANCEL_ID, &mut cx);
    assert!(!drawn(lightbox::CONFIRM_SELECTOR, &mut cx));
    focus(&view, FocusTarget::Shell, &mut cx);
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    click(&fixture, lightbox::CLOSE_ID, &mut cx);
    assert_eq!(selected_id(&window, &cx), None);
}

/// LightboxModal.test.tsx, "keeps the confirmation, typed word and Information open
/// across tag and favorite writes": the view half. A tag saved from the tag field
/// under the open confirmation keeps the focus in the tag field; the confirmation and
/// its word stay through the tag and the favorite write, and Confirm still deletes.
#[gpui::test]
fn keeps_the_confirmation_and_the_tag_field_focus_across_writes(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    open_confirm(&fixture, &view, "Yes", &mut cx);
    focus(&view, FocusTarget::TagInput, &mut cx);
    cx.simulate_input("dog");
    cx.simulate_keystrokes("enter");
    settle(&fixture, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(shown_tags(&view, &cx), ["cat", "dog"]);
    assert!(
        focused(&view, FocusTarget::TagInput, &mut cx),
        "not pulled back to the field"
    );
    assert!(!focused(&view, FocusTarget::ConfirmInput, &mut cx));
    assert!(confirm_open(&view, &cx));
    let word = |cx: &VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.confirm_input().read(cx).text().to_owned()
        })
    };
    assert_eq!(word(&cx), "Yes");

    command(&window, LightboxCommand::ToggleFavorite, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(
        fixture.calls("set_asset_favorite"),
        [Call::SetAssetFavorite {
            asset_id: 1,
            is_favorite: true
        }]
    );
    assert!(confirm_open(&view, &cx));
    assert_eq!(word(&cx), "Yes");
    click(&fixture, lightbox::CONFIRM_SUBMIT_ID, &mut cx);
    assert_eq!(fixture.calls("delete_asset").len(), 1);
    assert_eq!(selected_id(&window, &cx), None);
}

/// LightboxModal.test.tsx, "still resets the confirmation and Information when the
/// selected ID changes": the view half.
#[gpui::test]
fn another_selected_asset_closes_the_confirmation_and_information(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    click(&fixture, lightbox::INFO_TOGGLE_ID, &mut cx);
    click(&fixture, lightbox::DELETE_ID, &mut cx);
    assert!(drawn(lightbox::CONFIRM_SELECTOR, &mut cx));
    select(&fixture, &window, 1, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    assert!(!drawn(lightbox::CONFIRM_SELECTOR, &mut cx));
    assert!(!drawn(lightbox::INFO_PANEL_SELECTOR, &mut cx));
    assert!(!view.read_with(&cx, |view, _| view.session().unwrap().info_open()));
}

/// LightboxModal.test.tsx, "copies trimmed group names and rejects late clipboard
/// results": the copy half (GPUI's clipboard write answers at once, so there is no
/// late result).
#[gpui::test]
fn copies_the_trimmed_group_name(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    let input = view.read_with(&cx, |view, _| view.group_key_input().clone());
    let handle = input.read_with(&cx, gpui::Focusable::focus_handle);
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.simulate_input("  group-one  ");
    settle(&fixture, &mut cx);
    click(&fixture, lightbox::COPY_GROUP_ID, &mut cx);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("group-one"));
}

/// LightboxModal.test.tsx, "ignores shortcuts during deletion confirmation and on
/// logical close".
#[gpui::test]
fn ignores_shortcuts_during_the_confirmation_and_after_close(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    click(&fixture, lightbox::DELETE_ID, &mut cx);
    focus(&view, FocusTarget::Shell, &mut cx);
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(1));
    command(&window, LightboxCommand::Close, &mut cx);
    frame(&mut cx);
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), None);
    assert!(!view.read_with(&cx, |view, _| view.is_open()));
}

// ----- keyboard.test.tsx and imageControls.test.tsx, the window -----

/// keyboard.test.tsx, "handles the first %s after idle and a synchronous activity
/// update", both rows in the window: the first arrow after the controls faded
/// navigates and shows them again.
#[gpui::test]
fn the_first_arrow_after_idle_navigates_and_reveals_the_controls(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    let mut expected = 1;
    for key in ["right", "right", "right", "left", "left", "left"] {
        idle(&fixture, &mut cx);
        assert!(!controls_visible(&view, &cx), "hidden after three seconds");
        cx.simulate_keystrokes(key);
        settle(&fixture, &mut cx);
        expected = if key == "right" {
            expected + 1
        } else {
            expected - 1
        };
        assert_eq!(selected_id(&window, &cx), Some(expected), "{key}");
        assert!(controls_visible(&view, &cx), "{key} reveals");
    }
}

/// keyboard.test.tsx, "stops shortcuts when disabled, closed or disposed and reopens
/// with one listener": the window half. A closed lightbox takes no arrow, and after
/// reopening one arrow moves the selection exactly once.
#[gpui::test]
fn a_reopened_lightbox_takes_each_key_once(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    open_tile(&fixture, 0, &mut cx);
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    cx.simulate_keystrokes("escape");
    rest(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), None);
    for _ in 0..2 {
        open_tile(&fixture, 0, &mut cx);
        assert_eq!(selected_id(&window, &cx), Some(1));
        cx.simulate_keystrokes("right");
        settle(&fixture, &mut cx);
        assert_eq!(selected_id(&window, &cx), Some(2), "one step per press");
        cx.simulate_keystrokes("escape");
        rest(&fixture, &mut cx);
    }
}

/// keyboard.test.tsx, "exits the image DOM fullscreen when navigation reaches a video
/// and routes F and Escape to the video": the window half. The window leaves
/// fullscreen when a video is selected; the video's placeholder (plan decision D8)
/// takes `f` without fullscreen and no zoom key; Escape closes. Acceptance: the video
/// placeholder with its poster, and no decode of the video file.
#[gpui::test]
fn the_window_leaves_fullscreen_when_navigation_reaches_a_video(cx: &mut TestAppContext) {
    let mut kinds = IMAGES;
    kinds[1] = "video";
    let fixture = Fixture::with_kinds(kinds);
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    cx.simulate_keystrokes("f");
    settle(&fixture, &mut cx);
    assert!(cx.update(|window, _| window.is_fullscreen()));

    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    assert!(!view.read_with(&cx, |view, _| view.session().unwrap().is_fullscreen()));
    assert!(drawn(lightbox::VIDEO_PLACEHOLDER_SELECTOR, &mut cx));
    assert!(drawn(lightbox::VIDEO_POSTER_SELECTOR, &mut cx));
    assert!(!drawn(lightbox::IMAGE_SELECTOR, &mut cx));
    assert!(
        view.read_with(&cx, |view, cx| view
            .originals()
            .read(cx)
            .picture()
            .is_none()),
        "a video is not decoded"
    );

    cx.simulate_keystrokes("f");
    settle(&fixture, &mut cx);
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    cx.simulate_keystrokes("+");
    settle(&fixture, &mut cx);
    assert_eq!(zoom(&view, &cx), 1.0);
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), None);
}

/// Acceptance (epic E16): the zoom keys stop at 1 and 12, and a drag of the zoomed
/// picture stops at the edge of what overflows the stage (imageControls.test.tsx,
/// "preserves the wheel anchor and clamps drag and resize pan to the zoomed bounds",
/// with the window's pointer).
#[gpui::test]
fn the_zoom_stays_between_one_and_twelve_and_a_drag_stops_at_the_bounds(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    for _ in 0..20 {
        cx.simulate_keystrokes("+");
    }
    assert_eq!(zoom(&view, &cx), 12.0);
    for _ in 0..20 {
        cx.simulate_keystrokes("-");
    }
    assert_eq!(zoom(&view, &cx), 1.0);
    for _ in 0..4 {
        cx.simulate_keystrokes("=");
    }
    settle(&fixture, &mut cx);
    let stage = cx.debug_bounds(lightbox::STAGE_SELECTOR).unwrap();
    let start = stage.center();
    let far = start + point(px(5000.0), px(0.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(far, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(far, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    let (pan, bound, dragging) = view.read_with(&cx, |view, _| {
        let controls = view.session().unwrap().controls();
        let fitted = controls.fitted();
        let viewport = controls.viewport().unwrap();
        (
            controls.pan(),
            (fitted.width * controls.zoom() - viewport.width).max(0.0) / 2.0,
            controls.is_dragging(),
        )
    });
    assert!(bound > 0.0);
    assert!((pan.x - bound).abs() < 1e-6, "{pan:?} against {bound}");
    assert!(!dragging);
    assert!(
        view.read_with(&cx, |view, _| view.is_open()),
        "a drag does not close"
    );
}

// ----- presentation.test.ts, the view -----

/// presentation.test.ts, "reveals without consuming %s and releases timers on close":
/// the `defaultPrevented` half. A key typed into the tag field, a wheel turn over the
/// stage and a press on a button show the faded controls and still do what they do.
#[gpui::test]
fn input_reveals_the_controls_without_being_consumed(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);

    // keydown
    focus(&view, FocusTarget::TagInput, &mut cx);
    idle(&fixture, &mut cx);
    assert!(!controls_visible(&view, &cx));
    cx.simulate_keystrokes("x");
    settle(&fixture, &mut cx);
    assert!(controls_visible(&view, &cx));
    assert_eq!(draft(&view, &cx), "x", "the field got the key");

    // wheel
    idle(&fixture, &mut cx);
    assert!(!controls_visible(&view, &cx));
    let stage = cx.debug_bounds(lightbox::STAGE_SELECTOR).unwrap();
    cx.simulate_event(ScrollEvent {
        position: stage.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(50.0))),
        ..ScrollEvent::default()
    });
    settle(&fixture, &mut cx);
    assert!(controls_visible(&view, &cx));
    assert!(zoom(&view, &cx) > 1.0, "the stage got the wheel");

    // pointerdown
    idle(&fixture, &mut cx);
    assert!(!controls_visible(&view, &cx));
    click(&fixture, lightbox::INFO_TOGGLE_ID, &mut cx);
    assert!(controls_visible(&view, &cx));
    assert!(view.read_with(&cx, |view, _| view.session().unwrap().info_open()));

    // Closing ends the countdown: no timer is left armed.
    command(&window, LightboxCommand::Close, &mut cx);
    rest(&fixture, &mut cx);
    assert!(view.read_with(&cx, |view, _| view.session().is_none()));
}

/// No TS counterpart: `LightboxModal.tsx` takes the faded floating buttons out of the
/// pointer's reach with `pointer-events: none`. A press that finds the reopen or the
/// close button faded shows the controls and clicks neither, the next click is theirs,
/// and Enter activates the focused button although it is faded.
#[gpui::test]
fn a_press_on_a_faded_floating_button_shows_it_without_clicking_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    rest(&fixture, &mut cx);
    assert!(!drawn(lightbox::SIDEBAR_SELECTOR, &mut cx));

    // The reopen button.
    idle(&fixture, &mut cx);
    assert!(!controls_visible(&view, &cx));
    click(&fixture, lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    assert!(controls_visible(&view, &cx), "the press shows the controls");
    assert!(
        !drawn(lightbox::SIDEBAR_SELECTOR, &mut cx),
        "and opens nothing"
    );
    click(&fixture, lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    assert!(
        drawn(lightbox::SIDEBAR_SELECTOR, &mut cx),
        "the shown button takes the next click"
    );

    // The keyboard reaches the faded button.
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    rest(&fixture, &mut cx);
    focus(&view, FocusTarget::SidebarTrigger, &mut cx);
    idle(&fixture, &mut cx);
    assert!(!controls_visible(&view, &cx));
    // GPUI's keyboard click fires on the key's release.
    cx.simulate_keystrokes("enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").expect("a key"),
    });
    rest(&fixture, &mut cx);
    assert!(drawn(lightbox::SIDEBAR_SELECTOR, &mut cx));

    // The close button.
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    rest(&fixture, &mut cx);
    idle(&fixture, &mut cx);
    assert!(!controls_visible(&view, &cx));
    click(&fixture, lightbox::FLOATING_CLOSE_ID, &mut cx);
    assert!(controls_visible(&view, &cx));
    assert_eq!(selected_id(&window, &cx), Some(1), "nothing closes");
    click(&fixture, lightbox::FLOATING_CLOSE_ID, &mut cx);
    assert_eq!(selected_id(&window, &cx), None);
}

/// Zooms the picture in until it lies under the centre of `selector`.
fn zoom_under(
    fixture: &Fixture,
    view: &Entity<LightboxView>,
    selector: &'static str,
    cx: &mut VisualTestContext,
) -> f64 {
    for _ in 0..8 {
        cx.simulate_keystrokes("+");
    }
    settle(fixture, cx);
    let control = cx.debug_bounds(selector).expect("the control is drawn");
    let image = cx
        .debug_bounds(lightbox::IMAGE_SELECTOR)
        .expect("the picture is drawn");
    assert!(
        image.contains(&control.center()),
        "the picture lies under {selector}"
    );
    zoom(view, cx)
}

/// No TS counterpart: in `LightboxModal.tsx` the floating buttons are siblings of
/// the stage, so a click on one never reaches the picture under it. A click on the
/// reopen button over a zoomed picture opens the panel and leaves the zoom alone.
#[gpui::test]
fn a_click_on_a_floating_button_over_the_picture_does_not_zoom_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    rest(&fixture, &mut cx);
    let zoomed = zoom_under(&fixture, &view, lightbox::TRIGGER_ID, &mut cx);
    assert!(controls_visible(&view, &cx));

    click(&fixture, lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    assert!(
        drawn(lightbox::SIDEBAR_SELECTOR, &mut cx),
        "the button got the click"
    );
    assert_eq!(zoom(&view, &cx), zoomed, "the picture did not");
}

/// No TS counterpart: the details notice of `LightboxMediaStage.tsx` is a sibling of
/// the picture, so a click on its Retry never reaches the picture under it.
#[gpui::test]
fn a_click_on_the_details_retry_over_the_picture_does_not_zoom_it(cx: &mut TestAppContext) {
    let mut kinds = IMAGES;
    kinds[0] = "gif";
    let fixture = Fixture::with_kinds(kinds);
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    // A GIF is shown from its row while its details fail.
    fixture.details_fail.store(true, Ordering::SeqCst);
    open_tile(&fixture, 0, &mut cx);
    assert!(drawn(lightbox::DETAILS_NOTICE_SELECTOR, &mut cx));
    let zoomed = zoom_under(&fixture, &view, lightbox::RETRY_DETAILS_ID, &mut cx);

    fixture.details_fail.store(false, Ordering::SeqCst);
    click(&fixture, lightbox::RETRY_DETAILS_ID, &mut cx);
    assert!(
        !drawn(lightbox::DETAILS_NOTICE_SELECTOR, &mut cx),
        "Retry got the click"
    );
    assert_eq!(zoom(&view, &cx), zoomed, "the picture did not");
}

/// A wheel turn at the centre of `selector`.
fn wheel_over(fixture: &Fixture, selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_event(ScrollEvent {
        position: bounds.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(50.0))),
        ..ScrollEvent::default()
    });
    settle(fixture, cx);
}

/// No TS counterpart: in `LightboxModal.tsx` the floating buttons are siblings of
/// the stage, whose wheel listener (`createImageControls.ts`) never hears a wheel
/// turn over them, while the window's capturing listener (`createLightboxActivity`)
/// counts it as activity. A wheel turn over a shown button leaves the zoom alone and
/// starts the countdown again; over a faded one (`pointer-events: none`) it is the
/// stage's.
#[gpui::test]
fn a_wheel_turn_over_a_floating_button_shows_the_controls_and_does_not_zoom(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    rest(&fixture, &mut cx);
    let zoomed = zoom_under(&fixture, &view, lightbox::TRIGGER_ID, &mut cx);
    assert!(controls_visible(&view, &cx));

    cx.executor().advance_clock(Duration::from_millis(2000));
    settle(&fixture, &mut cx);
    wheel_over(&fixture, lightbox::TRIGGER_ID, &mut cx);
    assert_eq!(
        zoom(&view, &cx),
        zoomed,
        "the picture did not get the wheel"
    );
    cx.executor().advance_clock(Duration::from_millis(2000));
    settle(&fixture, &mut cx);
    assert!(
        controls_visible(&view, &cx),
        "the wheel turn started the countdown again"
    );
    wheel_over(&fixture, lightbox::FLOATING_CLOSE_ID, &mut cx);
    assert_eq!(zoom(&view, &cx), zoomed);
    assert_eq!(selected_id(&window, &cx), Some(1));

    // A faded button is out of the pointer's reach: the stage under it gets the
    // wheel, which shows the controls.
    idle(&fixture, &mut cx);
    assert!(!controls_visible(&view, &cx));
    wheel_over(&fixture, lightbox::TRIGGER_ID, &mut cx);
    assert!(controls_visible(&view, &cx));
    assert!(zoom(&view, &cx) > zoomed, "the stage got the wheel");
}

/// No TS counterpart: the save-failure alerts of `LightboxModal.tsx` are siblings of
/// the stage as well. A wheel turn over one shows the faded controls and does not
/// zoom the picture under it.
#[gpui::test]
fn a_wheel_turn_over_a_save_failure_alert_shows_the_controls_and_does_not_zoom(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    fixture
        .backend
        .on_set_asset_favorite(|_, _| Err(BackendError::rejected("save")));
    command(&window, LightboxCommand::ToggleFavorite, &mut cx);
    settle(&fixture, &mut cx);
    assert!(drawn(lightbox::FAVORITE_FAILED_SELECTOR, &mut cx));
    let zoomed = zoom_under(&fixture, &view, lightbox::FAVORITE_FAILED_SELECTOR, &mut cx);

    idle(&fixture, &mut cx);
    assert!(!controls_visible(&view, &cx));
    wheel_over(&fixture, lightbox::FAVORITE_FAILED_SELECTOR, &mut cx);
    assert!(controls_visible(&view, &cx), "a wheel turn is activity");
    assert_eq!(
        zoom(&view, &cx),
        zoomed,
        "the picture did not get the wheel"
    );
}

/// No TS counterpart: the details notice of `LightboxMediaStage.tsx` is a child of
/// the stage, whose wheel listener hears a wheel turn over it: the picture zooms, as
/// in the Solid lightbox.
#[gpui::test]
fn a_wheel_turn_over_the_details_notice_zooms_the_picture(cx: &mut TestAppContext) {
    let mut kinds = IMAGES;
    kinds[0] = "gif";
    let fixture = Fixture::with_kinds(kinds);
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    fixture.details_fail.store(true, Ordering::SeqCst);
    open_tile(&fixture, 0, &mut cx);
    let zoomed = zoom_under(&fixture, &view, lightbox::DETAILS_NOTICE_SELECTOR, &mut cx);

    wheel_over(&fixture, lightbox::DETAILS_NOTICE_SELECTOR, &mut cx);
    assert!(zoom(&view, &cx) > zoomed, "the stage got the wheel");
}

// ----- MediaGroupSetter.test.tsx, the view -----

/// MediaGroupSetter.test.tsx, "is a numeric-hinted textbox without native increment
/// controls": the arrow keys do not step the order.
#[gpui::test]
fn the_order_field_has_no_increment_keys(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    let input = view.read_with(&cx, |view, _| view.group_order_input().clone());
    let handle = input.read_with(&cx, gpui::Focusable::focus_handle);
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.simulate_input("2");
    cx.simulate_keystrokes("up");
    cx.simulate_keystrokes("down");
    settle(&fixture, &mut cx);
    assert_eq!(
        input.read_with(&cx, |input, _| input.text().to_owned()),
        "2"
    );
    assert_eq!(
        selected_id(&window, &cx),
        Some(1),
        "the arrows stay in the field"
    );
    assert_eq!(
        view.read_with(&cx, |view, _| view.shown().unwrap().group_order.clone()),
        "2"
    );
}

/// MediaGroupSetter.test.tsx, "keeps Apply disabled while pending": a second Apply
/// while the first write runs sends nothing.
#[gpui::test]
fn apply_sends_nothing_while_the_group_write_is_pending(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    for (input, text) in [
        (
            view.read_with(&cx, |view, _| view.group_key_input().clone()),
            "group",
        ),
        (
            view.read_with(&cx, |view, _| view.group_order_input().clone()),
            "2",
        ),
    ] {
        let handle = input.read_with(&cx, gpui::Focusable::focus_handle);
        cx.update(|window, cx| window.focus(&handle, cx));
        cx.simulate_input(text);
        settle(&fixture, &mut cx);
    }
    click_selector(lightbox::APPLY_GROUP_ID, &mut cx);
    frame(&mut cx);
    assert!(view.read_with(&cx, |view, _| view.shown().unwrap().group_pending));
    click_selector(lightbox::APPLY_GROUP_ID, &mut cx);
    frame(&mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(
        fixture.calls("set_asset_media_group"),
        [Call::SetAssetMediaGroup {
            asset_id: 1,
            media_group_key: Some("group".into()),
            media_group_order: Some(2.0)
        }]
    );
    assert!(!view.read_with(&cx, |view, _| view.shown().unwrap().group_pending));
}

/// No TS counterpart (the browser keeps the arrows of a focused `input` to the
/// field, and `keyboardEnabled` of `createImageControls.ts` skips form controls):
/// with the focus in the group name or the order field, Arrow left and right move
/// the caret and select no other asset. The lightbox binds both keys in its own
/// key context; the field binds them deeper.
#[gpui::test]
fn the_arrows_in_the_group_fields_move_the_caret_and_select_no_other_asset(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    for (input, typed, inserted, moved) in [
        (
            view.read_with(&cx, |view, _| view.group_key_input().clone()),
            "trip",
            "trxip",
            "trxiyp",
        ),
        (
            view.read_with(&cx, |view, _| view.group_order_input().clone()),
            "1234",
            "12534",
            "125364",
        ),
    ] {
        let handle = input.read_with(&cx, gpui::Focusable::focus_handle);
        cx.update(|window, cx| window.focus(&handle, cx));
        cx.simulate_input(typed);
        settle(&fixture, &mut cx);

        cx.simulate_keystrokes("left");
        cx.simulate_keystrokes("left");
        settle(&fixture, &mut cx);
        assert_eq!(selected_id(&window, &cx), Some(1), "{typed}: Arrow left");
        // The caret is two characters back: what is typed now lands there.
        cx.simulate_input(&inserted[2..inserted.len() - 2]);
        settle(&fixture, &mut cx);
        assert_eq!(
            input.read_with(&cx, |input, _| input.text().to_owned()),
            inserted
        );

        cx.simulate_keystrokes("right");
        settle(&fixture, &mut cx);
        assert_eq!(selected_id(&window, &cx), Some(1), "{typed}: Arrow right");
        cx.simulate_input(&moved[inserted.len() - 1..moved.len() - 1]);
        settle(&fixture, &mut cx);
        assert_eq!(
            input.read_with(&cx, |input, _| input.text().to_owned()),
            moved
        );
        assert!(cx.update(|window, _| handle.is_focused(window)));
    }
    let (key, order) = view.read_with(&cx, |view, _| {
        let shown = view.shown().unwrap();
        (shown.group_key.clone(), shown.group_order.clone())
    });
    assert_eq!((key.as_str(), order.as_str()), ("trxiyp", "125364"));
}

// ----- focus, the view -----

/// No TS counterpart: the closed side panel of `LightboxToolbar.tsx` is `inert`, so
/// Tab never stops in it while it slides out. Here its controls are drawn until the
/// slide has ended: a focus that Tab brings into the sliding drawer goes on to the
/// reopen button, and once the drawer is gone the viewer's keys still reach the
/// dialog.
#[gpui::test]
fn a_drawer_that_slides_out_takes_no_focus_and_the_keys_stay_with_the_dialog(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    let in_sidebar = |cx: &mut VisualTestContext| {
        let view = view.clone();
        cx.update(|window, cx| view.read(cx).sidebar_has_focus(window, cx))
    };
    cx.simulate_resize(size(px(700.0), px(720.0)));
    settle(&fixture, &mut cx);
    open_tile(&fixture, 0, &mut cx);
    click(&fixture, lightbox::TRIGGER_ID, &mut cx);
    rest(&fixture, &mut cx);
    assert!(focused(&view, FocusTarget::SidebarClose, &mut cx));
    assert!(in_sidebar(&mut cx));

    // Collapsed with the pointer: the dialog has the focus and the drawer slides.
    click(&fixture, lightbox::SIDEBAR_CLOSE_ID, &mut cx);
    assert!(
        drawn(lightbox::SIDEBAR_SELECTOR, &mut cx),
        "still sliding out"
    );
    assert!(focused(&view, FocusTarget::Shell, &mut cx));

    // The last tab stop of the dialog is inside the drawer.
    cx.simulate_keystrokes("shift-tab");
    settle(&fixture, &mut cx);
    assert!(drawn(lightbox::SIDEBAR_SELECTOR, &mut cx));
    assert!(!in_sidebar(&mut cx), "the sliding drawer takes no focus");
    assert!(focused(&view, FocusTarget::SidebarTrigger, &mut cx));

    rest(&fixture, &mut cx);
    assert!(!drawn(lightbox::SIDEBAR_SELECTOR, &mut cx));
    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(
        selected_id(&window, &cx),
        Some(2),
        "the arrows reach the dialog"
    );
}

/// No TS counterpart: the closing shell of `LightboxModal.tsx` is `inert`. The
/// dialog is drawn until it has faded out, and Tab during that time must not stop on
/// one of its controls.
#[gpui::test]
fn a_lightbox_that_fades_out_takes_no_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    let dialog = view.read_with(&cx, |view, cx| view.focus_handle_of(FocusTarget::Shell, cx));
    assert!(cx.update(|window, cx| dialog.contains_focused(window, cx)));

    command(&window, LightboxCommand::Close, &mut cx);
    frame(&mut cx);
    assert!(drawn(lightbox::DIALOG_SELECTOR, &mut cx), "fading out");
    assert!(!cx.update(|window, cx| dialog.contains_focused(window, cx)));
    cx.simulate_keystrokes("tab");
    frame(&mut cx);
    assert!(
        drawn(lightbox::DIALOG_SELECTOR, &mut cx),
        "still fading out"
    );
    assert!(
        !cx.update(|window, cx| dialog.contains_focused(window, cx)),
        "the closing dialog takes no focus"
    );
    assert_eq!(selected_id(&window, &cx), None);
}

/// No TS counterpart: a window that turns narrow closes the panel the user has not
/// chosen (`createLightboxSidebar`). The focus does not stay in the field of the
/// drawer that slides out: it goes to the reopen button.
#[gpui::test]
fn a_window_that_turns_narrow_takes_the_focus_out_of_the_closing_drawer(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);
    focus(&view, FocusTarget::TagInput, &mut cx);
    assert!(cx.update(|window, cx| view.read(cx).sidebar_has_focus(window, cx)));

    cx.simulate_resize(size(px(700.0), px(720.0)));
    settle(&fixture, &mut cx);
    assert!(
        drawn(lightbox::SIDEBAR_SELECTOR, &mut cx),
        "still sliding out"
    );
    assert!(!cx.update(|window, cx| view.read(cx).sidebar_has_focus(window, cx)));
    assert!(focused(&view, FocusTarget::SidebarTrigger, &mut cx));
    rest(&fixture, &mut cx);
    assert!(!drawn(lightbox::SIDEBAR_SELECTOR, &mut cx));
    assert!(focused(&view, FocusTarget::SidebarTrigger, &mut cx));
    assert_eq!(selected_id(&window, &cx), Some(1));
}

/// No TS counterpart: the keys of the Solid lightbox are the window's, whatever is
/// focused. A focused control that is disabled leaves GPUI's frame (the heart while
/// its write runs), and the focus must not fall out of the dialog's key context
/// with it: the dialog takes it, and the arrows still navigate.
#[gpui::test]
fn the_dialog_keeps_its_keys_when_its_focused_control_is_disabled(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open(&fixture, cx);
    let view = lightbox_view(&window, &cx);
    open_tile(&fixture, 0, &mut cx);

    // The click focuses the heart; it is disabled while the favorite is written.
    click_selector(lightbox::FAVORITE_ID, &mut cx);
    frame(&mut cx);
    assert!(view.read_with(&cx, |view, _| view.shown().unwrap().favorite_pending));
    settle(&fixture, &mut cx);
    assert!(!view.read_with(&cx, |view, _| view.shown().unwrap().favorite_pending));
    assert_eq!(fixture.calls("set_asset_favorite").len(), 1);
    assert!(focused(&view, FocusTarget::Shell, &mut cx));

    cx.simulate_keystrokes("right");
    settle(&fixture, &mut cx);
    assert_eq!(selected_id(&window, &cx), Some(2));
}
