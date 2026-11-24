//! The switch of the settings page (`toggle toggle-sm toggle-primary` on a checkbox in a
//! `label`): a tab stop that Space toggles; a click on it or on its text toggles too.
//! It is controlled: it asks the owner with `on_change` and shows `checked` as given.

use std::rc::Rc;

use gpui::{div, prelude::*, px, App, ElementId, Role, SharedString, Toggled, Window};
use media_tagger_client::{
    controls::{switch_visuals, SwitchState},
    theme::ThemeName,
};

use super::{
    button::keyed_focus_handle,
    theme::{self, color, focus_ring, RingBorder},
};

type ChangeHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    checked: bool,
    disabled: bool,
    label: SharedString,
    text: Option<SharedString>,
    theme: Option<ThemeName>,
    debug_selector: Option<SharedString>,
    on_change: Option<ChangeHandler>,
}

impl Switch {
    /// `label` is the accessible name (`aria-label`).
    pub fn new(id: impl Into<ElementId>, checked: bool, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            checked,
            disabled: false,
            label: label.into(),
            text: None,
            theme: None,
            debug_selector: None,
            on_change: None,
        }
    }

    /// Text beside the switch, inside the clickable label (`text-sm`).
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn theme(mut self, theme: ThemeName) -> Self {
        self.theme = Some(theme);
        self
    }

    /// A debug selector for tests (`VisualTestContext::debug_bounds`); a no-op outside
    /// test builds.
    pub fn debug_selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.debug_selector = Some(selector.into());
        self
    }

    /// Asked with the value a click or Space wants.
    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let visuals = switch_visuals(
            theme.tokens(),
            SwitchState {
                checked: self.checked,
                disabled: self.disabled,
            },
        );
        let focus_handle = keyed_focus_handle(&self.id, window, cx);
        let focus_visible =
            !self.disabled && focus_handle.is_focused(window) && window.last_input_was_keyboard();
        let tint = color(visuals.color);
        let knob = div()
            .size(px(visuals.knob_size))
            .rounded(px(visuals.knob_radius))
            .map(|knob| {
                if visuals.knob_filled {
                    knob.bg(tint)
                } else {
                    knob.border_1().border_color(tint)
                }
            });
        let track = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .when(visuals.knob_at_end, |track| track.justify_end())
            .w(px(visuals.width))
            .h(px(visuals.height))
            .p(px(visuals.padding))
            .rounded(px(visuals.radius))
            .border_1()
            .border_color(tint)
            .bg(color(visuals.background))
            .opacity(visuals.opacity)
            .child(knob)
            .when(focus_visible, |track| {
                track.child(focus_ring(
                    visuals.focus_ring,
                    visuals.radius,
                    RingBorder::all(visuals.border_width),
                ))
            });
        let checked = self.checked;
        let row = div()
            .id(self.id)
            .role(Role::Switch)
            .aria_label(self.label)
            .aria_toggled(if checked {
                Toggled::True
            } else {
                Toggled::False
            })
            .flex()
            .items_center()
            .gap_2()
            .child(track)
            .when_some(self.text, |row, text| row.child(text))
            .when_some(self.debug_selector, |row, selector| {
                row.debug_selector(move || selector.to_string())
            });
        if self.disabled {
            return row.cursor_not_allowed();
        }
        let on_change = self.on_change;
        row.track_focus(&focus_handle)
            .cursor_pointer()
            .when_some(on_change, |row, on_change| {
                row.on_click(move |_, window, cx| on_change(!checked, window, cx))
            })
    }
}
