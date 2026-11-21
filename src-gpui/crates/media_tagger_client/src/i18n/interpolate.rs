//! i18next interpolation as Tagrove configures it (`escapeValue: false`): every
//! `{{name}}` with a value is replaced by it, unescaped.

use std::borrow::Cow;

/// Replaces `{{name}}` placeholders in `template` with `value(name)`.
///
/// Like i18next:
/// - the name is trimmed, and `{{- name}}` (the unescaped form) is the same
///   placeholder, because values are never escaped;
/// - `{{name, format}}` takes the value of `name` unchanged: Tagrove registers no
///   format function, so i18next's default returns the value as it is;
/// - a name without a value keeps its placeholder text (`skipOnVariables`);
/// - an inserted value is not searched for placeholders again.
pub fn interpolate<'a>(
    template: &'a str,
    value: &dyn Fn(&str) -> Option<Cow<'a, str>>,
) -> Cow<'a, str> {
    if !template.contains("{{") {
        return Cow::Borrowed(template);
    }
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        // i18next's `{{(.+?)}}`: the shortest non-empty inner text.
        let inner_start = open + 2;
        // At least one character, which may be several bytes long.
        let Some(first) = rest[inner_start..].chars().next() else {
            break;
        };
        let search_from = inner_start + first.len_utf8();
        let Some(close) = rest[search_from..]
            .find("}}")
            .map(|offset| search_from + offset)
        else {
            break;
        };
        let inner = &rest[inner_start..close];
        let placeholder = &rest[open..close + 2];
        output.push_str(&rest[..open]);
        match value(placeholder_name(inner)) {
            Some(text) => output.push_str(&text),
            None => output.push_str(placeholder),
        }
        rest = &rest[close + 2..];
    }
    output.push_str(rest);
    Cow::Owned(output)
}

/// The variable a placeholder's inner text names.
fn placeholder_name(inner: &str) -> &str {
    let inner = inner.strip_prefix('-').unwrap_or(inner);
    let name = inner.split_once(',').map_or(inner, |(name, _format)| name);
    name.trim()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::interpolate;

    fn with<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<Cow<'a, str>> + 'a {
        |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| Cow::Borrowed(*value))
        }
    }

    #[test]
    fn replaces_every_placeholder_with_its_value() {
        let values = [("count", "3"), ("path", "/media/a")];

        assert_eq!(
            interpolate("{{count}} of {{count}} in {{path}}", &with(&values)),
            "3 of 3 in /media/a"
        );
        assert_eq!(interpolate("{{ count }}", &with(&values)), "3");
        assert_eq!(interpolate("{{- path}}", &with(&values)), "/media/a");
        assert_eq!(interpolate("{{count, number}}", &with(&values)), "3");
    }

    #[test]
    fn values_are_not_escaped_or_interpolated_again() {
        let values = [("tag", "<b>&{{count}}</b>"), ("count", "3")];

        assert_eq!(
            interpolate("Tag {{tag}}", &with(&values)),
            "Tag <b>&{{count}}</b>"
        );
    }

    #[test]
    fn a_placeholder_without_a_value_stays() {
        assert_eq!(
            interpolate("{{missing}} and {{count}}", &with(&[("count", "1")])),
            "{{missing}} and 1"
        );
    }

    #[test]
    fn text_without_complete_placeholders_is_unchanged() {
        let none = with(&[]);
        for text in ["plain", "{{", "{{}}", "a {{ b", "}} {{x", ""] {
            assert_eq!(interpolate(text, &none), text, "{text:?}");
        }
        assert!(matches!(interpolate("plain", &none), Cow::Borrowed(_)));
    }

    #[test]
    fn placeholders_after_non_ascii_text_are_found() {
        let values = [("count", "3"), ("é", "accent")];

        assert_eq!(
            interpolate("{{é}} żółw {{count}} {{missing}}", &with(&values)),
            "accent żółw 3 {{missing}}"
        );
        assert_eq!(interpolate("{{ż}} {{count}}", &with(&values)), "{{ż}} 3");
    }

    #[test]
    fn the_shortest_inner_text_is_taken() {
        let values = [("a", "1"), ("}", "brace")];

        assert_eq!(interpolate("{{a}}}}", &with(&values)), "1}}");
        assert_eq!(interpolate("{{}}}", &with(&values)), "brace");
    }
}
