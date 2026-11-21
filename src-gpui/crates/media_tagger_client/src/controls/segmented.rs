//! `UiSegmentedControl`: a radio group drawn as segments, with a highlight that slides
//! to the selected segment (`.ui-segmented-*` in `src/styles.css`).
//!
//! It is controlled like the Solid control: a click or an arrow key asks the owner for a
//! value, and the group keeps showing the owner's value until the owner changes it. One
//! segment is a tab stop (the selected one, as for native radios); the arrow keys move the
//! focus and ask for the segment they land on.

use super::FocusRing;
use crate::theme::{Rgba, ThemeTokens, RADII, TYPOGRAPHY};

/// The keys of a radio group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentedKey {
    /// Arrow right and arrow down: the next segment, wrapping to the first.
    Next,
    /// Arrow left and arrow up: the previous segment, wrapping to the last.
    Previous,
}

impl SegmentedKey {
    /// The segment a key moves to from `from` in a group of `count`; `None` for an
    /// empty group.
    pub fn target(self, from: usize, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let from = from.min(count - 1);
        Some(match self {
            Self::Next => (from + 1) % count,
            Self::Previous => (from + count - 1) % count,
        })
    }
}

/// A rectangle relative to the group, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// The rectangle `progress` (0..=1) of the way from `self` to `to`.
    pub fn lerp(self, to: Rect, progress: f32) -> Rect {
        let at = |a: f32, b: f32| a + (b - a) * progress;
        Rect {
            x: at(self.x, to.x),
            y: at(self.y, to.y),
            width: at(self.width, to.width),
            height: at(self.height, to.height),
        }
    }
}

/// What a measurement asks the highlight to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measure {
    /// Nothing is selected: no highlight.
    Hidden,
    /// The same layout and target as before: leave it, including a slide in flight.
    Unchanged,
    /// Jump to the rectangle without a transition.
    Snap(Rect),
    /// Slide from the rectangle the highlight was at.
    Slide { from: Rect, to: Rect },
}

/// Where the highlight is: a port of `measureSegmentedHighlight`. A new selection slides
/// when the layout of the segments (their rectangles and labels) is the same as at the
/// last measurement; any change of layout (a resize, a translation) snaps.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HighlightTrack {
    previous: Option<Measured>,
}

#[derive(Debug, Clone, PartialEq)]
struct Measured {
    bounds: Vec<Rect>,
    labels: Vec<String>,
    target: Rect,
}

impl HighlightTrack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Measures the segments. `animate` is true when the selection or the options
    /// changed since the last measurement, false when only the layout may have (the
    /// Solid control's `ResizeObserver`).
    pub fn measure(
        &mut self,
        bounds: &[Rect],
        labels: &[&str],
        selected: Option<usize>,
        animate: bool,
    ) -> Measure {
        let Some(target) = selected.and_then(|index| bounds.get(index)).copied() else {
            self.previous = None;
            return Measure::Hidden;
        };
        let same_layout = self.previous.as_ref().is_some_and(|previous| {
            previous.bounds == bounds
                && previous.labels.len() == labels.len()
                && previous.labels.iter().zip(labels).all(|(a, b)| a == b)
        });
        let previous_target = self.previous.as_ref().map(|previous| previous.target);
        if same_layout && previous_target == Some(target) {
            return Measure::Unchanged;
        }
        let outcome = match previous_target {
            Some(from) if animate && same_layout => Measure::Slide { from, to: target },
            _ => Measure::Snap(target),
        };
        self.previous = Some(Measured {
            bounds: bounds.to_vec(),
            labels: labels.iter().map(|label| (*label).to_owned()).collect(),
            target,
        });
        outcome
    }

    /// The rectangle the highlight ends at, once measured.
    pub fn target(&self) -> Option<Rect> {
        self.previous.as_ref().map(|previous| previous.target)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentedVisuals {
    /// `.ui-segmented-control`: `background: var(--surface-muted)`.
    pub background: Rgba,
    /// `box-shadow: inset 0 0 0 1px var(--border-strong)`
    pub ring: Rgba,
    pub radius: f32,
    /// `padding: 3px`
    pub padding: f32,
    /// `gap: 2px`
    pub gap: f32,
    /// `.ui-segmented-label`: `padding: 3px 12px`.
    pub label_padding_x: f32,
    pub label_padding_y: f32,
    /// `border-radius: 6px` of the label and of the highlight.
    pub label_radius: f32,
    /// `font-size: 12px; line-height: 20px`
    pub font_size: f32,
    pub line_height: f32,
    pub text: Rgba,
    /// The selected label: `color: var(--color-primary-content); font-weight: 600`; the
    /// hidden reserve copy of every label is semibold too, so the width never changes.
    pub selected_text: Rgba,
    pub selected_weight: f32,
    /// `label:hover input:not(:checked) + .ui-segmented-label`
    pub hover_background: Rgba,
    /// `.ui-segmented-highlight`: `background: var(--color-primary)`.
    pub highlight: Rgba,
    /// `.choice-input:focus-visible + span { outline: 2px solid var(--color-primary);
    /// outline-offset: 2px; }`
    pub focus_ring: FocusRing,
}

pub fn segmented_visuals(tokens: &ThemeTokens) -> SegmentedVisuals {
    let colors = &tokens.colors;
    SegmentedVisuals {
        background: colors.surface_muted,
        ring: colors.border_strong,
        radius: RADII.control,
        padding: 3.0,
        gap: 2.0,
        label_padding_x: 12.0,
        label_padding_y: 3.0,
        label_radius: 6.0,
        font_size: 12.0,
        line_height: 20.0,
        text: colors.base_content,
        selected_text: colors.primary_content,
        selected_weight: TYPOGRAPHY.weight_semibold,
        hover_background: colors.surface_raised,
        highlight: colors.primary,
        focus_ring: FocusRing {
            color: colors.primary,
            width: 2.0,
            offset: 2.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controls::STYLESHEET, theme::DARK};

    fn rects(width: f32) -> Vec<Rect> {
        (0..3)
            .map(|index| Rect {
                x: index as f32 * width + 3.0,
                y: 3.0,
                width,
                height: 26.0,
            })
            .collect()
    }

    const LABELS: [&str; 3] = ["All", "Images", "Video"];

    #[test]
    fn the_stylesheet_still_has_the_rules_copied_here() {
        for rule in [
            "padding: 3px; border: 0; border-radius: var(--radius-control); background: var(--surface-muted); box-shadow: inset 0 0 0 1px var(--border-strong); }",
            ".ui-segmented-label { display: grid; padding: 3px 12px; border-radius: 6px; font-size: 12px; line-height: 20px; color: var(--color-base-content); }",
            ".ui-segmented-reserve { visibility: hidden; font-weight: 600; }",
            "input:not(:checked) + .ui-segmented-label { background: var(--surface-raised);",
            "input:checked + .ui-segmented-label { background: transparent; color: var(--color-primary-content); font-weight: 600; }",
            "border-radius: 6px; background: var(--color-primary); transition: transform var(--motion-entrance) var(--motion-ease)",
            ".choice-input:focus-visible + span { outline: 2px solid var(--color-primary); outline-offset: 2px; }",
        ] {
            assert!(STYLESHEET.contains(rule), "src/styles.css lost: {rule}");
        }
        let visuals = segmented_visuals(&DARK);
        assert_eq!(visuals.highlight, DARK.colors.primary);
        assert_eq!(visuals.selected_text, DARK.colors.primary_content);
    }

    #[test]
    fn arrow_keys_wrap_around_the_group() {
        assert_eq!(SegmentedKey::Next.target(0, 3), Some(1));
        assert_eq!(SegmentedKey::Next.target(2, 3), Some(0));
        assert_eq!(SegmentedKey::Previous.target(0, 3), Some(2));
        assert_eq!(SegmentedKey::Previous.target(1, 3), Some(0));
        assert_eq!(SegmentedKey::Next.target(0, 0), None);
        assert_eq!(
            SegmentedKey::Next.target(9, 3),
            Some(0),
            "out of range counts as last"
        );
    }

    /// Port of "slides selection but snaps changed layout and translation".
    #[test]
    fn slides_a_new_selection_and_snaps_a_changed_layout() {
        let mut track = HighlightTrack::new();
        let narrow = rects(70.0);

        let first = track.measure(&narrow, &LABELS, Some(0), true);
        assert_eq!(first, Measure::Snap(narrow[0]), "the first placement snaps");
        assert_eq!(
            (narrow[0].x, narrow[0].y, narrow[0].width),
            (3.0, 3.0, 70.0)
        );

        let video = track.measure(&narrow, &LABELS, Some(2), true);
        assert_eq!(
            video,
            Measure::Slide {
                from: narrow[0],
                to: narrow[2]
            }
        );
        assert_eq!(narrow[2].x, 143.0);

        // An observer notification with the same layout keeps the slide going.
        assert_eq!(
            track.measure(&narrow, &LABELS, Some(2), false),
            Measure::Unchanged
        );

        let wide = rects(80.0);
        assert_eq!(
            track.measure(&wide, &LABELS, Some(2), false),
            Measure::Snap(wide[2])
        );
        assert_eq!((wide[2].x, wide[2].width), (163.0, 80.0));

        assert!(matches!(
            track.measure(&wide, &LABELS, Some(1), true),
            Measure::Slide { .. }
        ));

        let translated = ["Translated All", "Translated Images", "Translated Video"];
        assert_eq!(
            track.measure(&wide, &translated, Some(1), true),
            Measure::Snap(wide[1]),
            "new labels snap even when the rectangles stay"
        );
    }

    #[test]
    fn no_selection_hides_the_highlight_and_the_next_one_snaps() {
        let mut track = HighlightTrack::new();
        let bounds = rects(70.0);
        track.measure(&bounds, &LABELS, Some(1), true);

        assert_eq!(track.measure(&bounds, &LABELS, None, true), Measure::Hidden);
        assert_eq!(track.target(), None);
        assert_eq!(
            track.measure(&bounds, &LABELS, Some(9), true),
            Measure::Hidden
        );
        assert_eq!(
            track.measure(&bounds, &LABELS, Some(0), true),
            Measure::Snap(bounds[0])
        );
    }

    #[test]
    fn rectangles_interpolate_linearly() {
        let from = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 20.0,
        };
        let to = Rect {
            x: 100.0,
            y: 10.0,
            width: 30.0,
            height: 20.0,
        };
        assert_eq!(from.lerp(to, 0.0), from);
        assert_eq!(from.lerp(to, 1.0), to);
        assert_eq!(
            from.lerp(to, 0.5),
            Rect {
                x: 50.0,
                y: 5.0,
                width: 20.0,
                height: 20.0
            }
        );
    }
}
