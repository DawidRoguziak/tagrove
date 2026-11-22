//! The position bar and the month timeline drawn by the headless renderer (epic E10)
//! for a result that overflows the window and spans five years, in both themes: the
//! colours, the month label, the year labels on hover and the focus rings, measured
//! against `GalleryScrollbar.tsx` and `GalleryTimelineScrubber.tsx` (behaviour is in
//! `gallery_position.rs`). Skipped without a GPU or software driver, unless
//! `MEDIATAGGER_GPUI_REQUIRE_RENDERER` is set. With `MEDIATAGGER_GPUI_SCREENSHOT_DIR` set it
//! leaves one PNG per state there for review.

use std::{path::PathBuf, sync::Arc};

use gpui::{
    point, px, AnyWindowHandle, Entity, HeadlessAppContext, Keystroke, Modifiers, MouseMoveEvent,
    Pixels, PlatformInput, Point, WindowHandle,
};
use media_tagger_client::{
    backend::FakeBackend,
    exec::{Executor, ManualExecutor},
    gallery::{
        scrollbar::visuals::scrollbar_visuals,
        timeline::{visuals as rail, TIMELINE_RIGHT, TIMELINE_WIDTH},
    },
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::RuntimeConfig,
    theme::{Rgba, ThemeName},
};
use media_tagger_core::models::{
    AssetQueryPageResult, AssetQuerySection, AssetQuerySectionsResult, AssetSummary, ScanRoot,
    StartAssetQueryResult,
};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui::assets::Assets,
    views::{gallery::GalleryView, shell::ShellView},
    window::{init_application, open_main_window, WINDOW_SIZE},
};

const PAGE: usize = 128;
const TOTAL: usize = 100_000;

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

fn page(offset: usize) -> Vec<AssetSummary> {
    (offset..(offset + PAGE).min(TOTAL)).map(asset).collect()
}

/// Sixty months from December 2024 back to January 2020, 1 600 results each, and a
/// no-date tail of 4 000.
fn sections() -> Vec<AssetQuerySection> {
    let mut sections = Vec::new();
    let mut start = 0;
    for year in (2020..=2024).rev() {
        for month in (1..=12).rev() {
            sections.push(AssetQuerySection {
                t: year * 100 + month,
                start,
                count: 1_600,
            });
            start += 1_600;
        }
    }
    sections.push(AssetQuerySection {
        t: 0,
        start,
        count: TOTAL - start,
    });
    sections
}

fn parts(theme: ThemeName, executor: Arc<ManualExecutor>) -> RuntimeParts {
    let backend = Arc::new(FakeBackend::new());
    backend
        .on_start_asset_query(|request| {
            let offset = request.anchor_offset.unwrap_or(0) / PAGE * PAGE;
            Ok(StartAssetQueryResult::Ready {
                session_id: 1,
                revision: 1,
                total: TOTAL,
                offset,
                items: page(offset),
            })
        })
        .on_get_asset_query_page(|_, offset, _| {
            Ok(AssetQueryPageResult::Ready {
                session_id: 1,
                revision: 1,
                total: TOTAL,
                offset,
                items: page(offset),
            })
        })
        .on_get_asset_query_sections(|_| {
            Ok(AssetQuerySectionsResult::Ready {
                session_id: 1,
                total: TOTAL,
                sections: sections(),
            })
        })
        .on_scan_startup_roots(|| Ok(None))
        .on_list_scan_roots(|| {
            Ok(vec![ScanRoot {
                path: "/media".into(),
                auto_scan_on_startup: false,
            }])
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

fn screenshot_target(name: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("MEDIATAGGER_GPUI_SCREENSHOT_DIR")?);
    Some(dir.join(name))
}

/// A captured frame: RGBA rows in device pixels, read at window (CSS) coordinates.
struct Shot {
    width: u32,
    scale: f32,
    raw: Vec<u8>,
}

impl Shot {
    /// The device pixel at the centre of the CSS pixel whose top left is (x, y).
    fn at(&self, x: f32, y: f32) -> [u8; 3] {
        let x = ((x + 0.5) * self.scale) as u32;
        let y = ((y + 0.5) * self.scale) as u32;
        let i = ((y * self.width + x) * 4) as usize;
        [self.raw[i], self.raw[i + 1], self.raw[i + 2]]
    }
}

fn rgb(color: Rgba) -> [u8; 3] {
    [color.r, color.g, color.b]
}

/// `fill` at its own alpha over the opaque `under`, as the browser composites it.
fn over(fill: Rgba, under: Rgba) -> [u8; 3] {
    let alpha = f32::from(fill.a) / 255.0;
    let mix = |top: u8, bottom: u8| {
        (f32::from(bottom) + (f32::from(top) - f32::from(bottom)) * alpha).round() as u8
    };
    [
        mix(fill.r, under.r),
        mix(fill.g, under.g),
        mix(fill.b, under.b),
    ]
}

#[track_caller]
fn close(pixel: [u8; 3], expected: [u8; 3], what: &str) {
    for (channel, want) in pixel.iter().zip(expected) {
        assert!(
            (i32::from(*channel) - i32::from(want)).abs() <= 2,
            "{what}: {pixel:?}, expected {expected:?}"
        );
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

    /// Runs the backend's calls, the frame callbacks and the redraws they ask for.
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

    fn gallery(&mut self) -> Entity<GalleryView> {
        let window = self.window;
        self.cx.update(|cx| {
            window
                .read(cx)
                .expect("the window is open")
                .gallery()
                .cloned()
                .expect("the gallery is shown")
        })
    }

    fn hover(&mut self, position: Point<Pixels>) {
        let input = PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::none(),
        });
        self.cx
            .update_window(self.handle(), |_, window, cx| {
                window.dispatch_event(input, cx);
            })
            .expect("the window is open");
        self.settle();
    }

    fn key(&mut self, key: &'static str) {
        let keystroke = Keystroke::parse(key).expect("a keystroke");
        self.cx
            .update_window(self.handle(), |_, window, cx| {
                window.dispatch_keystroke(keystroke, cx);
            })
            .expect("the window is open");
        self.settle();
    }

    fn capture(&mut self, name: &str) -> Shot {
        let scale = self
            .cx
            .update_window(self.handle(), |_, window, _| window.scale_factor())
            .expect("the window is open");
        let image = self
            .cx
            .capture_screenshot(self.handle())
            .expect("capture the window");
        if let Some(target) = screenshot_target(name) {
            image.save(&target).expect("write the screenshot");
            eprintln!("screenshot: {}", target.display());
        }
        Shot {
            width: image.width(),
            scale,
            raw: image.into_raw(),
        }
    }
}

/// The idle bar and rail, the rail under the pointer with its month label and year
/// labels, and both focus rings after the keyboard, against the Solid classes:
/// `bg-base-content/5` (`/10` on hover) track, `--border-strong` thumb of at least
/// 32 px, `bg-primary/70` marker, `bg-base-content/30` ticks, a 26 px
/// `--surface-solid` label with a `--border-strong` border, and a 2 px primary ring
/// 2 px outside.
#[test]
fn the_bar_and_the_timeline_draw_as_in_the_solid_gallery() {
    if !renderer_available() {
        return;
    }
    for theme in [ThemeName::Dark, ThemeName::Light] {
        let name = |state: &str| format!("gallery-position-{state}-{theme:?}.png").to_lowercase();
        let tokens = theme.tokens();
        let colors = tokens.colors;
        let background = colors.bg_1;
        let bar_visuals = scrollbar_visuals(tokens);
        let rail_visuals = rail::timeline_visuals(tokens);

        let mut app = Headless::open(theme);
        let gallery = app.gallery();
        let (track, ticks, marker_y) = app.cx.update(|cx| {
            let timeline = gallery.read(cx).timeline();
            (
                timeline.track() as f32,
                timeline.ticks(),
                timeline.marker_y() as f32,
            )
        });
        let (width, height) = WINDOW_SIZE;
        // Both run from 4 px below the viewport's top to 4 px above the window's bottom.
        let top = height - 4.0 - track;
        let bar_x = width - 4.0 - 5.0;
        let rail_left = width - TIMELINE_RIGHT - TIMELINE_WIDTH;
        let rail_x = rail_left + 5.0;
        assert!(ticks.len() >= 5, "a tick per year: {ticks:?}");

        app.hover(point(px(400.0), px(400.0)));
        let idle = app.capture(&name("idle"));
        let at = |shot: &Shot, x: f32, y: f32| shot.at(x, y);
        close(
            at(&idle, width - 19.0, 500.0),
            rgb(background),
            "the gallery beside the rail",
        );
        close(
            at(&idle, bar_x, top + 40.0),
            over(bar_visuals.track, background),
            "the bar's track",
        );
        close(
            at(&idle, bar_x, top + 2.0),
            rgb(bar_visuals.thumb),
            "the thumb at the top",
        );
        close(
            at(&idle, bar_x, top + 30.0),
            rgb(bar_visuals.thumb),
            "the thumb's 32 px",
        );
        close(
            at(&idle, rail_x, top + 400.0),
            rgb(background),
            "the idle rail is clear",
        );
        assert_eq!(marker_y, 0.0);
        close(
            at(&idle, rail_x, top + 2.0),
            over(rail_visuals.marker, background),
            "the marker",
        );
        let tick = ticks[1];
        let tick_y = top + tick.y as f32;
        close(
            at(&idle, width - TIMELINE_RIGHT - 3.0, tick_y),
            over(rail_visuals.tick, background),
            "a year tick",
        );
        close(
            at(&idle, width - TIMELINE_RIGHT - 9.0, tick_y),
            rgb(background),
            "the tick is 8 px wide",
        );

        // The pointer over the rail at 40 %: the hover fill, the month label centred on
        // it and the year labels beside the ticks.
        let pointer = top + 0.4 * track;
        app.hover(point(px(rail_x), px(pointer)));
        let hover = app.capture(&name("rail-hover"));
        close(
            at(&hover, rail_x, top + 400.0),
            over(rail_visuals.rail_hover, background),
            "the rail under the pointer",
        );
        let label_right = width - TIMELINE_RIGHT - rail::LABEL_RIGHT;
        let label_top = pointer - rail::label_height() / 2.0;
        // Clear of the 8 px corners for the borders, inside the right padding for the fill.
        let label_x = label_right - 12.0;
        close(
            at(&hover, label_x, label_top),
            rgb(rail_visuals.label_border),
            "the label's top border",
        );
        close(
            at(&hover, label_x, label_top + rail::label_height() - 1.0),
            rgb(rail_visuals.label_border),
            "the label's bottom border",
        );
        for y in [
            label_top + 8.0,
            pointer,
            label_top + rail::label_height() - 9.0,
        ] {
            close(
                at(&hover, label_right - 4.0, y),
                rgb(rail_visuals.label_fill),
                "the label's fill",
            );
        }
        // `group-hover:block`: the year's digits appear left of its tick.
        let year_label = |shot: &Shot| {
            let mut pixels = Vec::new();
            for dy in -6..4 {
                for dx in 13..30 {
                    pixels.push(at(
                        shot,
                        width - TIMELINE_RIGHT - dx as f32,
                        tick_y + dy as f32,
                    ));
                }
            }
            pixels
        };
        let changed = year_label(&idle)
            .into_iter()
            .zip(year_label(&hover))
            .filter(|(before, after)| before != after)
            .count();
        assert!(
            changed > 10,
            "the year label is drawn on hover ({changed} pixels)"
        );

        // The keyboard on the bar: End, then its ring; Tab, then the rail's ring and its
        // label at the marker.
        app.hover(point(px(400.0), px(400.0)));
        let bar_focus = app
            .cx
            .update(|cx| gallery.read(cx).scrollbar_focus().clone());
        app.cx
            .update_window(app.handle(), |_, window, cx| window.focus(&bar_focus, cx))
            .expect("the window is open");
        app.key("end");
        let end = app.capture(&name("bar-focus-end"));
        let bottom = top + track;
        close(
            at(&end, bar_x, bottom - 2.0),
            rgb(bar_visuals.thumb),
            "the thumb at the end",
        );
        close(
            at(&end, bar_x, bottom - 40.0),
            over(bar_visuals.track, background),
            "the track above the thumb",
        );
        let ring = bar_visuals.focus_ring;
        close(
            at(&end, width - 4.0 - 10.0 - ring.outset() + 1.0, 500.0),
            rgb(ring.color),
            "the bar's ring",
        );
        close(
            at(&end, width - 4.0 - 10.0 - 1.0, 500.0),
            rgb(background),
            "the ring's offset",
        );

        app.key("tab");
        let rail_focus = app.capture(&name("rail-focus"));
        // Drawn over the tiles' edge (the tile area ends 1 px left of the rail), as the
        // rail's `z-10` outline is.
        let ring = rail_visuals.focus_ring;
        close(
            at(&rail_focus, rail_left - ring.outset() + 1.0, 500.0),
            rgb(ring.color),
            "the rail's ring",
        );
    }
}
