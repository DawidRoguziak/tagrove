//! `UiProgressBar`: a thin bar with its label beside it.

use std::time::Duration;

use gpui::{
    div, ease_in_out, prelude::*, px, relative, Animation, AnimationExt as _, App, ElementId, Role,
    SharedString, Window,
};
use media_tagger_client::{
    controls::{progress_visuals, ProgressValue},
    theme::ThemeName,
};

use super::theme::{self, color};

#[derive(IntoElement)]
pub struct ProgressBar {
    id: ElementId,
    value: ProgressValue,
    label: Option<SharedString>,
    theme: Option<ThemeName>,
}

impl ProgressBar {
    pub fn new(id: impl Into<ElementId>, value: ProgressValue) -> Self {
        Self {
            id: id.into(),
            value,
            label: None,
            theme: None,
        }
    }

    /// Shown beside the bar and announced as its name.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn theme(mut self, theme: ThemeName) -> Self {
        self.theme = Some(theme);
        self
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let visuals = progress_visuals(theme.tokens());
        let fill = color(visuals.fill);
        let track = div()
            .id(self.id.clone())
            .role(Role::ProgressIndicator)
            .relative()
            .flex_1()
            .min_w_0()
            .h(px(visuals.height))
            .rounded(px(visuals.radius))
            .overflow_hidden()
            .bg(color(visuals.track))
            .when_some(self.label.clone(), |track, label| track.aria_label(label));
        let track = match (self.value, self.value.fraction()) {
            (ProgressValue::Determinate { value, max }, Some(fraction)) => track
                .aria_numeric_value(value)
                .aria_min_numeric_value(0.0)
                .aria_max_numeric_value(max)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .w(relative(fraction as f32))
                        .rounded(px(visuals.radius))
                        .bg(fill),
                ),
            _ => {
                let width = visuals.stripe_width;
                track.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(relative(width))
                        .bg(fill)
                        .with_animation(
                            (self.id, "stripe"),
                            Animation::new(Duration::from_millis(visuals.stripe_period_ms))
                                .repeat()
                                .with_easing(ease_in_out),
                            move |stripe, progress| {
                                stripe.left(relative(-width + progress * (1.0 + width)))
                            },
                        ),
                )
            }
        };
        div()
            .flex()
            .items_center()
            .w_full()
            .max_w(px(visuals.max_width))
            .gap(px(visuals.gap))
            .child(track)
            .when_some(self.label, |row, label| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(px(visuals.label_size))
                        .line_height(px(visuals.label_line_height))
                        .text_color(color(visuals.label))
                        .child(label),
                )
            })
    }
}
