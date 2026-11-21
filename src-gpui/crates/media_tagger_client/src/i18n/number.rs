//! `new Intl.NumberFormat(language).format(count)` for whole counts, the value the
//! Solid views pass as `formattedCount` (`gallery.resultCount`,
//! `workspace.itemCount`, `settings.duplicates.lastResult*`).

use super::Language;

/// How a language groups the digits of an integer (CLDR 48 decimal format).
struct Grouping {
    separator: char,
    /// CLDR `minimumGroupingDigits`: with 2, a four-digit number stays ungrouped.
    minimum_digits: usize,
}

const fn grouping(language: Language) -> Grouping {
    let (separator, minimum_digits) = match language {
        Language::En | Language::Zh | Language::Ja | Language::Ko => (',', 1),
        Language::De => ('.', 1),
        Language::It | Language::Es => ('.', 2),
        // NARROW NO-BREAK SPACE.
        Language::Fr => ('\u{202f}', 1),
        // NO-BREAK SPACE.
        Language::Ru | Language::Cs => ('\u{a0}', 1),
        Language::Pl => ('\u{a0}', 2),
    };
    Grouping {
        separator,
        minimum_digits,
    }
}

/// A whole count in the digits and grouping of `language`: `12,345` in English,
/// `12 345` (no-break space) in Polish, `1234` in Spanish. Negative numbers take
/// the ASCII hyphen-minus, as `Intl.NumberFormat` does in all eleven languages.
pub fn format_count(language: Language, count: i64) -> String {
    let Grouping {
        separator,
        minimum_digits,
    } = grouping(language);
    let digits = count.unsigned_abs().to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3 * 3 + 1);
    if count < 0 {
        formatted.push('-');
    }
    // Grouped when the part above the lowest group has at least `minimum_digits`.
    let grouped = digits.len() >= 3 + minimum_digits;
    for (index, digit) in digits.chars().enumerate() {
        let remaining = digits.len() - index;
        if grouped && index > 0 && remaining.is_multiple_of(3) {
            formatted.push(separator);
        }
        formatted.push(digit);
    }
    formatted
}

#[cfg(test)]
mod tests {
    use super::format_count;
    use crate::i18n::Language;

    const COUNTS: [i64; 12] = [
        0, 7, 999, 1000, 1234, 9999, 10000, 12345, 123456, 1234567, 1000000, -1234,
    ];

    /// `new Intl.NumberFormat(code).format(n)` in Node 22 (ICU 78, CLDR 48) for
    /// [`COUNTS`], with `_` for NO-BREAK SPACE and `~` for NARROW NO-BREAK SPACE.
    #[test]
    fn counts_match_intl_number_format() {
        for (language, expected) in [
            (
                Language::En,
                "0 7 999 1,000 1,234 9,999 10,000 12,345 123,456 1,234,567 1,000,000 -1,234",
            ),
            (
                Language::Zh,
                "0 7 999 1,000 1,234 9,999 10,000 12,345 123,456 1,234,567 1,000,000 -1,234",
            ),
            (
                Language::Ja,
                "0 7 999 1,000 1,234 9,999 10,000 12,345 123,456 1,234,567 1,000,000 -1,234",
            ),
            (
                Language::Ko,
                "0 7 999 1,000 1,234 9,999 10,000 12,345 123,456 1,234,567 1,000,000 -1,234",
            ),
            (
                Language::De,
                "0 7 999 1.000 1.234 9.999 10.000 12.345 123.456 1.234.567 1.000.000 -1.234",
            ),
            (
                Language::It,
                "0 7 999 1000 1234 9999 10.000 12.345 123.456 1.234.567 1.000.000 -1234",
            ),
            (
                Language::Es,
                "0 7 999 1000 1234 9999 10.000 12.345 123.456 1.234.567 1.000.000 -1234",
            ),
            (
                Language::Fr,
                "0 7 999 1~000 1~234 9~999 10~000 12~345 123~456 1~234~567 1~000~000 -1~234",
            ),
            (
                Language::Ru,
                "0 7 999 1_000 1_234 9_999 10_000 12_345 123_456 1_234_567 1_000_000 -1_234",
            ),
            (
                Language::Cs,
                "0 7 999 1_000 1_234 9_999 10_000 12_345 123_456 1_234_567 1_000_000 -1_234",
            ),
            (
                Language::Pl,
                "0 7 999 1000 1234 9999 10_000 12_345 123_456 1_234_567 1_000_000 -1234",
            ),
        ] {
            let formatted: Vec<String> = COUNTS
                .iter()
                .map(|&count| {
                    format_count(language, count)
                        .replace('\u{a0}', "_")
                        .replace('\u{202f}', "~")
                })
                .collect();
            assert_eq!(formatted.join(" "), expected, "{}", language.code());
        }
    }

    #[test]
    fn extreme_counts_do_not_overflow() {
        assert_eq!(
            format_count(Language::En, i64::MAX),
            "9,223,372,036,854,775,807"
        );
        assert_eq!(
            format_count(Language::En, i64::MIN),
            "-9,223,372,036,854,775,808"
        );
    }
}
