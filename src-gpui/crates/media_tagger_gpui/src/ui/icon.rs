//! `UiIcon`: one of the 22 icons, drawn in the current text colour.

use gpui::{percentage, prelude::*, px, svg, App, Hsla, Transformation, Window};
use media_tagger_client::icons::IconName;

/// The default size: `h-5 w-5`.
pub const ICON_SIZE: f32 = 20.0;

#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    filled: bool,
    size: f32,
    color: Option<Hsla>,
    half_turn: bool,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            filled: false,
            size: ICON_SIZE,
            color: None,
            half_turn: false,
        }
    }

    /// `filled` of `UiIcon`: the shape is filled with the stroke colour.
    pub fn filled(mut self, filled: bool) -> Self {
        self.filled = filled;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The colour; without one the icon takes the text colour around it (`currentColor`).
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Draws the icon turned by half a turn (`rotate-180`): the left arrow points right.
    pub fn half_turn(mut self, half_turn: bool) -> Self {
        self.half_turn = half_turn;
        self
    }

    pub fn name(&self) -> IconName {
        self.name
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let color = self.color.unwrap_or_else(|| window.text_style().color);
        // An icon is decorative (aria-hidden) unless its owner names it.
        svg()
            .path(self.name.asset_path(self.filled))
            .size(px(self.size))
            .flex_none()
            .text_color(color)
            .when(self.half_turn, |icon| {
                icon.with_transformation(Transformation::rotate(percentage(0.5)))
            })
    }
}
