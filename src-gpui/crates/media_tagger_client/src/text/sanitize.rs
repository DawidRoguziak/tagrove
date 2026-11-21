//! What a single-line field keeps of inserted text.
//!
//! Typed, pasted, composed and programmatically set text all pass through
//! [`single_line`], so the buffer never holds a line break (GPUI's `shape_line`
//! refuses one) or another control character:
//!
//! - line breaks at the end of the text are dropped (a copied line usually ends in
//!   one);
//! - every other line break becomes one space, `\r\n` counting as one break, so
//!   pasting several lines keeps their words apart;
//! - a tab becomes a space;
//! - any other control character is dropped.
//!
//! Typed text that is nothing but control characters (the Tab or Enter key, where a
//! platform reports them as text) is ignored before it gets here
//! (`LineEditor::insert`).
//!
//! WebKit, which draws the fields of the Tauri interface, does the same with line
//! breaks in pasted text. It keeps a pasted tab, which this field turns into a space:
//! the line is shaped as one run without tab stops, and tags, search terms and the
//! confirmation word all treat the two alike.

use std::borrow::Cow;

fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// `text` as a single-line field stores it; see the module documentation.
pub fn single_line(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|c| c.is_control() || is_line_break(c)) {
        return Cow::Borrowed(text);
    }
    let body = text.trim_end_matches(is_line_break);
    let mut line = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.peek() == Some(&'\n') {
            continue;
        }
        if is_line_break(c) || c == '\t' {
            line.push(' ');
        } else if !c.is_control() {
            line.push(c);
        }
    }
    Cow::Owned(line)
}

#[cfg(test)]
mod tests {
    use super::single_line;
    use std::borrow::Cow;

    #[test]
    fn ordinary_text_is_borrowed_unchanged() {
        let text = "zażółć 👍🏽 漢字";

        assert!(matches!(single_line(text), Cow::Borrowed(kept) if kept == text));
    }

    #[test]
    fn inner_line_breaks_become_one_space_each() {
        assert_eq!(single_line("one\ntwo"), "one two");
        assert_eq!(single_line("one\r\ntwo\rthree"), "one two three");
        assert_eq!(single_line("a\n\nb"), "a  b");
        assert_eq!(single_line("a\u{2028}b\u{2029}c\u{85}d"), "a b c d");
    }

    #[test]
    fn trailing_line_breaks_are_dropped() {
        assert_eq!(single_line("tag\n"), "tag");
        assert_eq!(single_line("one\ntwo\r\n\r\n"), "one two");
        assert_eq!(single_line("\n"), "");
        assert_eq!(single_line("\r\n"), "");
    }

    #[test]
    fn leading_line_breaks_are_spaces() {
        assert_eq!(single_line("\ntag"), " tag");
    }

    #[test]
    fn tabs_become_spaces_and_other_controls_are_dropped() {
        assert_eq!(single_line("a\tb"), "a b");
        assert_eq!(single_line("a\u{0}b\u{7f}c\u{1b}d"), "abcd");
    }
}
