//! Measurements on a library of one million results (epic E19). Nothing here is a
//! check: the one test is ignored by default and prints what it measured
//! (`docs/development/testing.md`, "GPUI workspace").
//!
//! The main window runs on GPUI's test platform (no display, no GPU) against the real
//! core on a temporary profile whose library is seeded by SQL with the statements of
//! `src-tauri/tests/grid_baseline.rs` (`TempCore::seed_synthetic_library`), so the
//! numbers stand next to that baseline's on the same data. The query calls of the
//! gallery go to the core; every thumbnail is answered with one real 390 px JPEG the
//! core rendered, because no file exists behind the synthetic rows.
//!
//! Backend calls run on the test's own thread (`ManualExecutor`), so a step is
//! timed in two parts: the frame (the event, the model's work and `Window::draw`)
//! and the backend calls that step started. In the application the second part runs
//! off the UI thread.
//!
//! ```sh
//! TMPDIR=/var/tmp cargo test --manifest-path src-gpui/Cargo.toml --locked --release \
//!   -p media_tagger_gpui --test million -- --ignored --nocapture
//! ```
//!
//! `MEDIATAGGER_GPUI_PERF_ROWS` sets another library size (a quick run of the
//! measurement itself: 20000) and `MEDIATAGGER_GPUI_PERF_TOURS` another length of the
//! long session (8 tours by default). `MEDIATAGGER_GPUI_PERF_TRACE_MIB` prints the call
//! stack of every Rust allocation of at least that many MiB, to find what grows.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use gpui::{
    point, px, Entity, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext,
    WindowHandle,
};
use media_tagger_client::{
    backend::{
        core_backend::test_support::TempCore, AssetQueryRequest, Backend, FakeBackend, MediaKind,
        ThumbnailImage, ThumbnailReply, ThumbnailRequest, ThumbnailResponder,
    },
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    search::SearchCommand,
    theme::ThemeName,
};
use media_tagger_core::models::StartAssetQueryResult;
use media_tagger_gpui::{
    profile,
    runtime::{Runtime, RuntimeParts},
    views::{gallery::GalleryView, shell::ShellView},
    window::{init_application, open_main_window},
};

/// The system allocator with a count of the bytes Rust code holds. The resident set
/// alone cannot tell a leak from an allocator that keeps freed memory; this can. What
/// SQLite allocates in C is not counted.
struct CountingAllocator;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
/// An allocation of at least this many bytes prints its call stack; 0 (the default)
/// prints none. Set from `MEDIATAGGER_GPUI_PERF_TRACE_MIB` to find what grows.
static TRACE_BYTES: AtomicUsize = AtomicUsize::new(0);
static TRACING: AtomicBool = AtomicBool::new(false);

/// Prints the call stack of a large allocation. The stack is captured with the
/// allocator in use, so a flag keeps the allocations it makes from being traced.
fn trace(size: usize) {
    let threshold = TRACE_BYTES.load(Ordering::Relaxed);
    if threshold == 0 || size < threshold || TRACING.swap(true, Ordering::SeqCst) {
        return;
    }
    eprintln!(
        "[million] allocation of {:.1} MiB:\n{}",
        size as f64 / (1024.0 * 1024.0),
        std::backtrace::Backtrace::force_capture()
    );
    TRACING.store(false, Ordering::SeqCst);
}

// SAFETY: every call is passed on to `System` unchanged; the counter only observes.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for `layout`.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            trace(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: `pointer` came from `System` with this `layout`, as the caller
        // guarantees for this allocator.
        unsafe { System.dealloc(pointer, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract.
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            LIVE_BYTES.fetch_add(new_size, Ordering::Relaxed);
            LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
            trace(new_size);
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn live_mib() -> f64 {
    LIVE_BYTES.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0)
}

const DEFAULT_ROWS: i64 = 1_000_000;

fn rows() -> i64 {
    std::env::var("MEDIATAGGER_GPUI_PERF_ROWS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|rows| *rows > 0)
        .unwrap_or(DEFAULT_ROWS)
}

fn tours() -> usize {
    std::env::var("MEDIATAGGER_GPUI_PERF_TOURS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

/// `mean, median, p95, max` of the samples, in milliseconds.
fn summary(samples: &[Duration]) -> String {
    if samples.is_empty() {
        return "n=0".into();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let total: Duration = sorted.iter().sum();
    let pick = |q: f64| sorted[((sorted.len() - 1) as f64 * q).round() as usize];
    format!(
        "mean {:.2} ms, median {:.2} ms, p95 {:.2} ms, max {:.2} ms (n={})",
        ms(total) / sorted.len() as f64,
        ms(pick(0.5)),
        ms(pick(0.95)),
        ms(sorted[sorted.len() - 1]),
        sorted.len()
    )
}

/// The resident set of this process in MiB (`/proc/self/statm`); 0 where there is
/// no such file.
fn resident_mib() -> f64 {
    let pages = std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| statm.split_whitespace().nth(1)?.parse::<f64>().ok())
        .unwrap_or(0.0);
    // Linux on x86-64 and aarch64 as shipped by the distributions has 4 KiB pages.
    pages * 4096.0 / (1024.0 * 1024.0)
}

/// One real thumbnail: the JPEG the core renders for a synthetic picture of a
/// temporary library of its own.
fn real_thumbnail() -> Vec<u8> {
    let core = TempCore::start();
    core.write_image("one.png");
    let backend = core.backend();
    backend
        .scan_folder(TempCore::text(&core.media_dir()))
        .expect("scan the one picture");
    let StartAssetQueryResult::Ready { items, .. } = backend
        .start_asset_query(AssetQueryRequest::new("million-thumbnail", 1))
        .expect("query the one picture")
    else {
        panic!("the query was superseded");
    };
    let asset = items.first().expect("the one picture is indexed");
    let (sender, receiver) = mpsc::channel();
    backend.request_thumbnail(
        ThumbnailRequest {
            asset_id: asset.id,
            version: asset.thumb_version.clone(),
            cached_only: false,
            prefetch: false,
            retry: 0,
        },
        ThumbnailResponder::new(move |reply| {
            let _ = sender.send(reply);
        }),
    );
    match receiver.recv_timeout(Duration::from_secs(60)) {
        Ok(ThumbnailReply::Ready(image)) => image.bytes,
        other => panic!("no thumbnail was rendered: {other:?}"),
    }
}

/// How long the core took for the gallery's calls, as the handlers measured them.
#[derive(Default)]
struct CoreTimes {
    starts: Vec<Duration>,
    pages: Vec<Duration>,
    sections: Vec<Duration>,
}

fn timed<T>(record: impl FnOnce(Duration), call: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = call();
    record(started.elapsed());
    result
}

struct Fixture {
    /// Dropped last: the temporary profile with the library.
    core: TempCore,
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    core_times: Arc<Mutex<CoreTimes>>,
    /// Whether a thumbnail request is answered with the picture or kept waiting.
    serve_thumbnails: Arc<AtomicBool>,
    waiting_thumbnails: Arc<Mutex<Vec<ThumbnailResponder>>>,
}

impl Fixture {
    /// The gallery's calls go to the core; a thumbnail is `thumbnail`.
    fn new(core: TempCore, thumbnail: Vec<u8>) -> Self {
        let backend = Arc::new(FakeBackend::new());
        let real = core.shared();
        let (start, page, sections, roots) =
            (real.clone(), real.clone(), real.clone(), real.clone());
        let core_times = Arc::new(Mutex::new(CoreTimes::default()));
        let (start_times, page_times, section_times) =
            (core_times.clone(), core_times.clone(), core_times.clone());
        backend
            .on_start_asset_query(move |request| {
                timed(
                    |time| start_times.lock().expect("times").starts.push(time),
                    || start.start_asset_query(request),
                )
            })
            .on_get_asset_query_page(move |session, offset, limit| {
                timed(
                    |time| page_times.lock().expect("times").pages.push(time),
                    || page.get_asset_query_page(session, offset, limit),
                )
            })
            .on_get_asset_query_sections(move |session| {
                timed(
                    |time| section_times.lock().expect("times").sections.push(time),
                    || sections.get_asset_query_sections(session),
                )
            })
            .on_list_tags(move |query, offset, limit| real.list_tags(query, offset, limit))
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(move || roots.list_scan_roots());
        let serve_thumbnails = Arc::new(AtomicBool::new(false));
        let waiting_thumbnails = Arc::new(Mutex::new(Vec::new()));
        let (serve, waiting) = (serve_thumbnails.clone(), waiting_thumbnails.clone());
        backend.on_request_thumbnail(move |request, responder| {
            if serve.load(Ordering::SeqCst) {
                responder.respond(ThumbnailReply::Ready(ThumbnailImage {
                    bytes: thumbnail.clone(),
                    version: request.version,
                }));
            } else {
                waiting
                    .lock()
                    .expect("the waiting thumbnails")
                    .push(responder);
            }
        });
        Self {
            core,
            backend,
            executor: Arc::new(ManualExecutor::new()),
            core_times,
            serve_thumbnails,
            waiting_thumbnails,
        }
    }

    /// From now on every thumbnail request is answered with the picture; the ones
    /// kept waiting are given up (the tiles ask again).
    fn serve_thumbnails(&self) {
        self.serve_thumbnails.store(true, Ordering::SeqCst);
        self.waiting_thumbnails
            .lock()
            .expect("the waiting thumbnails")
            .clear();
    }

    fn parts(&self) -> RuntimeParts {
        RuntimeParts {
            config: RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            },
            backend: self.backend.clone(),
            prefs: Arc::new(MemoryPrefsStore::default()),
            executor: self.executor.clone() as Arc<dyn Executor>,
        }
    }

    /// Puts the asset with ID 1 into a media group of its own, or takes it out again.
    /// Either write changes the order of every query's results, so the next query
    /// start builds its ordered ID list again instead of reusing the cached one.
    fn regroup_first_asset(&self, grouped: bool) {
        self.core
            .backend()
            .set_asset_media_group(
                1,
                grouped.then(|| "million-measurement".to_owned()),
                grouped.then_some(1.0),
            )
            .expect("write a media group");
    }

    /// The core's times since the last call.
    fn take_core_times(&self) -> CoreTimes {
        std::mem::take(&mut *self.core_times.lock().expect("times"))
    }

    /// Prints the core's times since the last call under `label`.
    fn report_core(&self, label: &str) {
        let times = self.take_core_times();
        for (call, samples) in [
            ("start_asset_query", &times.starts),
            ("get_asset_query_page", &times.pages),
            ("get_asset_query_sections", &times.sections),
        ] {
            if !samples.is_empty() {
                println!("[million] {label}: core {call} {}", summary(samples));
            }
        }
    }

    /// Runs the waiting backend calls and returns how many there were and how long
    /// they took together. The fake's record of calls is dropped, so a long run keeps
    /// nothing of it.
    fn run_backend(&self) -> (usize, Duration) {
        let started = Instant::now();
        let ran = self.executor.run_all();
        let elapsed = started.elapsed();
        self.backend.take_calls();
        (ran, elapsed)
    }
}

/// Runs backend calls and lets their replies reach the UI thread until nothing is
/// left. Returns the time spent in the backend calls.
fn settle(fixture: &Fixture, cx: &mut VisualTestContext) -> Duration {
    let mut backend = Duration::ZERO;
    loop {
        cx.run_until_parked();
        let (ran, elapsed) = fixture.run_backend();
        backend += elapsed;
        if ran == 0 {
            break;
        }
    }
    cx.run_until_parked();
    backend
}

/// One frame as the platform's frame loop delivers it: first the callbacks asked for
/// with `on_next_frame` (every animated element asks for one in each layout), then
/// the draw. A test has no frame loop; a draw without the delivery would keep every
/// callback of every frame, which looks like a leak in a long run.
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}

/// Moves both clocks: GPUI's (the fast-scroll pause, animations) and the executor's
/// (the models' timers: retries, debounces, keepalives).
fn advance(fixture: &Fixture, by: Duration, cx: &mut VisualTestContext) {
    cx.executor().advance_clock(by);
    fixture.executor.advance(by);
}

fn runtime(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<Runtime> {
    window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open")
}

fn gallery_view(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<GalleryView> {
    window
        .read_with(cx, |shell, _| shell.gallery().cloned())
        .expect("the window is open")
        .expect("the gallery is shown")
}

/// A wheel step over the middle of the gallery; positive scrolls towards the end.
fn wheel(delta_y: f32, cx: &mut VisualTestContext) {
    let window = cx.update(|window, _| window.viewport_size());
    cx.simulate_event(ScrollEvent {
        position: point(window.width / 2.0, window.height * 0.6),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-delta_y))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
}

fn focus_grid(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) {
    let focus = gallery_view(window, cx).read_with(cx, |gallery, _| gallery.grid_focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));
}

struct Counters {
    total: usize,
    cached_pages: usize,
    cached_rows: usize,
    pictures: usize,
    picture_mib: f64,
    budget_mib: f64,
    /// Pictures the cache dropped whose textures are not released yet.
    retired: usize,
    /// Thumbnails asked for and not answered or decoded yet.
    pending: usize,
}

impl Fixture {
    /// Timers of the models that have not fired yet.
    fn waiting_timers(&self) -> usize {
        self.executor.pending_timers()
    }
}

fn counters(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Counters {
    let runtime = runtime(window, cx);
    runtime.read_with(cx, |runtime, cx| {
        let query = runtime.model().query();
        let thumbnails = runtime.thumbnails().read(cx);
        let mib = |bytes: usize| bytes as f64 / (1024.0 * 1024.0);
        Counters {
            total: query.total(),
            cached_pages: query.cached_pages(),
            cached_rows: query.cached_len(),
            pictures: thumbnails.cached_pictures(),
            picture_mib: mib(thumbnails.cached_bytes()),
            budget_mib: mib(thumbnails.budget()),
            retired: thumbnails.retired_pictures(),
            pending: thumbnails.pending(),
        }
    })
}

fn report_memory(label: &str, window: &WindowHandle<ShellView>, cx: &VisualTestContext) {
    let counters = counters(window, cx);
    println!(
        "[million] memory, {label}: resident {:.0} MiB, held by Rust code {:.0} MiB; {} pages \
         ({} rows) of {} results cached; {} decoded pictures, {:.0} of {:.0} MiB ({} dropped and \
         not released, {} pending)",
        resident_mib(),
        live_mib(),
        counters.cached_pages,
        counters.cached_rows,
        counters.total,
        counters.pictures,
        counters.picture_mib,
        counters.budget_mib,
        counters.retired,
        counters.pending,
    );
}

/// `frames` wheel steps of `step` pixels, 16 ms apart on the test clock. Returns the
/// frame times: each without the time of the backend calls its step started, which
/// the handlers measure themselves ([`Fixture::report_core`]).
fn scroll(
    fixture: &Fixture,
    frames: usize,
    step: f32,
    cx: &mut VisualTestContext,
) -> Vec<Duration> {
    let mut frame_times = Vec::with_capacity(frames);
    for _ in 0..frames {
        advance(fixture, Duration::from_millis(16), cx);
        let started = Instant::now();
        wheel(step, cx);
        cx.run_until_parked();
        let before_backend = started.elapsed();
        fixture.run_backend();
        let resumed = Instant::now();
        cx.run_until_parked();
        draw(cx);
        frame_times.push(before_backend + resumed.elapsed());
    }
    frame_times
}

/// One step that changes what the gallery shows: the time on the UI thread until the
/// frame with the answer is drawn, and the time of the backend calls in between.
fn timed_step(
    fixture: &Fixture,
    cx: &mut VisualTestContext,
    step: impl FnOnce(&mut VisualTestContext),
) -> (Duration, Duration) {
    let started = Instant::now();
    step(cx);
    let backend = settle(fixture, cx);
    // The pause of a far jump ends 150 ms after the last movement.
    advance(fixture, Duration::from_millis(200), cx);
    let backend = backend + settle(fixture, cx);
    draw(cx);
    (started.elapsed().saturating_sub(backend), backend)
}

fn send(window: &WindowHandle<ShellView>, input: RuntimeInput, cx: &mut VisualTestContext) {
    runtime(window, cx).update(cx, |runtime, cx| runtime.send(input, cx));
}

#[gpui::test]
#[ignore = "a measurement; run it on its own, in release mode, with --ignored --nocapture"]
fn scrolling_refreshing_and_memory_on_a_million_results(cx: &mut TestAppContext) {
    let rows = rows();
    if let Some(mib) = std::env::var("MEDIATAGGER_GPUI_PERF_TRACE_MIB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    {
        TRACE_BYTES.store(mib * 1024 * 1024, Ordering::Relaxed);
    }
    let thumbnail = real_thumbnail();
    println!(
        "[million] thumbnail: {} bytes of JPEG for every tile; resident before the library {:.0} MiB",
        thumbnail.len(),
        resident_mib()
    );

    let core = TempCore::start();
    let seeded = core.seed_synthetic_library(rows);
    // The seed runs in this process, so what SQLite allocated for it stays in the
    // resident set: read the later numbers against this one.
    println!(
        "[million] seed: {rows} rows in {:.1} s; resident after the seed {:.0} MiB, held by Rust code {:.0} MiB",
        seeded.as_secs_f64(),
        resident_mib(),
        live_mib()
    );
    let fixture = Fixture::new(core, thumbnail);

    // Opening: the first query builds the ordered ID list of the whole library.
    let started = Instant::now();
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window(profile::E2E, fixture.parts(), cx)
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let backend = settle(&fixture, &mut cx);
    draw(&mut cx);
    println!(
        "[million] open: first frame with tiles after {:.1} ms on the UI thread and {:.1} ms in the backend",
        ms(started.elapsed().saturating_sub(backend)),
        ms(backend)
    );
    fixture.report_core("open");
    assert_eq!(
        counters(&window, &cx).total,
        usize::try_from(rows).expect("a positive row count"),
        "the gallery shows the seeded library"
    );
    report_memory("after opening", &window, &cx);

    // Scrolling, twice: with every thumbnail still on its way (the frame alone: rows,
    // tiles and their placeholders), then with the picture answered at once. GPUI's
    // test platform runs background tasks on the test's thread, so the second pass
    // has the JPEG decodes in its frame times; the application decodes off the UI
    // thread.
    for pictures in [false, true] {
        if pictures {
            fixture.serve_thumbnails();
        }
        let pass = if pictures {
            "pictures decoded on the test thread"
        } else {
            "thumbnails pending"
        };
        for (name, tile_size, step, frames) in [
            (
                "default tiles (188 px), 48 px a frame",
                188.0,
                48.0_f32,
                600,
            ),
            ("smallest tiles (140 px), 48 px a frame", 140.0, 48.0, 600),
            ("smallest tiles, 900 px a frame", 140.0, 900.0, 600),
        ] {
            send(&window, RuntimeInput::SetTileSize(tile_size), &mut cx);
            focus_grid(&window, &mut cx);
            cx.simulate_keystrokes("home");
            advance(&fixture, Duration::from_millis(200), &mut cx);
            settle(&fixture, &mut cx);
            fixture.take_core_times();
            let frame_times = scroll(&fixture, frames, step, &mut cx);
            let tiles = gallery_view(&window, &cx)
                .read_with(&cx, |gallery, _| gallery.grid().range())
                .map(|range| range.end_index - range.start_index + 1)
                .unwrap_or_default();
            println!(
                "[million] scroll, {pass}, {name}, {tiles} tiles rendered: frame {}",
                summary(&frame_times)
            );
            fixture.report_core(&format!("scroll, {pass}, {name}"));
        }
    }
    report_memory("after the scroll measurements", &window, &cx);

    // Far jumps: the end of the library and back, each from a cache without the target.
    let mut to_end = Vec::new();
    let mut to_start = Vec::new();
    for _ in 0..5 {
        focus_grid(&window, &mut cx);
        to_end.push(timed_step(&fixture, &mut cx, |cx| {
            cx.simulate_keystrokes("end")
        }));
        // Twenty-four pages further on, the first page is no longer cached.
        scroll(&fixture, 40, -40_000.0, &mut cx);
        to_start.push(timed_step(&fixture, &mut cx, |cx| {
            cx.simulate_keystrokes("home")
        }));
        scroll(&fixture, 40, 40_000.0, &mut cx);
    }
    for (name, samples) in [("End", &to_end), ("Home", &to_start)] {
        let (ui, backend): (Vec<_>, Vec<_>) = samples.iter().copied().unzip();
        println!("[million] jump, {name}: UI thread {}", summary(&ui));
        println!("[million] jump, {name}: backend {}", summary(&backend));
    }
    fixture.report_core("jumps and the scrolling between them");

    // Query refresh: the same filters again (the Refresh button) with nothing changed,
    // then after a write that moved the library revision, which makes the core build
    // the ordered ID list again as the grid baseline does for every sample; then each
    // filter of the grid baseline as a new query from the unfiltered one.
    focus_grid(&window, &mut cx);
    cx.simulate_keystrokes("home");
    advance(&fixture, Duration::from_millis(200), &mut cx);
    settle(&fixture, &mut cx);
    let mut refresh = Vec::new();
    for _ in 0..5 {
        refresh.push(timed_step(&fixture, &mut cx, |cx| {
            send(&window, RuntimeInput::RefreshLibrary, cx)
        }));
    }
    let (ui, backend): (Vec<_>, Vec<_>) = refresh.iter().copied().unzip();
    println!(
        "[million] refresh, nothing changed: UI thread {}",
        summary(&ui)
    );
    println!(
        "[million] refresh, nothing changed: backend {}",
        summary(&backend)
    );
    fixture.report_core("refresh, nothing changed");

    let mut refresh = Vec::new();
    for sample in 0..5 {
        // A media group written through the core, outside the measured step: it
        // changes the order of the results, so the core builds the list again.
        fixture.regroup_first_asset(sample % 2 == 0);
        refresh.push(timed_step(&fixture, &mut cx, |cx| {
            send(&window, RuntimeInput::RefreshLibrary, cx)
        }));
    }
    let (ui, backend): (Vec<_>, Vec<_>) = refresh.iter().copied().unzip();
    println!(
        "[million] refresh, after a write (ID list rebuilt): UI thread {}",
        summary(&ui)
    );
    println!(
        "[million] refresh, after a write (ID list rebuilt): backend {}",
        summary(&backend)
    );
    fixture.report_core("refresh, after a write (ID list rebuilt)");

    type Apply = fn() -> SearchCommand;
    let filters: [(&str, Apply); 5] = [
        ("favorites", || SearchCommand::ApplyFavorites(true)),
        ("tag_common", || SearchCommand::ApplyInput("common".into())),
        ("tag_rare", || SearchCommand::ApplyInput("rare".into())),
        ("exclude_common", || {
            SearchCommand::ApplyInput("-common".into())
        }),
        ("kind_video", || {
            SearchCommand::ApplyKind(Some(MediaKind::Video))
        }),
    ];
    for (name, apply) in filters {
        let mut samples = Vec::new();
        let mut total = 0;
        let mut starts = Vec::new();
        for sample in 0..3 {
            // Each sample starts from a moved revision, as in the grid baseline.
            fixture.regroup_first_asset(sample % 2 == 0);
            fixture.take_core_times();
            samples.push(timed_step(&fixture, &mut cx, |cx| {
                send(&window, RuntimeInput::Search(apply()), cx)
            }));
            starts.extend(fixture.take_core_times().starts);
            total = counters(&window, &cx).total;
            timed_step(&fixture, &mut cx, |cx| {
                send(&window, RuntimeInput::Search(SearchCommand::Clear), cx)
            });
        }
        fixture.take_core_times();
        let (ui, backend): (Vec<_>, Vec<_>) = samples.iter().copied().unzip();
        println!(
            "[million] query, {name}, {total} results: UI thread {}",
            summary(&ui)
        );
        println!(
            "[million] query, {name}, {total} results: backend {}",
            summary(&backend)
        );
        println!(
            "[million] query, {name}, {total} results: core start_asset_query {}",
            summary(&starts)
        );
    }
    report_memory("after the queries", &window, &cx);

    // A long session: the same tour again and again. Each tour scrolls 3,000 frames at
    // reading speed, 1,000 fast ones, jumps to the end and back and runs every filter
    // once. Memory that keeps growing from tour to tour is a leak; caches that fill
    // once are not.
    for tour in 1..=tours() {
        send(&window, RuntimeInput::SetTileSize(188.0), &mut cx);
        focus_grid(&window, &mut cx);
        scroll(&fixture, 3_000, 48.0, &mut cx);
        send(&window, RuntimeInput::SetTileSize(140.0), &mut cx);
        scroll(&fixture, 1_000, 900.0, &mut cx);
        timed_step(&fixture, &mut cx, |cx| cx.simulate_keystrokes("end"));
        scroll(&fixture, 500, -900.0, &mut cx);
        timed_step(&fixture, &mut cx, |cx| cx.simulate_keystrokes("home"));
        for (_, apply) in filters {
            timed_step(&fixture, &mut cx, |cx| {
                send(&window, RuntimeInput::Search(apply()), cx)
            });
            scroll(&fixture, 100, 48.0, &mut cx);
            timed_step(&fixture, &mut cx, |cx| {
                send(&window, RuntimeInput::Search(SearchCommand::Clear), cx)
            });
        }
        report_memory(
            &format!("after tour {tour} of the long session"),
            &window,
            &cx,
        );
        println!(
            "[million] timers, after tour {tour} of the long session: {} waiting on the executor",
            fixture.waiting_timers()
        );
    }

    // The profile is removed with the fixture, after the window that reads it.
    drop(cx);
    drop(fixture.core);
}
