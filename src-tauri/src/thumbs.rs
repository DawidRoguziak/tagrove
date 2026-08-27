use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use image::imageops::FilterType;
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
/// `modified_at`; targets produced by older versions are regenerated on demand.
pub const THUMB_CACHE_VERSION: u32 = 2;

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

pub fn thumb_target(thumbs_dir: &Path, version: &SourceVersion) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(THUMB_CACHE_VERSION.to_le_bytes());
    hasher.update([0u8]);
    hasher.update(version.path.as_bytes());
    hasher.update([0u8]);
    hasher.update(version.size_bytes.to_le_bytes());
    hasher.update(version.mtime_ns.to_le_bytes());
    let key = format!("{:x}", hasher.finalize());
    thumbs_dir.join(format!("{}.jpg", key))
}

fn probe_image_dimensions(source_path: &Path) -> anyhow::Result<(u32, u32)> {
    let reader = image::ImageReader::open(source_path)?;
    Ok(reader.with_guessed_format()?.into_dimensions()?)
}

/// Pure budget rule: whether a source image must be rejected before decode.
pub fn exceeds_decode_budget(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) > MAX_DECODE_PIXELS
}

pub fn create_image_thumb(source_path: &Path, target_path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let (source_width, source_height) = probe_image_dimensions(source_path)?;
    if exceeds_decode_budget(source_width, source_height) {
        return Err(anyhow::anyhow!(
            "image {source_width}x{source_height} exceeds decode budget of {MAX_DECODE_PIXELS} pixels"
        ));
    }

    let image = image::open(source_path)?;
    let thumb =
        if let Some((x, y, size)) = top_biased_square_crop_rect(image.width(), image.height()) {
            image
                .crop_imm(x, y, size, size)
                .resize_exact(IMAGE_THUMB_SIZE, IMAGE_THUMB_SIZE, FilterType::Lanczos3)
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

pub fn create_video_thumb(
    ffmpeg_path: &Path,
    source_path: &Path,
    target_path: &Path,
    seek_seconds: f64,
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
        .arg("1")
        .arg("-ss")
        .arg(format!("{seek_seconds:.3}"))
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
    match wait_for_exit_with_timeout(&mut child, timeout)? {
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
    match wait_for_exit_with_timeout(&mut child, timeout)? {
        Some(_) => child.wait_with_output().map_err(Into::into),
        None => {
            terminate_child(&mut child);
            Err(anyhow::anyhow!(
                "video tool execution timed out after {}ms",
                timeout.as_millis()
            ))
        }
    }
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
mod tests {
    use super::{
        create_video_thumb, exceeds_decode_budget, parse_duration_ms_from_ffmpeg_stderr,
        parse_duration_ms_from_ffprobe_stdout, probe_video_duration_ms, resolve_video_seek_seconds,
        thumb_target, top_biased_square_crop_rect, SourceVersion, MAX_DECODE_PIXELS,
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
        create_video_thumb(ffmpeg, &video, &thumb, 0.1).expect("render video thumbnail");
        assert!(thumb.is_file());
    }
}
