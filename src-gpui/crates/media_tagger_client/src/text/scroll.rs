//! Horizontal scrolling of a line that is wider than its field.

/// How far the line is shifted to the left so that the caret is inside the field.
///
/// All lengths are in the same unit (pixels): `scroll` is the current shift,
/// `caret_x` the caret's position in the unscrolled line, `line_width` the width of
/// the text, `viewport_width` the width of the field's text area and `caret_width` the
/// width of the caret, which has to fit behind the last character.
///
/// A line that fits is not shifted. Otherwise the shift changes only as far as needed:
/// the caret pushes the view when it would leave on either side, and the view never
/// shows empty space behind the end of the line.
pub fn scroll_to_reveal(
    scroll: f32,
    caret_x: f32,
    line_width: f32,
    viewport_width: f32,
    caret_width: f32,
) -> f32 {
    let max_scroll = (line_width + caret_width - viewport_width).max(0.0);
    let mut scroll = scroll.clamp(0.0, max_scroll);
    if caret_x < scroll {
        scroll = caret_x;
    } else if caret_x + caret_width > scroll + viewport_width {
        scroll = caret_x + caret_width - viewport_width;
    }
    scroll.clamp(0.0, max_scroll)
}

#[cfg(test)]
mod tests {
    use super::scroll_to_reveal;

    const CARET: f32 = 1.0;

    /// The caret is drawn fully inside the viewport.
    fn caret_visible(scroll: f32, caret_x: f32, viewport: f32) -> bool {
        caret_x >= scroll && caret_x + CARET <= scroll + viewport
    }

    #[test]
    fn a_line_that_fits_is_never_shifted() {
        assert_eq!(scroll_to_reveal(0.0, 50.0, 80.0, 100.0, CARET), 0.0);
        assert_eq!(scroll_to_reveal(40.0, 80.0, 80.0, 100.0, CARET), 0.0);
        assert_eq!(scroll_to_reveal(0.0, 99.0, 99.0, 100.0, CARET), 0.0);
        assert_eq!(scroll_to_reveal(0.0, 0.0, 0.0, 100.0, CARET), 0.0);
    }

    #[test]
    fn typing_at_the_end_of_a_long_line_keeps_the_caret_at_the_right_edge() {
        let scroll = scroll_to_reveal(0.0, 300.0, 300.0, 100.0, CARET);

        assert_eq!(scroll, 201.0);
        assert!(caret_visible(scroll, 300.0, 100.0));
    }

    #[test]
    fn the_view_moves_only_when_the_caret_would_leave_it() {
        // Inside the view: nothing moves.
        assert_eq!(scroll_to_reveal(150.0, 180.0, 300.0, 100.0, CARET), 150.0);
        // Leaving on the left: the caret becomes the left edge.
        assert_eq!(scroll_to_reveal(150.0, 120.0, 300.0, 100.0, CARET), 120.0);
        // Leaving on the right: the caret becomes the right edge.
        assert_eq!(scroll_to_reveal(150.0, 260.0, 300.0, 100.0, CARET), 161.0);
        // Home.
        assert_eq!(scroll_to_reveal(201.0, 0.0, 300.0, 100.0, CARET), 0.0);
    }

    #[test]
    fn deleting_text_does_not_leave_empty_space_behind_the_line() {
        // The line shrank from 300 to 150 with the caret at its end.
        let scroll = scroll_to_reveal(201.0, 150.0, 150.0, 100.0, CARET);

        assert_eq!(scroll, 51.0);
        assert!(caret_visible(scroll, 150.0, 100.0));
    }

    #[test]
    fn the_caret_is_visible_wherever_it_goes() {
        let (line, viewport) = (1000.0, 137.0);
        let mut scroll = 0.0;
        for caret_x in [
            0.0, 999.0, 1000.0, 500.0, 499.0, 640.0, 3.0, 136.0, 137.0, 1000.0,
        ] {
            scroll = scroll_to_reveal(scroll, caret_x, line, viewport, CARET);
            assert!(
                caret_visible(scroll, caret_x, viewport),
                "caret at {caret_x}, scroll {scroll}"
            );
            assert!((0.0..=line + CARET - viewport).contains(&scroll));
        }
    }

    #[test]
    fn a_viewport_without_width_does_not_produce_a_negative_shift() {
        assert!(scroll_to_reveal(0.0, 0.0, 50.0, 0.0, CARET) >= 0.0);
        assert!(scroll_to_reveal(0.0, 50.0, 50.0, 0.0, CARET) >= 0.0);
        assert_eq!(scroll_to_reveal(0.0, 0.0, 0.0, 0.0, CARET), 1.0);
    }
}
