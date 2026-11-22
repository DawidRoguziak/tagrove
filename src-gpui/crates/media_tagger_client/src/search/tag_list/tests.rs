use std::sync::Arc;

use media_tagger_core::models::TagListPage;

use super::*;
use crate::backend::BackendError;

fn vocabulary(items: &[&str]) -> Arc<Vec<String>> {
    Arc::new(items.iter().map(|item| (*item).to_owned()).collect())
}

fn page(items: &[&str], total: i64) -> BackendResult<TagListPage> {
    Ok(TagListPage {
        items: items.iter().map(|item| (*item).to_owned()).collect(),
        total,
    })
}

fn offline() -> BackendResult<TagListPage> {
    Err(BackendError::rejected("offline"))
}

/// The page requests among `effects`: token, query and offset.
fn loads(effects: &[TagListEffect]) -> Vec<(RequestToken, String, i64)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TagListEffect::LoadPage {
                token,
                query,
                offset,
                limit,
            } => {
                assert_eq!(*limit, 100);
                Some((*token, query.clone(), *offset))
            }
            _ => None,
        })
        .collect()
}

fn timer(effects: &[TagListEffect]) -> RequestToken {
    effects
        .iter()
        .find_map(|effect| match effect {
            TagListEffect::ClickTimer { token, delay } => {
                assert_eq!(*delay, SINGLE_CLICK_DELAY);
                Some(*token)
            }
            _ => None,
        })
        .expect("a click timer")
}

fn items(list: &TagList) -> Vec<&str> {
    list.page().items.iter().map(String::as_str).collect()
}

/// An open list whose first page answered `answer`.
fn opened(known: &[&str], answer: BackendResult<TagListPage>) -> TagList {
    let mut list = TagList::new();
    list.handle(TagListInput::SetVocabulary(vocabulary(known)));
    let effects = list.handle(TagListInput::Open);
    let (token, _, _) = loads(&effects)[0].clone();
    list.handle(TagListInput::PageLoaded {
        token,
        result: answer,
    });
    list
}

#[test]
fn rejects_old_queries_deduplicates_page_requests_and_stops_on_close() {
    let mut list = TagList::new();
    list.handle(TagListInput::SetVocabulary(vocabulary(&["old", "new"])));
    list.handle(TagListInput::SetQuery("old".into()));
    let first = loads(&list.handle(TagListInput::Open));
    assert_eq!(first.len(), 1);
    assert_eq!((first[0].1.as_str(), first[0].2), ("old", 0));
    let second = loads(&list.handle(TagListInput::SetQuery("new".into())));
    assert_eq!(second[0].1, "new");

    list.handle(TagListInput::PageLoaded {
        token: first[0].0,
        result: page(&["obsolete"], 1),
    });
    assert!(!items(&list).contains(&"obsolete"));
    assert!(list.loading());
    assert_eq!(items(&list), ["new"], "the fallback of the new query");

    list.handle(TagListInput::PageLoaded {
        token: second[0].0,
        result: page(&["new"], 200),
    });
    assert!(!list.loading());
    let third = loads(&list.handle(TagListInput::LoadMore));
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].2, 1);
    assert!(
        loads(&list.handle(TagListInput::LoadMore)).is_empty(),
        "one request at a time"
    );
    assert!(list.loading_more());

    list.handle(TagListInput::Close);
    list.handle(TagListInput::PageLoaded {
        token: third[0].0,
        result: page(&["late"], 200),
    });
    assert!(items(&list).is_empty());
    assert!(!list.loading_more());
    assert!(!list.is_open());
}

#[test]
fn stops_automatic_pagination_after_an_empty_fallback_but_permits_an_explicit_retry() {
    let mut list = opened(&["one"], page(&["one"], 200));
    let fits = ListMetrics::default();
    let next = loads(&list.handle(TagListInput::ListMetrics(fits)));
    assert_eq!(
        next.len(),
        1,
        "the content fits: the next page loads on its own"
    );
    list.handle(TagListInput::PageLoaded {
        token: next[0].0,
        result: offline(),
    });
    assert!(list.auto_load_blocked());
    assert_eq!(items(&list), ["one"]);
    for _ in 0..3 {
        assert!(loads(&list.handle(TagListInput::ListMetrics(fits))).is_empty());
    }

    let retry = loads(&list.handle(TagListInput::LoadMore));
    assert_eq!(retry.len(), 1);
    list.handle(TagListInput::PageLoaded {
        token: retry[0].0,
        result: page(&["two"], 2),
    });
    assert_eq!(items(&list), ["one", "two"]);
    assert_eq!(list.page().total, 2);
}

#[test]
fn a_failed_next_page_is_filled_from_the_known_tags() {
    let mut list = opened(&["a", "b", "c"], page(&["a"], 3));
    let next = loads(&list.handle(TagListInput::LoadMore));
    list.handle(TagListInput::PageLoaded {
        token: next[0].0,
        result: offline(),
    });
    assert_eq!(items(&list), ["a", "b", "c"]);
    assert_eq!(list.page().total, 3);
    assert!(!list.auto_load_blocked());
}

#[test]
fn the_known_tags_are_prepared_only_while_open() {
    let mut list = TagList::new();
    list.handle(TagListInput::SetVocabulary(vocabulary(&["beta", "alpha"])));
    list.handle(TagListInput::SetVocabulary(vocabulary(&["gamma", "delta"])));
    assert_eq!(list.fallback_builds(), 0);

    let effects = list.handle(TagListInput::Open);
    assert_eq!(list.fallback_builds(), 1);
    assert_eq!(
        items(&list),
        ["delta", "gamma"],
        "the fallback shows at once"
    );
    assert!(list.loading());
    assert_eq!(list.placeholder_key(), None);
    let (token, _, _) = loads(&effects)[0].clone();
    list.handle(TagListInput::PageLoaded {
        token,
        result: offline(),
    });
    assert_eq!(items(&list), ["delta", "gamma"]);

    list.handle(TagListInput::Close);
    list.handle(TagListInput::SetVocabulary(vocabulary(&[
        "zeta", "epsilon",
    ])));
    assert_eq!(list.fallback_builds(), 1);
    list.handle(TagListInput::Open);
    assert_eq!(list.fallback_builds(), 2);
    assert_eq!(items(&list), ["epsilon", "zeta"]);
}

#[test]
fn a_new_vocabulary_while_open_starts_over() {
    let mut list = opened(&["a"], page(&["a"], 1));
    let same = list.vocabulary.clone();
    assert!(loads(&list.handle(TagListInput::SetVocabulary(same))).is_empty());
    let resets = list.scroll_resets();
    let effects = list.handle(TagListInput::SetVocabulary(vocabulary(&["a", "b"])));
    assert_eq!(loads(&effects).len(), 1);
    assert_eq!(list.scroll_resets(), resets + 1);
}

#[test]
fn the_fallback_deduplicates_and_filters_after_failures() {
    let mut list = opened(&[" Zebra ", "alpha", "Alpha", "beta"], offline());
    assert_eq!(items(&list), ["alpha", "beta", "Zebra"]);
    let effects = list.handle(TagListInput::SetQuery("alp".into()));
    let (token, query, offset) = loads(&effects)[0].clone();
    assert_eq!((query.as_str(), offset), ("alp", 0));
    list.handle(TagListInput::PageLoaded {
        token,
        result: offline(),
    });
    assert_eq!(items(&list), ["alpha"]);
}

#[test]
fn whitespace_around_the_filter_starts_nothing() {
    let mut list = opened(&["a"], page(&["a"], 1));
    assert_eq!(
        loads(&list.handle(TagListInput::SetQuery("a".into()))).len(),
        1
    );
    assert!(loads(&list.handle(TagListInput::SetQuery(" a ".into()))).is_empty());
    assert_eq!(list.query(), " a ");
}

#[test]
fn the_placeholder_tells_loading_empty_and_no_match_apart() {
    let mut list = TagList::new();
    let effects = list.handle(TagListInput::Open);
    assert_eq!(list.placeholder_key(), Some("tagList.loading"));
    let (token, _, _) = loads(&effects)[0].clone();
    list.handle(TagListInput::PageLoaded {
        token,
        result: page(&[], 0),
    });
    assert_eq!(list.placeholder_key(), Some("tagList.empty"));
    let effects = list.handle(TagListInput::SetQuery("x".into()));
    let (token, _, _) = loads(&effects)[0].clone();
    list.handle(TagListInput::PageLoaded {
        token,
        result: page(&[], 0),
    });
    assert_eq!(list.placeholder_key(), Some("tagList.noMatches"));
}

#[test]
fn scrolling_near_the_end_loads_the_next_page() {
    let mut list = opened(&[], page(&["Alpha", "Beta"], 4));
    let tall = ListMetrics {
        content_height: 400.0,
        viewport_height: 100.0,
        scroll_top: 0.0,
    };
    assert!(loads(&list.handle(TagListInput::ListMetrics(tall))).is_empty());
    let scrolled = loads(&list.handle(TagListInput::ListMetrics(ListMetrics {
        scroll_top: 260.0,
        ..tall
    })));
    assert_eq!(scrolled.len(), 1);
    assert_eq!(scrolled[0].2, 2);
    list.handle(TagListInput::PageLoaded {
        token: scrolled[0].0,
        result: page(&["Beta", "Gamma"], 4),
    });
    assert_eq!(items(&list), ["Alpha", "Beta", "Gamma"]);
    assert_eq!(
        list.metrics(),
        None,
        "a new page asks for a new measurement"
    );
}

#[test]
fn only_sizes_and_scrolls_near_the_end_are_worth_reporting() {
    let known = ListMetrics {
        content_height: 4_000.0,
        viewport_height: 300.0,
        scroll_top: 0.0,
    };
    assert!(known.worth_reporting(None), "nothing measured yet");
    assert!(!known.worth_reporting(Some(known)), "nothing changed");
    let middle = ListMetrics {
        scroll_top: 1_500.0,
        ..known
    };
    assert!(
        !middle.worth_reporting(Some(known)),
        "a scroll in the middle of the list costs no report"
    );
    assert!(ListMetrics {
        content_height: 4_100.0,
        ..middle
    }
    .worth_reporting(Some(known)));
    assert!(ListMetrics {
        viewport_height: 320.0,
        ..middle
    }
    .worth_reporting(Some(known)));
    let near_end = ListMetrics {
        scroll_top: 3_600.0,
        ..known
    };
    assert!(
        near_end.worth_reporting(Some(known)),
        "a scroll near the end"
    );
    assert!(ListMetrics {
        scroll_top: 3_650.0,
        ..known
    }
    .worth_reporting(Some(near_end)));
    assert!(
        middle.worth_reporting(Some(near_end)),
        "leaving the end is reported, so that coming back counts as a scroll"
    );
}

#[test]
fn a_scroll_back_to_the_end_after_leaving_it_loads_again() {
    let mut list = opened(&[], page(&["Alpha", "Beta"], 4));
    let tall = ListMetrics {
        content_height: 400.0,
        viewport_height: 100.0,
        scroll_top: 0.0,
    };
    // What the view does after each layout.
    fn report(list: &mut TagList, metrics: ListMetrics) -> Vec<(RequestToken, String, i64)> {
        if metrics.worth_reporting(list.metrics()) {
            loads(&list.handle(TagListInput::ListMetrics(metrics)))
        } else {
            Vec::new()
        }
    }
    assert!(report(&mut list, tall).is_empty());
    let end = ListMetrics {
        scroll_top: 260.0,
        ..tall
    };
    let first = report(&mut list, end);
    assert_eq!(first.len(), 1);
    // The page fails and adds nothing: loading on its own stops.
    list.handle(TagListInput::PageLoaded {
        token: first[0].0,
        result: offline(),
    });
    assert!(list.auto_load_blocked());
    assert!(
        report(&mut list, end).is_empty(),
        "the same measurement again"
    );
    let middle = |scroll_top| ListMetrics { scroll_top, ..tall };
    assert!(report(&mut list, middle(100.0)).is_empty());
    assert!(report(&mut list, middle(50.0)).is_empty());
    assert_eq!(
        list.metrics(),
        Some(middle(100.0)),
        "only leaving the end was reported"
    );
    assert_eq!(
        report(&mut list, end).len(),
        1,
        "scrolling back to the same end is a scroll again"
    );
}

#[test]
fn single_and_double_clicks_build_a_sorted_filter() {
    let mut list = opened(&[], page(&["banana", "Apple", "cherry"], 3));
    list.handle(TagListInput::DoubleClick {
        tag: "cherry".into(),
    });
    let first = timer(&list.handle(TagListInput::Click {
        tag: "banana".into(),
        count: 1,
    }));
    assert!(list.click_pending());
    assert_eq!(list.selections().included_count(), 0);
    list.handle(TagListInput::ClickTimerFired(first));
    assert_eq!(
        list.selections().mode("banana"),
        Some(TagSelectionMode::Include)
    );
    list.handle(TagListInput::Click {
        tag: "Apple".into(),
        count: 1,
    });
    // Apply flushes the waiting click.
    let effects = list.handle(TagListInput::Apply);
    assert_eq!(
        effects,
        [TagListEffect::ApplySearch("Apple banana -cherry".into())]
    );
    assert!(list.applying());
    list.handle(TagListInput::ApplySettled { ok: true });
    assert!(!list.is_open());
    assert!(list.selections().is_empty());
    assert_eq!(list.query(), "");
}

#[test]
fn a_click_on_another_tag_applies_the_waiting_one_and_a_stale_timer_does_nothing() {
    let mut list = opened(&[], page(&["a", "b"], 2));
    let first = timer(&list.handle(TagListInput::Click {
        tag: "a".into(),
        count: 1,
    }));
    let second = timer(&list.handle(TagListInput::Click {
        tag: "b".into(),
        count: 1,
    }));
    assert_eq!(list.selections().mode("a"), Some(TagSelectionMode::Include));
    list.handle(TagListInput::ClickTimerFired(first));
    assert_eq!(list.selections().mode("b"), None);
    list.handle(TagListInput::ClickTimerFired(second));
    assert_eq!(list.selections().mode("b"), Some(TagSelectionMode::Include));
    // The second click of a double click is not a click.
    assert!(list
        .handle(TagListInput::Click {
            tag: "a".into(),
            count: 2,
        })
        .is_empty());
}

#[test]
fn a_double_click_cancels_the_waiting_click() {
    let mut list = opened(&[], page(&["a"], 1));
    let token = timer(&list.handle(TagListInput::Click {
        tag: "a".into(),
        count: 1,
    }));
    list.handle(TagListInput::DoubleClick { tag: "a".into() });
    list.handle(TagListInput::ClickTimerFired(token));
    assert_eq!(list.selections().mode("a"), Some(TagSelectionMode::Exclude));
    assert_eq!(list.selections().filter_input(), "-a");
}

#[test]
fn a_failed_apply_keeps_the_list_and_its_choices() {
    let mut list = opened(&[], page(&["Alpha"], 1));
    list.handle(TagListInput::DoubleClick {
        tag: "Alpha".into(),
    });
    list.handle(TagListInput::Apply);
    assert!(
        list.handle(TagListInput::Apply).is_empty(),
        "one apply at a time"
    );
    list.handle(TagListInput::Close);
    list.handle(TagListInput::DoubleClick {
        tag: "Alpha".into(),
    });
    assert!(list.is_open(), "closing is refused while applying");
    assert_eq!(list.selections().filter_input(), "-Alpha");

    list.handle(TagListInput::ApplySettled { ok: false });
    assert!(list.is_open());
    assert!(!list.applying());
    assert_eq!(list.selections().filter_input(), "-Alpha");
    assert_eq!(
        list.handle(TagListInput::Apply),
        [TagListEffect::ApplySearch("-Alpha".into())]
    );
}

#[test]
fn literal_names_go_through_every_selection() {
    for name in ["constructor", "__proto__"] {
        for (action, expected) in [("single", name.to_owned()), ("double", format!("-{name}"))] {
            let mut list = opened(&[], page(&[name], 1));
            if action == "single" {
                let token = timer(&list.handle(TagListInput::Click {
                    tag: name.into(),
                    count: 1,
                }));
                list.handle(TagListInput::ClickTimerFired(token));
            } else {
                list.handle(TagListInput::DoubleClick { tag: name.into() });
            }
            assert_eq!(
                list.handle(TagListInput::Apply),
                [TagListEffect::ApplySearch(expected)]
            );
        }
    }
}
