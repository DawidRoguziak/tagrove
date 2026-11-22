//! The search behaviour of the Solid specification (epic E12), ported case by case:
//! \`src/utils/__tests__/media.test.ts\` and \`literalTagSearch.test.ts\` (grammar and
//! tokens), \`suggestions/__tests__/tokenService.test.ts\` and \`inputService.test.ts\`
//! (the token under the caret), \`__tests__/filterService.test.ts\` and
//! \`createSearch.test.ts\` (draft and applied filters), the cases of
//! \`createSuggestions.test.ts\` and \`SearchTagator.test.tsx\` that are about the
//! suggestion session, \`localization.test.tsx\` (parser errors in every language), and
//! the \`search-applied\` parity state on the real core.
//!
//! Layers: pure functions and models directly; the runtime on the scripted backend and
//! the deterministic executor; filters against SQLite on a fresh temporary profile
//! (\`TempCore\`). The suggestion cases already covered by the unit tests of
//! \`search::matcher\`, \`search::suggestions\` and \`search::combobox\` are not repeated.

use std::{
    collections::{BTreeSet, HashSet},
    sync::{Arc, Mutex},
};

use media_tagger_client::{
    backend::{
        core_backend::test_support::TempCore, AssetIdRange, AssetQueryRequest, Backend,
        FakeBackend, MediaKind, MetaFilter,
    },
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language, Locales},
    library::{LibraryInput, VocabularyInput},
    model::{Driver, Model},
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    search::{
        apply_suggestion_to_value, build_filter_descriptor, build_tag_suggestions,
        collect_used_tags, meta_filter_key, normalize_tag_token, parse_filter_tags,
        parse_search_filter, read_active_token, serialize_tag_token, ActiveToken, Combobox,
        ComboboxConfig, ComboboxEffect, ComboboxKey, EditorId, FilterMode, ParsedSearchFilter,
        Search, SearchCommand, SearchFilterValidationError, SearchFilters, SuggestionContext,
        SuggestionEffect, SuggestionEvent, SuggestionInput, SuggestionRequest, TagIndex,
        MAX_TAG_COUNT,
    },
    theme::ThemeName,
};
use media_tagger_core::models::{StartAssetQueryResult, TagListPage};

const DARK_EN: RuntimeConfig = RuntimeConfig {
    theme: ThemeName::Dark,
    language: Language::En,
};

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn tags_filter(include: &[&str], exclude: &[&str]) -> ParsedSearchFilter {
    ParsedSearchFilter {
        mode: FilterMode::Tags,
        include: strings(include),
        exclude: strings(exclude),
        meta_filter: None,
        validation_error: None,
    }
}

fn meta_filter(meta: MetaFilter) -> ParsedSearchFilter {
    ParsedSearchFilter {
        mode: FilterMode::Meta,
        include: Vec::new(),
        exclude: Vec::new(),
        meta_filter: Some(meta),
        validation_error: None,
    }
}

fn token(start: usize, end: usize, query: &str, negative: bool) -> ActiveToken {
    ActiveToken {
        start,
        end,
        query: query.into(),
        negative,
        literal: false,
    }
}

// --- media.test.ts -------------------------------------------------------------------

/// "parses include and exclude filter tags".
#[test]
fn parses_include_and_exclude_filter_tags() {
    assert_eq!(
        parse_filter_tags("cat  -dog car -bird"),
        (strings(&["cat", "car"]), strings(&["dog", "bird"]))
    );
    assert_eq!(parse_filter_tags("-"), (strings(&["-"]), Vec::new()));
}

/// "parses tags metatags as exact tag counts".
#[test]
fn parses_tags_metatags_as_exact_tag_counts() {
    assert_eq!(
        parse_search_filter("tags"),
        meta_filter(MetaFilter::HasNoTags { tag_count: 0 })
    );
    assert_eq!(
        parse_search_filter("tags:2"),
        meta_filter(MetaFilter::HasNoTags { tag_count: 2 })
    );
}

/// "parses gN metatags as exact group filters".
#[test]
fn parses_group_name_metatags_as_exact_group_filters() {
    assert_eq!(
        parse_search_filter("gN:Trip 2026"),
        meta_filter(MetaFilter::GroupName {
            group_name: "Trip 2026".into()
        })
    );
}

/// "rejects invalid metatag payloads and mixing metatags with normal tags".
#[test]
fn rejects_invalid_metatag_payloads_and_mixing_metatags_with_tags() {
    use SearchFilterValidationError::*;
    for (input, error) in [
        ("tags:abc", HasNoTagsInvalidCount),
        ("tags:-1", HasNoTagsInvalidCount),
        ("tags:", HasNoTagsInvalidCount),
        ("gN:", GroupNameMissingValue),
        ("cat tags", MetaTagRequiresSolo),
        ("dog gN:trip-2026", MetaTagRequiresSolo),
        ("cat,dog", TagInvalidCharacters),
    ] {
        assert_eq!(
            parse_search_filter(input).validation_error,
            Some(error),
            "{input}"
        );
    }
}

/// "accepts tags:N up to the largest safe integer, including leading zeroes".
#[test]
fn accepts_tag_counts_up_to_the_largest_safe_integer_with_leading_zeroes() {
    assert_eq!(
        parse_search_filter("tags:0003").meta_filter,
        Some(MetaFilter::HasNoTags { tag_count: 3 })
    );
    assert_eq!(
        parse_search_filter(&format!("tags:{MAX_TAG_COUNT}")).meta_filter,
        Some(MetaFilter::HasNoTags {
            tag_count: MAX_TAG_COUNT
        })
    );
    let padded = parse_search_filter(&format!("tags:000000000000000000000{MAX_TAG_COUNT}"));
    assert_eq!(
        padded.meta_filter,
        Some(MetaFilter::HasNoTags {
            tag_count: MAX_TAG_COUNT
        })
    );
    assert_eq!(padded.validation_error, None);
}

/// "rejects tags:N beyond the safe integer range with the count validation message".
#[test]
fn rejects_tag_counts_beyond_the_safe_integer_range() {
    let nines = "9".repeat(400);
    for count in [
        "9007199254740992",
        "9223372036854775808",
        "18446744073709551616",
        "99999999999999999999",
        nines.as_str(),
    ] {
        assert_eq!(
            parse_search_filter(&format!("tags:{count}")),
            ParsedSearchFilter {
                validation_error: Some(SearchFilterValidationError::HasNoTagsInvalidCount),
                ..tags_filter(&[], &[])
            },
            "{count}"
        );
    }
}

// --- literalTagSearch.test.ts --------------------------------------------------------

/// \`JSON.stringify\` of a string without control characters.
fn json_string(value: &str) -> String {
    let mut out = String::from('"');
    for c in value.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// "round trips stored tag %s through Tag list, typed search and suggestion
/// replacement". The tag list of one tag writes \`serializeTagToken\` of it.
#[test]
fn stored_tags_round_trip_through_typed_search_and_suggestion_replacement() {
    let names = [
        "tags",
        "tags:3",
        "gn:trip",
        "-holiday",
        "-",
        "say\"hello",
        "path\\photo",
        "\"",
        "\\",
        "normal",
    ];
    for name in names {
        for exclude in [false, true] {
            let query = serialize_tag_token(name, exclude);
            let expected = if exclude {
                tags_filter(&[], &[name])
            } else {
                tags_filter(&[name], &[])
            };
            assert_eq!(parse_search_filter(&query), expected, "{name} {exclude}");
            let quoted = format!("{}{}", if exclude { "-" } else { "" }, json_string(name));
            assert_eq!(parse_search_filter(&quoted), expected, "{quoted}");

            let draft = format!("cat {}\"ta bird", if exclude { "-" } else { "" });
            let token = read_active_token(&draft, 7).expect("a token under the caret");
            let (next, caret) = apply_suggestion_to_value(&draft, &token, name);
            assert_eq!(
                next,
                format!("cat {} bird", serialize_tag_token(name, exclude))
            );
            assert_eq!(caret, next.len() - 5, "{name}");
            assert_eq!(collect_used_tags(&query, &[]), set(&[name]), "{name}");
            let filters = |filter_input: String| SearchFilters {
                filter_input,
                ..SearchFilters::default()
            };
            assert_eq!(
                build_filter_descriptor(&filters(query.clone())),
                build_filter_descriptor(&filters(quoted.clone())),
                "{name}"
            );
        }
    }
}

/// "keeps quoted operator names eligible for autocomplete without confusing literal
/// minus names".
#[test]
fn quoted_operator_names_stay_eligible_for_autocomplete() {
    let token = read_active_token("-\"tags\"", 5).expect("a token");
    assert_eq!(
        (token.query.as_str(), token.negative, token.literal),
        ("tags", true, true)
    );
    let index = TagIndex::new(Arc::new(strings(&["tags", "tags:3"])));
    let found = build_tag_suggestions(
        Some(&token),
        &BTreeSet::new(),
        &HashSet::new(),
        &index,
        &Default::default(),
    )
    .expect("not cancelled");
    let values: Vec<&str> = found.iter().map(|item| item.value.as_str()).collect();
    assert_eq!(values, ["tags", "tags:3"]);
    assert_eq!(
        collect_used_tags("\"-holiday\"", &strings(&["-trip"])),
        set(&["-holiday", "-trip"])
    );
}

/// "preserves unquoted operators and rejects unfinished or invalid quoted tags".
#[test]
fn unquoted_operators_stay_and_unfinished_quotes_are_rejected() {
    assert_eq!(
        parse_search_filter("tags:3").meta_filter,
        Some(MetaFilter::HasNoTags { tag_count: 3 })
    );
    assert_eq!(
        parse_search_filter("gn:Trip 2026").meta_filter,
        Some(MetaFilter::GroupName {
            group_name: "Trip 2026".into()
        })
    );
    assert_eq!(parse_search_filter("-tags").exclude, ["tags"]);
    assert_eq!(
        parse_search_filter("\"tags\" -\"gn:trip\"").validation_error,
        None
    );
    for query in [
        "\"tags",
        "\"ta gs\"",
        "\"bad\\q\"",
        "\"tags\"suffix",
        "\"\"",
    ] {
        assert_eq!(
            parse_search_filter(query).validation_error,
            Some(SearchFilterValidationError::TagInvalidCharacters),
            "{query}"
        );
    }
}

// --- tokenService.test.ts, inputService.test.ts --------------------------------------

/// "normalizes tag tokens".
#[test]
fn normalizes_tag_tokens() {
    assert_eq!(normalize_tag_token("  -CaT  "), "cat");
}

/// "reads active token around caret and keeps negative flag".
#[test]
fn reads_the_active_token_around_the_caret_with_its_sign() {
    assert_eq!(
        read_active_token("dog -ca bird", 6),
        Some(token(4, 7, "ca", true))
    );
}

/// "returns null for empty or invalid token".
#[test]
fn no_active_token_for_blank_or_lone_minus() {
    assert_eq!(read_active_token("   ", 1), None);
    assert_eq!(read_active_token("-", 1), None);
}

/// "returns no active token while the anchored gN: rule consumes the input".
#[test]
fn no_active_token_while_the_group_rule_consumes_the_input() {
    for value in [
        "gN:Trip 2026",
        "  gn:trip 2026",
        "gN:\"Trip 2026",
        "GN:",
        "gN:Trip 2026 ",
    ] {
        for caret in 0..=value.len() {
            assert_eq!(read_active_token(value, caret), None, "{value} at {caret}");
        }
    }
}

/// "keeps active tokens when gN: is not at the start of the input".
#[test]
fn active_tokens_stay_when_the_group_operator_is_not_first() {
    assert_eq!(
        read_active_token("cat gN:Trip 2026", 16),
        Some(token(12, 16, "2026", false))
    );
    assert_eq!(
        read_active_token("\"gn:trip\" 2026", 14),
        Some(token(10, 14, "2026", false))
    );
    assert_eq!(
        read_active_token("-gn:trip 2026", 13),
        Some(token(9, 13, "2026", false))
    );
}

/// "collects used tags from value and excluded tags".
#[test]
fn collects_used_tags_from_the_value_and_the_exclusions() {
    assert_eq!(
        collect_used_tags("cat -dog", &strings(&["  BIRD ", ""])),
        set(&["cat", "dog", "bird"])
    );
}

/// inputService "replaces active token and keeps negative prefix".
#[test]
fn a_suggestion_replaces_the_token_and_keeps_its_minus() {
    assert_eq!(
        apply_suggestion_to_value("dog -ca bird", &token(4, 7, "ca", true), "cat"),
        ("dog -cat bird".to_owned(), 8)
    );
}

/// inputService "replaces active token without negative prefix".
#[test]
fn a_suggestion_replaces_a_positive_token() {
    assert_eq!(
        apply_suggestion_to_value("ca dog", &token(0, 2, "ca", false), "car"),
        ("car dog".to_owned(), 3)
    );
}

/// Offsets are bytes of the UTF-8 text; the Solid code counts UTF-16 units. A token
/// after a multi-byte tag is still found and replaced in place.
#[test]
fn offsets_are_bytes_after_multibyte_text() {
    let value = "żółw -ca";
    let token = read_active_token(value, value.len()).expect("a token");
    assert_eq!(token, self::token(8, 11, "ca", true));
    assert_eq!(
        apply_suggestion_to_value(value, &token, "cat"),
        ("żółw -cat".to_owned(), 12)
    );
}

// --- filterService.test.ts -----------------------------------------------------------

/// "compares search filters by value".
#[test]
fn search_filters_compare_by_value() {
    let filters = |input: &str| SearchFilters {
        filter_input: input.into(),
        media_kind: Some(MediaKind::Video),
        favorites_only: true,
    };
    assert_eq!(filters("cat"), filters("cat"));
    assert_ne!(filters("cat"), filters("dog"));
}

/// "defines empty defaults".
#[test]
fn search_filters_default_to_empty() {
    assert_eq!(
        SearchFilters::default(),
        SearchFilters {
            filter_input: String::new(),
            media_kind: None,
            favorites_only: false,
        }
    );
}

/// "builds stable meta filter keys".
#[test]
fn meta_filter_keys_are_stable() {
    assert_eq!(meta_filter_key(None), "");
    assert_eq!(
        meta_filter_key(Some(&MetaFilter::HasNoTags { tag_count: 2 })),
        "hasNoTags:2"
    );
    assert_eq!(
        meta_filter_key(Some(&MetaFilter::GroupName {
            group_name: "  Trip-2026  ".into()
        })),
        "groupName:trip-2026"
    );
}

// --- createSearch.test.ts ------------------------------------------------------------

/// "separates draft from applied state and explicitly resubmits an unchanged
/// normalized filter".
#[test]
fn the_draft_is_separate_and_an_unchanged_filter_is_resubmitted() {
    let mut search = Search::new();
    assert_eq!(
        search.handle(SearchCommand::Change("Cat dog cat".into())),
        None
    );
    assert!(search.applied().include.is_empty());
    let first = search
        .handle(SearchCommand::Submit)
        .expect("a valid submit applies");
    assert_eq!(search.applied().include, ["cat", "dog"]);
    assert_eq!(first.include, ["cat", "dog"]);
    let version = search.applied_version();
    let descriptor = search.applied().clone();
    let again = search
        .handle(SearchCommand::ApplyInput("DOG CAT".into()))
        .expect("an unchanged filter is applied again");
    assert_eq!(again, first);
    assert_eq!(search.applied(), &descriptor);
    assert_eq!(
        search.applied_version(),
        version,
        "the applied state is kept"
    );
}

/// "uses the existing parser for %s".
#[test]
fn submits_use_the_search_grammar() {
    let cases = [
        (
            "\"tags\" -\"-holiday\"",
            (strings(&["tags"]), strings(&["-holiday"]), None),
        ),
        (
            "tags:2",
            (
                Vec::new(),
                Vec::new(),
                Some(MetaFilter::HasNoTags { tag_count: 2 }),
            ),
        ),
        (
            "gN:Trip 2026",
            (
                Vec::new(),
                Vec::new(),
                Some(MetaFilter::GroupName {
                    group_name: "trip 2026".into(),
                }),
            ),
        ),
    ];
    for (input, expected) in cases {
        let applied = Search::new()
            .handle(SearchCommand::ApplyInput(input.into()))
            .expect("valid");
        assert_eq!(
            (applied.include, applied.exclude, applied.meta_filter),
            expected,
            "{input}"
        );
    }
}

/// "composes input, kind and favorites in the same batch".
#[test]
fn input_kind_and_favourites_compose() {
    let mut search = Search::new();
    search.handle(SearchCommand::Change("tags:3".into()));
    search.handle(SearchCommand::ApplyKind(Some(MediaKind::Video)));
    let applied = search
        .handle(SearchCommand::ApplyFavorites(true))
        .expect("valid");
    assert_eq!(
        applied.meta_filter,
        Some(MetaFilter::HasNoTags { tag_count: 3 })
    );
    assert_eq!(applied.media_kind, Some(MediaKind::Video));
    assert!(applied.favorites_only);
    assert_eq!(search.media_kind(), Some(MediaKind::Video));
    assert!(search.favorites_only());
}

/// "rejects %s, retains applied values and resets through clear".
#[test]
fn invalid_input_keeps_the_applied_values_until_clear() {
    for input in [
        "gN:",
        "tags:-1",
        "tags:99999999999999999999",
        "\"unfinished",
    ] {
        let mut search = Search::new();
        let valid = search
            .handle(SearchCommand::ApplyInput("cat".into()))
            .expect("valid");
        search.handle(SearchCommand::Change(input.into()));
        assert_eq!(
            search.handle(SearchCommand::ApplyFavorites(true)),
            None,
            "{input} applies nothing"
        );
        assert!(search.validation_error().is_some(), "{input}");
        if input.starts_with("tags:") {
            assert_eq!(
                search.validation_error(),
                Some(SearchFilterValidationError::HasNoTagsInvalidCount)
            );
        }
        assert_eq!(valid.include, ["cat"]);
        assert_eq!(search.applied().include, ["cat"]);
        assert!(!search.applied().favorites_only);
        let cleared = search.handle(SearchCommand::Clear).expect("clear applies");
        assert_eq!(cleared, Default::default());
        assert_eq!(search.filter_input(), "");
        assert_eq!(search.media_kind(), None);
        assert!(!search.favorites_only());
        assert_eq!(search.validation_error(), None);
    }
}

// --- localization.test.tsx (the search part) -----------------------------------------

/// "renders tag selection and parser errors in %s": each refused input names its
/// error, and every language has its own text for it (no English fallback).
#[test]
fn parser_errors_have_a_text_in_every_language() {
    use SearchFilterValidationError::*;
    let cases = [
        ("tags cat", MetaTagRequiresSolo),
        ("cat,dog", TagInvalidCharacters),
        ("gN:", GroupNameMissingValue),
        ("tags:-1", HasNoTagsInvalidCount),
    ];
    for (input, error) in cases {
        let mut search = Search::new();
        search.handle(SearchCommand::Change(input.into()));
        assert_eq!(
            search.validation_error(),
            None,
            "no message before a submit"
        );
        assert_eq!(search.handle(SearchCommand::Submit), None);
        assert_eq!(search.validation_error(), Some(error), "{input}");
        assert_eq!(error.i18n_key(), format!("search.errors.{}", error.name()));
    }
    let english = Locales::embedded().catalog(Language::En);
    for language in Language::ALL {
        let catalog = Locales::embedded().catalog(language);
        for error in SearchFilterValidationError::ALL {
            let own = catalog
                .text(error.i18n_key())
                .unwrap_or_else(|| panic!("{language:?} has no {}", error.i18n_key()));
            assert!(!own.trim().is_empty());
            assert_eq!(I18n::new(language).t(error.i18n_key()), own);
            if language != Language::En {
                assert_ne!(
                    Some(own),
                    english.text(error.i18n_key()),
                    "{language:?} {} is English",
                    error.name()
                );
            }
        }
    }
}

// --- createSuggestions.test.ts, SearchTagator.test.tsx: the session -----------------

fn vocabulary(tags: &[&str]) -> Arc<Vec<String>> {
    Arc::new(strings(tags))
}

fn searches(effects: &[ComboboxEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| {
            matches!(
                effect,
                ComboboxEffect::Suggest(SuggestionInput::Search { .. })
            )
        })
        .count()
}

fn request_for(value: &str) -> SuggestionRequest {
    SuggestionRequest {
        context: SuggestionContext {
            token: read_active_token(value, value.len()),
            used: collect_used_tags(value, &[]),
            excluded: Vec::new(),
        },
        retry: 0,
    }
}

fn settled(
    editor: EditorId,
    value: &str,
    vocabulary: &Arc<Vec<String>>,
    items: &[&str],
) -> SuggestionEvent {
    SuggestionEvent::Settled {
        editor,
        request: request_for(value),
        vocabulary: Arc::clone(vocabulary),
        result: Ok(items
            .iter()
            .map(|value| media_tagger_client::search::TagSuggestion {
                value: (*value).to_owned(),
                ranges: Vec::new(),
            })
            .collect::<Vec<_>>()
            .into()),
    }
}

/// createSuggestions "rejects obsolete results, stops blurred reads and cancels on
/// disposal": the same draft asks once, a late answer to an older draft is dropped
/// after the newer one arrived, a blurred field shows nothing and asks nothing, and
/// disposal cancels.
#[test]
fn a_late_answer_to_an_older_draft_is_dropped_and_disposal_cancels() {
    let tags = vocabulary(&["cat", "dog"]);
    let editor = EditorId::next();
    let mut combobox = Combobox::new(editor, ComboboxConfig::SEARCH, Arc::clone(&tags));
    assert_eq!(searches(&combobox.focus_in(0)), 0);
    assert_eq!(searches(&combobox.input("ca", 2)), 1);
    assert_eq!(
        searches(&combobox.input("ca", 2)),
        0,
        "the same draft asks once"
    );
    assert_eq!(searches(&combobox.input("do", 2)), 1);

    combobox.receive(settled(editor, "do", &tags, &["dog"]));
    combobox.receive(settled(editor, "ca", &tags, &["cat"]));
    let values: Vec<&str> = combobox
        .items()
        .iter()
        .map(|item| item.value.as_str())
        .collect();
    assert_eq!(values, ["dog"]);

    let effects = combobox.focus_out();
    assert_eq!(searches(&effects), 0);
    assert!(combobox.items().is_empty());
    assert!(!combobox.popup());
    let effects = combobox.dispose();
    assert!(matches!(
        &effects[..],
        [ComboboxEffect::Suggest(SuggestionInput::Cancel { editor: cancelled })] if *cancelled == editor
    ));
}

/// An answer for another field changes nothing (each editor has its own requests).
#[test]
fn an_answer_for_another_field_is_ignored() {
    let tags = vocabulary(&["cat"]);
    let editor = EditorId::next();
    let mut combobox = Combobox::new(editor, ComboboxConfig::SEARCH, Arc::clone(&tags));
    combobox.focus_in(0);
    combobox.input("ca", 2);
    combobox.receive(settled(EditorId::next(), "ca", &tags, &["cat"]));
    assert!(combobox.items().is_empty());
    combobox.receive(settled(editor, "ca", &tags, &["cat"]));
    assert_eq!(combobox.items().len(), 1);
}

/// SearchTagator "submits a multi-word gN: group name on Enter without rewriting its
/// words": no request for either spelling, also with the caret inside an earlier
/// word, and Enter submits the text as typed.
#[test]
fn a_group_name_asks_for_nothing_and_enter_submits_it_unchanged() {
    let mut combobox = Combobox::new(
        EditorId::next(),
        ComboboxConfig::SEARCH,
        vocabulary(&["trip-2026", "cat"]),
    );
    combobox.focus_in(0);
    for typed in ["gN:Trip 2026", "gN:\"Trip 2026\""] {
        assert_eq!(searches(&combobox.input(typed, typed.len())), 0, "{typed}");
        assert!(!combobox.popup());
        let (handled, effects) = combobox.key(ComboboxKey::Enter);
        assert!(handled);
        assert!(
            matches!(&effects[..], [ComboboxEffect::Submit]),
            "{effects:?}"
        );
        assert_eq!(combobox.value(), typed);
    }
    assert_eq!(searches(&combobox.caret_moved(5)), 0);
    assert_eq!(searches(&combobox.click(5)), 0);
    assert!(!combobox.popup());
}

/// SearchTagator "keeps suggestion list closed when input is empty": a focused empty
/// field asks nothing; typing asks; clearing closes the session and cancels.
#[test]
fn an_empty_field_keeps_the_session_closed() {
    let mut combobox = Combobox::new(
        EditorId::next(),
        ComboboxConfig::SEARCH,
        vocabulary(&["cat", "car"]),
    );
    assert_eq!(searches(&combobox.focus_in(0)), 0);
    assert_eq!(searches(&combobox.click(0)), 0);
    assert!(!combobox.session_open());
    assert_eq!(searches(&combobox.input("ca", 2)), 1);
    assert!(combobox.session_open());
    let effects = combobox.input("", 0);
    assert!(!combobox.session_open());
    assert!(matches!(
        &effects[..],
        [ComboboxEffect::Suggest(SuggestionInput::Cancel { .. })]
    ));
}

// --- The runtime on the scripted backend ---------------------------------------------

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
    requests: Arc<Mutex<Vec<AssetQueryRequest>>>,
}

impl Fixture {
    fn new() -> Self {
        let backend = Arc::new(FakeBackend::new());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        backend.on_start_asset_query(move |request| {
            seen.lock().unwrap().push(request.clone());
            Ok(StartAssetQueryResult::Ready {
                session_id: 1,
                revision: 1,
                total: 0,
                offset: 0,
                items: Vec::new(),
            })
        });
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(DARK_EN),
            RuntimeRunner::new(backend.clone(), Arc::new(MemoryPrefsStore::default())),
            executor.clone() as Arc<dyn Executor>,
        );
        Self {
            backend,
            executor,
            driver,
            requests,
        }
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn search(&mut self, command: SearchCommand) {
        self.send(RuntimeInput::Search(command));
        self.settle();
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn queries(&self) -> Vec<AssetQueryRequest> {
        self.requests.lock().unwrap().clone()
    }
}

/// createSearch "separates draft from applied state and explicitly resubmits an
/// unchanged normalized filter", through the runtime: the resubmit starts the query
/// again (a refresh).
#[test]
fn an_unchanged_resubmit_starts_the_query_again() {
    let mut fx = Fixture::new();
    fx.search(SearchCommand::Change("Cat dog cat".into()));
    assert!(fx.queries().is_empty(), "typing applies nothing");
    fx.search(SearchCommand::Submit);
    fx.search(SearchCommand::ApplyInput("DOG CAT".into()));
    let queries = fx.queries();
    assert_eq!(queries.len(), 2);
    for query in &queries {
        assert_eq!(query.tags_and, ["cat", "dog"]);
    }
    assert_eq!(fx.driver.model().search().applied_version(), 1);
}

/// createSearch "composes input, kind and favorites" and "rejects …": every valid
/// command starts one query with everything of the draft; a refused one starts none.
#[test]
fn each_valid_command_starts_one_query_with_the_whole_draft() {
    let mut fx = Fixture::new();
    fx.search(SearchCommand::Change("tags:3".into()));
    fx.search(SearchCommand::ApplyKind(Some(MediaKind::Video)));
    fx.search(SearchCommand::ApplyFavorites(true));
    let queries = fx.queries();
    assert_eq!(queries.len(), 2);
    let last = queries.last().unwrap();
    assert_eq!(
        last.meta_filter,
        Some(MetaFilter::HasNoTags { tag_count: 3 })
    );
    assert_eq!(last.kind, Some(MediaKind::Video));
    assert!(last.favorites_only);

    fx.search(SearchCommand::Change("tags:-1".into()));
    fx.search(SearchCommand::ApplyFavorites(false));
    assert_eq!(fx.queries().len(), 2, "a refused submit starts nothing");
    assert_eq!(
        fx.driver.model().query().filters().media_kind,
        Some(MediaKind::Video)
    );
    fx.search(SearchCommand::Clear);
    let cleared = fx.queries().pop().unwrap();
    assert_eq!(
        (cleared.kind, cleared.favorites_only, cleared.meta_filter),
        (None, false, None)
    );
    assert!(cleared.tags_and.is_empty() && cleared.tags_not.is_empty());
}

/// SuggestionClient "does not send worker vocabulary messages after repeated
/// existing-tag saves": the runtime's known tags stay the same vocabulary across a
/// refresh that finds them unchanged, so the suggestion index is built once; a changed
/// vocabulary builds it again.
#[test]
fn an_unchanged_vocabulary_refresh_does_not_rebuild_the_suggestion_index() {
    let mut fx = Fixture::new();
    let listed = Arc::new(Mutex::new(strings(&["cat", "dog"])));
    let source = Arc::clone(&listed);
    fx.backend.on_list_tags(move |_, _, _| {
        let items = source.lock().unwrap().clone();
        Ok(TagListPage {
            total: items.len() as i64,
            items,
        })
    });
    fx.send(RuntimeInput::library(LibraryInput::Vocabulary(
        VocabularyInput::Refresh { ticket: None },
    )));
    fx.settle();
    let editor = EditorId::next();
    let search = |fx: &mut Fixture| {
        let vocabulary = Arc::clone(fx.driver.model().library().known_tags());
        fx.send(RuntimeInput::Suggestions(SuggestionInput::Search {
            editor,
            vocabulary,
            request: request_for("ca"),
        }));
        fx.settle();
        fx.driver.model_mut().take_suggestion_events_for(editor)
    };
    assert_eq!(search(&mut fx).len(), 1);
    for vocabulary_changed in [false, true, false, true] {
        fx.send(RuntimeInput::library(LibraryInput::Vocabulary(
            VocabularyInput::RefreshAfterMutation {
                vocabulary_changed,
                ticket: None,
            },
        )));
        fx.settle();
        assert_eq!(search(&mut fx).len(), 1);
    }
    assert_eq!(fx.driver.model().suggestions().indexed_versions(), 1);

    *listed.lock().unwrap() = strings(&["cat", "new"]);
    fx.send(RuntimeInput::library(LibraryInput::Vocabulary(
        VocabularyInput::RefreshAfterMutation {
            vocabulary_changed: true,
            ticket: None,
        },
    )));
    fx.settle();
    assert_eq!(search(&mut fx).len(), 1);
    assert_eq!(fx.driver.model().suggestions().indexed_versions(), 2);
}

/// Acceptance criterion "typing does not block the UI with ≥ 100,000 tags": the
/// runtime's turn for a suggestion request only queues the search; the matching runs
/// as an executor task and its answer arrives later.
#[test]
fn a_suggestion_request_for_a_large_vocabulary_only_queues_work() {
    let mut fx = Fixture::new();
    let tags: Vec<String> = (0..100_000).map(|i| format!("tag-{i:06}")).collect();
    let editor = EditorId::next();
    let effects =
        fx.driver
            .model_mut()
            .handle(RuntimeInput::Suggestions(SuggestionInput::Search {
                editor,
                vocabulary: Arc::new(tags),
                request: request_for("tag-0999"),
            }));
    assert!(matches!(
        &effects[..],
        [media_tagger_client::runtime::RuntimeEffect::Suggest(
            SuggestionEffect::Search { .. }
        )]
    ));
    assert!(fx
        .driver
        .model_mut()
        .take_suggestion_events_for(editor)
        .is_empty());
}

// --- The real core: the search-applied parity state ----------------------------------

struct CoreFixture {
    core: TempCore,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl CoreFixture {
    fn new(images: usize) -> Self {
        let core = TempCore::start();
        for index in 0..images {
            core.write_image(&format!("image-{index:02}.png"));
        }
        let summary = core
            .backend()
            .scan_folder(TempCore::text(&core.media_dir()))
            .expect("scan the media directory");
        assert_eq!(summary.indexed, images);
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(DARK_EN),
            RuntimeRunner::new(core.shared(), Arc::new(MemoryPrefsStore::default())),
            executor.clone() as Arc<dyn Executor>,
        );
        let mut fixture = Self {
            core,
            executor,
            driver,
        };
        fixture.send(RuntimeInput::RefreshLibrary);
        assert_eq!(fixture.total(), images);
        fixture
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn search(&mut self, command: SearchCommand) -> usize {
        self.send(RuntimeInput::Search(command));
        self.total()
    }

    fn total(&self) -> usize {
        self.driver.model().query().total()
    }

    fn ids(&self, count: usize) -> Vec<i64> {
        let session = self.driver.model().query().session_id().expect("a session");
        match self
            .core
            .backend()
            .get_asset_query_id_range(session, 0, count - 1)
            .expect("read the session")
        {
            AssetIdRange::Ready(ids) => ids,
            AssetIdRange::Stale => panic!("the session is current"),
        }
    }
}

/// The \`search-applied\` parity state on SQLite: a tag exclusion with a media kind
/// counts the matching assets, and each operator of the grammar reaches the core with
/// the meaning the Solid frontend gives it.
#[test]
fn applied_filters_count_the_matching_assets_of_the_real_core() {
    let mut fx = CoreFixture::new(5);
    let ids = fx.ids(5);
    let backend = fx.core.backend();
    backend
        .set_asset_tags(ids[0], strings(&["cat", "pet"]))
        .expect("tag");
    backend
        .set_asset_tags(ids[1], strings(&["cat"]))
        .expect("tag");
    backend
        .set_asset_tags(ids[2], strings(&["-holiday"]))
        .expect("tag");
    backend.set_asset_favorite(ids[3], true).expect("favourite");
    backend
        .set_asset_media_group(ids[4], Some("Trip 2026".into()), Some(1.0))
        .expect("group");

    fx.search(SearchCommand::Change("-cat".into()));
    assert_eq!(fx.total(), 5, "typing applies nothing");
    assert_eq!(
        fx.search(SearchCommand::ApplyKind(Some(MediaKind::Image))),
        3
    );
    assert_eq!(
        fx.search(SearchCommand::ApplyKind(Some(MediaKind::Video))),
        0
    );
    assert_eq!(fx.search(SearchCommand::ApplyKind(None)), 3);
    assert_eq!(fx.search(SearchCommand::ApplyInput("CAT".into())), 2);
    assert_eq!(fx.search(SearchCommand::ApplyInput("cat pet".into())), 1);
    assert_eq!(fx.search(SearchCommand::ApplyInput("cat -pet".into())), 1);
    assert_eq!(
        fx.search(SearchCommand::ApplyInput("\"-holiday\"".into())),
        1
    );
    assert_eq!(fx.search(SearchCommand::ApplyInput("tags".into())), 2);
    assert_eq!(fx.search(SearchCommand::ApplyInput("tags:2".into())), 1);
    assert_eq!(
        fx.search(SearchCommand::ApplyInput("gN:TRIP 2026".into())),
        1
    );
    assert_eq!(fx.search(SearchCommand::ApplyInput("gN:Trip".into())), 0);
    assert_eq!(fx.search(SearchCommand::ApplyInput(String::new())), 5);
    assert_eq!(fx.search(SearchCommand::ApplyFavorites(true)), 1);

    // A refused draft keeps the applied filters and their result.
    assert_eq!(fx.search(SearchCommand::ApplyInput("cat tags".into())), 1);
    assert_eq!(
        fx.driver.model().search().validation_error(),
        Some(SearchFilterValidationError::MetaTagRequiresSolo)
    );
    assert_eq!(fx.search(SearchCommand::Clear), 5);
}
