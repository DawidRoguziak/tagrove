//! Fixtures for files whose content points at other files or at the network.
//! Shared by the libmpv player tests in `video/player.rs` and the ffmpeg/ffprobe
//! tests in `thumbs.rs` (ADR 0028).

use std::ffi::CString;
use std::io::{Error, ErrorKind};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// A loopback listener that only records whether anything connected to it.
pub fn loopback_listener() -> (std::net::TcpListener, String) {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    (listener, url)
}

pub fn connection_arrived(listener: &std::net::TcpListener) -> bool {
    match listener.accept() {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(error) => panic!("loopback listener failed: {error}"),
    }
}

/// Records, through inotify, every open of the watched files by any process.
pub struct OpenWatch(OwnedFd);

impl OpenWatch {
    pub fn new(paths: &[&Path]) -> Self {
        // SAFETY: the call takes no pointers; the descriptor is checked before it is owned.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        assert!(fd >= 0, "inotify_init1: {}", Error::last_os_error());
        // SAFETY: `fd` is a new descriptor that nothing else owns.
        let watch = Self(unsafe { OwnedFd::from_raw_fd(fd) });
        for path in paths {
            let name = CString::new(path.as_os_str().as_bytes()).unwrap();
            // SAFETY: `name` is a NUL-terminated string that outlives the call.
            let added = unsafe {
                libc::inotify_add_watch(fd, name.as_ptr(), libc::IN_OPEN | libc::IN_ACCESS)
            };
            assert!(added >= 0, "inotify_add_watch: {}", Error::last_os_error());
        }
        watch
    }

    /// Whether a watched file was opened or read since the previous call.
    pub fn opened(&self) -> bool {
        let mut events = [0u8; 4096];
        let mut opened = false;
        loop {
            // SAFETY: the buffer is writable for the length passed.
            let read =
                unsafe { libc::read(self.0.as_raw_fd(), events.as_mut_ptr().cast(), events.len()) };
            if read > 0 {
                opened = true;
            } else if read < 0 && Error::last_os_error().kind() == ErrorKind::WouldBlock {
                return opened;
            } else {
                panic!("inotify read failed: {}", Error::last_os_error());
            }
        }
    }
}

/// Text files named `*.mp4` whose content sends an unrestricted player to
/// `url`, to the absolute path `outside`, or to the sibling file `sibling`.
pub fn reference_fixtures(url: &str, outside: &str, sibling: &str) -> Vec<(&'static str, String)> {
    let port = url.rsplit(':').next().unwrap();
    let mpd = |target: &str| {
        format!(
            "<?xml version=\"1.0\"?>\n<MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\" \
             type=\"static\" mediaPresentationDuration=\"PT2S\" \
             profiles=\"urn:mpeg:dash:profile:isoff-on-demand:2011\"><Period>\
             <AdaptationSet mimeType=\"video/mp4\"><Representation id=\"1\" \
             bandwidth=\"1000\" codecs=\"mp4v.20.9\" width=\"160\" height=\"90\">\
             <BaseURL>{target}</BaseURL><SegmentBase indexRange=\"0-1\"/>\
             </Representation></AdaptationSet></Period></MPD>\n"
        )
    };
    vec![
        ("m3u.mp4", format!("#EXTM3U\n{url}/m3u.mp4\n{outside}\n")),
        ("m3u-local.mp4", format!("#EXTM3U\n{outside}\n")),
        (
            "pls.mp4",
            format!("[playlist]\nFile1={url}/pls.mp4\nFile2={outside}\nNumberOfEntries=2\n"),
        ),
        (
            "hls.mp4",
            format!(
                "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:2\n\
                 #EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:2.0,\n{url}/hls.ts\n\
                 #EXTINF:2.0,\n{outside}\n#EXT-X-ENDLIST\n"
            ),
        ),
        (
            "hls-master.mp4",
            format!("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000\n{url}/master.m3u8\n"),
        ),
        ("edl.mp4", format!("# mpv EDL v0\n{outside}\n")),
        ("edl-net.mp4", format!("# mpv EDL v0\n{url}/edl.mp4\n")),
        ("dash.mp4", mpd(&format!("file://{outside}"))),
        ("dash-net.mp4", mpd(&format!("{url}/dash.mp4"))),
        (
            "concat.mp4",
            format!("ffconcat version 1.0\nfile {sibling}\n"),
        ),
        (
            "sdp.mp4",
            format!(
                "v=0\no=- 0 0 IN IP4 127.0.0.1\ns=-\nc=IN IP4 127.0.0.1\nt=0 0\n\
                 m=video {port} RTP/AVP 96\na=rtpmap:96 H264/90000\n"
            ),
        ),
    ]
}
