use std::{
    convert::Infallible,
    future::Future,
    io,
    net::{Ipv4Addr, SocketAddrV4, TcpListener as StdTcpListener},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{mpsc, Arc},
    task::{Context as TaskContext, Poll},
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::Context;
use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Empty, StreamBody};
use hyper::{
    body::{Frame, Incoming},
    header::{
        HeaderName, HeaderValue, ACCEPT_RANGES, ALLOW, CACHE_CONTROL, CONTENT_LENGTH,
        CONTENT_RANGE, CONTENT_TYPE, RANGE,
    },
    server::conn::http1,
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use rand::{rngs::OsRng, RngCore};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, ReadBuf, SeekFrom},
    net::{TcpListener, TcpStream},
    runtime::Builder as RuntimeBuilder,
    sync::{oneshot, Mutex, Semaphore},
    task::JoinSet,
    time::{sleep, timeout, Sleep},
};
use tokio_util::io::ReaderStream;

use crate::db;

const MAX_HEADER_BYTES: usize = 32 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const MAX_ACTIVE_CONNECTIONS: usize = 32;
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONSECUTIVE_ACCEPT_ERRORS: u32 = 8;
const INITIAL_ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(25);
const MAX_ACCEPT_RETRY_DELAY: Duration = Duration::from_secs(1);
const X_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");

type ResponseBody = UnsyncBoxBody<Bytes, io::Error>;

pub struct MediaServerState {
    base_url: String,
    token: String,
    db_path: PathBuf,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    failure: Arc<std::sync::Mutex<Option<String>>>,
}

impl MediaServerState {
    pub fn start(db_path: PathBuf) -> anyhow::Result<Self> {
        let listener = StdTcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .context("cannot bind video media server")?;
        listener
            .set_nonblocking(true)
            .context("cannot configure video media server")?;
        let address = listener.local_addr()?;
        let token = process_token()?;
        let server_token = token.clone();
        let server_db_path = db_path.clone();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let failure = Arc::new(std::sync::Mutex::new(None));
        let server_failure = Arc::clone(&failure);
        let thread = thread::Builder::new()
            .name("video-media-server".to_string())
            .spawn(move || {
                run_server(
                    listener,
                    server_db_path,
                    server_token,
                    shutdown_rx,
                    ready_tx,
                    server_failure,
                )
            })
            .context("cannot start video media server")?;
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = thread.join();
                anyhow::bail!(error);
            }
            Err(_) => {
                let _ = thread.join();
                anyhow::bail!("video media server stopped during startup");
            }
        }

        Ok(Self {
            base_url: format!("http://127.0.0.1:{}", address.port()),
            token,
            db_path,
            shutdown_tx: Some(shutdown_tx),
            thread: Some(thread),
            failure,
        })
    }

    pub fn video_url(&self, asset_id: i64) -> anyhow::Result<String> {
        if let Some(error) = self
            .failure
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
        {
            anyhow::bail!("video media server is unavailable: {error}");
        }
        resolve_video_path(&self.db_path, asset_id)?;
        Ok(format!(
            "{}/{}/video/{asset_id}.mp4",
            self.base_url, self.token,
        ))
    }
}

impl Drop for MediaServerState {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn process_token() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .context("cannot generate video media server token")?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(token)
}

fn run_server(
    listener: StdTcpListener,
    db_path: PathBuf,
    token: String,
    shutdown_rx: oneshot::Receiver<()>,
    ready_tx: mpsc::SyncSender<std::result::Result<(), String>>,
    failure: Arc<std::sync::Mutex<Option<String>>>,
) {
    let runtime = match RuntimeBuilder::new_multi_thread()
        .enable_all()
        .thread_name("video-media-io")
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready_tx.send(Err(format!("cannot start video media runtime: {error}")));
            return;
        }
    };

    runtime.block_on(async move {
        let listener = match TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                let _ = ready_tx.send(Err(format!(
                    "cannot configure video media listener: {error}"
                )));
                return;
            }
        };
        let db_path = Arc::new(db_path);
        let token: Arc<str> = Arc::from(token);
        let connection_slots = Arc::new(Semaphore::new(MAX_ACTIVE_CONNECTIONS));
        let mut connections = JoinSet::new();
        let mut consecutive_accept_errors = 0_u32;
        let mut accept_retry_delay = INITIAL_ACCEPT_RETRY_DELAY;
        let _ = ready_tx.send(Ok(()));
        let mut shutdown_rx = shutdown_rx;

        loop {
            let accepted = tokio::select! {
                _ = &mut shutdown_rx => break,
                completed = connections.join_next(), if !connections.is_empty() => {
                    let _ = completed;
                    continue;
                }
                accepted = listener.accept() => accepted,
            };
            let (stream, _) = match accepted {
                Ok(accepted) => {
                    consecutive_accept_errors = 0;
                    accept_retry_delay = INITIAL_ACCEPT_RETRY_DELAY;
                    accepted
                }
                Err(error) => {
                    consecutive_accept_errors += 1;
                    if consecutive_accept_errors >= MAX_CONSECUTIVE_ACCEPT_ERRORS {
                        *failure
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(format!(
                            "listener failed after {consecutive_accept_errors} attempts: {error}"
                        ));
                        break;
                    }
                    tokio::select! {
                        _ = &mut shutdown_rx => break,
                        _ = sleep(accept_retry_delay) => {}
                    }
                    accept_retry_delay = (accept_retry_delay * 2).min(MAX_ACCEPT_RETRY_DELAY);
                    continue;
                }
            };
            let Ok(connection_slot) = Arc::clone(&connection_slots).try_acquire_owned() else {
                continue;
            };
            let db_path = Arc::clone(&db_path);
            let token = Arc::clone(&token);
            connections.spawn(async move {
                let _connection_slot = connection_slot;
                serve_connection(stream, db_path, token).await;
            });
        }

        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
}

async fn serve_connection(stream: TcpStream, db_path: Arc<PathBuf>, token: Arc<str>) {
    let (request_started_tx, request_started_rx) = oneshot::channel();
    let request_started_tx = Arc::new(Mutex::new(Some(request_started_tx)));
    let service = service_fn(move |request| {
        let db_path = Arc::clone(&db_path);
        let token = Arc::clone(&token);
        let request_started_tx = Arc::clone(&request_started_tx);
        async move {
            if let Some(sender) = request_started_tx.lock().await.take() {
                let _ = sender.send(());
            }
            match timeout(REQUEST_TIMEOUT, handle_request(request, db_path, token)).await {
                Ok(response) => response,
                Err(_) => Ok(empty_response(StatusCode::GATEWAY_TIMEOUT)),
            }
        }
    });
    let io = TokioIo::new(WriteTimeoutIo::new(stream, WRITE_IDLE_TIMEOUT));
    let connection = http1::Builder::new()
        .keep_alive(false)
        .max_buf_size(MAX_HEADER_BYTES)
        .serve_connection(io, service);
    tokio::pin!(connection);

    tokio::select! {
        _ = sleep(HEADER_READ_TIMEOUT) => {}
        _ = request_started_rx => {
            let _ = connection.await;
        }
        _ = &mut connection => {}
    }
}

struct WriteTimeoutIo {
    inner: TcpStream,
    timeout: Duration,
    deadline: Option<Pin<Box<Sleep>>>,
}

impl WriteTimeoutIo {
    fn new(inner: TcpStream, timeout: Duration) -> Self {
        Self {
            inner,
            timeout,
            deadline: None,
        }
    }

    fn poll_deadline(&mut self, cx: &mut TaskContext<'_>) -> Poll<io::Result<usize>> {
        let timeout = self.timeout;
        let deadline = self
            .deadline
            .get_or_insert_with(|| Box::pin(sleep(timeout)));
        match deadline.as_mut().poll(cx) {
            Poll::Ready(()) => {
                self.deadline = None;
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "video media response write timed out",
                )))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncRead for WriteTimeoutIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buffer)
    }
}

impl AsyncWrite for WriteTimeoutIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_write(cx, buffer) {
            Poll::Ready(result) => {
                self.deadline = None;
                Poll::Ready(result)
            }
            Poll::Pending => self.poll_deadline(cx),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_flush(cx) {
            Poll::Ready(result) => {
                self.deadline = None;
                Poll::Ready(result)
            }
            Poll::Pending => match self.poll_deadline(cx) {
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(_)) | Poll::Pending => Poll::Pending,
            },
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

async fn handle_request(
    request: Request<Incoming>,
    db_path: Arc<PathBuf>,
    token: Arc<str>,
) -> std::result::Result<Response<ResponseBody>, Infallible> {
    let method = request.method().clone();
    if method != Method::GET && method != Method::HEAD {
        let mut response = empty_response(StatusCode::METHOD_NOT_ALLOWED);
        response
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static("GET, HEAD"));
        return Ok(response);
    }

    let route_prefix = format!("/{token}/video/");
    let Some(asset_id) = request
        .uri()
        .path()
        .strip_prefix(&route_prefix)
        .and_then(|value| value.strip_suffix(".mp4"))
        .and_then(|id| id.parse::<i64>().ok())
        .filter(|id| *id > 0)
    else {
        return Ok(empty_response(StatusCode::NOT_FOUND));
    };

    let range_header = request.headers().get(RANGE).cloned();
    let path =
        match tokio::task::spawn_blocking(move || resolve_video_path(&db_path, asset_id)).await {
            Ok(Ok(path)) => path,
            _ => return Ok(empty_response(StatusCode::NOT_FOUND)),
        };
    let mut file = match File::open(&path).await {
        Ok(file) => file,
        Err(_) => return Ok(empty_response(StatusCode::NOT_FOUND)),
    };
    let length = match file.metadata().await {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        _ => return Ok(empty_response(StatusCode::NOT_FOUND)),
    };
    let byte_range = match (method == Method::GET, range_header.as_ref()) {
        (true, Some(value)) => match value
            .to_str()
            .map_err(|_| ())
            .and_then(|value| parse_range(value, length))
        {
            Ok(range) => Some(range),
            Err(()) => {
                let mut response = empty_response(StatusCode::RANGE_NOT_SATISFIABLE);
                response.headers_mut().insert(
                    CONTENT_RANGE,
                    HeaderValue::from_str(&format!("bytes */{length}"))
                        .expect("valid content range"),
                );
                return Ok(response);
            }
        },
        _ => None,
    };
    let (start, end, status) = byte_range
        .map(|(start, end)| (start, end, StatusCode::PARTIAL_CONTENT))
        .unwrap_or((0, length.saturating_sub(1), StatusCode::OK));
    let response_length = if length == 0 { 0 } else { end - start + 1 };
    let content_type = video_content_type(&path);

    let body = if method == Method::GET && response_length > 0 {
        if file.seek(SeekFrom::Start(start)).await.is_err() {
            return Ok(empty_response(StatusCode::NOT_FOUND));
        }
        StreamBody::new(
            ReaderStream::with_capacity(file.take(response_length), COPY_BUFFER_BYTES)
                .map_ok(Frame::data),
        )
        .boxed_unsync()
    } else {
        empty_body()
    };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&response_length.to_string()).expect("valid content length"),
    );
    headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    insert_safety_headers(headers);
    if byte_range.is_some() {
        headers.insert(
            CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{length}"))
                .expect("valid content range"),
        );
    }
    Ok(response)
}

fn resolve_video_path(db_path: &Path, asset_id: i64) -> anyhow::Result<PathBuf> {
    let conn = db::open_connection_read_only(db_path)?;
    let asset = db::get_asset_for_thumbnail(&conn, asset_id)?
        .with_context(|| format!("asset {asset_id} not found"))?;
    if asset.kind != "video" {
        anyhow::bail!("asset {asset_id} is not a video");
    }
    let path = PathBuf::from(asset.path);
    let metadata = path.metadata().context("video file is unavailable")?;
    if !metadata.is_file() {
        anyhow::bail!("video path is not a file");
    }
    Ok(path)
}

fn parse_range(value: &str, length: u64) -> std::result::Result<(u64, u64), ()> {
    let raw = value.strip_prefix("bytes=").ok_or(())?;
    if length == 0 || raw.contains(',') {
        return Err(());
    }
    let (start, end) = raw.split_once('-').ok_or(())?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        let count = suffix.min(length);
        return Ok((length - count, length - 1));
    }

    let start = start.parse::<u64>().map_err(|_| ())?;
    if start >= length {
        return Err(());
    }
    let end = if end.is_empty() {
        length - 1
    } else {
        end.parse::<u64>().map_err(|_| ())?.min(length - 1)
    };
    if end < start {
        return Err(());
    }
    Ok((start, end))
}

fn video_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("avi") => "video/x-msvideo",
        Some("mkv") => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

fn empty_body() -> ResponseBody {
    Empty::<Bytes>::new()
        .map_err(|error| match error {})
        .boxed_unsync()
}

fn empty_response(status: StatusCode) -> Response<ResponseBody> {
    let mut response = Response::new(empty_body());
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_LENGTH, HeaderValue::from_static("0"));
    insert_safety_headers(response.headers_mut());
    response
}

fn insert_safety_headers(headers: &mut hyper::HeaderMap) {
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpStream,
    };
    use tempfile::tempdir;

    fn request(url: &str, method: &str, range: Option<&str>) -> Vec<u8> {
        let without_scheme = url.strip_prefix("http://").expect("http URL");
        let (authority, path) = without_scheme.split_once('/').expect("URL path");
        let mut stream = TcpStream::connect(authority).expect("connect to media server");
        write!(stream, "{method} /{path} HTTP/1.1\r\nHost: {authority}\r\n").expect("request line");
        if let Some(range) = range {
            write!(stream, "Range: {range}\r\n").expect("range header");
        }
        write!(stream, "Connection: close\r\n\r\n").expect("request end");
        let mut response = Vec::new();
        stream.read_to_end(&mut response).expect("response");
        response
    }

    fn response_parts(response: &[u8]) -> (&str, &[u8]) {
        let separator = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("header separator");
        (
            std::str::from_utf8(&response[..separator]).expect("response headers"),
            &response[separator + 4..],
        )
    }

    #[test]
    fn parses_full_open_and_suffix_ranges() {
        assert_eq!(parse_range("bytes=2-5", 10), Ok((2, 5)));
        assert_eq!(parse_range("bytes=4-", 10), Ok((4, 9)));
        assert_eq!(parse_range("bytes=-3", 10), Ok((7, 9)));
        assert_eq!(parse_range("bytes=-20", 10), Ok((0, 9)));
        assert_eq!(parse_range("bytes=2-50", 10), Ok((2, 9)));
    }

    #[test]
    fn rejects_invalid_and_unsatisfiable_ranges() {
        for value in [
            "items=0-1",
            "bytes=",
            "bytes=5-2",
            "bytes=10-",
            "bytes=-0",
            "bytes=0-1,3-4",
        ] {
            assert_eq!(parse_range(value, 10), Err(()), "{value}");
        }
        assert_eq!(parse_range("bytes=0-", 0), Err(()));
    }

    #[test]
    fn resolves_only_existing_video_assets() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("media.db");
        let video_path = temp.path().join("clip.mp4");
        let image_path = temp.path().join("photo.jpg");
        std::fs::write(&video_path, b"video bytes").expect("video fixture");
        std::fs::write(&image_path, b"image bytes").expect("image fixture");
        let conn = db::open_connection(&db_path).expect("database");
        db::init_schema(&conn).expect("schema");
        conn.execute(
            "INSERT INTO assets (id, path, file_name, kind, size_bytes, modified_at) VALUES (?1, ?2, ?3, ?4, 1, 1)",
            rusqlite::params![
                1_i64,
                video_path.to_string_lossy().to_string(),
                "clip.mp4",
                "video"
            ],
        )
        .expect("video row");
        conn.execute(
            "INSERT INTO assets (id, path, file_name, kind, size_bytes, modified_at) VALUES (?1, ?2, ?3, ?4, 1, 1)",
            rusqlite::params![
                2_i64,
                image_path.to_string_lossy().to_string(),
                "photo.jpg",
                "image"
            ],
        )
        .expect("image row");
        drop(conn);

        assert_eq!(resolve_video_path(&db_path, 1).expect("video"), video_path);
        assert!(resolve_video_path(&db_path, 2).is_err());
        assert!(resolve_video_path(&db_path, 99).is_err());
        std::fs::remove_file(&video_path).expect("remove fixture");
        assert!(resolve_video_path(&db_path, 1).is_err());
    }

    #[test]
    fn streams_get_head_and_byte_ranges() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("media.db");
        let video_path = temp.path().join("clip.mp4");
        let image_path = temp.path().join("photo.jpg");
        std::fs::write(&video_path, b"abcdefghij").expect("video fixture");
        std::fs::write(&image_path, b"image").expect("image fixture");
        let conn = db::open_connection(&db_path).expect("database");
        db::init_schema(&conn).expect("schema");
        conn.execute(
            "INSERT INTO assets (id, path, file_name, kind, size_bytes, modified_at) VALUES (1, ?1, 'clip.mp4', 'video', 10, 1)",
            rusqlite::params![video_path.to_string_lossy().to_string()],
        )
        .expect("video row");
        conn.execute(
            "INSERT INTO assets (id, path, file_name, kind, size_bytes, modified_at) VALUES (2, ?1, 'photo.jpg', 'image', 5, 1)",
            rusqlite::params![image_path.to_string_lossy().to_string()],
        )
        .expect("image row");
        drop(conn);
        let server = MediaServerState::start(db_path).expect("media server");
        let url = server.video_url(1).expect("video URL");

        let response = request(&url, "GET", None);
        let (headers, body) = response_parts(&response);
        let normalized_headers = headers.to_ascii_lowercase();
        assert!(headers.starts_with("HTTP/1.1 200 OK"));
        assert!(normalized_headers.contains("content-type: video/mp4"));
        assert!(normalized_headers.contains("content-length: 10"));
        assert!(normalized_headers.contains("accept-ranges: bytes"));
        assert!(normalized_headers.contains("cache-control: private, no-store"));
        assert_eq!(body, b"abcdefghij");

        let response = request(&url, "HEAD", None);
        let (headers, body) = response_parts(&response);
        assert!(headers.starts_with("HTTP/1.1 200 OK"));
        assert!(headers.to_ascii_lowercase().contains("content-length: 10"));
        assert!(body.is_empty());

        let response = request(&url, "HEAD", Some("bytes=2-5"));
        let (headers, body) = response_parts(&response);
        assert!(headers.starts_with("HTTP/1.1 200 OK"));
        assert!(!headers.to_ascii_lowercase().contains("content-range"));
        assert!(body.is_empty());

        for (range, expected_range, expected_body) in [
            ("bytes=0-9", "bytes 0-9/10", b"abcdefghij".as_slice()),
            ("bytes=2-5", "bytes 2-5/10", b"cdef".as_slice()),
            ("bytes=7-", "bytes 7-9/10", b"hij".as_slice()),
            ("bytes=-3", "bytes 7-9/10", b"hij".as_slice()),
        ] {
            let response = request(&url, "GET", Some(range));
            let (headers, body) = response_parts(&response);
            assert!(headers.starts_with("HTTP/1.1 206 Partial Content"));
            assert!(headers
                .to_ascii_lowercase()
                .contains(format!("content-range: {expected_range}").as_str()));
            assert_eq!(body, expected_body);
        }

        for range in ["bytes=20-", "bytes=8-2", "bytes=0-1,3-4"] {
            let response = request(&url, "GET", Some(range));
            let (headers, body) = response_parts(&response);
            assert!(headers.starts_with("HTTP/1.1 416 Range Not Satisfiable"));
            assert!(headers
                .to_ascii_lowercase()
                .contains("content-range: bytes */10"));
            assert!(body.is_empty());
        }

        for rejected_url in [
            url.replace("/video/1", "/video/2"),
            url.replace("/video/1", "/video/99"),
        ] {
            let response = request(&rejected_url, "GET", None);
            let (headers, body) = response_parts(&response);
            assert!(headers.starts_with("HTTP/1.1 404 Not Found"));
            assert!(body.is_empty());
        }
    }

    #[test]
    fn serves_new_ranges_while_long_responses_are_backpressured() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("media.db");
        let video_path = temp.path().join("long.mp4");
        let file = std::fs::File::create(&video_path).expect("video fixture");
        file.set_len(32 * 1024 * 1024)
            .expect("sparse video fixture");
        let conn = db::open_connection(&db_path).expect("database");
        db::init_schema(&conn).expect("schema");
        conn.execute(
            "INSERT INTO assets (id, path, file_name, kind, size_bytes, modified_at) VALUES (1, ?1, 'long.mp4', 'video', ?2, 1)",
            rusqlite::params![video_path.to_string_lossy().to_string(), 32_i64 * 1024 * 1024],
        )
        .expect("video row");
        drop(conn);
        let server = MediaServerState::start(db_path).expect("media server");
        let url = server.video_url(1).expect("video URL");
        let without_scheme = url.strip_prefix("http://").expect("http URL");
        let (authority, path) = without_scheme.split_once('/').expect("URL path");

        let mut slow_clients = Vec::new();
        for _ in 0..24 {
            let mut stream = TcpStream::connect(authority).expect("connect slow client");
            write!(
                stream,
                "GET /{path} HTTP/1.1\r\nHost: {authority}\r\nRange: bytes=0-33554431\r\n\r\n"
            )
            .expect("slow range request");
            slow_clients.push(stream);
        }
        thread::sleep(Duration::from_millis(200));

        let response = request(&url, "GET", Some("bytes=33554431-33554431"));
        let (headers, body) = response_parts(&response);
        assert!(headers.starts_with("HTTP/1.1 206 Partial Content"));
        assert_eq!(body, &[0]);
        drop(slow_clients);
    }
}
