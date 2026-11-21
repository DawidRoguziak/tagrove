//! Loading spinners: the small arc beside a status text and the ring of a gallery tile,
//! optionally shown only after a delay (`.thumbnail-loader`).

use gpui::{
    div, percentage, prelude::*, px, svg, Animation, AnimationExt as _, App, ElementId, Hsla,
    Transformation, Window,
};
use media_tagger_client::{
    controls::{spinner_visuals, DelayedReveal, SpinnerSize},
    theme::ThemeName,
};

use super::theme::{self, color};

/// The arc of DaisyUI's `loading-spinner` mask: a circle of radius 9.5 drawn for 42 of
/// its length, with round caps.
const SMALL_ARC: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9.5" fill="none" stroke="black" stroke-width="3" stroke-linecap="round" stroke-dasharray="42 150"/></svg>"#;
/// The top quarter of the tile ring (`border-t-primary` of a 26px circle with a 3px border).
const TILE_ARC: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 26"><path d="M4.868 4.868 A11.5 11.5 0 0 1 21.132 4.868" fill="none" stroke="black" stroke-width="3"/></svg>"#;

#[derive(IntoElement)]
pub struct Spinner {
    id: ElementId,
    size: SpinnerSize,
    reveal: Option<DelayedReveal>,
    color: Option<Hsla>,
    theme: Option<ThemeName>,
}

impl Spinner {
    pub fn new(id: impl Into<ElementId>, size: SpinnerSize) -> Self {
        Self {
            id: id.into(),
            size,
            reveal: None,
            color: None,
            theme: None,
        }
    }

    /// Invisible for the reveal's delay after it is first drawn, then fading in.
    pub fn reveal(mut self, reveal: DelayedReveal) -> Self {
        self.reveal = Some(reveal);
        self
    }

    /// The arc's colour where the size has none of its own (the small spinner takes the
    /// text colour around it otherwise).
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn theme(mut self, theme: ThemeName) -> Self {
        self.theme = Some(theme);
        self
    }
}

impl RenderOnce for Spinner {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let visuals = spinner_visuals(theme.tokens(), self.size);
        let arc_color = visuals
            .arc
            .map(color)
            .or(self.color)
            .unwrap_or_else(|| window.text_style().color);
        let size = px(visuals.size);
        let arc = svg()
            .data(match self.size {
                SpinnerSize::ExtraSmall => SMALL_ARC,
                SpinnerSize::Tile => TILE_ARC,
            })
            .absolute()
            .top_0()
            .left_0()
            .size(size)
            .text_color(arc_color)
            .with_animation(
                (self.id.clone(), "spin"),
                Animation::new(visuals.period).repeat(),
                |arc, turn| arc.with_transformation(Transformation::rotate(percentage(turn))),
            );
        let spinner = div()
            .id(self.id.clone())
            .relative()
            .flex_none()
            .size(size)
            .when(visuals.track.a > 0, |spinner| {
                spinner.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_full()
                        .border_3()
                        .border_color(color(visuals.track)),
                )
            })
            .child(arc);
        match self.reveal {
            Some(reveal) => spinner
                .with_animation(
                    (self.id, "reveal"),
                    Animation::new(reveal.total()),
                    move |spinner, progress| spinner.opacity(reveal.opacity_at_progress(progress)),
                )
                .into_any_element(),
            None => spinner.into_any_element(),
        }
    }
}
