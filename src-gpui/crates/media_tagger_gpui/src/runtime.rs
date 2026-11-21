//! The application runtime as a GPUI entity: the [`AppRuntime`] model of the client crate
//! with its driver, woken into the UI thread whenever the background answers.
//!
//! Backend calls run on the executor the runtime is given (the application's
//! `ThreadExecutor`, a `ManualExecutor` in tests), never on the UI thread. A reply lands in
//! the model's inbox on whatever thread produced it; the inbox's wake raises a
//! [`WakeSignal`], and a task on GPUI's foreground executor waiting on that signal
//! pumps the inbox on the UI thread and redraws. Wakes are coalesced, so a burst of
//! replies costs one pump.
//!
//! Views hold an `Entity<Runtime>` and observe it; the runtime outlives every view.

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context as TaskContext, Poll, Waker},
};

use gpui::{App, AppContext as _, Context, Entity, Task};
use media_tagger_client::{
    backend::Backend,
    exec::Executor,
    i18n::I18n,
    model::Driver,
    prefs::PrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
};

use crate::{thumbs::Thumbnails, ui};

/// A wake-up for one waiting task, raised from any thread. Raising it while nobody
/// waits is remembered until the next wait.
#[derive(Clone, Default)]
pub struct WakeSignal {
    state: Arc<Mutex<SignalState>>,
}

#[derive(Default)]
struct SignalState {
    raised: bool,
    waker: Option<Waker>,
}

impl WakeSignal {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, SignalState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Raises the signal and wakes the waiting task.
    pub fn raise(&self) {
        let waker = {
            let mut state = self.state();
            state.raised = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Resolves once the signal has been raised, and lowers it.
    pub fn wait(&self) -> Wait<'_> {
        Wait { signal: self }
    }
}

pub struct Wait<'a> {
    signal: &'a WakeSignal,
}

impl Future for Wait<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<()> {
        let mut state = self.signal.state();
        if std::mem::take(&mut state.raised) {
            Poll::Ready(())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// What the runtime needs from the composition of the application.
#[derive(Clone)]
pub struct RuntimeParts {
    pub config: RuntimeConfig,
    pub backend: Arc<dyn Backend>,
    pub prefs: Arc<dyn PrefsStore>,
    pub executor: Arc<dyn Executor>,
}

/// The runtime entity.
pub struct Runtime {
    driver: Driver<AppRuntime, RuntimeRunner>,
    /// The gallery's thumbnails; they outlive the gallery view.
    thumbnails: Entity<Thumbnails>,
    _pump: Task<()>,
}

impl Runtime {
    /// The runtime of the application in `parts.config`; it also makes that theme the
    /// active one.
    pub fn new(parts: RuntimeParts, cx: &mut Context<Self>) -> Self {
        let signal = WakeSignal::new();
        let wake = signal.clone();
        let thumbnails = {
            let backend = parts.backend.clone();
            let executor = parts.executor.clone();
            cx.new(|cx| Thumbnails::new(backend, executor, cx))
        };
        let driver = Driver::with_wake(
            AppRuntime::new(parts.config),
            RuntimeRunner::new(parts.backend, parts.prefs),
            parts.executor,
            move || wake.raise(),
        );
        let pump = cx.spawn(async move |this, cx| loop {
            signal.wait().await;
            if this.update(cx, |runtime, cx| runtime.pump(cx)).is_err() {
                break;
            }
        });
        ui::theme::set_active(parts.config.theme, cx);
        Self {
            driver,
            thumbnails,
            _pump: pump,
        }
    }

    pub fn model(&self) -> &AppRuntime {
        self.driver.model()
    }

    /// The thumbnails of the gallery's tiles.
    pub fn thumbnails(&self) -> &Entity<Thumbnails> {
        &self.thumbnails
    }

    /// Texts in the runtime's language.
    pub fn i18n(&self) -> I18n<'static> {
        I18n::new(self.model().language())
    }

    /// Applies a command and starts what it asks for.
    pub fn send(&mut self, input: RuntimeInput, cx: &mut Context<Self>) {
        self.driver.handle(input);
        self.publish(cx);
    }

    /// Applies every reply that is waiting. Returns how many there were.
    pub fn pump(&mut self, cx: &mut Context<Self>) -> usize {
        let applied = self.driver.pump();
        if applied > 0 {
            self.publish(cx);
        }
        applied
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        ui::theme::set_active(self.model().theme(), cx);
        cx.notify();
    }
}

/// Sends `input` to `runtime` from a place that only has the `App`.
pub fn send(runtime: &gpui::WeakEntity<Runtime>, input: RuntimeInput, cx: &mut App) {
    runtime
        .update(cx, |runtime, cx| runtime.send(input, cx))
        .ok();
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        task::{Context, Poll, Wake},
    };

    use super::WakeSignal;

    struct CountingWaker(AtomicUsize);

    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn poll(signal: &WakeSignal, waker: &Arc<CountingWaker>) -> Poll<()> {
        let waker = waker.clone().into();
        let mut cx = Context::from_waker(&waker);
        let mut wait = std::pin::pin!(signal.wait());
        wait.as_mut().poll(&mut cx)
    }

    use std::future::Future as _;

    #[test]
    fn a_raise_before_the_wait_is_kept_and_a_raise_during_it_wakes_once() {
        let signal = WakeSignal::new();
        let waker = Arc::new(CountingWaker(AtomicUsize::new(0)));

        signal.raise();
        signal.raise();
        assert_eq!(poll(&signal, &waker), Poll::Ready(()));
        assert_eq!(poll(&signal, &waker), Poll::Pending);

        let other = signal.clone();
        std::thread::spawn(move || other.raise()).join().unwrap();
        assert_eq!(waker.0.load(Ordering::SeqCst), 1);
        assert_eq!(poll(&signal, &waker), Poll::Ready(()));
    }
}
