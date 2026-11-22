//! The bookkeeping of the lightbox's writes, without I/O: the save intent of a tag
//! editor (`TagIntent`, `rebase` of `createLightbox.ts`) and the pending favorite and group
//! writes per asset (`createPendingWrites`).

use std::collections::HashMap;

use crate::library::normalize_tags;

/// Where a write of the lightbox stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WriteState {
    #[default]
    Idle,
    /// Queued or sent; the backend has not answered.
    Pending,
    /// The last write failed; Retry is offered.
    Failed,
}

/// `list` as an edit of `confirmed`, applied to `canonical`: the canonical tags without
/// the ones the edit removed, then the ones it added (`rebase`).
pub fn rebased(list: &[String], confirmed: &[String], canonical: &[String]) -> Vec<String> {
    let removed = |tag: &String| confirmed.contains(tag) && !list.contains(tag);
    let mut next: Vec<String> = canonical
        .iter()
        .filter(|tag| !removed(tag))
        .cloned()
        .collect();
    next.extend(list.iter().filter(|tag| !confirmed.contains(tag)).cloned());
    normalize_tags(next)
}

/// What the tag editor of one asset wants saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TagIntent {
    /// Tells this intent from a later one of the same asset (a reply of a write that
    /// an identity reset or a new intent overtook is dropped).
    pub serial: u64,
    /// The canonical tags the edit was made on.
    pub confirmed: Vec<String>,
    /// The list the editor shows and wants saved; `None` once it is saved.
    pub desired: Option<Vec<String>>,
    pub state: WriteState,
    /// The write that is on its way.
    flight: Option<Flight>,
}

/// A tag write that was handed to the library and is not answered yet.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Flight {
    /// The canonical tags and the wanted list when the write was handed over: the
    /// library applies the difference of the two to the tags of the moment it sends.
    base: Vec<String>,
    wanted: Vec<String>,
    /// `wanted` over the canonical tags as they are now: what the backend is asked
    /// to store.
    sent: Vec<String>,
}

impl TagIntent {
    pub fn new(serial: u64, canonical: &[String]) -> Self {
        Self {
            serial,
            confirmed: canonical.to_vec(),
            desired: None,
            state: WriteState::Idle,
            flight: None,
        }
    }

    /// `rebase`: the canonical tags changed under an intent that is not being
    /// written.
    pub fn rebase(&mut self, canonical: &[String]) {
        if let Some(desired) = &self.desired {
            self.desired = Some(rebased(desired, &self.confirmed, canonical));
        }
        self.confirmed = canonical.to_vec();
    }

    /// The tags to add to and to remove from the canonical list, and the intent
    /// becomes pending. `None` without something to save.
    pub fn start(&mut self) -> Option<(Vec<String>, Vec<String>)> {
        let desired = self.desired.clone()?;
        let add = desired
            .iter()
            .filter(|tag| !self.confirmed.contains(tag))
            .cloned()
            .collect();
        let remove = self
            .confirmed
            .iter()
            .filter(|tag| !desired.contains(tag))
            .cloned()
            .collect();
        self.flight = Some(Flight {
            base: self.confirmed.clone(),
            wanted: desired.clone(),
            sent: desired,
        });
        self.state = WriteState::Pending;
        Some((add, remove))
    }

    /// The canonical tags changed while the write waits behind another writer: the
    /// library will apply this write's difference to them, and the editor shows the
    /// same merge (the callback of `mutations.saveTags` in `drain`). An edit made
    /// while the write waited is merged as well and saved by the next write.
    pub fn rebase_pending(&mut self, canonical: &[String]) {
        let Some(flight) = &mut self.flight else {
            return self.rebase(canonical);
        };
        let sent = rebased(&flight.wanted, &flight.base, canonical);
        self.desired = match self.desired.take() {
            Some(desired) if desired == flight.sent => Some(sent.clone()),
            Some(desired) => Some(rebased(&desired, &self.confirmed, canonical)),
            None => None,
        };
        flight.sent = sent;
        self.confirmed = canonical.to_vec();
    }

    /// The backend stored `canonical`: the intent is saved unless the editor moved on
    /// while the write ran.
    pub fn saved(&mut self, canonical: Vec<String>) {
        let sent = self.flight.take().map(|flight| flight.sent);
        self.confirmed = canonical;
        if self.desired.is_some() && self.desired == sent {
            self.desired = None;
        }
        self.state = WriteState::Idle;
    }

    /// The write was refused or failed: the wanted list stays for Retry.
    pub fn failed(&mut self) {
        self.flight = None;
        self.state = WriteState::Failed;
    }
}

/// Pending favorite or group writes per asset and identity epoch; writes queue, they
/// never reject (`createPendingWrites`).
#[derive(Debug)]
pub(super) struct PendingWrites<T> {
    entries: HashMap<i64, Entry<T>>,
}

#[derive(Debug)]
struct Entry<T> {
    epoch: u64,
    count: usize,
    failed: bool,
    value: Option<T>,
}

impl<T> Default for PendingWrites<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T> PendingWrites<T> {
    pub fn has(&self, asset_id: i64, epoch: u64) -> bool {
        self.entries
            .get(&asset_id)
            .is_some_and(|entry| entry.epoch == epoch)
    }

    /// The value the newest pending write asked for, used to chain queued toggles.
    pub fn latest(&self, asset_id: i64, epoch: u64) -> Option<&T> {
        self.entries
            .get(&asset_id)
            .filter(|entry| entry.epoch == epoch)
            .and_then(|entry| entry.value.as_ref())
    }

    pub fn start(&mut self, asset_id: i64, epoch: u64, value: Option<T>) {
        match self.entries.get_mut(&asset_id) {
            Some(entry) if entry.epoch == epoch => {
                entry.count += 1;
                entry.value = value;
            }
            _ => {
                self.entries.insert(
                    asset_id,
                    Entry {
                        epoch,
                        count: 1,
                        failed: false,
                        value,
                    },
                );
            }
        }
    }

    /// The settled state after the asset's last pending write, otherwise `None`. A
    /// write of another identity epoch settles on its own and releases nothing.
    pub fn finish(&mut self, asset_id: i64, epoch: u64, failed: bool) -> Option<WriteState> {
        let settled = |failed| {
            Some(if failed {
                WriteState::Failed
            } else {
                WriteState::Idle
            })
        };
        let Some(entry) = self
            .entries
            .get_mut(&asset_id)
            .filter(|entry| entry.epoch == epoch)
        else {
            return settled(failed);
        };
        entry.failed |= failed;
        entry.count = entry.count.saturating_sub(1);
        if entry.count > 0 {
            return None;
        }
        let failed = entry.failed;
        self.entries.remove(&asset_id);
        settled(failed)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{rebased, PendingWrites, TagIntent, WriteState};

    fn list(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| (*tag).to_owned()).collect()
    }

    #[test]
    fn a_rebase_keeps_what_the_edit_added_and_removed() {
        // "queues behind an external writer and rebases its intent onto the winner".
        assert_eq!(
            rebased(
                &list(&["mine"]),
                &list(&["cat"]),
                &list(&["cat", "external"])
            ),
            ["external", "mine"]
        );
        // Nothing is added twice, and a tag the edit removed stays removed.
        assert_eq!(
            rebased(
                &list(&["cat", "a", "b"]),
                &list(&["cat", "a"]),
                &list(&["a", "cat", "dog"])
            ),
            ["a", "cat", "dog", "b"]
        );
        assert_eq!(
            rebased(
                &list(&["dog"]),
                &list(&["cat", "dog"]),
                &list(&["cat", "dog", "fox"])
            ),
            ["dog", "fox"]
        );
    }

    #[test]
    fn an_intent_is_saved_when_the_editor_did_not_move_on() {
        let mut intent = TagIntent::new(1, &list(&["cat"]));
        intent.desired = Some(list(&["cat", "a"]));
        assert_eq!(intent.start(), Some((list(&["a"]), Vec::new())));
        assert_eq!(intent.state, WriteState::Pending);

        intent.saved(list(&["a", "cat"]));
        assert_eq!(intent.desired, None);
        assert_eq!(intent.confirmed, ["a", "cat"]);
        assert_eq!(intent.state, WriteState::Idle);
    }

    #[test]
    fn an_edit_made_while_the_write_ran_stays_wanted() {
        let mut intent = TagIntent::new(1, &list(&["cat"]));
        intent.desired = Some(list(&["cat", "a"]));
        intent.start();
        intent.desired = Some(list(&["cat", "final"]));

        intent.saved(list(&["cat", "a"]));
        assert_eq!(intent.desired, Some(list(&["cat", "final"])));
        assert_eq!(
            intent.start(),
            Some((list(&["final"]), list(&["a"]))),
            "the next write removes what the first one added"
        );
    }

    #[test]
    fn a_waiting_write_is_merged_onto_the_tags_of_the_writer_ahead() {
        let mut intent = TagIntent::new(1, &list(&["cat"]));
        intent.desired = Some(list(&["mine"]));
        assert_eq!(intent.start(), Some((list(&["mine"]), list(&["cat"]))));

        intent.rebase_pending(&list(&["cat", "external"]));
        assert_eq!(intent.desired, Some(list(&["external", "mine"])));
        assert_eq!(intent.state, WriteState::Pending);

        // Two writers ahead: merged from the tags the edit was made on, as the
        // library does when it sends.
        intent.rebase_pending(&list(&["cat", "other"]));
        assert_eq!(intent.desired, Some(list(&["other", "mine"])));

        intent.saved(list(&["mine", "other"]));
        assert_eq!(intent.desired, None);
    }

    #[test]
    fn a_failed_write_keeps_the_wanted_list_for_retry() {
        let mut intent = TagIntent::new(1, &list(&["cat"]));
        intent.desired = Some(list(&["b"]));
        intent.start();
        intent.failed();
        assert_eq!(intent.state, WriteState::Failed);
        assert_eq!(intent.desired, Some(list(&["b"])));

        intent.rebase(&list(&["cat", "dog"]));
        assert_eq!(intent.desired, Some(list(&["dog", "b"])));
    }

    #[test]
    fn pending_writes_count_per_asset_and_identity() {
        let mut writes = PendingWrites::<bool>::default();
        assert!(!writes.has(1, 0));
        writes.start(1, 0, Some(true));
        writes.start(1, 0, Some(false));
        assert!(writes.has(1, 0));
        assert_eq!(writes.latest(1, 0), Some(&false));
        assert!(!writes.has(1, 1), "another identity epoch");

        assert_eq!(writes.finish(1, 0, true), None);
        assert_eq!(writes.finish(1, 0, false), Some(WriteState::Failed));
        assert!(!writes.has(1, 0));
        assert_eq!(writes.len(), 0);
    }

    #[test]
    fn a_write_of_an_older_identity_releases_nothing() {
        let mut writes = PendingWrites::<bool>::default();
        writes.start(1, 0, Some(true));
        // The ID was reused in a new epoch and written again.
        writes.start(1, 1, Some(true));

        assert_eq!(writes.finish(1, 0, true), Some(WriteState::Failed));
        assert!(writes.has(1, 1));
        assert_eq!(writes.finish(1, 1, false), Some(WriteState::Idle));
    }
}
