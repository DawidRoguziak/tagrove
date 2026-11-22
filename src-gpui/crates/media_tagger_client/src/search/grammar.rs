//! The search grammar (`parseSearchFilter` and `readGroupNameFilterValue` of
//! `src/utils/media.ts`; `docs/subsystems/search-tags-and-media-groups.md`, "Search
//! grammar and filter semantics").
//!
//! The parser trims the whole input first and matches reserved words without regard to
//! ASCII case:
//!
//! - nothing: no filter;
//! - `gN:value` at the start of the input: everything after the first colon is one
//!   group name, trimmed at its edges (`gN:Trip 2026` is one filter);
//! - `tags` or `tags:N` as the only token: an exact tag count;
//! - otherwise whitespace-separated tag tokens, `-` in front excludes, quotes make a
//!   literal ([`super::tokens`]).
//!
//! The backend normalizes every filter again; this decides what the user is told.

use super::{
    text::{is_js_line_terminator, js_trim, js_words, starts_with_ignore_ascii_case},
    tokens::read_tag_token,
};
use crate::{backend::MetaFilter, library::is_valid_tag};

/// `Number.MAX_SAFE_INTEGER`: the largest `tags:N` the Solid parser accepts.
pub const MAX_TAG_COUNT: i64 = 9_007_199_254_740_991;

/// `SearchFilterValidationError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchFilterValidationError {
    MetaTagRequiresSolo,
    HasNoTagsInvalidCount,
    GroupNameMissingValue,
    TagInvalidCharacters,
}

impl SearchFilterValidationError {
    /// The name in the Solid frontend and in the locale files.
    pub const fn name(self) -> &'static str {
        match self {
            Self::MetaTagRequiresSolo => "metaTagRequiresSolo",
            Self::HasNoTagsInvalidCount => "hasNoTagsInvalidCount",
            Self::GroupNameMissingValue => "groupNameMissingValue",
            Self::TagInvalidCharacters => "tagInvalidCharacters",
        }
    }

    /// The message's key (`search.errors.<name>`).
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::MetaTagRequiresSolo => "search.errors.metaTagRequiresSolo",
            Self::HasNoTagsInvalidCount => "search.errors.hasNoTagsInvalidCount",
            Self::GroupNameMissingValue => "search.errors.groupNameMissingValue",
            Self::TagInvalidCharacters => "search.errors.tagInvalidCharacters",
        }
    }

    pub const ALL: [Self; 4] = [
        Self::MetaTagRequiresSolo,
        Self::HasNoTagsInvalidCount,
        Self::GroupNameMissingValue,
        Self::TagInvalidCharacters,
    ];
}

/// `mode` of `ParsedSearchFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Tags,
    Meta,
}

/// `ParsedSearchFilter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSearchFilter {
    pub mode: FilterMode,
    /// Included tags, lowercased, first-seen order without duplicates.
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub meta_filter: Option<MetaFilter>,
    pub validation_error: Option<SearchFilterValidationError>,
}

impl ParsedSearchFilter {
    fn tags(include: Vec<String>, exclude: Vec<String>) -> Self {
        Self {
            mode: FilterMode::Tags,
            include,
            exclude,
            meta_filter: None,
            validation_error: None,
        }
    }

    fn meta(meta_filter: MetaFilter) -> Self {
        Self {
            mode: FilterMode::Meta,
            include: Vec::new(),
            exclude: Vec::new(),
            meta_filter: Some(meta_filter),
            validation_error: None,
        }
    }

    fn invalid(error: SearchFilterValidationError) -> Self {
        Self {
            validation_error: Some(error),
            ..Self::tags(Vec::new(), Vec::new())
        }
    }
}

/// `readGroupNameFilterValue`: the group name when the trimmed input starts with
/// `gN:` (any ASCII case), trimmed at its edges; `None` when the rule does not apply.
/// Autocomplete uses the same rule, so no word of a group name gets suggestions.
pub fn read_group_name_filter_value(input: &str) -> Option<&str> {
    let trimmed = js_trim(input);
    if !starts_with_ignore_ascii_case(trimmed, "gn:") {
        return None;
    }
    let value = &trimmed[3..];
    // `/^gn:(.*)$/i`: `.` stops at a line terminator, and `$` is the end of the input.
    if value.contains(is_js_line_terminator) {
        return None;
    }
    Some(js_trim(value))
}

/// `/^tags(?::.*)?$/i` for one whitespace-free token.
fn is_tags_operator(token: &str) -> bool {
    starts_with_ignore_ascii_case(token, "tags")
        && (token.len() == 4 || token[4..].starts_with(':'))
}

/// `tags:N`'s count: one or more ASCII digits, at most [`MAX_TAG_COUNT`]; leading
/// zeroes are fine.
fn parse_tag_count(raw: &str) -> Option<i64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = raw.trim_start_matches('0');
    if digits.len() > 16 {
        return None;
    }
    let count: i64 = if digits.is_empty() {
        0
    } else {
        digits.parse().ok()?
    };
    (count <= MAX_TAG_COUNT).then_some(count)
}

/// `parseSearchFilter`.
pub fn parse_search_filter(input: &str) -> ParsedSearchFilter {
    let trimmed = js_trim(input);
    if trimmed.is_empty() {
        return ParsedSearchFilter::tags(Vec::new(), Vec::new());
    }

    if let Some(group_name) = read_group_name_filter_value(trimmed) {
        if group_name.is_empty() {
            return ParsedSearchFilter::invalid(SearchFilterValidationError::GroupNameMissingValue);
        }
        return ParsedSearchFilter::meta(MetaFilter::GroupName {
            group_name: group_name.to_owned(),
        });
    }

    let tokens: Vec<&str> = js_words(trimmed).collect();
    let parsed: Vec<_> = tokens.iter().map(|token| read_tag_token(token)).collect();
    if tokens
        .iter()
        .zip(&parsed)
        .any(|(token, parsed)| !parsed.quoted && starts_with_ignore_ascii_case(token, "gn:"))
    {
        return ParsedSearchFilter::invalid(SearchFilterValidationError::MetaTagRequiresSolo);
    }

    let operators: Vec<&str> = tokens
        .iter()
        .copied()
        .filter(|token| is_tags_operator(token))
        .collect();
    if let Some(operator) = operators.first() {
        if tokens.len() != 1 || operators.len() != 1 {
            return ParsedSearchFilter::invalid(SearchFilterValidationError::MetaTagRequiresSolo);
        }
        let Some((_, raw_count)) = operator.split_once(':') else {
            return ParsedSearchFilter::meta(MetaFilter::HasNoTags { tag_count: 0 });
        };
        return match parse_tag_count(js_trim(raw_count)) {
            Some(tag_count) => ParsedSearchFilter::meta(MetaFilter::HasNoTags { tag_count }),
            None => ParsedSearchFilter::invalid(SearchFilterValidationError::HasNoTagsInvalidCount),
        };
    }

    if parsed
        .iter()
        .any(|token| !token.complete || !is_valid_tag(&token.value))
    {
        return ParsedSearchFilter::invalid(SearchFilterValidationError::TagInvalidCharacters);
    }

    let mut include: Vec<String> = Vec::new();
    let mut exclude: Vec<String> = Vec::new();
    for token in parsed {
        let side = if token.negative {
            &mut exclude
        } else {
            &mut include
        };
        if !side.contains(&token.value) {
            side.push(token.value);
        }
    }
    ParsedSearchFilter::tags(include, exclude)
}

/// `parseFilterTags`: the tag sides of a valid tag filter, nothing for anything else.
pub fn parse_filter_tags(input: &str) -> (Vec<String>, Vec<String>) {
    let parsed = parse_search_filter(input);
    if parsed.mode != FilterMode::Tags || parsed.validation_error.is_some() {
        return (Vec::new(), Vec::new());
    }
    (parsed.include, parsed.exclude)
}

#[cfg(test)]
mod tests {
    use super::{parse_search_filter, read_group_name_filter_value, SearchFilterValidationError};
    use crate::backend::MetaFilter;

    #[test]
    fn the_group_rule_takes_the_rest_of_the_input() {
        assert_eq!(
            read_group_name_filter_value("  gN: Trip 2026 "),
            Some("Trip 2026")
        );
        assert_eq!(read_group_name_filter_value("GN:"), Some(""));
        assert_eq!(read_group_name_filter_value("cat gN:x"), None);
        assert_eq!(read_group_name_filter_value("gN:a\nb"), None);
        assert_eq!(
            parse_search_filter("gN:a\nb").validation_error,
            Some(SearchFilterValidationError::MetaTagRequiresSolo)
        );
    }

    #[test]
    fn counts_beyond_the_safe_integer_range_are_invalid() {
        assert_eq!(
            parse_search_filter("tags:9007199254740991").meta_filter,
            Some(MetaFilter::HasNoTags {
                tag_count: 9_007_199_254_740_991
            })
        );
        assert_eq!(
            parse_search_filter("tags:00").meta_filter,
            Some(MetaFilter::HasNoTags { tag_count: 0 })
        );
        for count in ["9007199254740992", "1e3", "+1", "1.0", "\u{661}"] {
            assert_eq!(
                parse_search_filter(&format!("tags:{count}")).validation_error,
                Some(SearchFilterValidationError::HasNoTagsInvalidCount),
                "{count}"
            );
        }
    }

    #[test]
    fn unrecognised_operator_lookalikes_are_tags() {
        let parsed = parse_search_filter("tagsfoo -tags gnn:value");
        assert_eq!(parsed.include, ["tagsfoo", "gnn:value"]);
        assert_eq!(parsed.exclude, ["tags"]);
    }
}
