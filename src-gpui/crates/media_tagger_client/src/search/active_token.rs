//! The token under the caret and how a suggestion replaces it
//! (`suggestions/tokenService.ts`, `suggestions/inputService.ts`).
//!
//! Offsets are byte offsets into the UTF-8 text, as the text field reports its caret;
//! the Solid code counts UTF-16 units of `selectionStart`. Both describe the same
//! positions in the same text.

use std::collections::BTreeSet;

use super::{
    grammar::read_group_name_filter_value,
    text::{is_js_whitespace, js_trim, js_words},
    tokens::{read_tag_token, serialize_tag_token},
};

/// `ActiveToken`: the whitespace-delimited token around the caret.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActiveToken {
    /// Byte range of the whole token in the text.
    pub start: usize,
    pub end: usize,
    /// The tag being typed: trimmed, lowercased, without its `-` and its quotes.
    pub query: String,
    pub negative: bool,
    /// Written in quotes: operators lose their meaning.
    pub literal: bool,
}

/// `normalizeTagToken`.
pub fn normalize_tag_token(token: &str) -> String {
    read_tag_token(js_trim(token)).value
}

/// The nearest character boundary at or before `at`, inside `text`.
fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// `readActiveToken`: the token around `caret`, scanning left and right to
/// whitespace. `None` for no token, a lone `-`, an empty literal, or while the
/// anchored `gN:` rule consumes the whole input as one group name.
pub fn read_active_token(input: &str, caret: usize) -> Option<ActiveToken> {
    if read_group_name_filter_value(input).is_some() {
        return None;
    }
    let position = floor_boundary(input, caret);
    let start = input[..position]
        .char_indices()
        .rev()
        .find(|(_, c)| is_js_whitespace(*c))
        .map_or(0, |(at, c)| at + c.len_utf8());
    let end = input[position..]
        .char_indices()
        .find(|(_, c)| is_js_whitespace(*c))
        .map_or(input.len(), |(at, _)| position + at);
    let raw = js_trim(&input[start..end]);
    if raw.is_empty() {
        return None;
    }
    let token = read_tag_token(raw);
    let query = normalize_tag_token(raw);
    if query.is_empty() || raw == "-" {
        return None;
    }
    Some(ActiveToken {
        start,
        end,
        query,
        negative: token.negative,
        literal: token.quoted,
    })
}

/// `collectUsedTags`: the tags of the draft (search syntax decoded) and the hard
/// exclusions (stored names, trimmed and lowercased).
pub fn collect_used_tags(value: &str, excluded: &[String]) -> BTreeSet<String> {
    js_words(value)
        .map(normalize_tag_token)
        .filter(|tag| !tag.is_empty())
        .chain(
            excluded
                .iter()
                .map(|tag| js_trim(tag).to_lowercase())
                .filter(|tag| !tag.is_empty()),
        )
        .collect()
}

/// `applySuggestionToValue`: the text with `tag` in place of the token (quoted when
/// the syntax needs it, its `-` kept), and the caret right after it.
pub fn apply_suggestion_to_value(value: &str, token: &ActiveToken, tag: &str) -> (String, usize) {
    let inserted = serialize_tag_token(tag, token.negative);
    let mut next = String::with_capacity(value.len() + inserted.len());
    next.push_str(&value[..token.start]);
    next.push_str(&inserted);
    next.push_str(&value[token.end..]);
    (next, token.start + inserted.len())
}

/// What [`apply_suggestion_to_value`] puts in place of the token.
pub fn suggestion_replacement(token: &ActiveToken, tag: &str) -> String {
    serialize_tag_token(tag, token.negative)
}
