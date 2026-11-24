//! The settings page drawn by the headless renderer in both themes: the scan section
//! with one root, the appearance and language sections, and the end of the page with
//! the danger zone and About (epic E17); the import/export section, the duplicate
//! section after a scan that found a group, and the duplicate resolver with that group
//! of two files, at rest and with a rename field focused (epic E18). Skipped without a
//! GPU or software driver, unless `MEDIATAGGER_GPUI_REQUIRE_RENDERER` is set. With
//! `MEDIATAGGER_GPUI_SCREENSHOT_DIR` set it leaves one PNG per state there for review.

use std::{path::PathBuf, sync::Arc, time::Duration};

use gpui::{AnyWindowHandle, Focusable as _, HeadlessAppContext, WindowHandle};
use media_tagger_client::{
    backend::{FakeBackend, ThumbnailImage, ThumbnailReply},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    settings::{layout, SettingsCommand, SettingsSection},
    theme::{Rgba, ThemeName},
};
use media_tagger_core::models::{
    DuplicateAsset, DuplicateGroup, DuplicateScanSummary, ScanRoot, StartAssetQueryResult,
    VideoToolStatus,
};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui::assets::Assets,
    views::{settings::SettingsView, shell::ShellView},
    window::{init_application, open_main_window, WINDOW_SIZE},
};

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

/// One group of two files with the same name, as the `duplicates` seed of the comparison
/// harness has it.
fn one_group() -> DuplicateScanSummary {
    let asset = |id: i64, path: &str| DuplicateAsset {
        id,
        path: path.to_owned(),
        record_version: 1,
        size_bytes: 10,
        thumb_version: format!("{id:016x}"),
    };
    DuplicateScanSummary {
        groups: vec![DuplicateGroup {
            file_name: "control-1.png".into(),
            assets: vec![
                asset(1, "/tmp/synthetic-media/control-1.png"),
                asset(2, "/tmp/synthetic-media/copy/control-1.png"),
            ],
        }],
        duplicate_groups: 1,
        duplicate_assets: 2,
        revision: 1,
    }
}

fn parts(theme: ThemeName, executor: Arc<ManualExecutor>) -> RuntimeParts {
    let backend = Arc::new(FakeBackend::new());
    backend
        .on_start_asset_query(|_| {
            Ok(StartAssetQueryResult::Ready {
                session_id: 1,
                revision: 1,
                total: 0,
                offset: 0,
                items: Vec::new(),
            })
        })
        .on_scan_startup_roots(|| Ok(None))
        .on_list_scan_roots(|| {
            Ok(vec![ScanRoot {
                path: "/tmp/synthetic-media".into(),
                auto_scan_on_startup: false,
            }])
        })
        .on_get_video_tool_status(|| {
            Ok(VideoToolStatus {
                ffmpeg_available: true,
                ffprobe_available: true,
            })
        })
        .on_find_duplicate_assets(|| Ok(one_group()))
        .on_request_thumbnail(|request, responder| {
            responder.respond(ThumbnailReply::Ready(ThumbnailImage {
                bytes: JPEG.to_vec(),
                version: request.version,
            }));
        });
    RuntimeParts {
        config: RuntimeConfig {
            theme,
            language: Language::En,
        },
        backend,
        prefs: Arc::new(MemoryPrefsStore::default()),
        executor: executor as Arc<dyn Executor>,
    }
}

fn renderer_available() -> bool {
    match gpui_platform::current_headless_renderer() {
        Ok(_) => true,
        Err(error) => {
            if std::env::var_os("MEDIATAGGER_GPUI_REQUIRE_RENDERER").is_some() {
                panic!("no headless renderer: {error:#}");
            }
            eprintln!("skipped: no GPU adapter for the headless renderer ({error:#})");
            false
        }
    }
}

struct Headless {
    cx: HeadlessAppContext,
    executor: Arc<ManualExecutor>,
    window: WindowHandle<ShellView>,
}

impl Headless {
    fn open(theme: ThemeName) -> Self {
        let text_system = gpui_platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), || {
            gpui_platform::current_headless_renderer()
        });
        let executor = Arc::new(ManualExecutor::new());
        let window = cx
            .update(|cx| {
                init_application(theme, cx)?;
                open_main_window(profile::E2E, parts(theme, executor.clone()), cx)
            })
            .expect("open the main window");
        let mut this = Self {
            cx,
            executor,
            window,
        };
        this.settle();
        this
    }

    fn handle(&self) -> AnyWindowHandle {
        self.window.into()
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            loop {
                self.cx.run_until_parked();
                if self.executor.run_all() == 0 {
                    break;
                }
            }
            self.cx
                .update_window(self.handle(), |_, window, cx| {
                    window.simulate_next_frame(cx);
                    window.refresh();
                })
                .expect("the window is open");
        }
        self.cx.run_until_parked();
    }

    fn send(&mut self, input: RuntimeInput) {
        let window = self.window;
        self.cx.update(|cx| {
            let runtime = window.read(cx).expect("open").runtime().clone();
            runtime.update(cx, |runtime, cx| runtime.send(input, cx));
        });
        self.settle();
    }

    fn navigate(&mut self, section: SettingsSection) {
        let window = self.window;
        self.cx.update(|cx| {
            let view = window
                .read(cx)
                .expect("open")
                .settings()
                .cloned()
                .expect("the settings page is shown");
            view.update(cx, |view, cx| view.navigate(section, cx));
        });
        self.settle();
    }

    fn active(&mut self) -> SettingsSection {
        let window = self.window;
        self.cx.update(|cx| {
            let view = window
                .read(cx)
                .expect("open")
                .settings()
                .cloned()
                .expect("the settings page is shown");
            view.read(cx).active_section()
        })
    }

    /// The pixel at the CSS pixel (x, y), and saves the frame when asked to.
    fn pixel(&mut self, name: &str, x: f32, y: f32) -> [u8; 3] {
        self.frame(name).at(x, y)
    }

    /// The window as it is drawn now, saved as `name` when asked to.
    fn frame(&mut self, name: &str) -> Frame {
        self.capture(Some(name))
    }

    fn capture(&mut self, name: Option<&str>) -> Frame {
        let scale = self
            .cx
            .update_window(self.handle(), |_, window, _| window.scale_factor())
            .expect("the window is open");
        let image = self
            .cx
            .capture_screenshot(self.handle())
            .expect("capture the window");
        if let (Some(dir), Some(name)) = (std::env::var_os("MEDIATAGGER_GPUI_SCREENSHOT_DIR"), name)
        {
            let target = PathBuf::from(dir).join(name);
            image.save(&target).expect("write the screenshot");
            eprintln!("screenshot: {}", target.display());
        }
        Frame {
            width: image.width() as usize,
            pixels: image.into_raw(),
            scale,
        }
    }

    fn command(&mut self, command: SettingsCommand) {
        self.send(RuntimeInput::settings(command));
    }

    /// Lets a dialog finish fading in or out.
    fn fade(&mut self) {
        self.cx.advance_clock(Duration::from_millis(500));
        self.settle();
    }

    fn view(&mut self) -> gpui::Entity<SettingsView> {
        let window = self.window;
        self.cx.update(|cx| {
            window
                .read(cx)
                .expect("open")
                .settings()
                .cloned()
                .expect("the settings page is shown")
        })
    }
}

/// A captured frame (RGBA rows), read in CSS pixels.
struct Frame {
    pixels: Vec<u8>,
    width: usize,
    scale: f32,
}

impl Frame {
    fn at(&self, x: f32, y: f32) -> [u8; 3] {
        let column = ((x + 0.5) * self.scale) as usize;
        let row = ((y + 0.5) * self.scale) as usize;
        let start = (row * self.width + column) * 4;
        [
            self.pixels[start],
            self.pixels[start + 1],
            self.pixels[start + 2],
        ]
    }
}

/// `over` drawn on the opaque `under`, as the renderer and a browser mix them.
fn mix(over: Rgba, under: [u8; 3]) -> [u8; 3] {
    let alpha = f32::from(over.a) / 255.0;
    let channel = |over: u8, under: u8| {
        (f32::from(over) * alpha + f32::from(under) * (1.0 - alpha)).round() as u8
    };
    [
        channel(over.r, under[0]),
        channel(over.g, under[1]),
        channel(over.b, under[2]),
    ]
}

fn opaque(colour: Rgba) -> [u8; 3] {
    [colour.r, colour.g, colour.b]
}

fn near(got: [u8; 3], want: [u8; 3]) -> bool {
    got.iter()
        .zip(want)
        .all(|(got, want)| (i32::from(*got) - i32::from(want)).abs() <= 3)
}

#[test]
fn the_settings_page_draws_its_sections_and_marks_the_one_in_view() {
    if !renderer_available() {
        return;
    }
    for theme in [ThemeName::Dark, ThemeName::Light] {
        let mut headless = Headless::open(theme);
        headless.send(RuntimeInput::OpenSettings);
        assert_eq!(headless.active(), SettingsSection::Scan);
        let name = theme.css_name();
        // The current link is drawn in the active suggestion colour: the navigation
        // starts 24 px from the left and 96 px from the top, its first link 9 px in.
        let link = headless.pixel(&format!("settings-scan-{name}.png"), 200.0, 115.0);
        let Rgba { r, g, b, .. } = theme.tokens().colors.tagator_suggestion_active_bg;
        for (channel, want) in link.iter().zip([r, g, b]) {
            assert!(
                (i32::from(*channel) - i32::from(want)).abs() <= 3,
                "{name}: {link:?}"
            );
        }

        headless.navigate(SettingsSection::Appearance);
        assert_eq!(headless.active(), SettingsSection::Appearance);
        headless.pixel(&format!("settings-appearance-{name}.png"), 0.0, 0.0);

        headless.navigate(SettingsSection::About);
        assert_eq!(headless.active(), SettingsSection::About);
        headless.pixel(&format!("settings-about-{name}.png"), 0.0, 0.0);
    }
}

/// The measures of `.settings-layout` and a section card that the checks below read
/// pixels by: the card's left edge, and the first `.settings-operation` row of a data
/// card brought to the scroll margin (border, padding, the heading's line, the gap).
const CARD_LEFT: f32 = layout::PAGE_PADDING_X + layout::NAV_WIDTH + layout::LAYOUT_GAP;
const CARD_RIGHT: f32 = WINDOW_SIZE.0 - layout::PAGE_PADDING_X;
const FIRST_ROW_TOP: f32 =
    96.0 + 1.0 + layout::card::PADDING + layout::card::HEADING_LINE_HEIGHT + layout::data::GAP;

/// Epic E18, reviewed against `ImportExportSection.tsx`, `DuplicatesSection.tsx`,
/// `DuplicateResolverDialog.tsx`, `DuplicateGroupCard.tsx` and `DuplicateAssetCard.tsx`
/// with `src/styles.css`.
#[test]
fn the_data_sections_and_the_resolver_draw_in_both_themes() {
    if !renderer_available() {
        return;
    }
    for theme in [ThemeName::Dark, ThemeName::Light] {
        let colors = &theme.tokens().colors;
        let name = theme.css_name();
        let mut headless = Headless::open(theme);
        headless.send(RuntimeInput::OpenSettings);
        let row_x = CARD_LEFT + 1.0 + layout::card::PADDING + 4.0;
        let row_y = FIRST_ROW_TOP + 4.0;

        // The import/export card at the scroll margin, with its first operation row.
        headless.navigate(SettingsSection::ImportExport);
        assert_eq!(headless.active(), SettingsSection::ImportExport);
        let frame = headless.frame(&format!("settings-import-export-{name}.png"));
        assert!(
            near(
                frame.at(CARD_LEFT + 8.0, 104.0),
                opaque(colors.surface_solid)
            ),
            "{name}: the card"
        );
        assert!(
            near(frame.at(row_x, row_y), opaque(colors.surface_muted)),
            "{name}: the CSV row"
        );

        // The resolver on the scan's one group.
        headless.navigate(SettingsSection::Duplicates);
        let page_corner = headless.capture(None).at(8.0, WINDOW_SIZE.1 - 8.0);
        headless.command(SettingsCommand::StartDuplicates);
        headless.fade();
        let frame = headless.frame(&format!("settings-duplicates-resolver-{name}.png"));
        check_resolver(&frame, theme, page_corner, None);

        // A rename field with the focus shows the outline of daisyUI's `.input:focus`.
        let view = headless.view();
        let handle = headless.handle();
        headless
            .cx
            .update_window(handle, |_, window, cx| {
                let field = view
                    .read(cx)
                    .duplicate_name_input(1)
                    .expect("the first card is drawn")
                    .focus_handle(cx);
                window.focus(&field, cx);
            })
            .expect("the window is open");
        headless.settle();
        let frame = headless.frame(&format!("settings-duplicates-resolver-focus-{name}.png"));
        check_resolver(
            &frame,
            theme,
            page_corner,
            Some(opaque(colors.base_content)),
        );

        // The duplicate card after the scan: Start and the last result beside it.
        headless.command(SettingsCommand::CloseDuplicateDialog);
        headless.fade();
        assert_eq!(headless.active(), SettingsSection::Duplicates);
        let frame = headless.frame(&format!("settings-duplicates-{name}.png"));
        assert!(
            near(frame.at(row_x, row_y), opaque(colors.surface_muted)),
            "{name}: the duplicate row"
        );
        // From the row's right padding leftwards: the muted result line, then the
        // primary Start button.
        let y = FIRST_ROW_TOP + layout::operation::PADDING + 6.0;
        let right = CARD_RIGHT - 1.0 - layout::card::PADDING - layout::operation::PADDING;
        let primary: Vec<f32> = (0..400)
            .map(|step| right - step as f32)
            .filter(|x| near(frame.at(*x, y), opaque(colors.primary)))
            .collect();
        let (Some(button_right), Some(button_left)) = (primary.first(), primary.last()) else {
            panic!("{name}: no Start button in the duplicate row");
        };
        assert!(
            button_right - button_left > 30.0,
            "{name}: Start is {} px wide",
            button_right - button_left
        );
        assert!(
            right - button_right > 60.0,
            "{name}: the last result is drawn right of Start"
        );
    }
}

/// The resolver over the page: the dialog's surface, the dimmed page beside it, and the
/// first rename field's frame (border and fill of `border-base-content/20
/// bg-base-100/70` on the asset card), found by its top border so the check does not
/// depend on a rounded line height above it. `outline` is the colour of the outline a
/// focused field shows; without one nothing is drawn around the field.
fn check_resolver(frame: &Frame, theme: ThemeName, page_corner: [u8; 3], outline: Option<[u8; 3]>) {
    use layout::resolver;
    let colors = &theme.tokens().colors;
    let name = theme.css_name();
    let dialog_top = (WINDOW_SIZE.1 - resolver::height(WINDOW_SIZE.1)) / 2.0;
    let surface = opaque(colors.surface_solid);
    assert!(
        near(frame.at(WINDOW_SIZE.0 / 2.0, dialog_top + 8.0), surface),
        "{name}: the dialog"
    );
    assert_ne!(
        frame.at(8.0, WINDOW_SIZE.1 - 8.0),
        page_corner,
        "{name}: the page beside the dialog is dimmed"
    );

    let group = mix(
        colors
            .base_100
            .with_alpha_percent(resolver::GROUP_FILL_PERCENT),
        surface,
    );
    let card = mix(
        colors
            .base_200
            .with_alpha_percent(resolver::ASSET_FILL_PERCENT),
        group,
    );
    let border = mix(
        colors
            .base_content
            .with_alpha_percent(resolver::INPUT_BORDER_PERCENT),
        card,
    );
    let fill = mix(
        colors
            .base_100
            .with_alpha_percent(resolver::INPUT_FILL_PERCENT),
        card,
    );
    // Right of the text, in the middle of the dialog: a column through the first card.
    let x = WINDOW_SIZE.0 / 2.0 + 200.0;
    // Below the card's top border and the lines above the field.
    let from = dialog_top + 250.0;
    let top = (0..40)
        .map(|step| from + step as f32)
        .find(|y| near(frame.at(x, *y), border))
        .unwrap_or_else(|| panic!("{name}: no field border {border:?} below y {from}"));
    let middle = top + resolver::INPUT_HEIGHT / 2.0;
    assert!(
        near(frame.at(x, middle), fill),
        "{name}: the field's fill {:?}, wanted {fill:?}",
        frame.at(x, middle)
    );
    assert!(
        near(frame.at(x, top + resolver::INPUT_HEIGHT - 1.0), border),
        "{name}: the field is {} px high",
        resolver::INPUT_HEIGHT
    );
    // The outline: 2 px wide, 2 px off the border, above and below the field; the card
    // shows between it and the border and beyond it.
    let outset = resolver::INPUT_OUTLINE_OFFSET + resolver::INPUT_OUTLINE_WIDTH;
    let bottom = top + resolver::INPUT_HEIGHT;
    for (around, edge) in [
        (frame.at(x, top - outset + 0.5), "above"),
        (frame.at(x, bottom - 1.0 + outset - 0.5), "below"),
    ] {
        match outline {
            Some(colour) => assert!(
                near(around, colour),
                "{name}: the outline {edge} the field is {around:?}"
            ),
            None => assert!(near(around, card), "{name}: nothing {edge} the field"),
        }
    }
    for y in [top - 1.0, bottom, top - outset - 2.0, bottom + outset + 1.0] {
        assert!(
            near(frame.at(x, y), card),
            "{name}: the card at {} from the field's top is {:?}",
            y - top,
            frame.at(x, y)
        );
    }
    // The file's name is drawn in the field.
    let ink = (0..120).any(|step| {
        let got = frame.at(WINDOW_SIZE.0 / 2.0 - 320.0 + step as f32, middle);
        !near(got, fill)
    });
    assert!(ink, "{name}: the field shows the file's name");
}
