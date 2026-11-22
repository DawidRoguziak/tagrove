//! The shell of the main window (`App.tsx`): the root of the layer stack, the key
//! context of the application, the view the runtime asks for and the window's resize
//! grips.
//!
//! The shell lives as long as the window and owns what views must not lose: the
//! runtime entity, the focus handle of the settings button (the focus goes back to it
//! when the settings page closes) and the settings page's layer. A view is made when
//! it is shown and dropped when it is left, so it loses its local state, as a Solid
//! view loses its DOM state when it unmounts.

use gpui::{div, prelude::*, Context, Entity, FocusHandle, Focusable, Subscription, Window};
use media_tagger_client::{
    overlay::LayerId,
    runtime::{RuntimeInput, View},
    shortcuts::KeyContext,
};

use super::{chrome, gallery::GalleryView, search::FocusSearch, settings::SettingsView};
use crate::{
    runtime::{self, Runtime},
    ui::{
        layers::{self, LayerOptions},
        theme::{self, app_root},
    },
};

/// The key context of the window's root: the layer stack's and the shell's.
pub fn shell_key_context() -> String {
    format!("{} {}", layers::KEY_CONTEXT, KeyContext::Shell.name())
}

/// The view the shell shows.
pub enum Content {
    Gallery(Entity<GalleryView>),
    Settings(Entity<SettingsView>),
}

pub struct ShellView {
    runtime: Entity<Runtime>,
    content: Content,
    settings_opener: FocusHandle,
    settings_layer: Option<LayerId>,
    _subscriptions: Vec<Subscription>,
}

impl ShellView {
    pub fn new(runtime: Entity<Runtime>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings_opener = cx.focus_handle().tab_stop(true);
        let subscriptions = vec![
            cx.observe_in(&runtime, window, |shell, _, window, cx| {
                shell.sync_view(window, cx)
            }),
            cx.observe_window_bounds(window, |_, _, cx| cx.notify()),
            // Keys reach the application's bindings only along the focused element's
            // path, so the focus falls back to the root, as a page's falls to its body.
            cx.on_focus_lost(window, |_, window, cx| layers::focus_root(window, cx)),
        ];
        let content = Content::Gallery(
            cx.new(|cx| GalleryView::new(runtime.clone(), settings_opener.clone(), window, cx)),
        );
        // A shell that goes away with its window leaves no layer behind.
        cx.on_release(|shell: &mut Self, cx| {
            if let Some(layer) = shell.settings_layer.take() {
                layers::forget(layer, cx);
            }
        })
        .detach();
        layers::focus_root(window, cx);
        let mut shell = Self {
            runtime,
            content,
            settings_opener,
            settings_layer: None,
            _subscriptions: subscriptions,
        };
        shell.sync_view(window, cx);
        shell
    }

    pub fn runtime(&self) -> &Entity<Runtime> {
        &self.runtime
    }

    pub fn content(&self) -> &Content {
        &self.content
    }

    /// The view shown now.
    pub fn view(&self) -> View {
        match self.content {
            Content::Gallery(_) => View::Gallery,
            Content::Settings(_) => View::Settings,
        }
    }

    pub fn gallery(&self) -> Option<&Entity<GalleryView>> {
        match &self.content {
            Content::Gallery(gallery) => Some(gallery),
            Content::Settings(_) => None,
        }
    }

    pub fn settings(&self) -> Option<&Entity<SettingsView>> {
        match &self.content {
            Content::Settings(settings) => Some(settings),
            Content::Gallery(_) => None,
        }
    }

    pub fn settings_opener(&self) -> &FocusHandle {
        &self.settings_opener
    }

    /// Ctrl+K: the gallery's search field takes the focus, unless the gallery is not
    /// shown or a modal layer is open (`focusSearch` of `SearchTagatorWrapper`).
    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if layers::has_modal(cx) {
            return;
        }
        if let Some(gallery) = self.gallery() {
            let search = gallery.read(cx).search().clone();
            let handle = search.read(cx).input().focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    /// Shows the view the runtime asks for, making it anew when it changes.
    fn sync_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.runtime.read(cx).model().view();
        if wanted == self.view() {
            cx.notify();
            return;
        }
        match wanted {
            View::Settings => {
                self.content = Content::Settings(
                    cx.new(|cx| SettingsView::new(self.runtime.clone(), window, cx)),
                );
                let runtime = self.runtime.downgrade();
                self.settings_layer = Some(layers::register(
                    LayerOptions::nonmodal(move |_, cx| {
                        runtime::send(&runtime, RuntimeInput::CloseSettings, cx)
                    }),
                    window,
                    cx,
                ));
            }
            View::Gallery => {
                self.content = Content::Gallery(cx.new(|cx| {
                    GalleryView::new(
                        self.runtime.clone(),
                        self.settings_opener.clone(),
                        window,
                        cx,
                    )
                }));
                if let Some(layer) = self.settings_layer.take() {
                    layers::release(layer, Some(self.settings_opener.clone()), window, cx);
                }
            }
        }
        cx.notify();
    }
}

impl Render for ShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let content = match &self.content {
            Content::Gallery(gallery) => gallery.clone().into_any_element(),
            Content::Settings(settings) => settings.clone().into_any_element(),
        };
        // The shell's key context sits on the layer stack's root, which holds the
        // focus when nothing else does, so its bindings are always on the path.
        let root = layers::root(cx)
            .key_context(shell_key_context().as_str())
            .on_action(
                cx.listener(|shell, _: &FocusSearch, window, cx| shell.focus_search(window, cx)),
            );
        app_root(root, theme).child(
            div()
                .relative()
                .size_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(content)
                .children(chrome::resize_grips(chrome::window_state(window))),
        )
    }
}
