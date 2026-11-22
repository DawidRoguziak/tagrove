//! `UiModal`: a dialog over a dimmed backdrop, one layer of the layer stack.
//!
//! The owner keeps a [`ModalState`] for as long as it lives and renders a [`Modal`]
//! with the content every frame, open or not, as Solid owners keep `UiModal` mounted:
//!
//! - **Open**: [`ModalState::set_open`] registers a modal layer. The dialog fades in
//!   (opacity only, `--motion-entrance`) and takes the focus one frame later.
//! - **Close request**: Escape (only while it is the top layer) or a press on the
//!   backdrop (only while open, on top, and `close_on_overlay_click`) emit
//!   [`ModalEvent::Dismiss`]. The owner decides and calls `set_open(false)`.
//! - **Close**: the layer is released at once, so the focus goes back one frame later,
//!   while the dialog fades out (`--motion-exit`). It stays drawn and blocks the pointer
//!   until the fade ends, but takes no input: a focus inside it moves to the window's
//!   root at once (`inert`). Owners show the values it had open with
//!   `media_tagger_client::overlay::ModalSnapshot`.
//!
//! Render the `Modal` inside the window's [`super::layers::root`]; it draws itself over
//! the whole window.

use std::time::Duration;

use gpui::{
    anchored, deferred, div, point, prelude::*, px, AnyElement, App, Context, ElementId, Entity,
    EventEmitter, FocusHandle, Focusable, MouseButton, Role, SharedString, Task, Window,
};
use media_tagger_client::{
    overlay::{LayerId, ModalMotion, ModalPresence, ModalSize, Phase},
    theme::{ThemeName, RADII},
};

use super::{
    layers::{self, LayerOptions},
    theme::{self, box_shadows, color},
};

/// The key context of an open dialog's content.
pub const KEY_CONTEXT: &str = "UiModal";

/// What a modal asks of its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalEvent {
    /// Escape or a press on the backdrop: the owner's `onClose`.
    Dismiss,
}

pub struct ModalState {
    presence: ModalPresence,
    layer: Option<LayerId>,
    close_on_escape: bool,
    close_on_overlay_click: bool,
    focus_handle: FocusHandle,
    initial_focus: Option<FocusHandle>,
    restore_focus: Option<FocusHandle>,
    origin: std::time::Instant,
    timer: Option<Task<()>>,
}

impl EventEmitter<ModalEvent> for ModalState {}

impl Focusable for ModalState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalState {
    /// A closed modal with the motion of every dialog except the lightbox.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::with_motion(ModalMotion::DEFAULT, cx)
    }

    pub fn with_motion(motion: ModalMotion, cx: &mut Context<Self>) -> Self {
        cx.on_release(|state: &mut Self, cx| {
            if let Some(layer) = state.layer.take() {
                layers::forget(layer, cx);
            }
        })
        .detach();
        let origin = cx.background_executor().now();
        Self {
            presence: ModalPresence::new(false, cx.reduce_motion(), motion, Duration::ZERO),
            layer: None,
            close_on_escape: true,
            close_on_overlay_click: true,
            focus_handle: cx.focus_handle(),
            initial_focus: None,
            restore_focus: None,
            origin,
            timer: None,
        }
    }

    fn now(&self, cx: &App) -> Duration {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
    }

    pub fn is_open(&self) -> bool {
        self.presence.is_open()
    }

    pub fn phase(&self) -> Phase {
        self.presence.phase()
    }

    /// Open, or still fading out.
    pub fn is_present(&self) -> bool {
        self.presence.is_present()
    }

    pub fn layer(&self) -> Option<LayerId> {
        self.layer
    }

    /// The opacity the backdrop and dialog are drawn with now.
    pub fn opacity(&self, cx: &App) -> f32 {
        self.presence.opacity(self.now(cx))
    }

    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.presence.is_open() == open {
            return;
        }
        self.presence.set_reduced_motion(cx.reduce_motion());
        let now = self.now(cx);
        let timer = self.presence.set_open(open, now);
        self.timer = timer.map(|(generation, duration)| {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(duration).await;
                this.update(cx, |state, cx| {
                    if state.presence.finish(generation) {
                        cx.notify();
                    }
                })
                .ok();
            })
        });
        if open {
            let this = cx.entity().downgrade();
            let mut options = LayerOptions::modal(self.focus_handle.clone(), move |_, cx| {
                this.update(cx, |_, cx| cx.emit(ModalEvent::Dismiss)).ok();
            });
            options.close_on_escape = self.close_on_escape;
            options.initial_focus = self.initial_focus.clone();
            self.layer = Some(layers::register(options, window, cx));
        } else if let Some(layer) = self.layer.take() {
            // The closing content is `inert`: the focus leaves it at once, as a browser
            // moves it to the body, so no key reaches the dialog while it fades out.
            // The restore one frame later moves it on from the root.
            if self.focus_handle.contains_focused(window, cx) {
                layers::focus_root(window, cx);
            }
            layers::release(layer, self.restore_focus.clone(), window, cx);
        }
        cx.notify();
    }

    /// `closeOnEscape` (default true).
    pub fn set_close_on_escape(&mut self, close_on_escape: bool, cx: &mut Context<Self>) {
        self.close_on_escape = close_on_escape;
        if let Some(layer) = self.layer {
            layers::set_close_on_escape(layer, close_on_escape, cx);
        }
    }

    /// `closeOnOverlayClick` (default true).
    pub fn set_close_on_overlay_click(&mut self, close: bool) {
        self.close_on_overlay_click = close;
    }

    /// The element focused when the dialog opens (`data-initial-focus`); without one its
    /// first tab stop, else the dialog itself.
    pub fn set_initial_focus(&mut self, focus: Option<FocusHandle>) {
        self.initial_focus = focus;
    }

    /// `getRestoreFocus`: where the focus goes when the dialog closes, instead of the
    /// element that had it when it opened.
    pub fn set_restore_focus(&mut self, focus: Option<FocusHandle>) {
        self.restore_focus = focus;
    }

    fn backdrop_pressed(&mut self, cx: &mut Context<Self>) {
        let on_top = self.layer.is_some_and(|layer| layers::is_top(layer, cx));
        if self.presence.is_open() && self.close_on_overlay_click && on_top {
            cx.emit(ModalEvent::Dismiss);
        }
    }
}

/// The dialog surface of `UiModal`: `rounded-[var(--radius-panel)] border
/// border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-modal)]
/// sm:p-6`.
pub fn modal_surface(theme: ThemeName, viewport_width: f32) -> gpui::Div {
    let tokens = theme.tokens();
    div()
        .rounded(px(RADII.panel))
        .border_1()
        .border_color(color(tokens.colors.border_soft))
        .bg(color(tokens.colors.surface_solid))
        .p(px(ModalSize::content_padding(viewport_width)))
        .shadow(box_shadows(tokens.shadows.modal))
}

/// The backdrop colour: `bg-neutral/58`.
pub fn backdrop_color(theme: ThemeName) -> gpui::Hsla {
    color(theme.tokens().colors.neutral.with_alpha_percent(58))
}

/// The element of a dialog. Render it every frame; it draws nothing while the state is
/// hidden.
#[derive(IntoElement)]
pub struct Modal {
    id: ElementId,
    state: Entity<ModalState>,
    label: Option<SharedString>,
    size: ModalSize,
    height: Option<f32>,
    theme: Option<ThemeName>,
    children: Vec<AnyElement>,
}

impl Modal {
    pub fn new(id: impl Into<ElementId>, state: &Entity<ModalState>) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            label: None,
            size: ModalSize::default(),
            height: None,
            theme: None,
            children: Vec::new(),
        }
    }

    /// The accessible name of the dialog (`ariaLabel`).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn size(mut self, size: ModalSize) -> Self {
        self.size = size;
        self
    }

    pub fn theme(mut self, theme: ThemeName) -> Self {
        self.theme = Some(theme);
        self
    }

    /// A fixed height of the dialog, padding included, in place of its content's
    /// (`h-[90vh]` in `contentClassName`).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }
}

impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Modal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.read(cx);
        if !state.is_present() {
            return div().into_any_element();
        }
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let open = state.is_open();
        let opacity = state.opacity(cx);
        if !state.presence.is_settled(state.now(cx)) {
            window.request_animation_frame();
        }
        let focus_handle = state.focus_handle.clone();
        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);

        let content = modal_surface(theme, width)
            .id((self.id.clone(), "dialog"))
            .relative()
            .w(px(self.size.width(width)))
            .when_some(self.height, |content, height| content.h(px(height)))
            .key_context(KEY_CONTEXT)
            .track_focus(&focus_handle)
            // A press inside the dialog never reaches the backdrop.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when(open, |content| {
                let content = content.role(Role::Dialog);
                match self.label.clone() {
                    Some(label) => content.aria_label(label),
                    None => content,
                }
            })
            .children(self.children)
            // Fading out: still drawn, no longer interactive (`inert`).
            .when(!open, |content| {
                content.child(div().id("inert").absolute().inset_0().occlude())
            });

        let state = self.state.clone();
        let overlay = div()
            .id(self.id)
            .occlude()
            .w(viewport.width)
            .h(viewport.height)
            .flex()
            .items_center()
            .justify_center()
            .p(px(ModalSize::overlay_padding(width)))
            .bg(backdrop_color(theme))
            .opacity(opacity)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                state.update(cx, |state, cx| state.backdrop_pressed(cx));
            })
            .child(content);
        deferred(anchored().position(point(px(0.), px(0.))).child(overlay))
            .with_priority(1)
            .into_any_element()
    }
}
