//! The settings page drawn by the headless renderer (epic E17) in both themes: the scan
//! section with one root, the appearance and language sections, and the end of the page
//! with the danger zone and About. Skipped without a GPU or software driver, unless
//! `MEDIATAGGER_GPUI_REQUIRE_RENDERER` is set. With `MEDIATAGGER_GPUI_SCREENSHOT_DIR` set
//! it leaves one PNG per state there for review.

use std::{path::PathBuf, sync::Arc};

use gpui::{AnyWindowHandle, HeadlessAppContext, WindowHandle};
use media_tagger_client::{
    backend::FakeBackend,
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    settings::SettingsSection,
    theme::{Rgba, ThemeName},
};
use media_tagger_core::models::{ScanRoot, StartAssetQueryResult, VideoToolStatus};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui::assets::Assets,
    views::shell::ShellView,
    window::{init_application, open_main_window},
};

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
        let scale = self
            .cx
            .update_window(self.handle(), |_, window, _| window.scale_factor())
            .expect("the window is open");
        let image = self
            .cx
            .capture_screenshot(self.handle())
            .expect("capture the window");
        if let Some(dir) = std::env::var_os("MEDIATAGGER_GPUI_SCREENSHOT_DIR") {
            let target = PathBuf::from(dir).join(name);
            image.save(&target).expect("write the screenshot");
            eprintln!("screenshot: {}", target.display());
        }
        let px = image.get_pixel(((x + 0.5) * scale) as u32, ((y + 0.5) * scale) as u32);
        [px[0], px[1], px[2]]
    }
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
