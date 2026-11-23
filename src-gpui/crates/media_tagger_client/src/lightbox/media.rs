//! What the media stage shows for the selected asset (`LightboxMediaStage.tsx`) and
//! what the GPUI adapter may decode for it.
//!
//! The Solid stage hands the source to the webview, which reads, decodes and scales it
//! by itself. The GPUI adapter reads the file and decodes it on a background thread
//! and holds the decoded pixels, so this module also decides what is too large to
//! hold ([`decode_refusal`]) and prepares the bytes: the format is taken from the
//! content, never from the file name ([`sniff_format`]), and a GIF is cut down to its
//! first frame ([`gif_first_frame`]), because the GPUI lightbox shows a GIF as a still
//! picture with a chip (plan decision D8; the animation is epic E20).

use super::model::SelectedAsset;

/// The kind of the selected asset (`AssetSummary::kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageKind {
    Image,
    Gif,
    Video,
}

impl StageKind {
    /// The kind of a summary's `kind`; anything that is neither a GIF nor a video is
    /// shown as an image, as the Solid stage does.
    pub fn of(kind: &str) -> Self {
        match kind {
            "gif" => Self::Gif,
            "video" => Self::Video,
            _ => Self::Image,
        }
    }

    /// The locale key of the kind's name (`lightbox.kind.*`).
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Image => "lightbox.kind.image",
            Self::Gif => "lightbox.kind.gif",
            Self::Video => "lightbox.kind.video",
        }
    }

    /// `data-lightbox-kind`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Gif => "gif",
            Self::Video => "video",
        }
    }

    pub const fn is_video(self) -> bool {
        matches!(self, Self::Video)
    }
}

/// The identity of what the stage loads: another asset, kind or path is another
/// source, and a failure of the one before it is forgotten (`source()` of the Solid
/// stage).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaSource {
    pub asset_id: i64,
    pub kind: StageKind,
    pub path: String,
}

impl MediaSource {
    /// The source of the selected asset; `None` while its path is not verified.
    pub fn of(selected: &SelectedAsset) -> Option<Self> {
        Some(Self {
            asset_id: selected.id(),
            kind: StageKind::of(&selected.summary.kind),
            path: selected.path.clone()?,
        })
    }
}

/// How far the adapter is with the picture of a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadState {
    /// Being read or decoded.
    Loading,
    Ready,
    Failed,
}

/// What the stage draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageContent {
    /// No verified path yet: the spinner (`lightbox-details-loading`).
    DetailsLoading,
    /// No verified path and the details failed: the notice with Retry
    /// (`lightbox-details-error`).
    DetailsFailed,
    /// The picture could not be shown (`lightbox-media-error`).
    MediaFailed,
    /// The picture is on its way: the stage, and a spinner once that has taken a
    /// moment (`lightbox-media-loading`; the webview shows nothing meanwhile).
    Pending,
    /// The picture of an image, zoomable.
    Image,
    /// The first frame of a GIF, zoomable, with the GIF chip.
    Gif,
    /// The placeholder of a video: its thumbnail as a poster, the Video label and its
    /// metadata; nothing plays (epic E21).
    Video,
}

impl StageContent {
    /// What the stage draws for an asset of `kind`. `has_path`: its source is
    /// verified; `details_failed`: the details could not be read; `load`: where the
    /// adapter is with the picture (a video loads none).
    pub fn of(kind: StageKind, has_path: bool, details_failed: bool, load: LoadState) -> Self {
        if !has_path {
            return if details_failed {
                Self::DetailsFailed
            } else {
                Self::DetailsLoading
            };
        }
        match (kind, load) {
            (StageKind::Video, _) => Self::Video,
            (_, LoadState::Failed) => Self::MediaFailed,
            (_, LoadState::Loading) => Self::Pending,
            (StageKind::Image, LoadState::Ready) => Self::Image,
            (StageKind::Gif, LoadState::Ready) => Self::Gif,
        }
    }

    /// A zoomable picture is on the stage.
    pub const fn is_picture(self) -> bool {
        matches!(self, Self::Image | Self::Gif)
    }
}

/// A picture format the adapter can decode, recognised by its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PictureFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
}

/// The format of a file from its content. The library indexes `jpg`, `jpeg`, `png`,
/// `webp`, `bmp` and `gif` files by extension; the content decides here.
pub fn sniff_format(bytes: &[u8]) -> Option<PictureFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(PictureFormat::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(PictureFormat::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(PictureFormat::Gif)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(PictureFormat::Webp)
    } else if bytes.starts_with(b"BM") {
        Some(PictureFormat::Bmp)
    } else {
        None
    }
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    let pair = bytes.get(at..at + 2)?;
    Some(u32::from(u16::from_le_bytes([pair[0], pair[1]])))
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let triple = bytes.get(at..at + 3)?;
    Some(u32::from_le_bytes([triple[0], triple[1], triple[2], 0]))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    let quad = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]))
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    let pair = bytes.get(at..at + 2)?;
    Some(u32::from(u16::from_be_bytes([pair[0], pair[1]])))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    let quad = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]]))
}

/// The size of a BMP's BITMAPCOREHEADER, the one DIB header with 16-bit sides.
const BMP_CORE_HEADER_BYTES: u32 = 12;

/// The pixel size a file's header declares, without decoding anything. `None` for a
/// header this function cannot read; the decoder then decides.
pub fn probe_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match sniff_format(bytes)? {
        // The IHDR chunk is first: width and height after its length and name.
        PictureFormat::Png => Some((be32(bytes, 16)?, be32(bytes, 20)?)),
        // The logical screen.
        PictureFormat::Gif => Some((le16(bytes, 6)?, le16(bytes, 8)?)),
        // The size of the DIB header tells how it stores the sides.
        PictureFormat::Bmp => match le32(bytes, 14)? {
            // BITMAPCOREHEADER (OS/2 1.x): two unsigned 16-bit sides.
            BMP_CORE_HEADER_BYTES => Some((le16(bytes, 18)?, le16(bytes, 20)?)),
            // BITMAPINFOHEADER and later: signed, a negative height is a top-down
            // bitmap.
            _ => {
                let width = le32(bytes, 18)? as i32;
                let height = le32(bytes, 22)? as i32;
                Some((width.unsigned_abs(), height.unsigned_abs()))
            }
        },
        PictureFormat::Jpeg => probe_jpeg(bytes),
        PictureFormat::Webp => probe_webp(bytes),
    }
}

/// The frame header of a JPEG: the first start-of-frame segment.
fn probe_jpeg(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    loop {
        // Fill bytes before a marker.
        while *bytes.get(at)? == 0xff && *bytes.get(at + 1)? == 0xff {
            at += 1;
        }
        if *bytes.get(at)? != 0xff {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        at += 2;
        match marker {
            // Markers without a segment.
            0x01 | 0xd0..=0xd8 => continue,
            // End of image, or the scan started without a frame header.
            0xd9 | 0xda => return None,
            // SOF0..SOF15 except DHT (C4), JPG (C8) and DAC (CC).
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                return Some((be16(bytes, at + 5)?, be16(bytes, at + 3)?));
            }
            _ => {
                let length = be16(bytes, at)? as usize;
                if length < 2 {
                    return None;
                }
                at += length;
            }
        }
    }
}

/// The canvas of a WebP file: the VP8X header, or the first frame of a simple file.
fn probe_webp(bytes: &[u8]) -> Option<(u32, u32)> {
    match bytes.get(12..16)? {
        b"VP8X" => Some((le24(bytes, 24)? + 1, le24(bytes, 27)? + 1)),
        b"VP8L" => {
            if *bytes.get(20)? != 0x2f {
                return None;
            }
            let bits = le32(bytes, 21)?;
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8 " => {
            if bytes.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            Some((le16(bytes, 26)? & 0x3fff, le16(bytes, 28)? & 0x3fff))
        }
        _ => None,
    }
}

/// The largest file the adapter reads into memory for a picture.
pub const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
/// The most decoded pixels the adapter holds for one picture: four bytes each in
/// memory and four more in the window's texture. This is the limit that holds: the
/// decoders GPUI uses refuse nothing below it (`image::ImageReader::into_decoder`
/// checks a picture's dimensions only; its default allocation limit of 512 MiB is
/// not held against the decoded picture; ADR 0046). While a picture is decoded it
/// takes more for a moment: an RGB picture is three bytes a pixel before it is
/// converted to four.
pub const MAX_DECODED_PIXELS: u64 = 192 * 1024 * 1024;
/// The longest side of a picture: the largest texture a common GPU accepts.
pub const MAX_SIDE: u32 = 16_384;

/// Why a picture is not decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeRefusal {
    /// The file is larger than [`MAX_SOURCE_BYTES`].
    SourceTooLarge,
    /// The picture has more than [`MAX_DECODED_PIXELS`] pixels or a side longer than
    /// [`MAX_SIDE`].
    PictureTooLarge,
    /// A side is zero.
    Empty,
}

/// Whether a file of `source_bytes` is read at all.
pub fn source_refusal(source_bytes: u64) -> Option<DecodeRefusal> {
    (source_bytes > MAX_SOURCE_BYTES).then_some(DecodeRefusal::SourceTooLarge)
}

/// Whether a picture of this size with `frames` frames is decoded (the adapter's
/// decoder keeps every frame of an animated WebP). The size is the one its header
/// declares ([`probe_dimensions`]), else the one the library stored for the asset;
/// without either the decoder decides.
pub fn decode_refusal(dimensions: Option<(u32, u32)>, frames: u64) -> Option<DecodeRefusal> {
    let (width, height) = dimensions?;
    if width == 0 || height == 0 {
        return Some(DecodeRefusal::Empty);
    }
    let pixels = (u64::from(width) * u64::from(height)).saturating_mul(frames.max(1));
    (width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_DECODED_PIXELS)
        .then_some(DecodeRefusal::PictureTooLarge)
}

/// The number of frames of a WebP file: its `ANMF` chunks, 1 for a still picture.
pub fn webp_frame_count(bytes: &[u8]) -> u64 {
    let mut frames = 0;
    let mut at = 12;
    while let (Some(name), Some(size)) = (bytes.get(at..at + 4), le32(bytes, at + 4)) {
        if name == b"ANMF" {
            frames += 1;
        }
        // A chunk's data is padded to an even length.
        let size = size as usize;
        match at.checked_add(8 + size + (size & 1)) {
            Some(next) => at = next,
            None => break,
        }
    }
    frames.max(1)
}

/// The dimensions the library stored for an asset, when both are positive.
pub fn stored_dimensions(width: Option<i64>, height: Option<i64>) -> Option<(u32, u32)> {
    let width = u32::try_from(width?).ok().filter(|width| *width > 0)?;
    let height = u32::try_from(height?).ok().filter(|height| *height > 0)?;
    Some((width, height))
}

const GIF_EXTENSION: u8 = 0x21;
const GIF_IMAGE: u8 = 0x2c;
const GIF_TRAILER: u8 = 0x3b;
const GIF_APPLICATION_EXTENSION: u8 = 0xff;

/// The size of a GIF colour table from the packed byte that announces it.
fn gif_color_table_len(packed: u8) -> usize {
    if packed & 0x80 == 0 {
        0
    } else {
        3 << ((packed & 0x07) + 1)
    }
}

/// The index after the data sub-blocks that start at `at` (each a length byte and
/// that many bytes, ended by a zero length).
fn gif_skip_sub_blocks(bytes: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let length = usize::from(*bytes.get(at)?);
        at += 1;
        if length == 0 {
            return Some(at);
        }
        at = at.checked_add(length)?;
        if at > bytes.len() {
            return None;
        }
    }
}

/// A GIF with the first frame of `bytes` only: the header, the screen, the global
/// colour table, the extensions that belong to the first picture (its graphic
/// control), the first picture and the trailer. Application extensions (the loop
/// count) are left out. `None` when `bytes` is no GIF or ends before its first
/// picture does.
///
/// Decoding the result needs the memory of one frame, whatever the length of the
/// animation.
pub fn gif_first_frame(bytes: &[u8]) -> Option<Vec<u8>> {
    if sniff_format(bytes)? != PictureFormat::Gif {
        return None;
    }
    let screen_end = 13 + gif_color_table_len(*bytes.get(10)?);
    let mut still = bytes.get(..screen_end)?.to_vec();
    let mut at = screen_end;
    loop {
        match *bytes.get(at)? {
            GIF_EXTENSION => {
                let label = *bytes.get(at + 1)?;
                let end = gif_skip_sub_blocks(bytes, at + 2)?;
                if label != GIF_APPLICATION_EXTENSION {
                    still.extend_from_slice(&bytes[at..end]);
                }
                at = end;
            }
            GIF_IMAGE => {
                let table = gif_color_table_len(*bytes.get(at + 9)?);
                // The descriptor, the local colour table and the code size.
                let data = at + 10 + table + 1;
                let end = gif_skip_sub_blocks(bytes, data)?;
                still.extend_from_slice(&bytes[at..end]);
                still.push(GIF_TRAILER);
                return Some(still);
            }
            // The trailer before any picture, or a byte that starts no block.
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GIF of `frames` 1x1 pictures with a global table of two colours, a loop
    /// extension and a graphic control before every picture.
    fn gif(frames: usize) -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        bytes.extend_from_slice(&[0x21, 0xff, 11]);
        bytes.extend_from_slice(b"NETSCAPE2.0");
        bytes.extend_from_slice(&[3, 1, 0, 0, 0]);
        for _ in 0..frames {
            bytes.extend_from_slice(&[0x21, 0xf9, 4, 0, 10, 0, 0, 0]);
            bytes.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
            bytes.extend_from_slice(&[2, 2, 0x44, 0x01, 0]);
        }
        bytes.push(0x3b);
        bytes
    }

    #[test]
    fn the_stage_shows_what_the_selection_and_the_load_allow() {
        use LoadState::*;
        use StageKind::*;
        for (kind, has_path, details_failed, load, content) in [
            (Image, false, false, Loading, StageContent::DetailsLoading),
            (Image, false, true, Loading, StageContent::DetailsFailed),
            (Video, false, true, Ready, StageContent::DetailsFailed),
            (Image, true, false, Loading, StageContent::Pending),
            (Image, true, true, Ready, StageContent::Image),
            (Gif, true, false, Ready, StageContent::Gif),
            (Gif, true, false, Failed, StageContent::MediaFailed),
            (Video, true, false, Loading, StageContent::Video),
            (Video, true, false, Failed, StageContent::Video),
        ] {
            assert_eq!(
                StageContent::of(kind, has_path, details_failed, load),
                content,
                "{kind:?} {has_path} {details_failed} {load:?}"
            );
        }
        assert!(StageContent::Gif.is_picture() && StageContent::Image.is_picture());
        assert!(!StageContent::Video.is_picture() && !StageContent::Pending.is_picture());
    }

    #[test]
    fn kinds_follow_the_summary_and_unknown_ones_are_images() {
        assert_eq!(StageKind::of("gif"), StageKind::Gif);
        assert_eq!(StageKind::of("video"), StageKind::Video);
        assert_eq!(StageKind::of("image"), StageKind::Image);
        assert_eq!(StageKind::of(""), StageKind::Image);
        assert_eq!(StageKind::Video.label_key(), "lightbox.kind.video");
        assert!(StageKind::Video.is_video() && !StageKind::Gif.is_video());
    }

    #[test]
    fn formats_are_recognised_by_content() {
        assert_eq!(
            sniff_format(b"\x89PNG\r\n\x1a\n...."),
            Some(PictureFormat::Png)
        );
        assert_eq!(
            sniff_format(&[0xff, 0xd8, 0xff, 0xe0]),
            Some(PictureFormat::Jpeg)
        );
        assert_eq!(sniff_format(b"GIF87a"), Some(PictureFormat::Gif));
        assert_eq!(
            sniff_format(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(PictureFormat::Webp)
        );
        assert_eq!(sniff_format(b"BM\0\0"), Some(PictureFormat::Bmp));
        assert_eq!(sniff_format(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(sniff_format(b""), None);
        assert_eq!(sniff_format(b"<svg/>"), None);
    }

    #[test]
    fn headers_declare_their_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&[0, 0, 0x12, 0x34, 0, 0, 0, 0x40]);
        assert_eq!(probe_dimensions(&png), Some((0x1234, 0x40)));
        assert_eq!(probe_dimensions(&png[..20]), None);

        assert_eq!(probe_dimensions(&gif(1)), Some((1, 1)));

        // The file header, then a BITMAPINFOHEADER (40 bytes) of a top-down bitmap.
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&[0; 12]);
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&640i32.to_le_bytes());
        bmp.extend_from_slice(&(-480i32).to_le_bytes());
        assert_eq!(probe_dimensions(&bmp), Some((640, 480)));
        assert_eq!(probe_dimensions(&bmp[..24]), None);

        // A BITMAPCOREHEADER (12 bytes) has 16-bit sides, then the planes and the
        // bits per pixel: read as 32-bit sides they would be millions of pixels.
        let mut core = b"BM".to_vec();
        core.extend_from_slice(&[0; 12]);
        core.extend_from_slice(&12u32.to_le_bytes());
        core.extend_from_slice(&640u16.to_le_bytes());
        core.extend_from_slice(&480u16.to_le_bytes());
        core.extend_from_slice(&[1, 0, 24, 0]);
        assert_eq!(probe_dimensions(&core), Some((640, 480)));
        assert_eq!(decode_refusal(probe_dimensions(&core), 1), None);
        assert_eq!(probe_dimensions(&core[..21]), None);

        // SOI, an APP0 segment of 4 bytes, then SOF2 with height 0x0102, width 0x0304.
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc2, 0, 11, 8, 1, 2, 3, 4, 3,
        ];
        assert_eq!(probe_dimensions(&jpeg), Some((0x0304, 0x0102)));
        assert_eq!(probe_dimensions(&[0xff, 0xd8, 0xff, 0xda, 0, 2]), None);
        assert_eq!(probe_dimensions(&[0xff, 0xd8, 0xff, 0xe0, 0, 0]), None);

        let mut vp8x = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
        vp8x.extend_from_slice(&[10, 0, 0, 0, 0, 0, 0, 0]);
        vp8x.extend_from_slice(&[0xff, 0x03, 0, 0x2f, 0x01, 0]);
        assert_eq!(probe_dimensions(&vp8x), Some((1024, 304)));

        let mut vp8l = b"RIFF\0\0\0\0WEBPVP8L".to_vec();
        vp8l.extend_from_slice(&[0, 0, 0, 0, 0x2f]);
        // 14 bits of width - 1 (99), then 14 bits of height - 1 (49).
        vp8l.extend_from_slice(&(99u32 | (49 << 14)).to_le_bytes());
        assert_eq!(probe_dimensions(&vp8l), Some((100, 50)));

        let mut vp8 = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
        vp8.extend_from_slice(&[
            0, 0, 0, 0, 0, 0, 0, 0x9d, 0x01, 0x2a, 0x40, 0x01, 0xf0, 0x00,
        ]);
        assert_eq!(probe_dimensions(&vp8), Some((320, 240)));
    }

    #[test]
    fn pictures_beyond_the_budget_are_refused_before_they_are_decoded() {
        assert_eq!(decode_refusal(None, 1), None);
        assert_eq!(decode_refusal(Some((6000, 4000)), 1), None);
        assert_eq!(decode_refusal(Some((MAX_SIDE, 8192)), 1), None);
        assert_eq!(decode_refusal(Some((0, 10)), 1), Some(DecodeRefusal::Empty));
        assert_eq!(
            decode_refusal(Some((MAX_SIDE + 1, 10)), 1),
            Some(DecodeRefusal::PictureTooLarge)
        );
        assert_eq!(
            decode_refusal(Some((MAX_SIDE, MAX_SIDE)), 1),
            Some(DecodeRefusal::PictureTooLarge)
        );
        // Every frame of an animated WebP is held.
        assert_eq!(decode_refusal(Some((1920, 1080)), 90), None);
        assert_eq!(
            decode_refusal(Some((1920, 1080)), 120),
            Some(DecodeRefusal::PictureTooLarge)
        );

        let mut animated = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
        animated.extend_from_slice(&[10, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        for _ in 0..3 {
            animated.extend_from_slice(b"ANMF");
            animated.extend_from_slice(&[3, 0, 0, 0, 1, 2, 3, 0]);
        }
        assert_eq!(webp_frame_count(&animated), 3);
        assert_eq!(webp_frame_count(b"RIFF\0\0\0\0WEBPVP8 "), 1);
        assert_eq!(webp_frame_count(b""), 1);
        assert_eq!(source_refusal(MAX_SOURCE_BYTES), None);
        assert_eq!(
            source_refusal(MAX_SOURCE_BYTES + 1),
            Some(DecodeRefusal::SourceTooLarge)
        );
        assert_eq!(stored_dimensions(Some(640), Some(480)), Some((640, 480)));
        assert_eq!(stored_dimensions(Some(640), None), None);
        assert_eq!(stored_dimensions(Some(0), Some(480)), None);
        assert_eq!(stored_dimensions(Some(-5), Some(480)), None);
    }

    #[test]
    fn a_gif_is_cut_down_to_its_first_frame() {
        let one = gif(1);
        let three = gif(3);
        let still = gif_first_frame(&three).expect("a still");
        assert_eq!(gif_first_frame(&one), Some(still.clone()));
        // The loop extension is gone, the graphic control and the picture stay.
        assert!(!still.windows(11).any(|window| window == b"NETSCAPE2.0"));
        assert_eq!(still.iter().filter(|byte| **byte == 0x2c).count(), 1);
        assert_eq!(still.last(), Some(&0x3b));
        assert_eq!(sniff_format(&still), Some(PictureFormat::Gif));
        assert_eq!(probe_dimensions(&still), Some((1, 1)));
        assert!(still.len() < three.len());
    }

    #[test]
    fn a_broken_gif_gives_no_still() {
        let whole = gif(2);
        assert_eq!(gif_first_frame(b"GIF89a"), None);
        assert_eq!(gif_first_frame(b"\x89PNG\r\n\x1a\n"), None);
        // Cut inside the first picture's data.
        let first_picture = whole
            .iter()
            .position(|byte| *byte == 0x2c)
            .expect("a picture");
        assert_eq!(gif_first_frame(&whole[..first_picture + 12]), None);
        // A trailer before any picture.
        let mut empty = whole[..19].to_vec();
        empty.push(0x3b);
        assert_eq!(gif_first_frame(&empty), None);
    }

    #[test]
    fn a_source_needs_a_verified_path() {
        use media_tagger_core::models::AssetSummary;
        let summary = AssetSummary {
            id: 7,
            file_name: "clip.gif".into(),
            preview_path: None,
            kind: "gif".into(),
            modified_at: 0,
            width: Some(96),
            height: Some(64),
            duration_ms: None,
            thumb_version: "0123456789abcdef".into(),
            is_favorite: false,
            media_group_key: None,
            media_group_order: None,
        };
        let mut selected = SelectedAsset {
            summary,
            path: None,
            size_bytes: None,
            tags: Vec::new(),
        };
        assert_eq!(MediaSource::of(&selected), None);
        selected.path = Some("/media/clip.gif".into());
        assert_eq!(
            MediaSource::of(&selected),
            Some(MediaSource {
                asset_id: 7,
                kind: StageKind::Gif,
                path: "/media/clip.gif".into()
            })
        );
    }
}
