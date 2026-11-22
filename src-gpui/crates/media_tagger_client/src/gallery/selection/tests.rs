//! The selection model on its own: inputs in, effects out, reads answered by hand.
//! Asset ID = index + 1 in session 1 unless a test answers otherwise.

use std::time::Instant;

use super::{
    merge_ranges, ranges_contain, IndexRange, Interaction, QuerySnapshot, Selection,
    SelectionEffect, SelectionError, SelectionInput, SelectionNotice, SelectionWrite,
    MATERIALIZE_LIMIT, SELECTION_TICKET_BASE, SESSION_KEEPALIVE,
};
use crate::{
    backend::{BackendError, SelectionSpec},
    library::{KeepAlive, Mutation, QueryError, Ticket},
    model::{Model as _, RequestToken},
};

struct Fixture {
    model: Selection,
    query: QuerySnapshot,
    /// Range reads that wait for an answer: ticket, first and last index.
    reads: Vec<(Ticket, usize, usize)>,
    /// Every range read asked for so far.
    read_log: Vec<(usize, usize)>,
    /// Keepalive reads that wait for an answer.
    keepalives: Vec<(Ticket, u64)>,
    /// The session of every keepalive read asked for so far.
    keepalive_log: Vec<u64>,
    /// The keepalive timers that are armed.
    timers: Vec<RequestToken>,
}

impl Fixture {
    /// Selection mode on, session 1 with a million results.
    fn new() -> Self {
        let query = QuerySnapshot {
            query_epoch: 1,
            identity_epoch: 0,
            failure_epoch: 0,
            session_id: Some(1),
            total: 1_000_000,
        };
        let mut fixture = Self {
            model: Selection::new(query),
            query,
            reads: Vec::new(),
            read_log: Vec::new(),
            keepalives: Vec::new(),
            keepalive_log: Vec::new(),
            timers: Vec::new(),
        };
        fixture.send(SelectionInput::ToggleMode);
        fixture
    }

    fn send(&mut self, input: SelectionInput) {
        for effect in self.model.handle(input) {
            match effect {
                SelectionEffect::ReadIdRange { ticket, start, end } => {
                    assert!(Selection::owns(ticket) && ticket.0 > SELECTION_TICKET_BASE);
                    self.reads.push((ticket, start, end));
                    self.read_log.push((start, end));
                }
                SelectionEffect::KeepSessionAlive { ticket, session_id } => {
                    assert!(Selection::owns(ticket));
                    self.keepalives.push((ticket, session_id));
                    self.keepalive_log.push(session_id);
                }
                SelectionEffect::KeepAliveTimer { token, delay } => {
                    assert_eq!(delay, SESSION_KEEPALIVE);
                    self.timers.push(token);
                }
            }
        }
    }

    fn interact(&mut self, interaction: Interaction) {
        self.send(SelectionInput::Interact(interaction));
    }

    fn click(&mut self, asset_id: i64, asset_index: usize) {
        self.click_with(asset_id, asset_index, false, false);
    }

    fn click_with(&mut self, asset_id: i64, asset_index: usize, shift: bool, ctrl_like: bool) {
        self.interact(Interaction::Click {
            asset_id,
            asset_index,
            ctrl_like,
            shift,
        });
    }

    fn commit(&mut self, ranges: &[IndexRange], additive: bool) {
        self.interact(Interaction::RectangleCommit {
            ranges: ranges.to_vec(),
            additive,
        });
    }

    /// Answers the oldest waiting range read with `answer(first, last)`.
    fn answer_with(&mut self, answer: impl FnOnce(usize, usize) -> Result<Vec<i64>, QueryError>) {
        let (ticket, first, last) = self.reads.remove(0);
        self.send(SelectionInput::IdRangeRead {
            ticket,
            result: answer(first, last),
        });
    }

    /// Answers the oldest waiting range read with ID = index + 1.
    fn answer(&mut self) {
        self.answer_with(|first, last| Ok((first..=last).map(|index| index as i64 + 1).collect()));
    }

    /// Answers the oldest waiting keepalive read.
    fn keepalive_answer(&mut self, result: Result<KeepAlive, QueryError>) {
        let (ticket, _) = self.keepalives.remove(0);
        self.send(SelectionInput::SessionKeptAlive { ticket, result });
    }

    /// One keepalive interval passes: every armed timer fires.
    fn tick(&mut self) {
        for token in std::mem::take(&mut self.timers) {
            self.send(SelectionInput::KeepAliveDue(token));
        }
    }

    /// A query result arrives: the session changes (or stays, for a cache hit).
    /// `None`: a query is pending.
    fn next_query(&mut self, session: Option<u64>) {
        self.query.query_epoch += 1;
        self.query.session_id = session;
        self.send(SelectionInput::Sync(self.query));
    }

    /// A query or page read fails; a failed refresh leaves no session behind.
    fn fail_read(&mut self) {
        self.query.failure_epoch += 1;
        self.send(SelectionInput::Sync(self.query));
    }

    fn reset_identity(&mut self) {
        self.query.identity_epoch += 1;
        self.send(SelectionInput::Sync(self.query));
    }

    fn ids(&self) -> Vec<i64> {
        self.model.ids().to_vec()
    }

    fn spec(&self) -> Option<SelectionSpec> {
        self.model.capture().spec
    }
}

fn spec(
    session_id: u64,
    ranges: &[IndexRange],
    excluded: &[i64],
    included: &[i64],
) -> SelectionSpec {
    SelectionSpec {
        session_id,
        ranges: ranges.to_vec(),
        excluded: excluded.to_vec(),
        included: included.to_vec(),
    }
}

fn cancelled() -> QueryError {
    QueryError::Cancelled
}

#[test]
fn nothing_is_selected_while_selection_mode_is_off() {
    let mut model = Selection::new(QuerySnapshot {
        session_id: Some(1),
        total: 10,
        ..QuerySnapshot::default()
    });
    assert!(!model.enabled());
    let effects = model.handle(SelectionInput::Interact(Interaction::Click {
        asset_id: 1,
        asset_index: 0,
        ctrl_like: false,
        shift: false,
    }));
    assert!(effects.is_empty());
    assert!(model
        .handle(SelectionInput::Interact(Interaction::SelectAll))
        .is_empty());
    assert_eq!(model.count(), 0);
    assert_eq!(model.key(), 0);
}

#[test]
fn toggles_additive_clicks_at_once_and_clears_when_disabled() {
    let mut fx = Fixture::new();
    fx.click(1, 0);
    fx.click(2, 1);
    fx.click(1, 0);
    assert_eq!(fx.ids(), [2]);
    assert_eq!(fx.model.count(), 1);
    assert!(fx.model.is_selected(1, Some(2)) && !fx.model.is_selected(0, Some(1)));

    fx.send(SelectionInput::ToggleMode);
    assert!(!fx.model.enabled());
    assert!(fx.ids().is_empty());
    assert_eq!(fx.model.anchor(), None);
}

#[test]
fn resolves_global_shift_ranges_and_rectangle_gaps_with_one_envelope_read() {
    let mut fx = Fixture::new();
    fx.click(1, 0);
    fx.click_with(2051, 2050, true, false);
    assert!(fx.model.busy());
    fx.answer();
    assert!(!fx.model.busy());
    assert_eq!(fx.ids().len(), 2051);
    assert_eq!(fx.read_log, [(0, 2050)]);
    // Shift does not move the anchor.
    assert_eq!(fx.model.anchor(), Some(0));

    fx.read_log.clear();
    fx.commit(&[[0, 1], [4, 5], [256, 257]], false);
    fx.answer();
    assert_eq!(fx.ids(), [1, 2, 5, 6, 257, 258]);
    assert_eq!(fx.read_log, [(0, 257)]);
    assert_eq!(fx.model.anchor(), None, "a rectangle clears the anchor");
}

#[test]
fn selects_a_100k_shift_range_without_reading_ids_and_keeps_it_when_a_read_fails() {
    let mut fx = Fixture::new();
    fx.click(1, 0);
    fx.click_with(100_000, 99_999, true, false);
    assert_eq!(fx.model.count(), 100_000);
    assert!(fx.model.ranged());
    assert!(fx.ids().is_empty());
    assert!(fx.read_log.is_empty());
    assert_eq!(fx.spec(), Some(spec(1, &[[0, 99_999]], &[], &[])));

    fx.commit(&[[0, 9]], false);
    fx.answer_with(|_, _| Err(cancelled()));
    assert_eq!(fx.model.count(), 100_000);
    assert_eq!(fx.model.error(), Some(&SelectionError::Read(cancelled())));
    assert!(fx.model.error().unwrap().to_string().contains("cancelled"));
}

#[test]
fn preserves_the_selection_and_offers_an_explicit_retry_for_incomplete_reads() {
    let mut fx = Fixture::new();
    fx.click(99, 98);
    fx.commit(&[[0, 2]], false);
    fx.answer_with(|_, _| Ok(vec![1]));
    assert_eq!(fx.ids(), [99]);
    assert_eq!(fx.model.error(), Some(&SelectionError::Incomplete));
    assert!(fx.model.error().unwrap().to_string().contains("incomplete"));
    assert!(!fx.model.busy());

    fx.send(SelectionInput::Retry);
    fx.answer();
    assert_eq!(fx.ids(), [1, 2, 3]);
    assert_eq!(fx.model.error(), None);
    // The retry was used up.
    fx.send(SelectionInput::Retry);
    assert!(fx.reads.is_empty());
}

#[test]
fn discards_a_late_selection_after_a_query_a_clear_a_disable_a_dispose_or_an_identity_reset() {
    for action in [
        "query", "clear", "disable", "dispose", "identity", "cancel", "click",
    ] {
        let mut fx = Fixture::new();
        fx.click(9, 8);
        fx.commit(&[[0, 2]], false);
        assert!(fx.model.busy(), "{action}");
        match action {
            "query" => fx.next_query(Some(1)),
            "clear" => fx.send(SelectionInput::Clear),
            "disable" => fx.send(SelectionInput::ToggleMode),
            "dispose" => fx.send(SelectionInput::Dispose),
            "identity" => fx.reset_identity(),
            "cancel" => fx.send(SelectionInput::Cancel),
            _ => fx.click(20, 19),
        }
        fx.answer();
        assert_ne!(fx.ids(), [1, 2, 3], "{action}");
        assert_eq!(fx.model.error(), None, "{action}");
    }
}

#[test]
fn a_newer_range_replaces_the_read_of_an_older_one() {
    let mut fx = Fixture::new();
    fx.commit(&[[0, 2]], false);
    fx.commit(&[[10, 11]], false);
    assert_eq!(fx.reads.len(), 2);
    // The older read answers last.
    let older = fx.reads.remove(0);
    fx.answer();
    assert_eq!(fx.ids(), [11, 12]);
    fx.reads.push(older);
    fx.answer();
    assert_eq!(fx.ids(), [11, 12]);
}

#[test]
fn retains_ids_across_queries_drops_the_anchor_and_clears_on_an_identity_reset() {
    let mut fx = Fixture::new();
    fx.click(1, 100);
    fx.next_query(Some(1));
    assert_eq!(fx.model.anchor(), None);
    // Without an anchor Shift adds the tile and sets the anchor.
    fx.click_with(2, 200, true, false);
    assert_eq!(fx.ids(), [1, 2]);
    assert_eq!(fx.model.anchor(), Some(200));
    assert!(fx.read_log.is_empty());

    fx.reset_identity();
    assert!(fx.ids().is_empty());
    assert!(fx.model.enabled(), "selection mode stays on");
}

#[test]
fn merges_sorted_overlapping_and_adjacent_ranges_and_searches_them() {
    assert_eq!(
        merge_ranges(&[[10, 20], [0, 3], [4, 5], [15, 30], [40, 39]]),
        [[0, 5], [10, 30]]
    );
    let ranges = merge_ranges(&[[0, 5], [10, 30], [100, 100]]);
    for index in [0, 5, 10, 30, 100] {
        assert!(ranges_contain(&ranges, index), "{index}");
    }
    for index in [6, 9, 31, 99, 101] {
        assert!(!ranges_contain(&ranges, index), "{index}");
    }
    assert!(!ranges_contain(&[], 0));
    assert!(merge_ranges(&[]).is_empty());
}

#[test]
fn selects_a_million_results_as_one_range_without_materializing_ids() {
    let mut fx = Fixture::new();
    fx.interact(Interaction::SelectAll);
    assert!(fx.read_log.is_empty());
    assert_eq!(fx.model.count(), 1_000_000);
    assert!(fx.ids().is_empty());
    assert!(fx.model.is_selected(0, Some(1)));
    assert!(fx.model.is_selected(999_999, Some(1_000_000)));
    assert!(fx.model.is_selected(999_999, None));

    // Ctrl-click inside the range excludes, again includes; counts follow.
    fx.click_with(501, 500, false, true);
    assert!(!fx.model.is_selected(500, Some(501)));
    assert!(
        fx.model.is_selected(500, None),
        "a slot without an asset follows the range"
    );
    assert_eq!(fx.model.count(), 999_999);
    fx.click_with(501, 500, false, true);
    assert_eq!(fx.model.count(), 1_000_000);
    fx.click_with(501, 500, false, true);
    assert_eq!(fx.spec(), Some(spec(1, &[[0, 999_999]], &[501], &[])));

    // A million lookups stay cheap: binary search over ranges plus a small map.
    let started = Instant::now();
    let selected = (0..1_000_000usize)
        .filter(|index| fx.model.is_selected(*index, Some(*index as i64 + 1)))
        .count();
    assert_eq!(selected, 999_999);
    assert!(started.elapsed().as_secs() < 5);
}

#[test]
fn select_all_of_an_empty_result_selects_nothing() {
    let mut fx = Fixture::new();
    fx.query.total = 0;
    fx.send(SelectionInput::Sync(fx.query));
    let key = fx.model.key();
    fx.interact(Interaction::SelectAll);
    assert_eq!((fx.model.count(), fx.model.key()), (0, key));
}

#[test]
fn adds_ctrl_clicks_outside_ranges_to_included_and_re_covers_them_with_a_larger_range() {
    let mut fx = Fixture::new();
    fx.click(1, 0);
    fx.click_with(20_000, 19_999, true, false);
    fx.click_with(30_001, 30_000, false, true);
    fx.click_with(101, 100, false, true);
    assert_eq!(fx.model.count(), 20_000);
    let held = fx.spec().expect("a ranged selection");
    assert_eq!((held.included, held.excluded), (vec![30_001], vec![101]));

    // A Ctrl+Shift range over both re-selects the excluded ID and covers the
    // included one.
    fx.click_with(1, 0, false, true);
    fx.click_with(1, 0, false, true);
    fx.click_with(40_000, 39_999, true, true);
    assert!(fx.read_log.is_empty());
    assert_eq!(fx.spec(), Some(spec(1, &[[0, 39_999]], &[], &[])));
    assert_eq!(fx.model.count(), 40_000);
}

#[test]
fn resolves_small_ranges_to_ids_that_survive_a_new_query_and_clears_large_ones_with_a_notice() {
    let mut fx = Fixture::new();
    fx.click(1, 0);
    fx.click_with(MATERIALIZE_LIMIT as i64, MATERIALIZE_LIMIT - 1, true, false);
    fx.answer();
    assert_eq!(fx.read_log, [(0, MATERIALIZE_LIMIT - 1)]);
    assert!(!fx.model.ranged());
    assert_eq!(fx.ids().len(), MATERIALIZE_LIMIT);
    fx.next_query(Some(2));
    assert_eq!(fx.ids().len(), MATERIALIZE_LIMIT);
    assert_eq!(fx.model.notice(), None);

    // Same filters, same session (a cache hit): the ranges stay valid.
    fx.interact(Interaction::SelectAll);
    fx.next_query(Some(2));
    assert_eq!(fx.model.count(), 1_000_000);
    // A pending query (no session yet) changes nothing until its result arrives.
    fx.next_query(None);
    assert_eq!(fx.model.count(), 1_000_000);
    fx.next_query(Some(3));
    assert_eq!(fx.model.count(), 0);
    assert_eq!(fx.model.notice(), Some(SelectionNotice::RangeCleared));
    fx.click(5, 4);
    assert_eq!(fx.model.notice(), None);
}

#[test]
fn one_position_more_than_the_limit_stays_a_range() {
    let mut fx = Fixture::new();
    fx.commit(&[[0, MATERIALIZE_LIMIT]], false);
    assert!(fx.model.ranged() && fx.read_log.is_empty());
    assert_eq!(fx.model.count(), MATERIALIZE_LIMIT + 1);
}

#[test]
fn keeps_the_identity_of_an_id_selection_when_only_the_query_session_changes() {
    let mut fx = Fixture::new();
    fx.click(7, 6);
    fx.click(9, 8);
    let key = fx.model.key();
    let ids = fx.model.ids();
    fx.next_query(Some(2));
    assert_eq!(fx.model.key(), key);
    assert!(std::sync::Arc::ptr_eq(&fx.model.ids(), &ids));
    assert_eq!(fx.model.count(), 2);
    assert_eq!(fx.model.notice(), None);
}

#[test]
fn finds_ids_kept_from_an_earlier_query_when_a_large_range_covers_them() {
    let mut fx = Fixture::new();
    fx.click(7, 6);
    // In the next session asset 7 sits at index 20 005, which the new range covers.
    fx.next_query(Some(2));
    fx.click(100_000, 0);
    fx.click(100_000, 0);
    fx.click_with(130_000, 30_000, true, true);
    fx.answer_with(|first, last| {
        Ok((first..=last)
            .map(|index| {
                if index == 20_005 {
                    7
                } else {
                    100_000 + index as i64
                }
            })
            .collect())
    });
    assert_eq!(fx.read_log, [(0, 30_000)]);
    assert_eq!(fx.model.count(), 30_001);
    assert_eq!(fx.spec(), Some(spec(2, &[[0, 30_000]], &[], &[])));
}

#[test]
fn an_id_kept_from_an_earlier_query_outside_the_new_range_stays_included() {
    let mut fx = Fixture::new();
    fx.click(7, 6);
    fx.next_query(Some(2));
    fx.commit(&[[0, 30_000]], true);
    fx.answer_with(|first, last| Ok((first..=last).map(|index| 100_000 + index as i64).collect()));
    assert_eq!(fx.model.count(), 30_002);
    assert_eq!(fx.spec(), Some(spec(2, &[[0, 30_000]], &[], &[7])));
    assert!(fx.model.is_selected(999_999, Some(7)));
}

#[test]
fn an_additive_rectangle_over_nothing_keeps_the_selection_and_a_plain_one_replaces_it() {
    let mut fx = Fixture::new();
    // An ID kept from an earlier query (its position is unknown) next to a range.
    fx.click(7, 6);
    fx.next_query(Some(2));
    fx.commit(&[[0, 20_000]], true);
    fx.answer_with(|first, last| Ok((first..=last).map(|index| 100_000 + index as i64).collect()));
    assert_eq!(fx.model.count(), 20_002);
    let key = fx.model.key();

    fx.commit(&[], true);
    assert!(fx.reads.is_empty());
    assert_eq!(fx.model.error(), None);
    assert_eq!(fx.model.count(), 20_002);
    assert_ne!(fx.model.key(), key, "a commit publishes");

    fx.commit(&[], false);
    assert_eq!(fx.model.count(), 0);
    assert!(!fx.model.ranged());
}

#[test]
fn drops_a_settled_failed_retry_on_an_identity_reset() {
    let mut fx = Fixture::new();
    fx.commit(&[[0, 1]], false);
    fx.answer_with(|_, _| Err(QueryError::Backend(BackendError::rejected("failed read"))));
    assert!(fx
        .model
        .error()
        .unwrap()
        .to_string()
        .contains("failed read"));
    fx.reset_identity();
    fx.send(SelectionInput::Retry);
    assert_eq!(fx.read_log.len(), 1);
    assert_eq!(fx.model.error(), None);
    assert!(fx.ids().is_empty());
}

#[test]
fn a_range_without_a_session_fails_with_a_retry_that_works_once_the_session_is_back() {
    let mut fx = Fixture::new();
    fx.click(1, 0);
    fx.next_query(None);
    fx.commit(&[[0, 2]], false);
    assert_eq!(fx.model.error(), Some(&SelectionError::Cancelled));
    assert!(fx.reads.is_empty() && !fx.model.busy());
    assert_eq!(fx.ids(), [1]);
    // The query's result cancels the failed intent with everything else.
    fx.next_query(Some(1));
    assert_eq!(fx.model.error(), None);
    fx.send(SelectionInput::Retry);
    assert!(fx.reads.is_empty());
}

#[test]
fn a_click_without_a_session_selects_the_id_without_a_position() {
    let mut fx = Fixture::new();
    fx.next_query(None);
    fx.click(5, 4);
    assert_eq!(fx.ids(), [5]);
    fx.next_query(Some(2));
    // Its position is unknown, so a large range reads its envelope to find it.
    fx.commit(&[[0, 20_000]], true);
    assert_eq!(fx.read_log, [(0, 20_000)]);
}

#[test]
fn a_rectangle_gesture_makes_the_selection_busy_until_it_ends() {
    let mut fx = Fixture::new();
    fx.click(1, 0);
    fx.interact(Interaction::RectangleStart);
    assert!(fx.model.busy() && fx.model.capture().busy);
    assert_eq!(
        fx.model
            .capture()
            .spec_mutation(SelectionWrite::ToggleFavorites),
        None
    );
    fx.interact(Interaction::RectangleCancel);
    assert!(!fx.model.busy());
    assert_eq!(fx.ids(), [1]);
}

#[test]
fn removing_deleted_ids_unselects_them() {
    let mut fx = Fixture::new();
    fx.click(1, 0);
    fx.click(2, 1);
    fx.click(3, 2);
    let key = fx.model.key();
    fx.send(SelectionInput::Remove(vec![2, 99]));
    assert_eq!(fx.ids(), [1, 3]);
    assert_eq!(fx.model.count(), 2);
    assert_ne!(fx.model.key(), key);
}

#[test]
fn a_range_selection_is_written_through_its_spec() {
    let mut fx = Fixture::new();
    fx.click(3, 2);
    let capture = fx.model.capture();
    assert_eq!(capture.spec, None);
    assert_eq!(capture.spec_mutation(SelectionWrite::ToggleFavorites), None);
    assert_eq!(
        (capture.count, capture.enabled, &*capture.ids),
        (1, true, &[3][..])
    );

    fx.interact(Interaction::SelectAll);
    fx.click_with(3, 2, false, true);
    let capture = fx.model.capture();
    let selection = spec(1, &[[0, 999_999]], &[3], &[]);
    assert_eq!(capture.key, fx.model.key());
    assert_eq!(
        capture.spec_mutation(SelectionWrite::MergeTags(vec!["cat".into()])),
        Some(Mutation::MergeTagsInSelection {
            selection: selection.clone(),
            tags: vec!["cat".into()]
        })
    );
    assert_eq!(
        capture.spec_mutation(SelectionWrite::ToggleFavorites),
        Some(Mutation::ToggleFavoritesInSelection {
            selection: selection.clone()
        })
    );
    assert_eq!(
        capture.spec_mutation(SelectionWrite::SetMediaGroup(Some("trip".into()))),
        Some(Mutation::SetMediaGroupInSelection {
            selection,
            key: Some("trip".into())
        })
    );
}

mod keepalive {
    use super::*;

    #[test]
    fn reads_the_session_at_once_and_then_once_per_interval_only_while_ranges_are_held() {
        let mut fx = Fixture::new();
        // IDs held directly (clicks, small ranges) need no session.
        fx.click(1, 0);
        fx.commit(&[[0, 9]], false);
        fx.answer();
        assert!(fx.timers.is_empty() && fx.keepalive_log.is_empty());

        fx.interact(Interaction::SelectAll);
        assert!(fx.model.ranged());
        // The session may already have been idle for most of its lifetime.
        assert_eq!(fx.keepalive_log, [1]);
        assert_eq!(fx.model.keepalive_session(), Some(1));
        fx.keepalive_answer(Ok(KeepAlive::Ready));
        // Idle for longer than the backend's five-minute session lifetime.
        for _ in 0..6 {
            fx.tick();
            fx.keepalive_answer(Ok(KeepAlive::Ready));
        }
        assert_eq!(fx.keepalive_log, [1; 7]);
        assert_eq!(fx.timers.len(), 1, "one timer at a time");

        // Edits inside the ranges keep the same timer and read nothing extra.
        fx.click(5, 4);
        fx.click_with(200_000, 199_999, true, true);
        assert_eq!(fx.keepalive_log.len(), 7);
        fx.tick();
        assert_eq!(fx.keepalive_log.len(), 8);
        assert_eq!(fx.spec().map(|spec| spec.session_id), Some(1));

        fx.send(SelectionInput::Clear);
        assert_eq!(fx.model.keepalive_session(), None);
        for _ in 0..5 {
            fx.tick();
        }
        assert_eq!(fx.keepalive_log.len(), 8);
        assert!(
            fx.timers.is_empty(),
            "the timer of dropped ranges is not armed again"
        );
    }

    #[test]
    fn stops_when_the_session_changes_selection_mode_ends_or_the_owner_goes_away() {
        let mut fx = Fixture::new();
        fx.interact(Interaction::SelectAll);
        assert_eq!(fx.keepalive_log, [1]);
        fx.next_query(Some(2));
        assert!(!fx.model.ranged());
        assert_eq!(fx.model.notice(), Some(SelectionNotice::RangeCleared));
        fx.tick();
        fx.tick();
        assert_eq!(fx.keepalive_log, [1]);

        fx.interact(Interaction::SelectAll);
        fx.keepalive_answer(Ok(KeepAlive::Skipped));
        fx.keepalive_answer(Ok(KeepAlive::Ready));
        fx.tick();
        assert_eq!(fx.keepalive_log, [1, 2, 2]);
        fx.send(SelectionInput::ToggleMode);
        fx.tick();
        fx.tick();
        assert_eq!(fx.keepalive_log.len(), 3);

        fx.send(SelectionInput::ToggleMode);
        fx.interact(Interaction::SelectAll);
        assert_eq!(fx.keepalive_log.len(), 4);
        fx.send(SelectionInput::Dispose);
        fx.tick();
        fx.tick();
        assert_eq!(fx.keepalive_log.len(), 4);
        assert!(fx.timers.is_empty());
    }

    #[test]
    fn keeps_the_timer_when_a_refresh_returns_the_same_cached_session() {
        let mut fx = Fixture::new();
        fx.interact(Interaction::SelectAll);
        fx.keepalive_answer(Ok(KeepAlive::Ready));
        fx.next_query(None);
        fx.next_query(Some(1));
        assert!(fx.model.ranged());
        fx.tick();
        assert_eq!(fx.keepalive_log, [1, 1]);
        assert_eq!(fx.timers.len(), 1);
    }

    #[test]
    fn never_overlaps_reads_and_retries_after_a_failed_one() {
        let mut fx = Fixture::new();
        fx.interact(Interaction::SelectAll);
        // The immediate read is still on its way through three ticks.
        for _ in 0..3 {
            fx.tick();
        }
        assert_eq!(fx.keepalive_log.len(), 1);
        fx.keepalive_answer(Ok(KeepAlive::Ready));
        fx.tick();
        fx.keepalive_answer(Err(QueryError::Backend(BackendError::rejected("busy"))));
        fx.tick();
        assert_eq!(fx.keepalive_log.len(), 3);
        assert_eq!(fx.model.error(), None);
        assert!(fx.model.ranged());
    }

    #[test]
    fn drops_ranges_with_the_expired_notice_when_the_read_made_at_selection_is_stale() {
        let mut fx = Fixture::new();
        fx.interact(Interaction::SelectAll);
        fx.keepalive_answer(Ok(KeepAlive::Stale));
        assert_eq!(fx.keepalive_log, [1]);
        assert!(!fx.model.ranged());
        assert_eq!(fx.model.count(), 0);
        assert_eq!(fx.spec(), None);
        assert_eq!(fx.model.notice(), Some(SelectionNotice::RangeExpired));
        // Nothing is held any more, so nothing keeps reading the dead session.
        for _ in 0..3 {
            fx.tick();
        }
        assert_eq!(fx.keepalive_log, [1]);
        assert!(fx.timers.is_empty());
    }

    #[test]
    fn leaves_a_later_stale_read_to_the_refresh_which_reports_a_changed_result() {
        let mut fx = Fixture::new();
        fx.interact(Interaction::SelectAll);
        fx.keepalive_answer(Ok(KeepAlive::Ready));
        fx.tick();
        fx.keepalive_answer(Ok(KeepAlive::Stale));
        assert_eq!(fx.keepalive_log.len(), 2);
        assert!(fx.model.ranged());
        assert_eq!(fx.model.notice(), None);
        fx.next_query(None);
        fx.next_query(Some(2));
        assert!(!fx.model.ranged());
        assert_eq!(fx.model.notice(), Some(SelectionNotice::RangeCleared));
    }

    #[test]
    fn ignores_a_stale_reply_that_arrives_after_the_ranges_were_replaced() {
        let mut fx = Fixture::new();
        fx.interact(Interaction::SelectAll);
        fx.send(SelectionInput::Clear);
        fx.interact(Interaction::SelectAll);
        assert_eq!(fx.keepalive_log.len(), 2);
        // The first read answers late.
        fx.keepalive_answer(Ok(KeepAlive::Stale));
        assert!(fx.model.ranged());
        assert_eq!(fx.model.notice(), None);
    }

    #[test]
    fn drops_ranges_with_the_expired_notice_when_the_refresh_fails() {
        let mut fx = Fixture::new();
        fx.interact(Interaction::SelectAll);
        fx.keepalive_answer(Ok(KeepAlive::Ready));
        // A failed page read keeps the session, so the ranges stay.
        fx.fail_read();
        assert!(fx.model.ranged());
        assert_eq!(fx.model.notice(), None);
        // The refresh starts (no session yet) and then fails.
        fx.next_query(None);
        assert!(fx.model.ranged());
        fx.fail_read();
        assert!(!fx.model.ranged());
        assert_eq!(fx.model.count(), 0);
        assert_eq!(fx.spec(), None);
        assert_eq!(fx.model.notice(), Some(SelectionNotice::RangeExpired));
        let reads = fx.keepalive_log.len();
        for _ in 0..6 {
            fx.tick();
        }
        assert_eq!(fx.keepalive_log.len(), reads);
        assert!(fx.timers.is_empty());
        // The retry's new session finds nothing left to clear and keeps the notice.
        fx.next_query(Some(2));
        assert_eq!(fx.model.notice(), Some(SelectionNotice::RangeExpired));
    }

    #[test]
    fn keeps_ids_held_directly_when_the_refresh_fails() {
        let mut fx = Fixture::new();
        fx.click(1, 0);
        fx.click(5, 4);
        fx.next_query(None);
        fx.fail_read();
        assert_eq!(fx.model.count(), 2);
        assert_eq!(fx.model.notice(), None);
    }
}
