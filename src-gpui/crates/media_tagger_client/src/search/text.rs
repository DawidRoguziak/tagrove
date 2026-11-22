//! What JavaScript calls whitespace, for the search grammar.
//!
//! The Solid parser splits on `/\s+/` and trims with `String.prototype.trim`; both use
//! the ECMAScript set of white space and line terminators. It differs from Rust's
//! `char::is_whitespace` (Unicode `White_Space`) in two characters: U+FEFF is
//! whitespace in JavaScript and not in Unicode, U+0085 the other way round. The
//! grammar keeps JavaScript's set, so a draft parses the same in both interfaces.

/// `/\s/` of JavaScript.
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{A}' | '\u{B}' | '\u{C}' | '\u{D}' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// A line terminator, which `.` of a JavaScript regular expression does not match.
pub fn is_js_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `String.prototype.trim`.
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

/// `text.split(/\s+/).filter(Boolean)`.
pub fn js_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(is_js_whitespace).filter(|word| !word.is_empty())
}

/// Whether `text` starts with `prefix`, ASCII letters compared without case (the `i`
/// flag on an ASCII pattern).
pub fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

#[cfg(test)]
mod tests {
    use super::{is_js_whitespace, js_trim, js_words, starts_with_ignore_ascii_case};

    #[test]
    fn javascript_whitespace_includes_the_byte_order_mark_and_not_next_line() {
        assert!(is_js_whitespace('\u{FEFF}'));
        assert!(!is_js_whitespace('\u{85}'));
        assert!(is_js_whitespace('\u{3000}'));
        assert_eq!(js_trim("\u{FEFF} cat\u{A0}"), "cat");
        assert_eq!(js_trim("\u{85}cat"), "\u{85}cat");
        assert_eq!(
            js_words(" cat \t dog\u{2028}x ").collect::<Vec<_>>(),
            ["cat", "dog", "x"]
        );
    }

    #[test]
    fn prefixes_ignore_ascii_case_only() {
        assert!(starts_with_ignore_ascii_case("gN:trip", "gn:"));
        assert!(starts_with_ignore_ascii_case("TAGS", "tags"));
        assert!(!starts_with_ignore_ascii_case("ta", "tags"));
        assert!(!starts_with_ignore_ascii_case("żgn:", "gn:"));
    }
}
