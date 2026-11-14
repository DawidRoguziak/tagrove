use std::{
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use image::{imageops::FilterType, metadata::Orientation};
use sha2::{Digest, Sha256};

const IMAGE_THUMB_SIZE: u32 = 390;
const TALL_IMAGE_RATIO_THRESHOLD: f32 = 1.6;
const TALL_IMAGE_TOP_OFFSET_PX: u32 = 96;
const VIDEO_SEEK_DURATION_EPSILON: f64 = 0.001;
const VIDEO_THUMB_TIMEOUT: Duration = Duration::from_secs(45);
const VIDEO_PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const VIDEO_TOOL_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Bump when thumbnail identity inputs or rendering rules change. Version 2
/// hashes path, size bytes, and nanosecond mtime instead of seconds-resolution
/// `modified_at`. Version 3 applies the EXIF orientation of images (ADR 0020);
/// a thumbnail recorded under an older version is reused only when the render
/// worker finds the current rules draw the same picture.
pub const THUMB_CACHE_VERSION: u32 = 3;

/// Upper bound on source pixels accepted before a full decode. The decoded
/// RGB8 buffer alone would be three times this many bytes; Lanczos resizing
/// needs additional intermediates, so exceeding images are rejected outright.
pub const MAX_DECODE_PIXELS: u64 = 50_000_000;

/// Identity of the exact source file version a thumbnail belongs to.
///
/// Every stage of the pipeline (scheduler task, result, SQL compare-and-set)
/// carries this value so a render finished for an older file version can never
/// be published onto a record that was re-indexed in the meantime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceVersion {
    pub path: String,
    pub size_bytes: i64,
    pub mtime_ns: i64,
}

impl SourceVersion {
    pub fn new(path: impl Into<String>, size_bytes: i64, mtime_ns: i64) -> Self {
        Self {
            path: path.into(),
            size_bytes,
            mtime_ns,
        }
    }
}

/// Hex characters of the cache key exposed to the frontend as `thumb_version`.
pub const THUMB_VERSION_LEN: usize = 16;

/// Full hex cache key of a source version; also the thumbnail file stem.
pub fn thumb_cache_key(path: &str, size_bytes: i64, mtime_ns: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(THUMB_CACHE_VERSION.to_le_bytes());
    hasher.update([0u8]);
    hasher.update(path.as_bytes());
    hasher.update([0u8]);
    hasher.update(size_bytes.to_le_bytes());
    hasher.update(mtime_ns.to_le_bytes());
    format!("{:x}", hasher.finalize())
}

/// Short public token of a source version, used as the `v` query parameter of
/// `thumb://` URLs. It is a prefix of the file stem, so a URL changes exactly
/// when the thumbnail file would change.
pub fn thumb_version_token(path: &str, size_bytes: i64, mtime_ns: i64) -> String {
    let mut key = thumb_cache_key(path, size_bytes, mtime_ns);
    key.truncate(THUMB_VERSION_LEN);
    key
}

pub fn thumb_target(thumbs_dir: &Path, version: &SourceVersion) -> PathBuf {
    let key = thumb_cache_key(&version.path, version.size_bytes, version.mtime_ns);
    thumbs_dir.join(format!("{}.jpg", key))
}

/// Pure budget rule: whether a source image must be rejected before decode.
pub fn exceeds_decode_budget(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) > MAX_DECODE_PIXELS
}

fn budget_error(width: u32, height: u32) -> anyhow::Error {
    anyhow::anyhow!("image {width}x{height} exceeds decode budget of {MAX_DECODE_PIXELS} pixels")
}

/// Smallest decoded size that still covers the thumbnail without upscaling:
/// the crop width for tall images, the longer side otherwise.
fn scaled_decode_request(width: u32, height: u32) -> (u16, u16) {
    let size = IMAGE_THUMB_SIZE;
    if top_biased_square_crop_rect(width, height).is_some() {
        // Both axes requested, so the decoded width (the crop) is at least `size`.
        let tall = (u64::from(size) * u64::from(height)).div_ceil(u64::from(width.max(1)));
        (size as u16, tall.min(u64::from(u16::MAX)) as u16)
    } else {
        (size as u16, size as u16)
    }
}

/// Whether applying `orientation` exchanges width and height.
fn swaps_axes(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

/// `scaled_decode_request` for a stored `width` x `height` that is displayed
/// after `orientation`: decided on the displayed size, returned in stored axes.
fn oriented_decode_request(width: u32, height: u32, orientation: Orientation) -> (u16, u16) {
    if swaps_axes(orientation) {
        let (request_height, request_width) = scaled_decode_request(height, width);
        (request_width, request_height)
    } else {
        scaled_decode_request(width, height)
    }
}

/// EXIF orientation of a JPEG after `read_info`, from the APP1 segment before
/// the frame header, where the Exif standard places it.
fn jpeg_orientation<R: Read>(decoder: &jpeg_decoder::Decoder<R>) -> Orientation {
    decoder
        .exif_data()
        .and_then(Orientation::from_exif_chunk)
        .unwrap_or(Orientation::NoTransforms)
}

/// Orientation that rendering applies to an image source, read from its
/// header without decoding pixels. The render worker uses it to decide whether
/// a thumbnail of an earlier cache version still shows the right picture.
pub fn image_orientation(source_path: &Path) -> anyhow::Result<Orientation> {
    if is_jpeg(source_path)? {
        let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(fs::File::open(source_path)?));
        if decoder.read_info().is_ok() {
            return Ok(jpeg_orientation(&decoder));
        }
    }
    let mut decoder = image::ImageReader::new(BufReader::new(fs::File::open(source_path)?))
        .with_guessed_format()?
        .into_decoder()?;
    Ok(image::ImageDecoder::orientation(&mut decoder)?)
}

/// Decodes a baseline or progressive JPEG directly at the smallest IDCT scale
/// (1/8, 1/4, 1/2 or 1) that still covers the thumbnail, then applies its EXIF
/// orientation. `None` means the caller must use the general decoder (CMYK,
/// 16-bit, or a decoder error).
fn decode_scaled_jpeg(source_path: &Path) -> anyhow::Result<Option<image::DynamicImage>> {
    let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(fs::File::open(source_path)?));
    if decoder.read_info().is_err() {
        return Ok(None);
    }
    let Some(info) = decoder.info() else {
        return Ok(None);
    };
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if exceeds_decode_budget(width, height) {
        return Err(budget_error(width, height));
    }
    let orientation = jpeg_orientation(&decoder);
    let (request_width, request_height) = oriented_decode_request(width, height, orientation);
    if decoder.scale(request_width, request_height).is_err() {
        return Ok(None);
    }
    let Ok(pixels) = decoder.decode() else {
        return Ok(None);
    };
    let Some(info) = decoder.info() else {
        return Ok(None);
    };
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let image = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            image::RgbImage::from_raw(width, height, pixels).map(image::DynamicImage::ImageRgb8)
        }
        jpeg_decoder::PixelFormat::L8 => {
            image::GrayImage::from_raw(width, height, pixels).map(image::DynamicImage::ImageLuma8)
        }
        jpeg_decoder::PixelFormat::L16 | jpeg_decoder::PixelFormat::CMYK32 => None,
    };
    // The scaled buffer is near thumbnail size, so rotating it is cheap.
    Ok(image.map(|mut image| {
        image.apply_orientation(orientation);
        image
    }))
}

fn is_jpeg(source_path: &Path) -> std::io::Result<bool> {
    let mut magic = [0u8; 3];
    let read = fs::File::open(source_path)?.read(&mut magic)?;
    Ok(read == 3 && magic == [0xFF, 0xD8, 0xFF])
}

/// Full decode through `image`, checking the budget from the header of the
/// same open file instead of probing it separately, then applying the EXIF
/// orientation the decoder reports.
fn decode_full(source_path: &Path) -> anyhow::Result<image::DynamicImage> {
    let mut decoder = image::ImageReader::new(BufReader::new(fs::File::open(source_path)?))
        .with_guessed_format()?
        .into_decoder()?;
    let (width, height) = image::ImageDecoder::dimensions(&decoder);
    if exceeds_decode_budget(width, height) {
        return Err(budget_error(width, height));
    }
    // Unreadable orientation metadata leaves the stored orientation.
    let orientation =
        image::ImageDecoder::orientation(&mut decoder).unwrap_or(Orientation::NoTransforms);
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    if orientation != Orientation::NoTransforms {
        // A rotation copies the buffer, so shrink to the decode request first.
        let (request_width, request_height) = oriented_decode_request(width, height, orientation);
        let (request_width, request_height) = (u32::from(request_width), u32::from(request_height));
        if image.width() > request_width || image.height() > request_height {
            image = image.thumbnail(request_width, request_height);
        }
        image.apply_orientation(orientation);
    }
    Ok(image)
}

pub fn create_image_thumb(source_path: &Path, target_path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let scaled = if is_jpeg(source_path)? {
        decode_scaled_jpeg(source_path)?
    } else {
        None
    };
    // Both decoders return the image in its displayed orientation, so the
    // tall-image crop below is decided on the displayed size.
    let image = match scaled {
        Some(image) => image,
        None => decode_full(source_path)?,
    };
    // The source is already near the target size after a scaled decode, so a
    // cheap filter is enough; `thumbnail` uses its own fast area sampling.
    let thumb =
        if let Some((x, y, size)) = top_biased_square_crop_rect(image.width(), image.height()) {
            image
                .crop_imm(x, y, size, size)
                .resize_exact(IMAGE_THUMB_SIZE, IMAGE_THUMB_SIZE, FilterType::Triangle)
                .to_rgb8()
        } else {
            image
                .thumbnail(IMAGE_THUMB_SIZE, IMAGE_THUMB_SIZE)
                .to_rgb8()
        };
    let temporary = temporary_thumb_path(target_path);
    let _ = fs::remove_file(&temporary);
    thumb.save_with_format(&temporary, image::ImageFormat::Jpeg)?;
    publish_thumbnail(&temporary, target_path)?;
    Ok(())
}

fn top_biased_square_crop_rect(width: u32, height: u32) -> Option<(u32, u32, u32)> {
    if width == 0 || height == 0 || height <= width {
        return None;
    }

    let ratio = height as f32 / width as f32;
    if ratio < TALL_IMAGE_RATIO_THRESHOLD {
        return None;
    }

    let square_size = width;
    let max_top_offset = height.saturating_sub(square_size);
    let top_offset = TALL_IMAGE_TOP_OFFSET_PX.min(max_top_offset);
    Some((0, top_offset, square_size))
}

/// Input options for every ffmpeg and ffprobe run on a library video (ADR 0028).
///
/// The media kind comes from the file extension, so the content can be a
/// manifest or a script. `-format_whitelist` allows only demuxers that read
/// the one file they are given: without it a DASH manifest or a `concat`
/// script named `*.mp4` makes the tool read the files it names, and the
/// thumbnail and duration then come from those files. `-protocol_whitelist`
/// keeps the input on the `file` protocol.
const LOCAL_VIDEO_INPUT_ARGS: [&str; 4] = [
    "-protocol_whitelist",
    "file",
    "-format_whitelist",
    "mov,matroska,avi,mpegts,mpeg,flv,asf,ogg",
];

/// How ffmpeg picks the thumbnail frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFrameSeek {
    /// Decode keyframes only and take the keyframe the input seek lands on.
    /// About 4x faster, but some files yield no frame; callers then retry
    /// with `Exact`.
    Keyframe,
    /// Decode up to the exact seek time.
    Exact,
}

pub fn create_video_thumb(
    ffmpeg_path: &Path,
    source_path: &Path,
    target_path: &Path,
    seek_seconds: f64,
    frame_seek: VideoFrameSeek,
) -> anyhow::Result<()> {
    if !is_launchable_tool_path(ffmpeg_path) {
        return Ok(());
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = temporary_thumb_path(target_path);
    let _ = fs::remove_file(&temporary);
    let mut command = ffmpeg_command(ffmpeg_path);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostats")
        .arg("-nostdin")
        .arg("-y")
        .arg("-threads")
        .arg("1");
    if frame_seek == VideoFrameSeek::Keyframe {
        command.args(["-skip_frame", "nokey", "-noaccurate_seek"]);
    }
    command
        .arg("-ss")
        .arg(format!("{seek_seconds:.3}"))
        .args(LOCAL_VIDEO_INPUT_ARGS)
        .arg("-i")
        .arg(source_path)
        .arg("-an")
        .arg("-sn")
        .arg("-dn")
        .arg("-frames:v")
        .arg("1")
        .arg("-update")
        .arg("1")
        .arg("-vf")
        .arg(format!("scale={}:-1", IMAGE_THUMB_SIZE))
        .arg("-q:v")
        .arg("7")
        .arg("-f")
        .arg("image2")
        .arg(&temporary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let status = run_command_status_with_timeout(&mut command, VIDEO_THUMB_TIMEOUT)?;

    if !status.success() {
        let _ = fs::remove_file(&temporary);
        return Err(anyhow::anyhow!("ffmpeg returned non-zero status"));
    }

    publish_thumbnail(&temporary, target_path)?;

    Ok(())
}

fn temporary_thumb_path(target_path: &Path) -> PathBuf {
    let file_name = target_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("thumbnail");
    target_path.with_file_name(format!("{file_name}.tmp.jpg"))
}

fn publish_thumbnail(temporary: &Path, target: &Path) -> anyhow::Result<()> {
    if target.exists() {
        let _ = fs::remove_file(temporary);
        return Ok(());
    }
    fs::rename(temporary, target)?;
    Ok(())
}

pub fn resolve_video_seek_seconds(duration_ms: Option<i64>) -> f64 {
    match duration_ms {
        Some(ms) => {
            let duration_s = (ms as f64 / 1000.0).max(0.0);
            if duration_s <= 0.0 {
                return 0.0;
            }

            let desired_seek = if duration_s < 1.0 {
                duration_s * 0.8
            } else if duration_s < 5.0 {
                1.0
            } else if duration_s >= 10.0 {
                10.0
            } else {
                duration_s - 1.0
            };

            let max_seek = (duration_s - VIDEO_SEEK_DURATION_EPSILON).max(0.0);
            desired_seek.clamp(0.0, max_seek)
        }
        None => 1.0,
    }
}

pub fn probe_video_duration_ms(ffmpeg_path: &Path, source_path: &Path) -> Option<i64> {
    if !source_path.exists() {
        return None;
    }

    for ffprobe_path in ffprobe_candidates(ffmpeg_path) {
        if !is_launchable_tool_path(&ffprobe_path) {
            continue;
        }

        let mut command = video_tool_command(&ffprobe_path);
        command
            .arg("-v")
            .arg("error")
            .arg("-show_entries")
            .arg("format=duration")
            .arg("-of")
            .arg("default=noprint_wrappers=1:nokey=1")
            .args(LOCAL_VIDEO_INPUT_ARGS)
            .arg(source_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let output = run_command_output_with_timeout(&mut command, VIDEO_PROBE_TIMEOUT);

        let output = match output {
            Ok(output) => output,
            Err(_) => continue,
        };

        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(duration_ms) = parse_duration_ms_from_ffprobe_stdout(&stdout) {
            return Some(duration_ms);
        }
    }

    if !is_launchable_tool_path(ffmpeg_path) {
        return None;
    }

    let mut command = ffmpeg_command(ffmpeg_path);
    command
        .arg("-hide_banner")
        .arg("-nostdin")
        .args(LOCAL_VIDEO_INPUT_ARGS)
        .arg("-i")
        .arg(source_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = run_command_output_with_timeout(&mut command, VIDEO_PROBE_TIMEOUT).ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_duration_ms_from_ffmpeg_stderr(&stderr)
}

fn parse_duration_ms_from_ffprobe_stdout(text: &str) -> Option<i64> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| line.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| (seconds * 1000.0).round() as i64)
}

fn parse_duration_ms_from_ffmpeg_stderr(text: &str) -> Option<i64> {
    let marker = "Duration: ";
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find(',')?;
    let value = rest[..end].trim();

    let mut parts = value.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }

    let total_seconds = hours * 3600.0 + minutes * 60.0 + seconds;
    Some((total_seconds * 1000.0).round() as i64)
}

fn ffprobe_candidates(ffmpeg_path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(file_name) = ffmpeg_path.file_name().and_then(|name| name.to_str()) {
        if let Some(suffix) = file_name.strip_prefix("ffmpeg") {
            candidates.push(ffmpeg_path.with_file_name(format!("ffprobe{suffix}")));
        }
    }

    if let Some(parent) = ffmpeg_path.parent() {
        candidates.push(parent.join("ffprobe"));
    }

    candidates.push(PathBuf::from("ffprobe"));
    candidates.push(PathBuf::from("/usr/bin/ffprobe"));

    let mut deduped = Vec::new();
    for candidate in candidates {
        if !deduped.contains(&candidate) {
            deduped.push(candidate);
        }
    }
    deduped
}

fn is_launchable_tool_path(path: &Path) -> bool {
    if path.components().count() == 1 {
        return true;
    }

    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

pub fn video_tool_status(ffmpeg_path: &Path) -> (bool, bool) {
    let available = |path: &Path| {
        let mut command = video_tool_command(path);
        command
            .arg("-version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        run_command_status_with_timeout(&mut command, Duration::from_secs(3))
            .map(|status| status.success())
            .unwrap_or(false)
    };
    let ffmpeg_available = available(ffmpeg_path);
    let ffprobe_available = ffprobe_candidates(ffmpeg_path)
        .iter()
        .any(|candidate| available(candidate));
    (ffmpeg_available, ffprobe_available)
}

fn ffmpeg_command(ffmpeg_path: &Path) -> Command {
    video_tool_command(ffmpeg_path)
}

fn video_tool_command(tool_path: &Path) -> Command {
    Command::new(tool_path)
}

fn run_command_status_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> anyhow::Result<ExitStatus> {
    let mut child = command.spawn()?;
    match wait_for_exit_with_timeout(&mut child, timeout)
        .inspect_err(|_| terminate_child(&mut child))?
    {
        Some(status) => Ok(status),
        None => {
            terminate_child(&mut child);
            Err(anyhow::anyhow!(
                "video tool execution timed out after {}ms",
                timeout.as_millis()
            ))
        }
    }
}

fn run_command_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> anyhow::Result<Output> {
    let mut child = command.spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    thread::scope(|scope| {
        let stdout_reader = scope.spawn(move || drain_diagnostics(stdout));
        let stderr_reader = scope.spawn(move || drain_diagnostics(stderr));
        let status = wait_for_exit_with_timeout(&mut child, timeout);
        if !matches!(status, Ok(Some(_))) {
            terminate_child(&mut child);
        }
        let stdout = stdout_reader
            .join()
            .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
        let stderr = stderr_reader
            .join()
            .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;
        let status = status?.ok_or_else(|| {
            anyhow::anyhow!(
                "video tool execution timed out after {}ms",
                timeout.as_millis()
            )
        })?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    })
}

fn drain_diagnostics(reader: Option<impl std::io::Read>) -> std::io::Result<Vec<u8>> {
    const MAX_DIAGNOSTICS: usize = 64 * 1024;
    let mut retained = Vec::new();
    if let Some(mut reader) = reader {
        let mut buffer = [0; 8192];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            let keep = count.min(MAX_DIAGNOSTICS - retained.len());
            retained.extend_from_slice(&buffer[..keep]);
        }
    }
    Ok(retained)
}

fn wait_for_exit_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> std::io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }

        if Instant::now() >= deadline {
            return Ok(None);
        }

        thread::sleep(VIDEO_TOOL_POLL_INTERVAL);
    }
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
pub(crate) mod test_support {
    /// A minimal big-endian Exif (TIFF) chunk with one Orientation entry.
    pub fn exif_orientation_chunk(orientation: u16) -> Vec<u8> {
        let mut tiff = b"MM\0\x2a".to_vec();
        tiff.extend_from_slice(&8u32.to_be_bytes());
        tiff.extend_from_slice(&1u16.to_be_bytes());
        tiff.extend_from_slice(&0x0112u16.to_be_bytes());
        tiff.extend_from_slice(&3u16.to_be_bytes());
        tiff.extend_from_slice(&1u32.to_be_bytes());
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&0u32.to_be_bytes());
        tiff
    }

    /// Writes `image` as a JPEG or PNG with an optional Exif Orientation.
    pub fn write_oriented(
        path: &std::path::Path,
        image: &image::DynamicImage,
        format: image::ImageFormat,
        orientation: Option<u16>,
    ) {
        use image::ImageEncoder;
        let mut bytes = Vec::new();
        let rgb = image.to_rgb8();
        let exif = orientation.map(exif_orientation_chunk).unwrap_or_default();
        match format {
            image::ImageFormat::Jpeg => {
                let mut encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95);
                encoder.set_exif_metadata(exif).unwrap();
                encoder
                    .write_image(
                        rgb.as_raw(),
                        rgb.width(),
                        rgb.height(),
                        image::ExtendedColorType::Rgb8,
                    )
                    .unwrap();
            }
            image::ImageFormat::Png => {
                let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
                encoder.set_exif_metadata(exif).unwrap();
                encoder
                    .write_image(
                        rgb.as_raw(),
                        rgb.width(),
                        rgb.height(),
                        image::ExtendedColorType::Rgb8,
                    )
                    .unwrap();
            }
            other => panic!("unsupported fixture format {other:?}"),
        }
        std::fs::write(path, bytes).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::write_oriented;
    use super::{
        create_video_thumb, exceeds_decode_budget, parse_duration_ms_from_ffmpeg_stderr,
        parse_duration_ms_from_ffprobe_stdout, probe_video_duration_ms, resolve_video_seek_seconds,
        thumb_target, top_biased_square_crop_rect, Orientation, SourceVersion, MAX_DECODE_PIXELS,
    };

    #[test]
    fn thumb_target_is_stable_for_identical_source_versions() {
        let dir = std::path::Path::new("/tmp");
        let version = SourceVersion::new("/media/a.png", 123, 456);
        assert_eq!(thumb_target(dir, &version), thumb_target(dir, &version));
    }

    #[test]
    fn thumb_target_changes_when_any_version_component_changes() {
        let dir = std::path::Path::new("/tmp");
        let base = SourceVersion::new("/media/a.png", 123, 456);
        let by_size = SourceVersion::new("/media/a.png", 124, 456);
        let by_mtime = SourceVersion::new("/media/a.png", 123, 457);
        let by_path = SourceVersion::new("/media/b.png", 123, 456);

        for changed in [by_size, by_mtime, by_path] {
            assert_ne!(
                thumb_target(dir, &base),
                thumb_target(dir, &changed),
                "target must differ after source change"
            );
        }
    }

    #[test]
    fn decode_budget_rejects_extreme_dimensions_only() {
        assert!(!exceeds_decode_budget(0, 0));
        assert!(!exceeds_decode_budget(390, 390));
        assert!(!exceeds_decode_budget(10_000, 5_000));
        // Exactly at the budget is still allowed.
        let side = (MAX_DECODE_PIXELS as f64).sqrt() as u32;
        assert!(!exceeds_decode_budget(side, side));
        assert!(exceeds_decode_budget(side, side + 1));
        assert!(exceeds_decode_budget(u32::MAX, u32::MAX));
    }

    #[test]
    fn top_biased_crop_rect_is_used_for_tall_images() {
        let crop = top_biased_square_crop_rect(1080, 1920);
        assert_eq!(crop, Some((0, 96, 1080)));
    }

    #[test]
    fn top_biased_crop_rect_clamps_offset_to_available_range() {
        let crop_default_offset = top_biased_square_crop_rect(500, 830);
        assert_eq!(crop_default_offset, Some((0, 96, 500)));

        let crop_small_range = top_biased_square_crop_rect(100, 160);
        assert_eq!(crop_small_range, Some((0, 60, 100)));
    }

    #[test]
    fn top_biased_crop_rect_skips_non_tall_images() {
        assert_eq!(top_biased_square_crop_rect(1200, 1200), None);
        assert_eq!(top_biased_square_crop_rect(1600, 900), None);
        assert_eq!(top_biased_square_crop_rect(1000, 1400), None);
    }

    #[test]
    fn parse_duration_ms_from_ffmpeg_output() {
        let stderr = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'clip.mp4':\n  Duration: 00:01:02.50, start: 0.000000, bitrate: 512 kb/s";
        let duration = parse_duration_ms_from_ffmpeg_stderr(stderr);
        assert_eq!(duration, Some(62_500));
    }

    #[test]
    fn parse_duration_returns_none_when_marker_is_missing() {
        let stderr = "ffmpeg version ...\nNo duration available";
        assert_eq!(parse_duration_ms_from_ffmpeg_stderr(stderr), None);
    }

    #[test]
    fn parse_duration_ms_from_ffprobe_output() {
        assert_eq!(
            parse_duration_ms_from_ffprobe_stdout("62.500000\n"),
            Some(62_500)
        );
        assert_eq!(parse_duration_ms_from_ffprobe_stdout("\n"), None);
    }

    #[test]
    fn resolve_video_seek_seconds_uses_expected_boundaries() {
        assert_eq!(resolve_video_seek_seconds(Some(4_000)), 1.0);
        assert_eq!(resolve_video_seek_seconds(Some(8_000)), 7.0);
        assert!((resolve_video_seek_seconds(Some(10_000)) - 9.999).abs() < 1e-9);
        assert_eq!(resolve_video_seek_seconds(Some(15_000)), 10.0);
    }

    #[test]
    fn resolve_video_seek_seconds_handles_subsecond_videos() {
        assert!((resolve_video_seek_seconds(Some(120)) - 0.096).abs() < 1e-9);
        assert!((resolve_video_seek_seconds(Some(400)) - 0.320).abs() < 1e-9);
        assert!((resolve_video_seek_seconds(Some(900)) - 0.720).abs() < 1e-9);
        assert!((resolve_video_seek_seconds(Some(1_000)) - 0.999).abs() < 1e-9);
    }

    #[test]
    fn resolve_video_seek_seconds_clamps_non_positive_values() {
        assert_eq!(resolve_video_seek_seconds(Some(0)), 0.0);
        assert_eq!(resolve_video_seek_seconds(Some(-500)), 0.0);
    }

    #[test]
    fn resolve_video_seek_seconds_defaults_without_duration() {
        assert_eq!(resolve_video_seek_seconds(None), 1.0);
    }

    #[test]
    #[cfg(unix)]
    fn probes_and_renders_thumbnail_with_system_video_tools() {
        let ffmpeg = std::path::Path::new("/usr/bin/ffmpeg");
        if !ffmpeg.is_file() {
            return;
        }
        let tmp = tempfile::tempdir().expect("tempdir");
        let video = tmp.path().join("sample.mp4");
        let thumb = tmp.path().join("sample.jpg");
        let status = std::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:d=0.5",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&video)
            .status()
            .expect("generate video fixture");
        assert!(status.success());
        assert!(probe_video_duration_ms(ffmpeg, &video).is_some());
        create_video_thumb(ffmpeg, &video, &thumb, 0.1, super::VideoFrameSeek::Exact)
            .expect("render video thumbnail");
        assert!(thumb.is_file());
    }
    #[cfg(unix)]
    #[test]
    fn drains_both_pipes_beyond_capacity_and_caps_retained_output() {
        let mut command = std::process::Command::new("sh");
        command
            .args([
                "-c",
                "head -c 262144 /dev/zero; head -c 262144 /dev/zero >&2",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output =
            super::run_command_output_with_timeout(&mut command, std::time::Duration::from_secs(3))
                .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 65536);
        assert_eq!(output.stderr.len(), 65536);
        let mut command = std::process::Command::new("sh");
        command
            .args(["-c", "while :; do :; done"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let started = std::time::Instant::now();
        assert!(super::run_command_output_with_timeout(
            &mut command,
            std::time::Duration::from_millis(30)
        )
        .is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn version_token_is_a_prefix_of_the_thumbnail_file_stem() {
        let version = super::SourceVersion::new("/library/a.jpg", 1234, 5_000_000_001);
        let target = super::thumb_target(std::path::Path::new("/thumbs"), &version);
        let stem = target.file_stem().unwrap().to_string_lossy().to_string();
        let token = super::thumb_version_token("/library/a.jpg", 1234, 5_000_000_001);
        assert_eq!(token.len(), super::THUMB_VERSION_LEN);
        assert!(stem.starts_with(&token));
        assert_ne!(
            token,
            super::thumb_version_token("/library/a.jpg", 1234, 5_000_000_002),
            "a new mtime changes the URL"
        );
    }
    fn thumb_dimensions(source: image::DynamicImage, format: image::ImageFormat) -> (u32, u32) {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("source.img");
        source.save_with_format(&source_path, format).unwrap();
        let target = dir.path().join("thumbs/out.jpg");
        super::create_image_thumb(&source_path, &target).unwrap();
        image::image_dimensions(&target).unwrap()
    }

    #[test]
    fn scaled_jpeg_decode_keeps_thumbnail_geometry() {
        let wide =
            image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(2_000, 1_000, |x, y| {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
            }));
        assert_eq!(thumb_dimensions(wide, image::ImageFormat::Jpeg), (390, 195));
        // Tall images keep the top-biased square crop at full thumbnail size.
        let tall = image::DynamicImage::ImageRgb8(image::RgbImage::new(800, 2_000));
        assert_eq!(thumb_dimensions(tall, image::ImageFormat::Jpeg), (390, 390));
        let gray = image::DynamicImage::ImageLuma8(image::GrayImage::new(1_600, 1_200));
        assert_eq!(thumb_dimensions(gray, image::ImageFormat::Jpeg), (390, 293));
        // Non-JPEG sources take the general decoder.
        let png = image::DynamicImage::ImageRgb8(image::RgbImage::new(1_200, 600));
        assert_eq!(thumb_dimensions(png, image::ImageFormat::Png), (390, 195));
    }

    #[test]
    fn scaled_decode_request_covers_the_crop_width_of_tall_images() {
        assert_eq!(super::scaled_decode_request(6_000, 4_000), (390, 390));
        assert_eq!(super::scaled_decode_request(1_000, 3_000), (390, 1_170));
        assert_eq!(super::scaled_decode_request(1, 65_535), (390, u16::MAX));
    }

    #[test]
    fn broken_jpeg_reports_an_error_instead_of_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("broken.jpg");
        std::fs::write(&source, [0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J']).unwrap();
        assert!(super::create_image_thumb(&source, &dir.path().join("out.jpg")).is_err());
    }
    const RED: [u8; 3] = [230, 20, 20];
    const GREEN: [u8; 3] = [20, 230, 20];
    const BLUE: [u8; 3] = [20, 20, 230];
    const WHITE: [u8; 3] = [240, 240, 240];

    /// Stored pixels: red top-left, green top-right, blue bottom-left, white
    /// bottom-right, so every orientation gives a distinct corner order.
    fn quadrants(width: u32, height: u32) -> image::DynamicImage {
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb(match (x < width / 2, y < height / 2) {
                (true, true) => RED,
                (false, true) => GREEN,
                (true, false) => BLUE,
                (false, false) => WHITE,
            })
        }))
    }

    /// Colors at the centers of the four quadrants: TL, TR, BL, BR.
    fn corner_colors(image: &image::DynamicImage) -> [[u8; 3]; 4] {
        let rgb = image.to_rgb8();
        let (width, height) = rgb.dimensions();
        [(1, 1), (3, 1), (1, 3), (3, 3)]
            .map(|(x, y)| rgb.get_pixel(width * x / 4, height * y / 4).0)
    }

    fn assert_corners(actual: [[u8; 3]; 4], expected: [[u8; 3]; 4], context: &str) {
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(a, e)| a.abs_diff(e) <= 48),
                "{context}: corners {actual:?} do not match {expected:?}"
            );
        }
    }

    /// Exif orientation, displayed (landscape?) and expected corner colors.
    const ORIENTATION_CASES: [(Option<u16>, bool, [[u8; 3]; 4]); 7] = [
        (None, true, [RED, GREEN, BLUE, WHITE]),
        (Some(1), true, [RED, GREEN, BLUE, WHITE]),
        // Mirrored horizontally.
        (Some(2), true, [GREEN, RED, WHITE, BLUE]),
        (Some(3), true, [WHITE, BLUE, GREEN, RED]),
        // Transposed (rotated 90 degrees clockwise and mirrored).
        (Some(5), false, [RED, BLUE, GREEN, WHITE]),
        // Rotated 90 degrees clockwise, the usual phone portrait.
        (Some(6), false, [BLUE, RED, WHITE, GREEN]),
        // Rotated 90 degrees counter-clockwise.
        (Some(8), false, [GREEN, WHITE, RED, BLUE]),
    ];

    #[test]
    fn exif_orientation_is_applied_on_the_scaled_jpeg_path() {
        let dir = tempfile::tempdir().unwrap();
        for (orientation, landscape, corners) in ORIENTATION_CASES {
            let source = dir.path().join("source.jpg");
            write_oriented(
                &source,
                &quadrants(1_200, 900),
                image::ImageFormat::Jpeg,
                orientation,
            );
            let context = format!("jpeg orientation {orientation:?}");
            let expected = orientation
                .and_then(|value| Orientation::from_exif(value as u8))
                .unwrap_or(Orientation::NoTransforms);
            assert_eq!(
                super::image_orientation(&source).unwrap(),
                expected,
                "{context}"
            );

            let decoded = super::decode_scaled_jpeg(&source)
                .unwrap()
                .expect("an RGB JPEG takes the scaled path");
            assert_eq!(decoded.width() > decoded.height(), landscape, "{context}");
            assert_corners(corner_colors(&decoded), corners, &context);

            let target = dir.path().join(format!("thumbs/jpeg-{orientation:?}.jpg"));
            super::create_image_thumb(&source, &target).unwrap();
            let thumb = image::open(&target).unwrap();
            let size = if landscape { (390, 293) } else { (293, 390) };
            assert_eq!((thumb.width(), thumb.height()), size, "{context}");
            assert_corners(corner_colors(&thumb), corners, &context);
        }
    }

    #[test]
    fn exif_orientation_is_applied_on_the_general_decoder_path() {
        let dir = tempfile::tempdir().unwrap();
        for (orientation, landscape, corners) in ORIENTATION_CASES {
            let context = format!("orientation {orientation:?}");
            // A JPEG through `image` itself, as CMYK and 16-bit JPEGs go.
            let jpeg = dir.path().join("source.jpg");
            write_oriented(
                &jpeg,
                &quadrants(1_200, 900),
                image::ImageFormat::Jpeg,
                orientation,
            );
            let decoded = super::decode_full(&jpeg).unwrap();
            assert_eq!(decoded.width() > decoded.height(), landscape, "{context}");
            assert_corners(corner_colors(&decoded), corners, &context);

            let png = dir.path().join("source.png");
            write_oriented(
                &png,
                &quadrants(1_200, 900),
                image::ImageFormat::Png,
                orientation,
            );
            let target = dir.path().join(format!("thumbs/png-{orientation:?}.jpg"));
            super::create_image_thumb(&png, &target).unwrap();
            let thumb = image::open(&target).unwrap();
            let size = if landscape { (390, 293) } else { (293, 390) };
            assert_eq!((thumb.width(), thumb.height()), size, "png {context}");
            assert_corners(corner_colors(&thumb), corners, &format!("png {context}"));
        }
    }

    #[test]
    fn tall_image_crop_is_decided_on_the_displayed_size() {
        let dir = tempfile::tempdir().unwrap();
        // Stored landscape 2:1. Rotated by 90 degrees it is a 1:2 portrait,
        // which gets the top-biased square crop.
        for (format, name) in [
            (image::ImageFormat::Jpeg, "wide.jpg"),
            (image::ImageFormat::Png, "wide.png"),
        ] {
            let source = dir.path().join(name);
            for (orientation, size) in [
                (Some(1), (390, 195)),
                (Some(3), (390, 195)),
                (Some(6), (390, 390)),
                (Some(8), (390, 390)),
            ] {
                write_oriented(&source, &quadrants(1_600, 800), format, orientation);
                let target = dir
                    .path()
                    .join(format!("thumbs/{name}-{orientation:?}.jpg"));
                super::create_image_thumb(&source, &target).unwrap();
                assert_eq!(
                    image::image_dimensions(&target).unwrap(),
                    size,
                    "{name} orientation {orientation:?}"
                );
            }
        }
        // The crop keeps the displayed top: for orientation 6 that is the
        // stored left edge (red over blue becomes blue left of red).
        let source = dir.path().join("portrait.jpg");
        write_oriented(
            &source,
            &quadrants(1_600, 800),
            image::ImageFormat::Jpeg,
            Some(6),
        );
        let target = dir.path().join("thumbs/portrait.jpg");
        super::create_image_thumb(&source, &target).unwrap();
        let thumb = image::open(&target).unwrap();
        let [top_left, top_right, ..] = corner_colors(&thumb);
        assert_corners(
            [top_left, top_right, BLUE, RED],
            [BLUE, RED, BLUE, RED],
            "cropped portrait",
        );
    }

    #[test]
    fn oriented_decode_request_swaps_axes_for_quarter_turns() {
        // Stored 3000x1000 displayed as a 1000x3000 portrait: the crop width
        // is the stored height.
        assert_eq!(
            super::oriented_decode_request(3_000, 1_000, Orientation::Rotate90),
            (1_170, 390)
        );
        assert_eq!(
            super::oriented_decode_request(3_000, 1_000, Orientation::Rotate270FlipH),
            (1_170, 390)
        );
        assert_eq!(
            super::oriented_decode_request(3_000, 1_000, Orientation::Rotate180),
            (390, 390)
        );
        assert_eq!(
            super::oriented_decode_request(1_000, 3_000, Orientation::NoTransforms),
            (390, 1_170)
        );
    }

    #[test]
    fn unreadable_exif_orientation_leaves_the_stored_orientation() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.jpg");
        // Orientation value 9 is outside the Exif range.
        write_oriented(
            &source,
            &quadrants(1_200, 900),
            image::ImageFormat::Jpeg,
            Some(9),
        );
        assert_eq!(
            super::image_orientation(&source).unwrap(),
            Orientation::NoTransforms
        );
        let target = dir.path().join("thumbs/out.jpg");
        super::create_image_thumb(&source, &target).unwrap();
        assert_eq!(image::image_dimensions(&target).unwrap(), (390, 293));
    }

    #[test]
    fn keyframe_mode_renders_a_long_gop_video_with_system_tools() {
        let ffmpeg = std::path::Path::new("/usr/bin/ffmpeg");
        if !ffmpeg.is_file() {
            return;
        }
        let tmp = tempfile::tempdir().expect("tempdir");
        let video = tmp.path().join("gop.mp4");
        let status = std::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=25",
                "-t",
                "6",
                "-g",
                "50",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&video)
            .status()
            .expect("generate video fixture");
        assert!(status.success());
        let thumb = tmp.path().join("gop.jpg");
        super::create_video_thumb(ffmpeg, &video, &thumb, 3.0, super::VideoFrameSeek::Keyframe)
            .expect("render keyframe thumbnail");
        assert_eq!(image::image_dimensions(&thumb).unwrap().0, 390);
    }

    #[test]
    fn video_tools_refuse_reference_content_and_still_read_real_containers() {
        use crate::test_support::{
            connection_arrived, loopback_listener, reference_fixtures, OpenWatch,
        };
        let ffmpeg = std::path::Path::new("/usr/bin/ffmpeg");
        if !ffmpeg.is_file() {
            return;
        }
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("root");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let clip = |path: &std::path::Path| {
            let status = std::process::Command::new(ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
                .args(["testsrc2=size=160x90:rate=10", "-t", "2", "-c:v", "mpeg4"])
                .arg(path)
                .status()
                .expect("generate video fixture");
            assert!(status.success());
        };
        let secret = outside.join("secret.mp4");
        let sibling = root.join("sibling.mp4");
        clip(&secret);
        clip(&sibling);
        let (listener, url) = loopback_listener();
        let fixtures = reference_fixtures(&url, secret.to_str().unwrap(), "sibling.mp4");
        for (name, content) in &fixtures {
            std::fs::write(root.join(name), content).unwrap();
        }
        let watch = OpenWatch::new(&[&secret, &sibling]);

        for (name, _) in &fixtures {
            let source = root.join(name);
            assert_eq!(probe_video_duration_ms(ffmpeg, &source), None, "{name}");
            let thumb = tmp.path().join(format!("{name}.jpg"));
            for seek in [
                super::VideoFrameSeek::Keyframe,
                super::VideoFrameSeek::Exact,
            ] {
                assert!(
                    create_video_thumb(ffmpeg, &source, &thumb, 0.0, seek).is_err(),
                    "{name}"
                );
            }
            assert!(!thumb.exists(), "{name} produced a thumbnail");
            assert!(!connection_arrived(&listener), "{name} reached the network");
            assert!(!watch.opened(), "{name} opened a file it names");
        }

        // WebM uses the Matroska demuxer; it is not generated here because it
        // needs an encoder that not every ffmpeg build has.
        for extension in ["mp4", "mkv", "mov", "avi"] {
            let source = root.join(format!("real.{extension}"));
            clip(&source);
            let duration = probe_video_duration_ms(ffmpeg, &source);
            assert!(
                duration.is_some_and(|ms| ms > 1_500),
                "{extension}: {duration:?}"
            );
            let thumb = tmp.path().join(format!("real-{extension}.jpg"));
            create_video_thumb(ffmpeg, &source, &thumb, 1.0, super::VideoFrameSeek::Exact)
                .expect("render video thumbnail");
            assert_eq!(
                image::image_dimensions(&thumb).unwrap().0,
                390,
                "{extension}"
            );
        }
    }
}
