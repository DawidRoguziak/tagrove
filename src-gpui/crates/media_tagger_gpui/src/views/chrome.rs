//! The window's own decorations (D14): the three window buttons, the eight resize
//! grips and the drag surface of a header. What is shown and what a gesture asks for
//! is decided by `media_tagger_client::window_chrome`; this module draws it and calls
//! the window.
//!
//! The Tauri window is frameless and always draws these; so does this one, whatever
//! decorations the platform granted. A window manager that keeps its own title bar
//! (X11 without a compositor, a Wayland compositor that only offers server-side
//! decorations) shows both.

use std::{cell::Cell, rc::Rc};

use gpui::{
    div, prelude::*, px, svg, AnyElement, App, CursorStyle, Div, ElementId, MouseButton,
    MouseDownEvent, MouseMoveEvent, ResizeEdge, Role, SharedString, Window,
};
use media_tagger_client::{
    controls::{button_visuals, ButtonState, ButtonVariant},
    i18n::I18n,
    theme::{ThemeName, RADII},
    window_chrome::{
        resize_edges, DragAction, DragSurface, ResizeCursor, ResizeDirection, WindowControl,
        WindowState, CONTROLS_GAP, CONTROLS_LABEL_KEY, CONTROLS_PADDING_LEFT, CONTROL_SIZE,
        EDGE_THICKNESS,
    },
};

use crate::ui::{
    button::keyed_focus_handle,
    theme::{color, focus_ring},
};

/// Debug selector of a window button (`VisualTestContext::debug_bounds`).
pub fn control_selector(control: WindowControl) -> &'static str {
    match control {
        WindowControl::Minimize => "window-minimize",
        WindowControl::ToggleMaximize => "window-toggle-maximize",
        WindowControl::Close => "window-close",
    }
}

/// Debug selector of a resize grip.
pub fn grip_selector(direction: ResizeDirection) -> String {
    format!("window-resize-{}", direction.name())
}

/// The state of `window` as the decorations need it.
pub fn window_state(window: &Window) -> WindowState {
    WindowState {
        maximized: window.is_maximized(),
        fullscreen: window.is_fullscreen(),
    }
}

/// Carries out a window button.
pub fn activate(control: WindowControl, window: &mut Window) {
    match control {
        WindowControl::Minimize => window.minimize_window(),
        WindowControl::ToggleMaximize => window.zoom_window(),
        WindowControl::Close => window.remove_window(),
    }
}

/// `WindowControls`: minimise, maximise or restore, close.
pub fn window_controls(i18n: &I18n<'_>, theme: ThemeName, state: WindowState) -> AnyElement {
    let tokens = theme.tokens();
    div()
        .id("window-controls")
        .role(Role::Group)
        .aria_label(SharedString::from(i18n.t(CONTROLS_LABEL_KEY)))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(CONTROLS_GAP))
        .pl(px(CONTROLS_PADDING_LEFT))
        .border_l_1()
        .border_color(color(tokens.colors.border_soft))
        .children(WindowControl::ALL.into_iter().map(|control| WindowButton {
            control,
            state,
            theme,
            label: SharedString::from(i18n.t(control.label_key(state))),
        }))
        .into_any_element()
}

/// One window button: a ghost `UiButton` in Tauri, so a tab stop with the
/// `:focus-visible` ring that Enter and Space press (GPUI's keyboard click).
#[derive(IntoElement)]
struct WindowButton {
    control: WindowControl,
    state: WindowState,
    theme: ThemeName,
    label: SharedString,
}

impl RenderOnce for WindowButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            control,
            state,
            theme,
            label,
        } = self;
        let tokens = theme.tokens();
        let rest = control.colors(tokens, false);
        let hover = control.colors(tokens, true);
        let ring = button_visuals(tokens, ButtonVariant::Ghost, ButtonState::default()).focus_ring;
        let id = ElementId::from(control_selector(control));
        let focus_handle = keyed_focus_handle(&id, window, cx);
        let focus_visible = focus_handle.is_focused(window) && window.last_input_was_keyboard();
        let group = SharedString::from(format!("{}-group", control_selector(control)));
        div()
            .id(id)
            .debug_selector(|| control_selector(control).to_owned())
            .group(group.clone())
            .role(Role::Button)
            .aria_label(label)
            .track_focus(&focus_handle)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(CONTROL_SIZE))
            .rounded(px(RADII.control))
            .bg(color(rest.background))
            .cursor_pointer()
            .hover(move |style| style.bg(color(hover.background)))
            // A press on a button never starts a window move.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, _| activate(control, window))
            .when(focus_visible, |button| {
                button.child(focus_ring(ring, RADII.control))
            })
            .child(
                svg()
                    .path(control.icon(state).asset_path(false))
                    .size(px(crate::ui::icon::ICON_SIZE))
                    .flex_none()
                    .text_color(color(rest.foreground))
                    .group_hover(group, move |style| {
                        style.text_color(color(hover.foreground))
                    }),
            )
    }
}

fn resize_edge(direction: ResizeDirection) -> ResizeEdge {
    match direction {
        ResizeDirection::North => ResizeEdge::Top,
        ResizeDirection::South => ResizeEdge::Bottom,
        ResizeDirection::East => ResizeEdge::Right,
        ResizeDirection::West => ResizeEdge::Left,
        ResizeDirection::NorthEast => ResizeEdge::TopRight,
        ResizeDirection::NorthWest => ResizeEdge::TopLeft,
        ResizeDirection::SouthEast => ResizeEdge::BottomRight,
        ResizeDirection::SouthWest => ResizeEdge::BottomLeft,
    }
}

fn cursor(cursor: ResizeCursor) -> CursorStyle {
    match cursor {
        ResizeCursor::UpDown => CursorStyle::ResizeUpDown,
        ResizeCursor::LeftRight => CursorStyle::ResizeLeftRight,
        ResizeCursor::UpLeftDownRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeCursor::UpRightDownLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

/// The resize grips of `state`, each absolutely placed over the window's content.
/// A primary press hands the resize to the window manager.
pub fn resize_grips(state: WindowState) -> Vec<AnyElement> {
    resize_edges(state)
        .iter()
        .map(|&direction| {
            let rect = direction.rect();
            let mut grip = div()
                .id(SharedString::from(grip_selector(direction)))
                .debug_selector(move || grip_selector(direction))
                .absolute()
                .cursor(cursor(direction.cursor()))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    window.start_window_resize(resize_edge(direction));
                });
            let inset = |value: Option<f32>| value.map(px);
            if let Some(top) = inset(rect.top) {
                grip = grip.top(top);
            }
            if let Some(right) = inset(rect.right) {
                grip = grip.right(right);
            }
            if let Some(bottom) = inset(rect.bottom) {
                grip = grip.bottom(bottom);
            }
            if let Some(left) = inset(rect.left) {
                grip = grip.left(left);
            }
            if let Some(width) = inset(rect.width) {
                grip = grip.w(width);
            }
            if let Some(height) = inset(rect.height) {
                grip = grip.h(height);
            }
            grip.into_any_element()
        })
        .collect()
}

/// The grips' thickness, for layouts that keep content clear of them.
pub const GRIP_THICKNESS: f32 = EDGE_THICKNESS;

/// Makes `surface` the drag surface of a header (`data-tauri-drag-region`): dragging it
/// with the primary button moves the window, a double click maximises or restores.
/// Interactive children stop the press from reaching it. `state` belongs to the view
/// that owns the header, so a redraw between the press and the move keeps it.
pub fn drag_surface(surface: Div, state: &Rc<Cell<DragSurface>>) -> Div {
    let on_press = state.clone();
    let on_move = state.clone();
    let on_release = state.clone();
    surface
        .on_mouse_down(
            MouseButton::Left,
            move |event: &MouseDownEvent, window, _| {
                let mut surface = on_press.get();
                let action = surface.press(event.click_count);
                on_press.set(surface);
                if action == Some(DragAction::ToggleMaximize) {
                    window.zoom_window();
                }
            },
        )
        .on_mouse_move(move |event: &MouseMoveEvent, window, _| {
            let mut surface = on_move.get();
            if !surface.is_armed() {
                return;
            }
            let action = surface.moved(event.pressed_button == Some(MouseButton::Left));
            on_move.set(surface);
            if action == Some(DragAction::StartMove) {
                window.start_window_move();
            }
        })
        .on_mouse_up(MouseButton::Left, move |_, _, _| {
            let mut surface = on_release.get();
            surface.release();
            on_release.set(surface);
        })
}

/// Keeps a press on `element` from reaching a drag surface around it.
pub fn no_drag<E: InteractiveElement>(element: E) -> E {
    element.on_mouse_down(MouseButton::Left, |_, _: &mut Window, cx: &mut App| {
        cx.stop_propagation()
    })
}
