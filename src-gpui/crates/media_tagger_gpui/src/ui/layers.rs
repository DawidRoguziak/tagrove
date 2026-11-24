//! The layer stack of the window (`UiLayerProvider`): every open overlay registers a
//! layer, Escape closes only the top one, a modal keeps Tab inside itself, takes the
//! focus when it opens and gives it back when it closes. The decisions are
//! `media_tagger_client::overlay::LayerStack`; this module keeps the focus handles and
//! the Escape callbacks per layer in a GPUI global and routes the keys.
//!
//! # Keys
//!
//! [`init`] binds Escape, Tab and Shift+Tab in the key context [`KEY_CONTEXT`], which
//! [`root`] puts on the root element of a window together with their handlers. Every
//! other context is deeper, so a focused text field or control keeps the keys it binds
//! itself; a key it does not handle (Escape in a text field, `text_input::Cancel`) falls
//! through to these bindings. Escape that closes nothing goes on to the next binding;
//! Tab moves to the next tab stop of the window, or of the top modal (wrapping inside it).
//! The tab stops of a released modal that is still drawn (a dialog that fades out) are
//! passed over, as a browser passes over `inert` content.
//!
//! # Focus
//!
//! The root element tracks a focus handle of its own, so a click on an empty part of the
//! window focuses the root, as a click on the page body does in a browser. "The focus is
//! still somewhere" means: on an element drawn inside the root, other than the root. A
//! window keeps its focus at least on the root ([`focus_root`]): with the focus nowhere,
//! GPUI dispatches keys to a node above every element, which no binding of the
//! application reaches.

use std::{collections::HashMap, rc::Rc};

use gpui::{
    actions, div, prelude::*, App, Div, FocusHandle, Global, KeyBinding, WeakFocusHandle, Window,
};
use media_tagger_client::overlay::{Escape, LayerId, LayerStack, Release, RestoreAction};

/// The key context of a window's root element.
pub const KEY_CONTEXT: &str = "UiLayers";

actions!(
    ui_layers,
    [
        /// Escape: closes the top layer.
        Dismiss,
        /// Tab: the next tab stop, inside the top modal if there is one.
        FocusNext,
        /// Shift+Tab: the previous tab stop.
        FocusPrevious,
    ]
);

/// The key bindings of the layer stack, all in [`KEY_CONTEXT`].
pub fn key_bindings() -> Vec<KeyBinding> {
    let context = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("escape", Dismiss, context),
        KeyBinding::new("tab", FocusNext, context),
        KeyBinding::new("shift-tab", FocusPrevious, context),
    ]
}

type EscapeHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// How a layer registers.
pub struct LayerOptions {
    /// A modal makes everything below it inert: it takes the focus, keeps Tab inside
    /// its container and keeps the restore of a layer released beneath it.
    pub modal: bool,
    pub close_on_escape: bool,
    /// The element the layer draws in. For a modal: focused when it has no tab stop,
    /// and the boundary of Tab.
    pub container: Option<FocusHandle>,
    /// The element a modal focuses when it opens (`data-initial-focus`); else its first
    /// tab stop.
    pub initial_focus: Option<FocusHandle>,
    /// Called for Escape while the layer is on top (`onEscape`).
    pub on_escape: EscapeHandler,
}

impl LayerOptions {
    pub fn modal(
        container: FocusHandle,
        on_escape: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            modal: true,
            close_on_escape: true,
            container: Some(container),
            initial_focus: None,
            on_escape: Rc::new(on_escape),
        }
    }

    /// A layer that is not modal, as the settings page and the bulk mode register.
    pub fn nonmodal(on_escape: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self {
            modal: false,
            close_on_escape: true,
            container: None,
            initial_focus: None,
            on_escape: Rc::new(on_escape),
        }
    }
}

struct Entry {
    container: Option<FocusHandle>,
    initial_focus: Option<FocusHandle>,
    /// The element that had the focus when the layer opened.
    previous: Option<WeakFocusHandle>,
    on_escape: EscapeHandler,
}

/// The layers of the application.
pub struct UiLayers {
    stack: LayerStack,
    entries: HashMap<LayerId, Entry>,
    root: FocusHandle,
    /// The containers of released modals. One counts while it is still drawn, which is
    /// as long as its dialog fades out: its tab stops are `inert` then.
    released: Vec<WeakFocusHandle>,
}

impl Global for UiLayers {}

/// Creates the layer stack and binds its keys. Part of `ui::init`.
pub fn init(cx: &mut App) {
    if !cx.has_global::<UiLayers>() {
        let root = cx.focus_handle();
        cx.set_global(UiLayers {
            stack: LayerStack::new(),
            entries: HashMap::new(),
            root,
            released: Vec::new(),
        });
    }
    cx.bind_keys(key_bindings());
}

fn layers(cx: &App) -> &UiLayers {
    cx.try_global::<UiLayers>()
        .expect("ui::init sets up the layer stack before a window opens")
}

/// The root element of a window: the key context and handlers of the layer stack and
/// the root focus handle. Every window's content goes inside it.
pub fn root(cx: &App) -> Div {
    div()
        .track_focus(&layers(cx).root)
        .key_context(KEY_CONTEXT)
        .on_action(|_: &Dismiss, window, cx| {
            if !dismiss_top(window, cx) {
                cx.propagate();
            }
        })
        .on_action(|_: &FocusNext, window, cx| move_focus(true, window, cx))
        .on_action(|_: &FocusPrevious, window, cx| move_focus(false, window, cx))
}

/// The focus handle of the root element.
pub fn root_focus_handle(cx: &App) -> FocusHandle {
    layers(cx).root.clone()
}

/// Focuses the root element. Keys reach the root's bindings only along the focused
/// element's path: a window whose focus is nowhere sends them to GPUI's own root node,
/// above every element. So a window focuses the root when it opens and whenever its
/// focus is lost (`Window::on_focus_lost`), as a page falls back to its body.
pub fn focus_root(window: &mut Window, cx: &mut App) {
    let root = layers(cx).root.clone();
    window.focus(&root, cx);
}

/// `handle` is drawn inside the root (and is not the root itself) in the last frame.
fn is_rendered(handle: &FocusHandle, window: &Window, cx: &App) -> bool {
    let root = &layers(cx).root;
    handle != root && root.contains(handle, window)
}

/// Puts a layer on top. A modal takes the focus one frame later, if it is still on top.
pub fn register(options: LayerOptions, window: &mut Window, cx: &mut App) -> LayerId {
    let previous = window.focused(cx).map(|handle| handle.downgrade());
    let layers = cx.global_mut::<UiLayers>();
    let id = layers
        .stack
        .register(options.modal, options.close_on_escape);
    if let Some(container) = &options.container {
        // A dialog opened again while it fades out takes input again.
        let container = container.downgrade();
        layers.released.retain(|released| *released != container);
    }
    layers.entries.insert(
        id,
        Entry {
            container: options.container,
            initial_focus: options.initial_focus,
            previous,
            on_escape: options.on_escape,
        },
    );
    if options.modal {
        window.on_next_frame(move |window, cx| {
            if cx
                .try_global::<UiLayers>()
                .is_some_and(|layers| layers.stack.initial_focus_due(id))
            {
                focus_initial(id, window, cx);
            }
        });
    }
    window.refresh();
    id
}

/// Removes a layer. One frame later the focus goes back to `restore_focus`, or to the
/// element focused when the layer opened, when the stack says so.
pub fn release(id: LayerId, restore_focus: Option<FocusHandle>, window: &mut Window, cx: &mut App) {
    if !cx.has_global::<UiLayers>() {
        return;
    }
    let layers = cx.global_mut::<UiLayers>();
    let entry = layers.entries.remove(&id);
    if let Some(container) = entry.as_ref().and_then(|entry| entry.container.as_ref()) {
        layers
            .released
            .retain(|released| released.upgrade().is_some());
        layers.released.push(container.downgrade());
    }
    let Release::Pending(pending) = layers.stack.release(id) else {
        return;
    };
    let target = restore_focus
        .map(|handle| handle.downgrade())
        .or_else(|| entry.and_then(|entry| entry.previous));
    window.on_next_frame(move |window, cx| {
        let Some(layers) = cx.try_global::<UiLayers>() else {
            return;
        };
        let target = target.and_then(|target| target.upgrade());
        let focus_kept = window
            .focused(cx)
            .is_some_and(|focused| is_rendered(&focused, window, cx));
        let target_usable = target.as_ref().is_some_and(|target| {
            is_rendered(target, window, cx)
                && match layers.stack.top_modal() {
                    // A modal makes everything outside it inert.
                    Some(modal) => layers
                        .entries
                        .get(&modal)
                        .and_then(|entry| entry.container.as_ref())
                        .is_some_and(|container| container.contains(target, window)),
                    None => true,
                }
        });
        match layers.stack.resolve(&pending, focus_kept, target_usable) {
            RestoreAction::Nothing => {}
            RestoreAction::Restore => {
                if let Some(target) = target {
                    window.focus(&target, cx);
                }
            }
            RestoreAction::FocusInitial(next) => focus_initial(next, window, cx),
        }
    });
    window.refresh();
}

/// Removes a layer whose owner is gone, without moving the focus.
pub fn forget(id: LayerId, cx: &mut App) {
    if cx.has_global::<UiLayers>() {
        let layers = cx.global_mut::<UiLayers>();
        layers.entries.remove(&id);
        layers.stack.release(id);
    }
}

/// `closeOnEscape` of a registered layer.
pub fn set_close_on_escape(id: LayerId, close_on_escape: bool, cx: &mut App) {
    cx.global_mut::<UiLayers>()
        .stack
        .set_close_on_escape(id, close_on_escape);
}

pub fn is_top(id: LayerId, cx: &App) -> bool {
    cx.try_global::<UiLayers>()
        .is_some_and(|layers| layers.stack.is_top(id))
}

/// Whether a modal layer is open (`[role="dialog"][aria-modal="true"]` in the DOM).
pub fn has_modal(cx: &App) -> bool {
    cx.try_global::<UiLayers>()
        .is_some_and(|layers| layers.stack.top_modal().is_some())
}

/// The registered layers from the bottom up.
pub fn layer_ids(cx: &App) -> Vec<LayerId> {
    cx.try_global::<UiLayers>()
        .map(|layers| layers.stack.ids().collect())
        .unwrap_or_default()
}

/// Escape: calls the top layer's handler. Returns false when no layer took the key.
pub fn dismiss_top(window: &mut Window, cx: &mut App) -> bool {
    let handler = {
        let Some(layers) = cx.try_global::<UiLayers>() else {
            return false;
        };
        match layers.stack.escape() {
            Escape::Dismiss(id) => layers.entries.get(&id).map(|entry| entry.on_escape.clone()),
            Escape::NoLayer | Escape::Ignored(_) => None,
        }
    };
    match handler {
        Some(handler) => {
            handler(window, cx);
            true
        }
        None => false,
    }
}

/// The focus goes to a layer's designated element, its first tab stop, or itself.
fn focus_initial(id: LayerId, window: &mut Window, cx: &mut App) {
    let Some(entry) = cx
        .try_global::<UiLayers>()
        .and_then(|layers| layers.entries.get(&id))
    else {
        return;
    };
    let (container, initial) = (entry.container.clone(), entry.initial_focus.clone());
    if let Some(initial) = initial.filter(|initial| is_rendered(initial, window, cx)) {
        window.focus(&initial, cx);
        return;
    }
    let Some(container) = container else {
        return;
    };
    // The container is no tab stop: the next one after it is its first.
    window.focus(&container, cx);
    window.focus_next(cx);
    let inside = window
        .focused(cx)
        .is_some_and(|focused| focused != container && container.contains(&focused, window));
    if !inside {
        window.focus(&container, cx);
    }
}

/// The containers of released modals that the last frame still drew: dialogs that fade
/// out. The ones that are gone are forgotten.
fn fading_containers(window: &Window, cx: &mut App) -> Vec<FocusHandle> {
    let Some(layers) = cx.try_global::<UiLayers>() else {
        return Vec::new();
    };
    if layers.released.is_empty() {
        return Vec::new();
    }
    let drawn: Vec<FocusHandle> = layers
        .released
        .iter()
        .filter_map(|released| released.upgrade())
        .filter(|container| is_rendered(container, window, cx))
        .collect();
    let kept: Vec<WeakFocusHandle> = drawn.iter().map(FocusHandle::downgrade).collect();
    cx.global_mut::<UiLayers>().released = kept;
    drawn
}

/// Tab and Shift+Tab. Inside a modal the focus wraps from its last tab stop to its first
/// and back, and comes back into it from anywhere outside. Without a modal the tab
/// stops of a dialog that fades out are passed over.
fn move_focus(forward: bool, window: &mut Window, cx: &mut App) {
    let step = |window: &mut Window, cx: &mut App| {
        if forward {
            window.focus_next(cx);
        } else {
            window.focus_prev(cx);
        }
    };
    let container = cx.try_global::<UiLayers>().and_then(|layers| {
        let modal = layers.stack.top_modal()?;
        layers.entries.get(&modal)?.container.clone()
    });
    let Some(container) = container else {
        let fading = fading_containers(window, cx);
        if fading.is_empty() {
            step(window, cx);
            return;
        }
        let start = window.focused(cx);
        let mut seen = Vec::new();
        loop {
            step(window, cx);
            let Some(focused) = window.focused(cx) else {
                return;
            };
            let inert = fading
                .iter()
                .any(|container| container == &focused || container.contains(&focused, window));
            if !inert {
                return;
            }
            if seen.contains(&focused) {
                break;
            }
            seen.push(focused);
        }
        // Every tab stop of the window is in a fading dialog: the focus stays put.
        match start {
            Some(start) => window.focus(&start, cx),
            None => focus_root(window, cx),
        }
        return;
    };
    // Walk the window's tab order until a stop inside the modal comes up; the stops
    // outside are inert.
    let mut seen = Vec::new();
    loop {
        step(window, cx);
        let Some(focused) = window.focused(cx) else {
            break;
        };
        if focused != container && container.contains(&focused, window) {
            return;
        }
        if seen.contains(&focused) {
            break;
        }
        seen.push(focused);
    }
    // No tab stop inside: the modal itself keeps the focus.
    window.focus(&container, cx);
}
