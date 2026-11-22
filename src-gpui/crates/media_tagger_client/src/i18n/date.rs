//! `new Intl.DateTimeFormat(language, { year: "numeric", month: "long", timeZone: "UTC" })`
//! for the first day of a month: the label of a timeline month
//! (`formatTimelineMonth` in `src/features/gallery/timeline.ts`).
//!
//! The month names and patterns are CLDR's (ICU 78, CLDR 48, the data Node 22 and
//! WebKitGTK's ICU format with); a test holds every month of every language to what
//! `Intl` printed.

use super::Language;

/// The stand-alone month names (`MMMM` in the `yMMMM` skeleton) of the languages
/// that put the name before the year. Chinese, Japanese and Korean write the month
/// as a number.
const fn month_names(language: Language) -> Option<[&'static str; 12]> {
    Some(match language {
        Language::En => [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ],
        Language::Pl => [
            "styczeń",
            "luty",
            "marzec",
            "kwiecień",
            "maj",
            "czerwiec",
            "lipiec",
            "sierpień",
            "wrzesień",
            "październik",
            "listopad",
            "grudzień",
        ],
        Language::Fr => [
            "janvier",
            "février",
            "mars",
            "avril",
            "mai",
            "juin",
            "juillet",
            "août",
            "septembre",
            "octobre",
            "novembre",
            "décembre",
        ],
        Language::De => [
            "Januar",
            "Februar",
            "März",
            "April",
            "Mai",
            "Juni",
            "Juli",
            "August",
            "September",
            "Oktober",
            "November",
            "Dezember",
        ],
        Language::It => [
            "gennaio",
            "febbraio",
            "marzo",
            "aprile",
            "maggio",
            "giugno",
            "luglio",
            "agosto",
            "settembre",
            "ottobre",
            "novembre",
            "dicembre",
        ],
        Language::Es => [
            "enero",
            "febrero",
            "marzo",
            "abril",
            "mayo",
            "junio",
            "julio",
            "agosto",
            "septiembre",
            "octubre",
            "noviembre",
            "diciembre",
        ],
        Language::Ru => [
            "январь",
            "февраль",
            "март",
            "апрель",
            "май",
            "июнь",
            "июль",
            "август",
            "сентябрь",
            "октябрь",
            "ноябрь",
            "декабрь",
        ],
        Language::Cs => [
            "leden",
            "únor",
            "březen",
            "duben",
            "květen",
            "červen",
            "červenec",
            "srpen",
            "září",
            "říjen",
            "listopad",
            "prosinec",
        ],
        Language::Zh | Language::Ja | Language::Ko => return None,
    })
}

/// `month` (1 to 12) of `year` as `Intl` writes it in `language`: `March 2024`,
/// `marzo de 2024`, `март 2024 г.`, `2024年3月`, `2024년 3월`. A month outside 1 to 12
/// gives `2024-13`, the fallback of `formatTimelineMonth`.
pub fn format_year_month(language: Language, year: u32, month: u32) -> String {
    if !(1..=12).contains(&month) {
        return format!("{year}-{month:02}");
    }
    let Some(names) = month_names(language) else {
        return match language {
            Language::Ko => format!("{year}년 {month}월"),
            // Chinese and Japanese.
            _ => format!("{year}年{month}月"),
        };
    };
    let name = names[(month - 1) as usize];
    match language {
        Language::Es => format!("{name} de {year}"),
        Language::Ru => format!("{name} {year} г."),
        _ => format!("{name} {year}"),
    }
}

#[cfg(test)]
mod tests {
    use super::format_year_month;
    use crate::i18n::Language;

    /// `new Intl.DateTimeFormat(code, { year: "numeric", month: "long", timeZone: "UTC" })`
    /// of the first day of every month of 2024, in Node 22 (ICU 78, CLDR 48).
    const INTL: [(Language, [&str; 12]); 11] = [
        (
            Language::En,
            [
                "January 2024",
                "February 2024",
                "March 2024",
                "April 2024",
                "May 2024",
                "June 2024",
                "July 2024",
                "August 2024",
                "September 2024",
                "October 2024",
                "November 2024",
                "December 2024",
            ],
        ),
        (
            Language::Pl,
            [
                "styczeń 2024",
                "luty 2024",
                "marzec 2024",
                "kwiecień 2024",
                "maj 2024",
                "czerwiec 2024",
                "lipiec 2024",
                "sierpień 2024",
                "wrzesień 2024",
                "październik 2024",
                "listopad 2024",
                "grudzień 2024",
            ],
        ),
        (
            Language::Fr,
            [
                "janvier 2024",
                "février 2024",
                "mars 2024",
                "avril 2024",
                "mai 2024",
                "juin 2024",
                "juillet 2024",
                "août 2024",
                "septembre 2024",
                "octobre 2024",
                "novembre 2024",
                "décembre 2024",
            ],
        ),
        (
            Language::De,
            [
                "Januar 2024",
                "Februar 2024",
                "März 2024",
                "April 2024",
                "Mai 2024",
                "Juni 2024",
                "Juli 2024",
                "August 2024",
                "September 2024",
                "Oktober 2024",
                "November 2024",
                "Dezember 2024",
            ],
        ),
        (
            Language::It,
            [
                "gennaio 2024",
                "febbraio 2024",
                "marzo 2024",
                "aprile 2024",
                "maggio 2024",
                "giugno 2024",
                "luglio 2024",
                "agosto 2024",
                "settembre 2024",
                "ottobre 2024",
                "novembre 2024",
                "dicembre 2024",
            ],
        ),
        (
            Language::Es,
            [
                "enero de 2024",
                "febrero de 2024",
                "marzo de 2024",
                "abril de 2024",
                "mayo de 2024",
                "junio de 2024",
                "julio de 2024",
                "agosto de 2024",
                "septiembre de 2024",
                "octubre de 2024",
                "noviembre de 2024",
                "diciembre de 2024",
            ],
        ),
        (
            Language::Ru,
            [
                "январь 2024 г.",
                "февраль 2024 г.",
                "март 2024 г.",
                "апрель 2024 г.",
                "май 2024 г.",
                "июнь 2024 г.",
                "июль 2024 г.",
                "август 2024 г.",
                "сентябрь 2024 г.",
                "октябрь 2024 г.",
                "ноябрь 2024 г.",
                "декабрь 2024 г.",
            ],
        ),
        (
            Language::Zh,
            [
                "2024年1月",
                "2024年2月",
                "2024年3月",
                "2024年4月",
                "2024年5月",
                "2024年6月",
                "2024年7月",
                "2024年8月",
                "2024年9月",
                "2024年10月",
                "2024年11月",
                "2024年12月",
            ],
        ),
        (
            Language::Ja,
            [
                "2024年1月",
                "2024年2月",
                "2024年3月",
                "2024年4月",
                "2024年5月",
                "2024年6月",
                "2024年7月",
                "2024年8月",
                "2024年9月",
                "2024年10月",
                "2024年11月",
                "2024年12月",
            ],
        ),
        (
            Language::Ko,
            [
                "2024년 1월",
                "2024년 2월",
                "2024년 3월",
                "2024년 4월",
                "2024년 5월",
                "2024년 6월",
                "2024년 7월",
                "2024년 8월",
                "2024년 9월",
                "2024년 10월",
                "2024년 11월",
                "2024년 12월",
            ],
        ),
        (
            Language::Cs,
            [
                "leden 2024",
                "únor 2024",
                "březen 2024",
                "duben 2024",
                "květen 2024",
                "červen 2024",
                "červenec 2024",
                "srpen 2024",
                "září 2024",
                "říjen 2024",
                "listopad 2024",
                "prosinec 2024",
            ],
        ),
    ];

    #[test]
    fn months_match_intl_date_time_format_in_every_language() {
        assert_eq!(INTL.len(), Language::ALL.len());
        for (language, expected) in INTL {
            for (index, text) in expected.into_iter().enumerate() {
                assert_eq!(
                    format_year_month(language, 2024, index as u32 + 1),
                    text,
                    "{language:?} month {}",
                    index + 1
                );
            }
        }
    }

    #[test]
    fn a_month_outside_the_calendar_falls_back_to_digits() {
        assert_eq!(format_year_month(Language::En, 2024, 0), "2024-00");
        assert_eq!(format_year_month(Language::De, 1999, 13), "1999-13");
    }
}
