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

use std::borrow::Cow;

use tauri::{
    http::{header, Request, Response, StatusCode},
    AppHandle, Manager, Runtime, UriSchemeResponder,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbRequest {
    pub asset_id: i64,
    pub version: String,
    pub cached_only: bool,
    pub prefetch: bool,
}

/// Parses `/<id>?v=<hex>[&c=1][&r=<n>]`; anything else is rejected. `r`
/// (frontend retry counter) is allowed and ignored.
pub fn parse_request(path: &str, query: Option<&str>) -> Option<ThumbRequest> {
    let id_text = path.strip_prefix('/')?;
    if id_text.is_empty() || id_text.len() > 19 || !id_text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let asset_id = id_text.parse::<i64>().ok().filter(|id| *id > 0)?;
    let mut version = None;
    let mut cached_only = false;
    let mut prefetch = false;
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
            "r" if !value.is_empty()
                && value.len() <= 3
                && value.bytes().all(|b| b.is_ascii_digit()) => {}
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

fn allowed_origin(request_origin: Option<&str>) -> &'static str {
    request_origin
        .and_then(|origin| {
            ALLOWED_ORIGINS
                .iter()
                .copied()
                .find(|allowed| *allowed == origin)
        })
        .unwrap_or(ALLOWED_ORIGINS[0])
}

fn empty(status: StatusCode, origin: &'static str) -> Response<Cow<'static, [u8]>> {
    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin)
        .header(header::CACHE_CONTROL, NO_STORE);
    if status == StatusCode::SERVICE_UNAVAILABLE {
        builder = builder.header(header::RETRY_AFTER, RETRY_AFTER_SECONDS);
    }
    builder
        .body(Cow::Borrowed(&[][..]))
        .expect("static response parts are valid")
}

/// 200 with the JPEG. Only a response whose bytes match the requested
/// version is immutable; a stale URL gets the current picture uncached.
pub fn image_response(
    served: ServedThumbnail,
    requested_version: &str,
    origin: &'static str,
) -> Response<Cow<'static, [u8]>> {
    let cache = if served.version == requested_version {
        IMMUTABLE
    } else {
        NO_STORE
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin)
        .header(header::CACHE_CONTROL, cache)
        .body(Cow::Owned(served.bytes))
        .expect("static response parts are valid")
}

pub fn not_found(origin: &'static str) -> Response<Cow<'static, [u8]>> {
    empty(StatusCode::NOT_FOUND, origin)
}

pub fn unavailable(origin: &'static str) -> Response<Cow<'static, [u8]>> {
    empty(StatusCode::SERVICE_UNAVAILABLE, origin)
}

pub fn bad_request(origin: &'static str) -> Response<Cow<'static, [u8]>> {
    empty(StatusCode::BAD_REQUEST, origin)
}

/// Entry point registered on the builder. Runs on the GTK main thread, so it
/// only parses and hands the work to the blocking pool.
pub fn handle<R: Runtime>(
    app: &AppHandle<R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let origin = allowed_origin(
        request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok()),
    );
    if request.method() != tauri::http::Method::GET {
        responder.respond(bad_request(origin));
        return;
    }
    let Some(parsed) = parse_request(request.uri().path(), request.uri().query()) else {
        responder.respond(bad_request(origin));
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || serve(app, parsed, origin, responder));
}

fn serve<R: Runtime>(
    app: AppHandle<R>,
    request: ThumbRequest,
    origin: &'static str,
    responder: UriSchemeResponder,
) {
    // Setup manages AppState last; requests before that are retried.
    let Some(state) = app.try_state::<AppState>() else {
        responder.respond(unavailable(origin));
        return;
    };
    let permit = match state.database.admit() {
        Ok(permit) => permit,
        Err(_) => {
            responder.respond(unavailable(origin));
            return;
        }
    };

    // The responder moves into the render callback only when a render is
    // enqueued; a shared slot lets the immediate path still answer.
    let slot = std::sync::Arc::new(std::sync::Mutex::new(Some(responder)));
    let callback_slot = std::sync::Arc::clone(&slot);
    let callback_app = app.clone();
    let requested_version = request.version.clone();
    let outcome = thumb_service::request_thumbnail(
        request.asset_id,
        thumb_service::ThumbnailRequestMode {
            cached_only: request.cached_only,
            prefetch: request.prefetch,
        },
        &state,
        &permit,
        move |pending| {
            if let Some(responder) = take(&callback_slot) {
                publisher().push(
                    callback_app,
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
        // Full queue, pool exhaustion or maintenance: transient.
        Err(_) => unavailable(origin),
    };
    if let Some(responder) = take(&slot) {
        responder.respond(response);
    }
}

/// Most finished renders published in one transaction.
const PUBLISH_BATCH: usize = 64;

struct Finished {
    pending: thumb_service::PendingThumbnail,
    responder: UriSchemeResponder,
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
    fn push<R: Runtime>(&'static self, app: AppHandle<R>, finished: Finished) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.waiting.push(finished);
        if state.running {
            return;
        }
        state.running = true;
        drop(state);
        tauri::async_runtime::spawn_blocking(move || self.drain(&app));
    }

    fn drain<R: Runtime>(&self, app: &AppHandle<R>) {
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
            publish_batch(app, batch);
        }
    }
}

fn publish_batch<R: Runtime>(app: &AppHandle<R>, batch: Vec<Finished>) {
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
    let served = app.try_state::<AppState>().map(|state| {
        state
            .database
            .admit()
            .and_then(|permit| thumb_service::finish_thumbnail_requests(pending, &state, &permit))
    });
    match served {
        Some(Ok(served)) => {
            for ((responder, version, origin), served) in replies.into_iter().zip(served) {
                responder.respond(match served {
                    Some(served) => image_response(served, &version, origin),
                    // Stale (record changed or thumbnails cleared) or failed:
                    // the frontend retries and a fresh request renders again.
                    None => not_found(origin),
                });
            }
        }
        _ => {
            for (responder, _, origin) in replies {
                responder.respond(unavailable(origin));
            }
        }
    }
}

fn take(slot: &std::sync::Mutex<Option<UriSchemeResponder>>) -> Option<UriSchemeResponder> {
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
        assert_eq!(fresh.status(), StatusCode::OK);
        assert_eq!(fresh.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(fresh.headers()[header::CACHE_CONTROL], IMMUTABLE);
        assert_eq!(
            fresh.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "tauri://localhost"
        );
        assert_eq!(fresh.body().as_ref(), &[1, 2, 3]);

        let stale = image_response(served, "ffffffffffffffff", "tauri://localhost");
        assert_eq!(stale.headers()[header::CACHE_CONTROL], NO_STORE);
    }

    #[test]
    fn error_responses_are_uncached_and_503_asks_for_retry() {
        let missing = not_found("tauri://localhost");
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_eq!(missing.headers()[header::CACHE_CONTROL], NO_STORE);
        let busy = unavailable("tauri://localhost");
        assert_eq!(busy.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(busy.headers()[header::RETRY_AFTER], RETRY_AFTER_SECONDS);
        assert_eq!(
            bad_request("tauri://localhost").status(),
            StatusCode::BAD_REQUEST
        );
    }
}
