//! `thumb://` URI scheme: gallery thumbnails addressed by asset ID.
//!
//! URL shape: `thumb://localhost/<id>?v=<version>[&c=1][&p=1][&r=<retry>]`.
//! `c=1` (cached only, sent during a fast-scroll pause) serves an existing file
//! and answers 404 instead of rendering a missing one. `p=1` (prefetch, sent by
//! overscan tiles) renders behind every visible request. The handler
//! accepts only a positive numeric ID and a hex version token; it never takes
//! a path, so the scheme cannot reach files outside the thumbnail cache. The
//! database resolves the asset and its current source version. A missing
//! thumbnail is rendered on demand through the scheduler's Interactive (LIFO)
//! queue without parking a thread while the render runs.
//!
//! WebKitGTK gives no cancellation signal for custom-scheme requests (wry
//! hands the handler a one-shot responder only), so every request is
//! answered; LIFO ordering keeps tiles that just came into view first.
//!
//! This is the framework-free part: replies are plain data ([`ThumbReply`])
//! and the runtime is reached through [`ThumbHost`]. The Tauri adapter
//! (`adapter/thumb_scheme.rs`) registers the scheme, reads the request, runs
//! [`serve`] on the blocking pool and turns each reply into an HTTP response.

use crate::{
    app::state::AppState,
    services::thumb_service::{self, ServedThumbnail, ThumbnailRequestOutcome},
    thumbs::THUMB_VERSION_LEN,
};

pub const SCHEME: &str = "thumb";

/// Page origins allowed to read thumbnails with `crossorigin="anonymous"`:
/// the bundled app (`tauri://localhost`) and the Vite dev server.
const ALLOWED_ORIGINS: [&str; 2] = ["tauri://localhost", "http://localhost:1420"];
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const NO_STORE: &str = "no-store";
const RETRY_AFTER_SECONDS: &str = "1";

pub const STATUS_OK: u16 = 200;
pub const STATUS_BAD_REQUEST: u16 = 400;
pub const STATUS_NOT_FOUND: u16 = 404;
pub const STATUS_SERVICE_UNAVAILABLE: u16 = 503;

/// Header names in their canonical lowercase form.
pub const CONTENT_TYPE: &str = "content-type";
pub const ACCESS_CONTROL_ALLOW_ORIGIN: &str = "access-control-allow-origin";
pub const CACHE_CONTROL: &str = "cache-control";
pub const RETRY_AFTER: &str = "retry-after";

/// One `thumb://` response as data; the adapter builds the HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbReply {
    pub status: u16,
    /// Header name and value pairs, in the order they are sent.
    pub headers: Vec<(&'static str, &'static str)>,
    pub body: Vec<u8>,
}

impl ThumbReply {
    /// The value of the first header named `name`.
    pub fn header(&self, name: &str) -> Option<&'static str> {
        self.headers
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    }
}

/// Answers one request exactly once.
pub type ThumbResponder = Box<dyn FnOnce(ThumbReply) + Send>;

/// The runtime a request is served in.
pub trait ThumbHost: Clone + Send + 'static {
    /// The managed state, or `None` until setup has managed it.
    fn app_state(&self) -> Option<&AppState>;
    /// Runs `task` on the runtime's blocking pool.
    fn spawn_blocking<F: FnOnce() + Send + 'static>(task: F);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbRequest {
    pub asset_id: i64,
    pub version: String,
    pub cached_only: bool,
    pub prefetch: bool,
}

/// Parses `/<id>?v=<hex>[&c=1][&p=1][&r=<n>]`; anything else is rejected,
/// including a repeated parameter. `r` (frontend retry counter) is allowed
/// once and ignored.
pub fn parse_request(path: &str, query: Option<&str>) -> Option<ThumbRequest> {
    let id_text = path.strip_prefix('/')?;
    if id_text.is_empty() || id_text.len() > 19 || !id_text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let asset_id = id_text.parse::<i64>().ok().filter(|id| *id > 0)?;
    let mut version = None;
    let mut cached_only = false;
    let mut prefetch = false;
    let mut retry_seen = false;
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
    {
        let (key, value) = pair.split_once('=')?;
        match key {
            "v" if version.is_none() => {
                let valid = value.len() == THUMB_VERSION_LEN
                    && value
                        .bytes()
                        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
                if !valid {
                    return None;
                }
                version = Some(value.to_string());
            }
            "c" if value == "1" && !cached_only => cached_only = true,
            "p" if value == "1" && !prefetch => prefetch = true,
            "r" if !retry_seen
                && !value.is_empty()
                && value.len() <= 3
                && value.bytes().all(|b| b.is_ascii_digit()) =>
            {
                retry_seen = true;
            }
            _ => return None,
        }
    }
    Some(ThumbRequest {
        asset_id,
        version: version?,
        cached_only,
        prefetch,
    })
}

pub fn allowed_origin(request_origin: Option<&str>) -> &'static str {
    request_origin
        .and_then(|origin| {
            ALLOWED_ORIGINS
                .iter()
                .copied()
                .find(|allowed| *allowed == origin)
        })
        .unwrap_or(ALLOWED_ORIGINS[0])
}

fn empty(status: u16, origin: &'static str) -> ThumbReply {
    let mut headers = vec![
        (ACCESS_CONTROL_ALLOW_ORIGIN, origin),
        (CACHE_CONTROL, NO_STORE),
    ];
    if status == STATUS_SERVICE_UNAVAILABLE {
        headers.push((RETRY_AFTER, RETRY_AFTER_SECONDS));
    }
    ThumbReply {
        status,
        headers,
        body: Vec::new(),
    }
}

/// 200 with the JPEG. Only a response whose bytes match the requested
/// version is immutable; a stale URL gets the current picture uncached.
pub fn image_response(
    served: ServedThumbnail,
    requested_version: &str,
    origin: &'static str,
) -> ThumbReply {
    let cache = if served.version == requested_version {
        IMMUTABLE
    } else {
        NO_STORE
    };
    ThumbReply {
        status: STATUS_OK,
        headers: vec![
            (CONTENT_TYPE, "image/jpeg"),
            (ACCESS_CONTROL_ALLOW_ORIGIN, origin),
            (CACHE_CONTROL, cache),
        ],
        body: served.bytes,
    }
}

pub fn not_found(origin: &'static str) -> ThumbReply {
    empty(STATUS_NOT_FOUND, origin)
}

pub fn unavailable(origin: &'static str) -> ThumbReply {
    empty(STATUS_SERVICE_UNAVAILABLE, origin)
}

pub fn bad_request(origin: &'static str) -> ThumbReply {
    empty(STATUS_BAD_REQUEST, origin)
}

/// Serves one parsed request. Runs on the blocking pool; the adapter answers
/// a non-GET method or a malformed URL before this.
pub fn serve<H: ThumbHost>(
    host: H,
    request: ThumbRequest,
    origin: &'static str,
    responder: ThumbResponder,
) {
    // Setup manages AppState last; requests before that are retried.
    let Some(state) = host.app_state() else {
        responder(unavailable(origin));
        return;
    };
    let permit = match state.database.admit() {
        Ok(permit) => permit,
        Err(_) => {
            responder(unavailable(origin));
            return;
        }
    };

    // The responder moves into the render callback only when a render is
    // enqueued; a shared slot lets the immediate path still answer.
    let slot = std::sync::Arc::new(std::sync::Mutex::new(Some(responder)));
    let callback_slot = std::sync::Arc::clone(&slot);
    let callback_host = host.clone();
    let requested_version = request.version.clone();
    let outcome = thumb_service::request_thumbnail(
        request.asset_id,
        thumb_service::ThumbnailRequestMode {
            cached_only: request.cached_only,
            prefetch: request.prefetch,
        },
        state,
        &permit,
        move |pending| {
            if let Some(responder) = take(&callback_slot) {
                publisher().push(
                    callback_host,
                    Finished {
                        pending,
                        responder,
                        requested_version,
                        origin,
                    },
                );
            }
        },
    );

    let response = match outcome {
        Ok(ThumbnailRequestOutcome::Ready(served)) => {
            image_response(served, &request.version, origin)
        }
        Ok(ThumbnailRequestOutcome::NotFound) => not_found(origin),
        Ok(ThumbnailRequestOutcome::Pending) => return,
        // Full scheduler queue, busy database pool or another request error:
        // transient. Maintenance never lands here; `admit()` waits for it.
        Err(_) => unavailable(origin),
    };
    if let Some(responder) = take(&slot) {
        responder(response);
    }
}

/// Most finished renders published in one transaction.
const PUBLISH_BATCH: usize = 64;

struct Finished {
    pending: thumb_service::PendingThumbnail,
    responder: ThumbResponder,
    requested_version: String,
    origin: &'static str,
}

/// Collects finished renders and publishes whatever is already waiting in one
/// transaction (as the former page stream did), off the scheduler workers.
/// Grouping adds no delay: the first result starts a publisher at once.
#[derive(Default)]
struct Publisher {
    state: std::sync::Mutex<PublisherState>,
}

#[derive(Default)]
struct PublisherState {
    waiting: Vec<Finished>,
    running: bool,
}

fn publisher() -> &'static Publisher {
    static PUBLISHER: std::sync::OnceLock<Publisher> = std::sync::OnceLock::new();
    PUBLISHER.get_or_init(Publisher::default)
}

impl Publisher {
    fn push<H: ThumbHost>(&'static self, host: H, finished: Finished) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.waiting.push(finished);
        if state.running {
            return;
        }
        state.running = true;
        drop(state);
        H::spawn_blocking(move || self.drain(&host));
    }

    fn drain<H: ThumbHost>(&self, host: &H) {
        loop {
            let batch = {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                if state.waiting.is_empty() {
                    state.running = false;
                    return;
                }
                let take = state.waiting.len().min(PUBLISH_BATCH);
                state.waiting.drain(..take).collect::<Vec<_>>()
            };
            publish_batch(host, batch);
        }
    }
}

fn publish_batch<H: ThumbHost>(host: &H, batch: Vec<Finished>) {
    let mut pending = Vec::with_capacity(batch.len());
    let mut replies = Vec::with_capacity(batch.len());
    for finished in batch {
        pending.push(finished.pending);
        replies.push((
            finished.responder,
            finished.requested_version,
            finished.origin,
        ));
    }
    let served = host.app_state().map(|state| {
        state
            .database
            .admit()
            .and_then(|permit| thumb_service::finish_thumbnail_requests(pending, state, &permit))
    });
    match served {
        Some(Ok(served)) => {
            for ((responder, version, origin), served) in replies.into_iter().zip(served) {
                responder(match served {
                    Some(served) => image_response(served, &version, origin),
                    // Stale (record changed or thumbnails cleared) or failed:
                    // the frontend retries and a fresh request renders again.
                    None => not_found(origin),
                });
            }
        }
        _ => {
            for (responder, _, origin) in replies {
                responder(unavailable(origin));
            }
        }
    }
}

fn take(slot: &std::sync::Mutex<Option<ThumbResponder>>) -> Option<ThumbResponder> {
    slot.lock().unwrap_or_else(|e| e.into_inner()).take()
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: &str = "0123456789abcdef";

    #[test]
    fn parses_numeric_id_and_hex_version() {
        assert_eq!(
            parse_request("/42", Some(&format!("v={V}"))),
            Some(ThumbRequest {
                asset_id: 42,
                version: V.into(),
                cached_only: false,
                prefetch: false
            })
        );
        assert!(parse_request("/42", Some(&format!("v={V}&r=2"))).is_some());
        assert!(parse_request("/42", Some(&format!("v={V}&c=1&p=1&r=999"))).is_some());
        assert!(
            parse_request("/42", Some(&format!("v={V}&c=1")))
                .unwrap()
                .cached_only
        );
        assert!(
            parse_request("/42", Some(&format!("v={V}&p=1")))
                .unwrap()
                .prefetch
        );
    }

    #[test]
    fn rejects_paths_and_malformed_parameters() {
        let v = format!("v={V}");
        for (path, query) in [
            ("/", Some(v.as_str())),
            ("/0", Some(v.as_str())),
            ("/-1", Some(v.as_str())),
            ("/1a", Some(v.as_str())),
            ("/..%2Fetc", Some(v.as_str())),
            ("/srv/media/a.jpg", Some(v.as_str())),
            ("/1/2", Some(v.as_str())),
            ("/99999999999999999999", Some(v.as_str())),
            ("/1", None),
            ("/1", Some("v=0123")),
            ("/1", Some("v=0123456789ABCDEF")),
            ("/1", Some("v=0123456789abcdeg")),
            ("/1", Some(&format!("v={V}&v={V}"))),
            ("/1", Some(&format!("v={V}&path=/etc/passwd"))),
            ("/1", Some(&format!("v={V}&r=abc"))),
            ("/1", Some(&format!("v={V}&s=780"))),
            ("/1", Some(&format!("v={V}&c=0"))),
            ("/1", Some(&format!("v={V}&c=1&c=1"))),
            ("/1", Some(&format!("v={V}&p=2"))),
            ("/1", Some(&format!("v={V}&p=1&p=1"))),
            ("/1", Some(&format!("v={V}&r=1&r=2"))),
            ("/1", Some(&format!("v={V}&r=1&c=1&r=1"))),
        ] {
            assert_eq!(parse_request(path, query), None, "{path} {query:?}");
        }
    }

    #[test]
    fn origin_is_echoed_only_when_allowed() {
        assert_eq!(
            allowed_origin(Some("http://localhost:1420")),
            "http://localhost:1420"
        );
        assert_eq!(
            allowed_origin(Some("tauri://localhost")),
            "tauri://localhost"
        );
        assert_eq!(
            allowed_origin(Some("https://example.com")),
            "tauri://localhost"
        );
        assert_eq!(allowed_origin(None), "tauri://localhost");
    }

    #[test]
    fn matching_version_is_immutable_and_stale_url_is_not_cached() {
        let served = ServedThumbnail {
            bytes: vec![1, 2, 3],
            version: V.into(),
        };
        let fresh = image_response(served.clone(), V, "tauri://localhost");
        assert_eq!(fresh.status, STATUS_OK);
        assert_eq!(fresh.header(CONTENT_TYPE), Some("image/jpeg"));
        assert_eq!(fresh.header(CACHE_CONTROL), Some(IMMUTABLE));
        assert_eq!(
            fresh.header(ACCESS_CONTROL_ALLOW_ORIGIN),
            Some("tauri://localhost")
        );
        assert_eq!(fresh.body, [1, 2, 3]);

        let stale = image_response(served, "ffffffffffffffff", "tauri://localhost");
        assert_eq!(stale.header(CACHE_CONTROL), Some(NO_STORE));
    }

    #[test]
    fn legacy_file_with_empty_version_is_not_cached() {
        // `thumb_service` serves a kept legacy file with an empty token, which
        // never matches a requested version.
        let legacy = ServedThumbnail {
            bytes: vec![1, 2, 3],
            version: String::new(),
        };
        let response = image_response(legacy, V, "tauri://localhost");
        assert_eq!(response.status, STATUS_OK);
        assert_eq!(response.header(CACHE_CONTROL), Some(NO_STORE));
    }

    #[test]
    fn error_responses_are_uncached_and_503_asks_for_retry() {
        let missing = not_found("tauri://localhost");
        assert_eq!(missing.status, STATUS_NOT_FOUND);
        assert_eq!(missing.header(CACHE_CONTROL), Some(NO_STORE));
        let busy = unavailable("tauri://localhost");
        assert_eq!(busy.status, STATUS_SERVICE_UNAVAILABLE);
        assert_eq!(busy.header(RETRY_AFTER), Some(RETRY_AFTER_SECONDS));
        assert_eq!(bad_request("tauri://localhost").status, STATUS_BAD_REQUEST);
    }

    mod serving {
        use std::{
            fs,
            path::{Path, PathBuf},
            sync::{
                atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
                mpsc, Arc, Mutex, RwLock,
            },
            time::Duration,
        };

        use tempfile::tempdir;

        use super::super::*;
        use super::V;
        use crate::{
            db,
            models::NewAsset,
            services::thumb_scheduler::{ThumbnailScheduler, ThumbnailTask},
        };

        const TIMEOUT: Duration = Duration::from_secs(5);

        /// A runtime without Tauri: the state is managed or not, and blocking
        /// work runs on a plain thread.
        #[derive(Clone)]
        struct TestHost(Option<Arc<AppState>>);

        impl ThumbHost for TestHost {
            fn app_state(&self) -> Option<&AppState> {
                self.0.as_deref()
            }

            fn spawn_blocking<F: FnOnce() + Send + 'static>(task: F) {
                std::thread::spawn(task);
            }
        }

        fn responder(tx: &mpsc::Sender<(i64, ThumbReply)>, asset_id: i64) -> ThumbResponder {
            let tx = tx.clone();
            Box::new(move |reply| {
                let _ = tx.send((asset_id, reply));
            })
        }

        fn request(asset_id: i64, cached_only: bool, prefetch: bool) -> ThumbRequest {
            ThumbRequest {
                asset_id,
                version: V.into(),
                cached_only,
                prefetch,
            }
        }

        fn state_with_renderer(
            db_path: &Path,
            thumbs: &Path,
            processor: impl Fn(&Path, &ThumbnailTask) -> Option<String> + Send + Sync + 'static,
        ) -> AppState {
            AppState {
                database: crate::services::db_pool::DatabaseRuntime::new(db_path.to_path_buf()),
                db_path: db_path.to_path_buf(),
                thumbs_dir: thumbs.to_path_buf(),
                ffmpeg_path: PathBuf::from("ffmpeg"),
                scan_lock: Mutex::new(()),
                thumb_lock: RwLock::new(()),
                thumb_scheduler: ThumbnailScheduler::with_processor(
                    1,
                    PathBuf::from("ffmpeg"),
                    Arc::new(processor),
                ),
                thumbnail_render_all_running: AtomicBool::new(false),
                thumbnail_render_all_cancel_requested: AtomicBool::new(false),
                thumbnail_generation: AtomicU64::new(0),
            }
        }

        /// A library of `count` image assets with sources and no thumbnails.
        fn library(dir: &Path, count: i64) -> (PathBuf, PathBuf) {
            let db_path = dir.join("media.db");
            let thumbs = dir.join("thumbs");
            fs::create_dir_all(&thumbs).unwrap();
            let conn = db::open_connection(&db_path).unwrap();
            db::init_schema(&conn).unwrap();
            for id in 1..=count {
                let source = dir.join(format!("source-{id}.png"));
                fs::write(&source, b"source").unwrap();
                db::upsert_asset(
                    &conn,
                    &NewAsset {
                        path: source.to_string_lossy().to_string(),
                        kind: "image".to_string(),
                        size_bytes: 10,
                        modified_at: 1,
                        width: Some(100),
                        height: Some(100),
                        duration_ms: None,
                        thumb_path: None,
                    },
                )
                .unwrap();
            }
            (db_path, thumbs)
        }

        #[test]
        fn serve_answers_503_until_app_state_exists() {
            let (tx, rx) = mpsc::channel();
            serve(
                TestHost(None),
                request(1, false, false),
                "http://localhost:1420",
                responder(&tx, 1),
            );
            let (_, reply) = rx.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(reply.status, STATUS_SERVICE_UNAVAILABLE);
            assert_eq!(reply.header(RETRY_AFTER), Some(RETRY_AFTER_SECONDS));
            assert_eq!(reply.header(CACHE_CONTROL), Some(NO_STORE));
            assert_eq!(
                reply.header(ACCESS_CONTROL_ALLOW_ORIGIN),
                Some("http://localhost:1420")
            );
            assert!(reply.body.is_empty());
        }

        #[test]
        fn cached_only_request_for_a_missing_thumbnail_is_404_without_a_render() {
            let dir = tempdir().unwrap();
            let (db_path, thumbs) = library(dir.path(), 1);
            let renders = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&renders);
            let state =
                state_with_renderer(&db_path, &thumbs, move |_: &Path, task: &ThumbnailTask| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    fs::write(&task.target_path, b"thumbnail").unwrap();
                    Some(task.target_path.to_string_lossy().into())
                });
            let (tx, rx) = mpsc::channel();
            serve(
                TestHost(Some(Arc::new(state))),
                request(1, true, false),
                "tauri://localhost",
                responder(&tx, 1),
            );
            let (_, reply) = rx.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(reply.status, STATUS_NOT_FOUND);
            assert_eq!(reply.header(CACHE_CONTROL), Some(NO_STORE));
            assert_eq!(renders.load(Ordering::SeqCst), 0);
        }

        #[test]
        fn visible_requests_render_lifo_ahead_of_prefetch_and_every_request_is_answered() {
            // The only test here that reaches the process-wide publisher.
            let dir = tempdir().unwrap();
            let (db_path, thumbs) = library(dir.path(), 5);
            let order = Arc::new(Mutex::new(Vec::new()));
            let (started_tx, started_rx) = mpsc::channel::<()>();
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let release_rx = Mutex::new(release_rx);
            let order_clone = Arc::clone(&order);
            let state =
                state_with_renderer(&db_path, &thumbs, move |_: &Path, task: &ThumbnailTask| {
                    if task.asset_id == 1 {
                        started_tx.send(()).unwrap();
                        release_rx.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
                    }
                    order_clone.lock().unwrap().push(task.asset_id);
                    fs::write(&task.target_path, format!("thumb-{}", task.asset_id)).unwrap();
                    Some(task.target_path.to_string_lossy().into())
                });
            let host = TestHost(Some(Arc::new(state)));
            let (tx, rx) = mpsc::channel();
            let send = |asset_id: i64, prefetch: bool| {
                serve(
                    host.clone(),
                    request(asset_id, false, prefetch),
                    "tauri://localhost",
                    responder(&tx, asset_id),
                );
            };
            // Asset 1 occupies the only worker.
            send(1, false);
            started_rx.recv_timeout(TIMEOUT).unwrap();
            // Overscan tiles 2 and 3, then visible tiles 4 and 5.
            send(2, true);
            send(3, true);
            send(4, false);
            send(5, false);
            // Tile 3 scrolled into view while its prefetch was queued (ADR 0021).
            send(3, false);
            release_tx.send(()).unwrap();

            let mut replies: Vec<_> = (0..6).map(|_| rx.recv_timeout(TIMEOUT).unwrap()).collect();
            // Visible requests newest first, the promoted prefetch on top,
            // the remaining prefetch last.
            assert_eq!(*order.lock().unwrap(), vec![1, 3, 5, 4, 2]);
            replies.sort_by_key(|(asset_id, _)| *asset_id);
            let answered: Vec<_> = replies
                .into_iter()
                .map(|(asset_id, reply)| {
                    assert_eq!(reply.status, STATUS_OK, "asset {asset_id}");
                    (asset_id, String::from_utf8(reply.body).unwrap())
                })
                .collect();
            let expected: Vec<_> = [1, 2, 3, 3, 4, 5]
                .into_iter()
                .map(|id| (id, format!("thumb-{id}")))
                .collect();
            assert_eq!(answered, expected);
        }
    }
}
