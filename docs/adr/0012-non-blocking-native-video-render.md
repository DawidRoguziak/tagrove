# 0012: Native video rendering never waits on the GTK main thread

- Status: Accepted
- Date: 2026-11-10
- Areas: lightbox, native video, GTK main loop, IPC responsiveness
- Related: [lightbox](../subsystems/lightbox.md#media-presentation), [testing](../development/testing.md)

## Context

libmpv draws into a `GtkGLArea` whose render handler runs on the GTK main thread. That thread also runs the WebView, every synchronous Tauri command, and wry's custom-protocol responders.

The audit run of 2026-11-08 reported that opening a 30-minute video froze the WebView for good and suspected the audio output, because every frozen video had an audio track and the harness had no audio server. Measurements with the E2E build on Xvfb with llvmpipe showed two causes, neither of them audio:

- `mpv_render_context_render` waits by default until the frame's display time (`MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME`). The main thread spent 840 ms of every second inside the call at 30 fps, and 877 ms at 24 fps.
- Each call returned when the next frame was already announced. The wake-up that calls `queue_render` ran at GLib default priority, so a redraw (`GDK_PRIORITY_REDRAW`) was always pending. wry finishes custom-protocol requests with `MainContext::invoke`, which queues at `DEFAULT_IDLE`, below the redraw, when a worker thread calls it. Tauri answers async commands through that protocol, so those answers waited until playback paused or looped: 7.4 s with the 8-second fixture, and more than 54 s (until the loop point) with a 60-second clip.

The result was the same for a clip without an audio track, with PipeWire reachable (`AO: [pipewire]`), with no reachable audio server (every output failed and libmpv continued without sound), and with `ao=null`. The short fixtures only looked healthy because they loop every 8 seconds, inside the 30-second WebDriver script timeout.

All of this was measured under Xvfb, where a timer drives GTK's frame clock. X11 without a compositing window manager uses the same timer and is expected to behave the same. A compositor paces the frame clock itself; that case was not measured. Whatever paces the clock, the blocking call parks the main thread until each frame's display time.

Code: `src-tauri/src/video_surface.rs` (`FrameRenderParams`, `spawn_render_wakeups`), `src-tauri/src/services/video_player_service.rs` (`PLAYER_OPTIONS`).

## Options considered

1. **Keep libmpv's blocking render.** It is libmpv's default. Rejected: it parks the main thread for up to 50 ms per frame and causes the starvation above.
2. **Non-blocking render only** (`BLOCK_FOR_TARGET_TIME=0`, `video-timing-offset=0`). Removes the wait, but a redraw is still always pending whenever drawing a frame takes longer than the frame interval (software GL, high frame rates), so the starvation can return.
3. **Non-blocking render plus frame wake-ups at `DEFAULT_IDLE` (chosen).** The redraw request shares a main-loop turn with wry's responders, so each redraw is preceded by a drain of pending responses, whatever a frame costs to draw.
4. **A dedicated render thread** (`MPV_RENDER_PARAM_ADVANCED_CONTROL`, own GL context, texture shared with GTK). Keeps libmpv's timing exact, but needs context sharing across GLX and EGL and a second owner for the surface lifecycle. Rejected as far larger than the defect.
5. **Force or restrict the audio output** (for example `ao=pipewire,pulse,alsa` or `ao=null` in tests). Rejected: audio was not the cause, and a restricted list would remove outputs that some desktops need.

## Decision

- The `GLArea` render handler never waits for libmpv's frame timing: every render passes `MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME=0`, and the player sets `video-timing-offset=0` so libmpv announces a frame at its display time.
- The task that turns libmpv's update callback into `queue_render` runs at `glib::Priority::DEFAULT_IDLE` (`RENDER_WAKEUP_PRIORITY`).
- The player sets `audio-fallback-to-null=yes`. When no audio output opens, the file plays through libmpv's null output without sound and without an error event. This makes explicit what libmpv already did by dropping the audio track.

## Consequences

- With 60-second clips at 640x360 30 fps and 1280x720 60 fps, the controller's IPC probe answers in about 0.2 s during playback, with and without a reachable audio server. In an instrumented build without an audio server, the render call took under 1 ms per frame and a `DEFAULT_IDLE` source ran within a millisecond.
- Video now yields to the rest of the application. When the main loop is busy, frames are drawn late, and libmpv drops a frame that the surface has not taken within 200 ms.
- A frame is now drawn slightly after its display time: it waits for one idle-priority turn of the main loop and for GTK's paint, where the blocking call returned at the display time. That delay was not measured, and neither were pacing and A/V sync on a hardware compositor.
- New invariant: no frame-timing wait and no synchronous libmpv call in the render handler, and frame wake-ups stay at idle priority. Tests: `frame_render_never_waits_for_the_display_time` and `frame_wakeups_leave_room_for_idle_priority_sources` (unit), `real_mpv_plays_an_audio_track_silently_when_no_audio_device_opens` (real libmpv), and the desktop case "keeps IPC responsive while a long video with an audio track plays" in `e2e/specs/app.workflows.e2e.js`.
- The priority rule depends on how wry 0.55 delivers custom-protocol responses. If a later wry version changes that priority, the E2E case still detects starvation.
- Silent playback is not shown in the UI.
