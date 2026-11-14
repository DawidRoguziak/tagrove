//! Tauri side of the `thumb://` scheme: takes the WebKit request, answers a
//! malformed one on the GTK main thread, runs `services::thumb_protocol::serve`
//! on the blocking pool and turns each `ThumbReply` into an HTTP response.
//! The managed `AppState` comes from `try_state`: until setup manages it, the
//! core answers `503` (see `services/thumb_protocol.rs`).

use std::borrow::Cow;

use tauri::{
    http::{header, Request, Response},
    AppHandle, Manager, Runtime, UriSchemeResponder,
};

use media_tagger_core::{
    app::state::AppState,
    services::thumb_protocol::{self, ThumbHost, ThumbReply},
};

/// Entry point registered on the builder. Runs on the GTK main thread, so it
/// only parses and hands the work to the blocking pool.
pub fn handle<R: Runtime>(
    app: &AppHandle<R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let origin = thumb_protocol::allowed_origin(
        request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok()),
    );
    if request.method() != tauri::http::Method::GET {
        responder.respond(into_response(thumb_protocol::bad_request(origin)));
        return;
    }
    let Some(parsed) = thumb_protocol::parse_request(request.uri().path(), request.uri().query())
    else {
        responder.respond(into_response(thumb_protocol::bad_request(origin)));
        return;
    };
    let host = TauriThumbHost(app.clone());
    tauri::async_runtime::spawn_blocking(move || {
        thumb_protocol::serve(
            host,
            parsed,
            origin,
            Box::new(move |reply| responder.respond(into_response(reply))),
        )
    });
}

/// The app handle as the core's `thumb://` runtime port.
struct TauriThumbHost<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Clone for TauriThumbHost<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<R: Runtime> ThumbHost for TauriThumbHost<R> {
    fn app_state(&self) -> Option<&AppState> {
        // Setup manages AppState last; until then this is `None`.
        self.0.try_state::<AppState>().map(|state| state.inner())
    }

    fn spawn_blocking<F: FnOnce() + Send + 'static>(task: F) {
        tauri::async_runtime::spawn_blocking(task);
    }
}

fn into_response(reply: ThumbReply) -> Response<Cow<'static, [u8]>> {
    let mut builder = Response::builder().status(reply.status);
    for (name, value) in reply.headers {
        builder = builder.header(name, value);
    }
    builder
        .body(Cow::Owned(reply.body))
        .expect("static response parts are valid")
}

#[cfg(test)]
mod tests {
    use tauri::http::StatusCode;

    use super::*;
    use media_tagger_core::services::thumb_service::ServedThumbnail;

    #[test]
    fn reply_becomes_an_http_response() {
        let served = ServedThumbnail {
            bytes: vec![1, 2, 3],
            version: "0123456789abcdef".into(),
        };
        let image = into_response(thumb_protocol::image_response(
            served,
            "0123456789abcdef",
            "http://localhost:1420",
        ));
        assert_eq!(image.status(), StatusCode::OK);
        assert_eq!(image.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(
            image.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "http://localhost:1420"
        );
        assert_eq!(
            image.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
        assert_eq!(image.headers().len(), 3);
        assert_eq!(image.body().as_ref(), &[1, 2, 3]);

        let busy = into_response(thumb_protocol::unavailable("tauri://localhost"));
        assert_eq!(busy.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(busy.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(busy.headers()[header::RETRY_AFTER], "1");
        assert_eq!(
            busy.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "tauri://localhost"
        );
        assert!(busy.body().is_empty());

        let bad = into_response(thumb_protocol::bad_request("tauri://localhost"));
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        assert!(!bad.headers().contains_key(header::RETRY_AFTER));
    }
}
