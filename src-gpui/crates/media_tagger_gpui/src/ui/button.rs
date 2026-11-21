//! `UiButton` and `UiIconButton`.
//!
//! A button is a focusable tab stop while it is enabled; Enter and Space activate the
//! focused one (GPUI's keyboard click). Its focus handle lives in the element state
//! under the button's id unless the owner gives one with [`Button::track_focus`],
//! which the owner needs to restore the focus to the button (a dialog's trigger).

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, App, ClickEvent, ElementId, FocusHandle, Role, SharedString, Window,
};
use media_tagger_client::{
    controls::{
        button_visuals, icon_button_visuals, ButtonState, ButtonVariant, ButtonVisuals,
        IconButtonKind, ICON_BUTTON_ICON_SIZE, ICON_BUTTON_SIZE,
    },
    icons::IconName,
    theme::ThemeName,
};

use super::{
    icon::Icon,
    theme::{self, color, focus_ring, shadows_beneath, weight},
};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

enum Content {
    Label(SharedString),
    Icon {
        icon: IconName,
        kind: IconButtonKind,
    },
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    content: Content,
    variant: ButtonVariant,
    disabled: bool,
    theme: Option<ThemeName>,
    focus_handle: Option<FocusHandle>,
    aria_label: Option<SharedString>,
    leading_icon: Option<(IconName, f32)>,
    debug_selector: Option<&'static str>,
    on_click: Option<ClickHandler>,
}

impl Button {
    /// A text button (`UiButton`), secondary unless [`Self::variant`] says otherwise.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            content: Content::Label(label.into()),
            variant: ButtonVariant::default(),
            disabled: false,
            theme: None,
            focus_handle: None,
            aria_label: None,
            leading_icon: None,
            debug_selector: None,
            on_click: None,
        }
    }

    /// An icon button (`UiIconButton`). `label` is its accessible name (`aria-label`).
    pub fn icon(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            content: Content::Icon {
                icon,
                kind: IconButtonKind::Plain,
            },
            aria_label: Some(label.into()),
            ..Self::new(id, SharedString::default())
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// `active` of an icon button; the heart is filled and tinted with the favourite
    /// colour.
    pub fn active(mut self, active: bool) -> Self {
        if let Content::Icon { icon, kind } = &mut self.content {
            if *kind != IconButtonKind::Danger {
                *kind = if active {
                    IconButtonKind::Active {
                        heart: *icon == IconName::Heart,
                    }
                } else {
                    IconButtonKind::Plain
                };
            }
        }
        self
    }

    /// `danger` of an icon button.
    pub fn danger(mut self, danger: bool) -> Self {
        if let Content::Icon { kind, .. } = &mut self.content {
            *kind = if danger {
                IconButtonKind::Danger
            } else {
                IconButtonKind::Plain
            };
        }
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Draws with these tokens instead of the active theme's.
    pub fn theme(mut self, theme: ThemeName) -> Self {
        self.theme = Some(theme);
        self
    }

    /// The focus handle of the button, for an owner that moves the focus to it.
    pub fn track_focus(mut self, focus_handle: &FocusHandle) -> Self {
        self.focus_handle = Some(focus_handle.clone());
        self
    }

    /// An icon of `size` CSS pixels before the label of a text button (`<UiIcon>` as the
    /// first child of `UiButton`), in the label's colour.
    pub fn leading_icon(mut self, icon: IconName, size: f32) -> Self {
        self.leading_icon = Some((icon, size));
        self
    }

    /// A debug selector for tests (`VisualTestContext::debug_bounds`); a no-op outside
    /// test builds.
    pub fn debug_selector(mut self, selector: &'static str) -> Self {
        self.debug_selector = Some(selector);
        self
    }

    /// The accessible name, when it differs from the label.
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    fn visuals(&self, theme: ThemeName, hovered: bool) -> ButtonVisuals {
        let state = ButtonState {
            hovered,
            disabled: self.disabled,
        };
        match self.content {
            Content::Label(_) => button_visuals(theme.tokens(), self.variant, state),
            Content::Icon { kind, .. } => icon_button_visuals(theme.tokens(), kind, state),
        }
    }
}

/// The focus handle a control keeps in its element state under `id`: a tab stop.
pub(crate) fn keyed_focus_handle(id: &ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    window
        .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true))
        .read(cx)
        .clone()
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let rest = self.visuals(theme, false);
        let hover = self.visuals(theme, true);
        let focus_handle = self
            .focus_handle
            .clone()
            .unwrap_or_else(|| keyed_focus_handle(&self.id, window, cx));
        let focus_visible =
            !self.disabled && focus_handle.is_focused(window) && window.last_input_was_keyboard();

        let button = div()
            .id(self.id.clone())
            .role(Role::Button)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(rest.gap))
            .h(px(rest.height))
            .px(px(rest.padding_x))
            .rounded(px(rest.radius))
            .border_1()
            .border_color(color(rest.border))
            .bg(color(rest.background))
            .text_color(color(rest.text))
            .text_size(px(rest.font_size))
            .font_weight(weight(rest.font_weight))
            .whitespace_nowrap()
            .shadow(shadows_beneath(rest.shadows, rest.background, rest.opacity))
            .opacity(rest.opacity)
            .when_some(self.aria_label.clone(), |button, label| {
                button.aria_label(label)
            })
            .when_some(self.debug_selector, |button, selector| {
                button.debug_selector(move || selector.to_owned())
            })
            .when(focus_visible, |button| {
                button.child(focus_ring(rest.focus_ring, rest.radius))
            });
        let button = match self.content {
            Content::Label(label) => button
                .when_some(self.leading_icon, |button, (icon, size)| {
                    button.child(Icon::new(icon).size(size).color(color(rest.text)))
                })
                .child(label),
            Content::Icon { icon, kind } => button.w(px(ICON_BUTTON_SIZE)).child(
                Icon::new(icon)
                    .size(ICON_BUTTON_ICON_SIZE)
                    .filled(
                        icon == IconName::Heart && matches!(kind, IconButtonKind::Active { .. }),
                    )
                    .color(color(rest.text)),
            ),
        };
        if self.disabled {
            return button.cursor_not_allowed();
        }
        let on_click = self.on_click;
        button
            .track_focus(&focus_handle)
            .cursor_pointer()
            .hover(move |style| {
                style
                    .bg(color(hover.background))
                    .border_color(color(hover.border))
                    .text_color(color(hover.text))
            })
            .when_some(on_click, |button, on_click| {
                button.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}
