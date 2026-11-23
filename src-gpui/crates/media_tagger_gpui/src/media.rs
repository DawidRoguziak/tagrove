//! The picture of the asset the lightbox shows: read from its file and decoded on
//! GPUI's background executor, never on the UI thread, and held once.
//!
//! The Solid stage gives the source to the webview through the asset protocol, whose
//! scope is every path; here the adapter reads the file the backend's details named
//! (`SelectedAsset::path`). What may be decoded is decided by
//! `media_tagger_client::lightbox::media`: the format comes from the content, a file
//! or a picture over the budget is refused before a pixel is decoded, and a GIF is
//! cut down to its first frame (plan decision D8). A refusal and a failed decode
//! look the same to the view: the picture could not be displayed.
//!
//! [`Originals`] holds the picture of one source. Another source drops it, and the
//! answer to a load that was started for a source that is no longer shown is dropped
//! by its generation. A dropped picture's texture is taken out of the window's atlas
//! with the next frame ([`Originals::release_textures`]); GPUI itself never frees an
//! image texture.
//!
//! One load runs at a time. A decode that a background thread has begun cannot be
//! stopped, so a source that is shown while one runs waits for it to end; the running
//! load gives up before its next step (reading the file, decoding it) once its source
//! is no longer shown, and the sources passed in between are never read. Holding an
//! arrow key over large pictures therefore holds the bytes and the pixels of one
//! picture, not of one per background thread.
//!
//! A read cannot be stopped either, and unlike a decode it may never end: a file on a
//! mount that stopped answering blocks its thread for as long as the mount is silent.
//! Such a read must not keep every later picture waiting. The read of a source that is
//! no longer shown is therefore given up once it has run for [`READ_DEADLINE`]: it is
//! left to return on its thread, where its bytes are dropped, and the shown source is
//! loaded. The read of the shown source itself is waited for as long as it is shown,
//! as a webview waits for its image; the stage shows a spinner meanwhile. Reads that
//! were given up still hold a background thread and their bytes each, so their number
//! is bounded ([`reads_allowed`]): beyond it the shown source waits until one of them
//! has returned.

use std::{
    fs::File,
    future::Future,
    io::Read,
    path::Path,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use gpui::{
    App, AppContext as _, Context, FutureExt as _, Image, ImageFormat, RenderImage, SvgRenderer,
    Task, Window,
};
use media_tagger_client::lightbox::{
    decode_refusal, gif_first_frame, probe_dimensions, sniff_format, webp_frame_count, LoadState,
    MediaSource, PictureFormat, MAX_SOURCE_BYTES,
};

/// A decoded picture.
pub type Picture = Arc<RenderImage>;

/// How long the read of a source that is no longer shown may keep the next source
/// waiting. A read that has run this long is checked again after every such span.
pub const READ_DEADLINE: Duration = Duration::from_secs(10);

/// The most reads that run at once: the one of the load that runs and those that were
/// given up and have not returned. Each holds a background thread and at most the
/// bytes of its file (`MAX_SOURCE_BYTES`).
pub const MAX_READS: usize = 3;

/// How many reads may run at once on a background executor of `threads` threads: one
/// thread stays free for everything else the executor does (thumbnails, timers), and
/// never more than [`MAX_READS`]. With two threads no read is given up.
pub fn reads_allowed(threads: usize) -> usize {
    threads.saturating_sub(1).clamp(1, MAX_READS)
}

/// Asked by a read before each of its steps: whether its bytes are still waited for.
pub type Wanted = Box<dyn Fn() -> bool + Send>;
/// The bytes of a source's file on their way, on the background executor. `None`:
/// there is nothing to decode.
pub type Reading = Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send>>;
/// What reads the file of a source.
pub type Reader = Box<dyn Fn(MediaSource, Wanted) -> Reading>;

pub struct Originals {
    source: Option<MediaSource>,
    state: LoadState,
    picture: Option<Picture>,
    /// The dimensions the library has for the shown source.
    stored: Option<(u32, u32)>,
    /// Counts the sources: a decode answers only the one it was started for.
    generation: u64,
    /// The generation again, for the load on the background thread to read: it gives
    /// up once this is no longer its own.
    shown: Arc<AtomicU64>,
    /// Dropped pictures whose textures are still in the atlas.
    retired: Vec<Picture>,
    /// The load that runs, of the shown source or of one that was shown before.
    load: Option<Task<()>>,
    loads_started: u64,
    reader: Reader,
    /// The reads that were given up and have not returned.
    abandoned: usize,
}

impl Default for Originals {
    fn default() -> Self {
        Self::new()
    }
}

impl Originals {
    pub fn new() -> Self {
        Self {
            source: None,
            state: LoadState::Loading,
            picture: None,
            stored: None,
            generation: 0,
            shown: Arc::new(AtomicU64::new(0)),
            retired: Vec::new(),
            load: None,
            loads_started: 0,
            reader: Box::new(read_file),
            abandoned: 0,
        }
    }

    /// Replaces what reads the files: a test's reader may never answer, as a file on
    /// a mount that stopped answering does.
    pub fn set_reader(&mut self, reader: Reader) {
        self.reader = reader;
    }

    /// The source shown now.
    pub fn source(&self) -> Option<&MediaSource> {
        self.source.as_ref()
    }

    /// How far the picture of the shown source is. A video has none and stays
    /// loading.
    pub fn state(&self) -> LoadState {
        self.state
    }

    /// The decoded picture of the shown source.
    pub fn picture(&self) -> Option<&Picture> {
        self.picture.as_ref()
    }

    /// The pixel size of the decoded picture.
    pub fn size(&self) -> Option<(u32, u32)> {
        let size = self.picture.as_ref()?.size(0);
        Some((
            u32::try_from(size.width.0).ok()?,
            u32::try_from(size.height.0).ok()?,
        ))
    }

    /// Dropped pictures whose textures wait for [`Self::release_textures`].
    pub fn retired_pictures(&self) -> usize {
        self.retired.len()
    }

    /// How many loads were handed to the background executor so far. A source that
    /// was replaced while another load ran is not among them.
    pub fn loads_started(&self) -> u64 {
        self.loads_started
    }

    /// How many reads were given up and have not returned yet.
    pub fn abandoned_reads(&self) -> usize {
        self.abandoned
    }

    /// Shows `source`: the picture of another source is dropped and, unless the new
    /// one is a video or nothing, its picture is read and decoded in the background,
    /// after the load that runs now, if one does ([`READ_DEADLINE`] bounds how long
    /// the read of that one is waited for). `stored` are the dimensions the
    /// library has for the asset, which stand in for a header the adapter cannot
    /// read. The same source again changes nothing, also after it failed.
    pub fn show(
        &mut self,
        source: Option<MediaSource>,
        stored: Option<(u32, u32)>,
        cx: &mut Context<Self>,
    ) {
        if self.source == source {
            return;
        }
        self.generation += 1;
        self.shown.store(self.generation, Ordering::Relaxed);
        self.retired.extend(self.picture.take());
        self.state = LoadState::Loading;
        self.source = source;
        self.stored = stored;
        cx.notify();
        self.start(cx);
    }

    /// Starts the load of the shown source if it still waits for one, unless a load
    /// runs or the reads that were given up leave no room for another: the running
    /// load starts it when it has ended ([`Self::loaded`]) or its read was given up
    /// ([`Self::give_up`]), a given-up read when it has returned.
    fn start(&mut self, cx: &mut Context<Self>) {
        if self.load.is_some() || self.state != LoadState::Loading {
            return;
        }
        let Some(source) = self.source.clone() else {
            return;
        };
        if source.kind.is_video() {
            return;
        }
        if self.abandoned >= reads_allowed(cx.background_executor().num_cpus()) {
            return;
        }
        let generation = self.generation;
        let stored = self.stored;
        let renderer = cx.svg_renderer();
        let wanted = {
            let shown = self.shown.clone();
            move || shown.load(Ordering::Relaxed) == generation
        };
        let mut read = cx.background_spawn((self.reader)(source, Box::new(wanted.clone())));
        self.loads_started += 1;
        self.load = Some(cx.spawn(async move |this, cx| {
            let executor = cx.background_executor().clone();
            let bytes = loop {
                let outcome = (&mut read).with_timeout(READ_DEADLINE, &executor).await;
                match outcome {
                    Ok(bytes) => break bytes,
                    // The picture is still waited for, and nothing else for its turn.
                    Err(_) if wanted() => {}
                    Err(_) => {
                        this.update(cx, |originals, cx| originals.give_up(read, cx))
                            .ok();
                        return;
                    }
                }
            };
            let picture = match bytes {
                Some(bytes) if wanted() => {
                    executor
                        .spawn(async move { decode(bytes, stored, renderer) })
                        .await
                }
                _ => None,
            };
            this.update(cx, |originals, cx| {
                originals.loaded(generation, picture, cx)
            })
            .ok();
        }));
    }

    /// The read of a source that is no longer shown has run for [`READ_DEADLINE`]
    /// and has not returned: the shown source no longer waits for it. The read
    /// cannot be stopped; it is counted until it returns, and its bytes are dropped
    /// then.
    fn give_up(&mut self, read: Task<Option<Vec<u8>>>, cx: &mut Context<Self>) {
        self.load = None;
        self.abandoned += 1;
        cx.spawn(async move |this, cx| {
            drop(read.await);
            this.update(cx, |originals, cx| {
                originals.abandoned -= 1;
                // A source that found no room for its read has waited for this one.
                originals.start(cx);
            })
            .ok();
        })
        .detach();
        self.start(cx);
    }

    fn loaded(&mut self, generation: u64, picture: Option<Picture>, cx: &mut Context<Self>) {
        self.load = None;
        if generation != self.generation {
            // The answer of a source that is no longer shown; its picture was never
            // drawn, so it has no texture. The shown source has waited for it.
            self.start(cx);
            return;
        }
        self.state = if picture.is_some() {
            LoadState::Ready
        } else {
            LoadState::Failed
        };
        self.picture = picture;
        cx.notify();
    }

    /// The window could not draw the picture (its GPU takes no texture of that
    /// size): it counts as failed.
    pub fn draw_failed(&mut self, cx: &mut Context<Self>) {
        if self.state == LoadState::Ready {
            self.retired.extend(self.picture.take());
            self.state = LoadState::Failed;
            cx.notify();
        }
    }

    /// Takes the textures of dropped pictures out of the sprite atlas. Call it with
    /// the window being drawn, before the stage is built.
    pub fn release_textures(&mut self, window: &mut Window, cx: &mut App) {
        for picture in self.retired.drain(..) {
            cx.drop_image(picture, Some(window));
        }
    }
}

/// Reads the file of `source` with `std::fs`, on the thread that polls the future.
/// `None`: there is no regular file to read or it is over the budget
/// (`MAX_SOURCE_BYTES`), or the source is no longer `wanted`.
fn read_file(source: MediaSource, wanted: Wanted) -> Reading {
    Box::pin(async move { read_within(Path::new(&source.path), MAX_SOURCE_BYTES, &wanted) })
}

/// Reads the regular file at `path` if it has at most `limit` bytes. `wanted` is
/// asked before each call that may block.
///
/// The indexer lists regular files only, but the path may name something else by the
/// time it is shown. A link is followed, as the webview's asset protocol follows it,
/// and what it leads to has to be a regular file: a device or a pipe has no length to
/// refuse and no end to read up to. The path is asked first, because opening a pipe
/// waits for its writer, and what was opened is asked again.
fn read_within(path: &Path, limit: u64, wanted: &dyn Fn() -> bool) -> Option<Vec<u8>> {
    if !wanted() || !std::fs::metadata(path).ok()?.is_file() || !wanted() {
        return None;
    }
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    read_bounded(file, metadata.len(), limit)
}

/// Reads `reader` to its end if that is within `limit` bytes; `length` is what the
/// file said it holds. The limit is held against what is read as well: a file can
/// grow after it was asked, and some (those of `/proc`) hold more than they say.
fn read_bounded(reader: impl Read, length: u64, limit: u64) -> Option<Vec<u8>> {
    if length > limit {
        return None;
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(usize::try_from(length).ok()?)
        .ok()?;
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

/// Decodes the bytes of a picture file as the stage shows it: a GIF as its first
/// frame, an animated WebP with every frame (the stage draws the first).
pub fn decode(
    bytes: Vec<u8>,
    stored: Option<(u32, u32)>,
    renderer: SvgRenderer,
) -> Option<Picture> {
    let format = sniff_format(&bytes)?;
    let frames = match format {
        PictureFormat::Webp => webp_frame_count(&bytes),
        _ => 1,
    };
    if decode_refusal(probe_dimensions(&bytes).or(stored), frames).is_some() {
        return None;
    }
    let (format, bytes) = match format {
        PictureFormat::Gif => (ImageFormat::Gif, gif_first_frame(&bytes)?),
        PictureFormat::Png => (ImageFormat::Png, bytes),
        PictureFormat::Jpeg => (ImageFormat::Jpeg, bytes),
        PictureFormat::Webp => (ImageFormat::Webp, bytes),
        PictureFormat::Bmp => (ImageFormat::Bmp, bytes),
    };
    let picture = Image::from_bytes(format, bytes)
        .to_image_data(renderer)
        .ok()?;
    let size = picture.size(0);
    (size.width.0 > 0 && size.height.0 > 0).then_some(picture)
}

#[cfg(test)]
mod tests {
    use std::{
        io::Read,
        process::Command,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc, Arc,
        },
        time::Duration,
    };

    use super::{read_bounded, read_within};

    const WANTED: &dyn Fn() -> bool = &|| true;

    #[test]
    fn a_file_within_the_budget_is_read_and_a_larger_one_is_not() {
        let media = tempfile::tempdir().unwrap();
        let path = media.path().join("picture.jpg");
        std::fs::write(&path, [7u8; 16]).unwrap();

        assert_eq!(read_within(&path, 16, WANTED), Some(vec![7u8; 16]));
        assert_eq!(read_within(&path, 15, WANTED), None);
        assert_eq!(read_within(&path, 16, &|| false), None, "no longer wanted");
        assert_eq!(
            read_within(&media.path().join("gone.jpg"), 16, WANTED),
            None
        );
    }

    /// The path of an indexed picture names something else by now. A device has no
    /// length to refuse (`/dev/null` stands for `/dev/zero`, which has no end
    /// either) and a pipe without a writer is never opened.
    #[test]
    fn only_a_regular_file_is_read() {
        let media = tempfile::tempdir().unwrap();
        let picture = media.path().join("picture.jpg");
        std::fs::write(&picture, [7u8; 16]).unwrap();

        let linked = media.path().join("linked.jpg");
        std::os::unix::fs::symlink(&picture, &linked).unwrap();
        assert_eq!(
            read_within(&linked, 16, WANTED),
            Some(vec![7u8; 16]),
            "a link to a regular file is followed"
        );

        let device = media.path().join("device.jpg");
        std::os::unix::fs::symlink("/dev/null", &device).unwrap();
        assert_eq!(read_within(&device, 16, WANTED), None);

        let directory = media.path().join("directory.jpg");
        std::fs::create_dir(&directory).unwrap();
        assert_eq!(read_within(&directory, 16, WANTED), None);

        let pipe = media.path().join("pipe.jpg");
        let made = Command::new("mkfifo").arg(&pipe).status().unwrap();
        assert!(made.success(), "mkfifo made the pipe");
        // On its own thread: an open of the pipe would wait for a writer for good.
        let (answer, answered) = mpsc::channel();
        std::thread::spawn(move || answer.send(read_within(&pipe, 16, &|| true)));
        assert_eq!(
            answered
                .recv_timeout(Duration::from_secs(10))
                .expect("the pipe is refused without being opened"),
            None
        );
    }

    /// Hands out `left` bytes and counts those that were taken.
    struct Source {
        left: u64,
        taken: Arc<AtomicU64>,
    }

    impl Read for Source {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let count = (buffer.len() as u64).min(self.left);
            buffer[..count as usize].fill(7);
            self.left -= count;
            self.taken.fetch_add(count, Ordering::Relaxed);
            Ok(count as usize)
        }
    }

    #[test]
    fn no_more_than_the_budget_is_read_whatever_length_the_file_reports() {
        let source = |left| {
            let taken = Arc::new(AtomicU64::new(0));
            (
                Source {
                    left,
                    taken: taken.clone(),
                },
                taken,
            )
        };

        // It says it is empty and holds a mebibyte.
        let (reader, taken) = source(1024 * 1024);
        assert_eq!(read_bounded(reader, 0, 16), None);
        assert_eq!(
            taken.load(Ordering::Relaxed),
            17,
            "one byte past the budget"
        );

        // It holds exactly the budget, whatever it said.
        let (reader, _) = source(16);
        assert_eq!(read_bounded(reader, 0, 16), Some(vec![7u8; 16]));

        // It says it is over the budget: nothing is read.
        let (reader, taken) = source(1024);
        assert_eq!(read_bounded(reader, 17, 16), None);
        assert_eq!(taken.load(Ordering::Relaxed), 0);
    }
}
