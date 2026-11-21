//! CLDR cardinal plural rules of the eleven languages, as `Intl.PluralRules` applies
//! them for i18next's `_one`/`_few`/`_many`/`_other` keys.
//!
//! Written from the CLDR 48 rules (the data of Node 22 and WebKitGTK at the time of the
//! port) for the operands a count can have: `i` (the integer digits of the absolute
//! value) and whether visible fraction digits exist (`v != 0`). The compact exponent
//! `e` is always 0. The tests pin every rule with vectors taken from `Intl.PluralRules`.

use super::Language;

/// A CLDR plural category; [`Self::suffix`] is the i18next key suffix. The
/// eleven languages use no `zero` and no `two` category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    One,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    /// `_one`, `_few`, ...: the suffix i18next appends to a key for a count.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::One => "_one",
            Self::Few => "_few",
            Self::Many => "_many",
            Self::Other => "_other",
        }
    }
}

/// The plural operands of a count: `i` and whether it has a fraction (`v != 0`).
/// The sign does not matter (CLDR uses the absolute value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluralOperands {
    /// The integer digits of the absolute value.
    pub i: u64,
    /// Whether the value has visible fraction digits.
    pub fraction: bool,
}

impl PluralOperands {
    pub const fn integer(value: i64) -> Self {
        Self {
            i: value.unsigned_abs(),
            fraction: false,
        }
    }

    /// The operands of a JavaScript number: `1.0` is the integer 1, `1.5` has a
    /// fraction. `None` for NaN and infinities, which `Intl.PluralRules` maps to
    /// `other`.
    pub fn number(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        let magnitude = value.abs();
        Some(Self {
            // Saturates above u64::MAX; every rule below then reads digits of
            // u64::MAX, which no count reaches.
            i: magnitude.trunc() as u64,
            fraction: magnitude.fract() != 0.0,
        })
    }
}

/// The category of `operands` in `language`.
pub fn plural_category(language: Language, operands: PluralOperands) -> PluralCategory {
    use PluralCategory::{Few, Many, One, Other};

    let PluralOperands { i, fraction } = operands;
    let whole = !fraction;
    let i10 = i % 10;
    let i100 = i % 100;
    // `e = 0 and i != 0 and i % 1000000 = 0 and v = 0` (fr, it, es).
    let million = whole && i != 0 && i % 1_000_000 == 0;
    match language {
        Language::En | Language::De => {
            if whole && i == 1 {
                One
            } else {
                Other
            }
        }
        Language::It | Language::Es => {
            if whole && i == 1 {
                One
            } else if million {
                Many
            } else {
                Other
            }
        }
        Language::Fr => {
            if i <= 1 {
                One
            } else if million {
                Many
            } else {
                Other
            }
        }
        Language::Pl => {
            if !whole {
                Other
            } else if i == 1 {
                One
            } else if (2..=4).contains(&i10) && !(12..=14).contains(&i100) {
                Few
            } else {
                Many
            }
        }
        Language::Ru => {
            if !whole {
                Other
            } else if i10 == 1 && i100 != 11 {
                One
            } else if (2..=4).contains(&i10) && !(12..=14).contains(&i100) {
                Few
            } else {
                Many
            }
        }
        Language::Cs => {
            if !whole {
                Many
            } else if i == 1 {
                One
            } else if (2..=4).contains(&i) {
                Few
            } else {
                Other
            }
        }
        Language::Zh | Language::Ja | Language::Ko => Other,
    }
}

#[cfg(test)]
mod tests {
    use super::{plural_category, PluralCategory, PluralOperands};
    use crate::i18n::Language;

    /// The counts every language is pinned with.
    const VECTOR: [i64; 8] = [0, 1, 2, 5, 12, 21, 22, 101];

    fn letter(category: PluralCategory) -> char {
        match category {
            PluralCategory::One => '1',
            PluralCategory::Few => 'f',
            PluralCategory::Many => 'm',
            PluralCategory::Other => 'o',
        }
    }

    fn categories(language: Language, counts: impl IntoIterator<Item = PluralOperands>) -> String {
        counts
            .into_iter()
            .map(|operands| letter(plural_category(language, operands)))
            .collect()
    }

    /// `new Intl.PluralRules(code).select(n)` for [`VECTOR`] in Node 22 (ICU 78,
    /// CLDR 48); `1` one, `f` few, `m` many, `o` other.
    #[test]
    fn every_language_matches_intl_plural_rules_on_the_vector() {
        for (language, expected) in [
            (Language::En, "o1oooooo"),
            (Language::Pl, "m1fmmmfm"),
            (Language::Fr, "11oooooo"),
            (Language::De, "o1oooooo"),
            (Language::It, "o1oooooo"),
            (Language::Es, "o1oooooo"),
            (Language::Ru, "m1fmm1f1"),
            (Language::Zh, "oooooooo"),
            (Language::Ja, "oooooooo"),
            (Language::Ko, "oooooooo"),
            (Language::Cs, "o1fooooo"),
        ] {
            assert_eq!(
                categories(language, VECTOR.map(PluralOperands::integer)),
                expected,
                "{}",
                language.code()
            );
        }
    }

    /// `select(n)` for every count from 0 to 120.
    #[test]
    fn every_language_matches_intl_plural_rules_up_to_120() {
        for (language, expected) in [
            (Language::En, "o1ooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
            (Language::Pl, "m1fffmmmmmmmmmmmmmmmmmfffmmmmmmmfffmmmmmmmfffmmmmmmmfffmmmmmmmfffmmmmmmmfffmmmmmmmfffmmmmmmmfffmmmmmmmfffmmmmmmmmmmmmmmmm"),
            (Language::Fr, "11ooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
            (Language::De, "o1ooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
            (Language::It, "o1ooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
            (Language::Es, "o1ooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
            (Language::Ru, "m1fffmmmmmmmmmmmmmmmm1fffmmmmmm1fffmmmmmm1fffmmmmmm1fffmmmmmm1fffmmmmmm1fffmmmmmm1fffmmmmmm1fffmmmmmm1fffmmmmmmmmmmmmmmmm"),
            (Language::Zh, "ooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
            (Language::Ja, "ooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
            (Language::Ko, "ooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
            (Language::Cs, "o1fffoooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo"),
        ] {
            assert_eq!(
                categories(language, (0..=120).map(PluralOperands::integer)),
                expected,
                "{}",
                language.code()
            );
        }
    }

    /// Millions (`many` in fr, it, es), the 11-14 exceptions and negative counts.
    #[test]
    fn edge_counts_match_intl_plural_rules() {
        let millions = [1_000_000, 2_000_000, 1_000_001, 3_000_000_000];
        for (language, expected) in [
            (Language::En, "oooo"),
            (Language::Pl, "mmmm"),
            (Language::Fr, "mmom"),
            (Language::It, "mmom"),
            (Language::Es, "mmom"),
            (Language::Ru, "mm1m"),
            (Language::Cs, "oooo"),
        ] {
            assert_eq!(
                categories(language, millions.map(PluralOperands::integer)),
                expected,
                "{} millions",
                language.code()
            );
        }

        let teens = [11, 12, 14, 111, 112, 1001, 1002, 1012];
        assert_eq!(
            categories(Language::Pl, teens.map(PluralOperands::integer)),
            "mmmmmmfm"
        );
        assert_eq!(
            categories(Language::Ru, teens.map(PluralOperands::integer)),
            "mmmmm1fm"
        );
        assert_eq!(
            categories(Language::Cs, [3, 4, 5].map(PluralOperands::integer)),
            "ffo"
        );

        for language in Language::ALL {
            assert_eq!(
                plural_category(language, PluralOperands::integer(-1)),
                plural_category(language, PluralOperands::integer(1)),
                "{}",
                language.code()
            );
        }
    }

    /// `select(0.5)`, `select(1.5)`, `select(2.5)` and `select(1.0)`.
    #[test]
    fn fractions_match_intl_plural_rules() {
        let counts = [0.5, 1.5, 2.5, 1.0].map(|n| PluralOperands::number(n).unwrap());
        for (language, expected) in [
            (Language::En, "ooo1"),
            (Language::Pl, "ooo1"),
            (Language::Fr, "11o1"),
            (Language::De, "ooo1"),
            (Language::It, "ooo1"),
            (Language::Es, "ooo1"),
            (Language::Ru, "ooo1"),
            (Language::Zh, "oooo"),
            (Language::Cs, "mmm1"),
        ] {
            assert_eq!(
                categories(language, counts),
                expected,
                "{}",
                language.code()
            );
        }
        assert_eq!(PluralOperands::number(f64::NAN), None);
        assert_eq!(PluralOperands::number(f64::INFINITY), None);
        assert_eq!(
            PluralOperands::number(-2.0),
            Some(PluralOperands::integer(2))
        );
    }

    #[test]
    fn suffixes_are_the_i18next_ones() {
        assert_eq!(PluralCategory::One.suffix(), "_one");
        assert_eq!(PluralCategory::Few.suffix(), "_few");
        assert_eq!(PluralCategory::Many.suffix(), "_many");
        assert_eq!(PluralCategory::Other.suffix(), "_other");
    }
}
