//! Which thumbnail each tile in range asks for, and how (`GalleryTile.tsx`).
//!
//! The grid tells [`ThumbnailDemand`] the tiles it renders, in view or in the
//! overscan, and whether fast scrolling paused demand. The demand answers with inputs
//! for the [`ThumbnailPipeline`](crate::thumbs::ThumbnailPipeline): a request per
//! tile without a picture, and a cancel for every tile that left the range. The
//! rules are the Solid tile's (`docs/subsystems/thumbnails.md`, "Frontend tiles"):
//!
//! - An asset first shown during a pause asks for a cached file only
//!   ([`ThumbnailMode::CachedOnly`]); one first shown in the overscan asks at prefetch
//!   priority ([`ThumbnailMode::Prefetch`]); any other at once
//!   ([`ThumbnailMode::Visible`]).
//! - Both choices stick while the asset stays in range, with two one-way exceptions.
//!   A cached-only miss asks for a render once the pause has ended, at the priority
//!   the tile was first shown with. A prefetch whose picture has not arrived when the
//!   tile comes into view outside a pause is asked for again as visible, which moves
//!   its queued render ahead of every prefetch (ADR 0021).
//! - A tile whose picture is in the picture cache asks for nothing.
//!
//! The pipeline's modes only get stronger, so repeating a request is harmless; the
//! demand still sends each mode once per showing.

use std::collections::HashMap;

use crate::thumbs::{ThumbnailEvent, ThumbnailInput, ThumbnailKey, ThumbnailMode};

/// One rendered tile whose asset is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileRequest {
    pub key: ThumbnailKey,
    /// In a row in view, not only in the overscan.
    pub visible: bool,
}

/// What a tile without a cached picture shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileThumbnail {
    /// Nothing was asked for yet (the tile is not in range).
    Idle,
    /// A request or a decode is on its way: the delayed spinner.
    Loading,
    /// The load failed, or a cached-only request missed: the empty tile.
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Delivery {
    /// The pipeline has not delivered the picture.
    Pending,
    /// The bytes arrived and are being decoded.
    Decoding,
    /// The picture is in the picture cache.
    Shown,
    /// The bytes could not be decoded; nothing more is asked for this showing.
    Undecodable,
}

#[derive(Debug)]
struct Tile {
    /// First shown during a pause.
    cached_only: bool,
    /// First shown in the overscan.
    prefetch: bool,
    /// The cached-only request missed.
    missed: bool,
    /// The picture arrived, or the rendering request failed after its quick retries.
    settled: bool,
    /// Came into view before its prefetch arrived.
    promoted: bool,
    visible: bool,
    /// The strongest mode sent to the pipeline for this showing.
    requested: Option<ThumbnailMode>,
    /// The pipeline reported a failure for the mode in `requested`.
    failed: bool,
    delivery: Delivery,
    /// The sync that last saw the tile.
    seen: u64,
}

impl Tile {
    fn mode(&mut self, paused: bool) -> ThumbnailMode {
        if self.cached_only && (paused || !self.missed) {
            return ThumbnailMode::CachedOnly;
        }
        if self.prefetch && !self.promoted {
            if self.visible && !paused && !self.settled {
                self.promoted = true;
            } else {
                return ThumbnailMode::Prefetch;
            }
        }
        ThumbnailMode::Visible
    }
}

#[derive(Debug, Default)]
pub struct ThumbnailDemand {
    tiles: HashMap<ThumbnailKey, Tile>,
    sync: u64,
}

impl ThumbnailDemand {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tiles in range.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Whether a tile of `key` is in range.
    pub fn wants(&self, key: &ThumbnailKey) -> bool {
        self.tiles.contains_key(key)
    }

    /// The keys of the tiles in range.
    pub fn keys(&self) -> impl Iterator<Item = &ThumbnailKey> {
        self.tiles.keys()
    }

    /// The mode last sent to the pipeline for `key`.
    pub fn requested(&self, key: &ThumbnailKey) -> Option<ThumbnailMode> {
        self.tiles.get(key).and_then(|tile| tile.requested)
    }

    /// What the tile of `key` shows while the picture cache has no picture for it.
    pub fn status(&self, key: &ThumbnailKey) -> TileThumbnail {
        match self.tiles.get(key) {
            None => TileThumbnail::Idle,
            Some(tile) => match tile.delivery {
                Delivery::Decoding => TileThumbnail::Loading,
                Delivery::Undecodable => TileThumbnail::Missing,
                Delivery::Shown => TileThumbnail::Idle,
                Delivery::Pending if tile.failed => TileThumbnail::Missing,
                Delivery::Pending if tile.requested.is_some() => TileThumbnail::Loading,
                Delivery::Pending => TileThumbnail::Idle,
            },
        }
    }

    /// The tiles rendered now. `has_picture` says whether the picture cache holds a
    /// key. Returns what the pipeline must be told, cancels first.
    pub fn sync(
        &mut self,
        tiles: &[TileRequest],
        paused: bool,
        has_picture: impl Fn(&ThumbnailKey) -> bool,
    ) -> Vec<ThumbnailInput> {
        self.sync += 1;
        let sync = self.sync;
        let mut requests = Vec::new();
        for request in tiles {
            // The same asset twice in one range (a stale fallback row next to its
            // fresh one) is one tile for the pipeline, in view if either is.
            let repeated = self
                .tiles
                .get(&request.key)
                .is_some_and(|tile| tile.seen == sync);
            let tile = self
                .tiles
                .entry(request.key.clone())
                .or_insert_with(|| Tile {
                    cached_only: paused,
                    prefetch: !request.visible,
                    missed: false,
                    settled: false,
                    promoted: false,
                    visible: request.visible,
                    requested: None,
                    failed: false,
                    delivery: Delivery::Pending,
                    seen: sync,
                });
            tile.visible = request.visible || (repeated && tile.visible);
            tile.seen = sync;
            if has_picture(&request.key) {
                tile.delivery = Delivery::Shown;
                tile.settled = true;
                continue;
            }
            match tile.delivery {
                Delivery::Decoding | Delivery::Undecodable => continue,
                // The cache dropped a picture that is in range again: ask anew.
                Delivery::Shown => {
                    tile.delivery = Delivery::Pending;
                    tile.requested = None;
                    tile.failed = false;
                    tile.settled = false;
                }
                Delivery::Pending => {}
            }
            let mode = tile.mode(paused);
            if tile.requested.is_none_or(|requested| mode > requested) {
                tile.requested = Some(mode);
                tile.failed = false;
                requests.push(ThumbnailInput::Request {
                    key: request.key.clone(),
                    mode,
                });
            }
        }
        let mut inputs = Vec::new();
        self.tiles.retain(|key, tile| {
            if tile.seen == sync {
                return true;
            }
            if tile.requested.is_some() && tile.delivery == Delivery::Pending {
                inputs.push(ThumbnailInput::Cancel { key: key.clone() });
            }
            false
        });
        inputs.extend(requests);
        inputs
    }

    /// Applies an event of the pipeline. Returns whether a tile in range changed,
    /// which is when the grid must sync and draw again.
    pub fn on_event(&mut self, event: &ThumbnailEvent) -> bool {
        match event {
            ThumbnailEvent::Ready { key, .. } => {
                let Some(tile) = self.tiles.get_mut(key) else {
                    return false;
                };
                tile.settled = true;
                tile.failed = false;
                tile.delivery = Delivery::Decoding;
                true
            }
            ThumbnailEvent::Failed { key } => {
                let Some(tile) = self.tiles.get_mut(key) else {
                    return false;
                };
                if tile.requested == Some(ThumbnailMode::CachedOnly) {
                    tile.missed = true;
                } else {
                    tile.settled = true;
                }
                tile.failed = true;
                true
            }
        }
    }

    /// The decode of the delivered bytes of `key` ended. A picture that could not be
    /// decoded leaves the tile empty for this showing.
    pub fn decoded(&mut self, key: &ThumbnailKey, ok: bool) {
        if let Some(tile) = self.tiles.get_mut(key) {
            if tile.delivery == Delivery::Decoding {
                tile.delivery = if ok {
                    Delivery::Shown
                } else {
                    Delivery::Undecodable
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ThumbnailDemand, TileRequest, TileThumbnail};
    use crate::{
        backend::ThumbnailImage,
        thumbs::{ThumbnailEvent, ThumbnailInput, ThumbnailKey, ThumbnailMode},
    };

    const VERSION: &str = "0123456789abcdef";

    fn key(asset_id: i64) -> ThumbnailKey {
        ThumbnailKey::new(asset_id, VERSION)
    }

    fn tile(asset_id: i64, visible: bool) -> TileRequest {
        TileRequest {
            key: key(asset_id),
            visible,
        }
    }

    fn request(asset_id: i64, mode: ThumbnailMode) -> ThumbnailInput {
        ThumbnailInput::Request {
            key: key(asset_id),
            mode,
        }
    }

    fn requests(inputs: &[ThumbnailInput]) -> Vec<(i64, ThumbnailMode)> {
        inputs
            .iter()
            .filter_map(|input| match input {
                ThumbnailInput::Request { key, mode } => Some((key.asset_id, *mode)),
                _ => None,
            })
            .collect()
    }

    fn ready(asset_id: i64) -> ThumbnailEvent {
        ThumbnailEvent::Ready {
            key: key(asset_id),
            image: ThumbnailImage {
                bytes: vec![0xff, 0xd8],
                version: VERSION.into(),
            },
            current: true,
        }
    }

    fn failed(asset_id: i64) -> ThumbnailEvent {
        ThumbnailEvent::Failed { key: key(asset_id) }
    }

    fn no_pictures(_: &ThumbnailKey) -> bool {
        false
    }

    #[test]
    fn visible_tiles_ask_at_once_and_overscan_tiles_at_prefetch_priority() {
        let mut demand = ThumbnailDemand::new();

        let inputs = demand.sync(&[tile(1, true), tile(2, false)], false, no_pictures);

        assert_eq!(
            requests(&inputs),
            [(1, ThumbnailMode::Visible), (2, ThumbnailMode::Prefetch)]
        );
        assert_eq!(demand.status(&key(1)), TileThumbnail::Loading);
        // The same range again asks for nothing.
        assert!(demand
            .sync(&[tile(1, true), tile(2, false)], false, no_pictures)
            .is_empty());
    }

    #[test]
    fn a_pending_prefetch_is_promoted_once_when_its_tile_comes_into_view() {
        let mut demand = ThumbnailDemand::new();
        demand.sync(&[tile(1, false)], false, no_pictures);

        let inputs = demand.sync(&[tile(1, true)], false, no_pictures);
        assert_eq!(inputs, [request(1, ThumbnailMode::Visible)]);

        // Back in the overscan and in view again: nothing more is sent.
        assert!(demand
            .sync(&[tile(1, false)], false, no_pictures)
            .is_empty());
        assert!(demand.sync(&[tile(1, true)], false, no_pictures).is_empty());
    }

    #[test]
    fn a_prefetch_that_reaches_the_view_during_a_pause_waits_for_its_end() {
        let mut demand = ThumbnailDemand::new();
        demand.sync(&[tile(1, false)], false, no_pictures);

        assert!(demand.sync(&[tile(1, true)], true, no_pictures).is_empty());
        assert_eq!(demand.requested(&key(1)), Some(ThumbnailMode::Prefetch));

        let inputs = demand.sync(&[tile(1, true)], false, no_pictures);
        assert_eq!(inputs, [request(1, ThumbnailMode::Visible)]);
    }

    #[test]
    fn a_prefetch_that_failed_is_not_promoted() {
        let mut demand = ThumbnailDemand::new();
        demand.sync(&[tile(1, false)], false, no_pictures);
        assert!(demand.on_event(&failed(1)));
        assert_eq!(demand.status(&key(1)), TileThumbnail::Missing);

        assert!(demand.sync(&[tile(1, true)], false, no_pictures).is_empty());
    }

    #[test]
    fn tiles_first_shown_during_a_pause_ask_for_cached_files_only() {
        let mut demand = ThumbnailDemand::new();

        let inputs = demand.sync(&[tile(1, true), tile(2, false)], true, no_pictures);
        assert_eq!(
            requests(&inputs),
            [
                (1, ThumbnailMode::CachedOnly),
                (2, ThumbnailMode::CachedOnly)
            ]
        );

        // The pause ends before the answers: the cached-only requests stay.
        assert!(demand
            .sync(&[tile(1, true), tile(2, false)], false, no_pictures)
            .is_empty());

        // A miss asks for a render at the priority the tile was first shown with.
        demand.on_event(&failed(1));
        demand.on_event(&failed(2));
        assert_eq!(demand.status(&key(1)), TileThumbnail::Missing);
        let inputs = demand.sync(&[tile(1, true), tile(2, false)], false, no_pictures);
        assert_eq!(
            requests(&inputs),
            [(1, ThumbnailMode::Visible), (2, ThumbnailMode::Prefetch)]
        );
        assert_eq!(demand.status(&key(1)), TileThumbnail::Loading);
    }

    #[test]
    fn a_cached_only_miss_waits_while_the_pause_lasts() {
        let mut demand = ThumbnailDemand::new();
        demand.sync(&[tile(1, true)], true, no_pictures);
        demand.on_event(&failed(1));

        assert!(demand.sync(&[tile(1, true)], true, no_pictures).is_empty());
        assert_eq!(
            demand.sync(&[tile(1, true)], false, no_pictures),
            [request(1, ThumbnailMode::Visible)]
        );
    }

    #[test]
    fn a_tile_that_leaves_the_range_cancels_its_pending_request() {
        let mut demand = ThumbnailDemand::new();
        demand.sync(
            &[tile(1, true), tile(2, true), tile(3, true)],
            false,
            no_pictures,
        );
        demand.on_event(&ready(2));

        let inputs = demand.sync(&[tile(3, true), tile(4, false)], false, no_pictures);

        assert_eq!(
            inputs,
            [
                ThumbnailInput::Cancel { key: key(1) },
                request(4, ThumbnailMode::Prefetch)
            ],
            "a delivered picture has nothing to cancel"
        );
        assert!(!demand.wants(&key(1)) && demand.wants(&key(3)));
        assert_eq!(demand.len(), 2);
        // Events for tiles that are gone change nothing.
        assert!(!demand.on_event(&ready(1)));
    }

    #[test]
    fn a_tile_with_a_cached_picture_asks_for_nothing() {
        let mut demand = ThumbnailDemand::new();

        let inputs = demand.sync(&[tile(1, true), tile(2, true)], false, |key| {
            key.asset_id == 1
        });

        assert_eq!(inputs, [request(2, ThumbnailMode::Visible)]);
        assert_eq!(demand.requested(&key(1)), None);
        // Leaving the range cancels nothing for it.
        assert_eq!(
            demand.sync(&[], false, no_pictures),
            [ThumbnailInput::Cancel { key: key(2) }]
        );
    }

    #[test]
    fn a_delivered_picture_is_decoded_once_and_asked_again_only_after_eviction() {
        let mut demand = ThumbnailDemand::new();
        demand.sync(&[tile(1, true)], false, no_pictures);
        demand.on_event(&ready(1));
        assert_eq!(demand.status(&key(1)), TileThumbnail::Loading);

        // While the decode runs, the tile asks for nothing.
        assert!(demand.sync(&[tile(1, true)], false, no_pictures).is_empty());
        demand.decoded(&key(1), true);
        assert!(demand.sync(&[tile(1, true)], false, |_| true).is_empty());

        // The cache lost the picture while the tile stayed in range.
        assert_eq!(
            demand.sync(&[tile(1, true)], false, no_pictures),
            [request(1, ThumbnailMode::Visible)]
        );
    }

    #[test]
    fn bytes_that_cannot_be_decoded_leave_the_tile_empty() {
        let mut demand = ThumbnailDemand::new();
        demand.sync(&[tile(1, true)], false, no_pictures);
        demand.on_event(&ready(1));
        demand.decoded(&key(1), false);

        assert_eq!(demand.status(&key(1)), TileThumbnail::Missing);
        assert!(demand.sync(&[tile(1, true)], false, no_pictures).is_empty());
        // A new showing starts over.
        demand.sync(&[], false, no_pictures);
        assert_eq!(
            demand.sync(&[tile(1, true)], false, no_pictures),
            [request(1, ThumbnailMode::Visible)]
        );
    }
}
