//! How the drawn line splits into runs of one style.

use std::ops::Range;

/// A stretch of the line drawn in one style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSpec {
    /// Length in UTF-8 bytes; never 0.
    pub len: usize,
    /// Inside the selection: drawn in the selection's text colour.
    pub selected: bool,
    /// Text an input method is composing: underlined.
    pub marked: bool,
}

/// The runs of a line of `len` bytes. Their lengths add up to `len` whatever the
/// ranges are (a renderer given fewer or more bytes than the text has misdraws or
/// panics), so ranges reaching past the line are cut and an empty line has no run.
pub fn style_runs(
    len: usize,
    selection: Option<&Range<usize>>,
    marked: Option<&Range<usize>>,
) -> Vec<RunSpec> {
    let clamp = |range: Option<&Range<usize>>| {
        range
            .map(|range| range.start.min(len)..range.end.min(len))
            .filter(|range| range.start < range.end)
    };
    let selection = clamp(selection);
    let marked = clamp(marked);
    let mut cuts = vec![0, len];
    for range in [&selection, &marked].into_iter().flatten() {
        cuts.extend([range.start, range.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let inside =
        |range: &Option<Range<usize>>, at: usize| range.as_ref().is_some_and(|r| r.contains(&at));
    cuts.windows(2)
        .map(|cut| RunSpec {
            len: cut[1] - cut[0],
            selected: inside(&selection, cut[0]),
            marked: inside(&marked, cut[0]),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{style_runs, RunSpec};

    fn run(len: usize, selected: bool, marked: bool) -> RunSpec {
        RunSpec {
            len,
            selected,
            marked,
        }
    }

    #[test]
    fn a_plain_line_is_one_run_and_an_empty_line_none() {
        assert_eq!(style_runs(7, None, None), vec![run(7, false, false)]);
        assert_eq!(style_runs(0, None, None), vec![]);
        assert_eq!(style_runs(0, Some(&(0..0)), Some(&(0..3))), vec![]);
    }

    #[test]
    fn a_selection_splits_the_line() {
        assert_eq!(
            style_runs(10, Some(&(2..5)), None),
            vec![
                run(2, false, false),
                run(3, true, false),
                run(5, false, false)
            ]
        );
        assert_eq!(
            style_runs(4, Some(&(0..4)), None),
            vec![run(4, true, false)]
        );
        assert_eq!(
            style_runs(4, Some(&(2..2)), None),
            vec![run(4, false, false)]
        );
    }

    #[test]
    fn selection_and_composition_may_overlap() {
        assert_eq!(
            style_runs(10, Some(&(4..8)), Some(&(2..6))),
            vec![
                run(2, false, false),
                run(2, false, true),
                run(2, true, true),
                run(2, true, false),
                run(2, false, false),
            ]
        );
    }

    #[test]
    fn the_lengths_always_add_up_to_the_line() {
        for (len, selection, marked) in [
            (5, 3..99, 0..2),
            (5, 99..120, 4..5),
            (1, 0..1, 0..1),
            (12, 0..12, 3..9),
        ] {
            let runs = style_runs(len, Some(&selection), Some(&marked));
            assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), len);
            assert!(runs.iter().all(|run| run.len > 0));
        }
    }
}
