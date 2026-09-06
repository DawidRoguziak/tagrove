//! Owned libmpv event client. All access stays on the playback worker.
//!
//! libmpv2 6's create_client uses unchecked NonNull construction. The native API
//! explicitly permits a null result, so handle creation and events are checked here.
use std::{
    ffi::{CStr, CString},
    ptr::NonNull,
};

use libmpv2::Mpv;
use libmpv2_sys as sys;

pub(super) enum NativeEvent {
    Metadata,
    StateChanged,
    Error(String),
    Other,
}

pub(super) struct VideoEventClient(NonNull<sys::mpv_handle>);

impl VideoEventClient {
    pub(super) fn new(mpv: &Mpv) -> Result<Self, String> {
        // SAFETY: mpv owns a live core. Null requests an automatically assigned name.
        let pointer = unsafe { sys::mpv_create_client(mpv.ctx.as_ptr(), std::ptr::null()) };
        let client = Self(NonNull::new(pointer).ok_or("libmpv event client is unavailable")?);
        for (id, name, format) in [
            (1, c"time-pos", sys::mpv_format_MPV_FORMAT_DOUBLE),
            (2, c"pause", sys::mpv_format_MPV_FORMAT_FLAG),
            (3, c"paused-for-cache", sys::mpv_format_MPV_FORMAT_FLAG),
            (4, c"volume", sys::mpv_format_MPV_FORMAT_DOUBLE),
            (5, c"mute", sys::mpv_format_MPV_FORMAT_FLAG),
            (6, c"speed", sys::mpv_format_MPV_FORMAT_DOUBLE),
            (7, c"seeking", sys::mpv_format_MPV_FORMAT_FLAG),
        ] {
            // SAFETY: names are static C strings; the client remains live for this call.
            let result =
                unsafe { sys::mpv_observe_property(client.0.as_ptr(), id, name.as_ptr(), format) };
            if result < 0 {
                return Err(error_message(result));
            }
        }
        Ok(client)
    }

    /// libmpv copies command arguments before returning. Completion arrives on this client.
    pub(super) fn command_async(&self, id: u64, args: &[&str]) -> Result<(), String> {
        let strings = args
            .iter()
            .map(|arg| CString::new(*arg))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "libmpv command contains a NUL byte".to_string())?;
        let mut pointers = strings.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        pointers.push(std::ptr::null());
        // SAFETY: the client and null-terminated argument array remain live through this call.
        let result = unsafe { sys::mpv_command_async(self.0.as_ptr(), id, pointers.as_mut_ptr()) };
        if result < 0 {
            Err(error_message(result))
        } else {
            Ok(())
        }
    }

    pub(super) fn wait_event(&self, timeout: f64) -> Option<NativeEvent> {
        // SAFETY: this client is owned by one worker. Copy everything we need before
        // the next wait_event invalidates the native event and its payload.
        let event = unsafe { sys::mpv_wait_event(self.0.as_ptr(), timeout).as_ref() }?;
        if event.error < 0 {
            return Some(NativeEvent::Error(error_message(event.error)));
        }
        match event.event_id {
            sys::mpv_event_id_MPV_EVENT_NONE => None,
            sys::mpv_event_id_MPV_EVENT_FILE_LOADED
            | sys::mpv_event_id_MPV_EVENT_VIDEO_RECONFIG => Some(NativeEvent::Metadata),
            sys::mpv_event_id_MPV_EVENT_PROPERTY_CHANGE
            | sys::mpv_event_id_MPV_EVENT_PLAYBACK_RESTART
            | sys::mpv_event_id_MPV_EVENT_SEEK => Some(NativeEvent::StateChanged),
            sys::mpv_event_id_MPV_EVENT_END_FILE => {
                // SAFETY: END_FILE defines data as mpv_event_end_file, valid until the next wait.
                let end = unsafe { event.data.cast::<sys::mpv_event_end_file>().as_ref() };
                Some(match end {
                    Some(end) if end.error < 0 => NativeEvent::Error(error_message(end.error)),
                    Some(end)
                        if end.reason == sys::mpv_end_file_reason_MPV_END_FILE_REASON_ERROR =>
                    {
                        NativeEvent::Error("libmpv could not decode the video".into())
                    }
                    Some(_) => NativeEvent::Other,
                    None => NativeEvent::Error("libmpv returned an empty end-file event".into()),
                })
            }
            sys::mpv_event_id_MPV_EVENT_SHUTDOWN => {
                Some(NativeEvent::Error("libmpv stopped unexpectedly".into()))
            }
            sys::mpv_event_id_MPV_EVENT_QUEUE_OVERFLOW => {
                Some(NativeEvent::Error("libmpv event queue overflowed".into()))
            }
            _ => Some(NativeEvent::Other),
        }
    }
}

impl Drop for VideoEventClient {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner. Destroy detaches this client, not the main handle.
        unsafe {
            sys::mpv_destroy(self.0.as_ptr());
        }
    }
}

fn error_message(code: i32) -> String {
    // SAFETY: libmpv returns a static, null-terminated error string.
    let pointer = unsafe { sys::mpv_error_string(code) };
    if pointer.is_null() {
        format!("libmpv error {code}")
    } else {
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}
