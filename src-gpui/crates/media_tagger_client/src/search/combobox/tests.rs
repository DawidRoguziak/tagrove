//! The cases of `SearchTagator.test.tsx` and `createSuggestions.test.ts` that are
//! about the state machine, with the real client and matcher run in place of the
//! worker.

use std::{collections::HashSet, sync::Arc};

use super::{Combobox, ComboboxConfig, ComboboxEffect, ComboboxKey};
use crate::{
    model::Model,
    search::{
        matcher::{build_tag_suggestions, TagIndex},
        suggestions::{
            EditorId, SuggestionClient, SuggestionEffect, SuggestionFailure, SuggestionInput,
        },
    },
};

struct Harness {
    combobox: Combobox,
    client: SuggestionClient,
    queued: Vec<SuggestionEffect>,
    searches: usize,
    fail_next: bool,
    picks: Vec<String>,
    submits: usize,
    focus_requests: usize,
}

fn vocabulary(tags: &[&str]) -> Arc<Vec<String>> {
    Arc::new(tags.iter().map(|tag| (*tag).to_owned()).collect())
}

impl Harness {
    fn new(tags: &[&str], config: ComboboxConfig) -> Self {
        Self {
            combobox: Combobox::new(EditorId::next(), config, vocabulary(tags)),
            client: SuggestionClient::new(),
            queued: Vec::new(),
            searches: 0,
            fail_next: false,
            picks: Vec::new(),
            submits: 0,
            focus_requests: 0,
        }
    }

    fn apply(&mut self, effects: Vec<ComboboxEffect>) {
        for effect in effects {
            match effect {
                ComboboxEffect::Suggest(input) => {
                    if matches!(input, SuggestionInput::Search { .. }) {
                        self.searches += 1;
                    }
                    let effects = self.client.handle(input);
                    self.queued.extend(effects);
                }
                ComboboxEffect::ReplaceToken { .. } => {}
                ComboboxEffect::Pick(tag) => self.picks.push(tag),
                ComboboxEffect::Submit => self.submits += 1,
                ComboboxEffect::FocusInput => self.focus_requests += 1,
            }
        }
    }

    /// Runs every search the client started, as the executor would, and hands the
    /// answers back.
    fn settle(&mut self) {
        loop {
            for event in self.client.take_events() {
                let effects = self.combobox.receive(event);
                self.apply(effects);
            }
            let Some(SuggestionEffect::Search {
                job,
                vocabulary,
                context,
                cancel,
            }) = self.queued.pop()
            else {
                break;
            };
            let result = if std::mem::take(&mut self.fail_next) {
                Err(SuggestionFailure)
            } else {
                let excluded: HashSet<String> = context.excluded.iter().cloned().collect();
                Ok(build_tag_suggestions(
                    context.token.as_ref(),
                    &context.used,
                    &excluded,
                    &TagIndex::new(vocabulary),
                    &cancel,
                ))
            };
            let effects = self
                .client
                .handle(SuggestionInput::Answered { job, result });
            self.queued.extend(effects);
        }
    }

    fn type_text(&mut self, value: &str) {
        let effects = self.combobox.input(value, value.len());
        self.apply(effects);
    }

    fn focus(&mut self) {
        let caret = self.combobox.value().len();
        let effects = self.combobox.focus_in(caret);
        self.apply(effects);
    }

    fn key(&mut self, key: ComboboxKey) -> bool {
        let (handled, effects) = self.combobox.key(key);
        self.apply(effects);
        handled
    }

    fn values(&self) -> Vec<String> {
        self.combobox
            .items()
            .iter()
            .map(|item| item.value.clone())
            .collect()
    }
}

#[test]
fn the_list_opens_with_results_and_closes_on_escape_blur_and_an_empty_value() {
    let mut h = Harness::new(&["cat", "car"], ComboboxConfig::SEARCH);
    h.focus();
    assert!(!h.combobox.session_open(), "an empty field opens nothing");
    h.type_text("ca");
    assert!(h.combobox.session_open());
    assert!(!h.combobox.popup(), "pending results are not shown");
    h.settle();
    assert_eq!(h.values(), ["cat", "car"]);
    assert!(h.combobox.popup());
    assert_eq!(h.combobox.active(), Some(0));

    h.type_text("cazzzz");
    h.settle();
    assert!(!h.combobox.popup());
    h.type_text("ca");
    h.settle();
    assert!(h.combobox.popup());
    assert!(h.key(ComboboxKey::Escape));
    assert!(!h.combobox.session_open());
    assert!(!h.key(ComboboxKey::Escape), "a second Escape goes on");

    h.type_text("ca");
    h.settle();
    let effects = h.combobox.focus_out();
    h.apply(effects);
    assert!(h.combobox.items().is_empty());
    h.focus();
    h.type_text("");
    assert!(!h.combobox.session_open());
}

#[test]
fn arrows_wrap_and_enter_replaces_the_token_keeping_its_sign() {
    let mut h = Harness::new(&["cat"], ComboboxConfig::SEARCH);
    h.focus();
    h.type_text("dog -ca");
    h.settle();
    assert!(h.key(ComboboxKey::Down));
    assert_eq!(
        h.combobox.active(),
        Some(0),
        "one item: down wraps to itself"
    );
    let (handled, effects) = h.combobox.key(ComboboxKey::Enter);
    assert!(handled);
    match &effects[..] {
        // The closed session also cancels its request.
        [ComboboxEffect::ReplaceToken {
            range,
            text,
            value,
            caret,
        }, ComboboxEffect::Suggest(SuggestionInput::Cancel { .. })] => {
            assert_eq!((range.clone(), text.as_str()), (4..7, "-cat"));
            assert_eq!((value.as_str(), *caret), ("dog -cat", 8));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(h.combobox.value(), "dog -cat");
    assert!(!h.combobox.session_open());
    assert_eq!(h.submits, 0);
}

#[test]
fn enter_without_items_submits() {
    let mut h = Harness::new(&[], ComboboxConfig::SEARCH);
    h.focus();
    h.type_text("bird");
    h.settle();
    assert!(h.key(ComboboxKey::Enter));
    assert_eq!(h.submits, 1);
}

#[test]
fn a_pick_for_the_owner_can_keep_the_list_open_on_the_same_item() {
    let config = ComboboxConfig {
        auto_select_first: true,
        pick_to_owner: true,
        keep_open_on_pick: true,
    };
    let mut h = Harness::new(&["cat", "car", "castle"], config);
    h.focus();
    h.type_text("ca");
    h.settle();
    assert!(h.key(ComboboxKey::Down));
    assert_eq!(h.combobox.active(), Some(1));
    assert!(h.key(ComboboxKey::Enter));
    assert_eq!(h.picks, ["car"]);
    assert_eq!(h.focus_requests, 1);
    assert!(h.combobox.popup());
    assert_eq!(h.combobox.active(), Some(1));
}

#[test]
fn without_auto_selection_enter_submits_until_an_arrow_key() {
    let config = ComboboxConfig {
        auto_select_first: false,
        pick_to_owner: true,
        keep_open_on_pick: false,
    };
    let mut h = Harness::new(&["cat", "car"], config);
    h.focus();
    h.type_text("ca");
    h.settle();
    assert_eq!(h.combobox.active(), None);
    assert!(h.key(ComboboxKey::Enter));
    assert_eq!((h.submits, h.picks.len()), (1, 0));
    assert!(h.key(ComboboxKey::Down));
    assert!(h.key(ComboboxKey::Enter));
    assert_eq!(h.picks, ["cat"]);
}

#[test]
fn operators_and_group_names_ask_for_nothing() {
    let mut h = Harness::new(
        &["tags", "tagstone", "trip-2026", "cat"],
        ComboboxConfig::SEARCH,
    );
    h.focus();
    for value in ["tags", "gN:trip-2026", "gN:Trip 2026", "gN:\"Trip 2026\""] {
        h.type_text(value);
        h.settle();
        assert!(h.combobox.items().is_empty(), "{value}");
    }
    assert_eq!(h.searches, 0);
    h.type_text("cat 2026");
    h.settle();
    assert_eq!(h.values(), ["trip-2026"]);
}

#[test]
fn obsolete_answers_are_dropped_and_blur_stops_reading() {
    let mut h = Harness::new(&["cat", "dog"], ComboboxConfig::SEARCH);
    h.focus();
    h.type_text("ca");
    assert_eq!(h.searches, 1);
    let effects = h.combobox.caret_moved(2);
    h.apply(effects);
    assert_eq!(h.searches, 1, "the same request is not sent twice");
    h.type_text("do");
    h.settle();
    assert_eq!(h.values(), ["dog"]);
    let effects = h.combobox.focus_out();
    h.apply(effects);
    assert!(h.combobox.items().is_empty());
    assert_eq!(h.searches, 2);
}

#[test]
fn a_failure_lasts_across_blur_until_the_context_or_vocabulary_changes() {
    let mut h = Harness::new(&["cat"], ComboboxConfig::SEARCH);
    h.focus();
    h.fail_next = true;
    h.type_text("ca");
    h.settle();
    assert!(h.combobox.failed());
    let effects = h.combobox.focus_out();
    h.apply(effects);
    assert!(h.combobox.failed(), "blur keeps the Retry");
    let effects = h.combobox.set_value("", 0);
    h.apply(effects);
    assert!(!h.combobox.failed());

    h.focus();
    h.type_text("ca");
    h.settle();
    assert!(h.combobox.failed(), "the client stays failed until Retry");
    let effects = h.combobox.retry();
    assert!(matches!(effects[0], ComboboxEffect::FocusInput));
    h.apply(effects);
    h.settle();
    assert!(!h.combobox.failed());
    assert_eq!(h.values(), ["cat"]);

    h.fail_next = true;
    h.type_text("c");
    h.settle();
    assert!(h.combobox.failed());
    let effects = h.combobox.set_vocabulary(&vocabulary(&["car"]));
    h.apply(effects);
    assert!(!h.combobox.failed());
}

#[test]
fn a_disabled_field_closes_and_takes_no_key() {
    let mut h = Harness::new(&["cat"], ComboboxConfig::SEARCH);
    h.focus();
    h.type_text("ca");
    h.settle();
    let effects = h.combobox.set_disabled(true);
    h.apply(effects);
    assert!(!h.combobox.session_open());
    assert!(!h.key(ComboboxKey::Enter));
    assert_eq!(h.submits, 0);
}

#[test]
fn a_new_vocabulary_hides_the_old_results_and_resets_the_active_item() {
    let mut h = Harness::new(&["cat", "car", "camera"], ComboboxConfig::SEARCH);
    h.focus();
    h.type_text("ca");
    h.settle();
    assert!(h.key(ComboboxKey::Down));
    assert!(h.key(ComboboxKey::Down));
    assert_eq!(h.values()[2], "camera");
    let effects = h.combobox.set_vocabulary(&vocabulary(&["camera", "cat"]));
    h.apply(effects);
    assert!(
        h.combobox.items().is_empty(),
        "results of the old vocabulary are hidden"
    );
    h.settle();
    assert_eq!(h.values(), ["camera", "cat"]);
    assert_eq!(h.combobox.active(), Some(0));
}
