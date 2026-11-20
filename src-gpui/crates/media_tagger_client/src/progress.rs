//! The progress stream: core progress → channel → model inputs.
//!
//! The core reports the progress of long operations through its `ProgressSink`
//! port. In the Tauri adapter that is the shared `process-progress` event; here it
//! is a [`ProgressHub`] owned by the backend. The contract is the same
//! (`docs/architecture/ipc-contract.md`): one shared, best-effort, uncorrelated
//! stream; consumers filter by `phase`; progress never proves that an operation
//! finished, only the result of the call does.
//!
//! A model subscribes before it starts the call ([`forward`]), so no event of the
//! operation can be published before the listener exists. Listeners run on the
//! thread that reports the progress, inside the running use case, so they must not
//! block: [`forward`] only queues an input for the model.

use std::sync::{Arc, Mutex, MutexGuard, Weak};

use media_tagger_core::models::ScanProgress;

use crate::{backend::Backend, model::Sender};

/// Called with every progress event, on the reporting thread.
pub type ProgressListener = Arc<dyn Fn(&ScanProgress) + Send + Sync>;

#[derive(Clone, Default)]
pub struct ProgressHub {
    listeners: Arc<Mutex<Listeners>>,
}

#[derive(Default)]
struct Listeners {
    next_id: u64,
    entries: Vec<(u64, ProgressListener)>,
}

fn lock(listeners: &Mutex<Listeners>) -> MutexGuard<'_, Listeners> {
    listeners
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl ProgressHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `listener` for every event published from now on, until the
    /// returned subscription is dropped.
    pub fn subscribe(&self, listener: ProgressListener) -> ProgressSubscription {
        let mut listeners = lock(&self.listeners);
        let id = listeners.next_id;
        listeners.next_id += 1;
        listeners.entries.push((id, listener));
        ProgressSubscription {
            listeners: Arc::downgrade(&self.listeners),
            id,
        }
    }

    /// Hands `progress` to every listener, in subscription order. Listeners run
    /// outside the hub's lock, so one may subscribe or unsubscribe meanwhile.
    pub fn publish(&self, progress: &ScanProgress) {
        let current: Vec<ProgressListener> = lock(&self.listeners)
            .entries
            .iter()
            .map(|(_, listener)| Arc::clone(listener))
            .collect();
        for listener in current {
            listener(progress);
        }
    }

    pub fn listener_count(&self) -> usize {
        lock(&self.listeners).entries.len()
    }
}

/// Keeps a progress listener registered; dropping it unsubscribes. An event that
/// is being delivered while the subscription is dropped may still arrive once.
#[must_use = "dropping the subscription stops the progress events"]
pub struct ProgressSubscription {
    listeners: Weak<Mutex<Listeners>>,
    id: u64,
}

impl Drop for ProgressSubscription {
    fn drop(&mut self) {
        if let Some(listeners) = self.listeners.upgrade() {
            lock(&listeners).entries.retain(|(id, _)| *id != self.id);
        }
    }
}

/// Subscribes a model to the backend's progress: every event becomes the input
/// `input(progress)` in the model's inbox. Call it before the backend call whose
/// progress the model wants, and keep the subscription until the call's result
/// has been applied.
pub fn forward<I: Send + 'static>(
    backend: &dyn Backend,
    sender: Sender<I>,
    input: impl Fn(ScanProgress) -> I + Send + Sync + 'static,
) -> ProgressSubscription {
    backend.subscribe_progress(Arc::new(move |progress: &ScanProgress| {
        sender.send(input(progress.clone()));
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use media_tagger_core::models::ScanProgress;

    use super::{forward, ProgressHub, ProgressListener};
    use crate::{backend::FakeBackend, model::Inbox};

    fn progress(phase: &str, processed: usize) -> ScanProgress {
        ScanProgress {
            phase: phase.to_owned(),
            processed,
            total: 10,
            message: String::new(),
        }
    }

    type Seen = Arc<Mutex<Vec<(String, usize)>>>;

    fn recording() -> (Seen, ProgressListener) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let listener: ProgressListener = Arc::new(move |event: &ScanProgress| {
            sink.lock()
                .unwrap()
                .push((event.phase.clone(), event.processed));
        });
        (seen, listener)
    }

    #[test]
    fn every_listener_gets_every_event_in_order() {
        let hub = ProgressHub::new();
        let (first, first_listener) = recording();
        let (second, second_listener) = recording();
        let _first = hub.subscribe(first_listener);
        let _second = hub.subscribe(second_listener);

        hub.publish(&progress("counting", 1));
        hub.publish(&progress("scanning", 2));

        let expected = [("counting".to_owned(), 1), ("scanning".to_owned(), 2)];
        assert_eq!(*first.lock().unwrap(), expected);
        assert_eq!(*second.lock().unwrap(), expected);
    }

    #[test]
    fn events_before_the_subscription_and_after_its_drop_are_not_delivered() {
        let hub = ProgressHub::new();
        let (seen, listener) = recording();

        hub.publish(&progress("counting", 1));
        let subscription = hub.subscribe(listener);
        hub.publish(&progress("scanning", 2));
        assert_eq!(hub.listener_count(), 1);
        drop(subscription);
        hub.publish(&progress("cleanup", 3));

        assert_eq!(*seen.lock().unwrap(), [("scanning".to_owned(), 2)]);
        assert_eq!(hub.listener_count(), 0);
    }

    #[test]
    fn a_subscription_may_outlive_its_hub() {
        let hub = ProgressHub::new();
        let (_, listener) = recording();
        let subscription = hub.subscribe(listener);

        drop(hub);
        drop(subscription);
    }

    #[test]
    fn a_listener_may_unsubscribe_another_while_an_event_is_delivered() {
        let hub = ProgressHub::new();
        let (seen, listener) = recording();
        let victim = Arc::new(Mutex::new(Some(hub.subscribe(listener))));
        let slot = Arc::clone(&victim);
        let _dropper = hub.subscribe(Arc::new(move |_: &ScanProgress| {
            slot.lock().unwrap().take();
        }));

        hub.publish(&progress("counting", 1));
        hub.publish(&progress("scanning", 2));

        assert_eq!(*seen.lock().unwrap(), [("counting".to_owned(), 1)]);
    }

    #[test]
    fn forwarded_progress_arrives_in_the_model_inbox_as_inputs() {
        #[derive(Debug, PartialEq)]
        enum Input {
            Progress(String, usize),
        }
        let fake = FakeBackend::new();
        let inbox = Inbox::new();

        fake.emit_progress(progress("counting", 0));
        let subscription = forward(&fake, inbox.sender(), |event| {
            Input::Progress(event.phase, event.processed)
        });
        fake.emit_progress(progress("scanning", 4));
        fake.emit_progress(progress("cleanup", 9));
        drop(subscription);
        fake.emit_progress(progress("scanning", 10));

        assert_eq!(
            inbox.try_recv(),
            Some(Input::Progress("scanning".into(), 4))
        );
        assert_eq!(inbox.try_recv(), Some(Input::Progress("cleanup".into(), 9)));
        assert_eq!(inbox.try_recv(), None);
    }
}
