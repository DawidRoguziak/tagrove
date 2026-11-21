//! The known tags against the scripted backend and the deterministic executor (the
//! cases of `createKnownTags.test.ts`).

use std::sync::Arc;

use media_tagger_core::models::TagListPage;

use super::{KnownTags, VocabularyEvent, VocabularyInput, VocabularyRunner, VOCABULARY_PAGE_SIZE};
use crate::{
    backend::{fake::sequence, fake::Call, BackendError, BackendResult, FakeBackend},
    exec::{Executor, ManualExecutor},
    library::Ticket,
    model::Driver,
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<KnownTags, VocabularyRunner>,
}

fn page(items: &[&str], total: i64) -> BackendResult<TagListPage> {
    Ok(TagListPage {
        items: items.iter().map(|item| (*item).to_owned()).collect(),
        total,
    })
}

fn numbered(range: std::ops::Range<usize>, total: i64) -> BackendResult<TagListPage> {
    Ok(TagListPage {
        items: range.map(|index| format!("tag-{index}")).collect(),
        total,
    })
}

impl Fixture {
    fn new() -> Self {
        let backend = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            KnownTags::new(),
            VocabularyRunner::new(backend.clone()),
            executor.clone() as Arc<dyn Executor>,
        );
        Self {
            backend,
            executor,
            driver,
        }
    }

    /// Answers the calls of the test in order.
    fn script(&self, answers: Vec<BackendResult<TagListPage>>) {
        let mut answers = sequence(answers);
        self.backend.on_list_tags(move |_, _, _| answers());
    }

    fn send(&mut self, input: VocabularyInput) {
        self.driver.handle(input);
    }

    fn refresh(&mut self, ticket: u64) {
        self.send(VocabularyInput::Refresh {
            ticket: Some(Ticket(ticket)),
        });
    }

    fn after_mutation(&mut self, vocabulary_changed: bool, ticket: u64) {
        self.send(VocabularyInput::RefreshAfterMutation {
            vocabulary_changed,
            ticket: Some(Ticket(ticket)),
        });
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn events(&mut self) -> Vec<VocabularyEvent> {
        self.driver.model_mut().take_events()
    }

    fn known(&self) -> Vec<String> {
        self.driver.model().known_tags().to_vec()
    }

    fn calls(&self) -> Vec<(i64, i64)> {
        self.backend
            .calls()
            .into_iter()
            .map(|call| match call {
                Call::ListTags {
                    query,
                    offset,
                    limit,
                } => {
                    assert_eq!(query, "");
                    (offset, limit)
                }
                other => panic!("unexpected call {other:?}"),
            })
            .collect()
    }
}

fn settled(ticket: u64, tags: &[&str]) -> VocabularyEvent {
    VocabularyEvent::Settled {
        ticket: Ticket(ticket),
        result: Ok(Arc::new(tags.iter().map(|tag| (*tag).to_owned()).collect())),
    }
}

#[test]
fn all_pages_are_loaded_in_backend_sized_chunks() {
    let mut fx = Fixture::new();
    fx.script(vec![numbered(0..200, 201), numbered(200..201, 201)]);

    fx.refresh(1);
    assert!(fx.driver.model().refreshing());
    fx.settle();

    assert_eq!(fx.calls(), [(0, VOCABULARY_PAGE_SIZE), (200, 200)]);
    assert_eq!(fx.known().len(), 201);
    assert!(!fx.driver.model().refreshing() && !fx.driver.model().is_dirty());
    assert_eq!(fx.events().len(), 1);
}

#[test]
fn a_failed_later_page_publishes_nothing_and_keeps_the_previous_vocabulary() {
    let mut fx = Fixture::new();
    fx.script(vec![
        page(&["confirmed"], 1),
        numbered(0..200, 201),
        Err(BackendError::rejected("page failed")),
    ]);
    fx.send(VocabularyInput::Refresh { ticket: None });
    fx.settle();

    fx.refresh(2);
    fx.settle();

    assert_eq!(fx.known(), ["confirmed"]);
    assert_eq!(
        fx.events(),
        [VocabularyEvent::Settled {
            ticket: Ticket(2),
            result: Err(BackendError::rejected("page failed")),
        }]
    );
    assert!(fx.driver.model().is_dirty());
}

#[test]
fn a_refresh_during_a_pass_joins_it_and_the_superseded_read_is_thrown_away() {
    for fail in [false, true] {
        let mut fx = Fixture::new();
        fx.script(vec![
            if fail {
                Err(BackendError::rejected("superseded"))
            } else {
                page(&["old"], 400)
            },
            page(&["new"], 1),
        ]);

        fx.after_mutation(true, 1);
        fx.after_mutation(true, 2);
        fx.after_mutation(false, 3);
        assert_eq!(fx.executor.pending(), 1, "one pass, one request at a time");
        fx.settle();

        assert_eq!(fx.known(), ["new"], "failure={fail}");
        assert_eq!(fx.calls(), [(0, 200), (0, 200)]);
        assert_eq!(
            fx.events(),
            [
                settled(1, &["new"]),
                settled(2, &["new"]),
                settled(3, &["new"])
            ]
        );
    }
}

#[test]
fn a_reset_during_pagination_publishes_nothing_and_answers_the_reader_at_once() {
    let mut fx = Fixture::new();
    fx.script(vec![page(&["old"], 400)]);

    fx.refresh(1);
    fx.send(VocabularyInput::Reset);
    assert_eq!(fx.events(), [settled(1, &[])]);
    fx.settle();

    assert!(fx.known().is_empty());
    assert_eq!(
        fx.calls().len(),
        1,
        "the interrupted pass asks for no more pages"
    );
    assert!(fx.driver.model().is_dirty());
}

#[test]
fn saves_that_change_nothing_read_nothing_once_the_vocabulary_is_current() {
    let mut fx = Fixture::new();
    fx.backend.on_list_tags(|_, _, _| page(&["cat", "dog"], 2));
    fx.send(VocabularyInput::Refresh { ticket: None });
    fx.settle();
    let original = fx.driver.model().known_tags().clone();
    let version = fx.driver.model().version();

    for ticket in 0..20 {
        fx.after_mutation(false, ticket);
        assert_eq!(fx.executor.pending(), 0);
    }
    assert_eq!(fx.events().len(), 20);
    assert_eq!(fx.calls().len(), 1);

    // A changed vocabulary is read again; equal content keeps the published list.
    fx.after_mutation(true, 30);
    fx.settle();
    assert_eq!(fx.calls().len(), 2);
    assert!(Arc::ptr_eq(fx.driver.model().known_tags(), &original));
    assert_eq!(fx.driver.model().version(), version);

    fx.backend.on_list_tags(|_, _, _| page(&["cat", "new"], 2));
    fx.after_mutation(true, 31);
    fx.settle();
    assert_eq!(fx.known(), ["cat", "new"]);
    assert!(!Arc::ptr_eq(fx.driver.model().known_tags(), &original));
    assert_eq!(fx.driver.model().version(), version + 1);
}

#[test]
fn the_published_list_is_compared_by_ordered_values_and_length() {
    let mut fx = Fixture::new();
    fx.script(vec![
        page(&["a", "b"], 2),
        page(&["b", "a"], 2),
        page(&["b"], 1),
    ]);
    fx.send(VocabularyInput::Refresh { ticket: None });
    fx.settle();
    let first = fx.driver.model().known_tags().clone();

    fx.after_mutation(true, 1);
    fx.settle();
    let second = fx.driver.model().known_tags().clone();
    assert!(!Arc::ptr_eq(&first, &second));

    fx.after_mutation(true, 2);
    fx.settle();
    assert!(!Arc::ptr_eq(fx.driver.model().known_tags(), &second));
    assert_eq!(fx.known(), ["b"]);
}

#[test]
fn a_failed_refresh_is_retried_by_the_next_save_even_an_unchanged_one() {
    let mut fx = Fixture::new();
    fx.script(vec![
        page(&["old"], 1),
        Err(BackendError::rejected("offline")),
        page(&["new"], 1),
    ]);
    fx.send(VocabularyInput::Refresh { ticket: None });
    fx.settle();

    fx.after_mutation(true, 1);
    fx.settle();
    assert_eq!(fx.known(), ["old"]);
    assert!(matches!(
        fx.events()[..],
        [VocabularyEvent::Settled { result: Err(_), .. }]
    ));

    fx.after_mutation(false, 2);
    fx.settle();
    assert_eq!(fx.known(), ["new"]);
    assert_eq!(fx.calls().len(), 3);
}

#[test]
fn a_vocabulary_that_never_loaded_is_read_by_an_unchanged_save() {
    let mut fx = Fixture::new();
    fx.script(vec![
        Err(BackendError::rejected("offline")),
        page(&["new"], 1),
    ]);
    fx.send(VocabularyInput::Refresh { ticket: None });
    fx.settle();
    assert!(fx.known().is_empty());

    fx.after_mutation(false, 1);
    fx.settle();
    assert_eq!(fx.known(), ["new"]);
}

#[test]
fn a_reset_allows_a_fresh_pass_and_an_obsolete_reply_cannot_end_the_dirty_state() {
    let mut fx = Fixture::new();
    // In the order the backend is called: the fresh pass, the obsolete read, the retry.
    fx.script(vec![
        Err(BackendError::rejected("offline")),
        page(&["stale"], 400),
        page(&["current"], 1),
    ]);

    fx.after_mutation(true, 1);
    fx.send(VocabularyInput::Reset);
    fx.after_mutation(false, 2);
    assert_eq!(fx.executor.pending(), 2, "the old read and the fresh pass");
    // The fresh pass fails first, then the obsolete read answers.
    assert!(fx.executor.run_nth(1));
    assert!(fx.executor.run_next());
    fx.driver.pump();
    assert!(fx.known().is_empty());
    assert!(fx.driver.model().is_dirty());
    assert_eq!(
        fx.events(),
        [
            settled(1, &[]),
            VocabularyEvent::Settled {
                ticket: Ticket(2),
                result: Err(BackendError::rejected("offline")),
            }
        ]
    );

    fx.after_mutation(false, 3);
    fx.settle();
    assert_eq!(fx.known(), ["current"]);
}

#[test]
fn disposal_stops_pagination_and_publication() {
    let mut fx = Fixture::new();
    fx.script(vec![page(&["stale"], 400)]);

    fx.after_mutation(true, 1);
    fx.send(VocabularyInput::Dispose);
    assert_eq!(fx.events(), [settled(1, &[])]);
    fx.settle();
    assert!(fx.known().is_empty());
    assert_eq!(fx.calls().len(), 1);

    // Nothing is read after the disposal.
    fx.refresh(2);
    assert_eq!(fx.executor.pending(), 0);
    assert_eq!(fx.events(), [settled(2, &[])]);
}

#[test]
fn an_empty_page_ends_a_pass_whose_total_is_never_reached() {
    let mut fx = Fixture::new();
    fx.script(vec![page(&["only"], 5), page(&[], 5)]);
    fx.refresh(1);
    fx.settle();
    assert_eq!(fx.known(), ["only"]);
    assert_eq!(fx.calls(), [(0, 200), (1, 200)]);
}

// ---- Ported behaviour cases (E08 test step) ----

/// createKnownTags.test.ts "joins an active refresh for unchanged saves without
/// requesting another pass".
#[test]
fn an_unchanged_save_joins_the_running_pass_without_another_read() {
    let mut fx = Fixture::new();
    fx.script(vec![page(&["cat"], 1)]);

    fx.after_mutation(true, 1);
    fx.after_mutation(false, 2);
    assert_eq!(fx.executor.pending(), 1);
    fx.settle();

    assert_eq!(fx.calls().len(), 1);
    assert_eq!(fx.known(), ["cat"]);
    assert_eq!(fx.events(), [settled(1, &["cat"]), settled(2, &["cat"])]);
}

/// createKnownTags.test.ts "ignores an older refresh that resolves after a newer
/// one": a second refresh joins the running pass, the older read is thrown away
/// and both callers get the newer list.
#[test]
fn a_second_refresh_joins_the_pass_and_publishes_the_newer_read() {
    let mut fx = Fixture::new();
    fx.script(vec![page(&["old"], 1), page(&["new"], 1)]);

    fx.refresh(1);
    fx.refresh(2);
    assert_eq!(fx.executor.pending(), 1, "one request at a time");
    fx.settle();

    assert_eq!(fx.known(), ["new"]);
    assert_eq!(fx.calls(), [(0, 200), (0, 200)]);
    assert_eq!(fx.events(), [settled(1, &["new"]), settled(2, &["new"])]);
    assert!(!fx.driver.model().is_dirty());
}
