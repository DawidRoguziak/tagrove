//! The tag state on its own (the cases of `createAssetTagState.test.ts` that need no
//! backend).

use super::{AssetTagState, TagStateError, TagToken, TurnId, MAX_INACTIVE_RECORDS};

fn list(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_owned()).collect()
}

/// Admits a write on idle assets and starts its tokens.
fn begin(state: &mut AssetTagState, ids: &[i64]) -> (TurnId, Vec<TagToken>) {
    let admission = state.enqueue_mutation(ids).expect("admitted");
    assert!(admission.ready, "the assets are idle");
    let tokens = state.begin(admission.turn).expect("started");
    (admission.turn, tokens)
}

fn known(state: &AssetTagState, asset_id: i64) -> Option<Vec<String>> {
    state.peek(asset_id).map(|known| known.tags.to_vec())
}

#[test]
fn details_are_blocked_while_a_write_is_pending_and_its_canonical_result_is_published() {
    let mut state = AssetTagState::new();
    let initial = state.capture_generation(7);
    assert!(state.publish_details(7, list(&["cat"]), &initial));
    let pending_read = state.capture_generation(7);
    let (turn, tokens) = begin(&mut state, &[7]);

    assert!(!state.publish_details(7, list(&["stale"]), &pending_read));
    assert_eq!(known(&state, 7), Some(list(&["cat"])));

    assert!(state.settle_mutation(&tokens[0], Some(list(&[" Cat ", "dog", "cat"]))));
    assert!(state.release(turn).is_empty());
    assert_eq!(known(&state, 7), Some(list(&["cat", "dog"])));
    assert!(state.is_idle());
}

#[test]
fn writes_on_one_asset_run_in_admission_order_and_a_failed_write_releases_the_next() {
    let mut state = AssetTagState::new();
    let (first, first_tokens) = begin(&mut state, &[5]);
    let second = state.enqueue_mutation(&[5]).unwrap();
    let third = state.enqueue_mutation(&[5]).unwrap();
    assert!(!second.ready && !third.ready);
    assert_eq!(state.begin(second.turn), Err(TagStateError::OutOfOrder));

    // The first write fails: it settles without tags and releases its turn.
    assert!(state.settle_mutation(&first_tokens[0], None));
    assert_eq!(state.release(first), [second.turn]);
    assert!(state.is_ready(second.turn) && !state.is_ready(third.turn));

    let second_tokens = state.begin(second.turn).unwrap();
    assert_eq!(state.release(second.turn), [third.turn]);
    assert!(state.settle_mutation(&second_tokens[0], None));
    assert!(state.release(third.turn).is_empty());
    assert!(state.is_idle());
}

#[test]
fn overlapping_writes_on_several_assets_follow_admission_order_without_deadlock() {
    let mut state = AssetTagState::new();
    let (a, _) = begin(&mut state, &[2, 1]);
    let b = state.enqueue_mutation(&[3, 2]).unwrap();
    let c = state.enqueue_mutation(&[1, 3]).unwrap();
    let d = state.enqueue_mutation(&[4]).unwrap();
    assert!(!b.ready && !c.ready);
    assert!(d.ready, "an unrelated asset does not wait");

    // c waits for a and for b.
    assert_eq!(state.release(a), [b.turn]);
    assert_eq!(state.release(b.turn), [c.turn]);
    assert!(state.release(c.turn).is_empty());
    assert!(state.release(d.turn).is_empty());
    assert!(state.is_idle());
}

#[test]
fn a_read_started_during_a_write_is_obsolete_also_when_the_write_failed() {
    let mut state = AssetTagState::new();
    let (turn, tokens) = begin(&mut state, &[3]);
    let during = state.capture_generation(3);
    assert!(state.settle_mutation(&tokens[0], None));
    state.release(turn);

    assert!(!state.publish_details(3, list(&["possibly-stale"]), &during));
    assert_eq!(known(&state, 3), None);
}

#[test]
fn a_barrier_refuses_new_writes_at_once_and_waits_for_queued_and_running_ones() {
    let mut state = AssetTagState::new();
    let (active, active_tokens) = begin(&mut state, &[1]);
    let queued = state.enqueue_mutation(&[1]).unwrap();

    let barrier = state.request_barrier().unwrap();
    assert_eq!(state.poll_barrier(), None);
    assert_eq!(
        state.enqueue_mutation(&[2]),
        Err(TagStateError::Maintenance)
    );
    assert_eq!(
        state.enqueue_library_mutation(),
        Err(TagStateError::Maintenance)
    );

    assert!(state.settle_mutation(&active_tokens[0], Some(list(&["saved"]))));
    assert_eq!(state.release(active), [queued.turn]);
    // A write admitted before the barrier still runs before maintenance.
    assert_eq!(state.poll_barrier(), None);
    let queued_tokens = state.begin(queued.turn).unwrap();
    assert!(state.settle_mutation(&queued_tokens[0], None));
    state.release(queued.turn);

    assert_eq!(state.poll_barrier(), Some(barrier));
    assert_eq!(state.poll_barrier(), None, "a barrier is handed out once");
    assert_eq!(
        state.enqueue_mutation(&[2]),
        Err(TagStateError::Maintenance)
    );
    assert!(state.release_barrier(barrier));
    let (after, tokens) = begin(&mut state, &[2]);
    assert_eq!(tokens.len(), 1);
    state.release(after);
}

#[test]
fn barriers_run_one_after_the_other() {
    let mut state = AssetTagState::new();
    let first = state.request_barrier().unwrap();
    let second = state.request_barrier().unwrap();

    assert_eq!(state.poll_barrier(), Some(first));
    assert_eq!(state.poll_barrier(), None);
    assert_eq!(
        state.enqueue_mutation(&[8]),
        Err(TagStateError::Maintenance)
    );
    // The first operation failed; its barrier comes down all the same.
    assert!(state.release_barrier(first));
    assert_eq!(state.poll_barrier(), Some(second));
    assert!(state.release_barrier(second));
    assert!(!state.release_barrier(second));
    assert!(!state.barrier_active());
    assert!(state.enqueue_mutation(&[8]).unwrap().ready);
}

#[test]
fn writes_admitted_before_a_reset_still_count_and_cannot_start_late() {
    let mut state = AssetTagState::new();
    let (active, active_tokens) = begin(&mut state, &[4]);
    let queued = state.enqueue_mutation(&[4]).unwrap();
    let barrier = state.request_barrier().unwrap();

    state.reset();
    assert_eq!(state.poll_barrier(), None, "the old writes are still out");

    assert!(!state.settle_mutation(&active_tokens[0], Some(list(&["stale-result"]))));
    assert_eq!(state.release(active), [queued.turn]);
    assert_eq!(state.begin(queued.turn), Err(TagStateError::LibraryChanged));
    state.release(queued.turn);
    assert_eq!(state.poll_barrier(), Some(barrier));
    assert_eq!(known(&state, 4), None);
}

#[test]
fn a_reset_starts_empty_queues_for_the_new_identity() {
    let mut state = AssetTagState::new();
    let (old, _) = begin(&mut state, &[4]);
    state.reset();

    let fresh = state.enqueue_mutation(&[4]).unwrap();
    assert!(
        fresh.ready,
        "the new identity does not wait for the old write"
    );
    assert_eq!(state.begin(fresh.turn).unwrap().len(), 1);
    state.release(old);
    state.release(fresh.turn);
}

#[test]
fn a_queued_write_for_an_asset_deleted_meanwhile_is_refused() {
    let mut state = AssetTagState::new();
    let (active, tokens) = begin(&mut state, &[6]);
    let queued = state.enqueue_mutation(&[6]).unwrap();
    state.settle_mutation(&tokens[0], None);
    assert!(state.remove(6, None));
    state.release(active);

    assert_eq!(state.begin(queued.turn), Err(TagStateError::AssetDeleted));
    assert_eq!(state.begin(queued.turn), Err(TagStateError::TurnUsed));
    state.release(queued.turn);
    assert!(state.is_idle());
}

#[test]
fn disposal_rejects_waiting_barriers_queued_writes_and_new_admissions() {
    let mut state = AssetTagState::new();
    let (active, _) = begin(&mut state, &[12]);
    let queued = state.enqueue_mutation(&[12]).unwrap();
    let barrier = state.request_barrier().unwrap();

    assert_eq!(state.dispose(), [barrier]);
    assert!(!state.barrier_active());

    state.release(active);
    assert_eq!(state.begin(queued.turn), Err(TagStateError::Unavailable));
    assert_eq!(
        state.enqueue_mutation(&[13]),
        Err(TagStateError::Unavailable)
    );
    assert_eq!(state.request_barrier(), Err(TagStateError::Unavailable));
}

#[test]
fn a_reset_rejects_old_reads_and_lets_a_reused_id_load_fresh_details() {
    let mut state = AssetTagState::new();
    let old = state.capture_generation(9);
    let epoch = state.epoch();
    state.reset();
    assert_eq!(state.epoch(), epoch + 1);
    let fresh = state.capture_generation(9);

    assert!(!state.publish_details(9, list(&["old-identity"]), &old));
    assert!(state.publish_details(9, list(&["new-identity"]), &fresh));
    assert_eq!(known(&state, 9), Some(list(&["new-identity"])));
    assert!(!state.remove(9, Some(&old)));
    assert_eq!(known(&state, 9), Some(list(&["new-identity"])));
}

#[test]
fn a_deletion_tombstone_rejects_pending_and_later_details() {
    let mut state = AssetTagState::new();
    let pending = state.capture_generation(11);
    let revision = state.revision();
    assert!(state.remove(11, None));
    assert!(state.revision() > revision, "consumers are told");

    assert!(!state.publish_details(11, list(&["deleted"]), &pending));
    let after = state.capture_generation(11);
    assert!(!state.is_current(&after));
    assert!(!state.publish_details(11, list(&["also-deleted"]), &after));
    assert_eq!(known(&state, 11), None);
}

#[test]
fn inactive_records_are_bounded_while_pinned_and_held_ones_stay() {
    let mut state = AssetTagState::new();
    state.pin(1);
    let pinned = state.capture_generation(1);
    let stale = state.capture_generation(2);
    let (turn, tokens) = begin(&mut state, &[3]);
    state.publish_details(1, list(&["pinned"]), &pinned);
    state.publish_details(2, list(&["old"]), &stale);
    for id in 4..519 {
        let token = state.capture_generation(id);
        state.publish_details(id, list(&["cached"]), &token);
    }

    assert_eq!(known(&state, 1), Some(list(&["pinned"])));
    assert_eq!(known(&state, 2), None, "the oldest unpinned record went");
    assert!(!state.is_current(&stale));
    assert!(!state.publish_details(2, list(&["stale"]), &stale));

    assert!(state.settle_mutation(&tokens[0], Some(list(&["saved"]))));
    state.unpin(1);
    state.release(turn);
    let retained = (1..=518).filter(|id| state.peek(*id).is_some()).count();
    assert!(retained <= MAX_INACTIVE_RECORDS, "{retained}");
    assert_eq!(known(&state, 3), Some(list(&["saved"])));
}

#[test]
fn every_asset_of_a_large_write_stays_held_while_the_inactive_list_prunes() {
    let mut state = AssetTagState::new();
    let ids: Vec<i64> = (1..=1200).collect();
    let (turn, tokens) = begin(&mut state, &ids);
    assert_eq!(tokens.len(), 1200);
    for token in &tokens {
        assert!(state.settle_mutation(token, Some(list(&["bulk"]))));
    }
    state.release(turn);
    assert_eq!(known(&state, 1200), Some(list(&["bulk"])));
}

#[test]
fn a_selection_wide_write_waits_for_earlier_writes_and_holds_back_later_ones() {
    let mut state = AssetTagState::new();
    let read = state.capture_generation(4);
    assert!(state.publish_details(4, list(&["cat"]), &read));
    let (first, first_tokens) = begin(&mut state, &[4]);
    let library = state.enqueue_library_mutation().unwrap();
    // A later write on an unrelated asset still waits for the library turn.
    let later = state.enqueue_mutation(&[9]).unwrap();
    assert!(!library.ready && !later.ready);

    state.settle_mutation(&first_tokens[0], Some(list(&["cat", "dog"])));
    assert_eq!(state.release(first), [library.turn]);

    // A read started before the write cannot publish; one started during it is
    // dropped when the write ends.
    let before = state.capture_generation(4);
    assert_eq!(state.begin(library.turn), Ok(Vec::new()));
    assert_eq!(known(&state, 4), None);
    let during = state.capture_generation(4);
    assert!(!state.publish_details(4, list(&["stale"]), &before));
    assert!(state.publish_details(4, list(&["stale"]), &during));
    assert_eq!(state.release(library.turn), [later.turn]);
    assert_eq!(known(&state, 4), None);

    assert_eq!(state.begin(later.turn).unwrap().len(), 1);
    state.release(later.turn);
    // With everything settled, new writes are ready at once again.
    assert!(state.enqueue_mutation(&[9]).unwrap().ready);
}

#[test]
fn a_selection_wide_write_is_refused_after_an_identity_reset() {
    let mut state = AssetTagState::new();
    let (blocker, _) = begin(&mut state, &[1]);
    let queued = state.enqueue_library_mutation().unwrap();
    state.reset();
    assert_eq!(state.release(blocker), [queued.turn]);
    assert_eq!(state.begin(queued.turn), Err(TagStateError::LibraryChanged));
    state.release(queued.turn);
    assert!(state.is_idle());
}

#[test]
fn duplicate_ids_of_a_write_count_once() {
    let mut state = AssetTagState::new();
    let (turn, tokens) = begin(&mut state, &[7, 7, 8, 7]);
    assert_eq!(
        tokens.iter().map(TagToken::asset_id).collect::<Vec<_>>(),
        [7, 8]
    );
    state.release(turn);
    assert!(
        state.release(turn).is_empty(),
        "releasing twice does nothing"
    );
}

#[test]
fn an_unchanged_detail_read_does_not_move_the_revision() {
    let mut state = AssetTagState::new();
    let token = state.capture_generation(1);
    assert!(state.publish_details(1, list(&["cat"]), &token));
    let revision = state.revision();
    assert!(state.publish_details(1, list(&["Cat "]), &token));
    assert_eq!(state.revision(), revision);
    assert!(state.publish_details(1, list(&["dog"]), &token));
    assert_eq!(state.revision(), revision + 1);
}
