//! The order board (`GroupOrderBoard.tsx`): the grid of thumbnails that reorders a
//! group by pointer drag or by the arrow keys on each tile's grip. The dock shows one
//! inline in its group tab and one in the larger order view, never both.
//!
//! Where the tiles lie, which rows are drawn, where a drop lands and what a key does
//! are decided by `media_tagger_client::bulk::board`; this file draws the rows near the
//! visible part of the list, turns GPUI's pointer events into the drag's inputs and
//! scrolls the list while a tile is dragged near its edge. The board reports the next
//! order to the dock's state and keeps none itself.
//!
//! A grip's focus handle is kept by asset ID, so the focus stays on the grip while its
//! tile moves; a key move scrolls the tile's new row into view, where it is drawn.

use std::{cell::Cell, collections::HashMap, rc::Rc};

use gpui::{
    canvas, deferred, div, img, point, prelude::*, px, Animation, AnimationExt as _, AnyElement,
    Bounds, Context, CursorStyle, DispatchPhase, ElementId, FocusHandle, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Pixels, Point, Role, ScrollHandle,
    SharedString, Window,
};
use media_tagger_client::{
    bulk::{
        board::edge_scroll_speed,
        layout::{board as measures, group},
        reorder_asset_ids_by_drop, BoardDrag, BoardKey, BoardLayout, DragEffect,
    },
    controls::{FocusRing, SpinnerSize, THUMBNAIL_LOADER},
    gallery::{layout::thumbnail_key, TileRequest},
    i18n::{I18n, Params},
    icons::IconName,
    theme::{ThemeName, RADII, TYPOGRAPHY},
};

use super::{measure, BulkDock, MoveDown, MoveLeft, MoveRight, MoveUp, HANDLE_KEY_CONTEXT};
use crate::{
    thumbs::TilePicture,
    ui::{
        fonts,
        icon::Icon,
        spinner::Spinner,
        theme::{box_shadows, color, focus_ring, weight, RingBorder},
    },
};

/// The two boards of the dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum BoardKind {
    /// In the group tab.
    Inline,
    /// In the larger order view.
    Larger,
}

impl BoardKind {
    /// The prefix of the board's debug selectors (`testIdPrefix`).
    const fn prefix(self) -> &'static str {
        match self {
            Self::Inline => "bulk-inline-order",
            Self::Larger => "bulk-order",
        }
    }

    const fn min_tile_width(self) -> f32 {
        match self {
            Self::Inline => group::INLINE_MIN_TILE_WIDTH,
            Self::Larger => group::MODAL_MIN_TILE_WIDTH,
        }
    }
}

fn prefix(larger: bool) -> &'static str {
    if larger {
        BoardKind::Larger.prefix()
    } else {
        BoardKind::Inline.prefix()
    }
}

/// The debug selector of the inline board's list (`larger`: of the larger view's).
pub fn grid_selector(larger: bool) -> String {
    format!("{}-grid", prefix(larger))
}

/// The debug selector of the tile of `asset_id`.
pub fn tile_selector(larger: bool, asset_id: i64) -> String {
    format!("{}-tile-{asset_id}", prefix(larger))
}

/// The debug selector of the grip of `asset_id`.
pub fn handle_selector(larger: bool, asset_id: i64) -> String {
    format!("{}-handle-{asset_id}", prefix(larger))
}

/// The debug selector of the insertion line.
pub fn insertion_line_selector(larger: bool) -> String {
    format!("{}-insertion-line", prefix(larger))
}

/// What one board keeps between frames.
pub(super) struct BoardState {
    pub(super) scroll: ScrollHandle,
    pub(super) drag: BoardDrag,
    /// Where the list was last laid out in the window.
    pub(super) bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The focus handles of the grips, by asset ID.
    pub(super) handles: HashMap<i64, FocusHandle>,
    /// The order `handles` was last pruned for.
    pruned_for: Vec<i64>,
    /// The next frame of a drag is scheduled.
    tick_pending: bool,
}

impl BoardState {
    pub(super) fn new() -> Self {
        Self {
            scroll: ScrollHandle::new(),
            drag: BoardDrag::new(),
            bounds: Rc::default(),
            handles: HashMap::new(),
            pruned_for: Vec::new(),
            tick_pending: false,
        }
    }

    /// The list's width and height as last laid out, or the sizes assumed before.
    fn size(&self) -> (f32, f32) {
        match self.bounds.get() {
            Some(bounds) if bounds.size.width > px(0.) => {
                (f32::from(bounds.size.width), f32::from(bounds.size.height))
            }
            _ => (measures::INITIAL_WIDTH, measures::INITIAL_VIEWPORT),
        }
    }

    fn scroll_top(&self) -> f32 {
        (-f32::from(self.scroll.offset().y)).max(0.0)
    }

    fn layout(&self, kind: BoardKind, count: usize) -> BoardLayout {
        BoardLayout::new(self.size().0, kind.min_tile_width(), count)
    }

    fn scroll_to(&self, top: f32) {
        let offset = self.scroll.offset();
        self.scroll.set_offset(point(offset.x, px(-top.max(0.0))));
    }

    /// Drops the grips of assets that are not in `order`. The board is drawn on every
    /// frame of the window and an order can hold thousands of IDs, so the set of them
    /// is built only for an order that is not the one of the last call.
    fn prune_handles(&mut self, order: &[i64]) {
        if self.pruned_for == order {
            return;
        }
        if !self.handles.is_empty() {
            let live: std::collections::HashSet<i64> = order.iter().copied().collect();
            self.handles.retain(|id, _| live.contains(id));
        }
        self.pruned_for.clear();
        self.pruned_for.extend_from_slice(order);
    }
}

/// What a tile shows, read from the runtime before the elements are built.
struct TileData {
    id: i64,
    index: usize,
    name: SharedString,
    picture: TilePicture,
    handle: FocusHandle,
}

impl BulkDock {
    fn board_state(&self, kind: BoardKind) -> &BoardState {
        match kind {
            BoardKind::Inline => &self.inline,
            BoardKind::Larger => &self.larger,
        }
    }

    fn board_state_mut(&mut self, kind: BoardKind) -> &mut BoardState {
        match kind {
            BoardKind::Inline => &mut self.inline,
            BoardKind::Larger => &mut self.larger,
        }
    }

    /// The order the board of `kind` shows now.
    fn board_order(&self, kind: BoardKind) -> Vec<i64> {
        match kind {
            BoardKind::Inline => self.dock.inline_order().to_vec(),
            BoardKind::Larger => self.dock.modal_order().to_vec(),
        }
    }

    /// Whether the board of `kind` takes reordering now.
    fn board_blocked(&mut self, kind: BoardKind, cx: &gpui::App) -> bool {
        let open = self.modal.read(cx).is_open();
        self.with_view(cx, |dock, view| match kind {
            BoardKind::Inline => {
                media_tagger_client::bulk::Dock::order_locked(view) || dock.order_modal_open(view)
            }
            BoardKind::Larger => !open || dock.modal_locked(view),
        })
    }

    fn set_board_order(
        &mut self,
        kind: BoardKind,
        order: Vec<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match kind {
            BoardKind::Inline => self.dock.set_inline_order(order),
            BoardKind::Larger => self.dock.set_modal_order(order),
        }
        self.apply(window, cx);
    }

    /// The tile under the window position `position`, or the nearest drawn one;
    /// none outside the board.
    fn board_target(&self, kind: BoardKind, position: Point<Pixels>) -> Option<i64> {
        let state = self.board_state(kind);
        let bounds = state.bounds.get()?;
        if !bounds.contains(&position) {
            return None;
        }
        let order = self.board_order(kind);
        let layout = state.layout(kind, order.len());
        let scroll_top = state.scroll_top();
        let rows = layout.visible_rows(scroll_top, state.size().1);
        let x = f32::from(position.x - bounds.origin.x);
        let y = f32::from(position.y - bounds.origin.y) + scroll_top;
        order.get(layout.drop_target(x, y, rows)?).copied()
    }

    /// The board whose tile is pressed, if one is.
    fn held_board(&self) -> Option<BoardKind> {
        [BoardKind::Inline, BoardKind::Larger]
            .into_iter()
            .find(|kind| self.board_state(*kind).drag.held())
    }

    /// Ends every press without a drop.
    pub(super) fn cancel_drags(&mut self, cx: &mut Context<Self>) {
        if self.held_board().is_some() {
            self.inline.drag.cancel();
            self.larger.drag.cancel();
            cx.notify();
        }
    }

    fn board_press(
        &mut self,
        kind: BoardKind,
        id: i64,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let blocked = self.board_blocked(kind, cx);
        if self.held_board().is_some() {
            return;
        }
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        let state = self.board_state_mut(kind);
        if !state.drag.press(id, x, y, blocked) {
            return;
        }
        // The grip takes the focus, as a press on the Solid tile focuses its handle.
        if let Some(handle) = state.handles.get(&id) {
            window.focus(handle, cx);
        }
        window.prevent_default();
        cx.notify();
    }

    fn board_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.held_board() else {
            return;
        };
        if !event.dragging() {
            // The button went up where the window did not see it.
            self.cancel_drags(cx);
            return;
        }
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        if self.board_state_mut(kind).drag.moved(x, y) {
            let target = self.board_target(kind, event.position);
            self.board_state_mut(kind).drag.set_target(target);
            self.schedule_tick(kind, window, cx);
            cx.notify();
        }
    }

    fn board_release(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.held_board() else {
            return;
        };
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        let target = self.board_target(kind, event.position);
        let blocked = self.board_blocked(kind, cx);
        let dropped = self.board_state_mut(kind).drag.release(x, y, target);
        if let (Some(DragEffect::Reorder { dragged, target }), false) = (dropped, blocked) {
            let order = reorder_asset_ids_by_drop(&self.board_order(kind), dragged, target);
            self.set_board_order(kind, order, window, cx);
        }
        cx.notify();
    }

    /// One animation frame of a drag: the list scrolls while the pointer is near its
    /// top or bottom edge, and the tile under the pointer is worked out again, since
    /// the rows moved.
    fn board_tick(&mut self, kind: BoardKind, window: &mut Window, cx: &mut Context<Self>) {
        self.board_state_mut(kind).tick_pending = false;
        let state = self.board_state(kind);
        let Some((x, y)) = state.drag.position() else {
            return;
        };
        let position = point(px(x), px(y));
        if let Some(bounds) = state
            .bounds
            .get()
            .filter(|bounds| bounds.contains(&position))
        {
            let speed = edge_scroll_speed(
                y,
                f32::from(bounds.origin.y),
                f32::from(bounds.origin.y + bounds.size.height),
            );
            if speed != 0.0 {
                let limit = f32::from(state.scroll.max_offset().y).max(0.0);
                state.scroll_to((state.scroll_top() + speed).min(limit));
            }
        }
        let target = self.board_target(kind, position);
        self.board_state_mut(kind).drag.set_target(target);
        self.schedule_tick(kind, window, cx);
        cx.notify();
    }

    fn schedule_tick(&mut self, kind: BoardKind, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.board_state_mut(kind);
        if state.tick_pending || state.drag.dragged().is_none() {
            return;
        }
        state.tick_pending = true;
        cx.on_next_frame(window, move |this, window, cx| {
            this.board_tick(kind, window, cx)
        });
    }

    /// An arrow key on the grip of `id`: the tile moves, its row is scrolled into view
    /// and the grip keeps the focus.
    fn board_key(
        &mut self,
        kind: BoardKind,
        id: i64,
        key: BoardKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.board_blocked(kind, cx) || self.held_board().is_some() {
            return;
        }
        let order = self.board_order(kind);
        let state = self.board_state(kind);
        let layout = state.layout(kind, order.len());
        let Some((next, row)) = layout.key_move(&order, id, key) else {
            return;
        };
        state.scroll_to(layout.reveal_row(row, state.scroll_top(), state.size().1));
        if let Some(handle) = state.handles.get(&id) {
            window.focus(handle, cx);
        }
        self.set_board_order(kind, next, window, cx);
    }

    /// Tells the thumbnails which order tiles the frame draws.
    pub(super) fn flush_board_tiles(&mut self, cx: &mut Context<Self>) {
        let tiles = std::mem::take(&mut self.board_tiles);
        self.thumbnails
            .update(cx, |thumbnails, _| thumbnails.sync_board(tiles));
    }

    /// The window-wide listeners of a drag, registered in every frame; they do nothing
    /// while no tile is pressed. The release is taken in the capture phase, before any
    /// click it would cause.
    pub(super) fn drag_listeners(&self, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let moved = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble {
                        moved
                            .update(cx, |view, cx| view.board_move(event, window, cx))
                            .ok();
                    }
                });
                let released = view.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        released
                            .update(cx, |view, cx| view.board_release(event, window, cx))
                            .ok();
                    }
                });
            },
        )
        .absolute()
        .size_0()
    }

    /// The board of `kind` showing `order`: the rows near the visible part of its
    /// list, the insertion line and the ghost of a dragged tile.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn board(
        &mut self,
        kind: BoardKind,
        order: &[i64],
        blocked: bool,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = theme.tokens();
        let colors = &tokens.colors;
        let larger = kind == BoardKind::Larger;
        let (layout, scroll_top, viewport_height) = {
            let state = self.board_state(kind);
            (
                state.layout(kind, order.len()),
                state.scroll_top(),
                state.size().1,
            )
        };
        let rows = layout.visible_rows(scroll_top, viewport_height);
        let drawn: Vec<usize> = rows.flat_map(|row| layout.row_indexes(row)).collect();

        // Grips are kept by asset ID; the ones of assets that left the order go.
        let state = self.board_state_mut(kind);
        state.prune_handles(order);
        let mut handles = std::mem::take(&mut state.handles);
        for index in &drawn {
            handles
                .entry(order[*index])
                .or_insert_with(|| cx.focus_handle().tab_stop(true));
        }

        let (dragged, target, pointer) = {
            let drag = &self.board_state(kind).drag;
            (drag.dragged(), drag.target(), drag.position())
        };
        let (tiles, ghost_picture): (Vec<TileData>, Option<TilePicture>) = {
            let runtime = self.runtime.read(cx);
            let summaries = runtime.model().bulk().bulk.summaries();
            let thumbnails = self.thumbnails.read(cx);
            let tiles = drawn
                .iter()
                .map(|index| {
                    let id = order[*index];
                    let asset = summaries.record(id);
                    if let Some(asset) = asset {
                        self.board_tiles.push(TileRequest {
                            key: thumbnail_key(asset),
                            visible: true,
                        });
                    }
                    TileData {
                        id,
                        index: *index,
                        name: asset
                            .map(|asset| SharedString::from(asset.file_name.clone()))
                            .unwrap_or_default(),
                        picture: asset
                            .map(|asset| thumbnails.picture(&thumbnail_key(asset)))
                            .unwrap_or(TilePicture::Empty),
                        handle: handles[&id].clone(),
                    }
                })
                .collect();
            let ghost = dragged.and_then(|id| summaries.record(id)).map(|asset| {
                self.board_tiles.push(TileRequest {
                    key: thumbnail_key(asset),
                    visible: true,
                });
                thumbnails.picture(&thumbnail_key(asset))
            });
            (tiles, ghost)
        };
        self.board_state_mut(kind).handles = handles;

        let keyboard = window.last_input_was_keyboard();
        let count = order.len();
        let mut content: Vec<AnyElement> = Vec::with_capacity(tiles.len() + 1);
        for tile in tiles {
            let rect = layout.tile_rect(tile.index);
            let id = tile.id;
            let position = tile.index + 1;
            let focus_visible = !blocked && keyboard && tile.handle.is_focused(window);
            let hover = colors
                .base_content
                .with_alpha_percent(measures::HANDLE_HOVER_PERCENT);
            let grip = div()
                .id(ElementId::NamedInteger(
                    format!("{}-handle", kind.prefix()).into(),
                    id as u64,
                ))
                .debug_selector(move || handle_selector(larger, id))
                .role(Role::Button)
                .aria_label(SharedString::from(i18n.t_with(
                    "bulk.panel.dragHandleAria",
                    &Params::new().with("position", position),
                )))
                .key_context(HANDLE_KEY_CONTEXT)
                .relative()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(measures::HANDLE_SIZE))
                .rounded(px(RADII.control))
                .child(
                    Icon::new(IconName::GripVertical)
                        .size(measures::HANDLE_ICON_SIZE)
                        .color(color(colors.base_content)),
                );
            let grip = if blocked {
                grip.opacity(measures::LOCKED_HANDLE_OPACITY)
                    .cursor_not_allowed()
            } else {
                grip.track_focus(&tile.handle)
                    .cursor(CursorStyle::OpenHand)
                    .hover(move |style| style.bg(color(hover)))
                    .when(focus_visible, |grip| {
                        grip.child(focus_ring(
                            FocusRing::base(tokens),
                            RADII.control,
                            RingBorder::NONE,
                        ))
                    })
                    .on_action(cx.listener(move |this, _: &MoveLeft, window, cx| {
                        this.board_key(kind, id, BoardKey::Left, window, cx)
                    }))
                    .on_action(cx.listener(move |this, _: &MoveRight, window, cx| {
                        this.board_key(kind, id, BoardKey::Right, window, cx)
                    }))
                    .on_action(cx.listener(move |this, _: &MoveUp, window, cx| {
                        this.board_key(kind, id, BoardKey::Up, window, cx)
                    }))
                    .on_action(cx.listener(move |this, _: &MoveDown, window, cx| {
                        this.board_key(kind, id, BoardKey::Down, window, cx)
                    }))
            };
            let caption = div()
                .flex()
                .flex_none()
                .items_center()
                .h(px(measures::CAPTION_HEIGHT))
                .gap(px(measures::CAPTION_GAP))
                .px(px(measures::CAPTION_PADDING_X))
                .border_t_1()
                .border_color(color(colors.border_soft))
                .font_family(fonts::MONO)
                .text_size(px(TYPOGRAPHY.xs.size))
                .child(
                    div()
                        .flex_none()
                        .font_weight(weight(TYPOGRAPHY.weight_semibold))
                        .text_color(color(colors.primary_text))
                        .child(position.to_string()),
                )
                .child(div().flex_1().min_w_0().truncate().child(tile.name))
                .child(grip);
            content.push(
                div()
                    .id(ElementId::NamedInteger(
                        format!("{}-tile", kind.prefix()).into(),
                        id as u64,
                    ))
                    .debug_selector(move || tile_selector(larger, id))
                    .role(Role::ListItem)
                    .aria_position_in_set(position)
                    .aria_size_of_set(count)
                    .absolute()
                    .left(px(rect.left))
                    .top(px(rect.top))
                    .w(px(rect.width))
                    .h(px(rect.height))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(RADII.control))
                    .border_1()
                    .border_color(color(
                        colors
                            .base_content
                            .with_alpha_percent(measures::BORDER_PERCENT),
                    ))
                    .bg(color(colors.base_200))
                    .when(dragged == Some(id), |tile| {
                        tile.opacity(measures::DRAGGED_OPACITY)
                    })
                    .map(|tile| {
                        if blocked {
                            tile.cursor_not_allowed()
                        } else {
                            tile.cursor(CursorStyle::OpenHand).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    this.board_press(kind, id, event, window, cx)
                                }),
                            )
                        }
                    })
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .h(px(layout.preview_height))
                            .child(thumbnail((kind.prefix(), id), tile.picture, theme)),
                    )
                    .child(caption)
                    .into_any_element(),
            );
        }
        let index_of = |id: i64| order.iter().position(|other| *other == id);
        if let Some(line) = dragged
            .and_then(index_of)
            .zip(target.and_then(index_of))
            .and_then(|(dragged, target)| layout.insertion_line(dragged, target))
        {
            content.push(
                div()
                    .debug_selector(move || insertion_line_selector(larger))
                    .absolute()
                    .left(px(line.left))
                    .top(px(line.top))
                    .w(px(measures::LINE_WIDTH))
                    .h(px(line.height))
                    .rounded_full()
                    .bg(color(colors.primary))
                    .into_any_element(),
            );
        }
        // Drawn over the window, so nothing of the dock or the dialog can clip it.
        let ghost = dragged.zip(pointer).map(|(id, (x, y))| {
            deferred(
                gpui::anchored()
                    .position(point(
                        px(x + measures::GHOST_OFFSET),
                        px(y + measures::GHOST_OFFSET),
                    ))
                    .child(
                        div()
                            .relative()
                            .size(px(measures::GHOST_SIZE))
                            .overflow_hidden()
                            .rounded(px(RADII.control))
                            .border(px(measures::GHOST_BORDER))
                            .border_color(color(colors.primary))
                            .bg(color(colors.base_200))
                            .opacity(measures::GHOST_OPACITY)
                            .shadow(box_shadows(tokens.shadows.popover))
                            .child(thumbnail(
                                ("bulk-order-ghost", id),
                                ghost_picture.clone().unwrap_or(TilePicture::Empty),
                                theme,
                            )),
                    ),
            )
            .with_priority(3)
        });

        let state = self.board_state(kind);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id(kind.prefix())
                    .debug_selector(move || grid_selector(larger))
                    .role(Role::List)
                    .aria_label(SharedString::from(i18n.t("bulk.panel.orderHeading")))
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&state.scroll)
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(layout.total_height()))
                            .children(content),
                    ),
            )
            .child(measure(state.bounds.clone(), cx.entity().downgrade()))
            .children(ghost)
            .into_any_element()
    }
}

/// The thumbnail of an order tile, whole inside its box (`object-contain`), or the
/// delayed loader while it is on its way.
fn thumbnail(id: (&'static str, i64), picture: TilePicture, theme: ThemeName) -> AnyElement {
    let colors = &theme.tokens().colors;
    match picture {
        TilePicture::Ready(picture) => img(picture)
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        TilePicture::Loading => {
            let loader = ElementId::NamedInteger(id.0.into(), id.1 as u64);
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(color(
                    colors
                        .base_100
                        .with_alpha_percent(measures::LOADER_VEIL_PERCENT),
                ))
                .child(Spinner::new(loader.clone(), SpinnerSize::Tile).theme(theme))
                .with_animation(
                    (loader, "veil"),
                    Animation::new(THUMBNAIL_LOADER.total()),
                    |veil, progress| veil.opacity(THUMBNAIL_LOADER.opacity_at_progress(progress)),
                )
                .into_any_element()
        }
        TilePicture::Empty => div().into_any_element(),
    }
}
