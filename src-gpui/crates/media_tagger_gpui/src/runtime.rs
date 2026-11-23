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
    scenario::{
        self, ScenarioClick, ScenarioDock, ScenarioLightbox, ScenarioSearch, ScenarioSettings,
        ScenarioTagList,
    },
    settings::{FolderPickerRequest, SettingsCommand, SettingsInput, SettingsSection},
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
    /// The clicks of a comparison scenario that wait for their tiles' rows.
    scripted_selection: &'static [ScenarioClick],
    /// A comparison scenario asks for the focus in the search field.
    scripted_search_focus: bool,
    /// The tag whose button a comparison scenario's tag list starts focused.
    scripted_tag_list_focus: Option<&'static str>,
    /// What a comparison scenario does in the bulk dock once its clicks are made.
    scripted_dock: Option<ScenarioDock>,
    /// The lightbox a comparison scenario opens once the row of its tile is loaded.
    scripted_lightbox: Option<ScenarioLightbox>,
    /// The settings section a comparison scenario scrolls to.
    scripted_settings_section: Option<SettingsSection>,
    /// The startup-scan switch a comparison scenario sets on the first root.
    scripted_auto_scan: Option<bool>,
    /// The folder dialog that is open; its answer goes to the operation that asked.
    _folder_picker: Option<Task<()>>,
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
            scripted_selection: &[],
            scripted_search_focus: false,
            scripted_tag_list_focus: None,
            scripted_dock: None,
            scripted_lightbox: None,
            scripted_settings_section: None,
            scripted_auto_scan: None,
            _folder_picker: None,
            _pump: pump,
        }
    }

    /// Makes `clicks` in selection mode as soon as the rows of their tiles are loaded:
    /// the selection a comparison scenario starts with (`Scenario::selection`).
    pub fn script_selection(&mut self, clicks: &'static [ScenarioClick], cx: &mut Context<Self>) {
        self.scripted_selection = clicks;
        self.publish(cx);
    }

    /// Gives the search header the state of a comparison scenario: its text typed, its
    /// media kind chosen (which submits), and the focus in the field once the gallery
    /// asks for it ([`Self::take_search_focus`]).
    pub fn script_search(&mut self, search: ScenarioSearch, cx: &mut Context<Self>) {
        for command in search.commands() {
            self.driver
                .handle(media_tagger_client::runtime::RuntimeInput::Search(command));
        }
        self.scripted_search_focus = search.focus;
        self.publish(cx);
    }

    /// Opens the tag list with the choices of a comparison scenario. The gallery's
    /// dialog follows the open list as soon as it is made, and gives the focus to the
    /// button of the last tag clicked ([`Self::take_tag_list_focus`]).
    pub fn script_tag_list(&mut self, tag_list: ScenarioTagList, cx: &mut Context<Self>) {
        for input in tag_list.inputs() {
            self.driver.handle(RuntimeInput::TagList(input));
        }
        self.scripted_tag_list_focus = tag_list.focused();
        self.publish(cx);
    }

    /// What a comparison scenario does in the bulk dock after its clicks
    /// (`Scenario::dock`); the dock takes it with [`Self::take_scripted_dock`].
    pub fn script_dock(&mut self, dock: ScenarioDock, cx: &mut Context<Self>) {
        self.scripted_dock = Some(dock);
        self.publish(cx);
    }

    /// The dock steps of a scenario, once its clicks are made and their range is read:
    /// the selection the steps act on is there. Answers once.
    pub fn take_scripted_dock(&mut self) -> Option<ScenarioDock> {
        let selection = self.driver.model().selection();
        if !self.scripted_selection.is_empty() || selection.busy() || selection.count() == 0 {
            return None;
        }
        self.scripted_dock.take()
    }

    /// The lightbox a comparison scenario opens (`Scenario::lightbox`); the lightbox
    /// view takes it with [`Self::take_scripted_lightbox`].
    pub fn script_lightbox(&mut self, lightbox: ScenarioLightbox, cx: &mut Context<Self>) {
        self.scripted_lightbox = Some(lightbox);
        self.publish(cx);
    }

    /// The lightbox of a scenario, once the startup scan and its refreshes have ended
    /// and the row of its tile is loaded: the click on that tile can be made. Answers
    /// once.
    pub fn take_scripted_lightbox(&mut self) -> Option<ScenarioLightbox> {
        let model = self.driver.model();
        let script = self.scripted_lightbox?;
        if !model.is_started()
            || model.maintenance_loading()
            || model.query().session_id().is_none()
            || model.query().query_pending()
            || model.query().asset_at(script.index).is_none()
        {
            return None;
        }
        self.scripted_lightbox.take()
    }

    /// Opens the settings page of a comparison scenario: the page at once, its section
    /// once the launch's work has ended and the page it left is laid out
    /// ([`Self::take_scripted_settings_section`]), and the startup-scan switch of the
    /// first root once the startup scan has ended and the roots are listed.
    pub fn script_settings(&mut self, settings: ScenarioSettings, cx: &mut Context<Self>) {
        self.driver.handle(RuntimeInput::OpenSettings);
        self.scripted_settings_section = settings.section;
        self.scripted_auto_scan = settings.first_root_auto_scan;
        self.publish(cx);
    }

    /// The settings section a scenario scrolls to, without taking it, once its
    /// navigation click can be made: the launch's work has ended (the startup scan with
    /// its refreshes, the root listing, the read of the video tools) and no operation
    /// runs, so the scan section above the target has the height it keeps. The Tauri
    /// steps click on a page in that state; a click made earlier would leave the
    /// section short of its place by what the scan section grows afterwards.
    pub fn scripted_settings_section(&self) -> Option<SettingsSection> {
        let model = self.driver.model();
        let settings = model.settings();
        if !model.is_started()
            || model.maintenance_loading()
            || model.scan_roots_loading()
            || settings.is_locked()
            || settings.video_tools().is_none()
            || self.scripted_auto_scan.is_some()
        {
            return None;
        }
        self.scripted_settings_section
    }

    /// The settings section a scenario scrolls to, once its click can be made
    /// ([`Self::scripted_settings_section`]); answers once.
    pub fn take_scripted_settings_section(&mut self) -> Option<SettingsSection> {
        let section = self.scripted_settings_section()?;
        self.scripted_settings_section = None;
        Some(section)
    }

    /// Sets the scripted startup-scan switch once nothing else runs and the roots are
    /// there.
    fn run_scripted_auto_scan(&mut self) {
        let model = self.driver.model();
        let Some(enabled) = self.scripted_auto_scan else {
            return;
        };
        if !model.is_started() || model.maintenance_loading() || model.settings().is_locked() {
            return;
        }
        let Some(root) = model.scan_roots().first() else {
            return;
        };
        let path = root.path.clone();
        self.scripted_auto_scan = None;
        self.driver
            .handle(RuntimeInput::settings(SettingsCommand::SetAutoScan {
                path,
                enabled,
            }));
    }

    /// Opens the folder dialog a settings operation asked for (GPUI's dialog, the XDG
    /// portal on Linux) and hands its answer to that operation. A dialog that could not
    /// open answers with the platform's error; one that closed without an answer counts
    /// as cancelled.
    fn open_folder_picker(&mut self, request: FolderPickerRequest, cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: None,
        });
        let op = request.op;
        self._folder_picker = Some(cx.spawn(async move |this, cx| {
            let result = match answer.await {
                Ok(Ok(paths)) => Ok(paths.map(|paths| {
                    paths
                        .into_iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect()
                })),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Ok(None),
            };
            this.update(cx, |runtime, cx| {
                runtime.send(
                    RuntimeInput::Settings(SettingsInput::FoldersPicked { op, result }),
                    cx,
                )
            })
            .ok();
        }));
    }

    /// The answers of the lightbox model to the view's commands since the last call
    /// (the settled deletions).
    pub fn take_lightbox_events(&mut self) -> Vec<media_tagger_client::lightbox::LightboxEvent> {
        self.driver.model_mut().take_lightbox_events()
    }

    /// The answers of the bulk model to the dock's commands since the last call.
    pub fn take_bulk_events(&mut self) -> Vec<media_tagger_client::bulk::BulkEvent> {
        self.driver.model_mut().take_bulk_events()
    }

    /// The tag whose button a scenario's tag list starts focused; answers once.
    pub fn take_tag_list_focus(&mut self) -> Option<&'static str> {
        self.scripted_tag_list_focus.take()
    }

    /// Whether a scenario asked for the focus in the search field; answers `true`
    /// once.
    pub fn take_search_focus(&mut self) -> bool {
        std::mem::take(&mut self.scripted_search_focus)
    }

    /// Makes the scripted clicks once, when the query has their rows.
    fn run_scripted_selection(&mut self) {
        // Not before the startup scan and its refreshes have ended: a refresh would
        // cancel the range read of a Shift click that is on its way.
        let model = self.driver.model();
        if self.scripted_selection.is_empty() || !model.is_started() || model.maintenance_loading()
        {
            return;
        }
        let Some(inputs) =
            scenario::selection_inputs(self.scripted_selection, self.driver.model().query())
        else {
            return;
        };
        self.scripted_selection = &[];
        for input in inputs {
            self.driver.handle(RuntimeInput::Selection(input));
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

    /// The suggestion client's answers to `editor` since the last call.
    pub fn take_suggestion_events_for(
        &mut self,
        editor: media_tagger_client::search::EditorId,
    ) -> Vec<media_tagger_client::search::SuggestionEvent> {
        self.driver.model_mut().take_suggestion_events_for(editor)
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
        self.run_scripted_selection();
        self.run_scripted_auto_scan();
        if let Some(request) = self.driver.model_mut().take_folder_picker_request() {
            self.open_folder_picker(request, cx);
        }
        // The bulk model and the lightbox model get their answers and every commit
        // from the client's runtime itself. What is left belongs to a caller with
        // tickets of its own, and no view is one: dropped here so that it does not
        // pile up with every write.
        self.driver.model_mut().take_library_events();
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
