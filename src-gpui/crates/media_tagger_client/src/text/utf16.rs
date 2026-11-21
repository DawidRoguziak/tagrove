//! Conversions between the UTF-8 byte offsets the editing model works in and the
//! UTF-16 code unit offsets of GPUI's input handler (the platform's IME protocol).
//!
//! Slicing a `str` at an offset inside a character panics, so every result is a
//! character boundary whatever the platform sends: an offset past the end is the end,
//! and an offset inside a character (the low half of a surrogate pair, or a byte offset
//! inside a multi-byte character) counts as the end of that character.

use std::ops::Range;

/// The length of `text` in UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The byte offset of the UTF-16 offset `offset_utf16`.
pub fn byte_offset(text: &str, offset_utf16: usize) -> usize {
    let mut bytes = 0;
    let mut units = 0;
    for c in text.chars() {
        if units >= offset_utf16 {
            break;
        }
        units += c.len_utf16();
        bytes += c.len_utf8();
    }
    bytes
}

/// The UTF-16 offset of the byte offset `offset`.
pub fn utf16_offset(text: &str, offset: usize) -> usize {
    let mut bytes = 0;
    let mut units = 0;
    for c in text.chars() {
        if bytes >= offset {
            break;
        }
        bytes += c.len_utf8();
        units += c.len_utf16();
    }
    units
}

/// The byte range of a UTF-16 range. A range that ends before it starts is empty at
/// its start.
pub fn byte_range(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
    let start = byte_offset(text, range_utf16.start);
    let end = byte_offset(text, range_utf16.end.max(range_utf16.start));
    start..end
}

/// The UTF-16 range of a byte range.
pub fn utf16_range(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = utf16_offset(text, range.start);
    let end = utf16_offset(text, range.end.max(range.start));
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    // "e" is 1 byte and 1 unit, "🙂" 4 bytes and 2 units, "漢" 3 bytes and 1 unit.
    const TEXT: &str = "e🙂漢";

    #[test]
    fn offsets_convert_in_both_directions() {
        assert_eq!(utf16_len(TEXT), 4);
        for (bytes, units) in [(0, 0), (1, 1), (5, 3), (8, 4)] {
            assert_eq!(byte_offset(TEXT, units), bytes, "{units} units");
            assert_eq!(utf16_offset(TEXT, bytes), units, "{bytes} bytes");
        }
    }

    #[test]
    fn offsets_inside_a_character_count_as_its_end() {
        // The low surrogate of the emoji.
        assert_eq!(byte_offset(TEXT, 2), 5);
        // Bytes inside the emoji and inside the ideograph.
        assert_eq!(utf16_offset(TEXT, 3), 3);
        assert_eq!(utf16_offset(TEXT, 6), 4);
    }

    #[test]
    fn offsets_past_the_end_are_the_end() {
        assert_eq!(byte_offset(TEXT, 99), TEXT.len());
        assert_eq!(utf16_offset(TEXT, 99), 4);
        assert_eq!(byte_offset("", 3), 0);
    }

    #[test]
    fn ranges_convert_and_never_run_backwards() {
        assert_eq!(byte_range(TEXT, &(1..3)), 1..5);
        assert_eq!(byte_range(TEXT, &(3..99)), 5..8);
        assert_eq!(utf16_range(TEXT, &(1..5)), 1..3);
        // A range that runs backwards, as a confused caller could send it.
        let backwards = |start, end| Range { start, end };
        assert_eq!(byte_range(TEXT, &backwards(3, 1)), 5..5);
        assert_eq!(utf16_range(TEXT, &backwards(5, 1)), 3..3);
    }

    #[test]
    fn every_converted_offset_can_slice_the_text() {
        let text = "a\u{301}👨\u{200d}👩\u{200d}👧漢🇵🇱";
        for units in 0..=utf16_len(text) + 2 {
            let offset = byte_offset(text, units);
            assert!(text.is_char_boundary(offset), "{units} units -> {offset}");
        }
    }
}
