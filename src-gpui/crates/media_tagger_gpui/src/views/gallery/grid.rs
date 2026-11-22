//! What the gallery draws below its header: the tiles of the rendered rows, the status
//! footer and the load error banner (`GalleryGrid.tsx`, `GalleryTile.tsx`,
//! `GalleryStatusFooter.tsx`).
//!
//! Everything here is drawing: where a tile is and what it asks for are decided by
//! `media_tagger_client::gallery` (the scroll state, the geometry, the thumbnail
//! demand). A tile is a button named after its file; videos and GIFs show their static
//! thumbnail with a chip (plan decision D8).

use std::{cell::Cell, rc::Rc};

use gpui::{
    actions, div, img, prelude::*, px, Animation, AnimationExt as _, AnyElement, App, ClickEvent,
    Div, ElementId, KeyBinding, MouseButton, ObjectFit, Role, SharedString, Stateful, Toggled,
    Window,
};
use media_tagger_client::{
    controls::{
        button_visuals, ButtonState, ButtonVariant, FocusRing, SpinnerSize, THUMBNAIL_LOADER,
    },
    gallery::layout::{
        error_banner, footer, placeholder_pulse_opacity, result_count, tile as tile_layout,
        MediaBadge, PLACEHOLDER_PULSE,
    },
    i18n::I18n,
    icons::IconName,
    shortcuts::{ShortcutCommand, SHORTCUTS},
    theme::{Shadow, ThemeName, RADII, TYPOGRAPHY},
};
use media_tagger_core::models::AssetSummary;

use crate::{
    thumbs::TilePicture,
    ui::{
        button::keyed_focus_handle,
        chip::Chip,
        fonts,
        icon::Icon,
        spinner::Spinner,
        theme::{box_shadow, box_shadows, color, focus_ring, weight},
    },
};

/// Debug selectors (`VisualTestContext::debug_bounds`).
pub const VIEWPORT_SELECTOR: &str = "gallery-scroll";
pub const FOOTER_SELECTOR: &str = "gallery-end-state";
pub const LOAD_ERROR_SELECTOR: &str = "gallery-load-error";
pub const LOAD_RETRY_ID: &str = "gallery-load-retry";
/// The rectangle of a drag selection (`data-testid="gallery-selection-rectangle"`).
pub const SELECTION_RECTANGLE_SELECTOR: &str = "gallery-selection-rectangle";

/// The debug selector of the tile at global index `index`.
pub fn tile_selector(index: usize) -> String {
    format!("gallery-tile-{index}")
}

/// The debug selector of the slot at `index` whose asset is not loaded.
pub fn placeholder_selector(index: usize) -> String {
    format!("gallery-placeholder-{index}")
}

/// The debug selector of the picture in the tile at `index`.
pub fn picture_selector(index: usize) -> String {
    format!("gallery-tile-picture-{index}")
}

/// The debug selector of the loader in the tile at `index`.
pub fn loader_selector(index: usize) -> String {
    format!("gallery-tile-loader-{index}")
}

/// The debug selector of the media chip in the tile at `index`.
pub fn chip_selector(index: usize) -> String {
    format!("gallery-tile-chip-{index}")
}

/// The debug selector of the check mark on the selected tile at `index`
/// (`.gallery-selection-mark`).
pub fn selection_mark_selector(index: usize) -> String {
    format!("gallery-selection-mark-{index}")
}

/// The debug selector of the media-group backplate whose first tile is at `index`.
pub fn group_plate_selector(index: usize) -> String {
    format!("gallery-group-plate-{index}")
}

actions!(
    gallery,
    [
        /// Home: the start of the result.
        ScrollStart,
        /// End: the end of the result.
        ScrollEnd,
        PageUp,
        PageDown,
        /// Arrow up: a short step.
        LineUp,
        /// Arrow down: a short step.
        LineDown,
        /// Ctrl+A: every result of the query, in selection mode.
        SelectAll,
    ]
);

/// The gallery's entries of the shortcut registry as GPUI bindings.
pub fn key_bindings() -> Vec<KeyBinding> {
    SHORTCUTS
        .iter()
        .map(|shortcut| {
            let context = Some(shortcut.context.name());
            match shortcut.command {
                ShortcutCommand::GalleryScrollStart => {
                    KeyBinding::new(shortcut.keystroke, ScrollStart, context)
                }
                ShortcutCommand::GalleryScrollEnd => {
                    KeyBinding::new(shortcut.keystroke, ScrollEnd, context)
                }
                ShortcutCommand::GalleryPageUp => {
                    KeyBinding::new(shortcut.keystroke, PageUp, context)
                }
                ShortcutCommand::GalleryPageDown => {
                    KeyBinding::new(shortcut.keystroke, PageDown, context)
                }
                ShortcutCommand::GalleryLineUp => {
                    KeyBinding::new(shortcut.keystroke, LineUp, context)
                }
                ShortcutCommand::GalleryLineDown => {
                    KeyBinding::new(shortcut.keystroke, LineDown, context)
                }
                ShortcutCommand::GallerySelectAll => {
                    KeyBinding::new(shortcut.keystroke, SelectAll, context)
                }
            }
        })
        .collect()
}

/// Binds the gallery's keys. Part of `views::init`.
pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

/// What the gallery asks of whoever shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryEvent {
    /// A tile was activated outside selection mode, or held for a long press in it:
    /// open this asset in the lightbox (epic E16). `index` is its global index in the
    /// result at that moment.
    OpenAsset { asset_id: i64, index: usize },
}

/// The ring of a selected tile or slot: `0 0 0 2px var(--color-primary)`.
fn selection_ring(theme: ThemeName) -> Shadow {
    Shadow::ring(
        false,
        tile_layout::SELECTION_RING_WIDTH,
        theme.tokens().colors.primary,
    )
}

/// One tile: the thumbnail, the delayed loader while it is on its way, the chip of a
/// video or a GIF and, when it is selected, the primary ring and the check mark.
#[allow(clippy::too_many_arguments)]
pub fn tile(
    asset: &AssetSummary,
    index: usize,
    size: f32,
    picture: TilePicture,
    selected: bool,
    i18n: &I18n<'_>,
    theme: ThemeName,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let tokens = theme.tokens();
    let colors = &tokens.colors;
    let fill = colors
        .base_300
        .with_alpha_percent(tile_layout::FILL_PERCENT);
    let hover_shadow = box_shadows(tokens.shadows.tile_hover);
    let inner_radius = (RADII.tile - 1.0).max(0.0);
    let tile = div()
        .id(("gallery-tile", index))
        .debug_selector(|| tile_selector(index))
        .role(Role::Button)
        // The file name only: `preview_path` is the full source path of videos and GIFs.
        .aria_label(SharedString::from(asset.file_name.clone()))
        // `aria-pressed`: whether the tile is selected.
        .aria_toggled(if selected {
            Toggled::True
        } else {
            Toggled::False
        })
        .absolute()
        .size(px(size))
        .rounded(px(RADII.tile))
        .border_1()
        .border_color(color(colors.border_soft))
        .bg(color(fill))
        .overflow_hidden()
        .on_click(on_click);
    // A selected tile keeps the hover edge under its ring
    // (`shadow-[0_0_0_2px_var(--color-primary),var(--shadow-tile-hover)]`); the ring is
    // drawn last, so it lies on top.
    let tile = if selected {
        let mut shadows = hover_shadow;
        shadows.push(box_shadow(&selection_ring(theme)));
        tile.shadow(shadows)
    } else {
        tile.hover(move |style| style.shadow(hover_shadow.clone()))
    };
    let tile = match picture {
        TilePicture::Ready(picture) => tile.child(
            img(picture)
                .debug_selector(|| picture_selector(index))
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(px(inner_radius))
                .object_fit(ObjectFit::Cover),
        ),
        // Invisible for a moment, so a picture that is already cached never flashes
        // a spinner. The id is the asset's: a slot that scrolls to another asset
        // starts the delay again.
        TilePicture::Loading => {
            let loader = ElementId::NamedInteger("gallery-tile-loader".into(), asset.id as u64);
            tile.child(
                div()
                    .debug_selector(|| loader_selector(index))
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(inner_radius))
                    .bg(color(
                        colors
                            .base_100
                            .with_alpha_percent(tile_layout::LOADER_VEIL_PERCENT),
                    ))
                    .child(Spinner::new(loader.clone(), SpinnerSize::Tile).theme(theme))
                    .with_animation(
                        (loader, "veil"),
                        Animation::new(THUMBNAIL_LOADER.total()),
                        |veil, progress| {
                            veil.opacity(THUMBNAIL_LOADER.opacity_at_progress(progress))
                        },
                    ),
            )
        }
        TilePicture::Empty => tile,
    };
    let tile = if selected {
        tile.child(
            div()
                .debug_selector(|| selection_mark_selector(index))
                .absolute()
                .left(px(tile_layout::SELECTION_MARK_INSET))
                .top(px(tile_layout::SELECTION_MARK_INSET))
                .size(px(tile_layout::SELECTION_MARK_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RADII.tile))
                .bg(color(colors.primary))
                .child(
                    Icon::new(IconName::CheckSquare)
                        .size(tile_layout::SELECTION_MARK_ICON_SIZE)
                        .color(color(colors.primary_content)),
                ),
        )
    } else {
        tile
    };
    match MediaBadge::of_kind(&asset.kind) {
        Some(badge) => tile.child(
            div()
                .debug_selector(|| chip_selector(index))
                .absolute()
                .right(px(tile_layout::CHIP_INSET))
                .bottom(px(tile_layout::CHIP_INSET))
                .child(
                    Chip::new(i18n.t(badge.label_key()))
                        .tone(badge.tone())
                        .theme(theme),
                ),
        ),
        None => tile,
    }
}

/// A slot whose asset is not loaded yet. It pulses unless fast scrolling paused
/// demand, when nothing is on its way. Inside a selected range it has the ring of a
/// selected tile (`ring-2 ring-primary`).
pub fn placeholder(
    index: usize,
    size: f32,
    pulsing: bool,
    selected: bool,
    theme: ThemeName,
) -> AnyElement {
    let colors = &theme.tokens().colors;
    let slot = div()
        .debug_selector(|| placeholder_selector(index))
        .absolute()
        .size(px(size))
        .rounded(px(RADII.surface))
        .bg(color(
            colors
                .base_300
                .with_alpha_percent(tile_layout::PLACEHOLDER_FILL_PERCENT),
        ))
        // A ring around the slot, not a shadow: GPUI draws a shadow beneath the whole
        // element, and the slot's fill lets it through.
        .when(selected, |slot| {
            slot.child(focus_ring(
                FocusRing {
                    color: colors.primary,
                    width: tile_layout::SELECTION_RING_WIDTH,
                    offset: 0.0,
                },
                RADII.surface,
            ))
        });
    if pulsing {
        slot.with_animation(
            ("gallery-placeholder", index),
            Animation::new(PLACEHOLDER_PULSE).repeat_synced(),
            |slot, progress| slot.opacity(placeholder_pulse_opacity(progress)),
        )
        .into_any_element()
    } else {
        slot.into_any_element()
    }
}

/// The backplate behind a run of `lanes` adjacent tiles of one media group.
pub fn group_plate(lanes: usize, size: f32, gap: f32, theme: ThemeName) -> Div {
    let colors = &theme.tokens().colors;
    let outset = tile_layout::GROUP_PLATE_OUTSET;
    let width = lanes as f32 * size + lanes.saturating_sub(1) as f32 * gap + 2.0 * outset;
    div()
        .absolute()
        .w(px(width))
        .h(px(size + 2.0 * outset))
        .rounded(px(RADII.surface + outset))
        .bg(color(colors.gallery_group_bg))
        .border_1()
        .border_color(color(colors.gallery_group_edge))
}

/// The end of the result with its count (`GalleryStatusFooter`).
pub fn status_footer(total: usize, i18n: &I18n<'_>, theme: ThemeName) -> Div {
    let colors = &theme.tokens().colors;
    let rule = || {
        div()
            .h(px(1.0))
            .flex_1()
            .min_w_0()
            .bg(color(colors.border_soft))
    };
    div()
        .debug_selector(|| FOOTER_SELECTOR.to_owned())
        .absolute()
        .left_0()
        .right_0()
        .h(px(footer::HEIGHT))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(footer::GAP))
        .px(px(footer::PADDING_X))
        .py(px(footer::PADDING_Y))
        .child(
            div()
                .flex()
                .items_center()
                .w_full()
                .max_w(px(footer::RULE_MAX_WIDTH))
                .gap(px(footer::RULE_GAP))
                .child(rule())
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(footer::MARK_SIZE))
                        .rounded_full()
                        .bg(color(colors.surface_raised))
                        .child(
                            Icon::new(IconName::CheckCircle)
                                .size(footer::MARK_ICON_SIZE)
                                .color(color(colors.success)),
                        ),
                )
                .child(rule()),
        )
        .child(
            div()
                .text_size(px(footer::TITLE_SIZE))
                .line_height(px(footer::TITLE_LINE_HEIGHT))
                .font_weight(weight(TYPOGRAPHY.weight_semibold))
                .text_color(color(colors.base_content))
                .child(i18n.t("gallery.endOfResults")),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(footer::GAP))
                .font_family(fonts::MONO)
                .text_size(px(footer::COUNT_SIZE))
                .line_height(px(footer::COUNT_LINE_HEIGHT))
                .text_color(color(colors.text_muted))
                .child(
                    div()
                        .size(px(footer::DOT_SIZE))
                        .flex_none()
                        .rounded_full()
                        .bg(color(colors.success)),
                )
                .child(result_count(i18n, total)),
        )
}

/// The rectangle of a drag selection, `width` by `height`
/// (`border border-primary bg-primary/20`). It takes no input.
pub fn selection_rectangle(width: f32, height: f32, theme: ThemeName) -> Div {
    let primary = theme.tokens().colors.primary;
    div()
        .debug_selector(|| SELECTION_RECTANGLE_SELECTOR.to_owned())
        .absolute()
        .w(px(width))
        .h(px(height))
        .border_1()
        .border_color(color(primary))
        .bg(color(primary.with_alpha_percent(
            tile_layout::SELECTION_RECTANGLE_FILL_PERCENT,
        )))
}

/// The banner of a failed load with its Retry button (`role="alert"`).
/// `control_pressed` is set when a primary press lands on the button: the gallery
/// starts no selection gesture from a press on a control.
pub fn load_error_banner(
    i18n: &I18n<'_>,
    theme: ThemeName,
    control_pressed: Rc<Cell<bool>>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let colors = &theme.tokens().colors;
    div()
        .id("gallery-load-error")
        .debug_selector(|| LOAD_ERROR_SELECTOR.to_owned())
        .role(Role::Alert)
        .h(px(error_banner::HEIGHT))
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .gap(px(error_banner::GAP))
        .px(px(error_banner::PADDING_X))
        .rounded(px(RADII.surface))
        .border_1()
        .border_color(color(
            colors
                .error
                .with_alpha_percent(error_banner::BORDER_PERCENT),
        ))
        .bg(color(
            colors.error.with_alpha_percent(error_banner::FILL_PERCENT),
        ))
        .text_size(px(TYPOGRAPHY.sm.size))
        .child(i18n.t("gallery.loadError"))
        .child(RetryButton {
            label: SharedString::from(i18n.t("gallery.retry")),
            theme,
            control_pressed,
            on_click: Box::new(on_retry),
        })
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The banner's Retry button (`btn btn-xs btn-outline`). Like the kit's buttons it is a
/// tab stop that Enter and Space activate (GPUI's keyboard click), with the focus
/// handle in its element state and the button outline while the keyboard focused it.
#[derive(IntoElement)]
struct RetryButton {
    label: SharedString,
    theme: ThemeName,
    control_pressed: Rc<Cell<bool>>,
    on_click: ClickHandler,
}

impl RenderOnce for RetryButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = self.theme.tokens();
        let content = tokens.colors.base_content;
        let inverse = tokens.colors.base_100;
        let ring =
            button_visuals(tokens, ButtonVariant::Secondary, ButtonState::default()).focus_ring;
        let id = ElementId::from(LOAD_RETRY_ID);
        let focus_handle = keyed_focus_handle(&id, window, cx);
        let focus_visible = focus_handle.is_focused(window) && window.last_input_was_keyboard();
        div()
            .id(id)
            .debug_selector(|| LOAD_RETRY_ID.to_owned())
            .role(Role::Button)
            .track_focus(&focus_handle)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .h(px(error_banner::BUTTON_HEIGHT))
            .px(px(error_banner::BUTTON_PADDING_X))
            .rounded(px(RADII.field))
            .border_1()
            .border_color(color(content))
            .text_color(color(content))
            .text_size(px(error_banner::BUTTON_FONT_SIZE))
            .font_weight(weight(TYPOGRAPHY.weight_semibold))
            .cursor_pointer()
            .hover(move |style| style.bg(color(content)).text_color(color(inverse)))
            // Seen before the viewport's own press handler, which lies beneath.
            .on_mouse_down(MouseButton::Left, {
                let control_pressed = self.control_pressed;
                move |_, _, _| control_pressed.set(true)
            })
            .on_click(self.on_click)
            .when(focus_visible, |button| {
                button.child(focus_ring(ring, RADII.field))
            })
            .child(self.label)
    }
}
