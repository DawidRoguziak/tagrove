//! The executor port: where backend calls and timers of the models run (plan
//! decision D4).
//!
//! Models are state machines: an input gives a new state and a list of effects. An
//! effect that calls the backend or waits is handed to an [`Executor`], and its
//! result comes back as another input through the model's inbox
//! ([`crate::model::Inbox`]). The model thread (the UI thread in the GPUI adapter)
//! therefore never waits for the backend.
//!
//! Two implementations: [`ThreadExecutor`] runs tasks on background threads and is
//! what the application uses; `ManualExecutor` (feature `test-support`) runs them
//! only when a test says so, in the order the test chooses, on a virtual clock.

use std::time::Duration;

#[cfg(any(test, feature = "test-support"))]
pub mod manual;
pub mod thread;

#[cfg(any(test, feature = "test-support"))]
pub use manual::ManualExecutor;
pub use thread::ThreadExecutor;

/// One unit of background work. A task dropped without running must be safe: a
/// reply that depends on it is answered by its own drop guard
/// ([`crate::model::Dispatch::call`]).
pub type Task = Box<dyn FnOnce() + Send + 'static>;

pub trait Executor: Send + Sync + 'static {
    /// Runs `task` off the calling thread. Never blocks the caller on the task.
    fn spawn(&self, task: Task);

    /// Runs `task` once `delay` has passed. Timers are not cancelled: a model
    /// drops the firing of an obsolete timer by its token.
    fn spawn_after(&self, delay: Duration, task: Task);
}
