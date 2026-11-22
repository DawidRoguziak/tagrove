//! Thumbnails for the gallery's tiles: the client's thumbnail pipeline and tile demand
//! on the UI thread, decoding on GPUI's background executor, and the bounded cache of
//! decoded pictures with their textures.
//!
//! [`Thumbnails`] is an entity the [`Runtime`](crate::runtime::Runtime) owns, so the
//! pictures survive a visit to the settings page. The grid tells it the tiles it
//! renders ([`Thumbnails::sync`]) and reads a picture or a loading state per tile.
//! What happens in between never runs on the UI thread:
//!
//! - requests go to the backend through the pipeline's
//!   [`ThumbnailFetcher`] on the application's executor;
//! - the JPEG bytes of an answer are decoded on GPUI's background executor;
//! - only the bookkeeping (demand, cache, events) runs here, woken like the runtime.
//!
//! A decoded picture is held twice: its pixels in the [`PictureCache`] and, once
//! drawn, a texture in the window's sprite atlas. The cache is bounded in bytes, and
//! every picture it drops is taken out of the atlas as well
//! ([`Thumbnails::release_textures`]); GPUI itself never frees an image texture.

use std::sync::Arc;

use gpui::{App, AppContext as _, Context, Image, ImageFormat, RenderImage, SvgRenderer, Task};
use media_tagger_client::{
    backend::{Backend, ThumbnailImage},
    exec::Executor,
    gallery::{PictureCache, ThumbnailDemand, TileRequest, TileThumbnail, PICTURE_CACHE_BUDGET},
    model::Driver,
    thumbs::{ThumbnailEvent, ThumbnailFetcher, ThumbnailKey, ThumbnailPipeline},
};

use crate::runtime::WakeSignal;

/// A decoded thumbnail.
pub type Picture = Arc<RenderImage>;

/// What a tile draws for its thumbnail.
#[derive(Clone)]
pub enum TilePicture {
    Ready(Picture),
    /// A request or a decode is on its way: the delayed spinner.
    Loading,
    /// Nothing to show: the load failed, a cached-only request missed, or the tile is
    /// not in range.
    Empty,
}

pub struct Thumbnails {
    driver: Driver<ThumbnailPipeline, ThumbnailFetcher>,
    demand: ThumbnailDemand,
    pictures: PictureCache<Picture>,
    /// The tiles of the last sync and its pause flag; an event that changes what a
    /// tile asks for syncs them again.
    tiles: Vec<TileRequest>,
    paused: bool,
    /// The tiles of the order board of the bulk dock, which shows selected assets
    /// wherever the grid is scrolled.
    board: Vec<TileRequest>,
    /// Pictures the cache dropped whose textures are still in the atlas.
    retired: Vec<Picture>,
    decoding: usize,
    _pump: Task<()>,
}

impl Thumbnails {
    pub fn new(
        backend: Arc<dyn Backend>,
        executor: Arc<dyn Executor>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_budget(backend, executor, PICTURE_CACHE_BUDGET, cx)
    }

    /// Thumbnails whose cache keeps at most `budget` bytes of pictures outside the
    /// rendered range.
    pub fn with_budget(
        backend: Arc<dyn Backend>,
        executor: Arc<dyn Executor>,
        budget: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        let signal = WakeSignal::new();
        let wake = signal.clone();
        let driver = Driver::with_wake(
            ThumbnailPipeline::new(),
            ThumbnailFetcher::new(backend),
            executor,
            move || wake.raise(),
        );
        let pump = cx.spawn(async move |this, cx| loop {
            signal.wait().await;
            if this
                .update(cx, |thumbnails, cx| thumbnails.pump(cx))
                .is_err()
            {
                break;
            }
        });
        Self {
            driver,
            demand: ThumbnailDemand::new(),
            pictures: PictureCache::new(budget),
            tiles: Vec::new(),
            paused: false,
            board: Vec::new(),
            retired: Vec::new(),
            decoding: 0,
            _pump: pump,
        }
    }

    /// The tiles the grid renders now and whether fast scrolling paused demand. Tiles
    /// that left the range give up their requests; their pictures stay cached within
    /// the budget.
    pub fn sync(&mut self, tiles: Vec<TileRequest>, paused: bool) {
        self.tiles = tiles;
        self.paused = paused;
        self.apply();
    }

    /// The tiles the order board of the bulk dock renders now; none when no board is
    /// shown. They are asked for next to the grid's tiles and keep their pictures
    /// while the board shows them.
    pub fn sync_board(&mut self, tiles: Vec<TileRequest>) {
        if self.board == tiles {
            return;
        }
        self.board = tiles;
        self.apply();
    }

    fn apply(&mut self) {
        let pictures = &self.pictures;
        let merged;
        let tiles = if self.board.is_empty() {
            &self.tiles
        } else {
            // A tile both show is asked for once, as the grid asks for it.
            let mut all = self.tiles.clone();
            all.extend(
                self.board
                    .iter()
                    .filter(|tile| !self.tiles.iter().any(|other| other.key == tile.key))
                    .cloned(),
            );
            merged = all;
            &merged
        };
        let inputs = self
            .demand
            .sync(tiles, self.paused, |key| pictures.contains(key));
        for input in inputs {
            self.driver.handle(input);
        }
        let dropped = self.pictures.pin(self.demand.keys().cloned());
        self.retired.extend(dropped);
    }

    /// What the tile of `key` draws.
    pub fn picture(&self, key: &ThumbnailKey) -> TilePicture {
        if let Some(picture) = self.pictures.peek(key) {
            return TilePicture::Ready(picture.clone());
        }
        match self.demand.status(key) {
            TileThumbnail::Loading => TilePicture::Loading,
            TileThumbnail::Idle | TileThumbnail::Missing => TilePicture::Empty,
        }
    }

    /// Applies every answer that is waiting and starts the decode of each picture
    /// that arrived. Returns how many answers there were.
    pub fn pump(&mut self, cx: &mut Context<Self>) -> usize {
        let applied = self.driver.pump();
        let mut changed = false;
        for event in self.driver.model_mut().take_events() {
            changed |= self.demand.on_event(&event);
            if let ThumbnailEvent::Ready {
                key,
                image,
                current,
            } = event
            {
                // A tile that left the range in the meantime has no use for it.
                if self.demand.wants(&key) {
                    self.decode(key, image, current, cx);
                }
            }
        }
        if changed {
            // A cached-only miss may now ask for a render.
            self.apply();
            cx.notify();
        }
        applied
    }

    fn decode(
        &mut self,
        key: ThumbnailKey,
        image: ThumbnailImage,
        current: bool,
        cx: &mut Context<Self>,
    ) {
        let renderer = cx.svg_renderer();
        let decoded = cx.background_spawn(async move { decode_picture(image.bytes, renderer) });
        self.decoding += 1;
        cx.spawn(async move |this, cx| {
            let picture = decoded.await;
            this.update(cx, |thumbnails, cx| {
                thumbnails.decoded(key, picture, current, cx)
            })
            .ok();
        })
        .detach();
    }

    fn decoded(
        &mut self,
        key: ThumbnailKey,
        picture: Option<(Picture, usize)>,
        current: bool,
        cx: &mut Context<Self>,
    ) {
        self.decoding = self.decoding.saturating_sub(1);
        self.demand.decoded(&key, picture.is_some());
        if let Some((picture, bytes)) = picture {
            // Bytes of another source version than the one asked for are shown while
            // the tile is in range and never kept for a later showing.
            let dropped = self.pictures.insert(key, picture, bytes, current);
            self.retired.extend(dropped);
        }
        cx.notify();
    }

    /// Takes the textures of dropped pictures out of the sprite atlas of every
    /// window. Call it with the window being drawn, before its tiles are built.
    pub fn release_textures(&mut self, window: &mut gpui::Window, cx: &mut App) {
        for picture in self.retired.drain(..) {
            cx.drop_image(picture, Some(window));
        }
    }

    /// Bytes of decoded pictures held.
    pub fn cached_bytes(&self) -> usize {
        self.pictures.bytes()
    }

    pub fn cached_pictures(&self) -> usize {
        self.pictures.len()
    }

    /// The byte budget of pictures outside the rendered range.
    pub fn budget(&self) -> usize {
        self.pictures.budget()
    }

    /// Dropped pictures whose textures wait for [`Self::release_textures`].
    pub fn retired_pictures(&self) -> usize {
        self.retired.len()
    }

    /// Requests without an answer and pictures being decoded.
    pub fn pending(&self) -> usize {
        self.driver.model().in_flight() + self.decoding
    }

    /// Tiles in range.
    pub fn tiles_in_range(&self) -> usize {
        self.demand.len()
    }
}

/// Decodes the JPEG bytes of a thumbnail into a picture and its size in bytes.
/// `None`: the bytes are no picture. Runs on a background thread.
fn decode_picture(bytes: Vec<u8>, renderer: SvgRenderer) -> Option<(Picture, usize)> {
    let picture = Image::from_bytes(ImageFormat::Jpeg, bytes)
        .to_image_data(renderer)
        .ok()?;
    let size = picture.size(0);
    let bytes = usize::try_from(size.width.0).ok()? * usize::try_from(size.height.0).ok()? * 4;
    (bytes > 0).then_some((picture, bytes))
}
