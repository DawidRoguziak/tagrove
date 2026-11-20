//! The model convention of the client crate (plan decision D4).
//!
//! A model is a state machine without I/O: [`Model::handle`] takes one input and
//! returns the effects to run. Inputs are the user's commands, the replies of
//! backend calls and the firing of timers. Every reply and every timer carries the
//! [`RequestToken`] (or generation) it was started with; a model that has moved on
//! drops it, so a late reply never changes state. State is published from the
//! backend's canonical result only, after the backend succeeded.
//!
//! Effects are data. A [`Runner`] turns them into backend calls on an
//! [`Executor`] and into timers; results come back through the model's [`Inbox`]
//! and are applied on the model thread by [`Driver::pump`]. The model thread never
//! waits for the backend, and no lock of the core is ever held across this
//! boundary: a use case takes and releases its locks inside one task.
//!
//! [`Driver`] ties the pieces together and is the same in the application
//! (`ThreadExecutor`, a wake callback into the UI loop) and in tests
//! (`ManualExecutor`, `FakeBackend`, explicit `pump`).

use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

use crate::{
    backend::{BackendError, BackendResult},
    exec::Executor,
};

/// A state machine: input → new state and effects.
pub trait Model {
    /// Commands, backend replies and timer firings.
    type Input: Send + 'static;
    /// Backend calls and timers to start, as data.
    type Effect;

    fn handle(&mut self, input: Self::Input) -> Vec<Self::Effect>;
}

/// Runs the effects of a model.
pub trait Runner<M: Model> {
    fn run(&mut self, effect: M::Effect, dispatch: &Dispatch<M::Input>);
}

/// Identifies one request or timer of a model. Tokens of one [`Tokens`] source
/// are unique and increase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestToken(u64);

impl RequestToken {
    pub fn value(self) -> u64 {
        self.0
    }
}

/// Source of request tokens; the first token is 1.
#[derive(Debug, Default, Clone)]
pub struct Tokens {
    last: u64,
}

impl Tokens {
    /// A token no earlier call returned.
    pub fn issue(&mut self) -> RequestToken {
        self.last += 1;
        RequestToken(self.last)
    }
}

/// A slot for "only the latest request counts": starting a request supersedes the
/// one before it, and only the reply with the current token is accepted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RequestSlot {
    current: Option<RequestToken>,
}

impl RequestSlot {
    /// Starts a request and supersedes the pending one.
    pub fn begin(&mut self, tokens: &mut Tokens) -> RequestToken {
        let token = tokens.issue();
        self.current = Some(token);
        token
    }

    /// Whether `token` is the pending request. The slot is left unchanged.
    pub fn is_current(&self, token: RequestToken) -> bool {
        self.current == Some(token)
    }

    /// Accepts the reply of `token`: `true` and the slot becomes empty when it is
    /// the pending request; `false` for a superseded or cancelled one, whose
    /// reply must be dropped.
    pub fn finish(&mut self, token: RequestToken) -> bool {
        let current = self.is_current(token);
        if current {
            self.current = None;
        }
        current
    }

    /// Drops the pending request; its reply will be refused.
    pub fn cancel(&mut self) {
        self.current = None;
    }

    pub fn is_pending(&self) -> bool {
        self.current.is_some()
    }
}

/// A counter that invalidates everything started before its last
/// [`advance`](Self::advance), such as the `generation` of a query owner.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Generation(u64);

impl Generation {
    /// Starts a new generation and returns its number (the first is 1).
    pub fn advance(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }

    pub fn current(self) -> u64 {
        self.0
    }

    pub fn is_current(self, generation: u64) -> bool {
        self.0 == generation
    }
}

type Wake = Arc<dyn Fn() + Send + Sync>;

/// The queue through which replies, timer firings and progress reach a model.
/// Sending never blocks; the model thread drains it.
pub struct Inbox<I> {
    receiver: mpsc::Receiver<I>,
    sender: Sender<I>,
}

/// The sending half of an [`Inbox`]; cheap to clone and usable on any thread.
pub struct Sender<I> {
    sender: mpsc::Sender<I>,
    wake: Wake,
    /// Set while the owner has been woken and has not yet found the inbox empty.
    woken: Arc<AtomicBool>,
}

impl<I> Clone for Sender<I> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            wake: Arc::clone(&self.wake),
            woken: Arc::clone(&self.woken),
        }
    }
}

impl<I> Sender<I> {
    /// Queues `input` and wakes the model thread unless it has been woken and
    /// has not drained the inbox since. An input for an inbox that is gone is
    /// dropped.
    pub fn send(&self, input: I) {
        if self.sender.send(input).is_ok() && !self.woken.swap(true, Ordering::SeqCst) {
            (self.wake)();
        }
    }
}

impl<I> Default for Inbox<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I> Inbox<I> {
    /// An inbox nobody is woken for; the owner polls it.
    pub fn new() -> Self {
        Self::with_wake(|| {})
    }

    /// An inbox that calls `wake` on the sending thread when an input is queued.
    /// Wakes are coalesced: after one wake there is no other until the owner has
    /// drained the inbox ([`try_recv`](Self::try_recv) returned `None`), so a
    /// burst of replies or progress events costs one wake. `wake` must be cheap
    /// and must not block: it only tells the model thread to call
    /// [`Driver::pump`].
    pub fn with_wake(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            receiver,
            sender: Sender {
                sender,
                wake: Arc::new(wake),
                woken: Arc::new(AtomicBool::new(false)),
            },
        }
    }

    pub fn sender(&self) -> Sender<I> {
        self.sender.clone()
    }

    /// The next queued input, without waiting. `None` re-arms the wake.
    pub fn try_recv(&self) -> Option<I> {
        if let Ok(input) = self.receiver.try_recv() {
            return Some(input);
        }
        // An input queued before the flag is cleared wakes nobody, so look once
        // more; one queued after it wakes the owner again.
        self.sender.woken.store(false, Ordering::SeqCst);
        self.receiver.try_recv().ok()
    }

    /// The next input, waiting for at most `timeout`.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<I> {
        self.try_recv()
            .or_else(|| self.receiver.recv_timeout(timeout).ok())
    }
}

/// What a [`Runner`] uses to run effects: the executor and the way back into
/// the model.
pub struct Dispatch<I> {
    executor: Arc<dyn Executor>,
    sender: Sender<I>,
}

impl<I> Clone for Dispatch<I> {
    fn clone(&self) -> Self {
        Self {
            executor: Arc::clone(&self.executor),
            sender: self.sender.clone(),
        }
    }
}

impl<I: Send + 'static> Dispatch<I> {
    pub fn new(executor: Arc<dyn Executor>, sender: Sender<I>) -> Self {
        Self { executor, sender }
    }

    /// Runs the blocking backend call `work` on the executor and sends
    /// `reply(result)` to the model. The model is answered exactly once: a panic
    /// in `work`, or a task the executor dropped, answers with a
    /// `WorkerFailed` error named after `operation`.
    pub fn call<T, W, R>(&self, operation: &'static str, work: W, reply: R)
    where
        T: 'static,
        W: FnOnce() -> BackendResult<T> + Send + 'static,
        R: FnOnce(BackendResult<T>) -> I + Send + 'static,
    {
        let mut answer = Answer {
            reply: Some(Box::new(reply)),
            sender: self.sender.clone(),
            operation,
        };
        self.executor.spawn(Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|payload| {
                Err(BackendError::worker_failed(
                    operation,
                    &panic_text(payload.as_ref()),
                ))
            });
            answer.send(result);
        }));
    }

    /// Runs `task` on the executor with a sender for whatever it reports. For
    /// calls that answer through a callback; nothing answers for a dropped task.
    pub fn spawn(&self, task: impl FnOnce(Sender<I>) + Send + 'static) {
        let sender = self.sender.clone();
        self.executor.spawn(Box::new(move || task(sender)));
    }

    /// Sends `input` to the model after `delay`: a timer effect.
    pub fn after(&self, delay: Duration, input: I) {
        let sender = self.sender.clone();
        self.executor
            .spawn_after(delay, Box::new(move || sender.send(input)));
    }

    /// Sends `input` to the model's inbox now.
    pub fn send(&self, input: I) {
        self.sender.send(input);
    }

    pub fn sender(&self) -> Sender<I> {
        self.sender.clone()
    }
}

/// Answers the model once, also when the task that owns it is dropped unrun.
struct Answer<T, I> {
    reply: Option<Box<dyn FnOnce(BackendResult<T>) -> I + Send>>,
    sender: Sender<I>,
    operation: &'static str,
}

impl<T, I> Answer<T, I> {
    fn send(&mut self, result: BackendResult<T>) {
        if let Some(reply) = self.reply.take() {
            self.sender.send(reply(result));
        }
    }
}

impl<T, I> Drop for Answer<T, I> {
    fn drop(&mut self) {
        if self.reply.is_some() {
            let operation = self.operation;
            self.send(Err(BackendError::worker_failed(
                operation,
                "the task was dropped before it ran",
            )));
        }
    }
}

/// The message of a panic payload, as `std` prints it.
pub fn panic_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "the task panicked".to_owned()
    }
}

/// A model with the runner of its effects and its inbox.
pub struct Driver<M: Model, R: Runner<M>> {
    model: M,
    runner: R,
    inbox: Inbox<M::Input>,
    dispatch: Dispatch<M::Input>,
}

impl<M: Model, R: Runner<M>> Driver<M, R> {
    /// A driver whose owner polls [`pump`](Self::pump) itself (tests).
    pub fn new(model: M, runner: R, executor: Arc<dyn Executor>) -> Self {
        Self::with_inbox(model, runner, executor, Inbox::new())
    }

    /// A driver that calls `wake` when an input is waiting, on the thread that
    /// queued it and once per burst ([`Inbox::with_wake`]). The application's
    /// `wake` schedules a `pump` on the model thread.
    pub fn with_wake(
        model: M,
        runner: R,
        executor: Arc<dyn Executor>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self::with_inbox(model, runner, executor, Inbox::with_wake(wake))
    }

    fn with_inbox(
        model: M,
        runner: R,
        executor: Arc<dyn Executor>,
        inbox: Inbox<M::Input>,
    ) -> Self {
        let dispatch = Dispatch::new(executor, inbox.sender());
        Self {
            model,
            runner,
            inbox,
            dispatch,
        }
    }

    pub fn model(&self) -> &M {
        &self.model
    }

    /// For reading out what the model collected for its view (event queues).
    /// State changes go through [`handle`](Self::handle).
    pub fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }

    /// Applies one input and starts the effects it asks for.
    pub fn handle(&mut self, input: M::Input) {
        for effect in self.model.handle(input) {
            self.runner.run(effect, &self.dispatch);
        }
    }

    /// Applies every input that is waiting in the inbox, in arrival order.
    /// Returns how many were applied.
    pub fn pump(&mut self) -> usize {
        let mut applied = 0;
        while let Some(input) = self.inbox.try_recv() {
            self.handle(input);
            applied += 1;
        }
        applied
    }

    /// Waits up to `timeout` for one input and applies it. For tests that run
    /// against real threads. Returns whether an input arrived.
    pub fn pump_one(&mut self, timeout: Duration) -> bool {
        match self.inbox.recv_timeout(timeout) {
            Some(input) => {
                self.handle(input);
                true
            }
            None => false,
        }
    }

    /// A sender into this model's inbox, for sources outside the runner such as
    /// a progress subscription.
    pub fn sender(&self) -> Sender<M::Input> {
        self.inbox.sender()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use media_tagger_core::models::TagListPage;

    use super::{
        Dispatch, Driver, Generation, Inbox, Model, RequestSlot, RequestToken, Runner, Tokens,
    };
    use crate::{
        backend::{Backend, BackendError, BackendErrorKind, BackendResult, FakeBackend},
        exec::{Executor, ManualExecutor, ThreadExecutor},
    };

    /// A minimal model in the convention: it shows the tags matching the latest
    /// query, loaded through the backend, and a status line cleared by a timer.
    #[derive(Default)]
    struct TagSearch {
        tokens: Tokens,
        load: RequestSlot,
        status_timer: RequestSlot,
        tags: Vec<String>,
        error: Option<BackendError>,
        status: Option<&'static str>,
    }

    enum Input {
        Search(String),
        Loaded {
            token: RequestToken,
            result: BackendResult<TagListPage>,
        },
        StatusExpired(RequestToken),
    }

    enum Effect {
        Load {
            token: RequestToken,
            query: String,
        },
        ExpireStatus {
            token: RequestToken,
            after: Duration,
        },
    }

    impl Model for TagSearch {
        type Input = Input;
        type Effect = Effect;

        fn handle(&mut self, input: Input) -> Vec<Effect> {
            match input {
                Input::Search(query) => {
                    // Published state does not change until the backend answered.
                    let token = self.load.begin(&mut self.tokens);
                    vec![Effect::Load { token, query }]
                }
                Input::Loaded { token, result } => {
                    if !self.load.finish(token) {
                        return Vec::new();
                    }
                    match result {
                        Ok(page) => {
                            self.tags = page.items;
                            self.error = None;
                            self.status = Some("loaded");
                            let token = self.status_timer.begin(&mut self.tokens);
                            vec![Effect::ExpireStatus {
                                token,
                                after: Duration::from_secs(2),
                            }]
                        }
                        Err(error) => {
                            self.error = Some(error);
                            Vec::new()
                        }
                    }
                }
                Input::StatusExpired(token) => {
                    if self.status_timer.finish(token) {
                        self.status = None;
                    }
                    Vec::new()
                }
            }
        }
    }

    struct TagSearchRunner(Arc<dyn Backend>);

    impl Runner<TagSearch> for TagSearchRunner {
        fn run(&mut self, effect: Effect, dispatch: &Dispatch<Input>) {
            match effect {
                Effect::Load { token, query } => {
                    let backend = Arc::clone(&self.0);
                    dispatch.call(
                        "list_tags",
                        move || backend.list_tags(query, 0, 50),
                        move |result| Input::Loaded { token, result },
                    );
                }
                Effect::ExpireStatus { token, after } => {
                    dispatch.after(after, Input::StatusExpired(token));
                }
            }
        }
    }

    fn page(items: &[&str]) -> TagListPage {
        TagListPage {
            items: items.iter().map(|item| (*item).to_owned()).collect(),
            total: items.len() as i64,
        }
    }

    fn tag_search(
        fake: &Arc<FakeBackend>,
        executor: &Arc<ManualExecutor>,
    ) -> Driver<TagSearch, TagSearchRunner> {
        fake.on_list_tags(|query, _, _| Ok(page(&[&format!("{query}-1"), &format!("{query}-2")])));
        Driver::new(
            TagSearch::default(),
            TagSearchRunner(Arc::clone(fake) as Arc<dyn Backend>),
            Arc::clone(executor) as Arc<dyn Executor>,
        )
    }

    #[test]
    fn state_is_published_only_after_the_backend_answered() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = tag_search(&fake, &executor);

        driver.handle(Input::Search("cat".into()));
        assert!(driver.model().tags.is_empty());
        assert!(driver.model().load.is_pending());
        assert_eq!(fake.calls().len(), 0, "the call waits for the executor");

        assert_eq!(executor.run_all(), 1);
        assert!(
            driver.model().tags.is_empty(),
            "the reply waits in the inbox"
        );
        assert_eq!(driver.pump(), 1);
        assert_eq!(driver.model().tags, ["cat-1", "cat-2"]);
        assert!(!driver.model().load.is_pending());
    }

    #[test]
    fn a_late_reply_with_a_stale_token_does_not_change_state() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = tag_search(&fake, &executor);

        driver.handle(Input::Search("old".into()));
        driver.handle(Input::Search("new".into()));
        assert_eq!(executor.pending(), 2);

        // The newer request answers first.
        assert!(executor.run_nth(1));
        assert_eq!(driver.pump(), 1);
        assert_eq!(driver.model().tags, ["new-1", "new-2"]);
        let published = driver.model().tags.clone();

        // The older request answers late: its token is stale.
        assert!(executor.run_nth(0));
        assert_eq!(driver.pump(), 1);
        assert_eq!(driver.model().tags, published);
        assert_eq!(driver.model().error, None);
        assert_eq!(
            executor.pending_timers(),
            1,
            "the stale reply started no timer"
        );
    }

    #[test]
    fn a_stale_reply_that_arrives_first_is_dropped_too() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = tag_search(&fake, &executor);

        driver.handle(Input::Search("old".into()));
        driver.handle(Input::Search("new".into()));
        assert!(executor.run_nth(0));
        assert_eq!(driver.pump(), 1);
        assert!(driver.model().tags.is_empty());
        assert!(driver.model().load.is_pending());

        assert!(executor.run_next());
        driver.pump();
        assert_eq!(driver.model().tags, ["new-1", "new-2"]);
    }

    #[test]
    fn a_failed_call_keeps_the_published_state() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = tag_search(&fake, &executor);
        driver.handle(Input::Search("cat".into()));
        executor.run_all();
        driver.pump();

        fake.on_list_tags(|_, _, _| Err(BackendError::rejected("i18n:common.libraryBusy")));
        driver.handle(Input::Search("dog".into()));
        executor.run_all();
        driver.pump();

        assert_eq!(driver.model().tags, ["cat-1", "cat-2"]);
        assert_eq!(
            driver
                .model()
                .error
                .as_ref()
                .and_then(BackendError::i18n_key),
            Some("common.libraryBusy")
        );
    }

    #[test]
    fn timers_are_effects_and_an_obsolete_timer_is_ignored() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = tag_search(&fake, &executor);

        driver.handle(Input::Search("a".into()));
        executor.run_all();
        driver.pump();
        assert_eq!(driver.model().status, Some("loaded"));

        // One second later a second search loads and restarts the status timer.
        executor.advance_and_run(Duration::from_secs(1));
        driver.handle(Input::Search("b".into()));
        executor.run_all();
        driver.pump();

        // The first timer fires: it is obsolete, the status stays.
        assert_eq!(executor.advance_and_run(Duration::from_secs(1)), 1);
        assert_eq!(driver.pump(), 1);
        assert_eq!(driver.model().status, Some("loaded"));

        // The second timer clears it.
        executor.advance_and_run(Duration::from_secs(1));
        driver.pump();
        assert_eq!(driver.model().status, None);
    }

    #[test]
    fn a_panicking_call_answers_with_a_worker_failure() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = tag_search(&fake, &executor);
        fake.on_list_tags(|_, _, _| panic!("synthetic backend panic"));

        driver.handle(Input::Search("cat".into()));
        executor.run_all();
        assert_eq!(driver.pump(), 1);

        let error = driver
            .model()
            .error
            .clone()
            .expect("the model was answered");
        assert_eq!(error.kind(), BackendErrorKind::WorkerFailed);
        assert_eq!(
            error.text(),
            "list_tags worker failed: synthetic backend panic"
        );
        assert!(!driver.model().load.is_pending());
    }

    #[test]
    fn a_task_the_executor_drops_still_answers_the_model() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = tag_search(&fake, &executor);

        driver.handle(Input::Search("cat".into()));
        assert!(executor.drop_nth(0));
        assert_eq!(driver.pump(), 1);

        let error = driver
            .model()
            .error
            .clone()
            .expect("the model was answered");
        assert_eq!(error.kind(), BackendErrorKind::WorkerFailed);
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn the_same_model_runs_on_background_threads_and_wakes_its_owner() {
        let fake = Arc::new(FakeBackend::new());
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        let caller = std::thread::current().id();
        let seen = Arc::new(std::sync::Mutex::new(None));
        let thread_of_call = Arc::clone(&seen);
        fake.on_list_tags(move |query, _, _| {
            *thread_of_call.lock().unwrap() = Some(std::thread::current().id());
            Ok(page(&[&query]))
        });
        let mut driver = Driver::with_wake(
            TagSearch::default(),
            TagSearchRunner(Arc::clone(&fake) as Arc<dyn Backend>),
            Arc::new(ThreadExecutor::new()) as Arc<dyn Executor>,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        );

        driver.handle(Input::Search("cat".into()));
        assert!(driver.pump_one(Duration::from_secs(10)));

        assert_eq!(driver.model().tags, ["cat"]);
        // The wake follows the queued input on the worker thread.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while wakes.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        let worker = seen.lock().unwrap().expect("the backend was called");
        assert_ne!(worker, caller, "the backend call ran on the model thread");
    }

    #[test]
    fn tokens_slots_and_generations() {
        let mut tokens = Tokens::default();
        let mut slot = RequestSlot::default();
        assert!(!slot.is_pending());

        let first = slot.begin(&mut tokens);
        let second = slot.begin(&mut tokens);
        assert!(second > first);
        assert_eq!((first.value(), second.value()), (1, 2));
        assert!(!slot.finish(first));
        assert!(slot.is_pending());
        assert!(slot.is_current(second));
        assert!(slot.finish(second));
        assert!(!slot.finish(second), "a reply is accepted once");

        let third = slot.begin(&mut tokens);
        slot.cancel();
        assert!(!slot.finish(third));

        let mut generation = Generation::default();
        assert_eq!(generation.current(), 0);
        assert_eq!(generation.advance(), 1);
        assert_eq!(generation.advance(), 2);
        assert!(generation.is_current(2));
        assert!(!generation.is_current(1));
    }

    #[test]
    fn a_burst_of_inputs_wakes_the_owner_once_until_the_inbox_is_drained() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        let inbox = Inbox::with_wake(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        let sender = inbox.sender();

        for input in 0..3 {
            sender.send(input);
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert_eq!(inbox.try_recv(), Some(0));
        sender.send(3);
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            1,
            "the owner is still draining"
        );

        while inbox.try_recv().is_some() {}
        sender.send(4);
        assert_eq!(wakes.load(Ordering::SeqCst), 2);
        assert_eq!(inbox.try_recv(), Some(4));
    }

    #[test]
    fn an_inbox_keeps_arrival_order_and_ignores_a_sender_after_it_is_gone() {
        let inbox = Inbox::new();
        let sender = inbox.sender();
        sender.send(1);
        sender.send(2);
        assert_eq!(inbox.try_recv(), Some(1));
        assert_eq!(inbox.try_recv(), Some(2));
        assert_eq!(inbox.try_recv(), None);

        drop(inbox);
        sender.send(3);
    }
}
