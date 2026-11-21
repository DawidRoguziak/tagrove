//! Where a caret may stand and where a word starts and ends: extended grapheme
//! clusters and word segments of UAX #29 (`unicode-segmentation`).
//!
//! All offsets are UTF-8 byte offsets. Every function accepts any offset, including
//! one past the end or inside a character, and returns one on a character boundary.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// The largest character boundary at or before `offset`, clamped to the text.
pub fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// How many bytes two texts have in common at their start, on a character boundary of
/// both.
pub fn common_prefix_len(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, in_a), in_b)| in_a != in_b)
        .map_or(a.len().min(b.len()), |((offset, _), _)| offset)
}

fn grapheme_starts(text: &str) -> impl DoubleEndedIterator<Item = usize> + '_ {
    text.grapheme_indices(true).map(|(start, _)| start)
}

/// The start of the grapheme cluster before `offset`: where the caret lands after one
/// step to the left, and where Backspace starts deleting. 0 at the start of the text.
pub fn prev_grapheme(text: &str, offset: usize) -> usize {
    grapheme_starts(text)
        .rev()
        .find(|start| *start < offset)
        .unwrap_or(0)
}

/// The end of the grapheme cluster after `offset`: where the caret lands after one
/// step to the right. The length of the text at its end.
pub fn next_grapheme(text: &str, offset: usize) -> usize {
    grapheme_starts(text)
        .find(|start| *start > offset)
        .unwrap_or(text.len())
}

/// The grapheme boundary nearest to `offset` (the earlier one on a tie), so a pointer
/// position inside a cluster never splits it.
pub fn nearest_grapheme(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    let before = grapheme_starts(text)
        .rev()
        .find(|start| *start <= offset)
        .unwrap_or(0);
    if before == offset {
        return offset;
    }
    let after = next_grapheme(text, before);
    if offset - before <= after - offset {
        before
    } else {
        after
    }
}

/// Whether a word segment is something Ctrl+Left and Ctrl+Right stop at: it has a
/// letter, a digit, an underscore, or a non-ASCII character that is neither whitespace
/// nor a control character (an emoji, an ideograph, typographic punctuation).
/// Whitespace and ASCII punctuation are skipped over.
fn is_word(segment: &str) -> bool {
    segment.chars().any(|c| {
        c.is_alphanumeric() || c == '_' || (!c.is_ascii() && !c.is_whitespace() && !c.is_control())
    })
}

/// The start of the word before `offset` (Ctrl+Left): whitespace and punctuation
/// between the caret and that word are skipped. 0 when no word is left.
pub fn prev_word_start(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .rev()
        .find(|(start, segment)| *start < offset && is_word(segment))
        .map_or(0, |(start, _)| start)
}

/// The end of the word after `offset` (Ctrl+Right): whitespace and punctuation before
/// that word are skipped. The length of the text when no word is left.
pub fn next_word_end(text: &str, offset: usize) -> usize {
    text.split_word_bound_indices()
        .map(|(start, segment)| (start + segment.len(), segment))
        .find(|(end, segment)| *end > offset && is_word(segment))
        .map_or(text.len(), |(end, _)| end)
}

/// The word segment that contains the character at `offset` (a double click): a word,
/// a run of spaces or one punctuation mark. At the end of the text it is the last
/// segment; an empty text gives `0..0`.
pub fn word_range_at(text: &str, offset: usize) -> Range<usize> {
    let mut last = 0..0;
    for (start, segment) in text.split_word_bound_indices() {
        let end = start + segment.len();
        if offset < end {
            return start..end;
        }
        last = start..end;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every offset the caret visits walking right from 0, then back left from the end.
    fn stops(text: &str) -> (Vec<usize>, Vec<usize>) {
        let mut right = vec![0];
        while *right.last().unwrap() < text.len() {
            right.push(next_grapheme(text, *right.last().unwrap()));
        }
        let mut left = vec![text.len()];
        while *left.last().unwrap() > 0 {
            left.push(prev_grapheme(text, *left.last().unwrap()));
        }
        left.reverse();
        (right, left)
    }

    #[test]
    fn the_caret_steps_over_whole_grapheme_clusters() {
        // A family joined with ZWJ, a flag, a skin-tone modifier, a keycap.
        let family = "👨\u{200d}👩\u{200d}👧";
        let text = format!("a{family}🇵🇱👍🏽1\u{fe0f}\u{20e3}z");
        let expected = vec![
            0,
            1,
            1 + family.len(),
            1 + family.len() + 8,
            1 + family.len() + 16,
            1 + family.len() + 16 + 7,
            text.len(),
        ];

        let (right, left) = stops(&text);

        assert_eq!(right, expected);
        assert_eq!(left, expected);
    }

    #[test]
    fn combining_marks_stay_with_their_base() {
        // "e" + combining acute, then a Hangul syllable built from three jamo.
        let text = "e\u{301}x\u{1100}\u{1161}\u{11a8}";

        let (right, left) = stops(text);

        assert_eq!(right, vec![0, 3, 4, text.len()]);
        assert_eq!(left, right);
    }

    #[test]
    fn cjk_characters_are_one_step_each() {
        let text = "漢字かな";

        let (right, _) = stops(text);

        assert_eq!(right, vec![0, 3, 6, 9, 12]);
    }

    #[test]
    fn offsets_inside_a_cluster_or_past_the_end_are_accepted() {
        let text = "a👍🏽";

        assert_eq!(prev_grapheme(text, 3), 1);
        assert_eq!(next_grapheme(text, 3), text.len());
        assert_eq!(next_grapheme(text, 99), text.len());
        assert_eq!(prev_grapheme(text, 99), 1);
        assert_eq!(floor_char_boundary(text, 3), 1);
        assert_eq!(floor_char_boundary(text, 99), text.len());
        assert_eq!(prev_grapheme("", 0), 0);
        assert_eq!(next_grapheme("", 0), 0);
    }

    #[test]
    fn the_common_start_of_two_texts_ends_on_a_character_boundary() {
        assert_eq!(common_prefix_len("tag", "tag"), 3);
        assert_eq!(common_prefix_len("tag", "tags"), 3);
        assert_eq!(common_prefix_len("tags", "tag"), 3);
        assert_eq!(common_prefix_len("tag に", "tag にほ"), 7);
        // に and ぬ share their first two bytes.
        assert_eq!(common_prefix_len("tag に", "tag ぬ"), 4);
        assert_eq!(common_prefix_len("abc", "xbc"), 0);
        assert_eq!(common_prefix_len("", "abc"), 0);
        assert_eq!(common_prefix_len("", ""), 0);
    }

    #[test]
    fn a_pointer_offset_snaps_to_the_nearest_cluster_boundary() {
        // 👍🏽 is 8 bytes: 1..9.
        let text = "a👍🏽b";

        assert_eq!(nearest_grapheme(text, 0), 0);
        assert_eq!(nearest_grapheme(text, 1), 1);
        assert_eq!(nearest_grapheme(text, 4), 1);
        assert_eq!(nearest_grapheme(text, 5), 1);
        assert_eq!(nearest_grapheme(text, 6), 9);
        assert_eq!(nearest_grapheme(text, 9), 9);
        assert_eq!(nearest_grapheme(text, 99), text.len());
    }

    #[test]
    fn word_steps_skip_spaces_and_ascii_punctuation() {
        let text = "foo, bar_baz  -qux";
        //          0123456789012345678

        let mut right = vec![0];
        while *right.last().unwrap() < text.len() {
            right.push(next_word_end(text, *right.last().unwrap()));
        }
        let mut left = vec![text.len()];
        while *left.last().unwrap() > 0 {
            left.push(prev_word_start(text, *left.last().unwrap()));
        }

        assert_eq!(right, vec![0, 3, 12, 18]);
        assert_eq!(left, vec![18, 15, 5, 0]);
    }

    #[test]
    fn word_steps_from_inside_a_word_stop_at_its_edges() {
        let text = "alpha beta";

        assert_eq!(prev_word_start(text, 8), 6);
        assert_eq!(next_word_end(text, 8), 10);
        assert_eq!(prev_word_start(text, 6), 0);
        assert_eq!(next_word_end(text, 5), 10);
    }

    #[test]
    fn word_steps_treat_emoji_and_ideographs_as_words() {
        let text = "a 👍🏽 漢字";
        //          a=0, space=1, emoji=2..10, space=10, 漢=11..14, 字=14..17

        assert_eq!(next_word_end(text, 1), 10);
        assert_eq!(next_word_end(text, 10), 14);
        assert_eq!(next_word_end(text, 14), 17);
        assert_eq!(prev_word_start(text, 17), 14);
        assert_eq!(prev_word_start(text, 11), 2);
    }

    #[test]
    fn word_steps_without_a_word_reach_the_ends() {
        assert_eq!(next_word_end("  ..  ", 0), 6);
        assert_eq!(prev_word_start("  ..  ", 6), 0);
        assert_eq!(next_word_end("", 0), 0);
        assert_eq!(prev_word_start("", 0), 0);
    }

    #[test]
    fn a_double_click_takes_the_segment_under_the_pointer() {
        let text = "foo  bar-baz";
        //          012345678901

        assert_eq!(word_range_at(text, 0), 0..3);
        assert_eq!(word_range_at(text, 2), 0..3);
        assert_eq!(word_range_at(text, 3), 3..5);
        assert_eq!(word_range_at(text, 6), 5..8);
        assert_eq!(word_range_at(text, 8), 8..9);
        assert_eq!(word_range_at(text, 12), 9..12);
        assert_eq!(word_range_at(text, 99), 9..12);
        assert_eq!(word_range_at("", 0), 0..0);
    }
}
