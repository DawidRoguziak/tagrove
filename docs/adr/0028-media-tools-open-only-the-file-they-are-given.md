# 0028: libmpv, ffmpeg and ffprobe open only the file they are given

- Status: Accepted
- Date: 2026-11-11
- Areas: lightbox, native video, thumbnails, scanning, security
- Related: [lightbox](../subsystems/lightbox.md#media-presentation), [thumbnails](../subsystems/thumbnails.md#video-probing-and-rendering), [security surface](../architecture/system-overview.md#tauri-security-surface-and-linux-packaging), [`PRIVACY.md`](../../PRIVACY.md), [ADR 0012](0012-non-blocking-native-video-render.md)

## Context

The media kind comes from the file extension (`indexer.rs`, `detect_kind`), so every file named `*.mp4`, `*.mkv`, `*.webm`, `*.mov` or `*.avi` in a scan folder is a video. `resolve_video_path` authorizes the path of that one file: a regular file whose canonical path is inside an assigned scan root. libmpv, ffmpeg and ffprobe then choose a demuxer from the file's content, and several demuxers treat the content as a list of other things to open.

The audit run of 2026-11-08 reported this for libmpv. It was measured again for this decision with libmpv 0.41 and FFmpeg 9.0, with text files named `*.mp4`, a loopback TCP listener, a real clip outside the folder and a real clip beside the text file:

| Content of the `*.mp4` | libmpv (player) | ffmpeg and ffprobe (thumbnail, duration) |
| --- | --- | --- |
| m3u or pls playlist, HLS playlist | requests the URL, tries to start `yt-dlp`, `yt-dlp_x86` and `youtube-dl` from `PATH` with that URL, then plays an absolute path outside the folder | not recognized |
| mpv EDL | plays the absolute path outside the folder, or requests the URL | not recognized |
| DASH manifest | plays a `file://` path outside the folder | thumbnail and duration come from the `file://` path outside the folder; `http` is refused |
| `concat` script | plays the file beside it | thumbnail comes from the file beside it; absolute paths and URLs are refused |
| SDP description | opens an RTP input and waits for a stream | refused |
| zip archive | plays the first entry | not opened as an archive |

`ffmpeg` and `ffprobe` refuse network references here only because libavformat gives a `file` input the default protocol list `file,crypto,data`. libmpv reads the file through its own stream layer, so that default does not apply to it.

Anyone who can put a file into an indexed folder (a download, a shared archive) could therefore make the app request a URL when the user opens the file, start whatever `yt-dlp` is first on `PATH`, show media from outside the selected folders in the player, and put a frame of such a file into the gallery. `PRIVACY.md` says that media stays in the selected folders and lists no network activity of the player or the thumbnail tools. The Flatpak has no network permission, but it can read the home directory, so the local cases apply there too.

Code: `src-tauri/src/services/video_player_service.rs` (`PLAYER_OPTIONS`, `SCRIPT_OPTIONS`), `src-tauri/src/thumbs.rs` (`LOCAL_VIDEO_INPUT_ARGS`).

## Options considered

1. **Check the content in Rust before calling a tool** (reject `#EXTM3U`, `[playlist]`, `ffconcat`, `<MPD` and so on). Rejected: it is a list of signatures that has to match what two other parsers detect, and each new demuxer reopens the gap.
2. **Derive the kind from content at scan time.** A larger change to scanning and to what `kind` means, and it still does not limit what the tools do with a file they accept.
3. **Restrict the tools with their own options (chosen).** libmpv has a switch for exactly this, and libavformat has protocol and format lists.
4. **`-protocol_whitelist file` alone for ffmpeg and ffprobe.** Not enough: the DASH and `concat` cases above stay on the `file` protocol.
5. **A format list for libmpv too** (`demuxer-lavf-o=format_whitelist=...`). Rejected: the option applies to every file the player opens, so the subtitle formats of sidecar files would have to be listed as well, and `access-references=no` already covers the manifest demuxers.
6. **Turn off sidecar files** (`sub-auto=no`, `cover-art-auto=no`). Rejected: libmpv looks for them only in the video's own folder (the video's name with another extension, and a fixed list of cover file names) and opens them under the same restrictions; turning them off would remove subtitles that are shown today.
7. **Turn off every built-in script by name** (`load-stats-overlay`, `load-console`, `load-select`, `load-positioning`, `load-commands`, `load-context-menu`). Rejected: the names depend on the libmpv version, and an unknown option fails initialization. Those scripts act on key bindings, and libmpv's embedding defaults leave `input-default-bindings`, `input-vo-keyboard` and `osc` off.

## Decision

libmpv, in `PLAYER_OPTIONS`:

- `access-references=no`: playlist, EDL, CUE and archive content is refused, and so is every file or URL that a libavformat demuxer asks libmpv to open (HLS segments, DASH representations).
- `demuxer-lavf-o=protocol_whitelist=data`: demuxers that open their references themselves (`concat`, `sdp`) find no usable protocol. libavformat needs none for the opened file, because libmpv reads it through its own stream layer; `data` is inline bytes and does no I/O.
- `ordered-chapters=no`: Matroska segment linking does not scan the folder for sibling files.
- `config=no`: no user mpv configuration is read. This is libmpv's embedding default, stated so that the options above cannot be overridden from a file.
- `load-unsafe-playlists=no`: libmpv's default, stated.

libmpv, in `SCRIPT_OPTIONS`: `ytdl=no` and `load-scripts=no`. The hook that starts `yt-dlp` is not loaded, and no script is loaded from a configuration folder. libmpv has these options only when it is built with script support. The Flatpak build is not (`-Dlua=disabled`, `-Djavascript=disabled`), so an option that libmpv reports as unknown is skipped; any other error still fails initialization.

ffmpeg and ffprobe, in `LOCAL_VIDEO_INPUT_ARGS`, on the thumbnail render, the ffprobe duration probe and the ffmpeg duration fallback:

- `-format_whitelist mov,matroska,avi,mpegts,mpeg,flv,asf,ogg`: only demuxers that read the one file they are given. The first three are the containers of the five video extensions; the others are containers that sometimes carry one of those extensions.
- `-protocol_whitelist file`: the input stays on the `file` protocol.

The options of ADR 0012 (`video-timing-offset=0`, `audio-fallback-to-null=yes`, non-blocking render) are unchanged.

## Consequences

- A video file with reference content fails in the player with the existing video error and Retry, in well under a second, and the player is ready for the next file. It gets no thumbnail and no duration.
- New invariant: the player and the video tools open the authorized file and nothing that its content names. Tests: `real_mpv_fails_reference_content_without_opening_what_it_names` (eleven reference files against a loopback listener and an inotify watch on the files they name, then normal playback on the same player), `real_mpv_follows_the_reference_fixtures_once_reference_access_is_allowed` (the same files do reach the listener and the outside file with `access-references=yes`, so the first test cannot pass by accident), `real_mpv_reports_the_reference_blocking_player_options`, `real_mpv_skips_only_an_unknown_script_option`, and `video_tools_refuse_reference_content_and_still_read_real_containers` in `thumbs.rs` (the same files through ffprobe and ffmpeg, then MP4, MKV, MOV and AVI).
- A file with one of the five extensions whose real container is not on the format list (a raw elementary stream, RealMedia, MXF) still plays, but gets no thumbnail and no duration.
- libmpv no longer plays Matroska files that are split with ordered chapters as one timeline, CUE and EDL files, or archives named as videos. A VobSub subtitle pair beside a video is expected to stop loading, because its second file is opened by libavformat; this was not measured.
- Sidecar subtitles and cover art in the video's folder are still opened by libmpv.
- Thumbnails and durations stored before this change are kept. `THUMB_CACHE_VERSION` is not bumped, because an ordinary video renders the same picture; a stale thumbnail of a reference file goes away with Clear thumbnails or when the file changes.
- Not covered: a defect in a demuxer or decoder that a crafted real container could trigger, and the global asset-protocol scope listed under known limitations in the system overview.
- The ordered-chapters case has no fixture; the test only reads the option back. The libmpv build without script support was not run; the skip was tested with an option name that no libmpv has.
