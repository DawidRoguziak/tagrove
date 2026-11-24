//! The settings page (epic E17) on GPUI's test platform with the scripted backend and the
//! deterministic executor: the sections, the navigation, the confirmations, the theme
//! cards and the language list, by pointer and by keyboard.

use std::sync::Arc;

use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use media_tagger_client::{
    backend::FakeBackend,
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    settings::{about::LegalText, layout, navigation, OperationSection, SettingsSection},
    theme::ThemeName,
};
use media_tagger_core::models::{ScanRoot, StartAssetQueryResult, VideoToolStatus};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    views::{settings, shell::ShellView},
    window::{init_application, open_main_window},
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
}

impl Fixture {
    fn with_root() -> Self {
        Self::with_roots(&["/media/synthetic"])
    }

    fn with_roots(paths: &'static [&'static str]) -> Self {
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
            .on_list_scan_roots(move || {
                Ok(paths
                    .iter()
                    .map(|path| ScanRoot {
                        path: (*path).to_owned(),
                        auto_scan_on_startup: true,
                    })
                    .collect())
            })
            .on_get_video_tool_status(|| {
                Ok(VideoToolStatus {
                    ffmpeg_available: true,
                    ffprobe_available: true,
                })
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
}

fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    loop {
        cx.run_until_parked();
        if fixture.executor.run_all() == 0 {
            break;
        }
    }
    cx.run_until_parked();
}

fn open_settings(
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
    send(&window, RuntimeInput::OpenSettings, &mut cx);
    settle(fixture, &mut cx);
    (window, cx)
}

fn send(window: &WindowHandle<ShellView>, input: RuntimeInput, cx: &mut VisualTestContext) {
    let runtime = window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open");
    runtime.update(cx, |runtime, cx| runtime.send(input, cx));
    cx.run_until_parked();
}

fn click(selector: &str, fixture: &Fixture, cx: &mut VisualTestContext) {
    let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    settle(fixture, cx);
}

fn settings_view(
    window: &WindowHandle<ShellView>,
    cx: &VisualTestContext,
) -> gpui::Entity<settings::SettingsView> {
    window
        .read_with(cx, |shell, _| shell.settings().cloned())
        .unwrap()
        .expect("the settings page is shown")
}

#[gpui::test]
fn the_page_shows_every_section_with_the_roots_and_the_video_tools(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (_window, mut cx) = open_settings(&fixture, cx);

    assert!(cx.debug_bounds(settings::SCAN_SECTION_SELECTOR).is_some());
    assert!(cx.debug_bounds(settings::VIDEO_TOOLS_SELECTOR).is_some());
    assert!(cx.debug_bounds(settings::NAV_SELECTOR).is_some());
    for section in SettingsSection::ALL {
        let id: &'static str =
            Box::leak(settings::section_id(section).to_string().into_boxed_str());
        assert!(cx.debug_bounds(id).is_some(), "{id}");
    }
    let sections: Vec<_> = SettingsSection::ALL
        .iter()
        .map(|section| {
            let id: &'static str =
                Box::leak(settings::section_id(*section).to_string().into_boxed_str());
            cx.debug_bounds(id).unwrap().top()
        })
        .collect();
    assert!(
        sections.windows(2).all(|pair| pair[0] <= pair[1]),
        "page order"
    );
}

#[gpui::test]
fn a_navigation_click_scrolls_to_its_section_and_marks_it(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    assert_eq!(
        view.read_with(&cx, |view, _| view.active_section()),
        SettingsSection::Scan
    );

    click(
        &settings::nav_link_id(SettingsSection::About),
        &fixture,
        &mut cx,
    );
    cx.run_until_parked();

    let (offset, active) = view.read_with(&cx, |view, _| {
        (view.scroll_handle().offset().y, view.active_section())
    });
    assert!(offset < gpui::px(0.), "the page scrolled");
    assert_eq!(active, SettingsSection::About);
}

fn bounds(selector: &str, cx: &mut VisualTestContext) -> gpui::Bounds<gpui::Pixels> {
    let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

/// `scrollIntoView({ block: "start" })` on a `.settings-section` (`scroll-margin-top:
/// 96px`): the clicked section starts 96 px below the top of the window, whatever the
/// header's height.
#[gpui::test]
fn a_navigation_click_brings_its_section_to_the_scroll_margin(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);

    for section in [SettingsSection::Appearance, SettingsSection::Language] {
        click(&settings::nav_link_id(section), &fixture, &mut cx);
        let (offset, max) = view.read_with(&cx, |view, _| {
            (
                view.scroll_handle().offset().y,
                view.scroll_handle().max_offset().y,
            )
        });
        assert!(-offset < max, "{section:?}: the page is not at its end");
        assert_eq!(
            bounds(&settings::section_id(section), &mut cx).top(),
            gpui::px(navigation::SCROLL_MARGIN_TOP),
            "{section:?}"
        );
    }
}

/// The two handlers of `SettingsNavigation.tsx`. The click handler scrolls
/// (`scrollIntoView`) and marks the clicked section; when the click moved the page, the
/// scroll event that follows marks by position again. "Danger zone" cannot reach the
/// scroll margin, because the page ends first, and the end of the page marks About. A
/// second click on it moves nothing, so no scroll event follows and "Danger zone" stays
/// marked until the page scrolls or the window is resized.
#[gpui::test]
fn a_click_on_a_section_the_page_ends_before_marks_the_last_section(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    let danger = settings::nav_link_id(SettingsSection::Danger);
    let state = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            (
                -f32::from(view.scroll_handle().offset().y),
                f32::from(view.scroll_handle().max_offset().y),
                view.active_section(),
            )
        })
    };
    // What the position alone marks, from the sections as they are drawn.
    let by_position = |cx: &mut VisualTestContext| {
        let tops: Vec<f32> = SettingsSection::ALL
            .iter()
            .map(|section| f32::from(bounds(&settings::section_id(*section), cx).top()))
            .collect();
        let (offset, max_offset, _) = state(cx);
        navigation::active_section(&tops, navigation::ScrollMetrics { offset, max_offset })
    };

    click(&danger, &fixture, &mut cx);
    let (offset, max, active) = state(&mut cx);
    assert!(offset > 0.0, "the click scrolled the page");
    assert_eq!(offset, max, "to its end");
    assert!(
        bounds(&settings::section_id(SettingsSection::Danger), &mut cx).top()
            > gpui::px(navigation::SCROLL_MARGIN_TOP),
        "the section could not reach the scroll margin"
    );
    assert_eq!(
        active,
        SettingsSection::About,
        "the end of the page marks the last section"
    );

    // Already at the end: the click scrolls nothing and its section stays marked.
    click(&danger, &fixture, &mut cx);
    assert_eq!(state(&mut cx), (max, max, SettingsSection::Danger));
    view.update(&mut cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(state(&mut cx).2, SettingsSection::Danger, "drawn again");

    // A lower window scrolls nothing either, and its resize marks by position.
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.simulate_resize(gpui::size(viewport.width, viewport.height - gpui::px(60.0)));
    cx.run_until_parked();
    let (resized_offset, resized_max, active) = state(&mut cx);
    assert_eq!(resized_offset, max, "the page stayed where it was");
    assert!(resized_max > max, "and is no longer at its end");
    assert_ne!(active, SettingsSection::Danger);
    assert_eq!(active, by_position(&mut cx));

    // Back at the end without a scroll of its own, then scrolled: the position again.
    click(&danger, &fixture, &mut cx);
    click(&danger, &fixture, &mut cx);
    assert_eq!(state(&mut cx).2, SettingsSection::Danger);
    view.update(&mut cx, |view, cx| {
        view.scroll_handle()
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(-(resized_max - 1.0))));
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        state(&mut cx).2,
        SettingsSection::About,
        "within a pixel of the end"
    );
    view.update(&mut cx, |view, cx| {
        view.scroll_handle()
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(state(&mut cx).2, SettingsSection::Scan);
}

/// The end of the page: `pb-4` of the panel and `pb-8` of the page view stay below
/// the last section.
#[gpui::test]
fn the_end_of_the_page_keeps_both_bottom_paddings(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);

    click(
        &settings::nav_link_id(SettingsSection::About),
        &fixture,
        &mut cx,
    );
    let (offset, max) = view.read_with(&cx, |view, _| {
        (
            view.scroll_handle().offset().y,
            view.scroll_handle().max_offset().y,
        )
    });
    assert_eq!(-offset, max, "About's link scrolls the page to its end");
    let viewport = cx.update(|window, _| window.viewport_size());
    let about = bounds(&settings::section_id(SettingsSection::About), &mut cx);
    assert_eq!(
        viewport.height - about.bottom(),
        gpui::px(layout::PAGE_PADDING_BOTTOM + layout::VIEW_PADDING_BOTTOM)
    );
}

#[gpui::test]
fn clear_library_asks_first_and_no_closes_the_question(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    view.update(&mut cx, |view, cx| {
        view.navigate(SettingsSection::Danger, cx)
    });
    cx.run_until_parked();

    click(settings::CLEAR_LIBRARY_ID, &fixture, &mut cx);
    assert!(view.read_with(&cx, |view, cx| view.clear_modal().read(cx).is_open()));
    click(settings::CLEAR_CANCEL_ID, &fixture, &mut cx);
    assert!(!view.read_with(&cx, |view, cx| view.clear_modal().read(cx).is_open()));
    let cleared = fixture
        .backend
        .called_methods()
        .into_iter()
        .filter(|method| *method == "clear_library_data")
        .count();
    assert_eq!(cleared, 0);
    let status = window
        .read_with(&cx, |shell, cx| {
            shell
                .runtime()
                .read(cx)
                .model()
                .settings()
                .status(OperationSection::Danger)
                .loading
        })
        .unwrap();
    assert!(!status);
}

#[gpui::test]
fn the_theme_cards_and_the_language_list_change_the_runtime(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    view.update(&mut cx, |view, cx| {
        view.navigate(SettingsSection::Appearance, cx)
    });
    cx.run_until_parked();

    click(
        &settings::theme_choice_id(ThemeName::Light),
        &fixture,
        &mut cx,
    );
    let theme = window
        .read_with(&cx, |shell, cx| shell.runtime().read(cx).model().theme())
        .unwrap();
    assert_eq!(theme, ThemeName::Light);

    click(settings::LANGUAGE_SELECT_ID, &fixture, &mut cx);
    assert!(view.read_with(&cx, |view, _| view.language_list_open()));
    click(
        &settings::language_option_id(Language::Pl),
        &fixture,
        &mut cx,
    );
    let language = window
        .read_with(&cx, |shell, cx| shell.runtime().read(cx).model().language())
        .unwrap();
    assert_eq!(language, Language::Pl);
    assert!(!view.read_with(&cx, |view, _| view.language_list_open()));
}

/// A pointer click on the open language field closes the list, as a `<select>` does; the
/// next click opens it again, and a press elsewhere that closed it leaves the next field
/// click free to open it.
#[gpui::test]
fn a_click_on_the_open_language_field_closes_the_list(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    view.update(&mut cx, |view, cx| {
        view.navigate(SettingsSection::Appearance, cx)
    });
    cx.run_until_parked();
    let open = |cx: &VisualTestContext| view.read_with(cx, |view, _| view.language_list_open());

    click(settings::LANGUAGE_SELECT_ID, &fixture, &mut cx);
    assert!(open(&cx), "the first click opens the list");
    click(settings::LANGUAGE_SELECT_ID, &fixture, &mut cx);
    assert!(!open(&cx), "a click on the open field closes the list");
    click(settings::LANGUAGE_SELECT_ID, &fixture, &mut cx);
    assert!(open(&cx), "the next click opens it again");

    click(
        &settings::theme_choice_id(ThemeName::Dark),
        &fixture,
        &mut cx,
    );
    assert!(!open(&cx), "a press elsewhere closes the list");
    click(settings::LANGUAGE_SELECT_ID, &fixture, &mut cx);
    assert!(
        open(&cx),
        "the field opens after a press elsewhere closed the list"
    );
}

fn focus(handle: &gpui::FocusHandle, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(handle, cx));
    cx.run_until_parked();
}

fn press(keys: &str, fixture: &Fixture, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(keys);
    settle(fixture, cx);
}

/// A key pressed and released: GPUI's keyboard click (Enter, Space) fires on the release.
fn activate(key: &'static str, fixture: &Fixture, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(key);
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse(key).expect("a key"),
    });
    settle(fixture, cx);
}

fn focused(handle: &gpui::FocusHandle, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| handle.is_focused(window))
}

/// The radios of `AppearanceSection.tsx`: the checked card is the group's one tab stop,
/// the arrows move the focus and the selection (wrapping), Space selects the focused card.
#[gpui::test]
fn the_theme_cards_are_one_tab_stop_driven_by_the_arrows(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    view.update(&mut cx, |view, cx| {
        view.navigate(SettingsSection::Appearance, cx)
    });
    cx.run_until_parked();
    let theme = |cx: &VisualTestContext| {
        window
            .read_with(cx, |shell, cx| shell.runtime().read(cx).model().theme())
            .unwrap()
    };
    let (dark, light, language) = view.read_with(&cx, |view, _| {
        (
            view.theme_focus(ThemeName::Dark).clone(),
            view.theme_focus(ThemeName::Light).clone(),
            view.language_focus().clone(),
        )
    });

    focus(&dark, &mut cx);
    press("tab", &fixture, &mut cx);
    assert!(
        focused(&language, &mut cx),
        "Tab leaves the group for the language field"
    );
    press("shift-tab", &fixture, &mut cx);
    assert!(
        focused(&dark, &mut cx),
        "Shift+Tab lands on the checked card, not on Light"
    );

    press("left", &fixture, &mut cx);
    assert_eq!(theme(&cx), ThemeName::Light);
    assert!(focused(&light, &mut cx));
    press("down", &fixture, &mut cx);
    assert_eq!(theme(&cx), ThemeName::Dark, "the arrows wrap");
    assert!(focused(&dark, &mut cx));

    focus(&light, &mut cx);
    activate("space", &fixture, &mut cx);
    assert_eq!(theme(&cx), ThemeName::Light);
}

/// The `<select>` of `LanguageSection.tsx`: Enter opens the list, the arrows mark a
/// language, Enter picks it and the focus stays on the field; Escape closes the list and
/// keeps the language and the page.
#[gpui::test]
fn the_language_field_opens_and_picks_from_the_keyboard(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    view.update(&mut cx, |view, cx| {
        view.navigate(SettingsSection::Language, cx)
    });
    cx.run_until_parked();
    let language = |cx: &VisualTestContext| {
        window
            .read_with(cx, |shell, cx| shell.runtime().read(cx).model().language())
            .unwrap()
    };
    let field = view.read_with(&cx, |view, _| view.language_focus().clone());
    focus(&field, &mut cx);

    activate("enter", &fixture, &mut cx);
    assert!(view.read_with(&cx, |view, _| view.language_list_open()));
    assert_eq!(
        view.read_with(&cx, |view, _| view.highlighted_language()),
        Language::En,
        "the list opens on the current language"
    );
    press("up", &fixture, &mut cx);
    assert_eq!(
        view.read_with(&cx, |view, _| view.highlighted_language()),
        Language::En,
        "no wrapping above the first language"
    );
    press("down", &fixture, &mut cx);
    assert_eq!(
        view.read_with(&cx, |view, _| view.highlighted_language()),
        Language::ALL[1]
    );
    activate("enter", &fixture, &mut cx);
    assert_eq!(language(&cx), Language::ALL[1]);
    assert!(!view.read_with(&cx, |view, _| view.language_list_open()));
    assert!(focused(&field, &mut cx), "the focus stays on the field");

    press("end", &fixture, &mut cx);
    assert!(
        view.read_with(&cx, |view, _| view.language_list_open()),
        "End opens the list"
    );
    press("end", &fixture, &mut cx);
    assert_eq!(
        view.read_with(&cx, |view, _| view.highlighted_language()),
        *Language::ALL.last().unwrap()
    );
    press("escape", &fixture, &mut cx);
    assert!(!view.read_with(&cx, |view, _| view.language_list_open()));
    assert_eq!(language(&cx), Language::ALL[1], "Escape picks nothing");
    assert!(
        window
            .read_with(&cx, |shell, _| shell.settings().is_some())
            .unwrap(),
        "Escape closes the list, not the page"
    );
}

/// The anchors and the `<details>` summaries of the page: tab stops that Enter and Space
/// activate.
#[gpui::test]
fn the_navigation_links_and_the_legal_summaries_answer_enter_and_space(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    let (about, license) = view.read_with(&cx, |view, _| {
        (
            view.nav_focus(SettingsSection::About).clone(),
            view.legal_focus(LegalText::License).clone(),
        )
    });
    assert!(about.tab_stop && license.tab_stop, "Tab reaches both");

    focus(&about, &mut cx);
    activate("enter", &fixture, &mut cx);
    cx.run_until_parked();
    assert_eq!(
        view.read_with(&cx, |view, _| view.active_section()),
        SettingsSection::About
    );
    assert!(!focused(&about, &mut cx), "the focus left the link");

    focus(&license, &mut cx);
    activate("space", &fixture, &mut cx);
    assert!(view.read_with(&cx, |view, _| view.legal_open(LegalText::License)));
    activate("enter", &fixture, &mut cx);
    assert!(!view.read_with(&cx, |view, _| view.legal_open(LegalText::License)));
}

fn drawn(selector: &str, cx: &mut VisualTestContext) -> bool {
    let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
    cx.debug_bounds(selector).is_some()
}

/// `.settings-nav` and its links (`src/styles.css`): the navigation starts 96 px from
/// the top and 24 px from the left, 240 px wide. A link is as high as its icon makes it:
/// `UiIcon` keeps its own `h-5 w-5` (20 px) over the link's `h-4 w-4`, so a link is
/// 40 px high and the links are 44 px apart.
#[gpui::test]
fn the_navigation_links_are_as_high_as_their_icons_make_them(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (_window, mut cx) = open_settings(&fixture, cx);
    assert_eq!(layout::nav::ICON_SIZE, 20.0);
    assert_eq!(layout::nav::LINK_HEIGHT, 40.0);

    let nav = bounds(settings::NAV_SELECTOR, &mut cx);
    assert_eq!(nav.top(), gpui::px(layout::nav::STICKY_TOP));
    assert_eq!(nav.left(), gpui::px(layout::PAGE_PADDING_X));
    assert_eq!(nav.size.width, gpui::px(layout::NAV_WIDTH));
    let links: Vec<_> = SettingsSection::ALL
        .iter()
        .map(|section| bounds(&settings::nav_link_id(*section), &mut cx))
        .collect();
    for link in &links {
        assert_eq!(link.size.height, gpui::px(40.0));
    }
    for pair in links.windows(2) {
        assert_eq!(pair[1].top() - pair[0].top(), gpui::px(44.0));
    }
    // Seven links, six gaps, the padding and the border: the card ends 418 px from the
    // top of the window.
    let count = SettingsSection::ALL.len() as f32;
    assert_eq!(
        nav.size.height,
        gpui::px(
            count * layout::nav::LINK_HEIGHT
                + (count - 1.0) * layout::nav::GAP
                + 2.0 * layout::nav::PADDING
                + 2.0
        )
    );
    assert_eq!(nav.bottom(), gpui::px(418.0));
}

/// `onClick` of `SettingsNavigation.tsx` (`target.focus({ preventScroll: true })` on a
/// section with `tabindex="-1"`), SettingsPanel.test.tsx "renders all sections in order
/// and navigates by focus" and AboutSection.test.tsx "moves navigation focus to its
/// section": an activated link moves the focus to its section, which is no tab stop
/// itself, and the next Tab reaches the first control of that section. The section shows
/// the focus ring after a key, not after a click.
#[gpui::test]
fn an_activated_navigation_link_moves_the_focus_to_its_section(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    let (link, appearance, language, dark, field) = view.read_with(&cx, |view, _| {
        (
            view.nav_focus(SettingsSection::Appearance).clone(),
            view.section_focus(SettingsSection::Appearance).clone(),
            view.section_focus(SettingsSection::Language).clone(),
            view.theme_focus(ThemeName::Dark).clone(),
            view.language_focus().clone(),
        )
    });
    assert!(
        !appearance.tab_stop && !language.tab_stop,
        "a section is not a tab stop"
    );
    assert!(!drawn(settings::SECTION_FOCUS_RING_SELECTOR, &mut cx));

    // By keyboard.
    focus(&link, &mut cx);
    activate("enter", &fixture, &mut cx);
    assert!(focused(&appearance, &mut cx), "Enter moves the focus");
    assert_eq!(
        view.read_with(&cx, |view, _| view.active_section()),
        SettingsSection::Appearance
    );
    assert_eq!(
        bounds(&settings::section_id(SettingsSection::Appearance), &mut cx).top(),
        gpui::px(navigation::SCROLL_MARGIN_TOP)
    );
    assert!(
        drawn(settings::SECTION_FOCUS_RING_SELECTOR, &mut cx),
        "the section shows where the keyboard is"
    );
    press("tab", &fixture, &mut cx);
    assert!(
        focused(&dark, &mut cx),
        "Tab enters the section at its first control, the checked theme card"
    );
    assert!(!drawn(settings::SECTION_FOCUS_RING_SELECTOR, &mut cx));

    // By pointer.
    click(
        &settings::nav_link_id(SettingsSection::Language),
        &fixture,
        &mut cx,
    );
    assert!(focused(&language, &mut cx), "a click moves the focus");
    assert_eq!(
        view.read_with(&cx, |view, _| view.active_section()),
        SettingsSection::Language
    );
    assert!(
        !drawn(settings::SECTION_FOCUS_RING_SELECTOR, &mut cx),
        "no ring after a pointer click"
    );
    press("tab", &fixture, &mut cx);
    assert!(
        focused(&field, &mut cx),
        "Tab enters the section at the language field"
    );
}

/// confirmationPresence.test.tsx "retains the root path through the remove confirmation
/// exit": the closed dialog keeps the path it asked about while it fades out, and the
/// next question shows its own root.
#[gpui::test]
fn the_remove_confirmation_keeps_its_path_while_it_fades_out(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(&["/media/synthetic", "/media/other"]);
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    let shown = |cx: &VisualTestContext| view.read_with(cx, |view, _| view.remove_dialog_path());
    let presence = |cx: &VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let modal = view.remove_modal().read(cx);
            (modal.is_open(), modal.is_present())
        })
    };
    let asked = |cx: &VisualTestContext| {
        window
            .read_with(cx, |shell, cx| {
                shell
                    .runtime()
                    .read(cx)
                    .model()
                    .settings()
                    .remove_root_confirm()
                    .map(str::to_owned)
            })
            .unwrap()
    };

    click(&settings::root_remove_id(0), &fixture, &mut cx);
    assert_eq!(presence(&cx), (true, true));
    assert_eq!(shown(&cx), "/media/synthetic");

    click(settings::REMOVE_CANCEL_ID, &fixture, &mut cx);
    assert_eq!(asked(&cx), None, "the model forgot the root");
    assert_eq!(
        presence(&cx),
        (false, true),
        "the dialog is closed and fades out"
    );
    assert!(
        drawn(settings::REMOVE_CONFIRM_ID, &mut cx),
        "it is still drawn"
    );
    assert_eq!(
        shown(&cx),
        "/media/synthetic",
        "with the path it asked about"
    );

    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(presence(&cx), (false, false), "the fade has ended");
    assert!(!drawn(settings::REMOVE_CONFIRM_ID, &mut cx));

    click(&settings::root_remove_id(1), &fixture, &mut cx);
    assert_eq!(presence(&cx), (true, true));
    assert_eq!(
        shown(&cx),
        "/media/other",
        "the next question shows its own root"
    );
}

/// The two `<details>` of `AboutSection.tsx` by pointer: a click on a summary unfolds its
/// text and turns its disclosure marker, a second click folds it again, and each summary
/// keeps its own state.
#[gpui::test]
fn a_click_on_a_legal_summary_unfolds_and_folds_its_text_alone(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    // The summaries are at the end of the page; an unfolded text moves what is below it.
    let to_end = |cx: &mut VisualTestContext| {
        view.update(cx, |view, cx| view.navigate(SettingsSection::About, cx));
        cx.run_until_parked();
    };
    let state = |text: LegalText, cx: &mut VisualTestContext| {
        let open = view.read_with(cx, |view, _| view.legal_open(text));
        assert_eq!(
            drawn(&settings::legal_marker_selector(text, true), cx),
            open,
            "{text:?}: the marker points down only while unfolded"
        );
        assert_eq!(
            drawn(&settings::legal_marker_selector(text, false), cx),
            !open,
            "{text:?}: the marker points right only while folded"
        );
        assert_eq!(
            drawn(&settings::legal_text_selector(text), cx),
            open,
            "{text:?}: the text is drawn only while unfolded"
        );
        open
    };
    let toggle = |text: LegalText, cx: &mut VisualTestContext| {
        to_end(cx);
        click(&settings::legal_summary_id(text), &fixture, cx);
    };

    to_end(&mut cx);
    assert!(!state(LegalText::License, &mut cx) && !state(LegalText::Privacy, &mut cx));

    toggle(LegalText::License, &mut cx);
    assert!(state(LegalText::License, &mut cx), "a click unfolds");
    assert!(
        !state(LegalText::Privacy, &mut cx),
        "the other stays folded"
    );

    toggle(LegalText::Privacy, &mut cx);
    assert!(state(LegalText::License, &mut cx) && state(LegalText::Privacy, &mut cx));

    toggle(LegalText::License, &mut cx);
    assert!(!state(LegalText::License, &mut cx), "a second click folds");
    assert!(
        state(LegalText::Privacy, &mut cx),
        "the other stays unfolded"
    );

    toggle(LegalText::Privacy, &mut cx);
    assert!(!state(LegalText::License, &mut cx) && !state(LegalText::Privacy, &mut cx));
}

/// The tab order of `AppSettingsView.tsx` and `SettingsPanel.tsx`, the order of their DOM:
/// the header's controls, the navigation links in page order, then the controls of the
/// sections in page order. Shift+Tab goes the same way back, also from a section's first
/// control and from a section a navigation link moved the focus to.
#[gpui::test]
fn tab_goes_through_the_header_the_navigation_and_then_the_sections(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (window, mut cx) = open_settings(&fixture, cx);
    let view = settings_view(&window, &cx);
    let (links, sections, dark, field, license, privacy) = view.read_with(&cx, |view, _| {
        (
            SettingsSection::ALL.map(|section| view.nav_focus(section).clone()),
            SettingsSection::ALL.map(|section| view.section_focus(section).clone()),
            view.theme_focus(ThemeName::Dark).clone(),
            view.language_focus().clone(),
            view.legal_focus(LegalText::License).clone(),
            view.legal_focus(LegalText::Privacy).clone(),
        )
    });
    /// Where the focus is.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Place {
        Header,
        Link(usize),
        /// On a control inside the section.
        Section(usize),
    }
    let place = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            if let Some(index) = links.iter().position(|link| link.is_focused(window)) {
                return Place::Link(index);
            }
            match sections
                .iter()
                .position(|section| section.contains_focused(window, cx))
            {
                Some(index) => {
                    assert!(
                        !sections[index].is_focused(window),
                        "a section is no tab stop"
                    );
                    Place::Section(index)
                }
                None => Place::Header,
            }
        })
    };
    let last = SettingsSection::ALL.len() - 1;

    // Forward, a whole round from the first navigation link.
    focus(&links[0], &mut cx);
    let mut round = vec![place(&mut cx)];
    loop {
        press("tab", &fixture, &mut cx);
        let at = place(&mut cx);
        if at == Place::Link(0) {
            break;
        }
        round.push(at);
        assert!(round.len() < 64, "Tab comes round: {round:?}");
    }
    let links_end = round
        .iter()
        .position(|at| !matches!(at, Place::Link(_)))
        .expect("the round leaves the navigation");
    assert_eq!(
        round[..links_end],
        std::array::from_fn::<_, 7, _>(Place::Link),
        "the links in page order"
    );
    let header_start = round
        .iter()
        .position(|at| *at == Place::Header)
        .expect("the round reaches the header");
    let in_sections: Vec<usize> = round[links_end..header_start]
        .iter()
        .map(|at| match at {
            Place::Section(index) => *index,
            other => panic!("{other:?} among the sections: {round:?}"),
        })
        .collect();
    assert!(
        in_sections.windows(2).all(|pair| pair[0] <= pair[1]),
        "the sections in page order: {in_sections:?}"
    );
    let visited: Vec<usize> = SettingsSection::ALL
        .iter()
        .map(|section| section.index())
        .filter(|index| in_sections.contains(index))
        .collect();
    // Every section has a control since the import/export and duplicate sections were
    // built (epic E18).
    assert_eq!(
        visited,
        SettingsSection::ALL.map(SettingsSection::index),
        "every section with a control: {round:?}"
    );
    assert_eq!(
        in_sections
            .iter()
            .filter(|index| **index == SettingsSection::ImportExport.index())
            .count(),
        4,
        "the four import and export actions: {round:?}"
    );
    assert_eq!(
        in_sections
            .iter()
            .filter(|index| **index == SettingsSection::Duplicates.index())
            .count(),
        1,
        "the duplicate section's Start: {round:?}"
    );
    assert!(
        round[header_start..].iter().all(|at| *at == Place::Header),
        "after the last section only the header is left before the links: {round:?}"
    );

    // The ends of the three parts.
    focus(&privacy, &mut cx);
    press("tab", &fixture, &mut cx);
    assert_eq!(
        place(&mut cx),
        Place::Header,
        "after the last control of the page"
    );
    focus(&links[last], &mut cx);
    press("tab", &fixture, &mut cx);
    assert_eq!(
        place(&mut cx),
        Place::Section(SettingsSection::Scan.index()),
        "after the last link, the first control of the first section"
    );
    press("shift-tab", &fixture, &mut cx);
    assert_eq!(
        place(&mut cx),
        Place::Link(last),
        "and back to the last link"
    );
    focus(&links[0], &mut cx);
    press("shift-tab", &fixture, &mut cx);
    assert_eq!(place(&mut cx), Place::Header, "before the first link");
    press("tab", &fixture, &mut cx);
    assert_eq!(place(&mut cx), Place::Link(0));

    // Backwards out of a section: the last control of the section before it.
    focus(&dark, &mut cx);
    press("shift-tab", &fixture, &mut cx);
    assert_eq!(
        place(&mut cx),
        Place::Section(SettingsSection::Scan.index()),
        "from the first control of Appearance"
    );
    press("tab", &fixture, &mut cx);
    assert!(focused(&dark, &mut cx));
    focus(&license, &mut cx);
    press("shift-tab", &fixture, &mut cx);
    assert_eq!(
        place(&mut cx),
        Place::Section(SettingsSection::About.index()),
        "the summaries follow About's link buttons"
    );
    assert!(!focused(&license, &mut cx) && !focused(&privacy, &mut cx));

    // From a section a link moved the focus to: forward into it, backwards to the
    // section before it.
    focus(&links[SettingsSection::Language.index()], &mut cx);
    activate("enter", &fixture, &mut cx);
    assert!(focused(
        &sections[SettingsSection::Language.index()],
        &mut cx
    ));
    press("shift-tab", &fixture, &mut cx);
    assert!(
        focused(&dark, &mut cx),
        "the last control before the Language section"
    );
    focus(&links[SettingsSection::Language.index()], &mut cx);
    activate("enter", &fixture, &mut cx);
    press("tab", &fixture, &mut cx);
    assert!(focused(&field, &mut cx), "the section's first control");
}

/// The settings header of `SettingsPage.tsx`: `py-3` around the heading block
/// (`text-2xl leading-tight` 30 px, `mt-0.5`, and a `text-xs` description of 16 px
/// line height) and its `border-b`, so its bottom border is the pixel row at y 72 as
/// in the Tauri window, not 74 (comparison scenarios `settings-*`, epic E19). The
/// heading block starts at the padding.
#[gpui::test]
fn the_settings_header_is_as_high_as_the_tauri_header(cx: &mut TestAppContext) {
    let fixture = Fixture::with_root();
    let (_window, mut cx) = open_settings(&fixture, cx);
    let header = bounds(settings::HEADER_SELECTOR, &mut cx);
    let heading = bounds(settings::HEADING_SELECTOR, &mut cx);
    // 12 + 30 + 2 + 16 + 12, and the 1 px border.
    assert_eq!(header.size.height, gpui::px(73.), "{header:?}");
    assert_eq!(header.bottom() - gpui::px(1.), header.top() + gpui::px(72.));
    assert_eq!(heading.top(), header.top() + gpui::px(12.));
    assert_eq!(heading.size.height, gpui::px(30.));
    // The page below starts under the header.
    let body = bounds(settings::BODY_SELECTOR, &mut cx);
    assert!(body.top() >= header.bottom());
}
