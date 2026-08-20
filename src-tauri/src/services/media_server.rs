use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::Context;
use rand::{rngs::OsRng, RngCore};
use rusqlite::{Connection, OpenFlags};

use crate::db;

const MAX_REQUEST_LINE_BYTES: usize = 8 * 1024;
const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_HEADER_COUNT: usize = 100;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const REQUEST_WORKERS: usize = 4;
const REQUEST_QUEUE_CAPACITY: usize = 16;

pub struct MediaServerState {
    base_url: String,
    token: String,
    db_path: PathBuf,
    shutdown_tx: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl MediaServerState {
    pub fn start(db_path: PathBuf) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .context("cannot bind video media server")?;
        listener
            .set_nonblocking(true)
            .context("cannot configure video media server")?;
        let address = listener.local_addr()?;
        let token = process_token()?;
        let server_token = token.clone();
        let server_db_path = db_path.clone();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("video-media-server".to_string())
            .spawn(move || {
                run_server(
                    listener,
                    server_db_path,
                    server_token,
                    shutdown_rx,
                    ready_tx,
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
        })
    }

    pub fn video_url(&self, asset_id: i64) -> anyhow::Result<String> {
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
    listener: TcpListener,
    db_path: PathBuf,
    token: String,
    shutdown_rx: Receiver<()>,
    ready_tx: mpsc::SyncSender<std::result::Result<(), String>>,
) {
    let (request_tx, request_rx) = mpsc::sync_channel(REQUEST_QUEUE_CAPACITY);
    let request_rx = Arc::new(Mutex::new(request_rx));
    let mut workers = Vec::with_capacity(REQUEST_WORKERS);
    for index in 0..REQUEST_WORKERS {
        let request_rx = Arc::clone(&request_rx);
        let db_path = db_path.clone();
        let token = token.clone();
        match thread::Builder::new()
            .name(format!("video-media-request-{index}"))
            .spawn(move || loop {
                let stream = {
                    let receiver = request_rx.lock().unwrap_or_else(|error| error.into_inner());
                    receiver.recv()
                };
                let Ok(stream) = stream else {
                    break;
                };
                let _ = handle_connection(stream, &db_path, &token);
            }) {
            Ok(worker) => workers.push(worker),
            Err(error) => {
                let _ = ready_tx.send(Err(format!(
                    "cannot start video media request worker: {error}"
                )));
                drop(request_tx);
                for worker in workers {
                    let _ = worker.join();
                }
                return;
            }
        }
    }
    let _ = ready_tx.send(Ok(()));

    loop {
        match shutdown_rx.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        match listener.accept() {
            Ok((stream, _)) => match request_tx.try_send(stream) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => break,
            },
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }

    drop(request_tx);
    drop(workers);
}

fn handle_connection(mut stream: TcpStream, db_path: &Path, token: &str) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if read_limited_line(&mut reader, &mut request_line)? == 0
        || request_line.len() > MAX_REQUEST_LINE_BYTES
    {
        return write_empty_response(&mut stream, 400, "Bad Request", &[]);
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if parts.next().is_some() || !version.starts_with("HTTP/1.") {
        return write_empty_response(&mut stream, 400, "Bad Request", &[]);
    }

    if method != "GET" && method != "HEAD" {
        return write_empty_response(
            &mut stream,
            405,
            "Method Not Allowed",
            &[("Allow", "GET, HEAD")],
        );
    }

    let route_prefix = format!("/{token}/video/");
    let Some(asset_id) = target
        .strip_prefix(&route_prefix)
        .and_then(|value| value.strip_suffix(".mp4"))
        .and_then(|id| id.parse::<i64>().ok())
        .filter(|id| *id > 0)
    else {
        return write_empty_response(&mut stream, 404, "Not Found", &[]);
    };

    let mut range_header = None;
    let mut header_bytes = 0;
    for header_index in 0..=MAX_HEADER_COUNT {
        if header_index == MAX_HEADER_COUNT {
            return write_empty_response(&mut stream, 431, "Request Header Fields Too Large", &[]);
        }
        let mut line = String::new();
        if read_limited_line(&mut reader, &mut line)? == 0 || line.len() > MAX_REQUEST_LINE_BYTES {
            return write_empty_response(&mut stream, 400, "Bad Request", &[]);
        }
        header_bytes += line.len();
        if header_bytes > MAX_HEADER_BYTES {
            return write_empty_response(&mut stream, 431, "Request Header Fields Too Large", &[]);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("range") {
                range_header = Some(value.trim().to_string());
            }
        }
    }

    let path = match resolve_video_path(db_path, asset_id) {
        Ok(path) => path,
        Err(_) => return write_empty_response(&mut stream, 404, "Not Found", &[]),
    };
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(_) => return write_empty_response(&mut stream, 404, "Not Found", &[]),
    };
    let length = match file.metadata() {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        _ => return write_empty_response(&mut stream, 404, "Not Found", &[]),
    };
    let byte_range = match (method, range_header.as_deref()) {
        ("GET", Some(value)) => match parse_range(value, length) {
            Ok(range) => Some(range),
            Err(()) => {
                return write_empty_response(
                    &mut stream,
                    416,
                    "Range Not Satisfiable",
                    &[("Content-Range", &format!("bytes */{length}"))],
                )
            }
        },
        _ => None,
    };
    let (start, end, status, reason) = byte_range
        .map(|(start, end)| (start, end, 206, "Partial Content"))
        .unwrap_or((0, length.saturating_sub(1), 200, "OK"));
    let response_length = if length == 0 { 0 } else { end - start + 1 };
    let content_type = video_content_type(&path);

    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {response_length}\r\nAccept-Ranges: bytes\r\nCache-Control: private, no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n"
    )?;
    if byte_range.is_some() {
        write!(stream, "Content-Range: bytes {start}-{end}/{length}\r\n")?;
    }
    write!(stream, "\r\n")?;

    if method == "GET" && response_length > 0 {
        file.seek(SeekFrom::Start(start))?;
        copy_exact(&mut file, &mut stream, response_length)?;
    }
    Ok(())
}

fn read_limited_line(reader: &mut BufReader<TcpStream>, line: &mut String) -> io::Result<usize> {
    reader
        .take((MAX_REQUEST_LINE_BYTES + 1) as u64)
        .read_line(line)
}

fn resolve_video_path(db_path: &Path, asset_id: i64) -> anyhow::Result<PathBuf> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_secs(5))?;
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

fn copy_exact(file: &mut File, stream: &mut TcpStream, mut remaining: u64) -> io::Result<()> {
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    while remaining > 0 {
        let count = file.read(&mut buffer[..remaining.min(COPY_BUFFER_BYTES as u64) as usize])?;
        if count == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "video file changed while streaming"));
        }
        stream.write_all(&buffer[..count])?;
        remaining -= count as u64;
    }
    Ok(())
}

fn video_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("avi") => "video/x-msvideo",
        Some("mkv") => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

fn write_empty_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    headers: &[(&str, &str)],
) -> io::Result<()> {
    write!(stream, "HTTP/1.1 {status} {reason}\r\n")?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(
        stream,
        "Content-Length: 0\r\nCache-Control: private, no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
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
        for value in ["items=0-1", "bytes=", "bytes=5-2", "bytes=10-", "bytes=-0", "bytes=0-1,3-4"] {
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
        assert!(headers.starts_with("HTTP/1.1 200 OK"));
        assert!(headers.contains("Content-Type: video/mp4"));
        assert!(headers.contains("Content-Length: 10"));
        assert!(headers.contains("Accept-Ranges: bytes"));
        assert!(headers.contains("Cache-Control: private, no-store"));
        assert_eq!(body, b"abcdefghij");

        let response = request(&url, "HEAD", None);
        let (headers, body) = response_parts(&response);
        assert!(headers.starts_with("HTTP/1.1 200 OK"));
        assert!(headers.contains("Content-Length: 10"));
        assert!(body.is_empty());

        let response = request(&url, "HEAD", Some("bytes=2-5"));
        let (headers, body) = response_parts(&response);
        assert!(headers.starts_with("HTTP/1.1 200 OK"));
        assert!(!headers.contains("Content-Range"));
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
            assert!(headers.contains(format!("Content-Range: {expected_range}").as_str()));
            assert_eq!(body, expected_body);
        }

        for range in ["bytes=20-", "bytes=8-2", "bytes=0-1,3-4"] {
            let response = request(&url, "GET", Some(range));
            let (headers, body) = response_parts(&response);
            assert!(headers.starts_with("HTTP/1.1 416 Range Not Satisfiable"));
            assert!(headers.contains("Content-Range: bytes */10"));
            assert!(body.is_empty());
        }

        for rejected_url in [url.replace("/video/1", "/video/2"), url.replace("/video/1", "/video/99")] {
            let response = request(&rejected_url, "GET", None);
            let (headers, body) = response_parts(&response);
            assert!(headers.starts_with("HTTP/1.1 404 Not Found"));
            assert!(body.is_empty());
        }
    }
}
