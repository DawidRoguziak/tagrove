//! The settings page (`AppSettingsView.tsx`, `SettingsPanel.tsx`): its header with
//! Back, the logo, the heading and the window controls, the navigation, the sections
//! and the two confirmations of the page.
//!
//! The page is a non-modal layer of the layer stack while it is shown, so Escape
//! closes it; the shell registers and releases that layer, because it outlives the
//! page and knows where the focus goes back to.
//!
//! Everything that must outlive the page (the operations, their statuses, the
//! confirmations, the video tools) lives in the runtime's settings model; the page
//! keeps only what the Solid page loses when it unmounts: the scroll position, the
//! expanded root list, the open language list and the open legal texts.
//!
//! Layout: the sections scroll below the header; the navigation stays where the Tauri
//! page's sticky navigation stays, 96 px below the top of the window. The scrolled
//! container's children are exactly the seven sections, in page order, so the
//! scroll handle knows where each one is.
//!
//! Focus: Tab goes through the header, the navigation links and then the controls of
//! the sections, the order of the Tauri page's DOM. A section is focusable without
//! being a tab stop (`tabindex="-1"` of `SettingsPanel.tsx`). Activating a navigation link, by pointer or by keyboard, moves
//! the focus to its section, so the next Tab reaches the first control of that section;
//! the section shows the focus ring only while the keyboard is in use.

use std::{cell::Cell, rc::Rc};

use gpui::{
    actions, anchored, canvas, deferred, div, point, prelude::*, px, relative, AnyElement, App,
    ClickEvent, Context, Entity, FocusHandle, KeyBinding, MouseButton, MouseDownEvent, PathBuilder,
    Pixels, Point, Role, ScrollHandle, SharedString, Subscription, Toggled, Window,
};
use media_tagger_client::{
    controls::{ButtonVariant, FocusRing, ProgressValue, SpinnerSize},
    i18n::{I18n, Language},
    icons::IconName,
    overlay::{LayerId, ModalSize, ModalSnapshot},
    runtime::RuntimeInput,
    settings::{
        about::{self, AboutLink, LegalText},
        layout,
        navigation::{active_section, scroll_target, ScrollMetrics},
        progress::progress_label,
        scan::{expand_toggle_key, has_expand_toggle, video_tool_line, visible_roots},
        status::progress_bar,
        OperationSection, SectionStatus, SettingsCommand, SettingsSection,
    },
    shell::{settings_header, PRODUCT_NAME},
    shortcuts::KeyContext,
    theme::{Rgba, ThemeName, RADII, TYPOGRAPHY},
    window_chrome::DragSurface,
};

use super::chrome::{self, drag_surface, no_drag};
use crate::{
    runtime::{self, Runtime},
    ui::{
        button::Button,
        fonts,
        icon::Icon,
        layers::{self, LayerOptions},
        logo::logo,
        modal::{Modal, ModalEvent, ModalState},
        progress::ProgressBar,
        spinner::Spinner,
        switch::Switch,
        theme::{self, box_shadows, color, focus_ring, weight},
    },
};

/// Element id of the Back button.
pub const BACK_ID: &str = "settings-back";
/// Debug selectors (`VisualTestContext::debug_bounds`).
pub const HEADER_SELECTOR: &str = "settings-header";
pub const HEADING_SELECTOR: &str = "settings-heading";
pub const BODY_SELECTOR: &str = "settings-body";
pub const NAV_SELECTOR: &str = "settings-nav";
/// The focus ring of the section the keyboard moved the focus to.
pub const SECTION_FOCUS_RING_SELECTOR: &str = "settings-section-focus-ring";
/// The scan section card (`data-testid="scan-settings-section"`).
pub const SCAN_SECTION_SELECTOR: &str = "scan-settings-section";
pub const SCAN_LOADER_SELECTOR: &str = "scan-section-loader";
pub const DANGER_LOADER_SELECTOR: &str = "danger-section-loader";
pub const VIDEO_TOOLS_SELECTOR: &str = "video-tool-status";
/// Element ids of the actions.
pub const CHOOSE_FOLDER_ID: &str = "settings-choose-folder";
pub const RESCAN_ALL_ID: &str = "settings-rescan-all";
pub const RENDER_ALL_ID: &str = "settings-render-all-thumbnails";
pub const RENDER_FAILED_ID: &str = "settings-render-failed-thumbnails";
pub const STOP_RENDER_ID: &str = "settings-stop-thumbnail-render";
pub const EXPAND_ROOTS_ID: &str = "settings-roots-expand";
pub const CLEAR_LIBRARY_ID: &str = "settings-clear-library";
pub const LANGUAGE_SELECT_ID: &str = "settings-language-select";
pub const REMOVE_CONFIRM_ID: &str = "settings-remove-root-confirm";
pub const REMOVE_CANCEL_ID: &str = "settings-remove-root-cancel";
pub const CLEAR_CONFIRM_ID: &str = "settings-clear-confirm";
pub const CLEAR_CANCEL_ID: &str = "settings-clear-cancel";

/// The key context of the theme cards and of the language field.
pub const THEME_KEY_CONTEXT: &str = "SettingsThemeChoice";
pub const LANGUAGE_KEY_CONTEXT: &str = "SettingsLanguageSelect";

actions!(
    settings_page,
    [
        /// Right and Down on a theme card: the next card, selected (a radio group).
        NextTheme,
        /// Left and Up on a theme card: the previous card, selected.
        PreviousTheme,
        /// Down on the language field: opens the list, or marks the next language.
        NextLanguage,
        /// Up on the language field: opens the list, or marks the previous language.
        PreviousLanguage,
        /// Home on the language field: opens the list, or marks the first language.
        FirstLanguage,
        /// End on the language field: opens the list, or marks the last language.
        LastLanguage,
    ]
);

/// The keys of the theme cards (one tab stop, arrows move and select, as the radios of
/// `AppearanceSection.tsx`) and of the language field (the list of the `<select>` of
/// `LanguageSection.tsx`). Enter and Space on these controls, on a navigation link and on
/// a legal text's summary are GPUI's keyboard click.
pub fn key_bindings() -> Vec<KeyBinding> {
    let theme = Some(THEME_KEY_CONTEXT);
    let language = Some(LANGUAGE_KEY_CONTEXT);
    vec![
        KeyBinding::new("right", NextTheme, theme),
        KeyBinding::new("down", NextTheme, theme),
        KeyBinding::new("left", PreviousTheme, theme),
        KeyBinding::new("up", PreviousTheme, theme),
        KeyBinding::new("down", NextLanguage, language),
        KeyBinding::new("up", PreviousLanguage, language),
        KeyBinding::new("home", FirstLanguage, language),
        KeyBinding::new("end", LastLanguage, language),
    ]
}

/// Binds the page's keys. Part of `views::init`.
pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

/// The theme cards in page order.
const THEME_CHOICES: [ThemeName; 2] = [ThemeName::Light, ThemeName::Dark];

/// The element id of a section's anchor (`settings-<id>`).
pub fn section_id(section: SettingsSection) -> SharedString {
    format!("settings-{}", section.id()).into()
}

/// The element id of a navigation link.
pub fn nav_link_id(section: SettingsSection) -> SharedString {
    format!("settings-nav-{}", section.id()).into()
}

/// Element ids of a root row's controls, by the row's index.
pub fn root_switch_id(index: usize) -> SharedString {
    format!("settings-root-{index}-auto-scan").into()
}
pub fn root_rescan_id(index: usize) -> SharedString {
    format!("settings-root-{index}-rescan").into()
}
pub fn root_remove_id(index: usize) -> SharedString {
    format!("settings-root-{index}-remove").into()
}
/// Element id of a theme card and of a language option.
pub fn theme_choice_id(theme: ThemeName) -> SharedString {
    format!("settings-theme-{}", theme.css_name()).into()
}
pub fn language_option_id(language: Language) -> SharedString {
    format!("settings-language-{}", language.code()).into()
}
/// Element id of a link button and of a folding legal text's summary.
pub fn about_link_id(link: AboutLink) -> SharedString {
    match link {
        AboutLink::Source => "settings-about-source".into(),
        AboutLink::Contact => "settings-about-contact".into(),
    }
}
pub fn legal_summary_id(text: LegalText) -> SharedString {
    match text {
        LegalText::License => "settings-about-license".into(),
        LegalText::Privacy => "settings-about-privacy".into(),
    }
}
/// Debug selector of a legal text's disclosure marker, by the state it is drawn in, and
/// of the unfolded text itself.
pub fn legal_marker_selector(text: LegalText, open: bool) -> String {
    let state = if open { "open" } else { "closed" };
    format!("{}-marker-{state}", legal_summary_id(text))
}
pub fn legal_text_selector(text: LegalText) -> String {
    format!("{}-text", legal_summary_id(text))
}

pub struct SettingsView {
    runtime: Entity<Runtime>,
    drag: Rc<Cell<DragSurface>>,
    scroll: ScrollHandle,
    /// The section a navigation click marked, with the scroll offset it left; it stays
    /// marked until the page scrolls.
    clicked: Option<(SettingsSection, f32)>,
    roots_expanded: bool,
    language_layer: Option<LayerId>,
    /// Where the last press outside the open list closed it. The field's own mouse down
    /// turns it into `press_closed_language_list`, so the click that follows the press
    /// does not open the list again.
    language_closed_at: Option<Point<Pixels>>,
    /// The current press on the language field closed the open list.
    press_closed_language_list: bool,
    /// The language the open list marks for Enter (an index into `Language::ALL`).
    language_highlight: usize,
    legal_open: [bool; 2],
    /// A scenario's navigation click waits for the frame that lays out the page the
    /// launch's work left; that frame has been drawn.
    scripted_page_laid_out: bool,
    /// The sections: focusable, not tab stops (`tabindex="-1"`).
    section_focus: [FocusHandle; SettingsSection::ALL.len()],
    /// Tab stops of the page's own controls: the navigation links, the theme cards (the
    /// checked one is the group's only stop), the language field and the legal summaries.
    nav_focus: [FocusHandle; SettingsSection::ALL.len()],
    theme_focus: [FocusHandle; 2],
    language_focus: FocusHandle,
    legal_focus: [FocusHandle; 2],
    remove_modal: Entity<ModalState>,
    clear_modal: Entity<ModalState>,
    remove_snapshot: ModalSnapshot<String>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(runtime: Entity<Runtime>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let remove_modal = cx.new(ModalState::new);
        let clear_modal = cx.new(ModalState::new);
        let subscriptions = vec![
            cx.observe_in(&runtime, window, |view, _, window, cx| {
                view.sync(window, cx)
            }),
            cx.observe_window_bounds(window, |_, _, cx| cx.notify()),
            cx.subscribe_in(&remove_modal, window, |view, _, event, _, cx| {
                if *event == ModalEvent::Dismiss {
                    view.command(SettingsCommand::CancelRemoveRoot, cx);
                }
            }),
            cx.subscribe_in(&clear_modal, window, |view, _, event, _, cx| {
                if *event == ModalEvent::Dismiss {
                    view.command(SettingsCommand::CloseClearConfirm, cx);
                }
            }),
        ];
        // A page that goes away with its language list open leaves no layer behind.
        cx.on_release(|view: &mut Self, cx| {
            if let Some(layer) = view.language_layer.take() {
                layers::forget(layer, cx);
            }
        })
        .detach();
        let mut view = Self {
            runtime,
            drag: Rc::default(),
            scroll: ScrollHandle::new(),
            clicked: None,
            roots_expanded: false,
            language_layer: None,
            language_closed_at: None,
            press_closed_language_list: false,
            language_highlight: 0,
            legal_open: [false; 2],
            scripted_page_laid_out: false,
            section_focus: std::array::from_fn(|_| cx.focus_handle()),
            nav_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            theme_focus: std::array::from_fn(|_| cx.focus_handle()),
            language_focus: cx.focus_handle().tab_stop(true),
            legal_focus: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            remove_modal,
            clear_modal,
            remove_snapshot: ModalSnapshot::new(),
            _subscriptions: subscriptions,
        };
        view.sync(window, cx);
        view
    }

    /// Whether the scan section is to be highlighted (opened by "Add first folder").
    pub fn highlights_scan(&self, cx: &gpui::App) -> bool {
        self.runtime.read(cx).model().highlight_scan()
    }

    /// The scroll handle of the sections.
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }

    /// The remove confirmation's dialog.
    pub fn remove_modal(&self) -> &Entity<ModalState> {
        &self.remove_modal
    }

    /// The clear confirmation's dialog.
    pub fn clear_modal(&self) -> &Entity<ModalState> {
        &self.clear_modal
    }

    /// Whether the language list is open.
    pub fn language_list_open(&self) -> bool {
        self.language_layer.is_some()
    }

    /// The language the open list marks for Enter.
    pub fn highlighted_language(&self) -> Language {
        Language::ALL[self.language_highlight]
    }

    /// The focus handle of a navigation link.
    pub fn nav_focus(&self, section: SettingsSection) -> &FocusHandle {
        &self.nav_focus[section.index()]
    }

    /// The focus handle of a section: where the focus goes when its navigation link is
    /// activated.
    pub fn section_focus(&self, section: SettingsSection) -> &FocusHandle {
        &self.section_focus[section.index()]
    }

    /// The path the remove confirmation shows: the root it asks about, kept while the
    /// closed dialog fades out. Empty before the first question.
    pub fn remove_dialog_path(&self) -> String {
        self.remove_snapshot
            .clone()
            .update(false, String::new)
            .clone()
    }

    /// The focus handle of a theme card.
    pub fn theme_focus(&self, choice: ThemeName) -> &FocusHandle {
        &self.theme_focus[theme_index(choice)]
    }

    /// The focus handle of the language field.
    pub fn language_focus(&self) -> &FocusHandle {
        &self.language_focus
    }

    /// The focus handle of a legal text's summary.
    pub fn legal_focus(&self, text: LegalText) -> &FocusHandle {
        &self.legal_focus[text as usize]
    }

    /// Whether a legal text is unfolded.
    pub fn legal_open(&self, text: LegalText) -> bool {
        self.legal_open[text as usize]
    }

    /// Whether the root list shows every root.
    pub fn roots_expanded(&self) -> bool {
        self.roots_expanded
    }

    /// The section the navigation marks now.
    pub fn active_section(&self) -> SettingsSection {
        let offset = f32::from(self.scroll.offset().y);
        if let Some((section, at)) = self.clicked {
            if at == offset {
                return section;
            }
        }
        let tops: Vec<f32> = (0..SettingsSection::ALL.len())
            .map_while(|index| self.scroll.bounds_for_item(index))
            .map(|bounds| f32::from(bounds.top()) + offset)
            .collect();
        if tops.len() < SettingsSection::ALL.len() {
            return SettingsSection::default();
        }
        active_section(&tops, self.metrics())
    }

    fn metrics(&self) -> ScrollMetrics {
        ScrollMetrics {
            offset: -f32::from(self.scroll.offset().y),
            max_offset: f32::from(self.scroll.max_offset().y).max(0.0),
        }
    }

    /// A navigation link activated by pointer or by keyboard (`onClick` of
    /// `SettingsNavigation.tsx`): the focus moves to the link's section, which is then
    /// scrolled to and marked ([`Self::navigate`]).
    pub fn activate_section(
        &mut self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.section_focus[section.index()], cx);
        self.navigate(section, cx);
    }

    /// The scrolling of an activated navigation link: its section to the top of the
    /// page, below the header, and marked at once (`scrollIntoView({ block: "start" })`
    /// with the section's scroll margin).
    pub fn navigate(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        if let Some(bounds) = self.scroll.bounds_for_item(section.index()) {
            let offset = f32::from(self.scroll.offset().y);
            let top = f32::from(bounds.top()) + offset;
            let viewport_top = f32::from(self.scroll.bounds().top());
            let target = scroll_target(top, viewport_top, self.metrics());
            self.scroll.set_offset(point(px(0.), px(-target)));
            self.clicked = Some((section, -target));
        } else {
            self.clicked = Some((section, f32::from(self.scroll.offset().y)));
        }
        cx.notify();
    }

    fn command(&mut self, command: SettingsCommand, cx: &mut Context<Self>) {
        self.runtime.update(cx, |runtime, cx| {
            runtime.send(RuntimeInput::settings(command), cx)
        });
    }

    /// Follows the model's confirmations with the two dialogs.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.runtime.read(cx).model().settings();
        let locked = settings.is_locked();
        let remove_open = settings.remove_root_confirm().is_some();
        let clear_open = settings.clear_confirm_open();
        for (modal, open) in [
            (self.remove_modal.clone(), remove_open),
            (self.clear_modal.clone(), clear_open),
        ] {
            modal.update(cx, |modal, cx| {
                modal.set_close_on_escape(!locked, cx);
                modal.set_close_on_overlay_click(!locked);
                if modal.is_open() != open {
                    modal.set_open(open, window, cx);
                }
            });
        }
        cx.notify();
    }

    fn toggle_language_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(layer) = self.language_layer.take() {
            layers::release(layer, None, window, cx);
        } else {
            let current = self.runtime.read(cx).i18n().language();
            self.language_highlight = language_index(current);
            let view = cx.entity().downgrade();
            self.language_layer = Some(layers::register(
                LayerOptions::nonmodal(move |window, cx| {
                    view.update(cx, |view, cx| view.close_language_list(window, cx))
                        .ok();
                }),
                window,
                cx,
            ));
        }
        cx.notify();
    }

    fn close_language_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(layer) = self.language_layer.take() {
            layers::release(layer, None, window, cx);
            cx.notify();
        }
    }

    /// A press outside the open list closes it and remembers where, for the field.
    fn press_out_of_language_list(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.language_layer.is_some() {
            self.language_closed_at = Some(event.position);
        }
        self.close_language_list(window, cx);
    }

    /// A press on the language field: the list's own press-out handler runs first (capture
    /// phase), so a press it closed the list with is the one recorded there.
    fn press_language_field(&mut self, event: &MouseDownEvent) {
        self.press_closed_language_list = self.language_closed_at.take() == Some(event.position);
    }

    /// A click on the language field: Enter or Space while the list is open picks the
    /// marked language, as in a `<select>`; otherwise the list opens or closes. A pointer
    /// click whose press already closed the list leaves it closed.
    fn click_language_field(
        &mut self,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, ClickEvent::Mouse(_))
            && std::mem::take(&mut self.press_closed_language_list)
        {
            return;
        }
        if matches!(event, ClickEvent::Keyboard(_)) && self.language_layer.is_some() {
            self.choose_language(self.highlighted_language(), window, cx);
        } else {
            self.toggle_language_list(window, cx);
        }
    }

    /// An arrow, Home or End on the language field: the closed list opens on the current
    /// language, the open one marks the language the key asks for (no wrapping).
    fn step_language(
        &mut self,
        target: impl FnOnce(usize) -> usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.language_layer.is_none() {
            self.toggle_language_list(window, cx);
            return;
        }
        self.language_highlight = target(self.language_highlight).min(Language::ALL.len() - 1);
        cx.notify();
    }

    /// An arrow on a theme card: the focus and the selection move to the next or previous
    /// card, wrapping, as in a radio group.
    fn step_theme(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.runtime.read(cx).model().theme();
        let from = self
            .theme_focus
            .iter()
            .position(|handle| handle.is_focused(window))
            .unwrap_or_else(|| theme_index(current));
        let count = THEME_CHOICES.len();
        let target = if forward {
            (from + 1) % count
        } else {
            (from + count - 1) % count
        };
        window.focus(&self.theme_focus[target], cx);
        let choice = THEME_CHOICES[target];
        if choice != current {
            self.runtime.update(cx, |runtime, cx| {
                runtime.send(RuntimeInput::SetTheme(choice), cx)
            });
        }
        cx.notify();
    }

    fn choose_language(&mut self, language: Language, window: &mut Window, cx: &mut Context<Self>) {
        self.close_language_list(window, cx);
        self.runtime.update(cx, |runtime, cx| {
            runtime.send(RuntimeInput::SetLanguage(language), cx)
        });
    }
}

fn theme_index(choice: ThemeName) -> usize {
    THEME_CHOICES
        .iter()
        .position(|theme| *theme == choice)
        .unwrap_or(0)
}

fn language_index(language: Language) -> usize {
    Language::ALL
        .iter()
        .position(|candidate| *candidate == language)
        .unwrap_or(0)
}

/// The chevron of the language select: `m6 9 6 6 6-6` on a 24-unit box.
fn chevron(theme: ThemeName) -> impl IntoElement {
    let stroke = color(Rgba::hex(match theme {
        ThemeName::Light => layout::select::CHEVRON_LIGHT,
        ThemeName::Dark => layout::select::CHEVRON_DARK,
    }));
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            let unit = f32::from(bounds.size.width) / 24.0;
            let at = |x: f32, y: f32| bounds.origin + point(px(x * unit), px(y * unit));
            let mut path = PathBuilder::stroke(px(layout::select::CHEVRON_STROKE * unit));
            path.move_to(at(6.0, 9.0));
            path.line_to(at(12.0, 15.0));
            path.line_to(at(18.0, 9.0));
            if let Ok(path) = path.build() {
                window.paint_path(path, stroke);
            }
        },
    )
    .size(px(layout::select::CHEVRON_SIZE))
}

/// The disclosure marker of a folding text's `<summary>`: a filled triangle pointing
/// right while closed and down while open, in the text colour.
fn disclosure_marker(open: bool, fill: gpui::Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            let side = layout::about::MARKER_TRIANGLE;
            let height = layout::about::MARKER_HEIGHT;
            let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));
            let mut path = PathBuilder::fill();
            if open {
                let top = (height - side) / 2.0 + 1.0;
                path.move_to(at(0.0, top));
                path.line_to(at(height, top));
                path.line_to(at(height / 2.0, top + side - 1.0));
            } else {
                path.move_to(at(1.0, 0.0));
                path.line_to(at(1.0 + side, height / 2.0));
                path.line_to(at(1.0, height));
            }
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, fill);
            }
        },
    )
    .flex_none()
    .w(px(layout::about::MARKER_WIDTH))
    .h(px(layout::about::MARKER_HEIGHT))
}

/// A section card: `grid gap-5 rounded border bg-[var(--surface-solid)] p-5`.
fn card(theme: ThemeName) -> gpui::Div {
    let colors = &theme.tokens().colors;
    div()
        .flex()
        .flex_col()
        .gap(px(layout::card::GAP))
        .rounded(px(RADII.surface))
        .border_1()
        .border_color(color(colors.border_soft))
        .bg(color(colors.surface_solid))
        .p(px(layout::card::PADDING))
        .shadow(box_shadows(theme.tokens().shadows.surface))
}

fn heading(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(layout::card::HEADING_SIZE))
        .line_height(px(layout::card::HEADING_LINE_HEIGHT))
        .font_weight(weight(TYPOGRAPHY.heading_weight))
        .child(text.into())
}

fn description(theme: ThemeName, text: impl Into<SharedString>) -> gpui::Div {
    div()
        .mt(px(layout::card::DESCRIPTION_MARGIN_TOP))
        .text_size(px(layout::card::DESCRIPTION_SIZE))
        .line_height(relative(layout::card::DESCRIPTION_LINE_HEIGHT))
        .text_color(color(theme.tokens().colors.text_muted))
        .child(text.into())
}

/// `SectionOperationStatus`: the spinner while the operation runs, the line, and the
/// progress bar while progress is known.
fn status_block(
    id: &'static str,
    selector: &'static str,
    status: &SectionStatus,
    i18n: &I18n<'_>,
    theme: ThemeName,
) -> gpui::Div {
    let muted = color(theme.tokens().colors.text_muted);
    let text = status.message.resolve(i18n);
    let line = div()
        .text_size(px(layout::status::TEXT_SIZE))
        .line_height(px(TYPOGRAPHY.xs.line_height_px()))
        .text_color(muted);
    let line = if status.loading {
        line.id((id, 2usize))
            .debug_selector(move || selector.to_owned())
            .role(Role::Status)
            .flex()
            .items_center()
            .gap(px(layout::status::LOADER_GAP))
            .child(
                Spinner::new((id, 0usize), SpinnerSize::ExtraSmall)
                    .color(muted)
                    .theme(theme),
            )
            .child(text)
            .into_any_element()
    } else if text.is_empty() {
        // An empty line takes no room, as an empty block does on the Tauri page.
        div().into_any_element()
    } else {
        line.child(text).into_any_element()
    };
    div()
        .flex()
        .flex_col()
        .gap(px(layout::status::GAP))
        .child(line)
        .when_some(status.progress.as_ref(), |block, progress| {
            let (value, max) = progress_bar(progress);
            let value = match value {
                Some(value) => ProgressValue::Determinate {
                    value: value as f64,
                    max: max as f64,
                },
                None => ProgressValue::Indeterminate,
            };
            block.child(
                ProgressBar::new((id, 1usize), value)
                    .label(progress_label(progress))
                    .theme(theme),
            )
        })
}

impl SettingsView {
    fn send_on_click(
        &self,
        input: impl Fn() -> RuntimeInput + 'static,
    ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
        let runtime = self.runtime.downgrade();
        move |_, _, cx| runtime::send(&runtime, input(), cx)
    }

    fn nav(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = theme.tokens();
        let colors = &tokens.colors;
        let active = self.active_section();
        let keyboard = window.last_input_was_keyboard();
        let links = SettingsSection::ALL.map(|section| {
            let current = section == active;
            let handle = &self.nav_focus[section.index()];
            let focus_visible = keyboard && handle.is_focused(window);
            let link = div()
                .id(nav_link_id(section))
                .debug_selector(move || nav_link_id(section).to_string())
                .role(Role::Link)
                .track_focus(handle)
                .relative()
                .overflow_hidden()
                .flex()
                .items_center()
                .gap(px(layout::nav::LINK_GAP))
                .px(px(layout::nav::LINK_PADDING_X))
                .py(px(layout::nav::LINK_PADDING_Y))
                .rounded(px(RADII.control))
                .text_size(px(layout::nav::LINK_FONT_SIZE))
                .cursor_pointer()
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.activate_section(section, window, cx)
                }))
                .child(Icon::new(section.icon()).size(layout::nav::ICON_SIZE))
                .child(i18n.t(section.label_key()))
                // The link clips its children, so the ring lies inside its edge.
                .when(focus_visible, |link| {
                    let ring = FocusRing::base(tokens);
                    link.child(focus_ring(
                        FocusRing {
                            offset: -ring.width,
                            ..ring
                        },
                        RADII.control,
                    ))
                });
            if current {
                link.aria_selected(true)
                    .bg(color(colors.tagator_suggestion_active_bg))
                    .text_color(color(colors.primary_text))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(layout::nav::ACTIVE_EDGE))
                            .bg(color(colors.primary)),
                    )
            } else {
                let hover_bg = color(colors.surface_raised);
                let hover_text = color(colors.base_content);
                link.text_color(color(colors.text_muted))
                    .hover(move |style| style.bg(hover_bg).text_color(hover_text))
            }
        });
        div()
            .id(NAV_SELECTOR)
            .debug_selector(|| NAV_SELECTOR.to_owned())
            .aria_label(SharedString::from(i18n.t("workspace.settingsNavigation")))
            .absolute()
            .top(px(layout::nav::STICKY_TOP))
            .left(px(layout::PAGE_PADDING_X))
            .w(px(layout::NAV_WIDTH))
            .flex()
            .flex_col()
            .gap(px(layout::nav::GAP))
            .p(px(layout::nav::PADDING))
            .border_1()
            .border_color(color(colors.border_soft))
            .rounded(px(RADII.panel))
            .bg(color(colors.surface_solid))
            .children(links)
            .into_any_element()
    }

    fn scan_section(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = theme.tokens();
        let colors = &tokens.colors;
        let model = self.runtime.read(cx).model();
        let settings = model.settings();
        let locked = settings.is_locked();
        let roots = model.scan_roots();
        let highlighted = model.highlight_scan();
        let muted = color(colors.text_muted);

        let header = div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .max_w(px(layout::card::SCAN_TEXT_MAX_WIDTH))
                    .child(heading(i18n.t("settings.scan.heading")))
                    .child(description(theme, i18n.t("settings.scan.description"))),
            )
            .child(
                Button::new(CHOOSE_FOLDER_ID, i18n.t("settings.scan.chooseFolder"))
                    .debug_selector(CHOOSE_FOLDER_ID)
                    .variant(ButtonVariant::Primary)
                    .disabled(locked)
                    .on_click(
                        self.send_on_click(|| RuntimeInput::settings(SettingsCommand::PickFolder)),
                    ),
            );

        let rows: Vec<AnyElement> = visible_roots(roots, self.roots_expanded)
            .iter()
            .enumerate()
            .map(|(index, root)| {
                let path = root.path.clone();
                let enabled = root.auto_scan_on_startup;
                let runtime = self.runtime.downgrade();
                let switch_path = path.clone();
                let rescan_path = path.clone();
                let remove_path = path.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(layout::roots::ROW_GAP))
                    .rounded(px(RADII.control))
                    .border_1()
                    .border_color(color(colors.border_soft))
                    .bg(color(
                        colors
                            .base_200
                            .with_alpha_percent(layout::roots::ROW_FILL_PERCENT),
                    ))
                    .p(px(layout::roots::ROW_PADDING))
                    .child(
                        div()
                            .flex_none()
                            .size(px(layout::roots::TILE_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(RADII.control))
                            .bg(color(
                                colors
                                    .primary
                                    .with_alpha_percent(layout::roots::TILE_FILL_PERCENT),
                            ))
                            .child(
                                Icon::new(IconName::Folder)
                                    .size(layout::roots::ICON_SIZE)
                                    .color(color(colors.primary_text)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(fonts::MONO)
                            .text_size(px(layout::roots::PATH_SIZE))
                            .text_color(muted)
                            .child(path.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(layout::roots::ACTIONS_GAP))
                            .child(
                                Switch::new(
                                    root_switch_id(index),
                                    enabled,
                                    i18n.t_with(
                                        "settings.scan.autoScanLabel",
                                        &media_tagger_client::i18n::Params::new()
                                            .with("path", &path),
                                    ),
                                )
                                .text(i18n.t("settings.scan.autoScan"))
                                .debug_selector(root_switch_id(index))
                                .disabled(locked)
                                .theme(theme)
                                .on_change(
                                    move |enabled, _, cx| {
                                        runtime::send(
                                            &runtime,
                                            RuntimeInput::settings(SettingsCommand::SetAutoScan {
                                                path: switch_path.clone(),
                                                enabled,
                                            }),
                                            cx,
                                        )
                                    },
                                ),
                            )
                            .child(
                                Button::new(root_rescan_id(index), i18n.t("settings.scan.rescan"))
                                    .debug_selector(root_rescan_id(index))
                                    .variant(ButtonVariant::Ghost)
                                    .disabled(locked)
                                    .on_click(self.send_on_click(move || {
                                        RuntimeInput::settings(SettingsCommand::RescanRoot(
                                            rescan_path.clone(),
                                        ))
                                    })),
                            )
                            .child(
                                Button::new(root_remove_id(index), i18n.t("settings.scan.remove"))
                                    .debug_selector(root_remove_id(index))
                                    .variant(ButtonVariant::Danger)
                                    .disabled(locked)
                                    .on_click(self.send_on_click(move || {
                                        RuntimeInput::settings(SettingsCommand::RequestRemoveRoot(
                                            remove_path.clone(),
                                        ))
                                    })),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

        let list = if roots.is_empty() {
            div()
                .p(px(layout::roots::EMPTY_PADDING))
                .text_color(muted)
                .child(i18n.t("settings.scan.noPaths"))
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(layout::roots::LIST_GAP))
                .children(rows)
        };
        let roots_block = div()
            .flex()
            .flex_col()
            .gap(px(layout::roots::BLOCK_GAP))
            .border_t_1()
            .border_color(color(colors.border_soft))
            .pt(px(layout::roots::BLOCK_PADDING_TOP))
            .child(
                div()
                    .text_size(px(layout::roots::HEADING_SIZE))
                    .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                    .font_weight(weight(TYPOGRAPHY.weight_semibold))
                    .text_color(muted)
                    .child(i18n.t("settings.scan.attachedPaths")),
            )
            .child(list)
            .when(has_expand_toggle(roots), |block| {
                block.child(
                    div().flex().justify_center().child(
                        Button::new(
                            EXPAND_ROOTS_ID,
                            i18n.t(expand_toggle_key(self.roots_expanded)),
                        )
                        .debug_selector(EXPAND_ROOTS_ID)
                        .variant(ButtonVariant::Ghost)
                        .pressed(self.roots_expanded)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.roots_expanded = !view.roots_expanded;
                            cx.notify();
                        })),
                    ),
                )
            });

        let operation = |text: String| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(layout::operation::GAP))
                .p(px(layout::operation::PADDING))
                .rounded(px(RADII.control))
                .bg(color(colors.surface_muted))
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(layout::operation::TEXT_SIZE))
                        .line_height(relative(layout::operation::TEXT_LINE_HEIGHT))
                        .text_color(muted)
                        .child(text),
                )
        };
        let rescan = operation(i18n.t("settings.scan.rescanDescription")).child(
            Button::new(RESCAN_ALL_ID, i18n.t("settings.scan.rescanAll"))
                .debug_selector(RESCAN_ALL_ID)
                .variant(ButtonVariant::Secondary)
                .disabled(roots.is_empty() || locked)
                .on_click(
                    self.send_on_click(|| RuntimeInput::settings(SettingsCommand::RescanAll)),
                ),
        );
        let thumbnails = operation(i18n.t("settings.scan.thumbnailsDescription")).child(
            div()
                .flex()
                .flex_none()
                .flex_wrap()
                .gap(px(layout::operation::ACTIONS_GAP))
                .child(
                    Button::new(RENDER_ALL_ID, i18n.t("settings.scan.renderAllThumbnails"))
                        .debug_selector(RENDER_ALL_ID)
                        .disabled(locked)
                        .on_click(self.send_on_click(|| {
                            RuntimeInput::settings(SettingsCommand::RenderAllThumbnails)
                        })),
                )
                .child(
                    Button::new(
                        RENDER_FAILED_ID,
                        i18n.t("settings.scan.retryFailedThumbnails"),
                    )
                    .debug_selector(RENDER_FAILED_ID)
                    .disabled(locked)
                    .on_click(self.send_on_click(|| {
                        RuntimeInput::settings(SettingsCommand::RenderFailedThumbnails)
                    })),
                )
                .when(settings.thumbnail_render_running(), |actions| {
                    actions.child(
                        Button::new(STOP_RENDER_ID, i18n.t("settings.scan.stopThumbnailRender"))
                            .debug_selector(STOP_RENDER_ID)
                            .variant(ButtonVariant::Danger)
                            .disabled(settings.cancel_thumbnail_pending())
                            .on_click(self.send_on_click(|| {
                                RuntimeInput::settings(SettingsCommand::CancelThumbnailRender)
                            })),
                    )
                }),
        );
        let video_tools = settings.video_tools().map(|tools| {
            div()
                .debug_selector(|| VIDEO_TOOLS_SELECTOR.to_owned())
                .flex()
                .flex_wrap()
                .gap(px(layout::VIDEO_TOOLS_GAP))
                .text_size(px(TYPOGRAPHY.xs.size))
                .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                .text_color(muted)
                .child(video_tool_line(i18n, "ffmpeg", tools.ffmpeg))
                .child(video_tool_line(i18n, "ffprobe", tools.ffprobe))
        });

        let mut section = card(theme)
            .debug_selector(|| SCAN_SECTION_SELECTOR.to_owned())
            .child(header)
            .child(roots_block)
            .child(rescan)
            .child(thumbnails)
            .children(video_tools)
            .child(status_block(
                "settings-scan-status",
                SCAN_LOADER_SELECTOR,
                settings.status(OperationSection::Scan),
                i18n,
                theme,
            ));
        if highlighted {
            let mut shadows = vec![gpui::BoxShadow {
                color: color(
                    colors
                        .primary
                        .with_alpha_percent(layout::card::HIGHLIGHT_RING_PERCENT),
                ),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(layout::card::HIGHLIGHT_RING),
                inset: false,
            }];
            shadows.extend(box_shadows(tokens.shadows.surface));
            section = section
                .border_color(color(
                    colors
                        .primary
                        .with_alpha_percent(layout::card::HIGHLIGHT_BORDER_PERCENT),
                ))
                .shadow(shadows);
        }
        section.into_any_element()
    }

    fn appearance_section(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = theme.tokens();
        let colors = &tokens.colors;
        // The chosen theme is the active one.
        let current = theme;
        let keyboard = window.last_input_was_keyboard();
        let cards = THEME_CHOICES.map(|choice| {
            let checked = choice == current;
            // The checked card is the group's one tab stop.
            let handle = self.theme_focus[theme_index(choice)]
                .clone()
                .tab_stop(checked);
            let focus_visible = keyboard && handle.is_focused(window);
            let [workspace, panel, tile, border] = match choice {
                ThemeName::Light => layout::theme_choice::LIGHT,
                ThemeName::Dark => layout::theme_choice::DARK,
            }
            .map(|hex| color(Rgba::hex(hex)));
            let stripes = (0..3).map(|stripe| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(relative(
                        stripe as f32 * layout::theme_choice::PREVIEW_TILE_STEP,
                    ))
                    .w(relative(layout::theme_choice::PREVIEW_TILE_FRACTION))
                    .bg(tile)
            });
            let preview = div()
                .h(px(layout::theme_choice::PREVIEW_HEIGHT))
                .flex()
                .flex_col()
                .gap(px(layout::theme_choice::PREVIEW_GAP))
                .overflow_hidden()
                .border_1()
                .border_color(border)
                .rounded(px(layout::theme_choice::PREVIEW_RADIUS))
                .bg(workspace)
                .child(
                    div()
                        .flex_none()
                        .h(px(layout::theme_choice::PREVIEW_BAR))
                        .bg(panel)
                        .border_b_1()
                        .border_color(border),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .gap(px(layout::theme_choice::PREVIEW_GAP))
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .ml(px(layout::theme_choice::PREVIEW_GAP))
                                .mb(px(layout::theme_choice::PREVIEW_GAP))
                                .rounded(px(layout::theme_choice::PREVIEW_TILE_RADIUS))
                                .overflow_hidden()
                                .children(stripes),
                        )
                        .child(
                            div()
                                .w(relative(layout::theme_choice::PREVIEW_PANEL_FRACTION))
                                .bg(panel),
                        ),
                );
            let runtime = self.runtime.downgrade();
            let card = div()
                .id(theme_choice_id(choice))
                .debug_selector(move || theme_choice_id(choice).to_string())
                .role(Role::RadioButton)
                .aria_toggled(if checked {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .track_focus(&handle)
                .relative()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(layout::theme_choice::CARD_GAP))
                .p(px(layout::theme_choice::CARD_PADDING))
                .border_1()
                .rounded(px(RADII.surface))
                // GPUI fills a shadow under the whole box: the card's own fill keeps the
                // selection ring a ring.
                .bg(color(colors.surface_solid))
                .text_size(px(layout::theme_choice::CARD_FONT_SIZE))
                .font_weight(weight(TYPOGRAPHY.weight_semibold))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    runtime::send(&runtime, RuntimeInput::SetTheme(choice), cx)
                })
                .child(preview)
                .child(i18n.t(match choice {
                    ThemeName::Light => "settings.appearance.theme.light",
                    ThemeName::Dark => "settings.appearance.theme.dark",
                }))
                .when(focus_visible, |card| {
                    card.child(focus_ring(FocusRing::base(tokens), RADII.surface))
                });
            let hover = color(colors.surface_raised);
            let card = card.hover(move |style| style.bg(hover));
            if checked {
                card.border_color(color(colors.primary_text))
                    .text_color(color(colors.primary_text))
                    .shadow(vec![gpui::BoxShadow {
                        color: color(colors.primary),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(1.),
                        inset: false,
                    }])
            } else {
                card.border_color(color(colors.border_soft))
            }
        });
        card(theme)
            .child(
                div()
                    .child(heading(i18n.t("settings.appearance.heading")))
                    .child(description(
                        theme,
                        i18n.t("settings.appearance.description"),
                    )),
            )
            .child(
                div()
                    .id("settings-theme-choices")
                    .max_w(px(layout::theme_choice::MAX_WIDTH))
                    .aria_label(SharedString::from(
                        i18n.t("settings.appearance.theme.ariaLabel"),
                    ))
                    .role(Role::RadioGroup)
                    .key_context(THEME_KEY_CONTEXT)
                    .on_action(cx.listener(|view, _: &NextTheme, window, cx| {
                        view.step_theme(true, window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &PreviousTheme, window, cx| {
                        view.step_theme(false, window, cx)
                    }))
                    .child(
                        div()
                            .mb(px(layout::theme_choice::LEGEND_MARGIN_BOTTOM))
                            .text_size(px(layout::theme_choice::LEGEND_SIZE))
                            .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                            .font_weight(weight(TYPOGRAPHY.weight_semibold))
                            .text_color(color(colors.text_muted))
                            .child(i18n.t("settings.appearance.theme.label")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(layout::theme_choice::GAP))
                            .children(cards),
                    ),
            )
            .into_any_element()
    }

    fn language_section(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = theme.tokens();
        let colors = &tokens.colors;
        let current = i18n.language();
        let open = self.language_layer.is_some();
        let focus_visible =
            window.last_input_was_keyboard() && self.language_focus.is_focused(window);
        let field = div()
            .id(LANGUAGE_SELECT_ID)
            .debug_selector(|| LANGUAGE_SELECT_ID.to_owned())
            .role(Role::ComboBox)
            .aria_label(SharedString::from(i18n.t("settings.language.ariaLabel")))
            .aria_expanded(open)
            .track_focus(&self.language_focus)
            .key_context(LANGUAGE_KEY_CONTEXT)
            .on_action(cx.listener(|view, _: &NextLanguage, window, cx| {
                view.step_language(|index| index + 1, window, cx)
            }))
            .on_action(cx.listener(|view, _: &PreviousLanguage, window, cx| {
                view.step_language(|index| index.saturating_sub(1), window, cx)
            }))
            .on_action(cx.listener(|view, _: &FirstLanguage, window, cx| {
                view.step_language(|_| 0, window, cx)
            }))
            .on_action(cx.listener(|view, _: &LastLanguage, window, cx| {
                view.step_language(|_| Language::ALL.len() - 1, window, cx)
            }))
            .relative()
            .h(px(layout::select::HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .pl(px(layout::select::PADDING_X))
            .pr(px(layout::select::PADDING_RIGHT))
            .rounded(px(RADII.control))
            .bg(color(colors.surface_raised))
            .shadow(box_shadows(if open || focus_visible {
                tokens.shadows.field_focus
            } else {
                tokens.shadows.field
            }))
            .text_size(px(layout::select::FONT_SIZE))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event, _, _| view.press_language_field(event)),
            )
            .on_click(
                cx.listener(|view, event, window, cx| view.click_language_field(event, window, cx)),
            )
            .child(current.native_label())
            .child(
                div()
                    .absolute()
                    .right(px(layout::select::CHEVRON_RIGHT))
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(chevron(theme)),
            );
        let list = open.then(|| {
            let highlighted = self.highlighted_language();
            let options = Language::ALL.map(|language| {
                let selected = language == current;
                // The language Enter picks, marked as the pointer marks one.
                let marked = !selected && language == highlighted;
                let hover = color(colors.surface_raised);
                div()
                    .id(language_option_id(language))
                    .debug_selector(move || language_option_id(language).to_string())
                    .role(Role::ListBoxOption)
                    .aria_selected(selected)
                    .h(px(layout::select::OPTION_HEIGHT))
                    .flex()
                    .items_center()
                    .px(px(layout::select::PADDING_X))
                    .text_size(px(layout::select::FONT_SIZE))
                    .cursor_pointer()
                    .when(selected, |option| {
                        option
                            .bg(color(colors.tagator_suggestion_active_bg))
                            .text_color(color(colors.primary_text))
                    })
                    .when(marked, |option| option.bg(hover))
                    .when(!selected, |option| {
                        option.hover(move |style| style.bg(hover))
                    })
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.choose_language(language, window, cx)
                    }))
                    .child(language.native_label())
            });
            deferred(
                anchored().snap_to_window().child(
                    div()
                        .id("settings-language-list")
                        .role(Role::ListBox)
                        .occlude()
                        .w(px(layout::select::MAX_WIDTH))
                        .mt(px(4.0))
                        .py(px(4.0))
                        .rounded(px(RADII.control))
                        .border_1()
                        .border_color(color(colors.border_soft))
                        .bg(color(colors.surface_solid))
                        .shadow(box_shadows(tokens.shadows.popover))
                        .on_mouse_down_out(cx.listener(|view, event, window, cx| {
                            view.press_out_of_language_list(event, window, cx)
                        }))
                        .children(options),
                ),
            )
            .with_priority(1)
        });
        card(theme)
            .child(
                div()
                    .child(heading(i18n.t("settings.language.heading")))
                    .child(description(theme, i18n.t("settings.language.description"))),
            )
            .child(
                div()
                    .max_w(px(layout::select::MAX_WIDTH))
                    .child(field)
                    .children(list),
            )
            .into_any_element()
    }

    fn danger_section(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.tokens().colors;
        let settings = self.runtime.read(cx).model().settings();
        let locked = settings.is_locked();
        div()
            .flex()
            .flex_col()
            .gap(px(layout::danger::GAP))
            .rounded(px(RADII.surface))
            .border_1()
            .border_color(color(
                colors
                    .error
                    .with_alpha_percent(layout::danger::BORDER_PERCENT),
            ))
            .bg(color(
                colors
                    .error
                    .with_alpha_percent(layout::danger::FILL_PERCENT),
            ))
            .p(px(layout::card::PADDING))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(layout::danger::GAP))
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(layout::danger::HEADING_SIZE))
                                    .line_height(relative(1.5))
                                    .font_weight(weight(TYPOGRAPHY.heading_weight))
                                    .text_color(color(colors.error))
                                    .child(i18n.t("settings.danger.heading")),
                            )
                            .child(
                                div()
                                    .mt(px(layout::card::DESCRIPTION_MARGIN_TOP))
                                    .max_w(px(layout::danger::HINT_MAX_WIDTH))
                                    .text_size(px(layout::danger::HINT_SIZE))
                                    .line_height(relative(layout::card::DESCRIPTION_LINE_HEIGHT))
                                    .text_color(color(colors.text_muted))
                                    .child(i18n.t("settings.danger.hint")),
                            ),
                    )
                    .child(
                        Button::new(CLEAR_LIBRARY_ID, i18n.t("settings.danger.clearLibrary"))
                            .debug_selector(CLEAR_LIBRARY_ID)
                            .variant(ButtonVariant::Danger)
                            .disabled(locked)
                            .on_click(self.send_on_click(|| {
                                RuntimeInput::settings(SettingsCommand::OpenClearConfirm)
                            })),
                    ),
            )
            .child(status_block(
                "settings-danger-status",
                DANGER_LOADER_SELECTOR,
                settings.status(OperationSection::Danger),
                i18n,
                theme,
            ))
            .into_any_element()
    }

    fn about_section(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = theme.tokens();
        let colors = &tokens.colors;
        let keyboard = window.last_input_was_keyboard();
        let muted = color(colors.text_muted);
        let info = about::info();
        // `grid-cols-[auto_1fr]`: a column of labels as wide as the widest, and the values
        // beside it; every row is one line.
        let rows = [
            ("settings.about.version", info.version.clone()),
            ("settings.about.publisher", info.publisher.clone()),
            ("settings.about.license", info.license.clone()),
        ];
        let column = || div().flex().flex_col().gap(px(layout::about::LIST_GAP_Y));
        let list = div()
            .flex()
            .gap(px(layout::about::LIST_GAP_X))
            .text_size(px(layout::about::TEXT_SIZE))
            .child(
                column()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_color(muted)
                    .children(rows.iter().map(|(label, _)| i18n.t(label))),
            )
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .children(rows.iter().map(|(_, value)| value.clone())),
            );
        let links = AboutLink::ALL.map(|link| {
            Button::new(about_link_id(link), i18n.t(link.label_key()))
                .debug_selector(about_link_id(link))
                .on_click(move |_, _, cx| cx.open_url(&link.url()))
        });
        let paragraph = |key: &str| {
            div()
                .text_size(px(layout::about::TEXT_SIZE))
                .line_height(relative(layout::card::DESCRIPTION_LINE_HEIGHT))
                .text_color(muted)
                .child(i18n.t(key))
        };
        let legal = LegalText::ALL.map(|text| {
            let index = text as usize;
            let open = self.legal_open[index];
            let handle = &self.legal_focus[index];
            let focus_visible = keyboard && handle.is_focused(window);
            div()
                .min_w_0()
                .rounded(px(RADII.control))
                .border_1()
                .border_color(color(colors.border_soft))
                .p(px(layout::about::DETAILS_PADDING))
                .child(
                    div()
                        .id(legal_summary_id(text))
                        .debug_selector(move || legal_summary_id(text).to_string())
                        .role(Role::Button)
                        .aria_expanded(open)
                        .track_focus(handle)
                        .relative()
                        .cursor_pointer()
                        .text_size(px(layout::about::TEXT_SIZE))
                        .font_weight(weight(TYPOGRAPHY.weight_semibold))
                        .flex()
                        .items_center()
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.legal_open[index] = !view.legal_open[index];
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex_none()
                                .debug_selector(move || legal_marker_selector(text, open))
                                .child(disclosure_marker(open, color(colors.base_content))),
                        )
                        .child(i18n.t(text.summary_key()))
                        .when(focus_visible, |summary| {
                            summary.child(focus_ring(FocusRing::base(tokens), RADII.control))
                        }),
                )
                .when(open, |details| {
                    details.child(
                        div()
                            .id((legal_summary_id(text), 1usize))
                            .debug_selector(move || legal_text_selector(text))
                            .mt(px(layout::about::DETAILS_TEXT_MARGIN_TOP))
                            .max_h(px(layout::about::DETAILS_TEXT_MAX_HEIGHT))
                            .overflow_y_scroll()
                            .text_size(px(layout::about::DETAILS_TEXT_SIZE))
                            .line_height(relative(layout::card::DESCRIPTION_LINE_HEIGHT))
                            .child(text.text()),
                    )
                })
        });
        card(theme)
            .child(
                div()
                    .child(heading(i18n.t("settings.about.heading")))
                    .child(
                        div()
                            .mt(px(layout::card::DESCRIPTION_MARGIN_TOP))
                            .text_size(px(layout::about::NAME_SIZE))
                            .line_height(px(TYPOGRAPHY.xl.line_height_px()))
                            .font_weight(weight(TYPOGRAPHY.weight_semibold))
                            .child(PRODUCT_NAME),
                    ),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(layout::about::LINKS_GAP))
                    .children(links),
            )
            .child(paragraph("settings.about.rights"))
            .child(paragraph("settings.about.fileSafety"))
            .children(legal)
            .into_any_element()
    }

    /// The remove-root and clear-library confirmations.
    fn dialogs(
        &mut self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &mut Context<Self>,
    ) -> [AnyElement; 2] {
        let colors = &theme.tokens().colors;
        let settings = self.runtime.read(cx).model().settings();
        let locked = settings.is_locked();
        let remove_open = settings.remove_root_confirm().is_some();
        let current = settings
            .remove_root_confirm()
            .unwrap_or_default()
            .to_owned();
        let path = self.remove_snapshot.update(remove_open, || current).clone();
        let text = |key: &str| {
            div()
                .text_size(px(layout::confirm::DESCRIPTION_SIZE))
                .line_height(relative(layout::confirm::DESCRIPTION_LINE_HEIGHT))
                .text_color(color(colors.text_muted))
                .child(i18n.t(key))
        };
        let dialog_heading = |key: &str| {
            div()
                .text_size(px(TYPOGRAPHY.h2_size))
                .line_height(px(TYPOGRAPHY.lg.line_height_px()))
                .font_weight(weight(TYPOGRAPHY.heading_weight))
                .child(i18n.t(key))
        };
        let remove = Modal::new("settings-remove-root-dialog", &self.remove_modal)
            .label(i18n.t("settings.dialogs.removeScanRoot.heading"))
            .size(ModalSize::Medium)
            .theme(theme)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(layout::confirm::GAP))
                    .child(dialog_heading("settings.dialogs.removeScanRoot.heading"))
                    .child(text("settings.dialogs.removeScanRoot.description"))
                    .child(
                        div()
                            .rounded(px(RADII.control))
                            .border_1()
                            .border_color(color(
                                colors
                                    .base_content
                                    .with_alpha_percent(layout::confirm::PATH_BORDER_PERCENT),
                            ))
                            .bg(color(
                                colors
                                    .base_200
                                    .with_alpha_percent(layout::confirm::PATH_FILL_PERCENT),
                            ))
                            .px(px(layout::confirm::PATH_PADDING_X))
                            .py(px(layout::confirm::PATH_PADDING_Y))
                            .text_size(px(layout::confirm::PATH_SIZE))
                            .text_color(color(colors.text_muted))
                            .child(path),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(layout::confirm::BUTTONS_GAP))
                            .child(
                                Button::new(REMOVE_CONFIRM_ID, i18n.t("common.remove"))
                                    .debug_selector(REMOVE_CONFIRM_ID)
                                    .variant(ButtonVariant::Danger)
                                    .disabled(locked)
                                    .on_click(self.send_on_click(|| {
                                        RuntimeInput::settings(SettingsCommand::ConfirmRemoveRoot)
                                    })),
                            )
                            .child(
                                Button::new(REMOVE_CANCEL_ID, i18n.t("common.cancel"))
                                    .debug_selector(REMOVE_CANCEL_ID)
                                    .disabled(locked)
                                    .on_click(self.send_on_click(|| {
                                        RuntimeInput::settings(SettingsCommand::CancelRemoveRoot)
                                    })),
                            ),
                    ),
            );
        let clear = Modal::new("settings-clear-library-dialog", &self.clear_modal)
            .label(i18n.t("settings.dialogs.clearLibrary.heading"))
            .size(ModalSize::Small)
            .theme(theme)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(layout::confirm::GAP))
                    .child(dialog_heading("settings.dialogs.clearLibrary.heading"))
                    .child(text("settings.dialogs.clearLibrary.description"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(layout::confirm::BUTTONS_GAP))
                            .child(
                                Button::new(CLEAR_CONFIRM_ID, i18n.t("common.yes"))
                                    .debug_selector(CLEAR_CONFIRM_ID)
                                    .variant(ButtonVariant::Danger)
                                    .disabled(locked)
                                    .on_click(self.send_on_click(|| {
                                        RuntimeInput::settings(SettingsCommand::ConfirmClear)
                                    })),
                            )
                            .child(
                                Button::new(CLEAR_CANCEL_ID, i18n.t("common.no"))
                                    .debug_selector(CLEAR_CANCEL_ID)
                                    .disabled(locked)
                                    .on_click(self.send_on_click(|| {
                                        RuntimeInput::settings(SettingsCommand::CloseClearConfirm)
                                    })),
                            ),
                    ),
            );
        [remove.into_any_element(), clear.into_any_element()]
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let colors = &theme.tokens().colors;
        let i18n = self.runtime.read(cx).i18n();
        let runtime = self.runtime.downgrade();
        let back = SharedString::from(i18n.t("common.back"));

        // A scenario's navigation click. The runtime holds it back until the launch's
        // work has ended; the section's place is read from the frame before, so the
        // click is made one frame after the page that work left was laid out.
        if self.runtime.read(cx).scripted_settings_section().is_some() {
            let laid_out = self
                .scroll
                .bounds_for_item(SettingsSection::ALL.len() - 1)
                .is_some();
            if self.scripted_page_laid_out && laid_out {
                let section = self
                    .runtime
                    .update(cx, |runtime, _| runtime.take_scripted_settings_section());
                if let Some(section) = section {
                    self.activate_section(section, window, cx);
                }
            } else {
                self.scripted_page_laid_out = true;
                window.request_animation_frame();
            }
        } else {
            self.scripted_page_laid_out = false;
        }

        let header = drag_surface(
            div()
                .debug_selector(|| HEADER_SELECTOR.to_owned())
                .flex_none()
                .bg(color(colors.window_chrome_bg))
                .border_b_1()
                .border_color(color(colors.border_soft))
                .px(px(settings_header::PADDING_X))
                .py(px(settings_header::PADDING_Y)),
            &self.drag,
        )
        .child(
            div()
                .flex()
                .w_full()
                .items_center()
                .gap(px(settings_header::GAP))
                .child(
                    no_drag(
                        div()
                            .id("settings-back-slot")
                            .debug_selector(|| BACK_ID.to_owned()),
                    )
                    .child(
                        Button::new(BACK_ID, back.clone())
                            .variant(ButtonVariant::Ghost)
                            .leading_icon(IconName::ArrowLeft, settings_header::BACK_ICON_SIZE)
                            .aria_label(back)
                            .on_click(move |_, _, cx| {
                                runtime::send(&runtime, RuntimeInput::CloseSettings, cx)
                            }),
                    ),
                )
                .child(logo(settings_header::LOGO_SIZE))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .border_l_1()
                        .border_color(color(colors.border_soft))
                        .pl(px(settings_header::HEADING_PADDING_LEFT))
                        .child(
                            div()
                                .debug_selector(|| HEADING_SELECTOR.to_owned())
                                .truncate()
                                .text_size(px(settings_header::HEADING_SIZE))
                                .line_height(relative(settings_header::HEADING_LINE_HEIGHT))
                                .font_weight(weight(TYPOGRAPHY.heading_weight))
                                .child(i18n.t("settings.page.heading")),
                        )
                        .child(
                            div()
                                .mt(px(settings_header::DESCRIPTION_MARGIN_TOP))
                                .text_size(px(TYPOGRAPHY.xs.size))
                                .text_color(color(colors.text_muted))
                                .child(i18n.t("settings.page.description")),
                        ),
                )
                .child(chrome::window_controls(
                    &i18n,
                    theme,
                    chrome::window_state(window),
                )),
        );

        let sections = [
            self.scan_section(&i18n, theme, cx),
            self.appearance_section(&i18n, theme, window, cx),
            self.language_section(&i18n, theme, window, cx),
            // The sections of epic E18: their anchors keep the page's order and the
            // navigation's targets.
            div().into_any_element(),
            div().into_any_element(),
            self.danger_section(&i18n, theme, cx),
            self.about_section(&i18n, theme, window, cx),
        ];
        let keyboard = window.last_input_was_keyboard();
        let section_ring = FocusRing::base(theme.tokens());
        let sections = SettingsSection::ALL
            .into_iter()
            .zip(sections)
            .map(|(section, content)| {
                let handle = &self.section_focus[section.index()];
                let focus_visible = keyboard && handle.is_focused(window);
                div()
                    .id(section_id(section))
                    .debug_selector(move || section_id(section).to_string())
                    .track_focus(handle)
                    .relative()
                    .min_w_0()
                    .child(content)
                    // `.settings-section` has no radius of its own: a square ring.
                    .when(focus_visible, |wrapper| {
                        wrapper.child(
                            focus_ring(section_ring, -section_ring.outset())
                                .debug_selector(|| SECTION_FOCUS_RING_SELECTOR.to_owned()),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let body = div()
            .id(BODY_SELECTOR)
            .debug_selector(|| BODY_SELECTOR.to_owned())
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(layout::SECTIONS_GAP))
            .pt(px(settings_header::BODY_PADDING_TOP))
            .pb(px(layout::PAGE_PADDING_BOTTOM + layout::VIEW_PADDING_BOTTOM))
            .pl(px(layout::PAGE_PADDING_X
                + layout::NAV_WIDTH
                + layout::LAYOUT_GAP))
            .pr(px(layout::PAGE_PADDING_X))
            // The navigation follows the scroll position.
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            .children(sections);

        let nav = self.nav(&i18n, theme, window, cx);
        let dialogs = self.dialogs(&i18n, theme, cx);
        div()
            .key_context(KeyContext::Settings.name())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            // Before the sections, as in the page's DOM: the tab order is the order the
            // elements are drawn in, header, navigation links, section controls. The
            // navigation is positioned on its own and nothing of the sections reaches
            // it, so the order changes no pixel.
            .child(nav)
            .child(body)
            // In a box of no size: the dialogs draw themselves over the window.
            .child(div().absolute().size_0().children(dialogs))
    }
}
