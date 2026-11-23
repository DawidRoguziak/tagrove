//! The main window and the GPUI event loop.

use std::{
    cell::RefCell,
    error::Error,
    ffi::{OsStr, OsString},
    fmt::{Display, Formatter},
    net::TcpStream,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    rc::Rc,
};

use gpui::{
    point, px, size, App, AppContext as _, Application, Bounds, Pixels, QuitMode, Size,
    TitlebarOptions, WindowBounds, WindowDecorations, WindowHandle, WindowOptions,
};
use media_tagger_client::{
    runtime::{RuntimeInput, DEFAULT_TILE_SIZE},
    scenario::{ScenarioClick, ScenarioDock, ScenarioLightbox, ScenarioSearch, ScenarioTagList},
    theme::ThemeName,
};

use crate::{
    profile::Profile,
    runtime::{Runtime, RuntimeParts},
    ui,
    views::{self, kit::KitView, shell::ShellView},
};

/// What a comparison scenario presets in the shell; nothing of it is a stored
/// preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellStart {
    /// The tile size the gallery starts with.
    pub tile_size: u32,
    /// The clicks the gallery starts with, in selection mode (`Scenario::selection`).
    pub selection: &'static [ScenarioClick],
    /// The search header the gallery starts with (`Scenario::search`).
    pub search: Option<ScenarioSearch>,
    /// The tag list, open with its choices (`Scenario::tag_list`).
    pub tag_list: Option<ScenarioTagList>,
    /// What happens in the bulk dock once the clicks are made (`Scenario::dock`).
    pub dock: Option<ScenarioDock>,
    /// The lightbox, open on a tile (`Scenario::lightbox`).
    pub lightbox: Option<ScenarioLightbox>,
}

impl Default for ShellStart {
    /// The application's own start: the default tile size, selection mode off.
    fn default() -> Self {
        Self {
            tile_size: DEFAULT_TILE_SIZE,
            selection: &[],
            search: None,
            tag_list: None,
            dock: None,
            lightbox: None,
        }
    }
}

/// What the main window shows. Made once per process, so the size of the shell's
/// start state does not matter.
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum MainScreen {
    /// The application: the shell over the runtime made of these parts, started as
    /// the scenario asks.
    Shell(RuntimeParts, ShellStart),
    /// The board of every shared control (`--scenario kit`).
    Kit,
}

/// Initial and minimum size of the main window, as in `src-tauri/tauri.conf.json`.
pub const WINDOW_SIZE: (f32, f32) = (1440.0, 900.0);
pub const WINDOW_MIN_SIZE: (f32, f32) = (1000.0, 720.0);

fn pixels((width, height): (f32, f32)) -> Size<Pixels> {
    size(px(width), px(height))
}

/// Why no window can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoDisplay {
    /// The environment names no display server.
    Unset,
    /// The environment names one, and nothing answers there.
    Unreachable,
}

impl Display for NoDisplay {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unset => "no display server: neither WAYLAND_DISPLAY nor DISPLAY is set",
            Self::Unreachable => {
                "no display server: the one WAYLAND_DISPLAY or DISPLAY names does not answer"
            }
        })
    }
}

impl Error for NoDisplay {}

/// The variables GPUI's Linux platform reads to find a display server.
#[derive(Debug, Clone, Default)]
pub struct DisplayEnvironment {
    pub wayland_display: Option<OsString>,
    /// A descriptor the parent process already connected to its compositor.
    pub wayland_socket: Option<OsString>,
    pub xdg_runtime_dir: Option<OsString>,
    pub x11_display: Option<OsString>,
}

/// The display server the main window opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    Wayland,
    X11,
}

/// Where libxcb looks for the socket of the local display `:N`: the file `X<N>` here,
/// and on Linux first the abstract socket of the same name.
const X11_SOCKET_DIR: &str = "/tmp/.X11-unix";
const X11_TCP_PORT: u16 = 6000;

/// Picks the display server the window opens on, in GPUI's order (Wayland, then X11),
/// before anything opens the profile.
///
/// GPUI itself only looks at whether the variables are set: with none it starts headless
/// and waits forever without a window to close, and with `WAYLAND_DISPLAY` set it never
/// tries X11 and panics when no compositor answers. So each named server is probed with
/// one socket connection, closed again at once, and GPUI is started on the first that
/// answers: a stale `WAYLAND_DISPLAY` next to a live `DISPLAY` opens the window on X11.
///
/// What cannot be probed counts as answering, so this never refuses a display GPUI
/// could have opened: a compositor inherited through `WAYLAND_SOCKET`, and a `DISPLAY`
/// that is not a local `:N`. A server that stops between the probe and GPUI's own
/// connection still ends in GPUI's panic.
pub fn require_display(environment: &DisplayEnvironment) -> Result<DisplayServer, NoDisplay> {
    select_display(environment, Path::new(X11_SOCKET_DIR))
}

fn select_display(
    environment: &DisplayEnvironment,
    x11_socket_dir: &Path,
) -> Result<DisplayServer, NoDisplay> {
    let wayland_display = named(&environment.wayland_display);
    let x11_display = named(&environment.x11_display);
    if wayland_display.is_none() && x11_display.is_none() {
        return Err(NoDisplay::Unset);
    }
    let wayland_answers = wayland_display.is_some_and(|display| {
        named(&environment.wayland_socket).is_some()
            || wayland_socket_path(display, named(&environment.xdg_runtime_dir))
                .is_some_and(|socket| UnixStream::connect(socket).is_ok())
    });
    if wayland_answers {
        Ok(DisplayServer::Wayland)
    } else if x11_display.is_some_and(|display| x11_answers(display, x11_socket_dir)) {
        Ok(DisplayServer::X11)
    } else {
        Err(NoDisplay::Unreachable)
    }
}

/// A variable counts as set when it is not empty, as in GPUI.
fn named(value: &Option<OsString>) -> Option<&OsStr> {
    value.as_deref().filter(|value| !value.is_empty())
}

/// The socket `WAYLAND_DISPLAY` names, as GPUI and libwayland resolve it: an absolute
/// path, or a name inside `XDG_RUNTIME_DIR`.
fn wayland_socket_path(display: &OsStr, runtime_dir: Option<&OsStr>) -> Option<PathBuf> {
    let display = Path::new(display);
    if display.is_absolute() {
        Some(display.to_path_buf())
    } else {
        runtime_dir.map(|directory| Path::new(directory).join(display))
    }
}

/// The number of the local display `:N` or `:N.S`. Every other form (a host, a
/// transport prefix, a socket path) is left to libxcb.
fn local_x11_display(display: &str) -> Option<u32> {
    let is_number = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    let display = display.strip_prefix(':')?;
    let (number, screen) = match display.split_once('.') {
        Some((number, screen)) => (number, Some(screen)),
        None => (display, None),
    };
    if !is_number(number) || !screen.is_none_or(is_number) {
        return None;
    }
    number.parse().ok()
}

/// Whether the X server `display` names answers, trying what libxcb tries for a local
/// display: the abstract socket, the socket file, then TCP on localhost.
fn x11_answers(display: &OsStr, socket_dir: &Path) -> bool {
    // GPUI refuses a display name that is not UTF-8.
    let Some(display) = display.to_str() else {
        return false;
    };
    let Some(number) = local_x11_display(display) else {
        return true;
    };
    let socket = socket_dir.join(format!("X{number}"));
    abstract_socket_answers(&socket)
        || UnixStream::connect(&socket).is_ok()
        || u16::try_from(number)
            .ok()
            .and_then(|number| X11_TCP_PORT.checked_add(number))
            .is_some_and(|port| TcpStream::connect(("localhost", port)).is_ok())
}

#[cfg(target_os = "linux")]
fn abstract_socket_answers(name: &Path) -> bool {
    use std::os::{
        linux::net::SocketAddrExt as _,
        unix::{ffi::OsStrExt as _, net::SocketAddr},
    };

    SocketAddr::from_abstract_name(name.as_os_str().as_bytes())
        .and_then(|address| UnixStream::connect_addr(&address))
        .is_ok()
}

#[cfg(not(target_os = "linux"))]
fn abstract_socket_answers(_name: &Path) -> bool {
    false
}

/// How far GPUI's X11 backend moves a new window right of the origin it was asked for:
/// it creates every window at `x + 2` (`gpui_linux` `X11Window::new`). Uncorrected, a
/// window as wide as the screen leaves a 2 px strip of the root window at its left edge
/// and loses its last 2 px past the right one. The shift is in device pixels and the
/// scale factor is not known before the window exists, so on a scaled X11 screen the
/// correction is off by the difference (the window starts a pixel or two left of centre).
pub const X11_CREATE_SHIFT: f32 = 2.0;

/// The bounds the main window asks for: centred on the primary display, as the Tauri
/// window (`"center": true`), with GPUI's X11 shift taken back. `compositor` is
/// [`App::compositor_name`]; other servers place the window themselves and keep the
/// centred bounds.
pub fn main_window_bounds(centered: Bounds<Pixels>, compositor: &str) -> Bounds<Pixels> {
    if compositor != "X11" {
        return centered;
    }
    Bounds::new(
        point(centered.origin.x - px(X11_CREATE_SHIFT), centered.origin.y),
        centered.size,
    )
}

/// Title and application id come from the profile, so the two profiles and the Tauri
/// application stay distinguishable on screen and to the window manager. The window
/// draws its own decorations (plan decision D14), as the frameless Tauri window does;
/// the title still names it to the window manager.
pub fn main_window_options(profile: Profile, cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(main_window_bounds(
            Bounds::centered(None, pixels(WINDOW_SIZE), cx),
            cx.compositor_name(),
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(profile.window_title.into()),
            appears_transparent: true,
            ..Default::default()
        }),
        app_id: Some(profile.identifier.to_owned()),
        window_min_size: Some(pixels(WINDOW_MIN_SIZE)),
        window_decorations: Some(WindowDecorations::Client),
        ..Default::default()
    }
}

/// Opens the main window: the shell over a runtime made of `parts`, which starts once
/// the window is open (`onSettled(runtime.start)`). The runtime lives as long as the
/// window.
pub fn open_main_window(
    profile: Profile,
    parts: RuntimeParts,
    cx: &mut App,
) -> gpui::Result<WindowHandle<ShellView>> {
    open_main_window_started(profile, parts, ShellStart::default(), cx)
}

/// [`open_main_window`] in the state a comparison scenario starts with: its tile
/// size, and its selection once the tiles are loaded.
pub fn open_main_window_started(
    profile: Profile,
    parts: RuntimeParts,
    start: ShellStart,
    cx: &mut App,
) -> gpui::Result<WindowHandle<ShellView>> {
    let options = main_window_options(profile, cx);
    let runtime = cx.new(|cx| Runtime::new(parts, cx));
    runtime.update(cx, |runtime, cx| {
        runtime.send(RuntimeInput::SetTileSize(f64::from(start.tile_size)), cx);
        runtime.script_selection(start.selection, cx);
        if let Some(search) = start.search {
            runtime.script_search(search, cx);
        }
        if let Some(tag_list) = start.tag_list {
            runtime.script_tag_list(tag_list, cx);
        }
        if let Some(dock) = start.dock {
            runtime.script_dock(dock, cx);
        }
        if let Some(lightbox) = start.lightbox {
            runtime.script_lightbox(lightbox, cx);
        }
    });
    let window = cx.open_window(options, |window, cx| {
        cx.new(|cx| ShellView::new(runtime.clone(), window, cx))
    })?;
    runtime.update(cx, |runtime, cx| runtime.send(RuntimeInput::Start, cx));
    Ok(window)
}

/// Opens the `kit` board.
pub fn open_kit_window(profile: Profile, cx: &mut App) -> gpui::Result<WindowHandle<KitView>> {
    let options = main_window_options(profile, cx);
    cx.open_window(options, |window, cx| cx.new(|cx| KitView::new(window, cx)))
}

/// A GPUI application allowed to use only `display`: it never falls back to headless
/// mode, and never picks a display server that [`require_display`] found silent. Its
/// assets are the icons of `ui::assets`.
fn application(display: DisplayServer) -> Application {
    #[cfg(target_os = "linux")]
    let application = gpui_platform::linux(match display {
        DisplayServer::Wayland => gpui::WindowingModes::WAYLAND,
        DisplayServer::X11 => gpui::WindowingModes::X11,
    });
    #[cfg(not(target_os = "linux"))]
    let application = {
        let _ = display;
        gpui_platform::application()
    };
    application.with_assets(ui::assets::Assets)
}

/// Prepares the application before its first window: fonts, the building blocks of
/// `ui` and the start theme.
pub fn init_application(theme: ThemeName, cx: &mut App) -> gpui::Result<()> {
    ui::fonts::register(cx)?;
    ui::init(cx);
    views::init(cx);
    ui::theme::set_active(theme, cx);
    Ok(())
}

/// Opens the main window on `display` and runs the event loop until the last window has
/// closed.
pub fn run_until_closed(
    profile: Profile,
    theme: ThemeName,
    screen: MainScreen,
    display: DisplayServer,
) -> Result<(), Box<dyn Error>> {
    run_application(application(display), move |cx| {
        init_application(theme, cx)?;
        match screen {
            MainScreen::Shell(parts, start) => {
                open_main_window_started(profile, parts, start, cx).map(|_| ())
            }
            MainScreen::Kit => open_kit_window(profile, cx).map(|_| ()),
        }
    })
}

/// Runs `application` until the last window `open` opened has closed, or returns the
/// error of an `open` that failed.
fn run_application(
    application: Application,
    open: impl FnOnce(&mut App) -> gpui::Result<()> + 'static,
) -> Result<(), Box<dyn Error>> {
    let failure = Rc::new(RefCell::new(None::<String>));
    let reported = failure.clone();
    application
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx: &mut App| match open(cx) {
            Ok(()) => cx.activate(true),
            Err(error) => {
                *reported.borrow_mut() = Some(format!("cannot open the main window: {error:#}"));
                // GPUI calls this closure before it starts the event loop, and the loop
                // forgets a quit requested before it runs. With no window to close
                // nothing else would end it, so the quit is requested from a task,
                // which the loop itself runs.
                cx.spawn(async |cx| cx.update(|cx| cx.quit())).detach();
            }
        });
    match failure.take() {
        Some(message) => Err(message.into()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::net::{UnixDatagram, UnixListener},
        path::{Path, PathBuf},
    };

    use tempfile::{tempdir, TempDir};

    use gpui::{point, px, size, Bounds};

    use super::{
        local_x11_display, main_window_bounds, require_display, select_display, DisplayEnvironment,
        DisplayServer, NoDisplay, X11_CREATE_SHIFT,
    };

    /// A display number without a TCP port (6000 + N does not fit), so only the sockets
    /// in the test's own directory decide.
    const X11_NUMBER: u32 = 65_000;
    const X11_DISPLAY: &str = ":65000";

    fn os(value: impl Into<OsString>) -> Option<OsString> {
        Some(value.into())
    }

    /// A directory for display sockets and a listening socket at `name` in it.
    fn listening(name: &str) -> (TempDir, PathBuf, UnixListener) {
        let dir = tempdir().expect("tempdir");
        let socket = dir.path().join(name);
        let listener = UnixListener::bind(&socket).expect("bind the display socket");
        (dir, socket, listener)
    }

    /// A directory for display sockets and, at `name` in it, the socket file a server
    /// that is gone left behind: nothing accepts a connection there.
    ///
    /// The file comes from a datagram socket, which never accepts one. Closing a
    /// listening socket would not do: a child process another test starts at that moment
    /// holds a copy of the listener from its fork until its exec, and connections
    /// succeed for that long.
    fn stale(name: &str) -> (TempDir, PathBuf) {
        let dir = tempdir().expect("tempdir");
        let socket = dir.path().join(name);
        drop(UnixDatagram::bind(&socket).expect("bind the stale socket"));
        assert!(socket.exists(), "the socket file is left behind");
        (dir, socket)
    }

    fn x11_socket_name() -> String {
        format!("X{X11_NUMBER}")
    }

    #[test]
    fn no_display_variable_is_refused_without_probing() {
        let empty = DisplayEnvironment {
            wayland_display: os(""),
            x11_display: os(""),
            ..Default::default()
        };

        assert_eq!(
            require_display(&DisplayEnvironment::default()),
            Err(NoDisplay::Unset)
        );
        assert_eq!(require_display(&empty), Err(NoDisplay::Unset));
        assert_eq!(
            NoDisplay::Unset.to_string(),
            "no display server: neither WAYLAND_DISPLAY nor DISPLAY is set"
        );
    }

    #[test]
    fn a_listening_wayland_socket_is_found_by_path_and_by_name() {
        let (dir, socket, _listener) = listening("wayland-test");
        let by_path = DisplayEnvironment {
            wayland_display: os(&socket),
            ..Default::default()
        };
        let by_name = DisplayEnvironment {
            wayland_display: os("wayland-test"),
            xdg_runtime_dir: os(dir.path()),
            ..Default::default()
        };

        assert_eq!(
            select_display(&by_path, dir.path()),
            Ok(DisplayServer::Wayland)
        );
        assert_eq!(
            select_display(&by_name, dir.path()),
            Ok(DisplayServer::Wayland)
        );
    }

    #[test]
    fn a_wayland_display_that_does_not_answer_is_refused() {
        // The compositor is gone and its socket file is left behind.
        let (dir, socket) = stale("wayland-test");
        let stale = DisplayEnvironment {
            wayland_display: os(&socket),
            ..Default::default()
        };
        let missing = DisplayEnvironment {
            wayland_display: os("wayland-missing"),
            xdg_runtime_dir: os(dir.path()),
            ..Default::default()
        };
        let without_runtime_dir = DisplayEnvironment {
            wayland_display: os("wayland-test"),
            ..Default::default()
        };

        for environment in [&stale, &missing, &without_runtime_dir] {
            assert_eq!(
                select_display(environment, dir.path()),
                Err(NoDisplay::Unreachable),
                "{environment:?}"
            );
        }
        assert_eq!(
            NoDisplay::Unreachable.to_string(),
            "no display server: the one WAYLAND_DISPLAY or DISPLAY names does not answer"
        );
    }

    #[test]
    fn a_dead_wayland_display_falls_back_to_a_live_x11_display() {
        let (dir, _socket, _listener) = listening(&x11_socket_name());
        let environment = DisplayEnvironment {
            wayland_display: os(dir.path().join("wayland-missing")),
            x11_display: os(X11_DISPLAY),
            ..Default::default()
        };

        assert_eq!(
            select_display(&environment, dir.path()),
            Ok(DisplayServer::X11)
        );
    }

    #[test]
    fn wayland_wins_when_both_displays_answer() {
        let (dir, _x11_socket, _x11_listener) = listening(&x11_socket_name());
        let wayland_socket = dir.path().join("wayland-test");
        let _wayland_listener = UnixListener::bind(&wayland_socket).expect("bind");
        let environment = DisplayEnvironment {
            wayland_display: os(&wayland_socket),
            x11_display: os(X11_DISPLAY),
            ..Default::default()
        };

        assert_eq!(
            select_display(&environment, dir.path()),
            Ok(DisplayServer::Wayland)
        );
    }

    #[test]
    fn a_local_x11_display_must_answer() {
        let (dir, _socket, _listener) = listening(&x11_socket_name());
        let with_screen = format!("{X11_DISPLAY}.0");

        for display in [X11_DISPLAY, with_screen.as_str()] {
            let environment = DisplayEnvironment {
                x11_display: os(display),
                ..Default::default()
            };
            assert_eq!(
                select_display(&environment, dir.path()),
                Ok(DisplayServer::X11),
                "{display}"
            );
        }

        // The X server is gone and its socket file is left behind.
        let (stale_dir, _stale_socket) = stale(&x11_socket_name());
        let environment = DisplayEnvironment {
            x11_display: os(X11_DISPLAY),
            ..Default::default()
        };
        assert_eq!(
            select_display(&environment, stale_dir.path()),
            Err(NoDisplay::Unreachable)
        );
        assert_eq!(
            select_display(&environment, Path::new("/nonexistent/x11-sockets")),
            Err(NoDisplay::Unreachable)
        );
    }

    #[test]
    fn displays_that_cannot_be_probed_are_left_to_gpui() {
        let dir = tempdir().expect("tempdir");
        let inherited = DisplayEnvironment {
            wayland_display: os(dir.path().join("wayland-missing")),
            wayland_socket: os("3"),
            ..Default::default()
        };
        assert_eq!(
            select_display(&inherited, dir.path()),
            Ok(DisplayServer::Wayland)
        );

        for display in ["remote.invalid:10.0", "unix:0", "tcp/remote.invalid:0"] {
            let environment = DisplayEnvironment {
                x11_display: os(display),
                ..Default::default()
            };
            assert_eq!(
                select_display(&environment, dir.path()),
                Ok(DisplayServer::X11),
                "{display}"
            );
        }
    }

    #[test]
    fn the_x11_create_shift_is_taken_back_and_nothing_else_moves() {
        let centered = Bounds::new(point(px(0.0), px(0.0)), size(px(1440.0), px(900.0)));

        let x11 = main_window_bounds(centered, "X11");
        assert_eq!(x11.origin, point(px(-X11_CREATE_SHIFT), px(0.0)));
        assert_eq!(x11.size, centered.size);
        // GPUI adds the shift back when it creates the window: the left edge lands on 0.
        assert_eq!(x11.origin.x + px(X11_CREATE_SHIFT), centered.origin.x);
        for compositor in ["Wayland", ""] {
            assert_eq!(
                main_window_bounds(centered, compositor),
                centered,
                "{compositor:?}"
            );
        }
    }

    #[test]
    fn only_a_bare_display_number_is_local() {
        assert_eq!(local_x11_display(":0"), Some(0));
        assert_eq!(local_x11_display(":99.1"), Some(99));
        for display in [
            "",
            ":",
            ":x",
            ":1.",
            ":1.x",
            "unix:1",
            "host:1",
            "/tmp/socket",
        ] {
            assert_eq!(local_x11_display(display), None, "{display:?}");
        }
    }

    /// The real Linux event loop, headless: no display server, no window, no app data.
    /// It runs in a child process, as GPUI's own platform tests do, because creating the
    /// platform changes the process environment, and so that a loop that never ends can
    /// be stopped.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_window_that_cannot_open_ends_the_event_loop_with_its_error() {
        use std::{
            process::{Command, Stdio},
            time::{Duration, Instant},
        };

        const NAME: &str =
            "window::tests::a_window_that_cannot_open_ends_the_event_loop_with_its_error";
        /// Set in the child process, which runs the event loop.
        const CHILD: &str = "MEDIATAGGER_GPUI_TEST_FAILED_OPEN_CHILD";
        /// Far above what a headless start takes; a lost quit never returns.
        const TIMEOUT: Duration = Duration::from_secs(30);

        if std::env::var_os(CHILD).is_some() {
            let application = gpui_platform::linux(gpui::WindowingModes::HEADLESS);

            let outcome = super::run_application(application, |_| {
                Err(std::io::Error::other("no usable adapter").into())
            });

            assert_eq!(
                outcome
                    .expect_err("the failed open is reported")
                    .to_string(),
                "cannot open the main window: no usable adapter"
            );
            return;
        }

        let mut child = Command::new(std::env::current_exe().expect("the test executable"))
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, "1")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env_remove("DISPLAY")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run the event loop in a child process");
        let deadline = Instant::now() + TIMEOUT;
        while child.try_wait().expect("poll the child").is_none() {
            if Instant::now() >= deadline {
                child.kill().expect("stop the child");
                child.wait().expect("reap the child");
                panic!("the event loop was still running {TIMEOUT:?} after the failed open");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().expect("read the child's output");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "stdout:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
