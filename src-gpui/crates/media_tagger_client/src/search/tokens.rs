//! Tag tokens of the search syntax (`src/utils/searchTokens.ts`).
//!
//! A token is a tag as the user types it in the search field. Double quotes make a
//! literal, so a stored tag that looks like an operator (`tags`, `tags:3`, `gn:trip`)
//! or starts with `-` can be searched for. Inside quotes `\"` is a quote and `\\` a
//! backslash. Stored tag names never carry these quotes.

use super::text::{is_js_line_terminator, starts_with_ignore_ascii_case};

/// `serializeTagToken`: `tag` as the search field writes it, quoted when it would
/// otherwise read as an operator, an exclusion or an unfinished literal, with `-` in
/// front for an excluded tag.
pub fn serialize_tag_token(tag: &str, negative: bool) -> String {
    let literal = tag.starts_with('-') || looks_like_operator(tag) || tag.contains(['"', '\\']);
    let mut token = String::with_capacity(tag.len() + 3);
    if negative {
        token.push('-');
    }
    if literal {
        token.push('"');
        for c in tag.chars() {
            if matches!(c, '"' | '\\') {
                token.push('\\');
            }
            token.push(c);
        }
        token.push('"');
    } else {
        token.push_str(tag);
    }
    token
}

/// `/^(?:tags(?::.*)?|gn:.*)$/i`: the whole tag reads as `tags`, `tags:…` or `gn:…`
/// (`.` matches no line terminator).
fn looks_like_operator(tag: &str) -> bool {
    let rest = if starts_with_ignore_ascii_case(tag, "tags") {
        let rest = &tag[4..];
        if rest.is_empty() {
            return true;
        }
        match rest.strip_prefix(':') {
            Some(rest) => rest,
            None => return false,
        }
    } else if starts_with_ignore_ascii_case(tag, "gn:") {
        &tag[3..]
    } else {
        return false;
    };
    !rest.contains(is_js_line_terminator)
}

/// What `readTagToken` makes of one raw token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagToken {
    /// The tag, lowercased; for a literal the text between the quotes, unescaped.
    pub value: String,
    /// A leading `-` with something after it.
    pub negative: bool,
    /// The tag part starts with a double quote.
    pub quoted: bool,
    /// Not a literal, or a literal whose closing quote is the token's last character
    /// and whose escapes are all valid.
    pub complete: bool,
}

/// `readTagToken`.
pub fn read_tag_token(raw: &str) -> TagToken {
    let negative = raw.starts_with('-') && raw.len() > 1;
    let body = if negative { &raw[1..] } else { raw };
    let Some(inner) = body.strip_prefix('"') else {
        return TagToken {
            value: body.to_lowercase(),
            negative,
            quoted: false,
            complete: true,
        };
    };
    let mut value = String::with_capacity(inner.len());
    let mut chars = inner.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => {
                return TagToken {
                    value: value.to_lowercase(),
                    negative,
                    quoted: true,
                    complete: at + 1 == inner.len(),
                };
            }
            '\\' => match chars.next() {
                Some((_, next @ ('"' | '\\'))) => value.push(next),
                // An unsupported escape: the Solid parser stops here and returns the
                // value as read so far, not lowercased, and incomplete.
                _ => {
                    return TagToken {
                        value,
                        negative,
                        quoted: true,
                        complete: false,
                    }
                }
            },
            c => value.push(c),
        }
    }
    TagToken {
        value: value.to_lowercase(),
        negative,
        quoted: true,
        complete: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{read_tag_token, serialize_tag_token, TagToken};

    fn token(value: &str, negative: bool, quoted: bool, complete: bool) -> TagToken {
        TagToken {
            value: value.into(),
            negative,
            quoted,
            complete,
        }
    }

    #[test]
    fn plain_tags_are_written_as_they_are() {
        assert_eq!(serialize_tag_token("cat", false), "cat");
        assert_eq!(serialize_tag_token("cat", true), "-cat");
        assert_eq!(serialize_tag_token("tagstone", false), "tagstone");
        assert_eq!(serialize_tag_token("gnu", false), "gnu");
    }

    #[test]
    fn operator_like_and_escaped_tags_are_quoted() {
        assert_eq!(serialize_tag_token("tags", false), "\"tags\"");
        assert_eq!(serialize_tag_token("TAGS:3", true), "-\"TAGS:3\"");
        assert_eq!(serialize_tag_token("gn:trip", false), "\"gn:trip\"");
        assert_eq!(serialize_tag_token("-holiday", false), "\"-holiday\"");
        assert_eq!(serialize_tag_token("-", false), "\"-\"");
        assert_eq!(serialize_tag_token("say\"hi", false), "\"say\\\"hi\"");
        assert_eq!(serialize_tag_token("a\\b", false), "\"a\\\\b\"");
    }

    #[test]
    fn tokens_are_read_with_their_sign_and_quotes() {
        assert_eq!(read_tag_token("Cat"), token("cat", false, false, true));
        assert_eq!(read_tag_token("-Cat"), token("cat", true, false, true));
        assert_eq!(read_tag_token("-"), token("-", false, false, true));
        assert_eq!(read_tag_token("\"Tags\""), token("tags", false, true, true));
        assert_eq!(read_tag_token("-\"-x\""), token("-x", true, true, true));
        assert_eq!(
            read_tag_token("\"a\\\"b\""),
            token("a\"b", false, true, true)
        );
        assert_eq!(read_tag_token("\"tags"), token("tags", false, true, false));
        assert_eq!(
            read_tag_token("\"tags\"x"),
            token("tags", false, true, false)
        );
        assert_eq!(
            read_tag_token("\"Bad\\q\""),
            token("Bad", false, true, false)
        );
    }
}
