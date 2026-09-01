use std::{
    cell::{Cell, RefCell},
    ffi::{c_char, c_int, c_void, CStr, CString},
    ptr::NonNull,
    rc::Rc,
    sync::{mpsc, OnceLock},
    time::Duration,
};

use gtk::{glib, prelude::*};
use libloading::Library;
use libmpv2_sys as mpv_sys;
use tauri::WebviewWindow;

use crate::{
    commands::video::{VideoBounds, VideoControlLabels},
    services::video_player_service::{PlayerCommand, VideoPlayerService},
};

const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_DRAW_FRAMEBUFFER: u32 = 0x8CA9;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_NO_ERROR: u32 = 0;

thread_local! {
    static SURFACE: RefCell<Option<VideoSurface>> = const { RefCell::new(None) };
}

struct VideoSurface {
    fixed: gtk::Fixed,
    gl_area: gtk::GLArea,
    controls: NativeVideoControls,
    sessions: SurfaceSessions,
    _overlay: gtk::Overlay,
    _css_provider: gtk::CssProvider,
    render_context: Rc<RefCell<RenderContextState>>,
    _render_source: glib::SourceId,
    _controls_source: glib::SourceId,
}

#[derive(Clone)]
struct NativeVideoControls {
    root: gtk::EventBox,
    play_button: gtk::Button,
    play_icon: gtk::Image,
    mute_button: gtk::Button,
    mute_icon: gtk::Image,
    time_label: gtk::Label,
    seek: gtk::Scale,
    rate_button: gtk::MenuButton,
    rate_popover: gtk::Popover,
    fullscreen_button: gtk::Button,
    fullscreen_icon: gtk::Image,
    session_id: Rc<Cell<Option<u64>>>,
    labels: Rc<RefCell<VideoControlLabels>>,
}

const NATIVE_CONTROLS_HEIGHT: i32 = 68;

#[derive(Debug, PartialEq, Eq)]
struct NativeControlsBounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

const NATIVE_CONTROLS_CSS: &[u8] = br#"
.media-tagger-video-controls {
  background-image: linear-gradient(to bottom, rgba(8, 9, 10, 0), rgba(8, 9, 10, 0.82));
  padding: 18px 12px 8px 12px;
}
.media-tagger-video-controls button,
.media-tagger-video-controls menubutton > button {
  min-width: 34px;
  min-height: 34px;
  padding: 4px;
  border: 0;
  border-radius: 17px;
  color: #f4f5f0;
  background: transparent;
  box-shadow: none;
}
.media-tagger-video-controls button:hover,
.media-tagger-video-controls menubutton > button:hover {
  background: rgba(255, 255, 255, 0.14);
}
.media-tagger-video-controls button:focus,
.media-tagger-video-controls menubutton > button:focus {
  box-shadow: inset 0 0 0 2px rgba(158, 226, 163, 0.92);
}
.media-tagger-video-time {
  color: rgba(244, 245, 240, 0.9);
  font-size: 12px;
}
.media-tagger-video-seek trough {
  min-height: 4px;
  border: 0;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.28);
}
.media-tagger-video-seek highlight {
  border: 0;
  border-radius: 2px;
  background: #90d596;
}
.media-tagger-video-seek slider {
  min-width: 13px;
  min-height: 13px;
  margin: -5px;
  border: 0;
  border-radius: 7px;
  background: #f4f5f0;
  box-shadow: none;
}
.media-tagger-video-rate-popover {
  padding: 6px;
  background: #17191b;
}
.media-tagger-video-rate-popover button {
  min-width: 72px;
  min-height: 30px;
  color: #f4f5f0;
  background: transparent;
}
"#;

enum RenderContextState {
    Pending,
    Ready(MpvRenderContext),
    Failed(String),
}

struct MpvRenderContext {
    context: NonNull<mpv_sys::mpv_render_context>,
    _update_sender: Box<async_channel::Sender<()>>,
}

impl MpvRenderContext {
    fn new(
        mpv: &'static libmpv2::Mpv,
        update_sender: async_channel::Sender<()>,
    ) -> Result<Self, String> {
        let mut init_params = mpv_sys::mpv_opengl_init_params {
            get_proc_address: Some(mpv_get_proc_address),
            get_proc_address_ctx: std::ptr::null_mut(),
        };
        let mut params = [
            mpv_sys::mpv_render_param {
                type_: mpv_sys::mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                data: mpv_sys::MPV_RENDER_API_TYPE_OPENGL
                    .as_ptr()
                    .cast_mut()
                    .cast(),
            },
            mpv_sys::mpv_render_param {
                type_: mpv_sys::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: (&mut init_params as *mut mpv_sys::mpv_opengl_init_params).cast(),
            },
            mpv_sys::mpv_render_param {
                type_: mpv_sys::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let mut context = std::ptr::null_mut();
        let result = unsafe {
            mpv_sys::mpv_render_context_create(&mut context, mpv.ctx.as_ptr(), params.as_mut_ptr())
        };
        if result < 0 {
            return Err(format!(
                "libmpv render context initialization failed: {}",
                mpv_error_message(result)
            ));
        }
        let context = NonNull::new(context)
            .ok_or_else(|| "libmpv returned a null render context".to_string())?;
        let update_sender = Box::new(update_sender);
        unsafe {
            mpv_sys::mpv_render_context_set_update_callback(
                context.as_ptr(),
                Some(render_update_callback),
                (&*update_sender as *const async_channel::Sender<()>)
                    .cast_mut()
                    .cast(),
            );
        }
        Ok(Self {
            context,
            _update_sender: update_sender,
        })
    }

    fn update(&self) -> u64 {
        unsafe { mpv_sys::mpv_render_context_update(self.context.as_ptr()) }
    }

    fn render(&self, fbo: i32, width: i32, height: i32) -> Result<(), String> {
        let mut fbo = mpv_sys::mpv_opengl_fbo {
            fbo,
            w: width,
            h: height,
            internal_format: 0,
        };
        let mut flip_y: c_int = 1;
        let mut params = [
            mpv_sys::mpv_render_param {
                type_: mpv_sys::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                data: (&mut fbo as *mut mpv_sys::mpv_opengl_fbo).cast(),
            },
            mpv_sys::mpv_render_param {
                type_: mpv_sys::mpv_render_param_type_MPV_RENDER_PARAM_FLIP_Y,
                data: (&mut flip_y as *mut c_int).cast(),
            },
            mpv_sys::mpv_render_param {
                type_: mpv_sys::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let result = unsafe {
            mpv_sys::mpv_render_context_render(self.context.as_ptr(), params.as_mut_ptr())
        };
        if result < 0 {
            Err(mpv_error_message(result))
        } else {
            Ok(())
        }
    }
}

impl Drop for MpvRenderContext {
    fn drop(&mut self) {
        unsafe {
            mpv_sys::mpv_render_context_set_update_callback(
                self.context.as_ptr(),
                None,
                std::ptr::null_mut(),
            );
            mpv_sys::mpv_render_context_free(self.context.as_ptr());
        }
    }
}

unsafe extern "C" fn render_update_callback(context: *mut c_void) {
    if let Some(sender) = (context as *const async_channel::Sender<()>).as_ref() {
        let _ = sender.try_send(());
    }
}

unsafe extern "C" fn mpv_get_proc_address(_: *mut c_void, name: *const c_char) -> *mut c_void {
    if name.is_null() {
        return std::ptr::null_mut();
    }
    get_proc_address(CStr::from_ptr(name))
}

fn mpv_error_message(code: c_int) -> String {
    let message = unsafe { mpv_sys::mpv_error_string(code) };
    if message.is_null() {
        format!("mpv error {code}")
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }
}

#[derive(Default)]
struct SurfaceSessions {
    latest: u64,
    active: Option<u64>,
}

impl SurfaceSessions {
    fn activate(&mut self, session_id: u64) -> bool {
        if session_id < self.latest {
            return false;
        }
        self.latest = session_id;
        self.active = Some(session_id);
        true
    }

    fn close(&mut self, session_id: u64) -> bool {
        if self.active != Some(session_id) {
            return false;
        }
        self.active = None;
        true
    }
}

impl NativeVideoControls {
    fn new(window: &WebviewWindow, player: &VideoPlayerService) -> Self {
        let root = gtk::EventBox::new();
        root.style_context()
            .add_class("media-tagger-video-controls");
        root.set_visible(false);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_valign(gtk::Align::End);
        root.add(&row);

        let (play_button, play_icon) = icon_button("media-playback-start-symbolic");
        row.pack_start(&play_button, false, false, 0);

        let (mute_button, mute_icon) = icon_button("audio-volume-high-symbolic");
        row.pack_start(&mute_button, false, false, 0);

        let time_label = gtk::Label::new(Some("0:00 / 0:00"));
        time_label
            .style_context()
            .add_class("media-tagger-video-time");
        time_label.set_width_chars(11);
        time_label.set_xalign(0.5);
        row.pack_start(&time_label, false, false, 2);

        let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.001);
        seek.set_draw_value(false);
        seek.set_hexpand(true);
        seek.style_context().add_class("media-tagger-video-seek");
        row.pack_start(&seek, true, true, 4);

        let rate_button = gtk::MenuButton::new();
        rate_button.set_label("1×");
        let rate_popover = gtk::Popover::new(Some(&rate_button));
        rate_popover
            .style_context()
            .add_class("media-tagger-video-rate-popover");
        let rate_choices = gtk::Box::new(gtk::Orientation::Vertical, 2);
        rate_popover.add(&rate_choices);
        rate_button.set_popover(Some(&rate_popover));
        row.pack_start(&rate_button, false, false, 0);

        let (fullscreen_button, fullscreen_icon) = icon_button("view-fullscreen-symbolic");
        row.pack_start(&fullscreen_button, false, false, 0);

        let session_id = Rc::new(Cell::new(None));
        let labels = Rc::new(RefCell::new(default_control_labels()));

        {
            let player = player.clone();
            let session_id = Rc::clone(&session_id);
            play_button.connect_clicked(move |_| {
                let Some(session_id) = session_id.get() else {
                    return;
                };
                let Some(snapshot) = player
                    .playback_snapshot()
                    .filter(|snapshot| snapshot.session_id == session_id)
                else {
                    return;
                };
                let command = if snapshot.paused {
                    PlayerCommand::Play
                } else {
                    PlayerCommand::Pause
                };
                if let Err(error) = player.control(session_id, command) {
                    eprintln!("native video play control failed: {error}");
                }
            });
        }
        {
            let player = player.clone();
            let session_id = Rc::clone(&session_id);
            mute_button.connect_clicked(move |_| {
                let Some(session_id) = session_id.get() else {
                    return;
                };
                let Some(snapshot) = player
                    .playback_snapshot()
                    .filter(|snapshot| snapshot.session_id == session_id)
                else {
                    return;
                };
                if let Err(error) =
                    player.control(session_id, PlayerCommand::SetMuted(!snapshot.muted))
                {
                    eprintln!("native video mute control failed: {error}");
                }
            });
        }
        {
            let player = player.clone();
            let session_id = Rc::clone(&session_id);
            seek.connect_change_value(move |_, _, value| {
                let Some(session_id) = session_id.get() else {
                    return glib::Propagation::Proceed;
                };
                let Some(snapshot) = player
                    .playback_snapshot()
                    .filter(|snapshot| snapshot.session_id == session_id)
                else {
                    return glib::Propagation::Proceed;
                };
                let time = value.clamp(0.0, 1.0) * snapshot.duration;
                if let Err(error) = player.control(session_id, PlayerCommand::Seek(time)) {
                    eprintln!("native video seek control failed: {error}");
                }
                glib::Propagation::Proceed
            });
        }
        for rate in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0] {
            let button = gtk::Button::with_label(&format_rate(rate));
            let player = player.clone();
            let session_id = Rc::clone(&session_id);
            let popover = rate_popover.clone();
            button.connect_clicked(move |_| {
                if let Some(session_id) = session_id.get() {
                    if let Err(error) = player.control(session_id, PlayerCommand::SetRate(rate)) {
                        eprintln!("native video rate control failed: {error}");
                    }
                }
                popover.popdown();
            });
            rate_choices.pack_start(&button, false, false, 0);
        }
        {
            let window = window.clone();
            let player = player.clone();
            let session_id = Rc::clone(&session_id);
            fullscreen_button.connect_clicked(move |_| {
                let Some(session_id) = session_id.get() else {
                    return;
                };
                let Some(snapshot) = player
                    .playback_snapshot()
                    .filter(|snapshot| snapshot.session_id == session_id)
                else {
                    return;
                };
                if let Err(error) =
                    set_fullscreen(&window, &player, session_id, !snapshot.fullscreen)
                {
                    eprintln!("native video fullscreen control failed: {error}");
                }
            });
        }
        {
            let window = window.clone();
            let player = player.clone();
            let session_id = Rc::clone(&session_id);
            root.connect_key_press_event(move |_, event| {
                let Some(session_id) = session_id.get() else {
                    return glib::Propagation::Proceed;
                };
                let key = event.keyval();
                let fullscreen = player
                    .playback_snapshot()
                    .filter(|snapshot| snapshot.session_id == session_id)
                    .is_some_and(|snapshot| snapshot.fullscreen);
                let requested =
                    if key == gtk::gdk::keys::constants::f || key == gtk::gdk::keys::constants::F {
                        Some(!fullscreen)
                    } else if key == gtk::gdk::keys::constants::Escape && fullscreen {
                        Some(false)
                    } else {
                        None
                    };
                if let Some(requested) = requested {
                    if let Err(error) = set_fullscreen(&window, &player, session_id, requested) {
                        eprintln!("native video keyboard fullscreen control failed: {error}");
                    }
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
        }

        rate_choices.show_all();
        row.show_all();

        Self {
            root,
            play_button,
            play_icon,
            mute_button,
            mute_icon,
            time_label,
            seek,
            rate_button,
            rate_popover,
            fullscreen_button,
            fullscreen_icon,
            session_id,
            labels,
        }
    }

    fn activate(&self, session_id: u64) {
        if self.session_id.get() != Some(session_id) {
            self.rate_popover.popdown();
        }
        self.session_id.set(Some(session_id));
        self.root.show();
    }

    fn deactivate(&self, session_id: u64) {
        if self.session_id.get() == Some(session_id) {
            self.session_id.set(None);
            self.rate_popover.popdown();
            self.root.hide();
        }
    }

    fn set_labels(&self, labels: VideoControlLabels) {
        *self.labels.borrow_mut() = labels;
        self.sync_labels(None);
    }

    fn sync(&self, player: &VideoPlayerService) {
        let Some(session_id) = self.session_id.get() else {
            return;
        };
        let Some(snapshot) = player
            .playback_snapshot()
            .filter(|snapshot| snapshot.session_id == session_id)
        else {
            return;
        };
        let play_icon = if snapshot.paused {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        };
        self.play_icon
            .set_from_icon_name(Some(play_icon), gtk::IconSize::Button);
        let mute_icon = if snapshot.muted || snapshot.volume <= 0.0 {
            "audio-volume-muted-symbolic"
        } else if snapshot.volume < 0.5 {
            "audio-volume-low-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        self.mute_icon
            .set_from_icon_name(Some(mute_icon), gtk::IconSize::Button);
        self.time_label.set_text(&format!(
            "{} / {}",
            format_time(snapshot.current_time),
            format_time(snapshot.duration)
        ));
        let progress = if snapshot.duration > 0.0 {
            (snapshot.current_time / snapshot.duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.seek.set_value(progress);
        self.rate_button.set_label(&format_rate(snapshot.rate));
        let fullscreen_icon = if snapshot.fullscreen {
            "view-restore-symbolic"
        } else {
            "view-fullscreen-symbolic"
        };
        self.fullscreen_icon
            .set_from_icon_name(Some(fullscreen_icon), gtk::IconSize::Button);
        self.sync_labels(Some(snapshot));
    }

    fn sync_labels(
        &self,
        snapshot: Option<crate::services::video_player_service::PlaybackSnapshot>,
    ) {
        let labels = self.labels.borrow();
        let paused = snapshot.is_none_or(|snapshot| snapshot.paused);
        let muted = snapshot.is_some_and(|snapshot| snapshot.muted || snapshot.volume <= 0.0);
        let fullscreen = snapshot.is_some_and(|snapshot| snapshot.fullscreen);
        self.play_button
            .set_tooltip_text(Some(if paused { &labels.play } else { &labels.pause }));
        self.mute_button
            .set_tooltip_text(Some(if muted { &labels.unmute } else { &labels.mute }));
        self.seek.set_tooltip_text(Some(&labels.seek));
        self.rate_button
            .set_tooltip_text(Some(&labels.playback_rate));
        self.fullscreen_button.set_tooltip_text(Some(if fullscreen {
            &labels.exit_fullscreen
        } else {
            &labels.fullscreen
        }));
    }
}

fn icon_button(icon_name: &str) -> (gtk::Button, gtk::Image) {
    let image = gtk::Image::from_icon_name(Some(icon_name), gtk::IconSize::Button);
    let button = gtk::Button::new();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_image(Some(&image));
    (button, image)
}

fn default_control_labels() -> VideoControlLabels {
    VideoControlLabels {
        play: "Play".to_string(),
        pause: "Pause".to_string(),
        mute: "Mute".to_string(),
        unmute: "Unmute".to_string(),
        seek: "Seek".to_string(),
        playback_rate: "Playback speed".to_string(),
        fullscreen: "Fullscreen".to_string(),
        exit_fullscreen: "Exit fullscreen".to_string(),
    }
}

fn format_time(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.0).round() as u64
    } else {
        0
    };
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn format_rate(rate: f64) -> String {
    if (rate - rate.round()).abs() < 0.001 {
        format!("{}×", rate.round() as i64)
    } else {
        let value = format!("{rate:.2}");
        format!("{}×", value.trim_end_matches('0').trim_end_matches('.'))
    }
}

pub fn setup(window: &WebviewWindow, player: &VideoPlayerService) -> Result<(), String> {
    let gtk_window = window.gtk_window().map_err(|error| error.to_string())?;
    let vbox = window.default_vbox().map_err(|error| error.to_string())?;
    let webview = vbox
        .children()
        .into_iter()
        .next()
        .ok_or_else(|| "Tauri WebView widget is unavailable".to_string())?;

    vbox.remove(&webview);
    gtk_window.remove(&vbox);

    let overlay = gtk::Overlay::new();
    let fixed = gtk::Fixed::new();
    fixed.set_hexpand(true);
    fixed.set_vexpand(true);

    let gl_area = gtk::GLArea::new();
    gl_area.set_auto_render(false);
    gl_area.set_has_alpha(false);
    gl_area.set_can_focus(false);
    gl_area.set_visible(false);
    fixed.put(&gl_area, 0, 0);

    let css_provider = gtk::CssProvider::new();
    css_provider
        .load_from_data(NATIVE_CONTROLS_CSS)
        .map_err(|error| format!("native video controls CSS failed: {error}"))?;
    let screen = gtk::gdk::Screen::default()
        .ok_or_else(|| "GTK screen is unavailable for native video controls".to_string())?;
    gtk::StyleContext::add_provider_for_screen(
        &screen,
        &css_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let controls = NativeVideoControls::new(window, player);
    controls.root.set_halign(gtk::Align::Start);
    controls.root.set_valign(gtk::Align::Start);

    webview.set_hexpand(true);
    webview.set_vexpand(true);
    overlay.add(&webview);
    overlay.add_overlay(&fixed);
    overlay.add_overlay(&controls.root);
    overlay.set_overlay_pass_through(&fixed, true);
    overlay.set_overlay_pass_through(&controls.root, false);
    gtk_window.add(&overlay);
    overlay.show();
    fixed.show();
    webview.show();

    let render_context = Rc::new(RefCell::new(RenderContextState::Pending));
    let (render_tx, render_rx) = async_channel::bounded(1);
    let mpv = player.mpv();
    {
        let render_context = Rc::clone(&render_context);
        gl_area.connect_realize(move |area| {
            area.make_current();
            if let Some(error) = area.error() {
                *render_context.borrow_mut() = RenderContextState::Failed(error.to_string());
                return;
            }
            let Ok(mpv) = &mpv else {
                *render_context.borrow_mut() = RenderContextState::Failed(
                    mpv.as_ref()
                        .err()
                        .cloned()
                        .unwrap_or_else(|| "libmpv is unavailable".to_string()),
                );
                return;
            };
            match MpvRenderContext::new(mpv, render_tx.clone()) {
                Ok(context) => {
                    *render_context.borrow_mut() = RenderContextState::Ready(context);
                }
                Err(error) => {
                    let message = format!("libmpv render context initialization failed: {error}");
                    eprintln!("{message}");
                    *render_context.borrow_mut() = RenderContextState::Failed(message);
                }
            }
        });
    }
    {
        let render_context = Rc::clone(&render_context);
        gl_area.connect_unrealize(move |_| {
            *render_context.borrow_mut() = RenderContextState::Pending;
        });
    }
    {
        let render_context = Rc::clone(&render_context);
        let logged_render_target = Cell::new(false);
        gl_area.connect_render(move |area, _| {
            area.make_current();
            if let Some(error) = area.error() {
                eprintln!("native video GL context failed: {error}");
                return glib::Propagation::Stop;
            }
            area.attach_buffers();
            let scale = area.scale_factor().max(1);
            let width = area.allocated_width().saturating_mul(scale);
            let height = area.allocated_height().saturating_mul(scale);
            if width <= 0 || height <= 0 {
                return glib::Propagation::Proceed;
            }
            let fbo = match current_draw_framebuffer() {
                Ok(fbo) => fbo,
                Err(error) => {
                    eprintln!("native video framebuffer query failed: {error}");
                    return glib::Propagation::Stop;
                }
            };
            let framebuffer_status = current_framebuffer_status();
            if !logged_render_target.get() {
                eprintln!(
                    "native video render target: api={}, fbo={fbo}, size={width}x{height}, scale={scale}, status={}",
                    current_context_api(),
                    framebuffer_status
                        .as_ref()
                        .map(|status| format!("0x{status:04x}"))
                        .unwrap_or_else(|error| error.clone())
                );
                logged_render_target.set(true);
            }
            if framebuffer_status != Ok(GL_FRAMEBUFFER_COMPLETE) {
                return glib::Propagation::Stop;
            }
            if let RenderContextState::Ready(context) = &*render_context.borrow() {
                let update_flags = context.update();
                if let Err(error) = context.render(fbo, width, height) {
                    eprintln!("libmpv frame render failed: {error}");
                }
                if let Ok(error) = current_gl_error() {
                    if error != GL_NO_ERROR {
                        eprintln!(
                            "native video OpenGL error after render: 0x{error:04x}, update_flags=0x{update_flags:x}"
                        );
                    }
                }
            }
            glib::Propagation::Stop
        });
    }
    {
        let gl_area = gl_area.clone();
        overlay.connect_realize(move |_| {
            if !gl_area.is_realized() {
                gl_area.realize();
            }
        });
    }
    let render_area = gl_area.clone();
    let render_source = glib::timeout_add_local(Duration::from_millis(8), move || {
        if render_rx.try_recv().is_ok() {
            render_area.queue_render();
        }
        glib::ControlFlow::Continue
    });
    let controls_player = player.clone();
    let controls_sync = controls.clone();
    let controls_source = glib::timeout_add_local(Duration::from_millis(100), move || {
        controls_sync.sync(&controls_player);
        glib::ControlFlow::Continue
    });

    SURFACE.with(|surface| {
        *surface.borrow_mut() = Some(VideoSurface {
            fixed,
            gl_area,
            controls,
            sessions: SurfaceSessions::default(),
            _overlay: overlay,
            _css_provider: css_provider,
            render_context,
            _render_source: render_source,
            _controls_source: controls_source,
        });
    });
    Ok(())
}

pub fn ensure_ready(window: &WebviewWindow) -> Result<(), String> {
    run_on_main_thread_result(window, || {
        SURFACE.with(|surface| {
            let surface = surface.borrow();
            let surface = surface
                .as_ref()
                .ok_or_else(|| "native video surface is unavailable".to_string())?;
            if !surface.gl_area.is_realized() {
                surface.gl_area.realize();
            }
            let readiness = match &*surface.render_context.borrow() {
                RenderContextState::Ready(_) => Ok(()),
                RenderContextState::Pending => {
                    Err("native video render context is not ready".to_string())
                }
                RenderContextState::Failed(message) => Err(message.clone()),
            };
            readiness
        })
    })
}

pub fn set_bounds(
    window: &WebviewWindow,
    session_id: u64,
    bounds: VideoBounds,
) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let inner = window.inner_size().map_err(|error| error.to_string())?;
    let max_width = f64::from(inner.width) / scale;
    let max_height = f64::from(inner.height) / scale;
    let x = bounds.x.min(max_width);
    let y = bounds.y.min(max_height);
    let width = bounds.width.min((max_width - x).max(0.0));
    let height = bounds.height.min((max_height - y).max(0.0));

    run_on_main_thread(window, move || {
        SURFACE.with(|surface| {
            let mut surface = surface.borrow_mut();
            let Some(surface) = surface.as_mut() else {
                return;
            };
            if !surface.sessions.activate(session_id) {
                return;
            }
            if width == 0.0 || height == 0.0 {
                surface.gl_area.hide();
                surface.controls.root.hide();
                return;
            }
            surface
                .fixed
                .move_(&surface.gl_area, x.round() as i32, y.round() as i32);
            surface
                .gl_area
                .set_size_request(width.round() as i32, height.round() as i32);
            let controls_bounds = native_controls_bounds(x, y, width, height);
            surface.controls.root.set_margin_start(controls_bounds.x);
            surface.controls.root.set_margin_top(controls_bounds.y);
            surface
                .controls
                .root
                .set_size_request(controls_bounds.width, controls_bounds.height);
            surface.controls.activate(session_id);
            surface.gl_area.show();
            surface.gl_area.queue_render();
        });
    })
}

fn native_controls_bounds(x: f64, y: f64, width: f64, height: f64) -> NativeControlsBounds {
    let height = height.round() as i32;
    let controls_height = NATIVE_CONTROLS_HEIGHT.min(height);
    NativeControlsBounds {
        x: x.round() as i32,
        y: y.round() as i32 + height - controls_height,
        width: width.round() as i32,
        height: controls_height,
    }
}

pub fn set_control_labels(
    window: &WebviewWindow,
    session_id: u64,
    labels: VideoControlLabels,
) -> Result<(), String> {
    run_on_main_thread(window, move || {
        SURFACE.with(|surface| {
            let surface = surface.borrow();
            let Some(surface) = surface.as_ref() else {
                return;
            };
            if surface.sessions.active == Some(session_id) {
                surface.controls.set_labels(labels);
            }
        });
    })
}

pub fn set_fullscreen(
    window: &WebviewWindow,
    player: &VideoPlayerService,
    session_id: u64,
    fullscreen: bool,
) -> Result<(), String> {
    player.require_current(session_id)?;
    window
        .set_fullscreen(fullscreen)
        .map_err(|error| error.to_string())?;
    player.set_fullscreen_state(session_id, fullscreen)?;
    player.send_fullscreen(session_id, fullscreen);
    Ok(())
}

pub fn hide(window: &WebviewWindow, session_id: u64) -> Result<(), String> {
    run_on_main_thread(window, move || {
        SURFACE.with(|surface| {
            let mut surface = surface.borrow_mut();
            if let Some(surface) = surface.as_mut() {
                if surface.sessions.close(session_id) {
                    surface.gl_area.hide();
                    surface.controls.deactivate(session_id);
                }
            }
        });
    })
}

fn run_on_main_thread(
    window: &WebviewWindow,
    operation: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    run_on_main_thread_result(window, move || {
        operation();
        Ok(())
    })
}

fn run_on_main_thread_result<T: Send + 'static>(
    window: &WebviewWindow,
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            let _ = done_tx.send(operation());
        })
        .map_err(|error| error.to_string())?;
    done_rx
        .recv()
        .map_err(|_| "GTK main-thread operation was cancelled".to_string())?
}

fn get_proc_address(name: &CStr) -> *mut c_void {
    unsafe {
        if has_current_glx_context() {
            let pointer = direct_symbol(gl_library(), name);
            if !pointer.is_null() {
                return pointer;
            }
            let pointer = glx_proc_address(name);
            if !pointer.is_null() {
                return pointer;
            }
        }
        if has_current_egl_context() {
            for library in [gles_library(), gl_library()] {
                let pointer = direct_symbol(library, name);
                if !pointer.is_null() {
                    return pointer;
                }
            }
            let pointer = egl_proc_address(name);
            if !pointer.is_null() {
                return pointer;
            }
        }

        for library in [gl_library(), gles_library()] {
            let pointer = direct_symbol(library, name);
            if !pointer.is_null() {
                return pointer;
            }
        }
        let pointer = egl_proc_address(name);
        if !pointer.is_null() {
            return pointer;
        }
        glx_proc_address(name)
    }
}

unsafe fn direct_symbol(library: Option<&Library>, name: &CStr) -> *mut c_void {
    library
        .and_then(|library| {
            library
                .get::<*mut c_void>(name.to_bytes_with_nul())
                .ok()
                .map(|symbol| *symbol)
        })
        .unwrap_or(std::ptr::null_mut())
}

unsafe fn egl_proc_address(name: &CStr) -> *mut c_void {
    type GetProcAddress = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    egl_library()
        .and_then(|library| library.get::<GetProcAddress>(b"eglGetProcAddress\0").ok())
        .map(|symbol| symbol(name.as_ptr()))
        .unwrap_or(std::ptr::null_mut())
}

unsafe fn glx_proc_address(name: &CStr) -> *mut c_void {
    type GetProcAddress = unsafe extern "C" fn(*const u8) -> *mut c_void;
    gl_library()
        .and_then(|library| {
            library
                .get::<GetProcAddress>(b"glXGetProcAddressARB\0")
                .ok()
        })
        .map(|symbol| symbol(name.as_ptr().cast()))
        .unwrap_or(std::ptr::null_mut())
}

fn current_context_api() -> &'static str {
    unsafe {
        if has_current_glx_context() {
            "glx"
        } else if has_current_egl_context() {
            "egl"
        } else {
            "unknown"
        }
    }
}

unsafe fn has_current_egl_context() -> bool {
    type GetCurrentContext = unsafe extern "C" fn() -> *mut c_void;
    egl_library()
        .and_then(|library| {
            library
                .get::<GetCurrentContext>(b"eglGetCurrentContext\0")
                .ok()
        })
        .is_some_and(|symbol| !symbol().is_null())
}

unsafe fn has_current_glx_context() -> bool {
    type GetCurrentContext = unsafe extern "C" fn() -> *mut c_void;
    gl_library()
        .and_then(|library| {
            library
                .get::<GetCurrentContext>(b"glXGetCurrentContext\0")
                .ok()
        })
        .is_some_and(|symbol| !symbol().is_null())
}

fn current_draw_framebuffer() -> Result<i32, String> {
    let pointer = gl_function("glGetIntegerv")?;
    type GetInteger = unsafe extern "C" fn(u32, *mut i32);
    let function: GetInteger = unsafe { std::mem::transmute(pointer) };
    let mut framebuffer = 0;
    unsafe { function(GL_DRAW_FRAMEBUFFER_BINDING, &mut framebuffer) };
    Ok(framebuffer)
}

fn current_framebuffer_status() -> Result<u32, String> {
    let pointer = gl_function("glCheckFramebufferStatus")?;
    type CheckFramebufferStatus = unsafe extern "C" fn(u32) -> u32;
    let function: CheckFramebufferStatus = unsafe { std::mem::transmute(pointer) };
    Ok(unsafe { function(GL_DRAW_FRAMEBUFFER) })
}

fn current_gl_error() -> Result<u32, String> {
    let pointer = gl_function("glGetError")?;
    type GetError = unsafe extern "C" fn() -> u32;
    let function: GetError = unsafe { std::mem::transmute(pointer) };
    Ok(unsafe { function() })
}

fn gl_function(name: &str) -> Result<*mut c_void, String> {
    let name = CString::new(name).map_err(|error| error.to_string())?;
    let pointer = get_proc_address(&name);
    if pointer.is_null() {
        Err(format!(
            "OpenGL symbol {} is unavailable",
            name.to_string_lossy()
        ))
    } else {
        Ok(pointer)
    }
}

fn egl_library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY
        .get_or_init(|| unsafe { Library::new("libEGL.so.1").ok() })
        .as_ref()
}

fn gl_library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY
        .get_or_init(|| unsafe { Library::new("libGL.so.1").ok() })
        .as_ref()
}

fn gles_library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY
        .get_or_init(|| unsafe { Library::new("libGLESv2.so.2").ok() })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::{
        format_rate, format_time, native_controls_bounds, NativeControlsBounds, SurfaceSessions,
    };

    #[test]
    fn limits_the_input_receiving_overlay_to_the_bottom_of_the_video() {
        assert_eq!(
            native_controls_bounds(20.0, 30.0, 640.0, 360.0),
            NativeControlsBounds {
                x: 20,
                y: 322,
                width: 640,
                height: 68,
            }
        );
        assert_eq!(
            native_controls_bounds(20.0, 30.0, 200.0, 40.0),
            NativeControlsBounds {
                x: 20,
                y: 30,
                width: 200,
                height: 40,
            }
        );
    }

    #[test]
    fn formats_native_control_time_and_rate_labels() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(65.0), "1:05");
        assert_eq!(format_time(3661.0), "1:01:01");
        assert_eq!(format_rate(1.0), "1×");
        assert_eq!(format_rate(0.75), "0.75×");
        assert_eq!(format_rate(1.5), "1.5×");
    }

    #[test]
    fn stale_surface_tasks_cannot_replace_or_hide_the_latest_session() {
        let mut sessions = SurfaceSessions::default();
        assert!(sessions.activate(1));
        assert!(sessions.activate(2));
        assert!(!sessions.activate(1));
        assert!(!sessions.close(1));
        assert!(sessions.close(2));
        assert!(!sessions.activate(1));
    }
}
