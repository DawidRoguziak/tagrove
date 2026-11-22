//! The gallery view (`AppGalleryView.tsx`): the workspace header and, below it, the
//! gallery: the virtual grid of tiles with the status footer, or the empty state
//! (`GalleryGrid.tsx`).
//!
//! The view is dropped when the settings page opens and made anew when it closes; it
//! keeps only what may be lost then (the slider entity, the drag gesture, the scroll
//! position). Everything else is read from the [`Runtime`], which also owns the
//! thumbnails.
//!
//! # The grid
//!
//! The grid does not use a GPUI scroll container: its scroll position is the logical
//! `f64` offset of [`VirtualGrid`], and only the rows in view (plus overscan) are
//! elements, each placed at `row top - offset`, a small number at any depth (ADR
//! 0042). Wheel and keys are inputs of that model; so is the measured size of the
//! viewport, which a `canvas` reports after layout. Rendering only reads: whatever
//! changes state (a scroll step, a measurement, the runtime's news, the idle timer)
//! goes through [`GalleryView::refresh`], which lays the grid out again, tells the
//! query model the viewport when it changed, tells the thumbnails the tiles in range
//! and asks for a redraw.
//!
//! # Selection mode
//!
//! The bulk-actions toggle of the tools row turns selection mode on and off. In it a
//! click on a tile toggles the tile instead of opening it, and a press that is held
//! and moved draws a rectangle (`selection.rs`). What is selected lives in the
//! runtime and survives the view; the gesture, its preview and the mode's layer in
//! the layer stack are the view's.
//!
//! While the mode is on, the bulk dock floats at the bottom centre of the viewport
//! (`bulk_dock.rs`, ADR 0035). It is made when the mode turns on and dropped when it
//! turns off or the view goes; a press or the wheel on it never reaches the grid.

mod grid;
mod position;
mod selection;

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{
    canvas, div, prelude::*, px, relative, AnyElement, ClickEvent, Context, Entity, EventEmitter,
    FocusHandle, MouseButton, MouseDownEvent, Pixels, ScrollEvent, SharedString, Size,
    Subscription, Task, Window,
};
use media_tagger_client::{
    controls::{ButtonVariant, SliderRange},
    gallery::{
        layout::{self as gallery_layout, area, group_plates, media_group, thumbnail_key},
        scrollbar_visible, GridEffect, GridGeometry, GridInput, LibraryRange, ScrollEdge,
        ScrollbarDrag, SelectionGesture, SelectionInput, TileRequest, TimelineEffect,
        TimelineInput, TimelineScrubber, VirtualGrid,
    },
    i18n::I18n,
    library::{QueryInput, Viewport},
    model::Model as _,
    overlay::LayerId,
    runtime::{RuntimeInput, TILE_SIZE_MAX, TILE_SIZE_MIN, TILE_SIZE_STEP},
    shell::{
        count_chip, empty_state, identity, item_count, search_row, tools_row, EmptyState,
        PRODUCT_NAME,
    },
    shortcuts::KeyContext,
    theme::{ThemeName, RADII, TYPOGRAPHY},
    window_chrome::DragSurface,
};

pub use grid::{
    chip_selector, group_plate_selector, init, key_bindings, loader_selector, picture_selector,
    placeholder_selector, selection_mark_selector, tile_selector, GalleryEvent, LineDown, LineUp,
    PageDown, PageUp, ScrollEnd, ScrollStart, SelectAll, FOOTER_SELECTOR, LOAD_ERROR_SELECTOR,
    LOAD_RETRY_ID, SELECTION_RECTANGLE_SELECTOR, VIEWPORT_SELECTOR,
};
pub use position::{
    timeline_tick_selector, SCROLLBAR_SELECTOR, SCROLLBAR_THUMB_SELECTOR, TIMELINE_LABEL_SELECTOR,
    TIMELINE_MARKER_SELECTOR, TIMELINE_SELECTOR,
};
pub use selection::BULK_TOGGLE_ID;

use super::{
    bulk_dock::BulkDock,
    chrome::{self, drag_surface, no_drag},
    search::SearchBar,
};
use crate::{
    runtime::{self, Runtime},
    thumbs::Thumbnails,
    ui::{
        button::Button,
        fonts,
        icon::Icon,
        layers,
        logo::logo,
        slider::{Slider, SliderEvent},
        theme::{self, color, weight},
    },
};

/// Element id of the settings button (`#open-settings-button`).
pub const OPEN_SETTINGS_ID: &str = "open-settings-button";
/// Debug selectors (`VisualTestContext::debug_bounds`).
pub const HEADER_SELECTOR: &str = "workspace-header";
pub const SEARCH_SLOT_SELECTOR: &str = "workspace-search-slot";
/// The logo and product name: part of the header's drag surface.
pub const IDENTITY_SELECTOR: &str = "workspace-identity";
pub const COUNT_SELECTOR: &str = "workspace-count";
pub const EMPTY_STATE_SELECTOR: &str = "gallery-empty-state";
pub const ADD_FIRST_FOLDER_ID: &str = "gallery-add-first-folder";

/// What the thumbnails were last told: the cache version of the rows, the range and
/// the pause flag. The tiles are collected again only when one of them changed.
type DemandStamp = (u64, Option<LibraryRange>, bool);

pub struct GalleryView {
    runtime: Entity<Runtime>,
    thumbnails: Entity<Thumbnails>,
    /// The search field and its tools; made and dropped with the view.
    search: Entity<SearchBar>,
    settings_opener: FocusHandle,
    tile_size: Entity<Slider>,
    drag: Rc<Cell<DragSurface>>,
    /// The scroll state of the grid.
    grid: VirtualGrid,
    /// The size of the scrolling viewport as last laid out; `None` before the first
    /// frame.
    viewport_size: Option<Size<Pixels>>,
    /// When the view was made, on the executor's clock: scroll steps are timed from
    /// it, so tests time them with the test clock.
    started: Instant,
    /// Ends the fast-scroll pause; replaced by every step that keeps it paused.
    idle_timer: Option<Task<()>>,
    /// The viewport the query model was last told, with the page-failure epoch it
    /// was told at: a new epoch publishes the same range again.
    published: (Option<Viewport>, u64),
    demand: Option<DemandStamp>,
    /// The grid's focus: keys scroll while it, or anything else in the view, has it.
    grid_focus: FocusHandle,
    /// The position bar: its focus, its drag and where it was last laid out.
    scrollbar_focus: FocusHandle,
    scrollbar_drag: Option<ScrollbarDrag>,
    scrollbar_bounds: position::LastBounds,
    /// The month timeline: its state, its focus, where it was last laid out and
    /// whether the pointer is over it (the year labels show then).
    timeline: TimelineScrubber,
    timeline_focus: FocusHandle,
    timeline_bounds: position::LastBounds,
    timeline_hovered: bool,
    /// Selection mode: the pointer gesture on the grid, what it was last told of the
    /// runtime and the layout, its long-press timer and whether its next animation
    /// frame is scheduled.
    gesture: SelectionGesture,
    gesture_stamp: selection::GestureStamp,
    long_press_timer: Option<Task<()>>,
    gesture_frame_pending: bool,
    /// Where the viewport was last laid out in the window: pointer positions of a
    /// gesture are relative to it.
    viewport_origin: Rc<Cell<Option<gpui::Point<Pixels>>>>,
    /// Set by a primary press on a control inside the viewport (the banner's Retry),
    /// which starts no gesture.
    control_pressed: Rc<Cell<bool>>,
    /// The layer of selection mode in the window's layer stack, while the mode is on.
    selection_layer: Option<LayerId>,
    /// The bulk dock, while selection mode is on.
    dock: Option<Entity<BulkDock>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<GalleryEvent> for GalleryView {}

impl GalleryView {
    /// `settings_opener` is the settings button's focus handle; the shell keeps it, so
    /// the focus can go back to the button of the next gallery view.
    pub fn new(
        runtime: Entity<Runtime>,
        settings_opener: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (size, label) = {
            let runtime = runtime.read(cx);
            (
                runtime.model().tile_size(),
                runtime.i18n().t("controls.tileSize"),
            )
        };
        let tile_size = cx.new(|cx| {
            Slider::new(
                SliderRange::with_step(
                    f64::from(TILE_SIZE_MIN),
                    f64::from(TILE_SIZE_MAX),
                    f64::from(TILE_SIZE_STEP),
                ),
                f64::from(size),
                label,
                cx,
            )
        });
        let thumbnails = runtime.read(cx).thumbnails().clone();
        let search = cx.new(|cx| SearchBar::new(runtime.clone(), window, cx));
        let subscriptions = vec![
            cx.subscribe(&tile_size, |this, _, event, cx| {
                let SliderEvent::Input(value) = *event;
                this.runtime.update(cx, |runtime, cx| {
                    runtime.send(RuntimeInput::SetTileSize(value), cx)
                });
            }),
            cx.observe_in(&runtime, window, |this, runtime, window, cx| {
                let (size, label) = {
                    let runtime = runtime.read(cx);
                    (
                        runtime.model().tile_size(),
                        runtime.i18n().t("controls.tileSize"),
                    )
                };
                this.tile_size.update(cx, |slider, cx| {
                    slider.set_value(f64::from(size), cx);
                    slider.set_label(label, cx);
                });
                this.sync_selection_layer(window, cx);
                this.sync_dock(window, cx);
                this.refresh(cx);
            }),
            // A picture arrived or failed: the tiles are drawn again.
            cx.observe(&thumbnails, |_, _, cx| cx.notify()),
            cx.observe_window_bounds(window, |_, _, cx| cx.notify()),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.window_deactivated(cx);
                }
            }),
            // A gallery that goes away wants no page and no thumbnail any more, holds
            // no gesture and leaves no layer behind. Its selection work goes with it,
            // held or released: a press, a range read on its way and a failed range
            // with its Retry (the cleanup of `createGridHandlers`' effects sends
            // `rectangle-cancel` on unmount whether a gesture is held or not). The
            // selection itself stays; outside selection mode the model ignores this.
            cx.on_release(|this, cx| {
                this.runtime.update(cx, |runtime, cx| {
                    runtime.send(
                        RuntimeInput::Selection(SelectionInput::Interact(
                            media_tagger_client::gallery::Interaction::RectangleCancel,
                        )),
                        cx,
                    )
                });
                if let Some(layer) = this.selection_layer.take() {
                    layers::forget(layer, cx);
                }
                if this.published.0.is_some() {
                    this.runtime.update(cx, |runtime, cx| {
                        runtime.send(RuntimeInput::Query(QueryInput::SetViewport(None)), cx)
                    });
                }
                this.thumbnails
                    .update(cx, |thumbnails, _| thumbnails.sync(Vec::new(), false));
            }),
        ];
        let (page_failure_epoch, gesture_stamp) = {
            let model = runtime.read(cx).model();
            (
                model.query().page_failure_epoch(),
                selection::GestureStamp {
                    mode: model.selection().enabled(),
                    query_epoch: model.query().query_epoch(),
                    layout: None,
                },
            )
        };
        let mut view = Self {
            runtime,
            thumbnails,
            search,
            settings_opener,
            tile_size,
            drag: Rc::default(),
            grid: VirtualGrid::new(),
            viewport_size: None,
            started: cx.background_executor().now(),
            idle_timer: None,
            published: (None, page_failure_epoch),
            demand: None,
            grid_focus: cx.focus_handle(),
            scrollbar_focus: cx.focus_handle().tab_stop(true),
            scrollbar_drag: None,
            scrollbar_bounds: Rc::default(),
            timeline: TimelineScrubber::new(),
            timeline_focus: cx.focus_handle().tab_stop(true),
            timeline_bounds: Rc::default(),
            timeline_hovered: false,
            gesture: SelectionGesture::new(),
            gesture_stamp,
            long_press_timer: None,
            gesture_frame_pending: false,
            viewport_origin: Rc::default(),
            control_pressed: Rc::default(),
            selection_layer: None,
            dock: None,
            _subscriptions: subscriptions,
        };
        // Selection mode outlives the view: one made while it is on (back from the
        // settings page) takes its layer at once, and shows its dock.
        view.sync_selection_layer(window, cx);
        view.sync_dock(window, cx);
        view
    }

    /// The bulk dock, while selection mode is on.
    pub fn dock(&self) -> Option<&Entity<BulkDock>> {
        self.dock.as_ref()
    }

    /// Shows the bulk dock exactly while selection mode is on
    /// (`selection.enabled ? <BulkDock/> : null`).
    fn sync_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match (self.selection_enabled(cx), self.dock.is_some()) {
            (true, false) => {
                let runtime = self.runtime.clone();
                let viewport = self.viewport_size;
                self.dock = Some(cx.new(|cx| {
                    let mut dock = BulkDock::new(runtime, window, cx);
                    if let Some(size) = viewport {
                        dock.set_viewport(size, cx);
                    }
                    dock
                }));
            }
            (false, true) => self.dock = None,
            _ => {}
        }
    }

    /// The search header.
    pub fn search(&self) -> &Entity<SearchBar> {
        &self.search
    }

    /// The pointer gesture of selection mode.
    pub fn gesture(&self) -> &SelectionGesture {
        &self.gesture
    }

    pub fn tile_size_slider(&self) -> &Entity<Slider> {
        &self.tile_size
    }

    /// The scroll state of the grid.
    pub fn grid(&self) -> &VirtualGrid {
        &self.grid
    }

    /// The focus handle of the grid's viewport.
    pub fn grid_focus(&self) -> &FocusHandle {
        &self.grid_focus
    }

    /// The focus handle of the position bar.
    pub fn scrollbar_focus(&self) -> &FocusHandle {
        &self.scrollbar_focus
    }

    /// The state of the month timeline.
    pub fn timeline(&self) -> &TimelineScrubber {
        &self.timeline
    }

    /// The focus handle of the month timeline.
    pub fn timeline_focus(&self) -> &FocusHandle {
        &self.timeline_focus
    }

    fn now(&self, cx: &App) -> Duration {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.started)
    }

    /// Brings the grid in line with the runtime and the measured viewport, tells the
    /// query model and the thumbnails what changed for them, and asks for a redraw.
    /// Everything that changes what the grid shows ends here; rendering only reads.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let (total, tile_size, load_failed, selection_mode, page_failure_epoch, version) = {
            let model = self.runtime.read(cx).model();
            let query = model.query();
            (
                query.total(),
                model.tile_size(),
                model.gallery_load_failed(),
                model.selection().enabled(),
                query.page_failure_epoch(),
                query.version(),
            )
        };
        if let Some(size) = self.viewport_size {
            let width = gallery_layout::tile_area_width(f32::from(size.width));
            self.grid.handle(GridInput::Layout {
                geometry: GridGeometry {
                    width: f64::from(width),
                    viewport: f64::from(f32::from(size.height)),
                    lead: f64::from(gallery_layout::lead(load_failed)),
                    trail: f64::from(gallery_layout::trail(total > 0, selection_mode)),
                },
                count: total,
                tile_size: f64::from(tile_size),
            });
        }

        // The timeline follows the session's sections and the grid's position.
        let mut timeline_effects = Vec::new();
        {
            let sections = self.runtime.read(cx).model().query().sections();
            if sections != self.timeline.sections() {
                let sections = sections.to_vec();
                timeline_effects.extend(self.timeline.handle(TimelineInput::Sections(sections)));
            }
        }
        let first_visible = self
            .grid
            .range()
            .map(|range| range.visible_start_index)
            .unwrap_or(0);
        timeline_effects.extend(self.timeline.handle(TimelineInput::View {
            total,
            viewport: self.grid.geometry().viewport,
            first_visible,
        }));
        // A rail or bar that is no longer drawn gets no release and no pointer leave:
        // what a drag or a hover on it held ends here (the timeline ends its own drag
        // and asks to resume demand), as the Solid parts' cleanup does on unmount.
        for effect in timeline_effects {
            if effect == TimelineEffect::ResumeDemand {
                self.grid_input(GridInput::ResumeDemand, cx);
            }
        }
        if !self.timeline.visible() {
            self.timeline_hovered = false;
        }
        if !scrollbar_visible(
            self.grid.layout().count,
            self.grid.min_offset(),
            self.grid.max_offset(),
        ) {
            self.scrollbar_drag = None;
        }
        self.sync_gesture(cx);

        let viewport = self.grid.viewport();
        if self.published != (viewport, page_failure_epoch) {
            self.published = (viewport, page_failure_epoch);
            self.runtime.update(cx, |runtime, cx| {
                runtime.send(RuntimeInput::Query(QueryInput::SetViewport(viewport)), cx)
            });
        }

        let range = self.grid.range();
        let paused = self.grid.paused();
        let stamp = (version, range, paused);
        if self.demand != Some(stamp) {
            self.demand = Some(stamp);
            let tiles = match range {
                Some(range) => {
                    let query = self.runtime.read(cx).model().query();
                    (range.start_index..=range.end_index)
                        .filter_map(|index| {
                            query.asset_at(index).map(|asset| TileRequest {
                                key: thumbnail_key(asset),
                                visible: range.is_visible(index),
                            })
                        })
                        .collect()
                }
                None => Vec::new(),
            };
            self.thumbnails
                .update(cx, |thumbnails, _| thumbnails.sync(tiles, paused));
        }
        cx.notify();
    }

    /// Applies a scroll input and starts the timer it asks for.
    fn scroll(&mut self, input: GridInput, cx: &mut Context<Self>) {
        self.grid_input(input, cx);
        self.refresh(cx);
    }

    /// Hands `input` to the grid and starts the timers it asks for, without a
    /// refresh.
    fn grid_input(&mut self, input: GridInput, cx: &mut Context<Self>) {
        for effect in self.grid.handle(input) {
            let GridEffect::IdleTimer { token, delay } = effect;
            self.idle_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                this.update(cx, |this, cx| {
                    this.grid.handle(GridInput::IdleElapsed(token));
                    this.refresh(cx);
                })
                .ok();
            }));
        }
    }

    /// The viewport was laid out at `size`.
    fn measured(&mut self, size: Size<Pixels>, cx: &mut Context<Self>) {
        if self.viewport_size != Some(size) {
            self.viewport_size = Some(size);
            if let Some(dock) = &self.dock {
                dock.update(cx, |dock, cx| dock.set_viewport(size, cx));
            }
            self.refresh(cx);
        }
    }

    /// The wheel over the gallery: with Ctrl it changes the tile size, otherwise it
    /// scrolls. GPUI's delta is how far the content moves, so its sign is the
    /// opposite of a DOM wheel event's `deltaY`.
    fn on_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(area::WHEEL_LINE));
        let delta_y = -f64::from(f32::from(delta.y));
        if event.modifiers.control {
            if delta_y != 0.0 {
                self.runtime.update(cx, |runtime, cx| {
                    runtime.send(RuntimeInput::Zoom(delta_y), cx)
                });
            }
            return;
        }
        let at = self.now(cx);
        self.scroll(GridInput::ScrollBy { delta: delta_y, at }, cx);
    }

    fn scroll_to_edge(&mut self, edge: ScrollEdge, cx: &mut Context<Self>) {
        let at = self.now(cx);
        self.scroll(GridInput::ScrollToEdge { edge, at }, cx);
    }

    fn scroll_by(&mut self, delta: f64, cx: &mut Context<Self>) {
        let at = self.now(cx);
        self.scroll(GridInput::ScrollBy { delta, at }, cx);
    }

    fn page(&self) -> f64 {
        self.grid.geometry().viewport * area::PAGE_FRACTION
    }

    fn header(&self, i18n: &I18n<'_>, theme: ThemeName, window: &Window, cx: &App) -> AnyElement {
        let colors = &theme.tokens().colors;
        let model = self.runtime.read(cx).model();
        let runtime = self.runtime.downgrade();

        let identity = div()
            .debug_selector(|| IDENTITY_SELECTOR.to_owned())
            .flex()
            .flex_none()
            .w(px(search_row::IDENTITY_WIDTH))
            .self_stretch()
            .items_center()
            .gap(px(identity::GAP))
            .text_size(px(identity::FONT_SIZE))
            .font_weight(weight(identity::FONT_WEIGHT))
            .child(logo(identity::LOGO_SIZE))
            .child(PRODUCT_NAME);

        let open_settings = {
            let runtime = runtime.clone();
            Button::icon(
                OPEN_SETTINGS_ID,
                media_tagger_client::icons::IconName::Settings,
                i18n.t("topBar.openSettings"),
            )
            .track_focus(&self.settings_opener)
            .on_click(move |_, _, cx| runtime::send(&runtime, RuntimeInput::OpenSettings, cx))
        };

        let search = drag_surface(
            div()
                .flex()
                .items_center()
                .gap(px(search_row::COLUMN_GAP))
                .min_h(px(search_row::MIN_HEIGHT))
                .py(px(search_row::PADDING_Y))
                .px(px(search_row::PADDING_X)),
            &self.drag,
        )
        .child(identity)
        .child(
            no_drag(
                div()
                    .id("workspace-search-slot")
                    .debug_selector(|| SEARCH_SLOT_SELECTOR.to_owned())
                    .flex()
                    .flex_1()
                    .min_w_0(),
            )
            .child(self.search.clone()),
        )
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(search_row::ACTIONS_GAP))
                .child(
                    no_drag(
                        div()
                            .id("open-settings-slot")
                            .debug_selector(|| OPEN_SETTINGS_ID.to_owned()),
                    )
                    .child(open_settings),
                )
                .child(chrome::window_controls(
                    i18n,
                    theme,
                    chrome::window_state(window),
                )),
        );

        let count = div()
            .debug_selector(|| COUNT_SELECTOR.to_owned())
            .flex()
            .flex_none()
            .items_center()
            .gap(px(count_chip::GAP))
            .py(px(count_chip::PADDING_Y))
            .px(px(count_chip::PADDING_X))
            .rounded(px(count_chip::RADIUS))
            .bg(color(colors.surface_muted))
            .font_family(fonts::MONO)
            .text_size(px(count_chip::FONT_SIZE))
            .text_color(color(colors.text_muted))
            .child(
                div()
                    .size(px(count_chip::DOT_SIZE))
                    .flex_none()
                    .rounded_full()
                    .bg(color(colors.primary)),
            )
            .child(item_count(i18n, model.query().total()));

        let tile_size = div()
            .flex()
            .items_center()
            .gap(px(tools_row::TILE_SIZE_GAP))
            .border_l_1()
            .border_color(color(colors.border_soft))
            .pl(px(tools_row::TILE_SIZE_PADDING_LEFT))
            .child(
                div()
                    .text_size(px(TYPOGRAPHY.xs.size))
                    .text_color(color(colors.text_muted))
                    .child(i18n.t("controls.tileSize")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .w(px(tools_row::SLIDER_WIDTH))
                    .py(px(tools_row::SLIDER_PADDING_Y))
                    .child(self.tile_size.clone()),
            );

        let tools = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(tools_row::GAP))
            .min_h(px(tools_row::MIN_HEIGHT))
            .py(px(tools_row::PADDING_Y))
            .px(px(tools_row::PADDING_X))
            .border_t_1()
            .border_color(color(colors.border_soft))
            // Media kind and favourites, then the tag list's place (E13).
            .children(self.search.read(cx).tools(i18n, theme, cx))
            .child(count)
            .child(
                div()
                    .ml_auto()
                    .flex()
                    .items_center()
                    .gap(px(tools_row::SIZING_GAP))
                    .child(self.bulk_toggle(i18n, cx))
                    .child(tile_size),
            );

        div()
            .debug_selector(|| HEADER_SELECTOR.to_owned())
            .relative()
            .flex_none()
            .bg(color(colors.window_chrome_bg))
            .border_b_1()
            .border_color(color(colors.border_strong))
            .child(search)
            .child(tools)
            .children(self.search.read(cx).validation(i18n, cx))
            .into_any_element()
    }

    fn empty_state(
        &self,
        state: EmptyState,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
    ) -> AnyElement {
        use media_tagger_client::shell::empty_state as layout;
        let colors = &theme.tokens().colors;
        let height = (f32::from(window.viewport_size().height) - layout::VIEWPORT_INSET).max(0.0);
        let runtime = self.runtime.downgrade();
        div()
            .debug_selector(|| EMPTY_STATE_SELECTOR.to_owned())
            .mx_auto()
            .w_full()
            .max_w(px(layout::MAX_WIDTH))
            .min_h(px(height))
            .p(px(layout::PADDING))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(layout::GAP))
            .text_center()
            .child(
                div()
                    .mb(px(layout::ICON_TILE_MARGIN_BOTTOM))
                    .size(px(layout::ICON_TILE_SIZE))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(RADII.control))
                    .bg(color(
                        colors
                            .primary
                            .with_alpha_percent(layout::ICON_TILE_FILL_PERCENT),
                    ))
                    .border_1()
                    .border_color(color(
                        colors
                            .primary
                            .with_alpha_percent(layout::ICON_TILE_RING_PERCENT),
                    ))
                    .child(
                        Icon::new(state.icon())
                            .size(layout::ICON_SIZE)
                            .color(color(colors.primary_text)),
                    ),
            )
            .child(
                div()
                    .text_size(px(layout::TITLE_SIZE))
                    .line_height(relative(layout::TITLE_LINE_HEIGHT))
                    .font_weight(weight(TYPOGRAPHY.heading_weight))
                    .text_color(color(colors.base_content))
                    .child(i18n.t(state.title_key())),
            )
            .child(
                div()
                    .max_w(px(layout::DESCRIPTION_MAX_WIDTH))
                    .text_size(px(layout::DESCRIPTION_SIZE))
                    .line_height(relative(layout::DESCRIPTION_LINE_HEIGHT))
                    .text_color(color(colors.text_muted))
                    .child(i18n.t(state.description_key())),
            )
            .when_some(state.action_key(), |element, key| {
                element.child(
                    Button::new(ADD_FIRST_FOLDER_ID, SharedString::from(i18n.t(key)))
                        .variant(ButtonVariant::Primary)
                        .debug_selector(ADD_FIRST_FOLDER_ID)
                        .on_click(move |_, _, cx| {
                            runtime::send(&runtime, RuntimeInput::OpenFirstFolderSettings, cx)
                        }),
                )
            })
            .into_any_element()
    }
}

use gpui::App;

impl GalleryView {
    /// The gallery without assets: the load error banner, if any, above the empty
    /// state, inside the section's paddings.
    fn empty_body(
        &self,
        state: EmptyState,
        load_failed: bool,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
    ) -> AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .pt(px(area::PADDING_TOP))
            .pl(px(area::PADDING_LEFT))
            .pr(px(area::PADDING_RIGHT))
            .when(load_failed, |body| {
                body.child(
                    self.load_error(i18n, theme)
                        .mb(px(gallery_layout::error_banner::MARGIN_BOTTOM)),
                )
            })
            .child(self.empty_state(state, i18n, theme, window))
            .into_any_element()
    }

    fn load_error(&self, i18n: &I18n<'_>, theme: ThemeName) -> gpui::Stateful<gpui::Div> {
        let runtime = self.runtime.downgrade();
        let grid_focus = self.grid_focus.clone();
        let control_pressed = self.control_pressed.clone();
        grid::load_error_banner(i18n, theme, control_pressed, move |_, window, cx| {
            // The banner leaves with the retry and its button with it: the focus goes
            // to the grid instead of nowhere, so the scroll keys work at once.
            window.focus(&grid_focus, cx);
            // The failed pages of the query, then the range the selection could not
            // read (`library.query.retryLoad(); selection.retry()`).
            runtime::send(&runtime, RuntimeInput::Query(QueryInput::RetryLoad), cx);
            runtime::send(&runtime, RuntimeInput::Selection(SelectionInput::Retry), cx)
        })
    }

    /// The rendered rows, the footer and the load error banner, each placed relative
    /// to the top of the viewport.
    fn grid_body(
        &self,
        load_failed: bool,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let model = self.runtime.read(cx).model();
        let query = model.query();
        let selection = model.selection();
        let selection_mode = selection.enabled();
        // What a tile shows while a rectangle is dragged over it, and what is
        // selected otherwise; a slot without an asset follows the ranges only.
        let selected = |index: usize, asset_id: Option<i64>| {
            selection_mode
                && self
                    .gesture
                    .is_preview_selected(index, selection.is_selected(index, asset_id))
        };
        let thumbnails = self.thumbnails.read(cx);
        let layout = *self.grid.layout();
        let size = layout.size as f32;
        let gap = layout.gap as f32;
        let stride = layout.stride as f32;
        let paused = self.grid.paused();
        let mut elements: Vec<AnyElement> = Vec::new();
        let mut tiles: Vec<AnyElement> = Vec::new();

        if let Some(rows) = self.grid.rows() {
            for row in rows.first..=rows.last {
                let y = self.grid.row_y(row) as f32;
                let first = row * layout.columns;
                let lanes = layout.columns.min(layout.count.saturating_sub(first));
                let assets: Vec<_> = (0..lanes)
                    .map(|lane| query.asset_at(first + lane))
                    .collect();
                let outset = gallery_layout::tile::GROUP_PLATE_OUTSET;
                for (from, to) in
                    group_plates(assets.iter().map(|asset| asset.and_then(media_group)))
                {
                    elements.push(
                        grid::group_plate(to - from + 1, size, gap, theme)
                            .debug_selector(move || grid::group_plate_selector(first + from))
                            .left(px(area::PADDING_LEFT + from as f32 * stride - outset))
                            .top(px(y - outset))
                            .into_any_element(),
                    );
                }
                for (lane, asset) in assets.into_iter().enumerate() {
                    let index = first + lane;
                    let x = area::PADDING_LEFT + lane as f32 * stride;
                    tiles.push(match asset {
                        Some(asset) => {
                            let asset_id = asset.id;
                            grid::tile(
                                asset,
                                index,
                                size,
                                thumbnails.picture(&thumbnail_key(asset)),
                                selected(index, Some(asset_id)),
                                i18n,
                                theme,
                                cx.listener(move |this, event: &ClickEvent, window, cx| {
                                    this.tile_clicked(asset_id, index, event, window, cx)
                                }),
                            )
                            .left(px(x))
                            .top(px(y))
                            .into_any_element()
                        }
                        None => div()
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .size(px(size))
                            .child(grid::placeholder(
                                index,
                                size,
                                !paused,
                                selected(index, None),
                                theme,
                            ))
                            .into_any_element(),
                    });
                }
            }
        }
        // Backplates lie behind every tile.
        elements.append(&mut tiles);

        if load_failed {
            let width = self
                .viewport_size
                .map(|size| gallery_layout::tile_area_width(f32::from(size.width)))
                .unwrap_or_default();
            elements.push(
                self.load_error(i18n, theme)
                    .absolute()
                    .left(px(area::PADDING_LEFT))
                    .top(px(self.grid.lead_y() as f32 + area::PADDING_TOP))
                    .w(px(width))
                    .into_any_element(),
            );
        }
        elements.push(
            grid::status_footer(query.total(), i18n, theme)
                .top(px(self.grid.trail_y() as f32))
                .into_any_element(),
        );
        // The rectangle of a drag selection, above everything of the grid.
        if let Some(preview) = self.gesture.preview() {
            let rectangle = preview.rectangle;
            elements.push(
                grid::selection_rectangle(rectangle.width as f32, rectangle.height as f32, theme)
                    .left(px(rectangle.left as f32))
                    .top(px(rectangle.top as f32))
                    .into_any_element(),
            );
        }
        elements
    }
}

impl Render for GalleryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let (i18n, empty, load_failed) = {
            let runtime = self.runtime.read(cx);
            (
                runtime.i18n(),
                empty_state(runtime.model()),
                runtime.model().gallery_load_failed(),
            )
        };
        // Textures of pictures the cache dropped since the last frame.
        self.thumbnails
            .update(cx, |thumbnails, cx| thumbnails.release_textures(window, cx));
        let header = self.header(&i18n, theme, window, cx);
        let body = match empty {
            Some(state) => {
                vec![self.empty_body(state, load_failed, &i18n, theme, window)]
            }
            None => {
                // The position bar and the month timeline first: their tab stops come
                // before the banner's Retry, as in the Solid DOM. Nothing of the grid
                // lies under them (the tile area keeps clear of the right 34 px).
                let mut body: Vec<AnyElement> = Vec::new();
                body.extend(self.scrollbar(&i18n, theme, window, cx));
                body.extend(self.timeline_rail(&i18n, theme, window, cx));
                body.extend(self.grid_body(load_failed, &i18n, theme, cx));
                body
            }
        };
        let drag_selecting = self.gesture.dragging();
        // Reports the viewport's size after layout. A new size is applied after this
        // frame, since nothing may change state while a frame is drawn.
        let measure = {
            let view = cx.entity().downgrade();
            let known = self.viewport_size;
            let origin = self.viewport_origin.clone();
            canvas(
                move |bounds, window, cx| {
                    origin.set(Some(bounds.origin));
                    if known != Some(bounds.size) {
                        let size = bounds.size;
                        window.defer(cx, move |_, cx| {
                            view.update(cx, |view, cx| view.measured(size, cx)).ok();
                        });
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };
        div()
            .key_context(KeyContext::Gallery.name())
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            // A modal dialog drawn from the header (the tag list) leaves the grid alone:
            // its keys do not scroll or select what lies under it.
            .on_action(cx.listener(|this, _: &ScrollStart, _, cx| {
                if !layers::has_modal(cx) {
                    this.scroll_to_edge(ScrollEdge::Start, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &ScrollEnd, _, cx| {
                if !layers::has_modal(cx) {
                    this.scroll_to_edge(ScrollEdge::End, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                if !layers::has_modal(cx) {
                    let page = this.page();
                    this.scroll_by(-page, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                if !layers::has_modal(cx) {
                    let page = this.page();
                    this.scroll_by(page, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &LineUp, _, cx| {
                if !layers::has_modal(cx) {
                    this.scroll_by(-area::KEY_LINE, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &LineDown, _, cx| {
                if !layers::has_modal(cx) {
                    this.scroll_by(area::KEY_LINE, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &SelectAll, window, cx| {
                if !layers::has_modal(cx) {
                    this.select_all(window, cx)
                }
            }))
            .child(header)
            .child(
                div()
                    .id("workspace-viewport")
                    .debug_selector(|| VIEWPORT_SELECTOR.to_owned())
                    .track_focus(&self.grid_focus)
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .on_scroll(cx.listener(Self::on_wheel))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            window.focus(&this.grid_focus, cx);
                            this.selection_press(event, window, cx);
                        }),
                    )
                    // `cursor-crosshair` while a rectangle is dragged.
                    .when(drag_selecting, |viewport| viewport.cursor_crosshair())
                    .child(measure)
                    .child(self.selection_listeners(cx))
                    .children(body)
                    // The dock, centred above the viewport's bottom edge. The strip
                    // around it takes no input: only the dock itself covers the grid.
                    .children(self.dock.clone().map(|dock| {
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(px(media_tagger_client::bulk::layout::dock::MARGIN))
                            .flex()
                            .justify_center()
                            .child(dock)
                    })),
            )
    }
}
