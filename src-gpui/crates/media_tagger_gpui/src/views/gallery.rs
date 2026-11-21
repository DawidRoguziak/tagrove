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

mod grid;

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{
    canvas, div, prelude::*, px, relative, AnyElement, Context, Entity, EventEmitter, FocusHandle,
    MouseButton, Pixels, ScrollEvent, SharedString, Size, Subscription, Task, Window,
};
use media_tagger_client::{
    controls::{ButtonVariant, SliderRange},
    gallery::{
        layout::{self as gallery_layout, area, footer, group_plates, media_group, thumbnail_key},
        GridEffect, GridGeometry, GridInput, LibraryRange, ScrollEdge, TileRequest, VirtualGrid,
    },
    i18n::I18n,
    library::{QueryInput, Viewport},
    model::Model as _,
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
    placeholder_selector, tile_selector, GalleryEvent, LineDown, LineUp, PageDown, PageUp,
    ScrollEnd, ScrollStart, FOOTER_SELECTOR, LOAD_ERROR_SELECTOR, LOAD_RETRY_ID, VIEWPORT_SELECTOR,
};

use super::chrome::{self, drag_surface, no_drag};
use crate::{
    runtime::{self, Runtime},
    thumbs::Thumbnails,
    ui::{
        button::Button,
        fonts,
        icon::Icon,
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
pub const COUNT_SELECTOR: &str = "workspace-count";
pub const EMPTY_STATE_SELECTOR: &str = "gallery-empty-state";
pub const ADD_FIRST_FOLDER_ID: &str = "gallery-add-first-folder";

/// What the thumbnails were last told: the cache version of the rows, the range and
/// the pause flag. The tiles are collected again only when one of them changed.
type DemandStamp = (u64, Option<LibraryRange>, bool);

pub struct GalleryView {
    runtime: Entity<Runtime>,
    thumbnails: Entity<Thumbnails>,
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
        let subscriptions = vec![
            cx.subscribe(&tile_size, |this, _, event, cx| {
                let SliderEvent::Input(value) = *event;
                this.runtime.update(cx, |runtime, cx| {
                    runtime.send(RuntimeInput::SetTileSize(value), cx)
                });
            }),
            cx.observe(&runtime, |this, runtime, cx| {
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
                this.refresh(cx);
            }),
            // A picture arrived or failed: the tiles are drawn again.
            cx.observe(&thumbnails, |_, _, cx| cx.notify()),
            cx.observe_window_bounds(window, |_, _, cx| cx.notify()),
            // A gallery that goes away wants no page and no thumbnail any more.
            cx.on_release(|this, cx| {
                if this.published.0.is_some() {
                    this.runtime.update(cx, |runtime, cx| {
                        runtime.send(RuntimeInput::Query(QueryInput::SetViewport(None)), cx)
                    });
                }
                this.thumbnails
                    .update(cx, |thumbnails, _| thumbnails.sync(Vec::new(), false));
            }),
        ];
        let page_failure_epoch = runtime.read(cx).model().query().page_failure_epoch();
        Self {
            runtime,
            thumbnails,
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
            _subscriptions: subscriptions,
        }
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

    fn now(&self, cx: &App) -> Duration {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.started)
    }

    /// Brings the grid in line with the runtime and the measured viewport, tells the
    /// query model and the thumbnails what changed for them, and asks for a redraw.
    /// Everything that changes what the grid shows ends here; rendering only reads.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let (total, tile_size, load_failed, page_failure_epoch, version) = {
            let model = self.runtime.read(cx).model();
            let query = model.query();
            (
                query.total(),
                model.tile_size(),
                query.load_error().is_some(),
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
                    trail: if total > 0 {
                        f64::from(footer::HEIGHT)
                    } else {
                        0.0
                    },
                },
                count: total,
                tile_size: f64::from(tile_size),
            });
        }

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
        self.refresh(cx);
    }

    /// The viewport was laid out at `size`.
    fn measured(&mut self, size: Size<Pixels>, cx: &mut Context<Self>) {
        if self.viewport_size != Some(size) {
            self.viewport_size = Some(size);
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
        // The search field and its toolbar are epic E12.
        .child(
            div()
                .debug_selector(|| SEARCH_SLOT_SELECTOR.to_owned())
                .flex_1()
                .min_w_0(),
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
            // Media kind, favourites and the tag list come first (E12, E13).
            .child(count)
            .child(
                div()
                    .ml_auto()
                    .flex()
                    .items_center()
                    .gap(px(tools_row::SIZING_GAP))
                    // The bulk-actions toggle goes here (E11, E14).
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
        grid::load_error_banner(i18n, theme, move |_, window, cx| {
            // The banner leaves with the retry and its button with it: the focus goes
            // to the grid instead of nowhere, so the scroll keys work at once.
            window.focus(&grid_focus, cx);
            runtime::send(&runtime, RuntimeInput::Query(QueryInput::RetryLoad), cx)
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
                                i18n,
                                theme,
                                cx.listener(move |_, _, _, cx| {
                                    cx.emit(GalleryEvent::OpenAsset { asset_id, index })
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
                            .child(grid::placeholder(index, size, !paused, theme))
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
                runtime.model().query().load_error().is_some(),
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
            None => self.grid_body(load_failed, &i18n, theme, cx),
        };
        // Reports the viewport's size after layout. A new size is applied after this
        // frame, since nothing may change state while a frame is drawn.
        let measure = {
            let view = cx.entity().downgrade();
            let known = self.viewport_size;
            canvas(
                move |bounds, window, cx| {
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
            .on_action(cx.listener(|this, _: &ScrollStart, _, cx| {
                this.scroll_to_edge(ScrollEdge::Start, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ScrollEnd, _, cx| this.scroll_to_edge(ScrollEdge::End, cx)),
            )
            .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                let page = this.page();
                this.scroll_by(-page, cx)
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                let page = this.page();
                this.scroll_by(page, cx)
            }))
            .on_action(cx.listener(|this, _: &LineUp, _, cx| this.scroll_by(-area::KEY_LINE, cx)))
            .on_action(cx.listener(|this, _: &LineDown, _, cx| this.scroll_by(area::KEY_LINE, cx)))
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
                        cx.listener(|this, _, window, cx| window.focus(&this.grid_focus, cx)),
                    )
                    .child(measure)
                    .children(body),
            )
    }
}
